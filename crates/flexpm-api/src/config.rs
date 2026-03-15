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
        }
    }
}

fn default_host() -> String { "127.0.0.1".into() }
fn default_port() -> u16 { 3210 }
fn default_database_url() -> String { "sqlite:flexpm.db?mode=rwc".into() }
fn default_log_level() -> String { "info".into() }
fn default_log_file() -> Option<String> { None }
fn default_storage_dir() -> String { "./storage".into() }

impl AppConfig {
    /// Load config from file, falling back to defaults.
    pub fn load() -> Self {
        // Try flexpm.toml in current directory
        if let Ok(content) = std::fs::read_to_string("flexpm.toml") {
            if let Ok(config) = toml::from_str(&content) {
                return config;
            }
        }

        // Fall back to environment variables
        let mut config = Self::default();
        if let Ok(v) = std::env::var("FLEXPM_HOST") { config.host = v; }
        if let Ok(v) = std::env::var("FLEXPM_PORT") { config.port = v.parse().unwrap_or(3210); }
        if let Ok(v) = std::env::var("FLEXPM_DATABASE_URL") { config.database_url = v; }
        if let Ok(v) = std::env::var("FLEXPM_LOG_LEVEL") { config.log_level = v; }
        if let Ok(v) = std::env::var("FLEXPM_LOG_JSON") { config.log_json = v == "true" || v == "1"; }
        if let Ok(v) = std::env::var("FLEXPM_LOG_FILE") { config.log_file = Some(v); }
        if let Ok(v) = std::env::var("FLEXPM_STORAGE_DIR") { config.storage_dir = v; }
        config
    }
}
