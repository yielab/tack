//! The docket daemon the runner keeps beside it: `docket start --dispatch`
//! listening on loopback. The runner looks at `GET /health` first and spawns
//! nothing when something already answers, or when `base_url` names a daemon
//! someone else runs. A daemon this runner started is restarted with a
//! backoff when it exits, and gets one `SIGTERM` when the runner shuts down:
//! docket finishes the task it is running and claims no other.

use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use tokio::{sync::Notify, task::JoinHandle};

use crate::config::DocketConfig;

/// The log the daemon's stdout and stderr go to, in the runner's log directory.
pub const LOG_FILE: &str = "docket-daemon.log";

/// The bounds the supervisor works within.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// How long a started daemon may take to answer `/health`.
    pub ready_timeout: Duration,
    /// The first wait before a restart; it doubles up to a minute.
    pub backoff: Duration,
    /// How long `shutdown` waits for the daemon to exit after its `SIGTERM`.
    pub grace: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            ready_timeout: Duration::from_secs(30),
            backoff: Duration::from_secs(2),
            grace: Duration::from_secs(5),
        }
    }
}

const MAX_BACKOFF: Duration = Duration::from_secs(60);

/// What the runner holds for the daemon. Dropping it leaves a running daemon
/// as it is; `shutdown` stops the supervisor and signals the daemon once.
pub struct DaemonHandle {
    shared: Arc<Shared>,
    task: Option<JoinHandle<()>>,
    grace: Duration,
}

#[derive(Default)]
struct Shared {
    stopping: AtomicBool,
    pid: Mutex<Option<u32>>,
    wake: Notify,
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .no_proxy()
        .build()
        .unwrap_or_default()
}

/// Whether `GET <url>/health` answers `200`.
async fn healthy(client: &reqwest::Client, url: &str) -> bool {
    client
        .get(format!("{url}/health"))
        .send()
        .await
        .is_ok_and(|response| response.status().is_success())
}

/// The sentence `tack runner doctor` prints after `daemon:`: where the daemon
/// answers, or why it does not.
pub async fn describe(config: &DocketConfig) -> Result<String, String> {
    let url = config.url();
    if healthy(&http(), &url).await {
        let place = url.trim_start_matches("http://");
        return Ok(format!("running on {place}"));
    }
    Err(if config.base_url.is_some() {
        format!("not answering at {url}/health; start it, or remove base_url so the runner can")
    } else if config.binary.is_none() {
        format!("not answering at {url}/health and no binary is set to start one")
    } else {
        format!("not answering at {url}/health; the runner starts it with the runner")
    })
}

/// `docket start --dispatch ...` with an environment of `PATH`, `HOME` and,
/// when `[docket].home` is set, `DOCKET_HOME`: nothing else the runner holds
/// reaches the daemon.
pub fn command(config: &DocketConfig) -> std::process::Command {
    let program = config
        .binary
        .clone()
        .unwrap_or_else(|| PathBuf::from("docket"));
    let mut command = std::process::Command::new(program);
    command
        .args(["start", "--dispatch", "--bind", "127.0.0.1", "--port"])
        .arg(config.port.to_string());
    if let Some(file) = &config.token_file {
        command.arg("--token-file").arg(file);
    }
    command.env_clear();
    for name in ["PATH", "HOME"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    if let Some(home) = &config.home {
        command.env("DOCKET_HOME", home);
    }
    command
}

fn spawn(config: &DocketConfig, log_dir: &Path) -> std::io::Result<tokio::process::Child> {
    std::fs::create_dir_all(log_dir)?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join(LOG_FILE))?;
    let mut command = command(config);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log));
    let mut command = tokio::process::Command::from(command);
    // Its own group: a Ctrl-C meant for the runner is not a stop for docket.
    #[cfg(unix)]
    command.process_group(0);
    command.spawn()
}

