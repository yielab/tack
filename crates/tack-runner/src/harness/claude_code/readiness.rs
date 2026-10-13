//! What `claude auth status` says about the login. Claude Code's CLI has no
//! command that lists models, so the ones offered are the aliases it accepts.

use std::{future::Future, pin::Pin};

use tack_orch::execution::{ModelListSource, ModelsReport, SignInReport, SignInState};

use crate::harness::local_process::{CommandOutcome, Readiness, ReadinessShell};

const COMMAND: &str = "claude auth status";
const SIGN_IN: &str = "claude";
const ALIASES: [&str; 3] = ["sonnet", "opus", "haiku"];

pub(super) fn check<'a>(
    shell: &'a ReadinessShell,
) -> Pin<Box<dyn Future<Output = Readiness> + Send + 'a>> {
    Box::pin(async move { readiness(&shell.run(&["auth", "status"]).await) })
}

fn readiness(status: &CommandOutcome) -> Readiness {
    Readiness {
        sign_in: sign_in(status),
        models: ModelsReport {
            source: ModelListSource::Suggested,
            ids: ALIASES.iter().map(|alias| (*alias).to_owned()).collect(),
            reason: None,
        },
        default_model: None,
        requires_model: None,
    }
}

fn sign_in(status: &CommandOutcome) -> SignInReport {
    let report = |state, detail: String| SignInReport {
        state,
        detail,
        command: Some(SIGN_IN.to_owned()),
    };
    let unknown = |why: String| report(SignInState::Unknown, why);
    let (stdout, stderr) = match status {
        CommandOutcome::Ran { stdout, stderr, .. } => (stdout, stderr),
        CommandOutcome::TimedOut => return unknown(format!("`{COMMAND}` timed out")),
        CommandOutcome::Failed(why) => return unknown(format!("`{COMMAND}` {why}")),
    };
    let parsed: Option<serde_json::Value> = serde_json::from_str(stdout.trim()).ok();
    match parsed
        .as_ref()
        .and_then(|value| value["loggedIn"].as_bool())
    {
        Some(true) => {
            let method = parsed
                .as_ref()
                .and_then(|value| value["authMethod"].as_str())
                .unwrap_or("signed in");
            report(SignInState::SignedIn, method.to_owned())
        }
        Some(false) => report(SignInState::SignedOut, "Not signed in".to_owned()),
        None => {
            let said: String = stdout
                .trim()
                .chars()
                .chain(stderr.trim().chars())
                .take(120)
                .collect();
            unknown(format!("`{COMMAND}` printed nothing readable: {said}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ran(code: i32, stdout: &str) -> CommandOutcome {
        CommandOutcome::Ran {
            code,
            stdout: stdout.to_owned(),
            stderr: String::new(),
        }
    }

    #[test]
    fn sign_in_reads_the_fixtures_and_says_why_when_it_cannot() {
        let rows = [
            (
                "signed in",
                ran(
                    0,
                    include_str!("../fixtures/claude_code/2.1.291/auth-status-signed-in.json"),
                ),
                SignInState::SignedIn,
                "claude.ai",
            ),
            (
                "signed out",
                ran(
                    1,
                    include_str!("../fixtures/claude_code/2.1.291/auth-status-signed-out.json"),
                ),
                SignInState::SignedOut,
                "Not signed in",
            ),
            (
                "garbage",
                ran(0, "<html>proxy error</html>"),
                SignInState::Unknown,
                "nothing readable",
            ),
            (
                "timeout",
                CommandOutcome::TimedOut,
                SignInState::Unknown,
                "timed out",
            ),
        ];
        for (name, outcome, state, detail) in rows {
            let report = sign_in(&outcome);
            assert_eq!(report.state, state, "{name}");
            assert!(report.detail.contains(detail), "{name}: {}", report.detail);
            assert_eq!(report.command.as_deref(), Some("claude"), "{name}");
        }
    }

    #[test]
    fn the_models_are_the_aliases_claude_accepts() {
        let ready = readiness(&CommandOutcome::TimedOut);
        assert_eq!(ready.models.source, ModelListSource::Suggested);
        assert_eq!(ready.models.ids, ["sonnet", "opus", "haiku"]);
        assert_eq!(ready.default_model, None);
    }
}
