//! What `opencode auth list`, `opencode models` and `opencode debug config`
//! say about the login, the models and the default model.

use std::{future::Future, pin::Pin};

use tack_protocol::execution::{ModelListSource, ModelsReport, SignInReport, SignInState};

use crate::harness::local_process::{CommandOutcome, Readiness, ReadinessShell};

const SIGN_IN: &str = "opencode auth login";
const NO_LOGIN: &str = "No login: only opencode's free models are available";

pub(super) fn check<'a>(
    shell: &'a ReadinessShell,
) -> Pin<Box<dyn Future<Output = Readiness> + Send + 'a>> {
    Box::pin(async move {
        let (auth, models, config) = tokio::join!(
            shell.run(&["auth", "list"]),
            shell.run(&["models"]),
            shell.run(&["debug", "config"])
        );
        let config = parse_config(&config);
        let default_model = config.as_ref().ok().and_then(|config| config.model.clone());
        Readiness {
            sign_in: sign_in(&auth, &config),
            models: match parse_models(&models) {
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
            requires_model: config.as_ref().ok().map(|_| default_model.is_none()),
            default_model,
        }
    })
}

#[derive(Debug, PartialEq, Eq)]
struct Config {
    providers: Vec<String>,
    model: Option<String>,
}

/// Drops terminal colour sequences, which `opencode auth list` prints when
/// it believes it is talking to a person.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn parse_config(config: &CommandOutcome) -> Result<Config, String> {
    let stdout = match config {
        CommandOutcome::Ran {
            code: 0, stdout, ..
        } => stdout,
        CommandOutcome::Ran { code, .. } => {
            return Err(format!("`opencode debug config` exited with status {code}"));
        }
        CommandOutcome::TimedOut => return Err("`opencode debug config` timed out".to_owned()),
        CommandOutcome::Failed(why) => return Err(format!("`opencode debug config` {why}")),
    };
    let value: serde_json::Value = serde_json::from_str(stdout)
        .ok()
        .filter(serde_json::Value::is_object)
        .ok_or("`opencode debug config` printed nothing readable")?;
    Ok(Config {
        providers: value["provider"]
            .as_object()
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default(),
        model: value["model"]
            .as_str()
            .filter(|model| !model.trim().is_empty())
            .map(str::to_owned),
    })
}

/// The credentials `opencode auth list` names, or why it could not be read.
fn parse_credentials(auth: &CommandOutcome) -> Result<Vec<String>, String> {
    let stdout = match auth {
        CommandOutcome::Ran {
            code: 0, stdout, ..
        } => strip_ansi(stdout),
        CommandOutcome::Ran { code, .. } => {
            return Err(format!("`opencode auth list` exited with status {code}"));
        }
        CommandOutcome::TimedOut => return Err("`opencode auth list` timed out".to_owned()),
        CommandOutcome::Failed(why) => return Err(format!("`opencode auth list` {why}")),
    };
    if !stdout.contains("credential") {
        return Err("`opencode auth list` printed nothing readable".to_owned());
    }
    Ok(stdout
        .lines()
        .filter_map(|line| line.trim().strip_prefix('●'))
        .map(|rest| rest.trim().to_owned())
        .filter(|name| !name.is_empty())
        .collect())
}

fn sign_in(auth: &CommandOutcome, config: &Result<Config, String>) -> SignInReport {
    let report = |state, detail: String| SignInReport {
        state,
        detail,
        command: Some(SIGN_IN.to_owned()),
    };
    let credentials = parse_credentials(auth);
    if let Ok(names) = &credentials
        && !names.is_empty()
    {
        return report(SignInState::SignedIn, names.join(", "));
    }
    if let Ok(config) = config
        && !config.providers.is_empty()
    {
        return report(
            SignInState::SignedIn,
            format!(
                "provider {} in opencode's config",
                config.providers.join(", ")
            ),
        );
    }
    match (credentials, config) {
        (Ok(_), Ok(_)) => report(SignInState::SignedOut, NO_LOGIN.to_owned()),
        (Err(why), _) => report(SignInState::Unknown, why),
        (_, Err(why)) => report(SignInState::Unknown, why.clone()),
    }
}

