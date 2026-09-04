//! The app's own settings.json and the first-run dialog that creates it.
//!
//! No settings.json yet means this is the first launch: a native dialog
//! (no second frontend) names the data root and offers pointing at an
//! existing tack.db instead of starting fresh. Every later launch finds
//! settings.json already there and is silent.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::paths::DataPaths;
use crate::supervisor::DEFAULT_PORT;

/// The app's own settings.json. Holds a database-path override and the
/// port -- nothing else; every other server default is untouched.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    #[serde(default)]
    pub database_path: Option<PathBuf>,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

impl Default for Settings {
    /// Not derived: a derived `Default` would give `port: 0` (`u16`'s own
    /// default) instead of the real default port.
    fn default() -> Self {
        Self {
            database_path: None,
            port: default_port(),
        }
    }
}

impl Settings {
    fn load(path: &std::path::Path) -> Option<Self> {
        let bytes = std::fs::read(path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        let json = serde_json::to_vec_pretty(self).expect("Settings always serializes");
        std::fs::write(path, json)
    }
}

/// What the first-run dialog produces, kept separate from the dialog calls
/// themselves so the settings this app would persist for a given answer are
/// unit-testable without a live window.
fn settings_for_answer(use_existing: bool, chosen_file: Option<PathBuf>) -> Settings {
    Settings {
        database_path: if use_existing { chosen_file } else { None },
        port: DEFAULT_PORT,
    }
}

/// Loads settings.json if it exists; otherwise runs the first-run dialog,
/// persists the answer, and returns it. `app` is only reached on the branch
/// where settings.json is missing, so every launch after the first never
/// shows a dialog.
pub fn ensure_settings(app: &tauri::AppHandle, paths: &DataPaths) -> Settings {
    if let Some(settings) = Settings::load(&paths.settings_file) {
        return settings;
    }

    let use_existing = app
        .dialog()
        .message(format!(
            "Tack stores its data at:\n{}\n\nUse an existing tack.db instead of starting fresh?",
            paths.root.display()
        ))
        .title("Welcome to Tack")
        .buttons(MessageDialogButtons::YesNo)
        .kind(MessageDialogKind::Info)
        .blocking_show();

    let chosen_file = if use_existing {
        app.dialog()
            .file()
            .add_filter("Tack database", &["db"])
            .blocking_pick_file()
            .and_then(|file| file.into_path().ok())
    } else {
        None
    };

    let settings = settings_for_answer(use_existing, chosen_file);
    if let Err(err) = settings.save(&paths.settings_file) {
        tracing::error!(error = %err, "failed to write settings.json");
    }
    settings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declining_an_existing_database_keeps_the_default() {
        let settings = settings_for_answer(false, Some(PathBuf::from("/ignored.db")));
        assert_eq!(settings.database_path, None);
        assert_eq!(settings.port, DEFAULT_PORT);
    }

    #[test]
    fn accepting_without_picking_a_file_keeps_the_default() {
        // The user says "yes, use an existing one" but cancels the file
        // picker -- must not silently invent a path.
        let settings = settings_for_answer(true, None);
        assert_eq!(settings.database_path, None);
    }

    #[test]
    fn accepting_and_picking_a_file_records_it() {
        let chosen = PathBuf::from("/home/alice/backup/tack.db");
        let settings = settings_for_answer(true, Some(chosen.clone()));
        assert_eq!(settings.database_path, Some(chosen));
    }

    #[test]
    fn settings_round_trip_through_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        let original = Settings {
            database_path: Some(PathBuf::from("/data/existing.db")),
            port: 4444,
        };
        original.save(&path).unwrap();

        let loaded = Settings::load(&path).expect("just-saved settings.json must load");
        assert_eq!(loaded, original);
    }

    #[test]
    fn load_returns_none_when_settings_json_does_not_exist() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        assert_eq!(Settings::load(&path), None);
    }

    #[test]
    fn load_returns_none_for_unparseable_settings_json() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, b"not json").unwrap();
        assert_eq!(Settings::load(&path), None);
    }
}
