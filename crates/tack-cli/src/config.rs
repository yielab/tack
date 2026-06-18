use anyhow::Context;
use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
struct RcFile {
    base_url: Option<String>,
    token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub base_url: String,
    pub token: Option<String>,
}

impl Config {
    /// Build config from CLI overrides → env vars → ~/.tackrc → defaults.
    pub fn load(base_url_override: Option<String>, token_override: Option<String>) -> Self {
        let rc = read_rc_file().unwrap_or_default();

        let base_url = base_url_override
            .or_else(|| std::env::var("TACK_API_URL").ok())
            .or(rc.base_url)
            .unwrap_or_else(|| "http://127.0.0.1:3210".to_string());

        let token = token_override
            .or_else(|| std::env::var("TACK_API_TOKEN").ok())
            .or(rc.token);

        Self { base_url, token }
    }
}

/// Write base_url (and optionally token) to ~/.tackrc.
pub fn save(base_url: &str, token: Option<&str>) -> anyhow::Result<()> {
    let home = std::env::var("HOME").context("HOME environment variable not set")?;
    let path = std::path::Path::new(&home).join(".tackrc");
    let mut content = format!("base_url = \"{base_url}\"\n");
    if let Some(t) = token {
        content.push_str(&format!("token = \"{t}\"\n"));
    }
    std::fs::write(&path, content)
        .with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

fn read_rc_file() -> Option<RcFile> {
    let home = std::env::var("HOME").ok()?;
    let path = std::path::Path::new(&home).join(".tackrc");
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}
