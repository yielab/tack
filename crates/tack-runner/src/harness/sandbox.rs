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
//! is left alone: the harness has to reach its model.

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
}

impl Sandbox {
    /// This machine's sandbox with the kind a harness that cannot start in
    /// it gets: no wrapping, and the reason why.
    pub fn without(&self, reason: String) -> Self {
        Self {
            kind: SandboxKind::None,
            reason,
            bwrap: None,
            home: self.home.clone(),
            looked: true,
        }
    }

    /// No sandbox, for a runner that has not looked or a test that wants none.
    pub fn none(reason: &str) -> Self {
        Self {
            kind: SandboxKind::None,
            reason: reason.to_owned(),
            bwrap: None,
            home: None,
            looked: false,
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
    /// lives outside the system directories is bound read-only. Each of
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
        let canonical = program
            .canonicalize()
            .unwrap_or_else(|_| program.to_path_buf());
        for path in [program, canonical.as_path()] {
            if !["/usr", "/bin", "/lib", "/etc"]
                .iter()
                .any(|system| path.starts_with(system))
            {
                bind(&mut argv, "--ro-bind", path, path);
            }
        }
        push(&mut argv, &["--die-with-parent", "--new-session", "--"]);
        argv.push(absolute(program).display().to_string());
        argv.extend(args.iter().cloned());
        Some((bwrap.clone(), argv))
    }
}

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Looks for `bwrap` on `path` and remembers `home`; both are the runner's
/// own, read once by the caller.
pub fn detect(path: Option<&OsStr>, home: Option<&Path>) -> Sandbox {
    let found = path.and_then(|path| {
        std::env::split_paths(path)
            .filter(|dir| !dir.as_os_str().is_empty())
            .map(|dir| dir.join("bwrap"))
            .find(|candidate| is_executable(candidate))
    });
    match found {
        Some(bwrap) => Sandbox {
            kind: SandboxKind::Bwrap,
            reason: "bwrap on PATH".to_owned(),
            bwrap: Some(bwrap),
            home: home.map(Path::to_path_buf),
            looked: true,
        },
        None => Sandbox {
            looked: true,
            ..Sandbox::none("bwrap not on PATH")
        },
    }
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
