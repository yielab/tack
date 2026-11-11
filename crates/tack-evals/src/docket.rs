//! docket in a world: everything the docket agent needs that the other agents carry in their own
//! binary. An isolated `DOCKET_HOME` under the world, the Anthropic provider from the key the
//! operator handed over, the model pinned on every role, docket's daemon on a port the kernel
//! chose and one team for the world's repository. The runner reaches the daemon through the
//! `[docket]` table of its settings.
//!
//! Nothing here touches the operator's docket: the daemon, its home, its secret store and its
//! port are the world's, and go with it.

use std::fs::File;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use tack_test_support::poll_until_sync;

use crate::api::Api;

/// The team of a world. Named for the repository's folder (`repo`), which is the team Tack
/// sends a run to when its profile names none: a built-in Planner or Reviewer reaches it too.
pub const TEAM: &str = "repo";
/// The one provider a world sets up, and the variable that holds its key.
const PROVIDER: &str = "anthropic";
const KEY_VARIABLE: &str = "ANTHROPIC_API_KEY";
/// docket's cap on one model call's output, in tokens.
const MAX_OUTPUT_TOKENS: &str = "8192";

/// What a world needs to run docket.
#[derive(Debug, Clone)]
pub struct DocketSetup {
    /// docket's name of the model every role runs on (`anthropic/claude-haiku-5-5`).
    pub model: String,
    /// Directories where `docket` is found, in front of the system's.
    pub agent_path: Vec<PathBuf>,
}

/// A running docket daemon with its team, stopped when dropped.
pub struct Docket {
    child: Child,
    home: PathBuf,
    pub base_url: String,
    pub token_file: PathBuf,
    pub log: PathBuf,
}

impl Docket {
    /// Sets docket up under `root` and starts it; `repo` is the codebase of the team.
    pub fn start(root: &Path, repo: &Path, setup: &DocketSetup) -> Result<Self> {
        let model_id = setup
            .model
            .strip_prefix(&format!("{PROVIDER}/"))
            .with_context(|| {
                format!(
                    "docket's model is named provider/model; a world sets up {PROVIDER}, \
                     so `--model {PROVIDER}/<model>`, not {:?}",
                    setup.model
                )
            })?;
        let key = std::env::var(KEY_VARIABLE).with_context(|| {
            format!("the docket agent needs {KEY_VARIABLE} in the environment (--env-secret {KEY_VARIABLE})")
        })?;

        let dir = root.join("docket");
        let user_home = dir.join("user");
        let home = dir.join("home");
        std::fs::create_dir_all(&user_home)?;
        let token_file = dir.join("token");

        let mut path = setup.agent_path.clone();
        path.extend(["/usr/local/bin", "/usr/bin", "/bin"].map(PathBuf::from));
        let path = std::env::join_paths(path).context("build PATH")?;
        // Every docket command gets nothing from this process but what is named here.
        let docket = |args: &[&str]| {
            let mut command = Command::new("docket");
            command
                .args(args)
                .env_clear()
                .env("HOME", &user_home)
                .env("DOCKET_HOME", &home)
                .env("PATH", &path)
                .current_dir(&dir)
                .stdin(Stdio::null());
            command
        };

        // The key reaches docket's own 0600 store through this one command's environment; it is
        // on no command line and in no log.
        let mut add = docket(&[
            "setup",
            "provider",
            "add",
            PROVIDER,
            "--model",
            model_id,
            "--max-tokens",
            MAX_OUTPUT_TOKENS,
            "--no-preset",
        ]);
        add.env(KEY_VARIABLE, key);
        run(add, "docket setup provider add")?;

        let roles = run(docket(&["setup", "model", "--json"]), "docket setup model")?;
        let roles: Value = serde_json::from_str(&roles).context("docket setup model --json")?;
        let roles = roles["roles"]
            .as_array()
            .context("docket lists no roles")?
            .iter()
            .filter_map(|role| role["role"].as_str())
            .chain(["default"]);
        for role in roles {
            run(
                docket(&["setup", "model", "set", role, &setup.model]),
                &format!("docket setup model set {role}"),
            )?;
        }
        // Tack sends a run without network as a restriction, and docket refuses one while its
        // isolation is off: only its sandbox can cut the network.
        run(
            docket(&["setup", "sandbox", "on"]),
            "docket setup sandbox on",
        )?;

        let port = free_port()?;
        let log = dir.join("daemon.log");
        let mut start = docket(&[
            "start",
            "--dispatch",
            "--bind",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--token-file",
        ]);
        let daemon_log = File::create(&log)?;
        start
            .arg(&token_file)
            .stdout(daemon_log.try_clone()?)
            .stderr(daemon_log);
        let child = start.spawn().context("start docket")?;
        let mut server = Self {
            child,
            home,
            base_url: format!("http://127.0.0.1:{port}"),
            token_file,
            log,
        };
        let api = server.wait_until_ready()?;

        api.post(
            "/pods",
            &json!({"name": TEAM, "location": repo, "ensure": true}),
        )
        .context("docket would not make the team")?;
        Ok(server)
    }

