use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlauConfig {
    #[serde(default = "default_http_port")]
    pub http_port: u16,

    #[serde(default = "default_tcp_port")]
    pub tcp_port: u16,

    #[serde(default = "default_db_path")]
    pub db_path: String,

    #[serde(default = "default_admin_password")]
    pub admin_password: String,

    #[serde(default = "default_uploads_dir")]
    pub uploads_dir: String,

    #[serde(default = "default_backups_dir")]
    pub backups_dir: String,

    #[serde(default = "default_max_upload_bytes")]
    pub max_upload_bytes: usize,

    #[serde(default = "default_log_level")]
    pub log_level: String,

    #[serde(default)]
    pub jwt_secret: Option<String>,

    #[serde(default)]
    pub cors_origins: Vec<String>,

    #[serde(default = "default_rate_limit")]
    pub rate_limit_per_minute: u32,
}

fn default_http_port() -> u16 { 8080 }
fn default_tcp_port() -> u16 { 8081 }
fn default_db_path() -> String { "glaudb.db".to_string() }
fn default_admin_password() -> String { String::new() }
fn default_uploads_dir() -> String { "uploads".to_string() }
fn default_backups_dir() -> String { "backups".to_string() }
fn default_max_upload_bytes() -> usize { 10 * 1024 * 1024 }
fn default_log_level() -> String { "info".to_string() }
fn default_rate_limit() -> u32 { 1000 }

impl Default for GlauConfig {
    fn default() -> Self {
        Self {
            http_port: default_http_port(),
            tcp_port: default_tcp_port(),
            db_path: default_db_path(),
            admin_password: default_admin_password(),
            uploads_dir: default_uploads_dir(),
            backups_dir: default_backups_dir(),
            max_upload_bytes: default_max_upload_bytes(),
            log_level: default_log_level(),
            jwt_secret: None,
            cors_origins: vec![],
            rate_limit_per_minute: default_rate_limit(),
        }
    }
}

impl GlauConfig {
    pub fn load() -> Self {
        let mut config = if let Ok(content) = std::fs::read_to_string("glaudb.json") {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            GlauConfig::default()
        };

        if let Ok(value) = std::env::var("GLAUDB_ADMIN_PASSWORD") {
            config.admin_password = value;
        }
        if let Ok(value) = std::env::var("GLAUDB_JWT_SECRET") {
            config.jwt_secret = Some(value);
        }
        if let Ok(value) = std::env::var("GLAUDB_DB_PATH") {
            config.db_path = value;
        }

        config
    }
}
