//! `tack service {install,uninstall,status}`: a user-level daemon for the
//! terminal path — a systemd user unit on Linux, a launchd agent on macOS —
//! so `tack serve --with-runner` outlives the shell that started it, the
//! same way the desktop app outlives its window. Windows has no supported
//! implementation here; the desktop app is the answer on that platform.
//!
//! The unit/plist only ever set the four `TACK_*` folder variables below —
//! never a `tack.toml` lookup — so the service's data root is not affected
//! by whatever directory happened to be current when it was installed.

use std::path::{Path, PathBuf};
use std::process::Command;

const LAUNCHD_LABEL: &str = "com.yielab.tack";
const HEALTH_URL: &str = "http://127.0.0.1:3210/api/health";

/// `tack service` has no implementation on this platform. Distinct from a
/// bare string error so it can be matched on rather than parsed from prose.
#[derive(Debug)]
pub struct UnsupportedPlatform;

impl std::fmt::Display for UnsupportedPlatform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "tack service is not supported on this platform; install the Tack desktop app instead"
        )
    }
}

impl std::error::Error for UnsupportedPlatform {}

/// The three platforms this command tells apart, decided once from
/// `std::env::consts::OS` and then threaded through as data so the
/// unsupported branch is exercised in tests without needing a Windows box.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Platform {
    Linux,
    MacOs,
    Other,
}

fn current_os() -> Platform {
    match std::env::consts::OS {
        "linux" => Platform::Linux,
        "macos" => Platform::MacOs,
        _ => Platform::Other,
    }
}

pub fn install() -> anyhow::Result<()> {
    install_for(current_os())
}

pub fn uninstall() -> anyhow::Result<()> {
    uninstall_for(current_os())
}

pub fn status() -> anyhow::Result<()> {
    status_for(current_os())
}

fn install_for(platform: Platform) -> anyhow::Result<()> {
    match platform {
        Platform::Linux => install_systemd(),
        Platform::MacOs => install_launchd(),
        Platform::Other => Err(UnsupportedPlatform.into()),
    }
}

fn uninstall_for(platform: Platform) -> anyhow::Result<()> {
    match platform {
        Platform::Linux => uninstall_systemd(),
        Platform::MacOs => uninstall_launchd(),
        Platform::Other => Err(UnsupportedPlatform.into()),
    }
}

fn status_for(platform: Platform) -> anyhow::Result<()> {
    match platform {
        Platform::Linux => status_systemd(),
        Platform::MacOs => status_launchd(),
        Platform::Other => Err(UnsupportedPlatform.into()),
    }
}

// ─── Data root, shared by both platforms ───────────────────────────────────

/// `dirs::data_dir()/tack` — the OS's per-user application-data folder.
/// Fixed to agree with `tack-desktop`'s own data-folder code without the two
/// sharing a dependency.
fn data_root() -> anyhow::Result<PathBuf> {
    dirs::data_dir()
        .map(|dir| dir.join("tack"))
        .ok_or_else(|| anyhow::anyhow!("could not determine this OS's per-user data directory"))
}

/// Creates the data root `0700` on Unix, then the subdirectories the server
/// expects under it. The root's permissions alone keep every path beneath it
/// unreachable to another local user, since traversal requires execute
/// permission on every ancestor directory.
fn ensure_data_root(root: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(root)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))?;
    }
    for sub in ["storage", "runner", "logs"] {
        std::fs::create_dir_all(root.join(sub))?;
    }
    Ok(())
}

