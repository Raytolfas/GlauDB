use crate::core::{
    Database, S3Client, S3Config, CollectionSchema, Query, ApiToken, TokenRole,
    hash_token_str, Webhook, WebhookPayload, AggregationPipeline, GlauConfig
};
use axum::{
    routing::{get, post, delete, put},
    Router,
    Json,
    extract::{Path, State, WebSocketUpgrade, Query as AxumQuery, FromRequestParts, ws::{Message, WebSocket}},
    http::{StatusCode, Uri, HeaderMap, HeaderName, HeaderValue, Method, header, request::Parts},
    response::{IntoResponse, Response},
    body::Body,
};
use tower_http::cors::{AllowOrigin, CorsLayer};
use rust_embed::Embed;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use sysinfo::System;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use jsonwebtoken::{encode, decode, Header, Algorithm, Validation, EncodingKey, DecodingKey};
use uuid::Uuid;

#[derive(Embed)]
#[folder = "glaudb-admin/dist/"]
#[prefix = "admin/"]
struct Assets;

#[derive(Clone)]
struct AppState {
    db: Database,
    config: GlauConfig,
    jwt_secret: String,
    tx_metrics: broadcast::Sender<SystemMetrics>,
    tx_events: broadcast::Sender<DatabaseEvent>,
    logs: Arc<Mutex<VecDeque<String>>>,
    rate_limiter: Arc<Mutex<HashMap<String, (u32, Instant)>>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SystemMetrics {
    cpu_usage: f32,
    memory_used: u64,
    memory_total: u64,
    db_size: u64,
    collections_count: usize,
    active_tcp_connections: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DatabaseEvent {
    pub event: String,
    pub collection: String,
    pub record: Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    role: String,
    exp: usize,
}

#[derive(Deserialize)]
struct QueryParams {
    expand: Option<bool>,
}

#[derive(Deserialize)]
struct BackupRequest {
    name: Option<String>,
}

struct AuthSession {
    role: TokenRole,
    #[allow(dead_code)]
    token_name: String,
}

impl AppState {
    fn log(&self, msg: &str) {
        let now = chrono::Utc::now().to_rfc3339();
        let formatted = format!("[{}] {}", now, msg);
        log::info!("{}", msg);
        if let Ok(mut guard) = self.logs.lock() {
            if guard.len() >= 500 {
                guard.pop_front();
            }
            guard.push_back(formatted);
        }
    }
}

#[axum::async_trait]
impl FromRequestParts<AppState> for AuthSession {
    type Rejection = (StatusCode, Json<Value>);

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let app_state = state;

        let ip = parts
            .headers
            .get("x-forwarded-for")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("127.0.0.1")
            .to_string();

        {
            let mut limiter = app_state.rate_limiter.lock().unwrap();
            let now = Instant::now();
            let limit = app_state.config.rate_limit_per_minute;

            let (tokens, last_update) = limiter
                .entry(ip.clone())
                .or_insert((limit, now));

            let elapsed = now.duration_since(*last_update).as_secs();
            if elapsed >= 60 {
                *tokens = limit;
                *last_update = now;
            }

            if *tokens == 0 {
                app_state.log(&format!("Rate limit exceeded for IP: {}", ip));
                return Err((
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(json!({ "error": "Too many requests. Rate limit exceeded." })),
                ));
            } else {
                *tokens -= 1;
            }
        }

        let auth_header = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok());

        let token = match auth_header {
            Some(h) if h.starts_with("Bearer ") => h.trim_start_matches("Bearer "),
            _ => {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": "Missing or invalid authorization token" })),
                ));
            }
        };

        let dec_key = DecodingKey::from_secret(app_state.jwt_secret.as_bytes());
        let validation = Validation::new(Algorithm::HS256);
        if let Ok(token_data) = decode::<Claims>(token, &dec_key, &validation) {
            let role = match token_data.claims.role.as_str() {
                "admin" => TokenRole::Admin,
                "write" => TokenRole::Write,
                _ => TokenRole::Read,
            };
            return Ok(AuthSession {
                role,
                token_name: token_data.claims.sub,
            });
        }

        let hash = hash_token_str(token);
        match app_state.db.get_token_by_hash(&hash) {
            Ok(Some(api_token)) => {
                if let Some(ref exp_str) = api_token.expires {
                    if let Ok(exp_time) = chrono::DateTime::parse_from_rfc3339(exp_str) {
                        if chrono::Utc::now() > exp_time {
                            return Err((
                                StatusCode::UNAUTHORIZED,
                                Json(json!({ "error": "API Token has expired" })),
                            ));
                        }
                    }
                }
                Ok(AuthSession {
                    role: api_token.role,
                    token_name: api_token.name,
                })
            }
            _ => Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Unauthorized or invalid credentials" })),
            )),
        }
    }
}

