//! What `docket setup harness --json` says about docket's own setup: which
//! models a run could reach, the default one, and why a run with no model
//! would be refused. It is offline and prints no credential value
//! (`fixtures/docket/0.2.0b5/README.md`).

use std::{future::Future, pin::Pin};

use tack_orch::execution::{ModelListSource, ModelsReport, SignInReport, SignInState};

use crate::harness::local_process::{CommandOutcome, Readiness, ReadinessShell};

/// docket's own first-run setup: it asks for whatever is missing.
const SIGN_IN: &str = "docket setup";

pub(super) fn check<'a>(
    shell: &'a ReadinessShell,
) -> Pin<Box<dyn Future<Output = Readiness> + Send + 'a>> {
    Box::pin(async move { readiness(&shell.run(&["setup", "harness", "--json"]).await) })
}

#[derive(serde::Deserialize)]
struct Document {
    ready: bool,
    #[serde(default)]
    reason: String,
    default_model: Option<String>,
    #[serde(default)]
    models: Vec<Model>,
}

#[derive(serde::Deserialize)]
struct Model {
    model: String,
    provider: String,
}

fn parse(outcome: &CommandOutcome) -> Result<Document, String> {
    let command = "`docket setup harness --json`";
    let stdout = match outcome {
        // 0 is ready and 1 is not; both print the document.
        CommandOutcome::Ran {
            code: 0 | 1,
            stdout,
            ..
        } => stdout,
        CommandOutcome::Ran { code, .. } => {
            return Err(format!("{command} exited with status {code}"));
        }
        CommandOutcome::TimedOut => return Err(format!("{command} timed out")),
        CommandOutcome::Failed(why) => return Err(format!("{command} {why}")),
    };
    serde_json::from_str(stdout).map_err(|_| format!("{command} printed nothing readable"))
}

pub(super) fn readiness(outcome: &CommandOutcome) -> Readiness {
    let document = match parse(outcome) {
        Ok(document) => document,
        Err(why) => {
            return Readiness {
                sign_in: SignInReport {
                    state: SignInState::Unknown,
                    detail: why.clone(),
                    command: Some(SIGN_IN.to_owned()),
                },
                models: ModelsReport {
                    source: ModelListSource::None,
                    ids: Vec::new(),
                    reason: Some(why),
                },
                default_model: None,
                requires_model: None,
            };
        }
    };
    let mut providers: Vec<&str> = document
        .models
        .iter()
        .map(|model| model.provider.as_str())
        .collect();
    providers.dedup();
    // A model is listed only when its provider needs no credential or has
    // one that resolves, so any listed model means docket can reach one.
    let sign_in = if document.models.is_empty() {
        SignInReport {
            state: SignInState::SignedOut,
            detail: document.reason.clone(),
            command: Some(SIGN_IN.to_owned()),
        }
    } else {
        let through = format!("provider {}", providers.join(", "));
        SignInReport {
            state: SignInState::SignedIn,
            detail: if document.ready {
                through
            } else {
                format!("{through}; {}", document.reason)
            },
            command: Some(SIGN_IN.to_owned()),
        }
    };
    let ids: Vec<String> = document.models.into_iter().map(|m| m.model).collect();
    let default_model = document
        .default_model
        .filter(|model| document.ready && !model.trim().is_empty());
    Readiness {
        sign_in,
        models: if ids.is_empty() {
            ModelsReport {
                source: ModelListSource::None,
                ids,
                reason: Some("docket's setup reaches no model".to_owned()),
            }
        } else {
            ModelsReport {
                source: ModelListSource::Listed,
                ids,
                reason: None,
            }
        },
        requires_model: Some(default_model.is_none()),
        default_model,
    }
}
