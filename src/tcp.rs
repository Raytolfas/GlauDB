use crate::core::{Database, CollectionSchema, Query, GlauConfig, hash_token_str};
use tokio::net::TcpListener;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

pub static ACTIVE_TCP_CONNECTIONS: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Deserialize, Serialize)]
struct TcpRequest {
    #[serde(default)]
    auth: Option<String>,
    cmd: u8,
    payload: Value,
}

pub async fn start_tcp_server(db: Database, config: GlauConfig, addr: &str) {
    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            log::error!("Failed to bind TCP server to {}: {}", addr, e);
            return;
        }
    };
    log::info!("TCP Query Server listening on {}", addr);

    loop {
        let (socket, _) = match listener.accept().await {
            Ok(s) => s,
            Err(e) => {
                log::error!("TCP accept error: {}", e);
                continue;
            }
        };

        let db = db.clone();
        let cfg = config.clone();
        tokio::spawn(async move {
            ACTIVE_TCP_CONNECTIONS.fetch_add(1, Ordering::SeqCst);
            let mut framed = Framed::new(socket, LengthDelimitedCodec::new());
            
            while let Some(result) = framed.next().await {
                let bytes = match result {
                    Ok(b) => b,
                    Err(e) => {
                        log::error!("TCP read frame error: {}", e);
                        break;
                    }
                };

                let request: TcpRequest = match serde_json::from_slice(&bytes) {
                    Ok(req) => req,
                    Err(e) => {
                        let resp = json!({ "error": format!("Invalid JSON request: {}", e) });
                        let resp_bytes = serde_json::to_vec(&resp).unwrap();
                        let mut out_bytes = vec![1];
                        out_bytes.extend(resp_bytes);
                        let _ = framed.send(bytes::Bytes::from(out_bytes)).await;
                        continue;
                    }
                };

                let is_authorized = match &request.auth {
                    Some(provided) => {
                        if provided == &cfg.admin_password {
                            true
                        } else {
                            let hash = hash_token_str(provided);
                            match db.get_token_by_hash(&hash) {
                                Ok(Some(token)) => {
                                    if let Some(exp) = &token.expires {
                                        if let Ok(exp_dt) = chrono::DateTime::parse_from_rfc3339(exp) {
                                            exp_dt > chrono::Utc::now()
                                        } else {
                                            false
                                        }
                                    } else {
                                        true
                                    }
                                }
                                _ => false,
                            }
                        }
                    }
                    None => false,
                };

                if !is_authorized {
                    let resp = json!({ "error": "Unauthorized: invalid or missing password/token" });
                    let resp_bytes = serde_json::to_vec(&resp).unwrap();
                    let mut out_bytes = vec![1];
                    out_bytes.extend(resp_bytes);
                    let _ = framed.send(bytes::Bytes::from(out_bytes)).await;
                    continue;
                }

                let (status, response_val) = match handle_command(&db, request.cmd, request.payload).await {
                    Ok(val) => (0, val),
                    Err(e) => (1, json!({ "error": e })),
                };

                let mut out_bytes = vec![status];
                if let Ok(resp_bytes) = serde_json::to_vec(&response_val) {
                    out_bytes.extend(resp_bytes);
                } else {
                    out_bytes.push(1);
                    out_bytes.extend(b"{\"error\":\"Failed to serialize response\"}");
                }

                if let Err(e) = framed.send(bytes::Bytes::from(out_bytes)).await {
                    log::error!("TCP send frame error: {}", e);
                    break;
                }
            }
            ACTIVE_TCP_CONNECTIONS.fetch_sub(1, Ordering::SeqCst);
        });
    }
}

