use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregateOp {
    Count,
    Sum,
    Avg,
    Min,
    Max,
    GroupBy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateStage {
    pub op: AggregateOp,
    pub field: Option<String>,
    pub group_by: Option<String>,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationPipeline {
    pub stages: Vec<AggregateStage>,
    pub filter: Option<crate::core::query::Query>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationResult {
    pub groups: Vec<Value>,
    pub total_docs: usize,
}

pub fn run_pipeline(
    docs: &[std::collections::HashMap<String, Value>],
    pipeline: &AggregationPipeline,
) -> AggregationResult {
    let filtered: Vec<&std::collections::HashMap<String, Value>> = if let Some(ref q) = pipeline.filter {
        docs.iter().filter(|d| q.matches(d)).collect()
    } else {
        docs.iter().collect()
    };

    let total_docs = filtered.len();

    if pipeline.stages.is_empty() {
        return AggregationResult { groups: vec![], total_docs };
    }

    let mut groups: Vec<Value> = Vec::new();

    for stage in &pipeline.stages {
        match stage.op {
            AggregateOp::Count => {
                let alias = stage.alias.as_deref().unwrap_or("count");
                groups.push(serde_json::json!({ alias: filtered.len() }));
            }

            AggregateOp::Sum => {
                if let Some(ref field) = stage.field {
                    let sum: f64 = filtered.iter()
                        .filter_map(|d| d.get(field))
                        .filter_map(|v| v.as_f64())
                        .sum();
                    let alias = stage.alias.as_deref().unwrap_or("sum");
                    groups.push(serde_json::json!({ alias: sum }));
                }
            }

            AggregateOp::Avg => {
                if let Some(ref field) = stage.field {
                    let vals: Vec<f64> = filtered.iter()
                        .filter_map(|d| d.get(field))
                        .filter_map(|v| v.as_f64())
                        .collect();
                    let avg = if vals.is_empty() { 0.0 } else { vals.iter().sum::<f64>() / vals.len() as f64 };
                    let alias = stage.alias.as_deref().unwrap_or("avg");
                    groups.push(serde_json::json!({ alias: avg }));
                }
            }

            AggregateOp::Min => {
                if let Some(ref field) = stage.field {
                    let min = filtered.iter()
                        .filter_map(|d| d.get(field))
                        .filter_map(|v| v.as_f64())
                        .fold(f64::INFINITY, f64::min);
                    let alias = stage.alias.as_deref().unwrap_or("min");
                    let val = if min.is_infinite() { Value::Null } else { serde_json::json!(min) };
                    groups.push(serde_json::json!({ alias: val }));
                }
            }

            AggregateOp::Max => {
                if let Some(ref field) = stage.field {
                    let max = filtered.iter()
                        .filter_map(|d| d.get(field))
                        .filter_map(|v| v.as_f64())
                        .fold(f64::NEG_INFINITY, f64::max);
                    let alias = stage.alias.as_deref().unwrap_or("max");
                    let val = if max.is_infinite() { Value::Null } else { serde_json::json!(max) };
                    groups.push(serde_json::json!({ alias: val }));
                }
            }

            AggregateOp::GroupBy => {
                if let Some(ref group_field) = stage.group_by {
                    let mut group_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
                    for doc in &filtered {
                        let key = doc.get(group_field)
                            .map(|v| match v {
                                Value::String(s) => s.clone(),
                                other => serde_json::to_string(other).unwrap_or_default(),
                            })
                            .unwrap_or_else(|| "null".to_string());
                        *group_map.entry(key).or_insert(0) += 1;
                    }

                    for (key, count) in group_map {
                        let mut obj = serde_json::Map::new();
                        obj.insert(group_field.clone(), Value::String(key));
                        obj.insert("count".to_string(), serde_json::json!(count));

                        if let Some(ref agg_field) = stage.field {
                            let _ = agg_field;
                        }
                        groups.push(Value::Object(obj));
                    }
                }
            }
        }
    }

    AggregationResult { groups, total_docs }
}