/// The four `TACK_*` variables that hand the data root to the server,
/// matching TODO.md §VII.0's table exactly.
fn env_vars(root: &Path) -> [(&'static str, String); 4] {
    [
        (
            "TACK_DATABASE_URL",
            format!("sqlite:{}?mode=rwc", root.join("tack.db").display()),
        ),
        (
            "TACK_STORAGE_DIR",
            root.join("storage").display().to_string(),
        ),
        (
            "TACK_RUNNER_STATE_DIR",
            root.join("runner").display().to_string(),
        ),
        (
            "TACK_LOG_FILE",
            root.join("logs").join("tack.log").display().to_string(),
        ),
    ]
}

fn binary_path() -> anyhow::Result<PathBuf> {
    let path = std::env::current_exe()?;
    path.canonicalize().map_err(|e| {
        anyhow::anyhow!(
            "could not resolve an absolute path for the running binary ({}): {e}",
            path.display()
        )
    })
}

fn run_checked(program: &str, args: &[&str]) -> anyhow::Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|e| anyhow::anyhow!("failed to run `{program} {}`: {e}", args.join(" ")))?;
    if !status.success() {
        anyhow::bail!("`{program} {}` exited with {status}", args.join(" "));
    }
    Ok(())
}

fn capture(program: &str, args: &[&str]) -> anyhow::Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| anyhow::anyhow!("failed to run `{program} {}`: {e}", args.join(" ")))?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

// ─── Linux: systemd user unit ──────────────────────────────────────────────

fn systemd_unit_path() -> anyhow::Result<PathBuf> {
    let config_dir = dirs::config_dir().ok_or_else(|| {
        anyhow::anyhow!("could not determine this OS's per-user config directory")
    })?;
    Ok(config_dir.join("systemd").join("user").join("tack.service"))
}

/// Renders the unit file's exact bytes. A pure function of its inputs so
/// tests can byte-assert it without a real systemd or filesystem.
fn systemd_unit_contents(binary: &Path, root: &Path) -> String {
    let mut unit = String::new();
    unit.push_str("[Unit]\n");
    unit.push_str("Description=Tack project management (agent execution service)\n");
    unit.push_str("After=network-online.target\n");
    unit.push_str("Wants=network-online.target\n");
    unit.push('\n');
    unit.push_str("[Service]\n");
    unit.push_str("Type=simple\n");
    unit.push_str(&format!("WorkingDirectory={}\n", root.display()));
    for (key, value) in env_vars(root) {
        unit.push_str(&format!("Environment={key}={value}\n"));
    }
    unit.push_str(&format!(
        "ExecStart={} serve --with-runner\n",
        binary.display()
    ));
    unit.push_str("Restart=on-failure\n");
    unit.push('\n');
    unit.push_str("[Install]\n");
    unit.push_str("WantedBy=default.target\n");
    unit
}

fn install_systemd() -> anyhow::Result<()> {
    let root = data_root()?;
    ensure_data_root(&root)?;
    let binary = binary_path()?;
    let unit_path = systemd_unit_path()?;
    if let Some(parent) = unit_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&unit_path, systemd_unit_contents(&binary, &root))?;

    run_checked("systemctl", &["--user", "daemon-reload"])?;
    run_checked("systemctl", &["--user", "enable", "--now", "tack"])?;

    println!("Installed and started the tack user service.");
    println!("  Unit file: {}", unit_path.display());
    println!("  Data root: {}", root.display());
    println!("  Health:    {HEALTH_URL}");
    Ok(())
}

fn uninstall_systemd() -> anyhow::Result<()> {
    // Disabling an already-absent unit fails; that failure is not this
    // command's problem to report, only to not crash on.
    let _ = run_checked("systemctl", &["--user", "disable", "--now", "tack"]);
    let unit_path = systemd_unit_path()?;
    if unit_path.exists() {
        std::fs::remove_file(&unit_path)?;
    }
    let _ = run_checked("systemctl", &["--user", "daemon-reload"]);

    println!("Removed the tack user service. The data root was left untouched.");
    Ok(())
}

fn status_systemd() -> anyhow::Result<()> {
    let state = capture("systemctl", &["--user", "is-active", "tack"])?;
    println!("State:  {state}");
    println!("Health: {HEALTH_URL}");
    Ok(())
}

// ─── macOS: launchd agent ───────────────────────────────────────────────────

