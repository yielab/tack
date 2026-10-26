//! Whether the embedded runner should run: the value the UI saved in the store, else the
//! environment or command-line default, read fresh on every call and never cached.

use serde::{Deserialize, Serialize};
use tack_db::{Repository, StoreError, queries};

const KEY: &str = "local_runner_config";

#[derive(Debug, Default, Serialize, Deserialize)]
struct Preference {
    #[serde(default)]
    enabled: Option<bool>,
}

/// The saved preference if the UI has ever set one, else `default_enabled` (the
/// `--with-runner` flag folded with its environment variable at load time).
pub async fn effective_enabled(repo: &Repository, default_enabled: bool) -> bool {
    let saved = queries::app_meta::value(repo, KEY).await.ok().flatten();
    saved
        .and_then(|raw| serde_json::from_str::<Preference>(&raw).ok())
        .and_then(|preference| preference.enabled)
        .unwrap_or(default_enabled)
}

/// Saves the preference. The caller reconciles the runtime afterwards: a crash between the two
/// still boots correctly next time.
pub async fn set_enabled(repo: &Repository, enabled: bool) -> Result<(), StoreError> {
    let value = serde_json::to_string(&Preference {
        enabled: Some(enabled),
    })
    .unwrap_or_else(|_| "{}".into());
    repo.set_app_meta(KEY, &value).await
}
