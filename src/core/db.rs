use crate::core::schema::{CollectionSchema, FieldType, SchemaError};
use crate::core::query::Query;
use crate::core::s3::S3Config;
use crate::core::tokens::ApiToken;
use crate::core::hooks::Webhook;
use crate::core::aggregation::{AggregationPipeline, AggregationResult};
use redb::{Database as RedbDb, TableDefinition, ReadableTable};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;
use chrono::Utc;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("Database error: {0}")]
    Redb(#[from] redb::Error),
    #[error("Database error: {0}")]
    Database(#[from] redb::DatabaseError),
    #[error("Transaction error: {0}")]
    Tx(#[from] redb::TransactionError),
    #[error("Table error: {0}")]
    Table(#[from] redb::TableError),
    #[error("Storage error: {0}")]
    Storage(#[from] redb::StorageError),
    #[error("Commit error: {0}")]
    Commit(#[from] redb::CommitError),
    #[error("Schema validation failed: {0}")]
    Schema(#[from] SchemaError),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Collection {0} not found")]
    CollectionNotFound(String),
    #[error("Document {0} not found in collection {1}")]
    DocumentNotFound(String, String),
    #[error("Unique constraint violation for field {0}")]
    UniqueViolation(String),
    #[error("Token not found")]
    TokenNotFound,
    #[error("Webhook not found")]
    WebhookNotFound,
}

const COLLECTIONS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("_collections");
const SETTINGS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("_settings");
const INDEX_TABLE: TableDefinition<&str, &str> = TableDefinition::new("_indexes");
const TOKENS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("_tokens");
const WEBHOOKS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("_webhooks");

#[derive(Clone)]
pub struct Database {
    inner: Arc<RedbDb>,
}

impl Database {
    pub fn open(path: &str) -> Result<Self, DbError> {
        let db = RedbDb::create(path)?;

        let write_tx = db.begin_write()?;
        {
            let _ = write_tx.open_table(COLLECTIONS_TABLE)?;
            let _ = write_tx.open_table(SETTINGS_TABLE)?;
            let _ = write_tx.open_table(INDEX_TABLE)?;
            let _ = write_tx.open_table(TOKENS_TABLE)?;
            let _ = write_tx.open_table(WEBHOOKS_TABLE)?;
        }
        write_tx.commit()?;

        Ok(Self { inner: Arc::new(db) })
    }


    pub fn create_collection(&self, schema: CollectionSchema) -> Result<(), DbError> {
        let write_tx = self.inner.begin_write()?;
        {
            let mut col_table = write_tx.open_table(COLLECTIONS_TABLE)?;
            let schema_json = serde_json::to_string(&schema)?;
            col_table.insert(schema.name.as_str(), schema_json.as_str())?;
            let col_def: TableDefinition<&str, &str> = TableDefinition::new(&schema.name);
            let _ = write_tx.open_table(col_def)?;
        }
        write_tx.commit()?;
        Ok(())
    }

    pub fn get_collection_schema(&self, name: &str) -> Result<Option<CollectionSchema>, DbError> {
        let read_tx = self.inner.begin_read()?;
        let col_table = read_tx.open_table(COLLECTIONS_TABLE)?;
        match col_table.get(name)? {
            Some(s) => Ok(Some(serde_json::from_str(s.value())?)),
            None => Ok(None),
        }
    }

    pub fn delete_collection(&self, name: &str) -> Result<(), DbError> {
        let write_tx = self.inner.begin_write()?;
        {
            let mut col_table = write_tx.open_table(COLLECTIONS_TABLE)?;
            col_table.remove(name)?;
            let mut idx_table = write_tx.open_table(INDEX_TABLE)?;
            let prefix = format!("{}:", name);
            let keys: Vec<String> = idx_table
                .range(prefix.as_str()..)?
                .map_while(|item| {
                    item.ok().and_then(|(k, _)| {
                        let s = k.value().to_string();
                        if s.starts_with(&prefix) { Some(s) } else { None }
                    })
                })
                .collect();
            for k in keys {
                idx_table.remove(k.as_str())?;
            }
        }
        write_tx.commit()?;
        Ok(())
    }

    pub fn list_collections(&self) -> Result<Vec<CollectionSchema>, DbError> {
        let read_tx = self.inner.begin_read()?;
        let col_table = read_tx.open_table(COLLECTIONS_TABLE)?;
        let mut schemas = Vec::new();
        for item in col_table.iter()? {
            let (_, v) = item?;
            schemas.push(serde_json::from_str(v.value())?);
        }
        Ok(schemas)
    }

    pub fn update_collection(&self, name: &str, schema: CollectionSchema) -> Result<(), DbError> {
        let write_tx = self.inner.begin_write()?;
        {
            let mut col_table = write_tx.open_table(COLLECTIONS_TABLE)?;
            let schema_json = serde_json::to_string(&schema)?;
            col_table.insert(name, schema_json.as_str())?;
        }
        write_tx.commit()?;
        Ok(())
    }


    fn make_index_key(collection: &str, field: &str, val: &Value, id: &str) -> String {
        let val_str = match val {
            Value::Null => "null".to_string(),
            Value::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
            Value::Number(n) => format!("{:020.6}", n.as_f64().unwrap_or(0.0)),
            Value::String(s) => s.clone(),
            _ => serde_json::to_string(val).unwrap_or_default(),
        };
        format!("{}:{}:{}:{}", collection, field, val_str, id)
    }

    fn check_unique_constraints(
        &self,
        tx: &redb::ReadTransaction,
        schema: &CollectionSchema,
        doc: &HashMap<String, Value>,
        exclude_id: Option<&str>,
    ) -> Result<(), DbError> {
        let idx_table = tx.open_table(INDEX_TABLE)?;
        for field in &schema.fields {
            if field.unique {
                if let Some(val) = doc.get(&field.name) {
                    if val != &Value::Null {
                        let val_str = match val {
                            Value::Null => "null".to_string(),
                            Value::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
                            Value::Number(n) => format!("{:020.6}", n.as_f64().unwrap_or(0.0)),
                            Value::String(s) => s.clone(),
                            _ => serde_json::to_string(val).unwrap_or_default(),
                        };
                        let target_prefix = format!("{}:{}:{}:", schema.name, field.name, val_str);
                        for item in idx_table.range(target_prefix.as_str()..)? {
                            let (k, _) = item?;
                            let key = k.value();
                            if !key.starts_with(&target_prefix) { break; }
                            let parts: Vec<&str> = key.split(':').collect();
                            if let Some(&found_id) = parts.last() {
                                if Some(found_id) != exclude_id {
                                    return Err(DbError::UniqueViolation(field.name.clone()));
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn insert_document(&self, collection: &str, mut doc: HashMap<String, Value>) -> Result<HashMap<String, Value>, DbError> {
        let schema = self.get_collection_schema(collection)?
            .ok_or_else(|| DbError::CollectionNotFound(collection.to_string()))?;

        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        doc.insert("id".to_string(), Value::String(id.clone()));
        doc.insert("created".to_string(), Value::String(now.clone()));
        doc.insert("updated".to_string(), Value::String(now));

        schema.validate_and_coerce(&mut doc)?;

        let write_tx = self.inner.begin_write()?;
        {
            let read_tx = self.inner.begin_read()?;
            self.check_unique_constraints(&read_tx, &schema, &doc, None)?;

            let col_def: TableDefinition<&str, &str> = TableDefinition::new(collection);
            let mut col_table = write_tx.open_table(col_def)?;
            let doc_json = serde_json::to_string(&doc)?;
            col_table.insert(id.as_str(), doc_json.as_str())?;

            let mut idx_table = write_tx.open_table(INDEX_TABLE)?;
            for field in &schema.fields {
                if let Some(val) = doc.get(&field.name) {
                    let key = Self::make_index_key(collection, &field.name, val, &id);
                    idx_table.insert(key.as_str(), "")?;
                }
            }
        }
        write_tx.commit()?;
        Ok(doc)
    }

    pub fn bulk_insert(&self, collection: &str, docs: Vec<HashMap<String, Value>>) -> Result<Vec<HashMap<String, Value>>, DbError> {
        let schema = self.get_collection_schema(collection)?
            .ok_or_else(|| DbError::CollectionNotFound(collection.to_string()))?;

        let mut prepared: Vec<HashMap<String, Value>> = Vec::with_capacity(docs.len());
        for mut doc in docs {
            let id = Uuid::new_v4().to_string();
            let now = Utc::now().to_rfc3339();
            doc.insert("id".to_string(), Value::String(id.clone()));
            doc.insert("created".to_string(), Value::String(now.clone()));
            doc.insert("updated".to_string(), Value::String(now));
            schema.validate_and_coerce(&mut doc)?;
            prepared.push(doc);
        }

        let mut seen_unique = HashSet::new();
        for doc in &prepared {
            for field in &schema.fields {
                if field.unique {
                    if let Some(value) = doc.get(&field.name) {
                        if !value.is_null() {
                            let key = format!("{}\u{0}{}", field.name, serde_json::to_string(value)?);
                            if !seen_unique.insert(key) {
                                return Err(DbError::UniqueViolation(field.name.clone()));
                            }
                        }
                    }
                }
            }
        }

        let write_tx = self.inner.begin_write()?;
        {
            let read_tx = self.inner.begin_read()?;
            for doc in &prepared {
                self.check_unique_constraints(&read_tx, &schema, doc, None)?;
            }
            let col_def: TableDefinition<&str, &str> = TableDefinition::new(collection);
            let mut col_table = write_tx.open_table(col_def)?;
            let mut idx_table = write_tx.open_table(INDEX_TABLE)?;

            for doc in &prepared {
                let id = doc["id"].as_str().unwrap_or_default();
                let doc_json = serde_json::to_string(doc)?;
                col_table.insert(id, doc_json.as_str())?;
                for field in &schema.fields {
                    if let Some(val) = doc.get(&field.name) {
                        let key = Self::make_index_key(collection, &field.name, val, id);
                        idx_table.insert(key.as_str(), "")?;
                    }
                }
            }
        }
        write_tx.commit()?;
        Ok(prepared)
    }

    pub fn get_document(&self, collection: &str, id: &str) -> Result<Option<HashMap<String, Value>>, DbError> {
        let read_tx = self.inner.begin_read()?;
        let col_def: TableDefinition<&str, &str> = TableDefinition::new(collection);
        let col_table = match read_tx.open_table(col_def) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };
        match col_table.get(id)? {
            Some(v) => Ok(Some(serde_json::from_str(v.value())?)),
            None => Ok(None),
        }
    }

    pub fn update_document(&self, collection: &str, id: &str, mut doc: HashMap<String, Value>) -> Result<HashMap<String, Value>, DbError> {
        let schema = self.get_collection_schema(collection)?
            .ok_or_else(|| DbError::CollectionNotFound(collection.to_string()))?;

        let old_doc = self.get_document(collection, id)?
            .ok_or_else(|| DbError::DocumentNotFound(id.to_string(), collection.to_string()))?;

        doc.insert("id".to_string(), Value::String(id.to_string()));
        doc.insert("created".to_string(), old_doc.get("created").cloned().unwrap_or_else(|| Value::String(Utc::now().to_rfc3339())));
        doc.insert("updated".to_string(), Value::String(Utc::now().to_rfc3339()));

        schema.validate_and_coerce(&mut doc)?;

        let write_tx = self.inner.begin_write()?;
        {
            let read_tx = self.inner.begin_read()?;
            self.check_unique_constraints(&read_tx, &schema, &doc, Some(id))?;

            let col_def: TableDefinition<&str, &str> = TableDefinition::new(collection);
            let mut col_table = write_tx.open_table(col_def)?;
            let doc_json = serde_json::to_string(&doc)?;
            col_table.insert(id, doc_json.as_str())?;

            let mut idx_table = write_tx.open_table(INDEX_TABLE)?;
            for field in &schema.fields {
                if let Some(old_val) = old_doc.get(&field.name) {
                    let old_key = Self::make_index_key(collection, &field.name, old_val, id);
                    idx_table.remove(old_key.as_str())?;
                }
                if let Some(new_val) = doc.get(&field.name) {
                    let new_key = Self::make_index_key(collection, &field.name, new_val, id);
                    idx_table.insert(new_key.as_str(), "")?;
                }
            }
        }
        write_tx.commit()?;
        Ok(doc)
    }

    pub fn patch_document(&self, collection: &str, id: &str, patch: HashMap<String, Value>) -> Result<HashMap<String, Value>, DbError> {
        let mut existing = self.get_document(collection, id)?
            .ok_or_else(|| DbError::DocumentNotFound(id.to_string(), collection.to_string()))?;
        for (k, v) in patch {
            existing.insert(k, v);
        }
        self.update_document(collection, id, existing)
    }

    pub fn upsert_document(
        &self,
        collection: &str,
        match_field: &str,
        doc: HashMap<String, Value>,
    ) -> Result<(HashMap<String, Value>, bool), DbError> {
        let match_val = doc.get(match_field).cloned().unwrap_or(Value::Null);
        let query = Query {
            filters: vec![crate::core::query::Filter {
                field: match_field.to_string(),
                op: crate::core::query::QueryOperator::Eq,
                value: match_val,
            }],
            sort: vec![],
            limit: Some(1),
            offset: None,
        };
        let existing = self.query_documents(collection, query)?;
        if let Some(found) = existing.into_iter().next() {
            let id = found["id"].as_str().unwrap_or_default().to_string();
            let updated = self.update_document(collection, &id, doc)?;
            Ok((updated, false))
        } else {
            let inserted = self.insert_document(collection, doc)?;
            Ok((inserted, true))
        }
    }

    pub fn find_one(&self, collection: &str, query: Query) -> Result<Option<HashMap<String, Value>>, DbError> {
        let mut q = query;
        q.limit = Some(1);
        let docs = self.query_documents(collection, q)?;
        Ok(docs.into_iter().next())
    }

    pub fn delete_document(&self, collection: &str, id: &str) -> Result<(), DbError> {
        let schema = self.get_collection_schema(collection)?
            .ok_or_else(|| DbError::CollectionNotFound(collection.to_string()))?;

        let old_doc = self.get_document(collection, id)?
            .ok_or_else(|| DbError::DocumentNotFound(id.to_string(), collection.to_string()))?;

        let write_tx = self.inner.begin_write()?;
        {
            let col_def: TableDefinition<&str, &str> = TableDefinition::new(collection);
            let mut col_table = write_tx.open_table(col_def)?;
            col_table.remove(id)?;

            let mut idx_table = write_tx.open_table(INDEX_TABLE)?;
            for field in &schema.fields {
                if let Some(old_val) = old_doc.get(&field.name) {
                    let old_key = Self::make_index_key(collection, &field.name, old_val, id);
                    idx_table.remove(old_key.as_str())?;
                }
            }
        }
        write_tx.commit()?;
        Ok(())
    }

    pub fn bulk_delete(&self, collection: &str, ids: &[String]) -> Result<usize, DbError> {
        let schema = self.get_collection_schema(collection)?
            .ok_or_else(|| DbError::CollectionNotFound(collection.to_string()))?;

        let write_tx = self.inner.begin_write()?;
        let mut deleted = 0;
        {
            let col_def: TableDefinition<&str, &str> = TableDefinition::new(collection);
            let mut col_table = write_tx.open_table(col_def)?;
            let mut idx_table = write_tx.open_table(INDEX_TABLE)?;

            for id in ids {
                let doc_str_opt = col_table.get(id.as_str())?.map(|v| v.value().to_string());
                if let Some(doc_str) = doc_str_opt {
                    let old_doc: HashMap<String, Value> = serde_json::from_str(&doc_str)?;
                    col_table.remove(id.as_str())?;
                    for field in &schema.fields {
                        if let Some(old_val) = old_doc.get(&field.name) {
                            let old_key = Self::make_index_key(collection, &field.name, old_val, id);
                            idx_table.remove(old_key.as_str())?;
                        }
                    }
                    deleted += 1;
                }
            }
        }
        write_tx.commit()?;
        Ok(deleted)
    }

    pub fn count_documents(&self, collection: &str, query: Option<&Query>) -> Result<usize, DbError> {
        let read_tx = self.inner.begin_read()?;
        let col_def: TableDefinition<&str, &str> = TableDefinition::new(collection);
        let col_table = match read_tx.open_table(col_def) {
            Ok(t) => t,
            Err(_) => return Ok(0),
        };
        let mut count = 0;
        for item in col_table.iter()? {
            let (_, v) = item?;
            if let Some(q) = query {
                let doc: HashMap<String, Value> = serde_json::from_str(v.value())?;
                if q.matches(&doc) { count += 1; }
            } else {
                count += 1;
            }
        }
        Ok(count)
    }

    pub fn query_documents(&self, collection: &str, query: Query) -> Result<Vec<HashMap<String, Value>>, DbError> {
        let read_tx = self.inner.begin_read()?;
        let col_def: TableDefinition<&str, &str> = TableDefinition::new(collection);
        let col_table = match read_tx.open_table(col_def) {
            Ok(t) => t,
            Err(_) => return Ok(Vec::new()),
        };

        let mut docs = Vec::new();

        let mut index_ids: Option<std::collections::HashSet<String>> = None;
        if let Some(schema) = self.get_collection_schema(collection)? {
            for filter in &query.filters {
                if let crate::core::query::QueryOperator::Eq = filter.op {
                    if schema.fields.iter().any(|f| f.name == filter.field && (f.unique || f.indexed)) {
                        let val_str = match &filter.value {
                            Value::Null => "null".to_string(),
                            Value::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
                            Value::Number(n) => format!("{:020.6}", n.as_f64().unwrap_or(0.0)),
                            Value::String(s) => s.clone(),
                            _ => serde_json::to_string(&filter.value).unwrap_or_default(),
                        };
                        let prefix = format!("{}:{}:{}:", collection, filter.field, val_str);
                        let idx_table = read_tx.open_table(INDEX_TABLE)?;
                        let mut ids = std::collections::HashSet::new();
                        for item in idx_table.range(prefix.as_str()..)? {
                            let (k, _) = item?;
                            let key = k.value();
                            if !key.starts_with(&prefix) { break; }
                            let parts: Vec<&str> = key.split(':').collect();
                            if let Some(&id) = parts.last() {
                                ids.insert(id.to_string());
                            }
                        }
                        index_ids = Some(ids);
                        break;
                    }
                }
            }
        }

        if let Some(ids) = index_ids {
            for id in ids {
                if let Some(v) = col_table.get(id.as_str())? {
                    let doc: HashMap<String, Value> = serde_json::from_str(v.value())?;
                    if query.matches(&doc) { docs.push(doc); }
                }
            }
        } else {
            for item in col_table.iter()? {
                let (_, v) = item?;
                let doc: HashMap<String, Value> = serde_json::from_str(v.value())?;
                if query.matches(&doc) { docs.push(doc); }
            }
        }

        query.sort_documents(&mut docs);

        if let Some(offset) = query.offset {
            if offset < docs.len() {
                docs = docs.split_off(offset);
            } else {
                docs.clear();
            }
        }
        if let Some(limit) = query.limit {
            docs.truncate(limit);
        }

        Ok(docs)
    }

    pub fn search_documents(
        &self,
        collection: &str,
        query_str: &str,
        fields: &[String],
        limit: Option<usize>,
    ) -> Result<Vec<HashMap<String, Value>>, DbError> {
        let all_docs = self.query_documents(collection, Query {
            filters: vec![],
            sort: vec![],
            limit: None,
            offset: None,
        })?;

        let searchable_fields: Vec<String> = if fields.is_empty() {
            if let Some(schema) = self.get_collection_schema(collection)? {
                schema.fields.iter()
                    .filter(|f| matches!(f.field_type, FieldType::Text))
                    .map(|f| f.name.clone())
                    .collect()
            } else {
                vec![]
            }
        } else {
            fields.to_vec()
        };

        let scored = crate::core::fts::search_docs(&all_docs, query_str, &searchable_fields, 0.1);
        let mut results: Vec<HashMap<String, Value>> = scored.into_iter().map(|(_, doc)| doc).collect();

        if let Some(l) = limit {
            results.truncate(l);
        }
        Ok(results)
    }

    pub fn aggregate(&self, collection: &str, pipeline: &AggregationPipeline) -> Result<AggregationResult, DbError> {
        let all_docs = self.query_documents(collection, Query {
            filters: vec![],
            sort: vec![],
            limit: None,
            offset: None,
        })?;
        Ok(crate::core::aggregation::run_pipeline(&all_docs, pipeline))
    }

    pub fn expand_relations(&self, collection: &str, docs: &mut [HashMap<String, Value>]) -> Result<(), DbError> {
        let schema = match self.get_collection_schema(collection)? {
            Some(s) => s,
            None => return Ok(()),
        };

        let relation_fields: Vec<(String, String)> = schema.fields.iter()
            .filter_map(|f| {
                if let FieldType::Relation { collection: target } = &f.field_type {
                    Some((f.name.clone(), target.clone()))
                } else {
                    None
                }
            })
            .collect();

        if relation_fields.is_empty() { return Ok(()); }

        for doc in docs {
            let mut expand_map = serde_json::Map::new();
            for (field_name, target_col) in &relation_fields {
                if let Some(Value::String(related_id)) = doc.get(field_name) {
                    if let Ok(Some(related_doc)) = self.get_document(target_col, related_id) {
                        expand_map.insert(field_name.clone(), Value::Object(serde_json::Map::from_iter(related_doc)));
                    }
                }
            }
            if !expand_map.is_empty() {
                doc.insert("expand".to_string(), Value::Object(expand_map));
            }
        }
        Ok(())
    }


    pub fn create_token(&self, token: &ApiToken) -> Result<(), DbError> {
        let write_tx = self.inner.begin_write()?;
        {
            let mut tbl = write_tx.open_table(TOKENS_TABLE)?;
            let json = serde_json::to_string(token)?;
            tbl.insert(token.id.as_str(), json.as_str())?;
        }
        write_tx.commit()?;
        Ok(())
    }

    pub fn list_tokens(&self) -> Result<Vec<ApiToken>, DbError> {
        let read_tx = self.inner.begin_read()?;
        let tbl = read_tx.open_table(TOKENS_TABLE)?;
        let mut tokens = Vec::new();
        for item in tbl.iter()? {
            let (_, v) = item?;
            tokens.push(serde_json::from_str(v.value())?);
        }
        Ok(tokens)
    }

    pub fn get_token_by_hash(&self, hash: &str) -> Result<Option<ApiToken>, DbError> {
        let read_tx = self.inner.begin_read()?;
        let tbl = read_tx.open_table(TOKENS_TABLE)?;
        for item in tbl.iter()? {
            let (_, v) = item?;
            let token: ApiToken = serde_json::from_str(v.value())?;
            if token.key_hash == hash {
                return Ok(Some(token));
            }
        }
        Ok(None)
    }

    pub fn revoke_token(&self, id: &str) -> Result<(), DbError> {
        let write_tx = self.inner.begin_write()?;
        {
            let mut tbl = write_tx.open_table(TOKENS_TABLE)?;
            tbl.remove(id)?;
        }
        write_tx.commit()?;
        Ok(())
    }


    pub fn create_webhook(&self, webhook: &Webhook) -> Result<(), DbError> {
        let write_tx = self.inner.begin_write()?;
        {
            let mut tbl = write_tx.open_table(WEBHOOKS_TABLE)?;
            let json = serde_json::to_string(webhook)?;
            tbl.insert(webhook.id.as_str(), json.as_str())?;
        }
        write_tx.commit()?;
        Ok(())
    }

    pub fn list_webhooks(&self) -> Result<Vec<Webhook>, DbError> {
        let read_tx = self.inner.begin_read()?;
        let tbl = read_tx.open_table(WEBHOOKS_TABLE)?;
        let mut hooks = Vec::new();
        for item in tbl.iter()? {
            let (_, v) = item?;
            hooks.push(serde_json::from_str(v.value())?);
        }
        Ok(hooks)
    }

    pub fn delete_webhook(&self, id: &str) -> Result<(), DbError> {
        let write_tx = self.inner.begin_write()?;
        {
            let mut tbl = write_tx.open_table(WEBHOOKS_TABLE)?;
            tbl.remove(id)?;
        }
        write_tx.commit()?;
        Ok(())
    }

    pub fn get_webhooks_for(&self, collection: &str, event: &str) -> Result<Vec<Webhook>, DbError> {
        Ok(self.list_webhooks()?.into_iter()
            .filter(|w| w.active && w.collection == collection && format!("{:?}", w.event).to_lowercase().contains(event))
            .collect())
    }


    pub fn get_settings(&self) -> Result<S3Config, DbError> {
        let read_tx = self.inner.begin_read()?;
        let tbl = read_tx.open_table(SETTINGS_TABLE)?;
        match tbl.get("s3_config")? {
            Some(v) => Ok(serde_json::from_str(v.value())?),
            None => Ok(S3Config::default()),
        }
    }

    pub fn save_settings(&self, settings: &S3Config) -> Result<(), DbError> {
        let write_tx = self.inner.begin_write()?;
        {
            let mut tbl = write_tx.open_table(SETTINGS_TABLE)?;
            let json = serde_json::to_string(settings)?;
            tbl.insert("s3_config", json.as_str())?;
        }
        write_tx.commit()?;
        Ok(())
    }


    pub fn backup(&self, dest_path: &str) -> Result<(), DbError> {
        let dest_db = RedbDb::create(dest_path)?;
        let read_tx = self.inner.begin_read()?;
        let write_tx = dest_db.begin_write()?;
        {
            let tables: &[&str] = &["_collections", "_settings", "_indexes", "_tokens", "_webhooks"];
            for &tbl_name in tables {
                let def: TableDefinition<&str, &str> = TableDefinition::new(tbl_name);
                let src = match read_tx.open_table(def) {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                let mut dst = write_tx.open_table(def)?;
                for item in src.iter()? {
                    let (k, v) = item?;
                    dst.insert(k.value(), v.value())?;
                }
            }

            let cols_def: TableDefinition<&str, &str> = TableDefinition::new("_collections");
            let cols_src = read_tx.open_table(cols_def)?;
            for item in cols_src.iter()? {
                let (k, _) = item?;
                let col_name = k.value().to_string();
                let col_def: TableDefinition<&str, &str> = TableDefinition::new(&col_name);
                if let Ok(src) = read_tx.open_table(col_def) {
                    let mut dst = write_tx.open_table(col_def)?;
                    for rec in src.iter()? {
                        let (rk, rv) = rec?;
                        dst.insert(rk.value(), rv.value())?;
                    }
                }
            }
        }
        write_tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::schema::{FieldSchema, FieldType};

    fn database() -> (tempfile::TempDir, Database) {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("test.db");
        let db = Database::open(path.to_str().expect("utf-8 path")).expect("database opens");
        (directory, db)
    }

    #[test]
    fn inserts_schema_default_values() {
        let (_directory, db) = database();
        db.create_collection(CollectionSchema {
            name: "tasks".to_string(),
            fields: vec![FieldSchema {
                name: "done".to_string(),
                field_type: FieldType::Bool,
                required: true,
                unique: false,
                indexed: false,
                searchable: false,
                default_value: Some(Value::Bool(false)),
            }],
        }).expect("collection created");

        let document = db.insert_document("tasks", HashMap::new()).expect("document inserted");
        assert_eq!(document.get("done"), Some(&Value::Bool(false)));
    }

    #[test]
    fn bulk_insert_rejects_duplicates_in_a_unique_field() {
        let (_directory, db) = database();
        db.create_collection(CollectionSchema {
            name: "users".to_string(),
            fields: vec![FieldSchema {
                name: "email".to_string(),
                field_type: FieldType::Text,
                required: true,
                unique: true,
                indexed: true,
                searchable: false,
                default_value: None,
            }],
        }).expect("collection created");

        let mut first = HashMap::new();
        first.insert("email".to_string(), Value::String("hello@glaudb.dev".to_string()));
        let mut second = HashMap::new();
        second.insert("email".to_string(), Value::String("hello@glaudb.dev".to_string()));

        assert!(matches!(db.bulk_insert("users", vec![first, second]), Err(DbError::UniqueViolation(field)) if field == "email"));
        assert_eq!(db.count_documents("users", None).expect("documents counted"), 0);
    }
}
