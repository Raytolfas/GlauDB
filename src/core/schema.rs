use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use serde_json::Value;
use chrono::{DateTime, Utc};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchemaError {
    #[error("Missing required field: {0}")]
    MissingRequired(String),
    #[error("Invalid field type for {0}: expected {1}, got {2}")]
    InvalidType(String, String, String),
    #[error("Field {0} is not defined in the schema")]
    UndefinedField(String),
    #[error("Field {0} must be unique")]
    UniqueConstraintViolation(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    Text,
    Number,
    Bool,
    Json,
    DateTime,
    File,
    Relation { collection: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSchema {
    pub name: String,
    pub field_type: FieldType,
    pub required: bool,
    pub unique: bool,
    #[serde(default)]
    pub indexed: bool,
    #[serde(default)]
    pub searchable: bool,
    #[serde(default)]
    pub default_value: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionSchema {
    pub name: String,
    pub fields: Vec<FieldSchema>,
}

impl CollectionSchema {
    pub fn validate_and_coerce(&self, doc: &mut HashMap<String, Value>) -> Result<(), SchemaError> {
        let mut doc_keys: std::collections::HashSet<String> = doc.keys().cloned().collect();

        doc_keys.remove("id");
        doc_keys.remove("created");
        doc_keys.remove("updated");

        for field in &self.fields {
            doc_keys.remove(&field.name);
            if !doc.contains_key(&field.name) {
                if let Some(default_value) = &field.default_value {
                    doc.insert(field.name.clone(), default_value.clone());
                }
            }
            let val = doc.get_mut(&field.name);

            match val {
                None | Some(Value::Null) => {
                    if field.required {
                        return Err(SchemaError::MissingRequired(field.name.clone()));
                    }
                }
                Some(v) => {
                    match &field.field_type {
                        FieldType::Text => {
                            if !v.is_string() {
                                if v.is_number() || v.is_boolean() {
                                    *v = Value::String(v.to_string());
                                } else {
                                    return Err(SchemaError::InvalidType(
                                        field.name.clone(),
                                        "string".to_string(),
                                        v.to_string(),
                                    ));
                                }
                            }
                        }
                        FieldType::Number => {
                            if !v.is_number() {
                                return Err(SchemaError::InvalidType(
                                    field.name.clone(),
                                    "number".to_string(),
                                    v.to_string(),
                                ));
                            }
                        }
                        FieldType::Bool => {
                            if !v.is_boolean() {
                                return Err(SchemaError::InvalidType(
                                    field.name.clone(),
                                    "boolean".to_string(),
                                    v.to_string(),
                                ));
                            }
                        }
                        FieldType::Json => {
                        }
                        FieldType::DateTime => {
                            if let Some(s) = v.as_str() {
                                if s.parse::<DateTime<Utc>>().is_err() {
                                    return Err(SchemaError::InvalidType(
                                        field.name.clone(),
                                        "ISO 8601 DateTime".to_string(),
                                        s.to_string(),
                                    ));
                                }
                            } else {
                                return Err(SchemaError::InvalidType(
                                    field.name.clone(),
                                    "DateTime string".to_string(),
                                    v.to_string(),
                                ));
                            }
                        }
                        FieldType::File => {
                            if !v.is_string() && !v.is_object() {
                                return Err(SchemaError::InvalidType(
                                    field.name.clone(),
                                    "File object or string".to_string(),
                                    v.to_string(),
                                ));
                            }
                        }
                        FieldType::Relation { .. } => {
                            if !v.is_string() {
                                return Err(SchemaError::InvalidType(
                                    field.name.clone(),
                                    "Relation (ID string)".to_string(),
                                    v.to_string(),
                                ));
                            }
                        }
                    }
                }
            }
        }

        if !doc_keys.is_empty() {
            let undefined_field = doc_keys.iter().next().unwrap();
            return Err(SchemaError::UndefinedField(undefined_field.clone()));
        }

        Ok(())
    }
}