    /// Waits for the daemon to answer with the token it wrote to the token file.
    fn wait_until_ready(&mut self) -> Result<Api> {
        let child = &mut self.child;
        let (base_url, token_file) = (&self.base_url, &self.token_file);
        let ready = poll_until_sync(Duration::from_secs(60), || {
            if let Ok(Some(status)) = child.try_wait() {
                return Some(Err(format!("docket exited before it answered: {status}")));
            }
            let token = std::fs::read_to_string(token_file).ok()?;
            let api = Api::with_token(base_url, Some(token.trim())).ok()?;
            api.get("/status.json").ok().map(|_| Ok(api))
        });
        match ready {
            Some(Ok(api)) => Ok(api),
            Some(Err(reason)) => bail!(reason),
            None => bail!("docket did not answer within 60s"),
        }
    }
}

impl Drop for Docket {
    fn drop(&mut self) {
        // Asked to stop first, so a step in flight is not left half-written; then made to.
        let _ = Command::new("kill")
            .args(["-TERM", &self.child.id().to_string()])
            .status();
        let stopped = poll_until_sync(Duration::from_secs(10), || {
            self.child.try_wait().ok().flatten().map(|_| ())
        });
        if stopped.is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        // The key rests in docket's store; a world that is kept does not keep it.
        for stale in [
            "secrets.json",
            "state/secrets.json",
            "state/secrets.json.bak",
        ] {
            let _ = std::fs::remove_file(self.home.join(stale));
        }
    }
}

/// Why a run never reached docket, in docket's team's terms (a Planner for a team that cannot
/// propose, a report team asked to change the repository): the attempt ended before it started
/// and Tack's reason begins "Not sent to docket". The job is one this team cannot do.
pub fn refusal(attempt: &Value) -> Option<String> {
    let reason = &attempt["terminal_reason"];
    (attempt["never_started"] == true && reason["code"] == "harness_rejected")
        .then(|| reason["message"].as_str())
        .flatten()
        .filter(|message| message.starts_with("Not sent to docket"))
        .map(str::to_owned)
}

/// Runs a docket command; its output comes back, and its failure names it (never the
/// environment, which holds the key).
fn run(mut command: Command, what: &str) -> Result<String> {
    let output = command.output().with_context(|| format!("run {what}"))?;
    if !output.status.success() {
        bail!(
            "{what} failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
                .trim()
                .chars()
                .take(400)
                .collect::<String>()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn free_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").context("find a free port")?;
    Ok(listener.local_addr()?.port())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_docket_never_got_is_a_refusal_with_its_reason() {
        let refused = json!({
            "never_started": true,
            "terminal_reason": {
                "code": "harness_rejected",
                "message": "Not sent to docket. The team repo cannot propose items."
            }
        });
        assert_eq!(
            refusal(&refused).as_deref(),
            Some("Not sent to docket. The team repo cannot propose items.")
        );
    }

    #[test]
    fn a_run_that_started_or_failed_otherwise_is_not_a_refusal() {
        let started = json!({
            "never_started": false,
            "terminal_reason": {"code": "harness_rejected", "message": "Not sent to docket."}
        });
        let other = json!({
            "never_started": true,
            "terminal_reason": {"code": "harness_rejected", "message": "the docket team works on a branch"}
        });
        assert_eq!(refusal(&started), None);
        assert_eq!(refusal(&other), None);
    }
}
