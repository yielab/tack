//! Installing a project's dependencies in the copy of the code an agent works in.
//!
//! The command comes from the repository's lockfile unless the project replaced or turned off the
//! install (`tack_protocol::prepare`); a replacement runs only when the runner lets the board
//! replace it. A package's own install script runs as the person who started the runner, so the
//! command runs where an agent does: in the sandbox, with a scratch home, an environment of a
//! few allowed names and, when the runner holds traffic to a proxy, only the package registry
//! reachable through it. Where the machine has no sandbox and the administrator turned the
//! requirement off, it runs in the checkout with the runner's own environment, as the agent
//! does. What npm, pnpm and yarn install into
//! `node_modules` is kept once, under the runner's state directory, and each checkout gets its own
//! tree of directories whose files are hard links to the kept ones, so the next attempt with the
//! same lockfile does not install again and the agent's sandbox, which sees only the checkout, can
//! read it.

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

use tack_protocol::execution::{EgressMode, ExecutionRequestSnapshot};
use tack_protocol::prepare::{PrepareSetting, Resolution, Why};

use crate::config::{ProcessEnv, RunnerConfig};
use crate::harness::{
    egress::{EgressPolicy, EgressProxy},
    process::{ProcessExit, ProcessLimits, ProcessSpec},
    redact::SecretMaterial,
    sandbox::{self, Sandbox, SharedFiles},
    sha256::sha256_hex,
};

/// The folder a cached install fills.
const FOLDER: &str = "node_modules";

/// A file the runner puts in that folder to say it prepared it. The evidence of an attempt leaves
/// the folder out when it holds this file, because the install is not the agent's work.
const MARKER: &str = ".tack-prepared";

/// Output kept from a command, per stream.
const OUTPUT_BYTES: usize = 1024 * 1024;

/// How many of the last lines of a failed command's output the reason carries.
const REASON_LINES: usize = 40;

/// Variables an install in the sandbox gets from the runner's environment besides `PATH` and
/// `HOME`: the locale, and the certificates a registry behind a private authority needs.
const PASSED: [&str; 6] = [
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TZ",
    "SSL_CERT_FILE",
    "NODE_EXTRA_CA_CERTS",
];

/// The package registry's configuration, which an install in the sandbox keeps from the runner's
/// environment: where to fetch from, and the token that lets it.
const REGISTRY: [&str; 10] = [
    "NPM_CONFIG_REGISTRY",
    "npm_config_registry",
    "NPM_TOKEN",
    "PIP_INDEX_URL",
    "PIP_EXTRA_INDEX_URL",
    "PIP_TRUSTED_HOST",
    "UV_INDEX_URL",
    "UV_DEFAULT_INDEX",
    "UV_EXTRA_INDEX_URL",
    "UV_INDEX",
];

/// The system directories the sandbox already shows, so a program found there needs no bind.
const SYSTEM: [&str; 6] = ["/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc"];

/// What the machine and its administrator decide about an install: the sandbox it runs in, whether
/// the runner holds traffic to a proxy, and whether the board may replace the command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rules {
    pub sandbox: Sandbox,
    /// Whether the install's traffic goes through a proxy that reaches the registry only
    /// (`[egress] enforce`).
    pub enforce_egress: bool,
    /// `[prepare] board_may_replace`.
    pub board_may_replace: bool,
}

impl Default for Rules {
    /// No sandbox asked for, no proxy, and no replacement from the board.
    fn default() -> Self {
        Self {
            sandbox: Sandbox::none("not detected"),
            enforce_egress: false,
            board_may_replace: false,
        }
    }
}

impl Rules {
    /// The rules this runner's configuration and machine give.
    pub fn from_config(config: &RunnerConfig) -> Self {
        Self {
            sandbox: sandbox::detect(
                config.process.search_path().as_deref(),
                config.process.home().as_deref(),
            )
            .requiring(config.sandbox.required),
            enforce_egress: config.egress.enforce,
            board_may_replace: config.prepare.board_may_replace,
        }
    }

    /// Whether an install here runs outside any sandbox because the runner found none and the
    /// administrator turned the requirement off.
    pub fn unsandboxed(&self) -> bool {
        self.sandbox.found_none() && !self.sandbox.is_required()
    }
}

/// Why the dependencies could not be prepared; the text is the attempt's reason.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct PrepareError(String);

