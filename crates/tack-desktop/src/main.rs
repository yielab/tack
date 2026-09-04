// The app that supervises `tack`: no window of its own content, no API of its
// own — it spawns (or attaches to) the real server and loads its web UI.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod lifecycle;
mod supervisor;
mod tray;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::CommandChild;

use supervisor::{
    DEFAULT_PORT, Outcome, ServerFolders, SidecarHandle, SidecarLauncher, SupervisorError,
    attach_or_start, shutdown,
};

/// [`SidecarHandle`] backed by the real Tauri sidecar child.
struct TauriSidecarHandle(CommandChild);

impl SidecarHandle for TauriSidecarHandle {
    fn pid(&self) -> u32 {
        self.0.pid()
    }

    fn kill(self) -> std::io::Result<()> {
        self.0
            .kill()
            .map_err(|e| std::io::Error::other(e.to_string()))
    }
}

/// [`SidecarLauncher`] that spawns the bundled `tack` binary through
/// `tauri-plugin-shell`'s sidecar mechanism (ADR 0062 decision 2).
struct TauriLauncher {
    app: tauri::AppHandle,
    port: u16,
}

impl SidecarLauncher for TauriLauncher {
    type Process = TauriSidecarHandle;

    fn spawn(&self, env: &[(String, String)]) -> Result<Self::Process, SupervisorError> {
        let mut command = self
            .app
            .shell()
            .sidecar("tack")
            .map_err(|e| SupervisorError::SpawnFailed(e.to_string()))?
            .args(["serve", "--with-runner"])
            .env("TACK_HOST", "127.0.0.1")
            .env("TACK_PORT", self.port.to_string());
        for (key, value) in env {
            command = command.env(key, value);
        }
        let (_events, child) = command
            .spawn()
            .map_err(|e| SupervisorError::SpawnFailed(e.to_string()))?;
        Ok(TauriSidecarHandle(child))
    }
}

/// Holds whatever the supervisor decided so the shutdown path (on app exit)
/// knows whether there is a child to stop. `None` until the async setup task
/// resolves; `Attached` is never touched on exit (rule: never stop a server
/// this app did not start).
enum ServerMode {
    Attached,
    Started(TauriSidecarHandle),
}

struct DesktopState(Mutex<Option<ServerMode>>);

/// Where the sidecar's data lives until VII-B3 computes the pinned per-OS
/// root from `dirs::data_dir()`. A temp directory keeps this card's proofs
/// (attach/spawn/shutdown) independent of that later card.
fn temporary_data_root() -> PathBuf {
    std::env::temp_dir().join("tack-desktop-dev")
}

fn temporary_folders(root: &Path) -> ServerFolders {
    ServerFolders {
        database_url: format!("sqlite:{}/tack.db?mode=rwc", root.display()),
        storage_dir: root.join("storage"),
        runner_state_dir: root.join("runner"),
        log_file: root.join("logs/tack.log"),
    }
}

fn main() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        // Must be registered before any other plugin (tauri-plugin-single-instance's
        // own requirement): a second launch focuses the existing window instead of
        // opening a new one.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            lifecycle::show_and_focus(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(DesktopState(Mutex::new(None)))
        .setup(|app| {
            let handle = app.handle().clone();
            let port = DEFAULT_PORT;
            let base_url = format!("http://127.0.0.1:{port}");
            let root = temporary_data_root();
            std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
            let folders = temporary_folders(&root);

            tray::ensure_launch_at_login_default_on_first_run(&handle, &root);
            tray::build(&handle).map_err(|e| e.to_string())?;

            tauri::async_runtime::spawn(async move {
                let client = reqwest::Client::new();
                let launcher = TauriLauncher {
                    app: handle.clone(),
                    port,
                };

                match attach_or_start(&client, &base_url, port, &launcher, &folders).await {
                    Ok(outcome) => {
                        let mode = match outcome {
                            Outcome::Attached { health } => {
                                tracing::info!(version = %health.version, status = %health.status, "attached to an existing Tack server");
                                ServerMode::Attached
                            }
                            Outcome::Started { health, process } => {
                                tracing::info!(version = %health.version, status = %health.status, pid = process.pid(), "started the Tack server");
                                ServerMode::Started(process)
                            }
                        };
                        if let Some(state) = handle.try_state::<DesktopState>() {
                            *state.0.lock().unwrap() = Some(mode);
                        }

                        let url = base_url.parse().expect("base_url is a valid URL");
                        if let Err(err) =
                            WebviewWindowBuilder::new(&handle, "main", WebviewUrl::External(url))
                                .title("Tack")
                                .inner_size(1200.0, 800.0)
                                .build()
                        {
                            tracing::error!(error = %err, "failed to open the main window");
                            handle.exit(1);
                        }
                    }
                    Err(SupervisorError::PortOccupiedByOther(port)) => {
                        tracing::error!(port, "port is occupied by something that is not Tack");
                        handle
                            .dialog()
                            .message(format!(
                                "Port {port} is already in use by something other than Tack. \
                                 Close whatever is using it and reopen Tack."
                            ))
                            .title("Tack")
                            .kind(MessageDialogKind::Error)
                            .blocking_show();
                        handle.exit(1);
                    }
                    Err(err) => {
                        tracing::error!(error = %err, "could not attach to or start a Tack server");
                        handle.exit(1);
                    }
                }
            });

            Ok(())
        })
        .on_window_event(lifecycle::handle_window_event)
        .build(tauri::generate_context!())
        .expect("error while building the tauri application")
        .run(|app_handle, event| {
            // The window hides rather than closes, so this fires only from
            // an explicit Quit or an external signal — never from a window
            // close. Either way, a spawned child must never outlive the app.
            if let RunEvent::ExitRequested { .. } = event
                && let Some(state) = app_handle.try_state::<DesktopState>()
            {
                let mode = state.0.lock().unwrap().take();
                if let Some(ServerMode::Started(process)) = mode
                    && let Err(err) = shutdown(process)
                {
                    tracing::error!(error = %err, "failed to stop the spawned Tack server");
                }
            }
        });
}