fn spawn_webhook_worker(state: AppState) {
    let mut rx = state.tx_events.subscribe();
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        while let Ok(event) = rx.recv().await {
            let webhooks = match state.db.get_webhooks_for(&event.collection, &event.event) {
                Ok(w) => w,
                Err(_) => continue,
            };

            for hook in webhooks {
                let payload = WebhookPayload {
                    event: event.event.clone(),
                    collection: event.collection.clone(),
                    record: event.record.clone(),
                };
                let url = hook.url.clone();
                let client_clone = client.clone();
                tokio::spawn(async move {
                    let _ = client_clone
                        .post(&url)
                        .json(&payload)
                        .timeout(Duration::from_secs(5))
                        .send()
                        .await;
                });
            }
        }
    });
}

pub async fn start_http_server(db: Database, config: GlauConfig, addr: &str) {
    let (tx_metrics, _) = broadcast::channel(16);
    let (tx_events, _) = broadcast::channel(128);
    let logs = Arc::new(Mutex::new(VecDeque::with_capacity(500)));
    let rate_limiter = Arc::new(Mutex::new(HashMap::new()));
    let jwt_secret = config.jwt_secret.clone().unwrap_or_else(|| Uuid::new_v4().to_string());

    let tx_metrics_clone = tx_metrics.clone();
    let db_metrics = db.clone();
    let db_path_str = config.db_path.clone();
    tokio::spawn(async move {
        let mut sys = System::new_all();
        loop {
            sys.refresh_all();
            let cpu = sys.global_cpu_info().cpu_usage();
            let memory_used = sys.used_memory();
            let memory_total = sys.total_memory();
            let db_size = std::fs::metadata(&db_path_str).map(|m| m.len()).unwrap_or(0);
            let collections_count = db_metrics.list_collections().map(|c| c.len()).unwrap_or(0);
            let active_tcp = crate::tcp::ACTIVE_TCP_CONNECTIONS.load(std::sync::atomic::Ordering::SeqCst);

            let _ = tx_metrics_clone.send(SystemMetrics {
                cpu_usage: cpu,
                memory_used,
                memory_total,
                db_size,
                collections_count,
                active_tcp_connections: active_tcp,
            });
            tokio::time::sleep(Duration::from_millis(1000)).await;
        }
    });

    let state = AppState {
        db,
        config,
        jwt_secret,
        tx_metrics,
        tx_events,
        logs,
        rate_limiter,
    };

    spawn_webhook_worker(state.clone());

    let _ = std::fs::create_dir_all(&state.config.uploads_dir);
    let _ = std::fs::create_dir_all(&state.config.backups_dir);

    let allowed_origins: Vec<HeaderValue> = state.config.cors_origins.iter()
        .filter_map(|origin| HeaderValue::from_str(origin).ok())
        .collect();
    let cors = if allowed_origins.is_empty() {
        CorsLayer::new()
    } else {
        CorsLayer::new()
            .allow_origin(AllowOrigin::list(allowed_origins))
            .allow_methods([Method::GET, Method::POST, Method::PUT, Method::PATCH, Method::DELETE])
            .allow_headers([
                header::AUTHORIZATION,
                header::CONTENT_TYPE,
                HeaderName::from_static("x-glaudb-query"),
            ])
    };

    let app = Router::new()
        .route("/api/login", post(login_handler))
        .route("/api/collections", get(list_collections).post(create_collection))
        .route("/api/collections/:name", put(update_collection).delete(delete_collection))
        .route("/api/collections/:name/records", get(query_records).post(insert_record))
        .route("/api/collections/:name/records/:id", get(get_record).put(update_record).patch(patch_record).delete(delete_record))
        .route("/api/collections/:name/bulk", post(bulk_insert_handler))
        .route("/api/collections/:name/bulk-delete", post(bulk_delete_handler))
        .route("/api/collections/:name/search", get(search_handler))
        .route("/api/collections/:name/aggregate", post(aggregate_handler))
        .route("/api/collections/:name/count", get(count_handler))
        .route("/api/collections/:name/upsert", post(upsert_handler))
        .route("/api/collections/:name/find-one", post(find_one_handler))
        .route("/api/tokens", get(list_tokens_handler).post(create_token_handler))
        .route("/api/tokens/:id", delete(revoke_token_handler))
        .route("/api/webhooks", get(list_webhooks_handler).post(create_webhook_handler))
        .route("/api/webhooks/:id", delete(delete_webhook_handler))
        .route("/api/settings", get(get_settings).post(save_settings))
        .route("/api/backup", post(backup_database))
        .route("/api/logs", get(get_logs_handler))
        .route("/api/upload", post(upload_file))
        .route("/api/metrics/ws", get(metrics_ws_handler))
        .route("/api/realtime/ws", get(realtime_ws_handler))
        .route("/uploads/*path", get(serve_upload))
        .fallback(serve_admin_assets)
        .layer(cors)
        .with_state(state);

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            log::error!("Failed to bind HTTP server to {}: {}", addr, e);
            return;
        }
    };
    log::info!("Admin HTTP & REST Server listening on http://{}", addr);
    axum::serve(listener, app).await.unwrap();
}