/// The tools that read and never run a command, compared case-insensitively.
const READ_ONLY_TOOLS: [&str; 5] = ["read", "grep", "glob", "webfetch", "websearch"];

/// The acceptance criteria that run something, and so need what the project installs.
const RUNNING_CRITERIA: [&str; 3] = ["command", "test", "metric"];

/// Why a run installs nothing, as the run's timeline says it.
pub const NO_INSTALL_REASON: &str = "No install: this profile runs no commands";

/// Why an install ran outside a sandbox, as the run's timeline says it.
pub const UNSANDBOXED_REASON: &str = "The install ran without a sandbox: this computer has none and the administrator turned the requirement off";

/// Whether `request` needs nothing installed: its tools are named and none of them runs a
/// command, and the item has no check that does. The tools are the run's own, else the profile's
/// for the run's agent; `*` or no list is not a list, and docket's pipeline decides for itself.
pub fn needs_no_install(request: &ExecutionRequestSnapshot) -> bool {
    let harness = request.requested_harness_kind.as_str();
    if harness == "docket" {
        return false;
    }
    let mut tools = request.permission_policy.tools.clone();
    if tools.is_empty() {
        tools = request.resolved_agent_profile.tool_policy["tools"][harness]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|tool| tool.as_str().map(str::to_owned))
            .collect();
    }
    let reads_only = !tools.is_empty()
        && tools.iter().all(|tool| {
            READ_ONLY_TOOLS
                .iter()
                .any(|read| tool.eq_ignore_ascii_case(read))
        });
    let has_check = request
        .brief
        .as_ref()
        .and_then(|brief| brief["acceptance"].as_array())
        .into_iter()
        .flatten()
        .any(|criterion| {
            criterion["kind"]
                .as_str()
                .is_some_and(|kind| RUNNING_CRITERIA.contains(&kind))
        });
    reads_only && !has_check
}

/// Prepares `checkout` as `setting` says, within `timeout`, and says whether a command ran (a kept
/// install is linked, not run). Nothing runs when the install is off, the repository has no
/// lockfile, or the setting is a replacement the runner does not let the board make.
pub async fn prepare(
    checkout: &Path,
    setting: &PrepareSetting,
    state_dir: &Path,
    timeout: Duration,
    env: &ProcessEnv,
    rules: &Rules,
) -> Result<bool, PrepareError> {
    if let Some(reason) = setting.refusal(rules.board_may_replace) {
        return Err(PrepareError(reason.to_owned()));
    }
    let files = root_files(checkout)
        .map_err(|error| PrepareError(format!("Could not read the checkout: {error}")))?;
    match setting.resolve(&files) {
        Resolution::Nothing(why) => {
            let why = match why {
                Why::Off => "the install is turned off",
                Why::NoLockfile => "the repository has no lockfile",
            };
            tracing::info!("nothing is installed: {why}");
            Ok(false)
        }
        Resolution::Run { command, lockfile } => {
            install(checkout, &command, lockfile, state_dir, timeout, env, rules).await
        }
    }
}

/// The names of the files at the checkout's root.
fn root_files(checkout: &Path) -> io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(checkout)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    Ok(names)
}

/// The folders under `checkout` that the runner prepared.
pub fn prepared_folders(checkout: &Path) -> Vec<&'static str> {
    if checkout.join(FOLDER).join(MARKER).is_file() {
        vec![FOLDER]
    } else {
        Vec::new()
    }
}

/// Whether the install a lockfile implies is kept: poetry, uv and cargo keep their packages in a
/// cache of their own.
fn is_kept(lockfile: &str) -> bool {
    matches!(
        lockfile,
        "package-lock.json" | "pnpm-lock.yaml" | "yarn.lock"
    )
}

