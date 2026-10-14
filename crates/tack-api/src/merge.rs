//! The merge, once: every trigger (the automatic tick, the Merge button, a move
//! to Done) calls [`attempt`]. It opens the pull request when missing, reads CI
//! on the head commit, asks `tack_core::autonomy::evaluate`, merges with `sha`
//! set to that head, and writes the attempt's `tack_merges` row and an activity
//! comment on the item whether it merged or not.

use chrono::{SecondsFormat, Utc};
use serde_json::Value;
use tack_core::autonomy::{Check, CiState, Facts, Level, Trigger, evaluate};
use tack_core::mrp::MergeDecision;
use tack_db::repo::autonomy::{self as ledger, MergeRow};
use tack_db::repo::pull_requests;

use crate::github_sync::{ci_state, github_token_for_project, merge_pull_request};
use crate::handlers::runner_protocol::artifact_storage::ArtifactStorage;
use crate::router::AppState;

/// What only the caller can gather: the checks the automatic merge adds and the
/// facts the ledger keeps. The default is what a person's trigger needs, which
/// reads none of them.
#[derive(Debug, Clone)]
pub struct FactsExtra {
    pub decision: MergeDecision,
    pub level: Level,
    pub domain: Option<String>,
    pub require: Check,
    pub attestation: Check,
    pub attestation_keyid: Option<String>,
    pub budget: Check,
    pub pack_artifact_id: Option<String>,
    /// The settings that decided, kept with the row.
    pub policy: Value,
}

impl Default for FactsExtra {
    fn default() -> Self {
        FactsExtra {
            decision: MergeDecision::Review,
            level: Level::L3,
            domain: None,
            require: Check::Pass,
            attestation: Check::Pass,
            attestation_keyid: None,
            budget: Check::Pass,
            pack_artifact_id: None,
            policy: Value::Null,
        }
    }
}

fn ci_text(ci: CiState) -> &'static str {
    match ci {
        CiState::Green => "green",
        CiState::Pending => "pending",
        CiState::Red => "red",
        CiState::Absent => "absent",
    }
}