async fn serve_upload(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> impl IntoResponse {
    if path.contains("..") || path.contains('\\') {
        return (StatusCode::BAD_REQUEST, "Invalid file path").into_response();
    }
    let file_path = PathBuf::from(&state.config.uploads_dir).join(path.trim_start_matches('/'));
    if !file_path.exists() || !file_path.is_file() {
        return (StatusCode::NOT_FOUND, "File not found").into_response();
    }
    match tokio::fs::read(&file_path).await {
        Ok(bytes) => {
            let mime = mime_guess::from_path(&file_path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime.to_string())], bytes).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read file").into_response(),
    }
}

async fn serve_admin_assets(uri: Uri) -> Response {
    let mut path = uri.path().trim_start_matches('/').to_string();
    if path.is_empty() || path == "admin" || path == "admin/" {
        path = "admin/index.html".to_string();
    } else if !path.starts_with("admin/") {
        path = "admin/index.html".to_string();
    }
    match Assets::get(&path) {
        Some(content) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.to_string())
                .body(Body::from(content.data))
                .unwrap()
        }
        None => {
            if let Some(content) = Assets::get("admin/index.html") {
                Response::builder()
                    .header(header::CONTENT_TYPE, "text/html")
                    .body(Body::from(content.data))
                    .unwrap()
            } else {
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("Admin panel assets not embedded. Make sure to build frontend first."))
                    .unwrap()
            }
        }
    }
}


#[derive(Deserialize)]
struct LoginRequest {
    password: String,
}

async fn login_handler(State(state): State<AppState>, Json(req): Json<LoginRequest>) -> impl IntoResponse {
    if req.password == state.config.admin_password {
        let exp = chrono::Utc::now()
            .checked_add_signed(chrono::Duration::hours(24))
            .unwrap()
            .timestamp() as usize;

        let claims = Claims {
            sub: "admin".to_string(),
            role: "admin".to_string(),
            exp,
        };

        match encode(&Header::default(), &claims, &EncodingKey::from_secret(state.jwt_secret.as_bytes())) {
            Ok(token) => {
                state.log("Admin logged in successfully");
                (StatusCode::OK, Json(json!({ "token": token, "role": "admin" }))).into_response()
            }
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Token generation failed" }))).into_response(),
        }
    } else {
        state.log("Failed admin login attempt");
        (StatusCode::UNAUTHORIZED, Json(json!({ "error": "Invalid password" }))).into_response()
    }
}


