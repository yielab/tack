//! Running a harness inside `bwrap` where the machine has it.
//!
//! [`detect`] is read once per runner start. [`Sandbox::wrap`] turns the
//! program and arguments a spawn would run into a `bwrap` command line that
//! shows the harness only what a run needs: the workspace and a scratch home
//! read-write, the system directories read-only, a private `/tmp`, `/proc`
//! and `/dev`, no IPC or hostname shared with the host and no
//! capabilities. Of a CLI's own directory under the person's home (`~/.claude`)
//! it shows the settings read-only and the login as it is; the rest of the
//! directory is the scratch home's. The runner's state directory, `~/.tackrc`, `~/.ssh` and the
//! token file are never bound, so the harness cannot see them. The network
//! is left alone, unless the run is [`Sandbox::bridged`]: then the harness has no network of its
//! own and reaches the outside only through the runner's proxy, by a Unix socket the sandbox
//! shows and a forwarder inside it.

use std::{
    collections::BTreeMap,
    ffi::OsStr,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// What the runner could wrap a harness in on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxKind {
    Bwrap,
    None,
}

impl SandboxKind {
    /// The wire word the capability report and the start event carry.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bwrap => "bwrap",
            Self::None => "none",
        }
    }
}

/// A CLI's own directory under `$HOME` (`.claude`, `.codex`): what of it a run
/// reads, and the login it shares. Everything else in it is the run's own,
/// empty and thrown away with the scratch home, so the settings, hooks and
/// plugins the person keeps there cannot be rewritten from inside the
/// sandbox and what the CLI writes while it works (sessions, caches,
/// databases) has somewhere to go.
#[derive(Debug, Clone, Copy)]
pub struct ConfigDir {
    /// The directory, relative to `$HOME`.
    pub name: &'static str,
    /// Entries of it the run only reads, bound read-only when they exist.
    pub read_only: &'static [&'static str],
    /// Entries of it that hold the login. The CLI refreshes them in place, so
    /// they are shared as they are and not copied.
    pub login: &'static [&'static str],
}

/// Files of the person's a run sees, each `(source, destination)` with the
/// destination in the scratch home: some it writes in place, some it only
/// reads.
#[derive(Debug, Clone, Copy, Default)]
pub struct SharedFiles<'a> {
    pub writable: &'a [(PathBuf, PathBuf)],
    pub read_only: &'a [(PathBuf, PathBuf)],
}

/// The way a sandboxed run reaches the runner's proxy once it has no network of its own: the
/// proxy listens on a Unix socket in `socket_dir`, which the sandbox shows at
/// [`BRIDGE_SOCKET_DIR`], and `helper` (the runner's own executable, started as
/// [`BRIDGE_ARG`]) listens on [`BRIDGE_PORT`] of the sandbox's loopback and forwards to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bridge {
    pub helper: PathBuf,
    pub socket_dir: PathBuf,
}

/// The first argument that makes the runner's executable the forwarder inside a sandbox.
pub const BRIDGE_ARG: &str = "__tack-egress-bridge";
/// The port on the sandbox's own loopback where the agent finds the proxy.
pub const BRIDGE_PORT: u16 = 3128;
/// Where the sandbox shows the directory holding the proxy's socket.
pub const BRIDGE_SOCKET_DIR: &str = "/run/tack-egress";
/// The proxy's socket inside that directory.
pub const BRIDGE_SOCKET_NAME: &str = "proxy.sock";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sandbox {
    pub kind: SandboxKind,
    /// Why there is no sandbox, or where `bwrap` was found.
    pub reason: String,
    bwrap: Option<PathBuf>,
    /// The runner's `$HOME`, where the scratch home and the shared
    /// directories are bound.
    home: Option<PathBuf>,
    /// Whether the runner looked for a sandbox, rather than having none
    /// because it never asked (a test, or before discovery).
    looked: bool,
    /// Whether an agent that finds none must not run: the administrator's `[sandbox]` setting.
    /// Off, an agent without a sandbox runs but asks before each action.
    required: bool,
    /// Whether `bwrap` is installed but the system does not let it make a sandbox (on Ubuntu,
    /// AppArmor refusing a background service the user namespace bwrap needs).
    refused: bool,
    /// How a run reaches the runner's proxy without a network of its own; set per run by
    /// [`Self::bridged`].
    bridge: Option<Bridge>,
}

impl Sandbox {
    /// This sandbox for a run that has no network of its own: [`Self::wrap`] then adds
    /// `--unshare-net` and starts the program behind the forwarder to the proxy. Without a
    /// `bwrap` there is nothing to close the network with and nothing changes.
    pub fn bridged(&self, bridge: Bridge) -> Self {
        Self {
            bridge: Some(bridge),
            ..self.clone()
        }
    }

