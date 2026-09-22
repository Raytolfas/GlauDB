use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QueryOperator {
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
    Contains,
    #[serde(rename = "in")]
    In,
    #[serde(rename = "nin")]
    NotIn,
    #[serde(rename = "startswith")]
    StartsWith,
    #[serde(rename = "endswith")]
    EndsWith,
    #[serde(rename = "exists")]
    Exists,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    pub field: String,
    pub op: QueryOperator,
    pub value: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sort {
    pub field: String,
    pub desc: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    pub filters: Vec<Filter>,
    pub sort: Vec<Sort>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

impl Query {
    pub fn matches(&self, doc: &HashMap<String, Value>) -> bool {
        for filter in &self.filters {
            let doc_val = match doc.get(&filter.field) {
                Some(v) => v,
                None => &Value::Null,
            };

            if !eval_filter(doc_val, &filter.op, &filter.value) {
                return false;
            }
        }
        true
    }

    pub fn sort_documents(&self, docs: &mut [HashMap<String, Value>]) {
        if self.sort.is_empty() {
            return;
        }

        docs.sort_by(|a, b| {
            for sort_rule in &self.sort {
                let val_a = a.get(&sort_rule.field).unwrap_or(&Value::Null);
                let val_b = b.get(&sort_rule.field).unwrap_or(&Value::Null);

                let cmp = compare_values(val_a, val_b);
                if cmp != std::cmp::Ordering::Equal {
                    return if sort_rule.desc { cmp.reverse() } else { cmp };
                }
            }
            std::cmp::Ordering::Equal
        });
    }
}

fn eval_filter(doc_val: &Value, op: &QueryOperator, filter_val: &Value) -> bool {
    match op {
        QueryOperator::Eq => doc_val == filter_val,
        QueryOperator::Neq => doc_val != filter_val,
        QueryOperator::Gt => compare_values(doc_val, filter_val) == std::cmp::Ordering::Greater,
        QueryOperator::Gte => {
            let cmp = compare_values(doc_val, filter_val);
            cmp == std::cmp::Ordering::Greater || cmp == std::cmp::Ordering::Equal
        }
        QueryOperator::Lt => compare_values(doc_val, filter_val) == std::cmp::Ordering::Less,
        QueryOperator::Lte => {
            let cmp = compare_values(doc_val, filter_val);
            cmp == std::cmp::Ordering::Less || cmp == std::cmp::Ordering::Equal
        }
        QueryOperator::Contains => {
            match (doc_val, filter_val) {
                (Value::String(s), Value::String(f)) => s.to_lowercase().contains(&f.to_lowercase()),
                (Value::Array(arr), _) => arr.contains(filter_val),
                _ => false,
            }
        }
        QueryOperator::In => {
            if let Value::Array(arr) = filter_val {
                arr.contains(doc_val)
            } else {
                false
            }
        }
        QueryOperator::NotIn => {
            if let Value::Array(arr) = filter_val {
                !arr.contains(doc_val)
            } else {
                true
            }
        }
        QueryOperator::StartsWith => {
            match (doc_val, filter_val) {
                (Value::String(s), Value::String(f)) => s.to_lowercase().starts_with(&f.to_lowercase()),
                _ => false,
            }
        }
        QueryOperator::EndsWith => {
            match (doc_val, filter_val) {
                (Value::String(s), Value::String(f)) => s.to_lowercase().ends_with(&f.to_lowercase()),
                _ => false,
            }
        }
        QueryOperator::Exists => {
            let should_exist = filter_val.as_bool().unwrap_or(true);
            let exists = doc_val != &Value::Null;
            exists == should_exist
        }
    }
}

fn compare_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (a, b) {
        (Value::Null, Value::Null) => std::cmp::Ordering::Equal,
        (Value::Null, _) => std::cmp::Ordering::Less,
        (_, Value::Null) => std::cmp::Ordering::Greater,
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (Value::Number(x), Value::Number(y)) => {
            if let (Some(xf), Some(yf)) = (x.as_f64(), y.as_f64()) {
                xf.partial_cmp(&yf).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                std::cmp::Ordering::Equal
            }
        }
        (Value::String(x), Value::String(y)) => {
            if let (Ok(xd), Ok(yd)) = (x.parse::<DateTime<Utc>>(), y.parse::<DateTime<Utc>>()) {
                xd.cmp(&yd)
            } else {
                x.cmp(y)
            }
        }
        _ => std::cmp::Ordering::Equal,
    }
}