async fn handle_command(db: &Database, cmd: u8, payload: Value) -> Result<Value, String> {
    match cmd {
        0x01 => {
            let schema: CollectionSchema = serde_json::from_value(payload)
                .map_err(|e| format!("Invalid schema payload: {}", e))?;
            db.create_collection(schema)
                .map_err(|e| format!("Failed to create collection: {}", e))?;
            Ok(json!({ "success": true }))
        }
        0x02 => {
            let name = payload["name"].as_str()
                .ok_or_else(|| "Missing collection name".to_string())?;
            db.delete_collection(name)
                .map_err(|e| format!("Failed to delete collection: {}", e))?;
            Ok(json!({ "success": true }))
        }
        0x03 => {
            let col = payload["collection"].as_str()
                .ok_or_else(|| "Missing collection name".to_string())?;
            let doc: HashMap<String, Value> = serde_json::from_value(payload["data"].clone())
                .map_err(|e| format!("Invalid document data: {}", e))?;
            let inserted = db.insert_document(col, doc)
                .map_err(|e| format!("Insert error: {}", e))?;
            Ok(serde_json::to_value(inserted).unwrap())
        }
        0x04 => {
            let col = payload["collection"].as_str()
                .ok_or_else(|| "Missing collection name".to_string())?;
            let id = payload["id"].as_str()
                .ok_or_else(|| "Missing document id".to_string())?;
            let expand = payload["expand"].as_bool().unwrap_or(false);
            let doc = db.get_document(col, id)
                .map_err(|e| format!("Get document error: {}", e))?;
            match doc {
                Some(mut d) => {
                    if expand {
                        let mut arr = vec![d];
                        db.expand_relations(col, &mut arr)
                            .map_err(|e| format!("Relation expansion error: {}", e))?;
                        d = arr.remove(0);
                    }
                    Ok(serde_json::to_value(d).unwrap())
                }
                None => Err("Document not found".to_string()),
            }
        }
        0x05 => {
            let col = payload["collection"].as_str()
                .ok_or_else(|| "Missing collection name".to_string())?;
            let id = payload["id"].as_str()
                .ok_or_else(|| "Missing document id".to_string())?;
            let doc: HashMap<String, Value> = serde_json::from_value(payload["data"].clone())
                .map_err(|e| format!("Invalid document data: {}", e))?;
            let updated = db.update_document(col, id, doc)
                .map_err(|e| format!("Update error: {}", e))?;
            Ok(serde_json::to_value(updated).unwrap())
        }
        0x06 => {
            let col = payload["collection"].as_str()
                .ok_or_else(|| "Missing collection name".to_string())?;
            let id = payload["id"].as_str()
                .ok_or_else(|| "Missing document id".to_string())?;
            db.delete_document(col, id)
                .map_err(|e| format!("Delete error: {}", e))?;
            Ok(json!({ "success": true }))
        }
        0x07 => {
            let col = payload["collection"].as_str()
                .ok_or_else(|| "Missing collection name".to_string())?;
            let query: Query = serde_json::from_value(payload["query"].clone())
                .unwrap_or(Query { filters: vec![], sort: vec![], limit: None, offset: None });
            let expand = payload["expand"].as_bool().unwrap_or(false);
            let mut docs = db.query_documents(col, query)
                .map_err(|e| format!("Query error: {}", e))?;
            if expand {
                db.expand_relations(col, &mut docs)
                    .map_err(|e| format!("Relation expansion error: {}", e))?;
            }
            Ok(serde_json::to_value(docs).unwrap())
        }
        0x08 => {
            let dest = payload["dest"].as_str()
                .ok_or_else(|| "Missing destination path".to_string())?;
            db.backup(dest)
                .map_err(|e| format!("Backup error: {}", e))?;
            Ok(json!({ "success": true }))
        }
        0x09 => {
            let col = payload["collection"].as_str()
                .ok_or_else(|| "Missing collection name".to_string())?;
            let id = payload["id"].as_str()
                .ok_or_else(|| "Missing document id".to_string())?;
            let patch: HashMap<String, Value> = serde_json::from_value(payload["data"].clone())
                .map_err(|e| format!("Invalid patch data: {}", e))?;
            let patched = db.patch_document(col, id, patch)
                .map_err(|e| format!("Patch error: {}", e))?;
            Ok(serde_json::to_value(patched).unwrap())
        }
        0x0A => {
            let col = payload["collection"].as_str()
                .ok_or_else(|| "Missing collection name".to_string())?;
            let docs: Vec<HashMap<String, Value>> = serde_json::from_value(payload["data"].clone())
                .map_err(|e| format!("Invalid document list: {}", e))?;
            let inserted = db.bulk_insert(col, docs)
                .map_err(|e| format!("Bulk insert error: {}", e))?;
            Ok(serde_json::to_value(inserted).unwrap())
        }
        0x0B => {
            let col = payload["collection"].as_str()
                .ok_or_else(|| "Missing collection name".to_string())?;
            let ids: Vec<String> = serde_json::from_value(payload["ids"].clone())
                .map_err(|e| format!("Invalid ids list: {}", e))?;
            let count = db.bulk_delete(col, &ids)
                .map_err(|e| format!("Bulk delete error: {}", e))?;
            Ok(json!({ "count": count }))
        }
        0x0C => {
            let col = payload["collection"].as_str()
                .ok_or_else(|| "Missing collection name".to_string())?;
            let q = payload["q"].as_str()
                .ok_or_else(|| "Missing search query".to_string())?;
            let fields: Vec<String> = serde_json::from_value(payload["fields"].clone())
                .unwrap_or_default();
            let limit = payload["limit"].as_u64().map(|l| l as usize);
            let docs = db.search_documents(col, q, &fields, limit)
                .map_err(|e| format!("Search error: {}", e))?;
            Ok(serde_json::to_value(docs).unwrap())
        }
        0x0D => {
            let col = payload["collection"].as_str()
                .ok_or_else(|| "Missing collection name".to_string())?;
            let pipeline: crate::core::AggregationPipeline = serde_json::from_value(payload["pipeline"].clone())
                .map_err(|e| format!("Invalid pipeline payload: {}", e))?;
            let res = db.aggregate(col, &pipeline)
                .map_err(|e| format!("Aggregation error: {}", e))?;
            Ok(serde_json::to_value(res).unwrap())
        }
        0x0E => {
            let col = payload["collection"].as_str()
                .ok_or_else(|| "Missing collection name".to_string())?;
            let query: Option<Query> = serde_json::from_value(payload["query"].clone()).ok();
            let count = db.count_documents(col, query.as_ref())
                .map_err(|e| format!("Count error: {}", e))?;
            Ok(json!({ "count": count }))
        }
        0x0F => {
            let col = payload["collection"].as_str()
                .ok_or_else(|| "Missing collection name".to_string())?;
            let match_field = payload["match_field"].as_str()
                .ok_or_else(|| "Missing match_field".to_string())?;
            let doc: HashMap<String, Value> = serde_json::from_value(payload["data"].clone())
                .map_err(|e| format!("Invalid document data: {}", e))?;
            let (result, inserted) = db.upsert_document(col, match_field, doc)
                .map_err(|e| format!("Upsert error: {}", e))?;
            Ok(json!({ "record": result, "inserted": inserted }))
        }
        0x10 => {
            let col = payload["collection"].as_str()
                .ok_or_else(|| "Missing collection name".to_string())?;
            let query: Query = serde_json::from_value(payload["query"].clone())
                .unwrap_or(Query { filters: vec![], sort: vec![], limit: Some(1), offset: None });
            match db.find_one(col, query).map_err(|e| format!("Find one error: {}", e))? {
                Some(doc) => Ok(serde_json::to_value(doc).unwrap()),
                None => Err("No matching document".to_string()),
            }
        }
        _ => Err(format!("Unknown TCP command code: 0x{:02X}", cmd)),
    }
}