    /// Whether this machine can wrap a program in a sandbox that holds its network shut.
    pub fn can_close_network(&self) -> bool {
        self.bwrap.is_some()
    }

    /// This machine's sandbox with the kind a harness that cannot start in
    /// it gets: no wrapping, and the reason why.
    pub fn without(&self, reason: String) -> Self {
        Self {
            kind: SandboxKind::None,
            reason,
            bwrap: None,
            home: self.home.clone(),
            looked: true,
            required: self.required,
            refused: false,
            bridge: None,
        }
    }

    /// Whether `bwrap` is installed but this computer does not let it make a sandbox.
    pub fn is_refused(&self) -> bool {
        self.refused
    }

    /// What a person does so that an agent this machine cannot contain is contained.
    pub fn remedy(&self) -> &'static str {
        if self.refused {
            "let bwrap make a sandbox on this computer (on Ubuntu 24.04 and later, an AppArmor \
             profile for bwrap; see \"What the Agent Can Reach\" in the Administrator guide)"
        } else {
            "install bubblewrap"
        }
    }

    /// This machine's sandbox under the administrator's setting: with `required`, an agent that
    /// finds no sandbox does not run here; without it, that agent asks before each action.
    pub fn requiring(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    /// Whether an agent that finds no sandbox must not run.
    pub fn is_required(&self) -> bool {
        self.required
    }

    /// No sandbox, for a runner that has not looked or a test that wants none.
    pub fn none(reason: &str) -> Self {
        Self {
            kind: SandboxKind::None,
            reason: reason.to_owned(),
            bwrap: None,
            home: None,
            looked: false,
            required: false,
            refused: false,
            bridge: None,
        }
    }

    /// No sandbox because the runner looked and found none, or found one
    /// this harness cannot start in: the case where an agent must be able to
    /// ask before it acts.
    pub fn found_none(&self) -> bool {
        self.looked && self.kind == SandboxKind::None
    }

    /// The `bwrap` command line for `program args…`, or `None` when this
    /// machine has no sandbox and the program runs as it is.
    ///
    /// `scratch_home` is bound at the runner's `$HOME` and at its own path
    /// (a harness may keep state there); of each of `binds`, a directory
    /// under `$HOME`, only the entries it names are shared, when they exist.
    /// A program that
    /// lives outside the system directories is bound read-only, with the Homebrew prefix
    /// directories it starts from when it is installed there. Each of
    /// `files` is one file shared at the second path, which lies in the
    /// scratch home.
    pub fn wrap(
        &self,
        program: &Path,
        args: &[String],
        workspace: &Path,
        scratch_home: &Path,
        binds: &[ConfigDir],
        files: SharedFiles<'_>,
    ) -> Option<(PathBuf, Vec<String>)> {
        let bwrap = self.bwrap.as_ref()?;
        let mut argv: Vec<String> = Vec::new();
        let push = |argv: &mut Vec<String>, parts: &[&str]| {
            argv.extend(parts.iter().map(|part| (*part).to_owned()));
        };
        push(
            &mut argv,
            &[
                "--unshare-pid",
                "--unshare-ipc",
                "--unshare-uts",
                "--cap-drop",
                "ALL",
                "--proc",
                "/proc",
                "--dev",
                "/dev",
                "--tmpfs",
                "/tmp",
                "--ro-bind",
                "/usr",
                "/usr",
                "--ro-bind",
                "/etc",
                "/etc",
                "--ro-bind",
                "/bin",
                "/bin",
                "--ro-bind",
                "/lib",
                "/lib",
            ],
        );
        if Path::new("/lib64").exists() {
            push(&mut argv, &["--ro-bind", "/lib64", "/lib64"]);
        }
        // bwrap resolves a relative path against the program's working
        // directory (the workspace), not the runner's; a storage directory
        // configured as `./storage` would name nothing there.
        let bind = |argv: &mut Vec<String>, flag: &str, source: &Path, destination: &Path| {
            argv.extend([
                flag.to_owned(),
                absolute(source).display().to_string(),
                absolute(destination).display().to_string(),
            ]);
        };
        if let Some(home) = &self.home {
            bind(&mut argv, "--bind", scratch_home, home);
            for dir in binds {
                let root = home.join(dir.name);
                for (flag, names) in [("--ro-bind", dir.read_only), ("--bind", dir.login)] {
                    for name in names {
                        let shared = root.join(name);
                        if shared.exists() {
                            bind(&mut argv, flag, &shared, &shared);
                        }
                    }
                }
            }
        }
        // After the home binds and before the workspace: a repository under `$HOME` shows on top
        // of the scratch home, and a workspace inside the repository stays read-write.
        for (flag, source, destination) in worktree_git(workspace, scratch_home) {
            bind(&mut argv, flag, &source, &destination);
        }
        // After the home binds: a workspace or scratch directory under
        // `$HOME` must stay visible on top of the scratch home.
        bind(&mut argv, "--bind", workspace, workspace);
        bind(&mut argv, "--bind", scratch_home, scratch_home);
        for (source, destination) in files.writable {
            bind(&mut argv, "--bind", source, destination);
        }
        for (source, destination) in files.read_only {
            bind(&mut argv, "--ro-bind", source, destination);
        }
        let mut shown = vec![program];
        if let Some(bridge) = &self.bridge {
            push(&mut argv, &["--unshare-net"]);
            bind(
                &mut argv,
                "--bind",
                &bridge.socket_dir,
                Path::new(BRIDGE_SOCKET_DIR),
            );
            shown.push(&bridge.helper);
        }
        for path in shown {
            let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            for path in [path, canonical.as_path()] {
                if !SYSTEM_DIRS.iter().any(|system| path.starts_with(system)) {
                    bind(&mut argv, "--ro-bind", path, path);
                }
            }
            for dir in homebrew_dirs(&canonical) {
                bind(&mut argv, "--ro-bind", &dir, &dir);
            }
        }
        push(&mut argv, &["--die-with-parent", "--new-session", "--"]);
        if let Some(bridge) = &self.bridge {
            argv.extend([
                absolute(&bridge.helper).display().to_string(),
                BRIDGE_ARG.to_owned(),
                "--".to_owned(),
            ]);
        }
        argv.push(absolute(program).display().to_string());
        argv.extend(args.iter().cloned());
        Some((bwrap.clone(), argv))
    }
}

