//! The automatic merge's turn in the GitHub poll loop (ADR 0079): after the poll
//! has told the board what happened on GitHub, take a level off any domain whose
//! automatic merge was reverted, then try every change that may merge by itself.

use chrono::Utc;
use serde_json::{Value, json};
use tack_core::autonomy::{Check, Level, Trigger, level_for};
use tack_core::mrp::{MergeDecision, MergeReadinessPack};
use tack_db::repo::autonomy as store;
use tack_db::repo::budgets;
use tack_orch::execution::{HarnessCapability, TeamReport};
use tokio::io::AsyncReadExt;

use crate::handlers::runner_protocol::artifact_storage::ArtifactStorage;
use crate::merge::{self, FactsExtra};
use crate::router::AppState;

const MRP_MEDIA_TYPE: &str = "application/vnd.tack.mrp+json";

/// One pass: demotions first, so a revert costs its domain the level before the
/// next change in it is tried, then the held `done` merges, then the candidates.
pub async fn tick(state: &AppState) {
    let pool = state.repo.pool();
    match store::pending_demotions(pool).await {
        Ok(pending) => {
            for demotion in pending {
                demote(state, &demotion).await;
            }
        }
        Err(error) => tracing::warn!(%error, "autonomy: could not read pending demotions"),
    }
    for attempt_id in store::held_done_candidates(pool).await.unwrap_or_default() {
        let _ = merge::attempt(state, &attempt_id, Trigger::Done, FactsExtra::default()).await;
    }
    match store::auto_candidates(pool).await {
        Ok(candidates) => {
            for attempt_id in candidates {
                if let Some(extra) = gather(state, &attempt_id).await {
                    let _ = merge::attempt(state, &attempt_id, Trigger::Auto, extra).await;
                }
            }
        }
        Err(error) => tracing::warn!(%error, "autonomy: could not read candidates"),
    }
}

async fn demote(state: &AppState, pending: &store::PendingDemotion) {
    match store::demote(state.repo.pool(), pending, Utc::now()).await {
        Ok(Some((from, to))) => {
            let scope = pending
                .domain
                .as_ref()
                .map_or_else(|| "the project".to_owned(), |name| format!("domain {name}"));
            let why = if pending.reverted {
                "was reverted"
            } else {
                "went red on its merge commit"
            };
            merge::tell(
                state,
                &pending.item_id,
                format!(
                    "Tack took {scope} from {from} to {to}: pull request #{} (merged automatically) {why}",
                    pending.number
                ),
            )
            .await;
            // A project that drops to L3 no longer starts its own work.
            if pending.domain.is_none()
                && to == Level::L3
                && sqlx::query(
                    "UPDATE projects SET auto_start_enabled = 0 WHERE id = ? AND auto_start_enabled = 1",
                )
                .bind(&pending.project_id)
                .execute(state.repo.pool())
                .await
                .is_ok_and(|done| done.rows_affected() > 0)
            {
                merge::tell(
                    state,
                    &pending.item_id,
                    tack_core::auto_start::DROPPED_TO_L3.to_owned(),
                )
                .await;
            }
        }
        Ok(None) => {}
        Err(error) => tracing::warn!(%error, "autonomy: demotion failed"),
    }
}

async fn read_artifact(state: &AppState, reference: &str) -> Option<Vec<u8>> {
    let storage = ArtifactStorage::new(format!("{}/execution-artifacts", state.config.storage_dir));
    let mut file = storage.open_for_read(reference).await.ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).await.ok()?;
    Some(bytes)
}

/// The harnesses a stored capability snapshot reports; an entry or a snapshot
/// that does not parse (an old or sparse one) reports nothing.
pub(crate) fn harnesses_of(snapshot: &str) -> Vec<HarnessCapability> {
    serde_json::from_str::<Value>(snapshot)
        .ok()
        .and_then(|value| value.get("harnesses").and_then(Value::as_array).cloned())
        .into_iter()
        .flatten()
        .filter_map(|entry| serde_json::from_value(entry).ok())
        .collect()
}

pub(crate) fn kind_is(harness: &HarnessCapability, kind: &str) -> bool {
    serde_json::to_value(&harness.harness_kind)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .is_some_and(|name| name == kind)
}

/// A team that needs a person during a run: its profile is not lights-out, or it
/// reports a violation of the one it names.
pub(crate) fn is_attended(team: &TeamReport) -> bool {
    team.profile != "lights-out" || !team.violations.is_empty()
}

/// The most autonomy the team that ran an attempt allows. The named team's when
/// the runner reports teams (attended means L3); otherwise the harness's own
/// ceiling; otherwise unknown.
fn ceiling_of(snapshot: &str, kind: Option<&str>, team: Option<&str>) -> Option<Level> {
    let kind = kind?;
    let harness = harnesses_of(snapshot)
        .into_iter()
        .find(|harness| kind_is(harness, kind))?;
    let named = harness
        .teams
        .as_ref()
        .zip(team)
        .and_then(|(teams, name)| teams.iter().find(|t| t.name == name));
    match named {
        Some(team) => is_attended(team).then_some(Level::L3),
        None => harness.autonomy_ceiling.as_deref()?.parse().ok(),
    }
}

fn fail(reason: &str) -> Check {
    Check::Fail(reason.to_owned())
}