async fn list_collections(
    _auth: AuthSession,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match state.db.list_collections() {
        Ok(cols) => (StatusCode::OK, Json(cols)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn create_collection(
    auth: AuthSession,
    State(state): State<AppState>,
    Json(schema): Json<CollectionSchema>,
) -> impl IntoResponse {
    if auth.role != TokenRole::Admin {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "Only admins can create collections" }))).into_response();
    }
    match state.db.create_collection(schema) {
        Ok(_) => StatusCode::CREATED.into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn update_collection(
    auth: AuthSession,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(schema): Json<CollectionSchema>,
) -> impl IntoResponse {
    if auth.role != TokenRole::Admin {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "Only admins can edit collections" }))).into_response();
    }
    match state.db.update_collection(&name, schema) {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn delete_collection(
    auth: AuthSession,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if auth.role != TokenRole::Admin {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "Only admins can delete collections" }))).into_response();
    }
    match state.db.delete_collection(&name) {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn query_records(
    _auth: AuthSession,
    State(state): State<AppState>,
    Path(name): Path<String>,
    AxumQuery(params): AxumQuery<QueryParams>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let query_str = headers.get("x-glaudb-query")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("{}");
    
    let query: Query = serde_json::from_str(query_str)
        .unwrap_or(Query { filters: vec![], sort: vec![], limit: None, offset: None });

    match state.db.query_documents(&name, query) {
        Ok(mut docs) => {
            if params.expand.unwrap_or(false) {
                let _ = state.db.expand_relations(&name, &mut docs);
            }
            (StatusCode::OK, Json(docs)).into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn get_record(
    _auth: AuthSession,
    State(state): State<AppState>,
    Path((name, id)): Path<(String, String)>,
    AxumQuery(params): AxumQuery<QueryParams>,
) -> impl IntoResponse {
    match state.db.get_document(&name, &id) {
        Ok(Some(mut doc)) => {
            if params.expand.unwrap_or(false) {
                let mut docs = vec![doc.clone()];
                let _ = state.db.expand_relations(&name, &mut docs);
                doc = docs.remove(0);
            }
            (StatusCode::OK, Json(doc)).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({ "error": "Record not found" }))).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn insert_record(
    auth: AuthSession,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(doc): Json<HashMap<String, Value>>,
) -> impl IntoResponse {
    if !auth.role.has_write() {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "Write permission required" }))).into_response();
    }
    match state.db.insert_document(&name, doc) {
        Ok(inserted) => {
            let record_val = serde_json::to_value(&inserted).unwrap_or(Value::Null);
            let _ = state.tx_events.send(DatabaseEvent {
                event: "insert".to_string(),
                collection: name.clone(),
                record: record_val,
            });
            (StatusCode::CREATED, Json(inserted)).into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn update_record(
    auth: AuthSession,
    State(state): State<AppState>,
    Path((name, id)): Path<(String, String)>,
    Json(doc): Json<HashMap<String, Value>>,
) -> impl IntoResponse {
    if !auth.role.has_write() {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "Write permission required" }))).into_response();
    }
    match state.db.update_document(&name, &id, doc) {
        Ok(updated) => {
            let record_val = serde_json::to_value(&updated).unwrap_or(Value::Null);
            let _ = state.tx_events.send(DatabaseEvent {
                event: "update".to_string(),
                collection: name.clone(),
                record: record_val,
            });
            (StatusCode::OK, Json(updated)).into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn patch_record(
    auth: AuthSession,
    State(state): State<AppState>,
    Path((name, id)): Path<(String, String)>,
    Json(patch): Json<HashMap<String, Value>>,
) -> impl IntoResponse {
    if !auth.role.has_write() {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "Write permission required" }))).into_response();
    }
    match state.db.patch_document(&name, &id, patch) {
        Ok(patched) => {
            let record_val = serde_json::to_value(&patched).unwrap_or(Value::Null);
            let _ = state.tx_events.send(DatabaseEvent {
                event: "update".to_string(),
                collection: name.clone(),
                record: record_val,
            });
            (StatusCode::OK, Json(patched)).into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn delete_record(
    auth: AuthSession,
    State(state): State<AppState>,
    Path((name, id)): Path<(String, String)>,
) -> impl IntoResponse {
    if !auth.role.has_write() {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "Write permission required" }))).into_response();
    }
    match state.db.get_document(&name, &id) {
        Ok(Some(old_doc)) => {
            match state.db.delete_document(&name, &id) {
                Ok(_) => {
                    let record_val = serde_json::to_value(&old_doc).unwrap_or(Value::Null);
                    let _ = state.tx_events.send(DatabaseEvent {
                        event: "delete".to_string(),
                        collection: name.clone(),
                        record: record_val,
                    });
                    StatusCode::NO_CONTENT.into_response()
                }
                Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
            }
        }
        _ => (StatusCode::NOT_FOUND, Json(json!({ "error": "Record not found" }))).into_response(),
    }
}