/// The directories every sandbox shows read-only, where a program needs no bind of its own.
const SYSTEM_DIRS: [&str; 4] = ["/usr", "/bin", "/lib", "/etc"];

/// The directories of a Homebrew prefix a program installed there needs to start, none when
/// `program` (already resolved) does not live in a keg. A keg's binary is linked against
/// libraries that Homebrew reaches through the prefix's `opt` and `lib` (its dynamic loader is
/// `<prefix>/lib/ld.so`, a link out of the keg), and its dependencies are other kegs, so the
/// program's own file is not enough. `etc`, `var` and the rest of the prefix hold configuration and
/// data of other packages and stay out of sight.
fn homebrew_dirs(program: &Path) -> Vec<PathBuf> {
    let Some(prefix) = program
        .ancestors()
        .find(|dir| dir.file_name().is_some_and(|name| name == "Cellar"))
        .and_then(Path::parent)
    else {
        return Vec::new();
    };
    ["Cellar", "opt", "lib", "bin"]
        .into_iter()
        .map(|name| prefix.join(name))
        .filter(|dir| dir.is_dir() && !SYSTEM_DIRS.iter().any(|system| dir.starts_with(system)))
        .collect()
}

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Where the shared repository's configuration, cut to what git needs to read it, is written in
/// the scratch home.
const WORKTREE_CONFIG: &str = ".tack-git-config";

/// The binds that make git work in a workspace `git worktree add` made, each `(flag, source,
/// destination)`; none for a workspace that holds its own repository.
///
/// Such a workspace's `.git` is a file naming the worktree's own directory inside the person's
/// repository (its index and HEAD), which points on to the shared one (objects, refs, hooks). The
/// worktree's own is shown read-write and the shared one read-only, so the agent reads the
/// history and the status but cannot rewrite a hook, another branch or the configuration git
/// runs outside the sandbox. The shared configuration is shown as a copy that keeps only its
/// `[core]` and `[extensions]` sections: a remote URL or header holding a token stays out of
/// sight.
fn worktree_git(workspace: &Path, scratch_home: &Path) -> Vec<(&'static str, PathBuf, PathBuf)> {
    let Some(own) = std::fs::read_to_string(workspace.join(".git"))
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|line| line.strip_prefix("gitdir:"))
                .map(|dir| workspace.join(dir.trim()))
        })
        .and_then(|dir| dir.canonicalize().ok())
    else {
        return Vec::new();
    };
    let shared = std::fs::read_to_string(own.join("commondir"))
        .ok()
        .and_then(|dir| own.join(dir.trim()).canonicalize().ok())
        .unwrap_or_else(|| own.clone());
    let mut binds = vec![("--ro-bind", shared.clone(), shared.clone())];
    if own != shared {
        binds.push(("--bind", own.clone(), own));
    }
    let config = shared.join("config");
    let copy = scratch_home.join(WORKTREE_CONFIG);
    match std::fs::read_to_string(&config) {
        Ok(text) if std::fs::write(&copy, core_sections(&text)).is_ok() => {
            binds.push(("--ro-bind", copy, config));
        }
        // A configuration that cannot be cut is not shown at all.
        Ok(_) => binds.push(("--ro-bind", PathBuf::from("/dev/null"), config)),
        Err(_) => {}
    }
    binds
}

