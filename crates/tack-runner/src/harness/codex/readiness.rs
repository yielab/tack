//! What `codex login status` and `codex debug models` say about the login and
//! the models Codex offers.

use std::{future::Future, pin::Pin};

use tack_orch::execution::{ModelListSource, ModelsReport, SignInReport, SignInState};

use crate::harness::local_process::{CommandOutcome, Readiness, ReadinessShell};

const SIGN_IN: &str = "codex login";

pub(super) fn check<'a>(
    shell: &'a ReadinessShell,
) -> Pin<Box<dyn Future<Output = Readiness> + Send + 'a>> {
    Box::pin(async move {
        let (status, listing) = tokio::join!(
            shell.run(&["login", "status"]),
            shell.run(&["debug", "models"])
        );
        let mut models = parse_models(&listing, "codex debug models");
        if models.is_err() {
            // The catalog bundled in the binary needs no network and no login.
            let bundled = shell.run(&["debug", "models", "--bundled"]).await;
            models = parse_models(&bundled, "codex debug models --bundled");
        }
        Readiness {
            sign_in: sign_in(&status),
            models: match models {
                Ok(ids) => ModelsReport {
                    source: ModelListSource::Listed,
                    ids,
                    reason: None,
                },
                Err(reason) => ModelsReport {
                    source: ModelListSource::None,
                    ids: Vec::new(),
                    reason: Some(reason),
                },
            },
            default_model: None,
            requires_model: None,
        }
    })
}

fn sign_in(status: &CommandOutcome) -> SignInReport {
    let report = |state, detail: String| SignInReport {
        state,
        detail,
        command: Some(SIGN_IN.to_owned()),
    };
    let (code, said) = match status {
        CommandOutcome::Ran {
            code,
            stdout,
            stderr,
        } => (*code, format!("{stdout}\n{stderr}")),
        CommandOutcome::TimedOut => {
            return report(
                SignInState::Unknown,
                "`codex login status` timed out".to_owned(),
            );
        }
        CommandOutcome::Failed(why) => {
            return report(SignInState::Unknown, format!("`codex login status` {why}"));
        }
    };
    // Codex may print a warning before the answer.
    let answer = said
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("WARNING"));
    match answer {
        Some(line) if code == 0 && line.starts_with("Logged in") => {
            report(SignInState::SignedIn, line.to_owned())
        }
        Some(line) if code != 0 && line.to_lowercase().contains("not logged in") => {
            report(SignInState::SignedOut, line.to_owned())
        }
        _ => report(
            SignInState::Unknown,
            format!("`codex login status` printed nothing readable (exit {code})"),
        ),
    }
}

/// The slugs Codex offers (`visibility: list`), in the order it gives them.
fn parse_models(listing: &CommandOutcome, command: &str) -> Result<Vec<String>, String> {
    let stdout = match listing {
        CommandOutcome::Ran {
            code: 0, stdout, ..
        } => stdout,
        CommandOutcome::Ran { code, .. } => {
            return Err(format!("`{command}` exited with status {code}"));
        }
        CommandOutcome::TimedOut => return Err(format!("`{command}` timed out")),
        CommandOutcome::Failed(why) => return Err(format!("`{command}` {why}")),
    };
    let value: serde_json::Value = serde_json::from_str(stdout)
        .map_err(|_| format!("`{command}` printed nothing readable"))?;
    let entries = value["models"]
        .as_array()
        .ok_or_else(|| format!("`{command}` printed nothing readable"))?;
    let ids: Vec<String> = entries
        .iter()
        .filter(|entry| entry["visibility"] == "list")
        .filter_map(|entry| entry["slug"].as_str().map(str::to_owned))
        .collect();
    if ids.is_empty() {
        return Err(format!("`{command}` listed no models"));
    }
    Ok(ids)
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
                    include_str!("../fixtures/codex/0.149.1/login-status-signed-in.txt"),
                ),
                SignInState::SignedIn,
                "Logged in using ChatGPT",
            ),
            (
                "signed out, after a warning",
                ran(
                    1,
                    include_str!("../fixtures/codex/0.149.1/login-status-signed-out.txt"),
                ),
                SignInState::SignedOut,
                "Not logged in",
            ),
            (
                "garbage",
                ran(0, "segmentation noise"),
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
            assert_eq!(report.command.as_deref(), Some("codex login"), "{name}");
        }
    }

    #[test]
    fn models_keep_only_the_entries_codex_lists() {
        let rows = [
            (
                "refreshed catalog",
                ran(
                    0,
                    include_str!("../fixtures/codex/0.149.1/debug-models.json"),
                ),
                Ok(vec!["gpt-5.6-terra", "gpt-5.6-luna"]),
            ),
            (
                "bundled catalog",
                ran(
                    0,
                    include_str!("../fixtures/codex/0.149.1/debug-models-bundled.json"),
                ),
                Ok(vec![
                    "gpt-5.6-sol",
                    "gpt-5.6-terra",
                    "gpt-5.6-luna",
                    "gpt-5.5",
                    "gpt-5.2",
                ]),
            ),
            ("garbage", ran(0, "not json"), Err("nothing readable")),
            ("failure", ran(2, ""), Err("status 2")),
            ("timeout", CommandOutcome::TimedOut, Err("timed out")),
        ];
        for (name, outcome, expected) in rows {
            let parsed = parse_models(&outcome, "codex debug models");
            match expected {
                Ok(ids) => {
                    let parsed = parsed.unwrap_or_else(|why| panic!("{name}: {why}"));
                    assert_eq!(parsed, ids, "{name}");
                }
                Err(why) => assert!(parsed.unwrap_err().contains(why), "{name}"),
            }
        }
    }
}