async fn bulk_insert_handler(
    auth: AuthSession,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(docs): Json<Vec<HashMap<String, Value>>>,
) -> impl IntoResponse {
    if !auth.role.has_write() {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "Write permission required" }))).into_response();
    }
    match state.db.bulk_insert(&name, docs) {
        Ok(inserted) => {
            let record_val = serde_json::to_value(&inserted).unwrap_or(Value::Null);
            let _ = state.tx_events.send(DatabaseEvent {
                event: "bulk_insert".to_string(),
                collection: name,
                record: record_val,
            });
            (StatusCode::CREATED, Json(inserted)).into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn bulk_delete_handler(
    auth: AuthSession,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(ids): Json<Vec<String>>,
) -> impl IntoResponse {
    if !auth.role.has_write() {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "Write permission required" }))).into_response();
    }
    match state.db.bulk_delete(&name, &ids) {
        Ok(count) => {
            let _ = state.tx_events.send(DatabaseEvent {
                event: "bulk_delete".to_string(),
                collection: name,
                record: json!({ "deleted_count": count, "ids": ids }),
            });
            (StatusCode::OK, Json(json!({ "count": count }))).into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}


#[derive(Deserialize)]
struct SearchParams {
    q: String,
    fields: Option<String>,
    limit: Option<usize>,
}

async fn search_handler(
    _auth: AuthSession,
    State(state): State<AppState>,
    Path(name): Path<String>,
    AxumQuery(params): AxumQuery<SearchParams>,
) -> impl IntoResponse {
    let fields: Vec<String> = params.fields.as_ref()
        .map(|f| f.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    match state.db.search_documents(&name, &params.q, &fields, params.limit) {
        Ok(docs) => (StatusCode::OK, Json(docs)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn aggregate_handler(
    _auth: AuthSession,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(pipeline): Json<AggregationPipeline>,
) -> impl IntoResponse {
    match state.db.aggregate(&name, &pipeline) {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn count_handler(
    _auth: AuthSession,
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let query_str = headers.get("x-glaudb-query")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("{}");
    
    let query: Option<Query> = serde_json::from_str(query_str).ok();
    match state.db.count_documents(&name, query.as_ref()) {
        Ok(count) => (StatusCode::OK, Json(json!({ "count": count }))).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

#[derive(Deserialize)]
struct UpsertRequest {
    match_field: String,
    data: serde_json::Map<String, Value>,
}

async fn upsert_handler(
    auth: AuthSession,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<UpsertRequest>,
) -> impl IntoResponse {
    if auth.role == TokenRole::Read {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "Write permission required" }))).into_response();
    }
    let doc: HashMap<String, Value> = req.data.into_iter().collect();
    match state.db.upsert_document(&name, &req.match_field, doc) {
        Ok((doc, inserted)) => {
            let event = if inserted { "insert" } else { "update" };
            state.log(&format!("Upsert ({}) in {} on field {}", event, name, req.match_field));
            let _ = state.tx_events.send(DatabaseEvent {
                event: if inserted { "after_insert".into() } else { "after_update".into() },
                collection: name.clone(),
                record: serde_json::to_value(&doc).unwrap_or(Value::Null),
            });
            let status = if inserted { StatusCode::CREATED } else { StatusCode::OK };
            (status, Json(json!({ "record": doc, "inserted": inserted }))).into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn find_one_handler(
    _auth: AuthSession,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(query): Json<Query>,
) -> impl IntoResponse {
    match state.db.find_one(&name, query) {
        Ok(Some(doc)) => (StatusCode::OK, Json(doc)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({ "error": "No matching document" }))).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}


#[derive(Deserialize)]
struct TokenCreateRequest {
    name: String,
    role: TokenRole,
    expires_in_hours: Option<u64>,
}

async fn create_token_handler(
    auth: AuthSession,
    State(state): State<AppState>,
    Json(req): Json<TokenCreateRequest>,
) -> impl IntoResponse {
    if auth.role != TokenRole::Admin {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "Only admins can manage API tokens" }))).into_response();
    }

    let token_uuid = Uuid::new_v4().to_string();
    let raw_key = format!("glau_tok_{}_{}", token_uuid, Uuid::new_v4().to_string().replace('-', ""));
    let hash = hash_token_str(&raw_key);

    let expires = req.expires_in_hours.map(|h| {
        chrono::Utc::now()
            .checked_add_signed(chrono::Duration::hours(h as i64))
            .unwrap()
            .to_rfc3339()
    });

    let api_token = ApiToken {
        id: token_uuid,
        name: req.name,
        key_hash: hash,
        role: req.role,
        created: chrono::Utc::now().to_rfc3339(),
        expires,
    };

    match state.db.create_token(&api_token) {
        Ok(_) => {
            state.log(&format!("Created API Token: {}", api_token.name));
            (StatusCode::CREATED, Json(json!({ "token": raw_key, "token_info": api_token }))).into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn list_tokens_handler(
    auth: AuthSession,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if auth.role != TokenRole::Admin {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "Only admins can manage API tokens" }))).into_response();
    }
    match state.db.list_tokens() {
        Ok(tokens) => {
            let mut safe_tokens = tokens;
            for t in &mut safe_tokens {
                t.key_hash = "[masked]".to_string();
            }
            (StatusCode::OK, Json(safe_tokens)).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn revoke_token_handler(
    auth: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if auth.role != TokenRole::Admin {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "Only admins can manage API tokens" }))).into_response();
    }
    match state.db.revoke_token(&id) {
        Ok(_) => {
            state.log(&format!("Revoked API Token ID: {}", id));
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}


async fn create_webhook_handler(
    auth: AuthSession,
    State(state): State<AppState>,
    Json(req): Json<Webhook>,
) -> impl IntoResponse {
    if auth.role != TokenRole::Admin {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "Only admins can manage webhooks" }))).into_response();
    }
    match state.db.create_webhook(&req) {
        Ok(_) => {
            state.log(&format!("Created webhook for collection: {}", req.collection));
            (StatusCode::CREATED, Json(req)).into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn list_webhooks_handler(
    auth: AuthSession,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if auth.role != TokenRole::Admin {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "Only admins can manage webhooks" }))).into_response();
    }
    match state.db.list_webhooks() {
        Ok(w) => (StatusCode::OK, Json(w)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn delete_webhook_handler(
    auth: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if auth.role != TokenRole::Admin {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "Only admins can manage webhooks" }))).into_response();
    }
    match state.db.delete_webhook(&id) {
        Ok(_) => {
            state.log(&format!("Revoked Webhook ID: {}", id));
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}


async fn get_settings(
    auth: AuthSession,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if auth.role != TokenRole::Admin {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "Admin required" }))).into_response();
    }
    match state.db.get_settings() {
        Ok(settings) => (StatusCode::OK, Json(settings)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn save_settings(
    auth: AuthSession,
    State(state): State<AppState>,
    Json(settings): Json<S3Config>,
) -> impl IntoResponse {
    if auth.role != TokenRole::Admin {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "Admin required" }))).into_response();
    }
    match state.db.save_settings(&settings) {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn backup_database(
    auth: AuthSession,
    State(state): State<AppState>,
    Json(req): Json<BackupRequest>,
) -> impl IntoResponse {
    if auth.role != TokenRole::Admin {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "Admin required" }))).into_response();
    }
    let requested_name = req.name.unwrap_or_else(|| {
        format!("glaudb-{}.db", chrono::Utc::now().format("%Y%m%dT%H%M%SZ"))
    });
    if requested_name.is_empty()
        || requested_name.contains("..")
        || requested_name.contains(['/', '\\'])
    {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid backup name" }))).into_response();
    }
    let name = if requested_name.ends_with(".db") {
        requested_name
    } else {
        format!("{}.db", requested_name)
    };
    let destination = PathBuf::from(&state.config.backups_dir).join(&name);

    match state.db.backup(destination.to_string_lossy().as_ref()) {
        Ok(_) => (StatusCode::OK, Json(json!({ "success": true, "name": name }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn get_logs_handler(
    auth: AuthSession,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if auth.role != TokenRole::Admin {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "Admin required" }))).into_response();
    }
    let guard = state.logs.lock().unwrap();
    let logs_vec: Vec<String> = guard.iter().cloned().collect();
    (StatusCode::OK, Json(logs_vec)).into_response()
}