/// Try to merge the attempt's change. `Ok` is the merge commit's sha; `Err` is
/// the first condition that held it, already recorded and told to the item.
pub async fn attempt(
    state: &AppState,
    attempt_id: &str,
    trigger: Trigger,
    extra: FactsExtra,
) -> Result<String, String> {
    let pool = state.repo.pool();
    let context = match ledger::attempt_context(pool, attempt_id).await {
        Ok(Some(context)) => context,
        _ => return Err("The attempt could not be read".to_owned()),
    };
    let previous = ledger::get_merge(pool, attempt_id).await.ok().flatten();
    if let Some(done) = previous.as_ref().filter(|row| row.merged_at.is_some()) {
        return Ok(done.merge_sha.clone().unwrap_or_default());
    }
    let snapshot: Value = serde_json::from_str(&context.request_snapshot).unwrap_or(Value::Null);
    let head = context
        .actual_execution
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .and_then(|v| v["git"]["head_commit"].as_str().map(str::to_owned));

    let mut row = MergeRow {
        attempt_id: attempt_id.to_owned(),
        project_id: context.project_id.clone(),
        trigger: match trigger {
            Trigger::Button => "button",
            Trigger::Done => "done",
            Trigger::Auto => "auto",
        }
        .to_owned(),
        domain: extra.domain.clone(),
        level: Some(extra.level.to_string()),
        policy_snapshot: (!extra.policy.is_null()).then(|| extra.policy.to_string()),
        pack_artifact_id: extra.pack_artifact_id.clone(),
        attestation_keyid: extra.attestation_keyid.clone(),
        ci_state: None,
        held_reason: None,
        checked_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        merged_at: None,
        merge_sha: None,
        demoted_at: None,
    };
    let mut facts = Facts {
        trigger,
        from_mcp: snapshot["origin"] == "mcp",
        decision: extra.decision,
        level: extra.level,
        require: extra.require,
        attestation: extra.attestation,
        budget: extra.budget,
        pr_open: Check::Pass,
        ci: CiState::Green,
    };

    // Everything that does not need GitHub first, in the policy's order.
    if let Err(hold) = evaluate(&facts) {
        return held(state, &context.item_id, row, previous, hold.to_string()).await;
    }

    let project = context.project_id.parse().unwrap_or_default();
    let Some(token) = github_token_for_project(state, project).await else {
        return held(
            state,
            &context.item_id,
            row,
            previous,
            "GitHub is not reachable from this board (no GitHub token is set), so no pull request can be opened or merged"
                .to_owned(),
        )
        .await;
    };
    let mut pr = pull_requests::get_for_attempt(pool, attempt_id)
        .await
        .ok()
        .flatten();
    if pr.is_none() {
        let storage =
            ArtifactStorage::new(format!("{}/execution-artifacts", state.config.storage_dir));
        crate::handlers::pull_requests::open_pull_request_for_attempt(
            state.clone(),
            &storage,
            attempt_id,
        )
        .await;
        pr = pull_requests::get_for_attempt(pool, attempt_id)
            .await
            .ok()
            .flatten();
    }
    let linked = match context.item_id.parse() {
        Ok(item) => state
            .repo
            .get_github_link(item)
            .await
            .ok()
            .flatten()
            .is_some(),
        Err(_) => false,
    };
    let (Some(pr), Some(head)) = (pr, head) else {
        let reason = if !linked {
            "The task is not linked to a GitHub issue, so no pull request can be opened"
        } else {
            "No pull request could be opened: the branch was not pushed or GitHub refused it"
        };
        return held(state, &context.item_id, row, previous, reason.to_owned()).await;
    };

    let client = match reqwest::Client::builder()
        .user_agent("Tack/1.0 (github.com/yielab/tack)")
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(client) => client,
        Err(error) => return held(state, &context.item_id, row, previous, error.to_string()).await,
    };
    let base = state.config.github_api_base.as_str();
    let ci = match ci_state(&client, base, &token, &pr.repo, &head).await {
        Ok(ci) => ci,
        Err(error) => {
            let reason = format!("Tack could not read CI on the head commit: {error}");
            return held(state, &context.item_id, row, previous, reason).await;
        }
    };
    row.ci_state = Some(ci_text(ci).to_owned());
    facts.ci = ci;
    if let Err(hold) = evaluate(&facts) {
        return held(state, &context.item_id, row, previous, hold.to_string()).await;
    }

    match merge_pull_request(&client, base, &token, &pr.repo, pr.number, &head).await {
        Err(refused) => held(state, &context.item_id, row, previous, refused.0).await,
        Ok(sha) => {
            let now = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
            row.held_reason = None;
            row.merged_at = Some(now.clone());
            row.merge_sha = Some(sha.clone());
            if let Err(error) = ledger::record_merge(pool, &row).await {
                tracing::warn!(attempt_id, %error, "merge: merged but not recorded");
            }
            let _ =
                pull_requests::observe(pool, &pr.repo, pr.number, "merged", Some(&now), Some(&now))
                    .await;
            let _ = pull_requests::set_merge_sha(pool, &pr.repo, pr.number, &sha).await;
            let by = match (trigger, &row.domain) {
                (Trigger::Auto, Some(domain)) => {
                    format!("automatically (domain {domain}, {})", extra.level)
                }
                (Trigger::Auto, None) => format!("automatically ({})", extra.level),
                (Trigger::Button, _) => "with the Merge button".to_owned(),
                (Trigger::Done, _) => "when the task moved to Done".to_owned(),
            };
            tell(
                state,
                &context.item_id,
                format!("Tack merged pull request #{} {by}.", pr.number),
            )
            .await;
            if trigger == Trigger::Auto {
                let _ = crate::handlers::mrp::apply_mrp_accepted(state, &context.request_id).await;
            }
            Ok(sha)
        }
    }
}

/// Record that the merge did not happen, and say so on the item when the reason
/// is new.
async fn held(
    state: &AppState,
    item_id: &str,
    mut row: MergeRow,
    previous: Option<MergeRow>,
    reason: String,
) -> Result<String, String> {
    row.held_reason = Some(reason.clone());
    if let Err(error) = ledger::record_merge(state.repo.pool(), &row).await {
        tracing::warn!(attempt_id = %row.attempt_id, %error, "merge: hold not recorded");
    }
    if previous.and_then(|p| p.held_reason).as_deref() != Some(reason.as_str()) {
        tell(state, item_id, format!("Tack did not merge: {reason}")).await;
    }
    Err(reason)
}

/// An activity entry on the item.
pub(crate) async fn tell(state: &AppState, item_id: &str, text: String) {
    if let Ok(item) = item_id.parse() {
        let _ = state
            .repo
            .create_system_comment(item, text, tack_core::models::CommentType::System)
            .await;
    }
}