fn launchd_plist_path() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("could not determine the home directory"))?;
    Ok(home
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{LAUNCHD_LABEL}.plist")))
}

/// Renders the plist's exact bytes. A pure function of its inputs so tests
/// can byte-assert it without a real launchd or filesystem.
fn launchd_plist_contents(binary: &Path, root: &Path) -> String {
    let mut plist = String::new();
    plist.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    plist.push_str(
        "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
    );
    plist.push_str("<plist version=\"1.0\">\n");
    plist.push_str("<dict>\n");
    plist.push_str("    <key>Label</key>\n");
    plist.push_str(&format!("    <string>{LAUNCHD_LABEL}</string>\n"));
    plist.push_str("    <key>ProgramArguments</key>\n");
    plist.push_str("    <array>\n");
    plist.push_str(&format!("        <string>{}</string>\n", binary.display()));
    plist.push_str("        <string>serve</string>\n");
    plist.push_str("        <string>--with-runner</string>\n");
    plist.push_str("    </array>\n");
    plist.push_str("    <key>WorkingDirectory</key>\n");
    plist.push_str(&format!("    <string>{}</string>\n", root.display()));
    plist.push_str("    <key>RunAtLoad</key>\n");
    plist.push_str("    <true/>\n");
    plist.push_str("    <key>KeepAlive</key>\n");
    plist.push_str("    <true/>\n");
    plist.push_str("    <key>EnvironmentVariables</key>\n");
    plist.push_str("    <dict>\n");
    for (key, value) in env_vars(root) {
        plist.push_str(&format!("        <key>{key}</key>\n"));
        plist.push_str(&format!("        <string>{value}</string>\n"));
    }
    plist.push_str("    </dict>\n");
    plist.push_str("</dict>\n");
    plist.push_str("</plist>\n");
    plist
}

fn launchd_gui_target() -> anyhow::Result<String> {
    let uid = capture("id", &["-u"])?;
    Ok(format!("gui/{uid}"))
}

fn install_launchd() -> anyhow::Result<()> {
    let root = data_root()?;
    ensure_data_root(&root)?;
    let binary = binary_path()?;
    let plist_path = launchd_plist_path()?;
    if let Some(parent) = plist_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&plist_path, launchd_plist_contents(&binary, &root))?;

    let target = launchd_gui_target()?;
    run_checked(
        "launchctl",
        &["bootstrap", &target, &plist_path.to_string_lossy()],
    )?;

    println!("Installed and started the tack launch agent.");
    println!("  Plist:     {}", plist_path.display());
    println!("  Data root: {}", root.display());
    println!("  Health:    {HEALTH_URL}");
    Ok(())
}

fn uninstall_launchd() -> anyhow::Result<()> {
    if let Ok(gui) = launchd_gui_target() {
        let target = format!("{gui}/{LAUNCHD_LABEL}");
        let _ = run_checked("launchctl", &["bootout", &target]);
    }
    let plist_path = launchd_plist_path()?;
    if plist_path.exists() {
        std::fs::remove_file(&plist_path)?;
    }

    println!("Removed the tack launch agent. The data root was left untouched.");
    Ok(())
}

