//! Window and app lifecycle (ADR 0062 decision 3): closing the window hides
//! it instead of quitting, *Open Tack* shows and focuses it again, and Quit
//! is the only way to actually stop the app — warning first when an agent
//! attempt is in flight.

use std::time::Duration;

use serde::Deserialize;
use tauri::menu::CheckMenuItem;
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_dialog::{
    DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult,
};

const MAIN_WINDOW: &str = "main";

/// Execution states that count as "in flight" are everything except the
/// three terminal ones (`crates/tack-orch/src/execution_observability.rs`'s
/// `known_request_states` is the closed vocabulary this reads from: queued,
/// leased, preparing, running, waiting_decision, succeeded, failed,
/// cancelled, lost, needs_operator). `lost` and `needs_operator` are not
/// terminal — they mean the request has neither finished nor been given up
/// on — so both still count toward the Quit warning.
const TERMINAL_STATES: [&str; 3] = ["succeeded", "failed", "cancelled"];

#[derive(Debug, Deserialize)]
struct ExecutionSummary {
    state: String,
}

#[derive(Debug, Deserialize)]
struct ExecutionListResponse {
    data: Vec<ExecutionSummary>,
}

/// Installed as the app-wide window event handler. The only window this app
/// ever has is `"main"`; closing it hides rather than quits so the server
/// keeps running underneath — Quit (tray menu) is the only way to stop it.
pub fn handle_window_event(window: &tauri::Window, event: &WindowEvent) {
    if window.label() != MAIN_WINDOW {
        return;
    }
    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        if let Err(err) = window.hide() {
            tracing::error!(error = %err, "failed to hide the main window on close");
        }
    }
}

/// *Open Tack* (tray) and a second launch (single-instance) both land here:
/// show the one window and bring it to the front.
pub fn show_and_focus(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        tracing::error!("no main window to show and focus");
        return;
    };
    if let Err(err) = window.show() {
        tracing::error!(error = %err, "failed to show the main window");
    }
    if let Err(err) = window.set_focus() {
        tracing::error!(error = %err, "failed to focus the main window");
    }
}

/// Flips launch-at-login and reflects the result back onto the checkbox.
/// Left unchanged (and logged) rather than guessed at if the OS call fails,
/// so the checkbox never claims a state that was not actually applied.
pub fn toggle_launch_at_login(app: &AppHandle, item: &CheckMenuItem<tauri::Wry>) {
    let manager = app.autolaunch();
    let currently_enabled = manager.is_enabled().unwrap_or(false);
    let result = if currently_enabled {
        manager.disable()
    } else {
        manager.enable()
    };
    match result {
        Ok(()) => {
            if let Err(err) = item.set_checked(!currently_enabled) {
                tracing::error!(error = %err, "failed to update the launch-at-login checkbox");
            }
            tracing::info!(enabled = !currently_enabled, "launch at login toggled");
        }
        Err(err) => {
            tracing::error!(error = %err, "failed to toggle launch at login");
        }
    }
}

/// The tray's Quit item. Counts in-flight attempts through
/// `GET /api/executions`; with any, blocks on a confirmation dialog before
/// doing anything else. `AppHandle::exit` triggers `RunEvent::ExitRequested`,
/// where `main.rs`'s existing handler stops a server this app spawned and
/// leaves an attached one alone — this function only decides whether that
/// happens at all, never how.
pub fn quit(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let base_url = format!("http://127.0.0.1:{}", crate::supervisor::DEFAULT_PORT);
        let client = reqwest::Client::new();
        let in_flight = count_in_flight_executions(&client, &base_url).await;

        if in_flight > 0 {
            let message = quit_warning_message(in_flight);
            let app_for_dialog = app.clone();
            let confirmed = tauri::async_runtime::spawn_blocking(move || {
                app_for_dialog
                    .dialog()
                    .message(message)
                    .title("Tack")
                    .kind(MessageDialogKind::Warning)
                    .buttons(MessageDialogButtons::OkCancel)
                    .blocking_show_with_result()
            })
            .await
            .map(|result| matches!(result, MessageDialogResult::Ok))
            .unwrap_or(false);

            if !confirmed {
                return;
            }
        }

        app.exit(0);
    });
}

fn quit_warning_message(in_flight: usize) -> String {
    let noun = if in_flight == 1 {
        "attempt"
    } else {
        "attempts"
    };
    let verb = if in_flight == 1 { "is" } else { "are" };
    format!("{in_flight} agent {noun} {verb} running. Quit anyway?")
}

/// A fetch or parse failure counts as zero in flight: Quit must never hang
/// or become unusable because a health check hiccuped. The graceful-then-kill
/// shutdown a spawned server gets either way does not depend on this count.
async fn count_in_flight_executions(client: &reqwest::Client, base_url: &str) -> usize {
    let url = format!("{base_url}/api/executions");
    let Ok(response) = client
        .get(&url)
        .timeout(Duration::from_secs(3))
        .send()
        .await
    else {
        return 0;
    };
    let Ok(body) = response.json::<ExecutionListResponse>().await else {
        return 0;
    };
    body.data
        .into_iter()
        .filter(|execution| !TERMINAL_STATES.contains(&execution.state.as_str()))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn quit_warning_message_agrees_with_the_count() {
        assert_eq!(
            quit_warning_message(1),
            "1 agent attempt is running. Quit anyway?"
        );
        assert_eq!(
            quit_warning_message(3),
            "3 agent attempts are running. Quit anyway?"
        );
    }

    /// A one-shot raw HTTP server: accepts a single connection, ignores the
    /// request, and replies with a fixed JSON body shaped like
    /// `GET /api/executions`'s response. Exercises the real `reqwest` parse
    /// path, not a hand-built `ExecutionListResponse`.
    fn serve_once(body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        });
        format!("http://127.0.0.1:{port}")
    }

    #[tokio::test]
    async fn counts_only_non_terminal_states() {
        let base_url = serve_once(
            r#"{"protocol_version":1,"data":[
                {"request_id":"a","item_id":"i","state":"queued","cancellation_requested_at":null,"created_at":"2026-01-01T00:00:00Z"},
                {"request_id":"b","item_id":"i","state":"running","cancellation_requested_at":null,"created_at":"2026-01-01T00:00:00Z"},
                {"request_id":"c","item_id":"i","state":"needs_operator","cancellation_requested_at":null,"created_at":"2026-01-01T00:00:00Z"},
                {"request_id":"d","item_id":"i","state":"succeeded","cancellation_requested_at":null,"created_at":"2026-01-01T00:00:00Z"},
                {"request_id":"e","item_id":"i","state":"failed","cancellation_requested_at":null,"created_at":"2026-01-01T00:00:00Z"},
                {"request_id":"f","item_id":"i","state":"cancelled","cancellation_requested_at":null,"created_at":"2026-01-01T00:00:00Z"}
            ]}"#,
        );
        let client = reqwest::Client::new();

        let count = count_in_flight_executions(&client, &base_url).await;

        assert_eq!(
            count, 3,
            "queued, running and needs_operator are in flight; the rest are terminal"
        );
    }

    #[tokio::test]
    async fn treats_an_unreachable_server_as_zero_in_flight() {
        // Nothing bound this port — the connection itself fails before any
        // response, exercising the same fail-open path as a timeout.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base_url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        drop(listener);
        let client = reqwest::Client::new();

        let count = count_in_flight_executions(&client, &base_url).await;

        assert_eq!(count, 0, "a fetch failure must never block Quit");
    }
}
