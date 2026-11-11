//! Shared fixtures: task directories written on the fly, the `tack` binary, the fake agent.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use tack_evals::locate_tack_bin;
use tack_evals::task::LoadedTask;
use tack_evals::trial::TrialOptions;
use tack_evals::world::WorldOptions;
use tack_test_support::fake_agent::FakeAgent;

/// The `tack` binary: the one already built beside the tests, else built once here.
pub fn tack_bin() -> PathBuf {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| {
        if let Ok(found) = locate_tack_bin(None) {
            return found;
        }
        let built = Command::new(env!("CARGO"))
            .args(["build", "-p", "tack-cli"])
            .status()
            .expect("run cargo");
        assert!(built.success(), "cargo build -p tack-cli failed");
        locate_tack_bin(None).expect("tack was built")
    })
    .clone()
}

/// The text of a task whose hidden check is `check`, with `extra` lines before `graders`.
pub fn task_yaml(id: &str, extra: &str) -> String {
    format!(
        "id: {id}\n\
         version: 1\n\
         suite: regression\n\
         instruction: Make the greeting loud.\n\
         item: {{ type: task }}\n\
         profile: {{ harness: claude-code, model: claude-haiku-5-5 }}\n\
         sandbox: any\n\
         {extra}\n\
         graders:\n\
         \x20 required:\n\
         \x20   - outcome.hidden_tests: {{ command: \"sh hidden/check.sh\" }}\n\
         \x20   - outcome.run_state: succeeded\n\
         \x20   - governance.no_violation\n\
         limits: {{ wall_s: 120 }}\n"
    )
}

/// A task directory under `root`: a `greeting.txt` seed, an exam that wants it loud and a
/// solution that makes it so.
pub fn write_task(root: &Path, id: &str, extra: &str) -> PathBuf {
    let dir = root.join(id);
    for sub in ["repo", "hidden", "solution"] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
    }
    std::fs::write(dir.join("task.yaml"), task_yaml(id, extra)).unwrap();
    std::fs::write(dir.join("repo/greeting.txt"), "hello\n").unwrap();
    std::fs::write(dir.join("solution/greeting.txt"), "HELLO\n").unwrap();
    std::fs::write(dir.join("hidden/check.sh"), "grep -q HELLO greeting.txt\n").unwrap();
    dir
}

pub fn load(dir: &Path) -> LoadedTask {
    LoadedTask::load(dir).expect("the task loads")
}

/// Puts a fake `claude` that behaves as `agent` says in `root/agent` and returns that directory.
pub fn install_agent(root: &Path, agent: &FakeAgent) -> PathBuf {
    let dir = root.join("agent");
    agent.install(&dir).expect("install the fake agent");
    dir
}

/// A fake agent that makes the greeting loud, as the reference solution does.
pub fn loud_agent() -> FakeAgent {
    FakeAgent {
        writes: vec![("greeting.txt".to_owned(), "HELLO\n".to_owned())],
        asks: None,
    }
}

pub fn trial_options(root: &Path, agent_dir: PathBuf) -> TrialOptions {
    TrialOptions {
        out: root.join("out"),
        epoch: 1,
        harness: None,
        model: None,
        world: WorldOptions {
            tack_bin: tack_bin(),
            agent_path: vec![agent_dir],
            env_secrets: Vec::new(),
            keep: false,
        },
    }
}