async fn until_healthy(client: &reqwest::Client, url: &str, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    loop {
        if healthy(client, url).await {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn term(pid: u32) {
    // Without a libc binding: the one signal the daemon is asked to handle.
    let _ = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status();
}

async fn supervise(
    config: DocketConfig,
    log_dir: PathBuf,
    options: Options,
    shared: Arc<Shared>,
    first: tokio::sync::oneshot::Sender<()>,
) {
    let client = http();
    let url = config.url();
    let mut first = Some(first);
    let mut delay = options.backoff;
    while !shared.stopping.load(Ordering::SeqCst) {
        let mut child = match spawn(&config, &log_dir) {
            Ok(child) => child,
            Err(error) => {
                tracing::warn!(%error, "the docket daemon could not be started");
                if let Some(first) = first.take() {
                    let _ = first.send(());
                }
                wait_or_stop(&shared, delay).await;
                delay = (delay * 2).min(MAX_BACKOFF);
                continue;
            }
        };
        let pid = child.id();
        *shared.pid.lock().unwrap_or_else(|e| e.into_inner()) = pid;
        if shared.stopping.load(Ordering::SeqCst)
            && let Some(pid) = pid
        {
            term(pid);
        }
        let started = Instant::now();
        let mut exited = None;
        let up = {
            let wait = child.wait();
            tokio::pin!(wait);
            tokio::select! {
                up = until_healthy(&client, &url, options.ready_timeout) => up,
                status = &mut wait => { exited = Some(status); false }
            }
        };
        if let Some(first) = first.take() {
            let _ = first.send(());
        }
        if !up && exited.is_none() {
            tracing::warn!(
                url,
                "the docket daemon did not answer /health in time; waiting for it"
            );
        }
        let status = match exited {
            Some(status) => status,
            None => child.wait().await,
        };
        *shared.pid.lock().unwrap_or_else(|e| e.into_inner()) = None;
        if shared.stopping.load(Ordering::SeqCst) {
            break;
        }
        if up && started.elapsed() > options.ready_timeout {
            delay = options.backoff;
        }
        tracing::warn!(
            status = ?status.ok(),
            restart_in_ms = u64::try_from(delay.as_millis()).unwrap_or(u64::MAX),
            "the docket daemon exited; restarting it"
        );
        wait_or_stop(&shared, delay).await;
        delay = (delay * 2).min(MAX_BACKOFF);
    }
}

async fn wait_or_stop(shared: &Shared, delay: Duration) {
    let _ = tokio::time::timeout(delay, shared.wake.notified()).await;
}

/// Makes sure a daemon answers: nothing is spawned when `base_url` is set or
/// when `/health` already answers. Returns once the first start has either
/// answered or used up its time.
pub async fn ensure_running(config: &DocketConfig, log_dir: &Path) -> DaemonHandle {
    ensure_running_with(config, log_dir, Options::default()).await
}

pub async fn ensure_running_with(
    config: &DocketConfig,
    log_dir: &Path,
    options: Options,
) -> DaemonHandle {
    let shared = Arc::new(Shared::default());
    let idle = DaemonHandle {
        shared: Arc::clone(&shared),
        task: None,
        grace: options.grace,
    };
    if config.base_url.is_some() || healthy(&http(), &config.url()).await {
        return idle;
    }
    let (first, started) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(supervise(
        config.clone(),
        log_dir.to_path_buf(),
        options,
        Arc::clone(&shared),
        first,
    ));
    let _ = started.await;
    DaemonHandle {
        task: Some(task),
        ..idle
    }
}

impl DaemonHandle {
    /// Whether this runner started the daemon it is holding.
    pub fn spawned(&self) -> bool {
        self.task.is_some()
    }

    /// One `SIGTERM` to the daemon this runner started; nothing for one it
    /// found running. Waits a short grace for it to exit and no longer:
    /// docket finishes its running task on its own time.
    pub async fn shutdown(mut self) {
        self.shared.stopping.store(true, Ordering::SeqCst);
        let pid = *self.shared.pid.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(pid) = pid {
            term(pid);
        }
        self.shared.wake.notify_one();
        if let Some(task) = self.task.take() {
            let _ = tokio::time::timeout(self.grace, task).await;
        }
    }
}

#[cfg(test)]
mod tests;
