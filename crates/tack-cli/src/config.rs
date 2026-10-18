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
    /// Build config from CLI overrides → env vars → ~/.tackrc → the local server's `api-token` file → defaults.
    pub fn load(base_url_override: Option<String>, token_override: Option<String>) -> Self {
        let rc = read_rc_file().unwrap_or_default();

        let base_url = base_url_override
            .or_else(|| std::env::var("TACK_API_URL").ok())
            .or(rc.base_url)
            .unwrap_or_else(|| "http://127.0.0.1:3210".to_string());

        let token = token_override
            .or_else(|| std::env::var("TACK_API_TOKEN").ok())
            .or(rc.token)
            .or_else(read_server_token_file);

        Self { base_url, token }
    }
}

/// Write base_url (and optionally token) to ~/.tackrc.
///
/// `~/.tackrc` can carry `TACK_API_TOKEN` — a credential — so the write goes
/// through `secure_fs::write_owner_only_atomic` (temp file + `fsync` +
/// rename, `0600`) instead of a plain `fs::write`, which would both leave a
/// window where a crash mid-write drops a torn file and rely on umask alone
/// to keep it unreadable by other local users.
pub fn save(base_url: &str, token: Option<&str>) -> anyhow::Result<()> {
    let home = std::env::var("HOME").context("HOME environment variable not set")?;
    let path = std::path::Path::new(&home).join(".tackrc");
    let mut content = format!("base_url = \"{base_url}\"\n");
    if let Some(t) = token {
        content.push_str(&format!("token = \"{t}\"\n"));
    }
    crate::secure_fs::write_owner_only_atomic(&path, content.as_bytes())
        .with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

fn read_rc_file() -> Option<RcFile> {
    let home = std::env::var("HOME").ok()?;
    let path = std::path::Path::new(&home).join(".tackrc");
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}

/// `dirs::data_dir()/tack` — the OS's per-user application-data folder, the one place the
/// terminal path's server keeps its files. Fixed to agree with `tack-desktop`'s own
/// data-folder code without the two sharing a dependency. It lives here, in the library,
/// because the token lookup below and the binary's `service` and `serve` all read it.
pub fn data_root() -> Option<std::path::PathBuf> {
    dirs::data_dir().map(|dir| dir.join("tack"))
}

/// The token a local `tack serve` keeps in `<data root>/api-token`.
fn read_server_token_file() -> Option<String> {
    let path = data_root()?.join("api-token");
    let token = std::fs::read_to_string(path).ok()?.trim().to_string();
    (!token.is_empty()).then_some(token)
}