/// Runs `command`, or builds the install a previous attempt kept for the same lockfile. Says
/// whether it ran.
async fn install(
    checkout: &Path,
    command: &str,
    lockfile: Option<&str>,
    state_dir: &Path,
    timeout: Duration,
    env: &ProcessEnv,
    rules: &Rules,
) -> Result<bool, PrepareError> {
    let run = || run(checkout, command, lockfile, state_dir, timeout, env, rules);
    let installed = checkout.join(FOLDER);
    // A replaced command is the project's own and may do anything, so it always runs. A folder the
    // repository already holds is the project's own too.
    let kept = lockfile
        .filter(|lockfile| is_kept(lockfile))
        .filter(|_| fs::symlink_metadata(&installed).is_err());
    let Some(lockfile) = kept else {
        return run().await.map(|()| true);
    };
    let Ok(lock) = fs::read(checkout.join(lockfile)) else {
        return run().await.map(|()| true);
    };
    let cache = state_dir.join("prepare-cache");
    let entry = cache.join(sha256_hex(&lock));
    if link_tree(&entry.join(FOLDER), &installed).is_ok() {
        tracing::info!("reused the dependencies kept for {lockfile}");
        return mark(&installed).map(|()| false);
    }
    // Nothing usable was kept, or it could not be linked: whatever was half built goes.
    let _ = fs::remove_dir_all(&installed);
    run().await?;
    keep(checkout, &cache, &entry);
    mark(&installed).map(|()| true)
}

/// Keeps the install in the checkout as `entry`, built from hard links beside the cache's other
/// entries and renamed into place, so an entry appears whole or not at all. When another attempt
/// renamed first, theirs stays and this copy is dropped. The checkout keeps its own install either
/// way, so a failure here only means the next attempt installs again.
fn keep(checkout: &Path, cache: &Path, entry: &Path) {
    let staging = cache.join(format!(
        ".staging-{}",
        checkout
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
    ));
    let _ = fs::remove_dir_all(&staging);
    let kept = fs::create_dir_all(&staging)
        .and_then(|()| link_tree(&checkout.join(FOLDER), &staging.join(FOLDER)))
        .and_then(|()| fs::rename(&staging, entry));
    if let Err(error) = kept {
        tracing::info!("the installed dependencies were not kept: {error}");
        let _ = fs::remove_dir_all(&staging);
    }
}

/// Says the runner prepared `installed`, with a file of its own (not a link to a kept one).
fn mark(installed: &Path) -> Result<(), PrepareError> {
    fs::write(installed.join(MARKER), "")
        .map_err(|error| PrepareError(format!("Could not prepare the dependencies: {error}")))
}

