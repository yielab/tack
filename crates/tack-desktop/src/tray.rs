//! System tray icon and menu (ADR 0062 decision 3): *Open Tack*, the agent
//! execution switch, *Launch at login*, and *Quit*. Tauri's tray icon does
//! not emit click events on Linux, so every action lives in the menu —
//! nothing here depends on clicking the icon itself.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

use crate::lifecycle;

const MENU_ID_OPEN: &str = "open";
const MENU_ID_AGENT_EXECUTION: &str = "agent_execution";
const MENU_ID_LAUNCH_AT_LOGIN: &str = "launch_at_login";
const MENU_ID_QUIT: &str = "quit";

/// The agent-execution switch reads `GET /api/local-runner`, which does not
/// exist yet (Part VI's VI-B3 has not landed). Disabled and labeled
/// accordingly rather than a second switch guessing at a shape that isn't
/// there yet.
const AGENT_EXECUTION_LABEL: &str = "Agent execution: unknown — the switch arrives with the Agents page";

/// Builds the tray icon and attaches its menu. Call once from `setup`, after
/// [`ensure_launch_at_login_default_on_first_run`] so the checkbox's initial
/// state matches whatever that just decided.
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let open_item = MenuItem::with_id(app, MENU_ID_OPEN, "Open Tack", true, None::<&str>)?;
    let agent_execution_item = MenuItem::with_id(
        app,
        MENU_ID_AGENT_EXECUTION,
        AGENT_EXECUTION_LABEL,
        false,
        None::<&str>,
    )?;
    let launch_at_login_checked = app.autolaunch().is_enabled().unwrap_or(false);
    let launch_at_login_item = CheckMenuItem::with_id(
        app,
        MENU_ID_LAUNCH_AT_LOGIN,
        "Launch at login",
        true,
        launch_at_login_checked,
        None::<&str>,
    )?;
    let quit_item = MenuItem::with_id(app, MENU_ID_QUIT, "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &open_item,
            &agent_execution_item,
            &PredefinedMenuItem::separator(app)?,
            &launch_at_login_item,
            &PredefinedMenuItem::separator(app)?,
            &quit_item,
        ],
    )?;

    let launch_at_login_for_handler = launch_at_login_item.clone();
    TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            MENU_ID_OPEN => lifecycle::show_and_focus(app),
            MENU_ID_LAUNCH_AT_LOGIN => {
                lifecycle::toggle_launch_at_login(app, &launch_at_login_for_handler)
            }
            MENU_ID_QUIT => lifecycle::quit(app.clone()),
            _ => {}
        })
        .build(app)?;

    Ok(())
}

/// Applies the "on by default" half of decision 3 exactly once: the first
/// time this app ever runs, launch-at-login is turned on; every later run
/// leaves whatever the user has it set to alone. The marker lives next to
/// B1's temporary data root — VII-B3's real first-run flow will fold it into
/// its own marker once persistent per-OS folders land; until then this is
/// self-contained and does not touch VII-B3's files.
pub fn ensure_launch_at_login_default_on_first_run(app: &AppHandle, data_root: &std::path::Path) {
    let marker = data_root.join(".autostart-initialized");
    if marker.exists() {
        return;
    }
    match app.autolaunch().enable() {
        Ok(()) => {
            if let Err(err) = std::fs::write(&marker, b"") {
                tracing::error!(error = %err, "failed to write the autostart first-run marker");
            }
            tracing::info!("launch at login enabled by default on first run");
        }
        Err(err) => {
            tracing::error!(error = %err, "failed to enable launch at login by default");
        }
    }
}
