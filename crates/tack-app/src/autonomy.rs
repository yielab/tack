//! The automatic merge's turn in the GitHub poll loop: after the poll
//! has told the board what happened on GitHub, take a level off any domain whose
//! automatic merge was reverted, then try every change that may merge by itself.

use serde_json::{Value, json};
use tack_core::autonomy::{Check, Level, Trigger, level_for};
use tack_db::repo::autonomy as store;
use tack_db::{Repository, queries};
use tack_protocol::Clock;
use tack_protocol::execution::{HarnessCapability, TeamReport};
use tack_protocol::mrp::{MergeDecision, MergeReadinessPack};

use crate::AppError;
use crate::host::Host;
use crate::merge::{self, FactsExtra};

pub(crate) const MRP_MEDIA_TYPE: &str = "application/vnd.tack.mrp+json";

/// What one pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Outcome {
    /// Domains (or projects) that lost a level.
    pub demoted: usize,
    /// Changes merged, by a held `done` trigger or by themselves.
    pub merged: usize,
}

/// One pass: demotions first, so a revert costs its domain the level before the
/// next change in it is tried, then the held `done` merges, then the candidates.
/// A phase that cannot read its rows does not stop the next; the first such failure is
/// returned after the pass.
pub async fn tick(
    repo: &Repository,
    clock: &dyn Clock,
    host: &dyn Host,
) -> Result<Outcome, AppError> {
    let mut outcome = Outcome::default();
    let mut failure = None;
    match queries::autonomy::pending_demotions(repo).await {
        Ok(pending) => {
            for demotion in pending {
                if demote(repo, clock, &demotion).await {
                    outcome.demoted += 1;
                }
            }
        }
        Err(error) => failure = failure.or(Some(error)),
    }
    for attempt_id in queries::autonomy::held_done_candidates(repo)
        .await
        .unwrap_or_default()
    {
        let merged = merge::attempt(
            repo,
            clock,
            host,
            &attempt_id,
            Trigger::Done,
            FactsExtra::default(),
        )
        .await;
        outcome.merged += usize::from(merged.is_ok());
    }
    match queries::autonomy::auto_candidates(repo).await {
        Ok(candidates) => {
            for attempt_id in candidates {
                if let Some(extra) = gather(repo, clock, host, &attempt_id).await {
                    let merged =
                        merge::attempt(repo, clock, host, &attempt_id, Trigger::Auto, extra).await;
                    outcome.merged += usize::from(merged.is_ok());
                }
            }
        }
        Err(error) => failure = failure.or(Some(error)),
    }
    match failure {
        Some(error) => Err(error.into()),
        None => Ok(outcome),
    }
}

/// Take the level off the domain a reverted merge belonged to. Whether it was taken.
async fn demote(repo: &Repository, clock: &dyn Clock, pending: &store::PendingDemotion) -> bool {
    match repo.demote_after_revert(pending, clock.now()).await {
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
                repo,
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
                && repo
                    .disable_auto_start(&pending.project_id)
                    .await
                    .unwrap_or(false)
            {
                merge::tell(
                    repo,
                    &pending.item_id,
                    tack_core::auto_start::DROPPED_TO_L3.to_owned(),
                )
                .await;
            }
            true
        }
        Ok(None) => false,
        Err(error) => {
            tracing::warn!(%error, "autonomy: demotion failed");
            false
        }
    }
}

/// The harnesses a stored capability snapshot reports; an entry or a snapshot
/// that does not parse (an old or sparse one) reports nothing.
pub fn harnesses_of(snapshot: &str) -> Vec<HarnessCapability> {
    serde_json::from_str::<Value>(snapshot)
        .ok()
        .and_then(|value| value.get("harnesses").and_then(Value::as_array).cloned())
        .into_iter()
        .flatten()
        .filter_map(|entry| serde_json::from_value(entry).ok())
        .collect()
}

pub fn kind_is(harness: &HarnessCapability, kind: &str) -> bool {
    serde_json::to_value(&harness.harness_kind)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .is_some_and(|name| name == kind)
}

/// A team that needs a person during a run: its profile is not lights-out, or it
/// reports a violation of the one it names.
pub fn is_attended(team: &TeamReport) -> bool {
    team.profile != "lights-out" || !team.violations.is_empty()
}

/// The most autonomy the team that ran an attempt allows: the named team's as the
/// computer reports it now (attended means L3), or the sentence that says the
/// computer does not report it. A run of an agent with no team keeps the
/// harness's own ceiling; `Ok(None)` is unknown.
fn ceiling_of(
    snapshot: &str,
    kind: Option<&str>,
    team: Option<&str>,
) -> Result<Option<Level>, String> {
    let harness = kind.and_then(|kind| {
        harnesses_of(snapshot)
            .into_iter()
            .find(|harness| kind_is(harness, kind))
    });
    let Some(name) = team else {
        return Ok(harness.and_then(|harness| harness.autonomy_ceiling?.parse().ok()));
    };
    harness
        .and_then(|harness| harness.teams?.into_iter().find(|t| t.name == name))
        .map(|team| is_attended(&team).then_some(Level::L3))
        .ok_or_else(|| format!("Team {name} is not reported yet by this computer"))
}

