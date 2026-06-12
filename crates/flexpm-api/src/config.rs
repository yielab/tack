use serde::{Deserialize, Serialize};

/// Application configuration loaded from flexpm.toml or environment variables.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_database_url")]
    pub database_url: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub log_json: bool,
    #[serde(default = "default_log_file")]
    pub log_file: Option<String>,
    #[serde(default = "default_storage_dir")]
    pub storage_dir: String,

    /// Origins allowed by CORS. Comma-separated in env ($FLEXPM_ALLOWED_ORIGINS).
    /// Defaults to localhost variants suitable for local-first use.
    #[serde(default = "default_allowed_origins")]
    pub allowed_origins: Vec<String>,

    /// Global maximum body size for non-attachment requests (bytes).
    /// Attachments use their own higher limit. Default: 2 MB.
    #[serde(default = "default_max_body_size_bytes")]
    pub max_body_size_bytes: usize,

    /// Optional Bearer token. When set, all `/api/*` routes (except `/api/health`)
    /// require `Authorization: Bearer <token>`. Leave unset for pure-local use.
    #[serde(default)]
    pub api_token: Option<String>,

    /// Optional Amazon Alexa skill ID (e.g. `amzn1.ask.skill.…`). When set,
    /// `POST /api/alexa` accepts requests from that skill (verified against the
    /// application ID in each request). Unset disables the endpoint entirely.
    #[serde(default)]
    pub alexa_skill_id: Option<String>,

    /// Optional outbound webhook URL. When set, FlexPM POSTs a JSON payload to
    /// this URL on every item create/update/delete, sprint status change, and
    /// when items become due within the next hour (background check every 60 min).
    #[serde(default)]
    pub webhook_url: Option<String>,

    /// Optional HMAC-SHA256 signing secret for webhook deliveries. When set,
    /// each request includes an `X-FlexPM-Signature: sha256=<hex>` header so
    /// the receiver can verify authenticity.
    #[serde(default)]
    pub webhook_secret: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            database_url: default_database_url(),
            log_level: default_log_level(),
            log_json: false,
            log_file: default_log_file(),
            storage_dir: default_storage_dir(),
            allowed_origins: default_allowed_origins(),
            max_body_size_bytes: default_max_body_size_bytes(),
            api_token: None,
            alexa_skill_id: None,
            webhook_url: None,
            webhook_secret: None,
        }
    }
}

fn default_host() -> String {
    "127.0.0.1".into()
}
fn default_port() -> u16 {
    3210
}
fn default_database_url() -> String {
    "sqlite:flexpm.db?mode=rwc".into()
}
fn default_log_level() -> String {
    "info".into()
}
fn default_log_file() -> Option<String> {
    None
}
fn default_storage_dir() -> String {
    "./storage".into()
}
fn default_max_body_size_bytes() -> usize {
    2 * 1024 * 1024
} // 2 MB

fn default_allowed_origins() -> Vec<String> {
    vec![
        "http://localhost:8080".into(),
        "http://127.0.0.1:8080".into(),
        "https://flexpm.test".into(),
    ]
}

impl AppConfig {
    /// Extract the filesystem path from the database URL.
    /// Returns `None` for in-memory databases.
    pub fn db_file_path(&self) -> Option<std::path::PathBuf> {
        let url = &self.database_url;
        if url.contains(":memory:") {
            return None;
        }
        let rest = url.strip_prefix("sqlite:")?;
        let rest = rest.split('?').next().unwrap_or(rest);
        if rest.is_empty() {
            return None;
        }
        Some(std::path::PathBuf::from(rest))
    }

    /// Load config from file, falling back to defaults.
    pub fn load() -> Self {
        if let Ok(content) = std::fs::read_to_string("flexpm.toml")
            && let Ok(config) = toml::from_str(&content)
        {
            return config;
        }

        let mut config = Self::default();
        if let Ok(v) = std::env::var("FLEXPM_HOST") {
            config.host = v;
        }
        if let Ok(v) = std::env::var("FLEXPM_PORT") {
            config.port = v.parse().unwrap_or(3210);
        }
        if let Ok(v) = std::env::var("FLEXPM_DATABASE_URL") {
            config.database_url = v;
        }
        if let Ok(v) = std::env::var("FLEXPM_LOG_LEVEL") {
            config.log_level = v;
        }
        if let Ok(v) = std::env::var("FLEXPM_LOG_JSON") {
            config.log_json = v == "true" || v == "1";
        }
        if let Ok(v) = std::env::var("FLEXPM_LOG_FILE") {
            config.log_file = Some(v);
        }
        if let Ok(v) = std::env::var("FLEXPM_STORAGE_DIR") {
            config.storage_dir = v;
        }
        if let Ok(v) = std::env::var("FLEXPM_ALLOWED_ORIGINS") {
            config.allowed_origins = v.split(',').map(|s| s.trim().to_string()).collect();
        }
        if let Ok(v) = std::env::var("FLEXPM_MAX_BODY_SIZE") {
            config.max_body_size_bytes = v.parse().unwrap_or(default_max_body_size_bytes());
        }
        // Never log the token value
        if let Ok(v) = std::env::var("FLEXPM_API_TOKEN")
            && !v.is_empty()
        {
            config.api_token = Some(v);
        }
        if let Ok(v) = std::env::var("FLEXPM_ALEXA_SKILL_ID")
            && !v.is_empty()
        {
            config.alexa_skill_id = Some(v);
        }
        if let Ok(v) = std::env::var("FLEXPM_WEBHOOK_URL")
            && !v.is_empty()
        {
            config.webhook_url = Some(v);
        }
        if let Ok(v) = std::env::var("FLEXPM_WEBHOOK_SECRET")
            && !v.is_empty()
        {
            config.webhook_secret = Some(v);
        }
        config
    }
}
