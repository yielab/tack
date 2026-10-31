use anyhow::Context;
use serde::Deserialize;

/// The variable `tack serve --runner-config` stands in for, naming the TOML file the embedded
/// runner loads, the one `tack runner start --config` reads. Without it the runner reads its
/// per-user default when that file exists; with neither it has defaults only, which leaves its
/// `[docket]`, `[verify]`, `[git]`, `[secrets]` and `[environment]` tables unset.
pub const RUNNER_CONFIG_ENV: &str = "TACK_LOCAL_RUNNER_CONFIG";

/// What the process environment says about the embedded runner and the database, read once by
/// whoever starts the server and handed down as values.
#[derive(Debug, Clone, Default)]
pub struct ServeEnvironment {
    /// `TACK_LOCAL_RUNNER_CONFIG`: the settings file the embedded runner loads. Set but empty,
    /// it names none.
    pub runner_config: Option<std::ffi::OsString>,
    /// `TACK_RUNNER_STATE_DIR` is set: the runner's state directory is the operator's, not the
    /// one scoped to the server's storage.
    pub runner_state_dir_named: bool,
    /// `TACK_DATABASE_URL` is set: the operator named the database.
    pub database_named: bool,
}

impl ServeEnvironment {
    pub fn from_process() -> Self {
        Self {
            runner_config: std::env::var_os(RUNNER_CONFIG_ENV),
            runner_state_dir_named: std::env::var_os("TACK_RUNNER_STATE_DIR").is_some(),
            database_named: std::env::var_os("TACK_DATABASE_URL").is_some(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
struct RcFile {
    base_url: Option<String>,
    token: Option<String>,
}

/// What the process environment says about reaching a server, read once and handed to
/// [`Config::load_with`] and [`save_in`] as values.
#[derive(Debug, Clone, Default)]
pub struct ClientEnvironment {
    /// `TACK_API_URL`.
    pub api_url: Option<String>,
    /// `TACK_API_TOKEN`.
    pub api_token: Option<String>,
    /// The user's home directory, where `~/.tackrc` lives.
    pub home: Option<std::path::PathBuf>,
    /// The folder a local `tack serve` keeps its files in ([`data_root`]), where its `api-token`
    /// file is.
    pub data_root: Option<std::path::PathBuf>,
}

impl ClientEnvironment {
    pub fn from_process() -> Self {
        Self {
            api_url: std::env::var("TACK_API_URL").ok(),
            api_token: std::env::var("TACK_API_TOKEN").ok(),
            home: std::env::var_os("HOME").map(std::path::PathBuf::from),
            data_root: data_root(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub base_url: String,
    pub token: Option<String>,
}

impl Config {
    /// Build config from CLI overrides → env vars → ~/.tackrc → the local server's `api-token` file → defaults.
    pub fn load(base_url_override: Option<String>, token_override: Option<String>) -> Self {
        Self::load_with(
            base_url_override,
            token_override,
            &ClientEnvironment::from_process(),
        )
    }

    /// [`Self::load`] over an environment the caller already read.
    pub fn load_with(
        base_url_override: Option<String>,
        token_override: Option<String>,
        environment: &ClientEnvironment,
    ) -> Self {
        let rc = read_rc_file(environment.home.as_deref()).unwrap_or_default();

        let base_url = base_url_override
            .or_else(|| environment.api_url.clone())
            .or(rc.base_url)
            .unwrap_or_else(|| "http://127.0.0.1:3210".to_string());

        let token = token_override
            .or_else(|| environment.api_token.clone())
            .or(rc.token)
            .or_else(|| read_server_token_file(environment.data_root.as_deref()));

        Self { base_url, token }
    }
}

/// Write base_url (and optionally token) to ~/.tackrc.
pub fn save(base_url: &str, token: Option<&str>) -> anyhow::Result<()> {
    save_in(
        ClientEnvironment::from_process().home.as_deref(),
        base_url,
        token,
    )
}

/// [`save`] into the `.tackrc` of `home`.
///
/// `~/.tackrc` can carry `TACK_API_TOKEN` — a credential — so the write goes
/// through `secure_fs::write_owner_only_atomic` (temp file + `fsync` +
/// rename, `0600`) instead of a plain `fs::write`, which would both leave a
/// window where a crash mid-write drops a torn file and rely on umask alone
/// to keep it unreadable by other local users.
pub fn save_in(
    home: Option<&std::path::Path>,
    base_url: &str,
    token: Option<&str>,
) -> anyhow::Result<()> {
    let home = home.context("HOME environment variable not set")?;
    let path = home.join(".tackrc");
    let mut content = format!("base_url = \"{base_url}\"\n");
    if let Some(t) = token {
        content.push_str(&format!("token = \"{t}\"\n"));
    }
    crate::secure_fs::write_owner_only_atomic(&path, content.as_bytes())
        .with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

fn read_rc_file(home: Option<&std::path::Path>) -> Option<RcFile> {
    let content = std::fs::read_to_string(home?.join(".tackrc")).ok()?;
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
fn read_server_token_file(data_root: Option<&std::path::Path>) -> Option<String> {
    let path = data_root?.join("api-token");
    let token = std::fs::read_to_string(path).ok()?.trim().to_string();
    (!token.is_empty()).then_some(token)
}

/// This computer's host name, empty when the system will not say. `hostname` answers first;
/// `HOSTNAME` and `COMPUTERNAME` stand in where that program is missing.
pub fn host_name() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .unwrap_or_default()
}

/// `TACK_RUNNER_SECRET_VALUE`: the value `tack secret set` stores, where it is not piped in.
pub fn secret_value_from_environment() -> Option<String> {
    std::env::var("TACK_RUNNER_SECRET_VALUE").ok()
}

/// `BROWSER`: the program `tack board` opens the board's URL with, when one is named.
pub fn browser_program() -> Option<String> {
    std::env::var("BROWSER")
        .ok()
        .filter(|browser| !browser.is_empty())
}