async fn upload_file(
    auth: AuthSession,
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> impl IntoResponse {
    if !auth.role.has_write() {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "Write permission required" }))).into_response();
    }

    let mut file_data = Vec::new();
    let mut file_name = String::new();
    let mut content_type = "application/octet-stream".to_string();

    while let Some(field) = match multipart.next_field().await {
        Ok(field) => field,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid multipart upload" }))).into_response(),
    } {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            file_name = field.file_name().unwrap_or("file").to_string();
            content_type = field.content_type().unwrap_or("application/octet-stream").to_string();
            file_data = match field.bytes().await {
                Ok(bytes) => bytes.to_vec(),
                Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Failed to read upload" }))).into_response(),
            };
            break;
        }
    }

    if file_data.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "No file uploaded" }))).into_response();
    }
    if file_data.len() > state.config.max_upload_bytes {
        return (StatusCode::PAYLOAD_TOO_LARGE, Json(json!({ "error": "File exceeds configured upload limit" }))).into_response();
    }

    let file_uuid = uuid::Uuid::new_v4().to_string();
    let file_ext = std::path::Path::new(&file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .filter(|ext| ext.len() <= 10 && ext.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("bin");
    let target_key = format!("{}.{}", file_uuid, file_ext);

    let s3_config = state.db.get_settings().unwrap_or_default();

    if s3_config.enabled {
        let client = S3Client::new(s3_config);
        match client.upload_file(&target_key, &file_data, &content_type).await {
            Ok(url) => (StatusCode::OK, Json(json!({
                "url": url,
                "name": file_name,
                "size": file_data.len()
            }))).into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("S3 upload failed: {}", e) }))).into_response(),
        }
    } else {
        let dest = PathBuf::from(&state.config.uploads_dir).join(&target_key);
        match tokio::fs::write(&dest, &file_data).await {
            Ok(_) => {
                let url = format!("/uploads/{}", target_key);
                (StatusCode::OK, Json(json!({
                    "url": url,
                    "name": file_name,
                    "size": file_data.len()
                }))).into_response()
            }
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("Local write failed: {}", e) }))).into_response(),
        }
    }
}


async fn metrics_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_metrics_ws(socket, state))
}

async fn handle_metrics_ws(mut socket: WebSocket, state: AppState) {
    let mut rx = state.tx_metrics.subscribe();
    loop {
        tokio::select! {
            Ok(metrics) = rx.recv() => {
                let msg_text = serde_json::to_string(&metrics).unwrap_or_default();
                if socket.send(Message::Text(msg_text.into())).await.is_err() {
                    break;
                }
            }
            msg = socket.recv() => {
                if msg.is_none() {
                    break;
                }
            }
        }
    }
}


async fn realtime_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_realtime_ws(socket, state))
}

async fn handle_realtime_ws(mut socket: WebSocket, state: AppState) {
    let mut rx = state.tx_events.subscribe();
    loop {
        tokio::select! {
            Ok(event) = rx.recv() => {
                let msg_text = serde_json::to_string(&event).unwrap_or_default();
                if socket.send(Message::Text(msg_text.into())).await.is_err() {
                    break;
                }
            }
            msg = socket.recv() => {
                if msg.is_none() {
                    break;
                }
            }
        }
    }
}
