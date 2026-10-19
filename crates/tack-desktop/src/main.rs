// The app that supervises `tack`: no window of its own content, no API of its
// own — it spawns (or attaches to) the real server and loads its web UI.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod first_run;
mod lifecycle;
mod paths;
mod supervisor;
mod tray;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};

use paths::DataPaths;
use supervisor::{
    ExitReport, Outcome, SidecarHandle, SidecarLauncher, SupervisorError, attach_or_start, shutdown,
};

/// [`SidecarHandle`] backed by the real Tauri sidecar child. `CommandChild`
/// has no `try_wait`; the exit arrives as a `Terminated` event on the
/// receiver `spawn` returns. That receiver is drained continuously by a task
/// spawned in [`TauriLauncher::spawn`], never polled lazily here: the
/// plugin's channel holds one event and its stdout/stderr reader threads
/// block on every send, so a receiver read only every few seconds would
/// stall the child's own output pipe and, behind it, the server. The task
/// keeps only the terminated payload, in `exit`.
struct TauriSidecarHandle {
    child: CommandChild,
    exit: Arc<Mutex<Option<ExitReport>>>,
}

impl SidecarHandle for TauriSidecarHandle {
    fn pid(&self) -> u32 {
        self.child.pid()
    }

    fn kill(self) -> std::io::Result<()> {
        self.child
            .kill()
            .map_err(|e| std::io::Error::other(e.to_string()))
    }

    fn exited(&mut self) -> Option<ExitReport> {
        *self.exit.lock().unwrap()
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
        let (mut events, child) = command
            .spawn()
            .map_err(|e| SupervisorError::SpawnFailed(e.to_string()))?;
        let exit = Arc::new(Mutex::new(None));
        let exit_for_drain = Arc::clone(&exit);
        tauri::async_runtime::spawn(async move {
            // Stdout and stderr lines are dropped here on purpose: the
            // server writes its own log, and this app only needs to know
            // when the process ends. The loop ends when the plugin drops its
            // sender, which happens after `Terminated`.
            while let Some(event) = events.recv().await {
                if let CommandEvent::Terminated(payload) = event {
                    *exit_for_drain.lock().unwrap() = Some(ExitReport {
                        code: payload.code,
                        signal: payload.signal,
                    });
                }
            }
        });
        Ok(TauriSidecarHandle { child, exit })
    }
}

/// Holds whatever the supervisor decided so the shutdown path (on app exit)
/// knows whether there is a child to stop. `None` until the async setup task
/// resolves; `Attached` is never touched on exit (rule: never stop a server
/// this app did not start). `Stopped` replaces `Started` once the tray's
/// watch has observed the child exit on its own, so the exit-requested
/// handler below stops matching a process that is already gone.
enum ServerMode {
    Attached,
    Started(TauriSidecarHandle),
    Stopped,
}

struct DesktopState(Mutex<Option<ServerMode>>);

