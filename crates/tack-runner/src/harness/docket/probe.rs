//! What a running docket reports about itself: its API version, its teams and
//! what keeps each from running lights-out. Everything is read over HTTP;
//! nothing here starts docket or spends a token.

use std::collections::BTreeSet;

use chrono::Utc;
use tack_orch::execution::{
    DocsLinks, HarnessCapability, HarnessKind as DomainHarnessKind, ModelListSource, ModelsReport,
    SignInReport, SignInState, TeamReport, TeamViolation,
};

use super::client::Client;

/// The only operator API version this harness speaks.
pub const API_VERSION: &str = "5";

/// The profile a team must hold to run without a person.
const LIGHTS_OUT: &str = "lights-out";

fn needs_beta_6(found: &str) -> String {
    format!("needs docket 0.2.0-beta.6 (API {API_VERSION}); this docket answers API {found}")
}

/// `GET /status.json`: the model ids its team members run on and docket's own version when it
/// sends one, or the sentence saying why this docket cannot be used.
pub(super) async fn status(client: &Client) -> Result<(Vec<String>, Option<String>), String> {
    let reply = client
        .status()
        .await
        .map_err(|why| format!("docket is not answering at {}: {why}", client.base()))?;
    if reply.status != 200 {
        return Err(format!(
            "docket is not answering at {}: {}",
            client.base(),
            reply.error()
        ));
    }
    let found = reply.body["apiVersion"].as_str().unwrap_or("unknown");
    if found != API_VERSION {
        return Err(needs_beta_6(found));
    }
    let models: BTreeSet<String> = reply.body["agents"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|agent| agent["model"].as_str())
        .filter(|model| !model.is_empty())
        .map(str::to_owned)
        .collect();
    let version = reply.body["version"]
        .as_str()
        .filter(|version| !version.is_empty())
        .map(str::to_owned);
    Ok((models.into_iter().collect(), version))
}

/// The teams docket reports, each with its profile's violations and the folders its sandbox reads.
pub(super) async fn teams(client: &Client) -> Result<Vec<TeamReport>, String> {
    let list = client.pods().await?;
    if list.status != 200 {
        return Err(list.error());
    }
    let mut teams = Vec::new();
    for pod in list.body["pods"].as_array().into_iter().flatten() {
        let Some(name) = pod["name"].as_str() else {
            continue;
        };
        let report = client.pod(name).await?;
        if report.status != 200 {
            return Err(report.error());
        }
        let profile = &report.body["profile"];
        teams.push(TeamReport {
            name: name.to_owned(),
            profile: profile["name"]
                .as_str()
                .or_else(|| pod["profile"].as_str())
                .unwrap_or_default()
                .to_owned(),
            violations: profile["violations"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|violation| TeamViolation {
                    requirement: violation["requirement"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    fix: violation["fix"].as_str().unwrap_or_default().to_owned(),
                })
                .collect(),
            // An older docket sends no key; that stays unknown rather than becoming "none".
            read_only: report.body["readOnly"].as_array().map(|paths| {
                paths
                    .iter()
                    .filter_map(|path| path.as_str().map(str::to_owned))
                    .collect()
            }),
        });
    }
    Ok(teams)
}

/// A team that needs a person during a run: attended, or with a violation.
fn capped(team: &TeamReport) -> bool {
    team.profile != LIGHTS_OUT || !team.violations.is_empty()
}

pub(super) async fn probe(
    client: &Client,
    decisions: tack_orch::execution::CapabilityValue,
) -> HarnessCapability {
    let mut capability = HarnessCapability {
        harness_kind: DomainHarnessKind::new(super::DESCRIPTOR.kind),
        installed_version: String::new(),
        probe_error: None,
        probed_at: Utc::now(),
        decisions: Some(decisions),
        requires_model: false,
        requires_model_reason: None,
        models: None,
        default_model: None,
        sign_in: None,
        docs: Some(DocsLinks {
            models: super::DESCRIPTOR.models_docs.to_owned(),
            sign_in: super::DESCRIPTOR.sign_in_docs.to_owned(),
        }),
        autonomy_ceiling: None,
        teams: None,
        additional: [
            ("sandbox".to_owned(), serde_json::json!("harness")),
            (
                "sandbox_reason".to_owned(),
                serde_json::json!("docket confines its own agents in a jail it sets up"),
            ),
        ]
        .into(),
    };
    let (models, version) = match status(client).await {
        Ok(answer) => answer,
        Err(why) => {
            // An answer from the wrong API version still says which one.
            if why.starts_with("needs docket")
                && let Some(found) = why.rsplit("answers API ").next()
            {
                capability.set_api_version(found);
            }
            capability.probe_error = Some(why);
            return capability;
        }
    };
    capability.installed_version = version.unwrap_or_default();
    capability.set_api_version(API_VERSION);
    let signed_in = !models.is_empty();
    capability.sign_in = Some(SignInReport {
        state: if signed_in {
            SignInState::SignedIn
        } else {
            SignInState::Unknown
        },
        detail: if signed_in {
            format!("docket's team members run on {}", models.join(", "))
        } else {
            "docket reports no model for its team members".to_owned()
        },
        command: None,
    });
    capability.models = Some(ModelsReport {
        source: if signed_in {
            ModelListSource::Listed
        } else {
            ModelListSource::None
        },
        reason: (!signed_in).then(|| "docket reports no model for its team members".to_owned()),
        ids: models,
    });
    match teams(client).await {
        Ok(teams) => {
            if !teams.is_empty() {
                capability.autonomy_ceiling =
                    Some(if teams.iter().any(capped) { "L3" } else { "L5" }.to_owned());
            }
            capability.teams = Some(teams);
        }
        Err(why) => {
            capability.probe_error = Some(format!("docket's teams could not be read: {why}"))
        }
    }
    capability
}
