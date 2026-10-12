//! Running a harness inside `bwrap` where the machine has it.
//!
//! [`detect`] is read once per runner start. [`Sandbox::wrap`] turns the
//! program and arguments a spawn would run into a `bwrap` command line that
//! shows the harness only what a run needs: the workspace and a scratch home
//! read-write, the system directories read-only, a private `/tmp`, `/proc`
//! and `/dev`. The runner's state directory, `~/.tackrc`, `~/.ssh` and the
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sandbox {
    pub kind: SandboxKind,
    /// Why there is no sandbox, or where `bwrap` was found.
    pub reason: String,
    bwrap: Option<PathBuf>,
    /// The runner's `$HOME`, where the scratch home and the shared
    /// directories are bound.
    home: Option<PathBuf>,
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
        }
    }

    /// No sandbox, for a runner that has not looked or a test that wants none.
    pub fn none(reason: &str) -> Self {
        Self {
            kind: SandboxKind::None,
            reason: reason.to_owned(),
            bwrap: None,
            home: None,
        }
    }

    /// The `bwrap` command line for `program args…`, or `None` when this
    /// machine has no sandbox and the program runs as it is.
    ///
    /// `scratch_home` is bound at the runner's `$HOME` and at its own path
    /// (a harness may keep state there); each of `binds` names a directory
    /// under `$HOME` that is shared as it is, when it exists. A program that
    /// lives outside the system directories is bound read-only.
    pub fn wrap(
        &self,
        program: &Path,
        args: &[String],
        workspace: &Path,
        scratch_home: &Path,
        binds: &[&str],
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
        let bind = |argv: &mut Vec<String>, flag: &str, source: &Path, destination: &Path| {
            argv.extend([
                flag.to_owned(),
                source.display().to_string(),
                destination.display().to_string(),
            ]);
        };
        if let Some(home) = &self.home {
            bind(&mut argv, "--bind", scratch_home, home);
            for name in binds {
                let shared = home.join(name);
                if shared.exists() {
                    bind(&mut argv, "--bind", &shared, &shared);
                }
            }
        }
        // After the home binds: a workspace or scratch directory under
        // `$HOME` must stay visible on top of the scratch home.
        bind(&mut argv, "--bind", workspace, workspace);
        bind(&mut argv, "--bind", scratch_home, scratch_home);
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
        argv.push(program.display().to_string());
        argv.extend(args.iter().cloned());
        Some((bwrap.clone(), argv))
    }
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
        },
        None => Sandbox::none("bwrap not on PATH"),
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