/// What the harness a request asked for is called when it is docket's.
const DOCKET: &str = "docket";

/// The level a run was asked to run at, when a person asked for one ("Run again at L3").
fn requested_level(snapshot: &Value) -> Option<Level> {
    snapshot.get("autonomy")?.as_str()?.parse().ok()
}

/// The level the board tells docket a claimed task may run at: the one the run was created with,
/// as it is, and none otherwise. Docket decides what a team may do alone; the project's merge
/// level is never turned into it. `None` when the harness is not docket or no level was asked
/// for; the key is then left out.
pub(crate) fn level_to_send(snapshot: &Value) -> Option<Level> {
    if snapshot["requested_harness_kind"].as_str() != Some(DOCKET) {
        return None;
    }
    requested_level(snapshot)
}

/// Whether "Run again at L3" may be pressed on an attempt that ended asking for a person, and
/// the reason when it may not: only a docket run can be run again with a level, and not one that
/// already ran with L3. Whether docket accepts the level is docket's answer.
pub async fn rerun_at_l3(repo: &Repository, attempt_id: &str) -> (bool, Option<String>) {
    let no = |reason: &str| (false, Some(reason.to_owned()));
    let Ok(Some(context)) = queries::autonomy::attempt_context(repo, attempt_id).await else {
        return no("The run could not be read");
    };
    let snapshot: Value = serde_json::from_str(&context.request_snapshot).unwrap_or(Value::Null);
    if snapshot["requested_harness_kind"].as_str() != Some(DOCKET) {
        return no("Only a run by a docket team can be run again at L3");
    }
    if requested_level(&snapshot) == Some(Level::L3) {
        return no("This run already ran at L3");
    }
    (true, None)
}

fn fail(reason: &str) -> Check {
    Check::Fail(reason.to_owned())
}

/// The facts an automatic merge needs from the board. `None` when the pack does
/// not say `merge` (nothing for Tack to decide) or cannot be read.
async fn gather(
    repo: &Repository,
    clock: &dyn Clock,
    host: &dyn Host,
    attempt_id: &str,
) -> Option<FactsExtra> {
    let context = queries::autonomy::attempt_context(repo, attempt_id)
        .await
        .ok()??;
    let settings = queries::autonomy::project_settings(repo, &context.project_id)
        .await
        .ok()??;
    let (pack_id, pack_ref) =
        queries::autonomy::newest_artifact_of_media_type(repo, attempt_id, MRP_MEDIA_TYPE)
            .await
            .ok()??;
    let pack: MergeReadinessPack =
        serde_json::from_slice(&host.read_artifact(&pack_ref).await?).ok()?;
    if pack.recommendation.decision != MergeDecision::Merge {
        return None;
    }

    let paths: Vec<String> =
        match queries::autonomy::newest_artifact_of_kind(repo, attempt_id, "evidence")
            .await
            .ok()?
        {
            Some((_, reference)) => host
                .read_artifact(&reference)
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
    let envelope =
        match queries::autonomy::newest_artifact_of_kind(repo, attempt_id, "mrp_attestation").await
        {
            Ok(Some((_, reference))) => host.read_artifact(&reference).await,
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
            // project's settings clears it (`put_autonomy_settings`).
            let verified = queries::autonomy::merge_of_attempt(repo, attempt_id)
                .await
                .ok()
                .flatten()
                .and_then(|row| row.attestation_keyid);
            let checked = match verified {
                Some(keyid) => Ok(keyid),
                None => host.verify_attestation(&envelope, key, head).await,
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

    extra.budget = budget(repo, clock, &context.project_id, &settings).await;
    if let Ok(Some(inputs)) = queries::autonomy::attempt_ceiling_inputs(repo, attempt_id).await {
        extra.ceiling = ceiling_of(
            &inputs.capability_snapshot,
            inputs.harness_kind.as_deref(),
            inputs.team.as_deref(),
        );
    }
    Some(extra)
}

async fn budget(
    repo: &Repository,
    clock: &dyn Clock,
    project_id: &str,
    settings: &store::Settings,
) -> Check {
    let now = clock.now();
    if let Ok(Some(day)) = queries::budgets::day_room(repo, project_id, now).await
        && day.counted >= day.limit
    {
        return fail("This project used its tokens for today");
    }
    if let Some(limit) = settings.budgets.auto_merges_per_day
        && queries::budgets::auto_merges_today(repo, project_id, now)
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