fn status_launchd() -> anyhow::Result<()> {
    let gui = launchd_gui_target()?;
    let target = format!("{gui}/{LAUNCHD_LABEL}");
    let state = if Command::new("launchctl")
        .args(["print", &target])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
    {
        "active"
    } else {
        "inactive"
    };
    println!("State:  {state}");
    println!("Health: {HEALTH_URL}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(case: &str) -> PathBuf {
        std::env::temp_dir().join(format!("tack-service-test-{case}-{}", std::process::id()))
    }

    #[test]
    fn install_is_unsupported_on_other_platforms() {
        let err = install_for(Platform::Other).unwrap_err();
        assert!(err.downcast_ref::<UnsupportedPlatform>().is_some());
        assert!(err.to_string().contains("desktop app"));
    }

    #[test]
    fn uninstall_is_unsupported_on_other_platforms() {
        let err = uninstall_for(Platform::Other).unwrap_err();
        assert!(err.downcast_ref::<UnsupportedPlatform>().is_some());
    }

    #[test]
    fn status_is_unsupported_on_other_platforms() {
        let err = status_for(Platform::Other).unwrap_err();
        assert!(err.downcast_ref::<UnsupportedPlatform>().is_some());
    }

    #[test]
    fn systemd_unit_has_the_expected_keys() {
        let root = Path::new("/home/alice/.local/share/tack");
        let binary = Path::new("/home/alice/.local/bin/tack");
        let unit = systemd_unit_contents(binary, root);

        assert_eq!(
            unit,
            "[Unit]\n\
Description=Tack project management (agent execution service)\n\
After=network-online.target\n\
Wants=network-online.target\n\
\n\
[Service]\n\
Type=simple\n\
WorkingDirectory=/home/alice/.local/share/tack\n\
Environment=TACK_DATABASE_URL=sqlite:/home/alice/.local/share/tack/tack.db?mode=rwc\n\
Environment=TACK_STORAGE_DIR=/home/alice/.local/share/tack/storage\n\
Environment=TACK_RUNNER_STATE_DIR=/home/alice/.local/share/tack/runner\n\
Environment=TACK_LOG_FILE=/home/alice/.local/share/tack/logs/tack.log\n\
ExecStart=/home/alice/.local/bin/tack serve --with-runner\n\
Restart=on-failure\n\
\n\
[Install]\n\
WantedBy=default.target\n"
        );
    }

    #[test]
    fn launchd_plist_has_the_expected_keys() {
        let root = Path::new("/Users/alice/Library/Application Support/tack");
        let binary = Path::new("/Users/alice/.local/bin/tack");
        let plist = launchd_plist_contents(binary, root);

        // A raw string, not `\n\`-continued lines: Rust's backslash-newline
        // continuation eats the following line's leading whitespace, which
        // would silently strip every bit of the indentation being pinned.
        assert_eq!(
            plist,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.yielab.tack</string>
    <key>ProgramArguments</key>
    <array>
        <string>/Users/alice/.local/bin/tack</string>
        <string>serve</string>
        <string>--with-runner</string>
    </array>
    <key>WorkingDirectory</key>
    <string>/Users/alice/Library/Application Support/tack</string>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>EnvironmentVariables</key>
    <dict>
        <key>TACK_DATABASE_URL</key>
        <string>sqlite:/Users/alice/Library/Application Support/tack/tack.db?mode=rwc</string>
        <key>TACK_STORAGE_DIR</key>
        <string>/Users/alice/Library/Application Support/tack/storage</string>
        <key>TACK_RUNNER_STATE_DIR</key>
        <string>/Users/alice/Library/Application Support/tack/runner</string>
        <key>TACK_LOG_FILE</key>
        <string>/Users/alice/Library/Application Support/tack/logs/tack.log</string>
    </dict>
</dict>
</plist>
"#
        );
    }

    #[test]
    fn ensure_data_root_creates_root_and_subdirs_0700_on_unix() {
        let root = test_dir("ensure-root");
        std::fs::remove_dir_all(&root).ok();

        ensure_data_root(&root).unwrap();

        assert!(root.join("storage").is_dir());
        assert!(root.join("runner").is_dir());
        assert!(root.join("logs").is_dir());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&root).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700);
        }

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn env_vars_match_the_four_documented_variables() {
        let root = Path::new("/data/tack");
        let vars = env_vars(root);
        let keys: Vec<&str> = vars.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            keys,
            vec![
                "TACK_DATABASE_URL",
                "TACK_STORAGE_DIR",
                "TACK_RUNNER_STATE_DIR",
                "TACK_LOG_FILE",
            ]
        );
    }
}