/// One `provider/model` per line.
fn parse_models(listing: &CommandOutcome) -> Result<Vec<String>, String> {
    let stdout = match listing {
        CommandOutcome::Ran {
            code: 0, stdout, ..
        } => stdout,
        CommandOutcome::Ran { code, .. } => {
            return Err(format!("`opencode models` exited with status {code}"));
        }
        CommandOutcome::TimedOut => return Err("`opencode models` timed out".to_owned()),
        CommandOutcome::Failed(why) => return Err(format!("`opencode models` {why}")),
    };
    let ids: Vec<String> = strip_ansi(stdout)
        .lines()
        .map(str::trim)
        .filter(|line| line.contains('/') && !line.contains(char::is_whitespace))
        .map(str::to_owned)
        .collect();
    if ids.is_empty() {
        return Err("`opencode models` listed no models".to_owned());
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

    const AUTH: &str = include_str!("../fixtures/opencode/1.18.30/auth-list.txt");
    const AUTH_EMPTY: &str =
        include_str!("../fixtures/opencode/1.18.30/auth-list-scratch-home.txt");
    const CONFIG: &str = include_str!("../fixtures/opencode/1.18.30/debug-config.json");
    const CONFIG_EMPTY: &str =
        include_str!("../fixtures/opencode/1.18.30/debug-config-scratch-home.json");

    #[test]
    fn sign_in_reads_the_fixtures_and_says_why_when_it_cannot() {
        let rows = [
            (
                "a stored credential",
                ran(0, AUTH),
                ran(0, CONFIG_EMPTY),
                SignInState::SignedIn,
                "Anthropic oauth",
            ),
            (
                "a provider in the config",
                ran(0, AUTH_EMPTY),
                ran(0, CONFIG),
                SignInState::SignedIn,
                "llamacpp",
            ),
            (
                "neither",
                ran(0, AUTH_EMPTY),
                ran(0, CONFIG_EMPTY),
                SignInState::SignedOut,
                NO_LOGIN,
            ),
            (
                "garbage",
                ran(0, "hello"),
                ran(0, "nope"),
                SignInState::Unknown,
                "nothing readable",
            ),
            (
                "timeout",
                CommandOutcome::TimedOut,
                ran(0, CONFIG_EMPTY),
                SignInState::Unknown,
                "timed out",
            ),
            (
                "coloured output",
                ran(
                    0,
                    "\u{1b}[0m●  \u{1b}[32mOpenCode api\u{1b}[0m\n└  1 credential",
                ),
                ran(0, CONFIG_EMPTY),
                SignInState::SignedIn,
                "OpenCode api",
            ),
        ];
        for (name, auth, config, state, detail) in rows {
            let report = sign_in(&auth, &parse_config(&config));
            assert_eq!(report.state, state, "{name}");
            assert!(report.detail.contains(detail), "{name}: {}", report.detail);
            assert_eq!(
                report.command.as_deref(),
                Some("opencode auth login"),
                "{name}"
            );
        }
    }

    #[test]
    fn models_come_from_the_listing_and_the_default_from_the_config() {
        let listed = parse_models(&ran(
            0,
            include_str!("../fixtures/opencode/1.18.30/models.txt"),
        ))
        .expect("the fixture lists models");
        assert!(listed.len() > 50 && listed.iter().all(|id| id.contains('/')));
        let free = parse_models(&ran(
            0,
            include_str!("../fixtures/opencode/1.18.30/models-scratch-home.txt"),
        ))
        .expect("free models");
        assert_eq!(free.len(), 7);
        assert_eq!(free[0], "opencode/big-pickle");
        for (name, outcome, why) in [
            (
                "garbage",
                ran(0, "something went wrong"),
                "listed no models",
            ),
            ("failure", ran(1, ""), "status 1"),
            ("timeout", CommandOutcome::TimedOut, "timed out"),
        ] {
            assert!(parse_models(&outcome).unwrap_err().contains(why), "{name}");
        }
        let with_model = parse_config(&ran(0, CONFIG)).expect("config");
        assert_eq!(
            with_model.model.as_deref(),
            Some("opencode/claude-sonnet-5")
        );
        let without = parse_config(&ran(0, CONFIG_EMPTY)).expect("config");
        assert_eq!(without.model, None);
        assert!(parse_config(&ran(0, "garbage")).is_err());
    }
}
