use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    AfterInsert,
    AfterUpdate,
    AfterDelete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Webhook {
    pub id: String,
    pub collection: String,
    pub event: HookEvent,
    pub url: String,
    pub active: bool,
    pub created: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookPayload {
    pub event: String,
    pub collection: String,
    pub record: serde_json::Value,
}