/// The `[core]` and `[extensions]` sections of a git configuration file, which say how to read
/// the repository; every other section (remotes, credentials, includes) is dropped.
fn core_sections(config: &str) -> String {
    let mut kept = String::new();
    let mut keep = false;
    for line in config.lines() {
        if let Some(header) = line.trim_start().strip_prefix('[') {
            let name: String = header
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect::<String>()
                .to_ascii_lowercase();
            keep = name == "core" || name == "extensions";
        }
        if keep {
            kept.push_str(line);
            kept.push('\n');
        }
    }
    kept
}

/// Looks for `bwrap` on `path` and remembers `home`; both are the runner's
/// own, read once by the caller. A `bwrap` that is there but cannot make a sandbox (the system
/// refuses it a user namespace) counts as none, and the reason says so.
pub fn detect(path: Option<&OsStr>, home: Option<&Path>) -> Sandbox {
    let found = path.and_then(|path| {
        std::env::split_paths(path)
            .filter(|dir| !dir.as_os_str().is_empty())
            .map(|dir| dir.join("bwrap"))
            .find(|candidate| is_executable(candidate))
    });
    match found {
        Some(bwrap) => match tried(&bwrap) {
            Ok(()) => Sandbox {
                kind: SandboxKind::Bwrap,
                reason: "bwrap on PATH".to_owned(),
                bwrap: Some(bwrap),
                home: home.map(Path::to_path_buf),
                looked: true,
                required: false,
                refused: false,
                bridge: None,
            },
            Err(why) => Sandbox {
                looked: true,
                refused: true,
                home: home.map(Path::to_path_buf),
                ..Sandbox::none(&format!(
                    "bwrap is installed but cannot make a sandbox here: {why}"
                ))
            },
        },
        None => Sandbox {
            looked: true,
            ..Sandbox::none("bwrap not on PATH")
        },
    }
}

/// Whether `bwrap` makes a sandbox on this computer: it starts a shell that does nothing inside
/// one showing the whole file system read-only, which needs nothing but the user namespace the
/// system may refuse. The answer only changes with the program, so it is kept per process.
fn tried(bwrap: &Path) -> Result<(), String> {
    static TRIED: std::sync::OnceLock<std::sync::Mutex<BTreeMap<PathBuf, Result<(), String>>>> =
        std::sync::OnceLock::new();
    let tried = TRIED.get_or_init(Default::default);
    if let Some(hit) = tried.lock().ok().and_then(|map| map.get(bwrap).cloned()) {
        return hit;
    }
    let args = ["--ro-bind", "/", "/", "--", "/bin/sh", "-c", "exit 0"].map(str::to_owned);
    let answer = run_probe(
        bwrap,
        &args,
        &BTreeMap::new(),
        Path::new("/"),
        Duration::from_secs(10),
    );
    if let Ok(mut map) = tried.lock() {
        map.insert(bwrap.to_path_buf(), answer.clone());
    }
    answer
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

/// Runs `program args…` to completion, blocking, with a clean environment
/// and no stdin. `Ok` when it exits 0; otherwise the first line of its
/// stderr (or how it ended) as the error.
pub fn run_probe(
    program: &Path,
    args: &[String],
    env: &BTreeMap<String, String>,
    working_directory: &Path,
    timeout: Duration,
) -> Result<(), String> {
    let mut child = Command::new(program)
        .args(args)
        .env_clear()
        .envs(env)
        .current_dir(working_directory)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    let mut stderr = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(stderr) = stderr.as_mut() {
            let _ = stderr.take(8192).read_to_string(&mut text);
        }
        text
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => return Err(error.to_string()),
        }
    };
    let text = reader.join().unwrap_or_default();
    match status {
        Some(status) if status.success() => Ok(()),
        Some(status) => Err(text
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map_or_else(
                || format!("exited with {status}"),
                |line| line.chars().take(120).collect(),
            )),
        None => Err("timed out".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::core_sections;

    #[test]
    fn the_shared_configuration_keeps_only_how_to_read_the_repository() {
        let config = "\
[core]
\trepositoryformatversion = 1
\tbare = false
[remote \"origin\"]
\turl = https://x-access-token:ghp_secret@github.com/acme/app.git
[http \"https://github.com/\"]
\textraHeader = Authorization: Bearer secret
[Extensions]
\tobjectFormat = sha256
[include]
\tpath = ~/.gitconfig-work
";
        let kept = core_sections(config);
        assert!(kept.contains("repositoryformatversion = 1"), "{kept}");
        assert!(kept.contains("objectFormat = sha256"), "{kept}");
        for gone in ["secret", "remote", "include", "http"] {
            assert!(!kept.contains(gone), "{gone} is shown: {kept}");
        }
    }
}