/// The facts an automatic merge needs from the board. `None` when the pack does
/// not say `merge` (nothing for Tack to decide) or cannot be read.
async fn gather(state: &AppState, attempt_id: &str) -> Option<FactsExtra> {
    let pool = state.repo.pool();
    let context = store::attempt_context(pool, attempt_id).await.ok()??;
    let settings = store::get_settings(pool, &context.project_id)
        .await
        .ok()??;
    let (pack_id, pack_ref) =
        store::newest_artifact_of_media_type(pool, attempt_id, MRP_MEDIA_TYPE)
            .await
            .ok()??;
    let pack: MergeReadinessPack =
        serde_json::from_slice(&read_artifact(state, &pack_ref).await?).ok()?;
    if pack.recommendation.decision != MergeDecision::Merge {
        return None;
    }

    let paths: Vec<String> = match store::newest_artifact_of_kind(pool, attempt_id, "evidence")
        .await
        .ok()?
    {
        Some((_, reference)) => read_artifact(state, &reference)
            .await
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .and_then(|evidence| {
                evidence["files"].as_array().map(|files| {
                    files
                        .iter()
                        .filter_map(|f| f["path"].as_str().map(str::to_owned))
                        .collect()
                })
            })
            .unwrap_or_default(),
        None => Vec::new(),
    };
    let (level, domain) = level_for(
        settings.level,
        &settings.domains,
        &paths,
        &context.item_type,
    );
    let mut extra = FactsExtra {
        decision: MergeDecision::Merge,
        level,
        domain,
        pack_artifact_id: Some(pack_id),
        policy: json!({
            "trigger": "auto",
            "project_level": settings.level.to_string(),
            "verify_require": settings.verify_require,
            "paths": paths,
        }),
        ..FactsExtra::default()
    };
    // Outside every domain authorised for automatic merge the change is L3, as
    // today: a person reviews it, and there is nothing to hold or to say.
    if level < tack_core::autonomy::Level::L4 {
        return None;
    }

    let snapshot: Value = serde_json::from_str(&context.request_snapshot).unwrap_or(Value::Null);
    let actual: Value = context
        .actual_execution
        .as_deref()
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or(Value::Null);
    // The stages the run was asked for, read off the pack: the merge is Tack's,
    // so whether the evidence is enough is Tack's to check.
    let required: Vec<String> = snapshot["verify_require"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|stage| stage.as_str().map(str::to_owned))
        .collect();
    extra.require = if required.is_empty() {
        fail(
            "The project's required verification stages are empty; automatic merge needs at least one",
        )
    } else {
        match pack.required_stage_missing(&required) {
            None => Check::Pass,
            Some(reason) => Check::Fail(reason),
        }
    };

    let head = actual["git"]["head_commit"].as_str().unwrap_or_default();
    let envelope = match store::newest_artifact_of_kind(pool, attempt_id, "mrp_attestation").await {
        Ok(Some((_, reference))) => read_artifact(state, &reference).await,
        _ => None,
    };
    extra.attestation = match (envelope, settings.attestation_public_key.as_deref()) {
        (None, _) => fail("The merge-readiness pack is not signed"),
        (Some(_), None) => {
            fail("The project has no public key to check the pack's signature against")
        }
        (Some(_), Some(_)) if head.is_empty() => {
            fail("The attempt recorded no head commit to check the signature against")
        }
        (Some(envelope), Some(key)) => {
            // The envelope and the head commit of an attempt never change, so a
            // signature verified on an earlier tick still holds; saving the
            // project's settings clears it (`put_settings`).
            let verified = store::get_merge(pool, attempt_id)
                .await
                .ok()
                .flatten()
                .and_then(|row| row.attestation_keyid);
            let checked = match verified {
                Some(keyid) => Ok(keyid),
                None => {
                    crate::prill::attest_verify(&state.config.prill_program, &envelope, key, head)
                        .await
                }
            };
            match checked {
                Ok(keyid) => {
                    extra.attestation_keyid = Some(keyid);
                    Check::Pass
                }
                Err(reason) => Check::Fail(format!("The pack's signature was refused: {reason}")),
            }
        }
    };

    extra.budget = budget(state, &context.project_id, &settings).await;
    extra.ceiling = match store::attempt_ceiling_inputs(pool, attempt_id).await {
        Ok(Some(inputs)) => ceiling_of(
            &inputs.capability_snapshot,
            inputs.harness_kind.as_deref(),
            inputs.team.as_deref(),
        ),
        _ => None,
    };
    Some(extra)
}

async fn budget(state: &AppState, project_id: &str, settings: &store::Settings) -> Check {
    let pool = state.repo.pool();
    let now = Utc::now();
    if let Some(limit) = settings.budgets.tokens_per_day
        && budgets::spent_today(pool, project_id, now)
            .await
            .unwrap_or(0)
            >= limit
    {
        return fail("This project used its tokens for today");
    }
    if let Some(limit) = settings.budgets.auto_merges_per_day
        && budgets::auto_merges_today(pool, project_id, now)
            .await
            .unwrap_or(0)
            >= limit
    {
        return Check::Fail(format!(
            "This project reached its limit of {limit} automatic merges for today"
        ));
    }
    Check::Pass
}