/// Builds `to` as a copy of the directory tree `from`: real directories, symlinks recreated, and
/// every file a hard link to the original.
fn link_tree(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            link_tree(&entry.path(), &target)?;
        } else if kind.is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(fs::read_link(entry.path())?, &target)?;
            #[cfg(not(unix))]
            return Err(io::ErrorKind::Unsupported.into());
        } else {
            fs::hard_link(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Where a command runs: its program and arguments, its environment, and what to clean up after.
struct Launch {
    program: PathBuf,
    args: Vec<String>,
    env: BTreeMap<String, String>,
    /// Values the output must not show.
    secrets: SecretMaterial,
    /// The scratch home of a sandboxed install, thrown away after it.
    scratch: Option<PathBuf>,
    /// The proxy the install's traffic goes through, while it runs.
    proxy: Option<EgressProxy>,
}

/// Runs `command` with `sh -c` in `checkout`, bounded by `timeout`: in the sandbox where the
/// machine has one, else as the runner.
async fn run(
    checkout: &Path,
    command: &str,
    lockfile: Option<&str>,
    state_dir: &Path,
    timeout: Duration,
    env: &ProcessEnv,
    rules: &Rules,
) -> Result<(), PrepareError> {
    let mut launch = launch(checkout, command, lockfile, state_dir, env, rules).await?;
    let mut refusals = launch.proxy.as_mut().and_then(EgressProxy::take_refusals);
    let result = follow(checkout, command, timeout, &launch).await;
    if let Some(scratch) = &launch.scratch {
        let _ = fs::remove_dir_all(scratch);
    }
    let Err(PrepareError(reason)) = result else {
        return Ok(());
    };
    // A refused destination is the likeliest reason an install in the proxy fails.
    let mut blocked = Vec::new();
    while let Some(refusal) = refusals.as_mut().and_then(|r| r.try_recv().ok()) {
        blocked.push(format!("{}:{}", refusal.host, refusal.port));
    }
    let reason = if blocked.is_empty() {
        reason
    } else {
        format!(
            "{reason}\nThe runner refused: {} (the install may reach only its package registry)",
            blocked.join(", ")
        )
    };
    Err(PrepareError(launch.secrets.scrub(&reason)))
}

async fn launch(
    checkout: &Path,
    command: &str,
    lockfile: Option<&str>,
    state_dir: &Path,
    env: &ProcessEnv,
    rules: &Rules,
) -> Result<Launch, PrepareError> {
    let error = |what: String| PrepareError(format!("Could not prepare the dependencies: {what}"));
    let sandbox = &rules.sandbox;
    let mut secrets = SecretMaterial::new();
    secrets.register_all(registry_secrets(env));
    if sandbox.is_required() && sandbox.found_none() {
        return Err(error(format!(
            "this computer has no sandbox to install in ({}), so nothing is installed here",
            sandbox.reason
        )));
    }
    let sh = |env: BTreeMap<String, String>| Launch {
        program: PathBuf::from("sh"),
        args: vec!["-c".to_owned(), command.to_owned()],
        env,
        secrets: secrets.clone(),
        scratch: None,
        proxy: None,
    };
    let scratch = state_dir.join("prepare-scratch").join(
        checkout
            .file_name()
            .map_or_else(|| "checkout".into(), |name| name.to_os_string()),
    );
    let home = env.home();
    let tools = tool_dirs(
        &env.path_or_standard(),
        home.as_deref(),
        state_dir,
        checkout,
    );
    // Made before the wrap, which may write the repository's cut configuration into it.
    let _ = fs::remove_dir_all(&scratch);
    fs::create_dir_all(&scratch).map_err(|e| error(format!("no scratch home ({e})")))?;
    // `None` where the machine has no sandbox: the install runs as the runner.
    let wrapped = sandbox.wrap(
        Path::new("/bin/sh"),
        &["-c".to_owned(), command.to_owned()],
        checkout,
        &scratch,
        &[],
        SharedFiles {
            writable: &[],
            read_only: &tools,
        },
    );
    let Some((program, args)) = wrapped else {
        let _ = fs::remove_dir_all(&scratch);
        return Ok(sh(env.text_pairs()));
    };
    let mut vars = BTreeMap::from([
        ("PATH".to_owned(), env.path_or_standard()),
        (
            "HOME".to_owned(),
            home.as_deref().unwrap_or(&scratch).display().to_string(),
        ),
    ]);
    for name in PASSED.into_iter().chain(REGISTRY) {
        if let Some(value) = env.var(name) {
            vars.insert(name.to_owned(), value);
        }
    }
    let mut proxy = None;
    if rules.enforce_egress {
        let hosts = registry_hosts(lockfile, env);
        let policy = EgressPolicy::new(
            EgressMode::Listed,
            hosts.iter().map(String::as_str),
            std::iter::empty(),
        );
        let started = EgressProxy::start(policy).await.map_err(|e| {
            let _ = fs::remove_dir_all(&scratch);
            tracing::warn!(%e, "the egress proxy for the install could not start");
            error("the runner's web access proxy could not start".to_owned())
        })?;
        vars.extend(started.environment());
        proxy = Some(started);
    }
    Ok(Launch {
        program,
        args,
        env: vars,
        secrets,
        scratch: Some(scratch),
        proxy,
    })
}

async fn follow(
    checkout: &Path,
    command: &str,
    timeout: Duration,
    launch: &Launch,
) -> Result<(), PrepareError> {
    let failed = |what: String, output: &str| {
        let tail = last_lines(output);
        let reason = format!("Could not prepare the dependencies: `{command}` {what}");
        PrepareError(if tail.is_empty() {
            reason
        } else {
            format!("{reason}\n{tail}")
        })
    };
    let spec = ProcessSpec {
        program: launch.program.clone(),
        args: launch.args.clone(),
        env: launch.env.clone(),
        stdin: None,
        working_directory: checkout.to_path_buf(),
        workspace_root: checkout.to_path_buf(),
        keep_stdin_open: false,
    };
    let limits = ProcessLimits::new(OUTPUT_BYTES, OUTPUT_BYTES, timeout);
    let process = spec
        .spawn()
        .await
        .map_err(|error| failed(format!("could not be started ({error})"), ""))?;
    let result = process
        .wait_with_capture(&limits, &launch.secrets)
        .await
        .map_err(|error| failed(format!("could not be followed ({error})"), ""))?;
    let output = format!("{}\n{}", result.stdout.text, result.stderr.text);
    match result.exit {
        ProcessExit::Exited(0) => Ok(()),
        ProcessExit::Exited(code) => Err(failed(format!("exited {code}"), &output)),
        #[cfg(unix)]
        ProcessExit::Signaled(signal) => {
            Err(failed(format!("was stopped by signal {signal}"), &output))
        }
        ProcessExit::TimedOut => Err(failed(
            format!("timed out after {} s", timeout.as_secs()),
            &output,
        )),
    }
}

/// The `PATH` directories an install in the sandbox needs to find its tools, each bound read-only
/// where it is. A version manager keeps a tool's libraries beside its `bin`, so the prefix is bound
/// with it, unless the prefix is the person's `home` or a dot-directory directly in it
/// (`~/.cargo`, `~/.local`), which holds more than tools. Nothing that holds the home, the state
/// directory or the checkout, or lies in the state directory, is bound.
fn tool_dirs(
    path: &str,
    home: Option<&Path>,
    state_dir: &Path,
    checkout: &Path,
) -> Vec<(PathBuf, PathBuf)> {
    let protected = [Some(state_dir), Some(checkout), home];
    let mut bound: Vec<PathBuf> = Vec::new();
    for dir in std::env::split_paths(path) {
        if !dir.is_absolute() || !dir.is_dir() || SYSTEM.iter().any(|root| dir.starts_with(root)) {
            continue;
        }
        let prefix = dir
            .parent()
            .filter(|_| dir.file_name().is_some_and(|name| name == "bin"))
            .filter(|prefix| prefix.parent().is_some_and(|up| up != Path::new("/")))
            .filter(|prefix| {
                home.is_none_or(|home| *prefix != home && prefix.parent() != Some(home))
            });
        let candidate = prefix.unwrap_or(&dir).to_path_buf();
        let hides = protected
            .iter()
            .flatten()
            .any(|path| path.starts_with(&candidate))
            || candidate.starts_with(state_dir);
        if !hides && !bound.contains(&candidate) {
            bound.push(candidate);
        }
    }
    bound.into_iter().map(|dir| (dir.clone(), dir)).collect()
}

/// The hosts an install may reach through the proxy: the registries of the package manager the
/// lockfile names (every one, for a replaced command), and any host the runner's own registry
/// settings name.
fn registry_hosts(lockfile: Option<&str>, env: &ProcessEnv) -> Vec<String> {
    const NPM: &[&str] = &["registry.npmjs.org"];
    const YARN: &[&str] = &["registry.yarnpkg.com", "registry.npmjs.org"];
    const PYTHON: &[&str] = &["pypi.org", "files.pythonhosted.org"];
    const CARGO: &[&str] = &["index.crates.io", "static.crates.io", "crates.io"];
    let defaults: Vec<&[&str]> = match lockfile {
        Some("package-lock.json" | "pnpm-lock.yaml") => vec![NPM],
        Some("yarn.lock") => vec![YARN],
        Some("uv.lock" | "poetry.lock") => vec![PYTHON],
        Some("Cargo.lock") => vec![CARGO],
        _ => vec![YARN, PYTHON, CARGO],
    };
    let named = REGISTRY
        .iter()
        .filter(|name| !name.to_ascii_uppercase().contains("TOKEN"))
        .filter_map(|name| host_of(&env.var(name)?));
    let mut hosts: Vec<String> = defaults
        .into_iter()
        .flatten()
        .map(|host| (*host).to_owned())
        .chain(named)
        .collect();
    hosts.sort();
    hosts.dedup();
    hosts
}

/// The host of a URL, lower-cased, without credentials or port.
fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?;
    let host = host.split(':').next()?.to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

/// What in the registry settings the output must not show: a token, and the credentials in a URL.
fn registry_secrets(env: &ProcessEnv) -> Vec<String> {
    REGISTRY
        .iter()
        .filter_map(|name| Some((name, env.var(name)?)))
        .flat_map(|(name, value)| {
            let credentials = value
                .split_once("://")
                .and_then(|(_, rest)| rest.split(['/', '?', '#']).next())
                .and_then(|authority| authority.rsplit_once('@'))
                .map(|(userinfo, _)| userinfo.to_owned());
            let token = name.to_ascii_uppercase().contains("TOKEN").then_some(value);
            credentials.into_iter().chain(token)
        })
        .collect()
}

/// The last lines of `output`, without the blank ones at either end.
fn last_lines(output: &str) -> String {
    let lines: Vec<&str> = output.trim().lines().collect();
    lines[lines.len().saturating_sub(REASON_LINES)..].join("\n")
}

#[cfg(test)]
mod tests;