/// The one origin the window may show and the only one granted any command.
fn board_origin(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

/// True only for a URL on the board's own origin.
fn is_board_url(url: &tauri::Url, port: u16) -> bool {
    url.scheme() == "http" && url.host_str() == Some("127.0.0.1") && url.port() == Some(port)
}

/// The capability the board page gets, written from the port the supervisor
/// chose: the template's remote URL names no real port.
fn server_page_capability(port: u16) -> String {
    let mut capability: serde_json::Value =
        serde_json::from_str(include_str!("../capabilities/server-page.json"))
            .expect("server-page.json is valid JSON");
    capability["remote"]["urls"] = serde_json::json!([format!("{}/*", board_origin(port))]);
    capability.to_string()
}

/// The key `decisionTokenStore` reads in the board page
/// (`frontend/src/shared/execution/decisions.ts`), before the page's origin.
const DECISION_TOKEN_STORAGE_KEY: &str = "tack_execution_decision_token";

/// The script that hands the board's decision token to its window: it sets
/// the page's session storage under the exact key the page reads, for the
/// board's own origin only. `None` when the file is missing or empty, in
/// which case the page asks for the token as in a browser. Both strings are
/// JSON-encoded into the script, never interpolated raw, and the value is
/// never logged.
fn decision_token_script(token_file: &Path, origin: &str) -> Option<String> {
    let token = std::fs::read_to_string(token_file).ok()?;
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    let json = |value: &str| serde_json::to_string(value).expect("a string is valid JSON");
    Some(format!(
        "if (location.origin === {origin}) {{ try {{ sessionStorage.setItem({key}, {token}); }} catch (e) {{}} }}",
        origin = json(origin),
        key = json(&format!("{DECISION_TOKEN_STORAGE_KEY}:{origin}")),
        token = json(token),
    ))
}

/// Canonicalises `path` and returns it only when it is an existing directory
/// inside the canonical `repository` or the canonical `data_dir`. Symlinks and
/// `..` resolve before the prefix test, so neither can point outside.
fn checked_folder(path: &Path, repository: &Path, data_dir: &Path) -> Result<PathBuf, String> {
    let target = path
        .canonicalize()
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    if !target.is_dir() {
        return Err(format!("{} is not a folder", target.display()));
    }
    let allowed = [repository, data_dir]
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .any(|root| target.starts_with(root));
    if allowed {
        Ok(target)
    } else {
        Err(format!(
            "{} is outside the project's repository and Tack's data folder",
            target.display()
        ))
    }
}

/// Opens a folder in the system's file manager: the board's only command.
#[tauri::command]
fn open_folder(app: tauri::AppHandle, path: String, repository: String) -> Result<(), String> {
    let data_dir = DataPaths::resolve().map_err(|e| e.to_string())?.root;
    let target = checked_folder(Path::new(&path), Path::new(&repository), &data_dir)?;
    app.opener()
        .open_path(target.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

fn main() {
    // WebKitGTK's DMA-BUF renderer aborts the whole process when it cannot
    // open a GBM/EGL display (NVIDIA's driver, or any driver whose kernel
    // module and libraries disagree until a reboot): "Could not create GBM
    // EGL display". Without it WebKit falls back to rendering that works
    // everywhere. Set before any thread exists; an explicit value wins.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        // SAFETY: first statement of `main`, before any other thread runs.
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
    }
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
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![open_folder])
        .manage(DesktopState(Mutex::new(None)))
        .setup(|app| {
            let handle = app.handle().clone();

            // Only creates/locates the data directory — never touches
            // settings.json or shows a dialog. That has to wait for the
            // async task below: `setup()` runs on the main thread before
            // `.run()` starts the event loop, and `ensure_settings`'s
            // first-run dialog is a `blocking_show()`, which
            // tauri-plugin-dialog documents as unsafe to call on that
            // thread — it marshals the dialog itself onto the main
            // thread's event loop via `run_on_main_thread` and waits for
            // the answer there, so calling it from the thread whose loop
            // hasn't started yet deadlocks forever: no window, no log
            // line, no spawned sidecar, indistinguishable from the app
            // doing nothing at all.
            let paths = DataPaths::resolve().map_err(|e| e.to_string())?;

            tray::build(&handle).map_err(|e| e.to_string())?;
            lifecycle::exit_on_signal(handle.clone());

            tauri::async_runtime::spawn(async move {
                // Runs on a runtime worker thread, not the one `setup()`
                // ran on — safe for `blocking_show()` to call into.
                let settings = tauri::async_runtime::spawn_blocking({
                    let handle = handle.clone();
                    let paths = paths.clone();
                    move || first_run::ensure_settings(&handle, &paths)
                })
                .await
                .expect("ensure_settings must not panic");
                let port = settings.port;
                let base_url = board_origin(port);
                let nonce = supervisor::new_launch_nonce();
                let folders = paths.server_folders(settings.database_path.as_deref());

                let client = reqwest::Client::new();
                let launcher = TauriLauncher {
                    app: handle.clone(),
                    port,
                };

                match attach_or_start(
                    &client,
                    &base_url,
                    port,
                    &launcher,
                    &folders,
                    env!("CARGO_PKG_VERSION"),
                    &nonce,
                )
                .await
                {
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

                        // The page reaches `open_folder` and the opener only
                        // through this capability, which names the chosen
                        // port and nothing else.
                        if let Err(err) = handle.add_capability(server_page_capability(port)) {
                            tracing::error!(error = %err, "failed to grant the board page its capability");
                            handle.exit(1);
                            return;
                        }

                        let url = base_url.parse().expect("base_url is a valid URL");
                        let mut window =
                            WebviewWindowBuilder::new(&handle, "main", WebviewUrl::External(url))
                                .title("Tack")
                                .inner_size(1200.0, 800.0)
                                // Ctrl + / Ctrl − / Ctrl 0 zoom, as in a browser
                                // tab; off by default in a webview.
                                .zoom_hotkeys_enabled(true)
                                .on_navigation(move |url| is_board_url(url, port));
                        // The server has answered, so it has made its
                        // decision-token file; the window gets its value so
                        // nobody pastes it there.
                        let token_file =
                            paths.decision_token_file(settings.database_path.as_deref());
                        match decision_token_script(&token_file, &base_url) {
                            Some(script) => window = window.initialization_script(script),
                            None => tracing::warn!(
                                file = %token_file.display(),
                                "no decision token to hand to the window; the Inbox will ask for it"
                            ),
                        }
                        if let Err(err) = window.build() {
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
                    Err(SupervisorError::OutdatedServer {
                        server_version,
                        bundled_version,
                    }) => {
                        tracing::error!(
                            server_version = %server_version,
                            bundled_version = %bundled_version,
                            "attached server is older than the bundled version"
                        );
                        handle
                            .dialog()
                            .message(format!(
                                "The Tack server already running is version {server_version}, \
                                 older than the {bundled_version} this app bundles. Update the \
                                 server, then reopen Tack."
                            ))
                            .title("Tack")
                            .kind(MessageDialogKind::Error)
                            .blocking_show();
                        handle.exit(1);
                    }
                    Err(err) => {
                        // Covers `HealthTimeout` and `SpawnFailed` — the two
                        // variants above already have their own tailored
                        // dialogs. `{err}` carries the specific reason
                        // (`thiserror`'s `Display`), including the bounded
                        // wait's own length for a timeout — never a generic
                        // apology, and never a longer sleep in its place.
                        tracing::error!(error = %err, "could not attach to or start a Tack server");
                        handle
                            .dialog()
                            .message(format!("Tack could not start: {err}"))
                            .title("Tack")
                            .kind(MessageDialogKind::Error)
                            .blocking_show();
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
            // an explicit Quit or a stop signal (`lifecycle::exit_on_signal`)
            // — never from a window close. Either way, a spawned child must never outlive the app.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_gets_the_decision_token_as_a_json_string_under_the_pages_own_key() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("decision-token");
        let origin = "http://127.0.0.1:3210";

        assert_eq!(
            decision_token_script(&file, origin),
            None,
            "no file, no script"
        );

        std::fs::write(&file, "  \n").unwrap();
        assert_eq!(decision_token_script(&file, origin), None, "an empty file");

        // A value with quotes and a script end must stay a string.
        let token = "ab\"c</script>';d";
        std::fs::write(&file, format!("{token}\n")).unwrap();
        let script = decision_token_script(&file, origin).expect("a script");
        let key =
            serde_json::to_string(&format!("tack_execution_decision_token:{origin}")).unwrap();
        let value = serde_json::to_string(token).unwrap();
        assert!(
            script.contains(&format!("sessionStorage.setItem({key}, {value})")),
            "{script}"
        );
        assert!(script.contains(&format!("location.origin === \"{origin}\"")));
    }

    #[test]
    fn open_folder_opens_only_inside_the_repository_or_the_data_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        let data = tmp.path().join("data");
        let outside = tmp.path().join("outside");
        for dir in [
            &repo,
            &data,
            &outside,
            &repo.join("src"),
            &data.join("logs"),
        ] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(repo.join("file.txt"), b"x").unwrap();

        let ok = |p: &Path| checked_folder(p, &repo, &data).is_ok();
        assert!(ok(&repo), "the repository itself");
        assert!(ok(&repo.join("src")), "inside the repository");
        assert!(ok(&data.join("logs")), "inside the data dir");
        assert!(!ok(&outside), "outside both");
        assert!(
            !ok(&repo.join("..").join("outside")),
            "`..` out of the repository"
        );
        assert!(!ok(&repo.join("missing")), "does not exist");
        assert!(!ok(&repo.join("file.txt")), "a file, not a folder");
        assert!(
            checked_folder(&outside, Path::new(""), &data).is_err(),
            "an empty repository grants nothing"
        );

        #[cfg(unix)]
        {
            let link = repo.join("escape");
            std::os::unix::fs::symlink(&outside, &link).unwrap();
            assert!(!ok(&link), "a symlink pointing outside");
        }
    }
}
