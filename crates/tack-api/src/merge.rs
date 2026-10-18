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
    /// The most autonomy the team that ran the change allows; see `Facts::ceiling`.
    pub ceiling: Result<Option<Level>, String>,
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
            ceiling: Ok(None),
            policy: Value::Null,
        }
    }
}

const NOT_LINKED: &str =
    "The task is not linked to a GitHub issue, so no pull request can be opened";
const NO_TOKEN: &str = "GitHub is not reachable from this board (no GitHub token is set), so no pull request can be opened or merged";
const ALREADY_MERGED: &str = "Already merged";
const TOKEN_REFUSED: &str = "The GitHub token can't merge";

fn github_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("Tack/1.0 (github.com/yielab/tack)")
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
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
        ceiling: extra.ceiling,
    };

    // Everything that does not need GitHub first, in the policy's order.
    if let Err(hold) = evaluate(&facts) {
        return held(state, &context.item_id, row, previous, hold.to_string()).await;
    }

    let project = context.project_id.parse().unwrap_or_default();
    let Some(token) = github_token_for_project(state, project).await else {
        return held(state, &context.item_id, row, previous, NO_TOKEN.to_owned()).await;
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
            NOT_LINKED
        } else {
            "No pull request could be opened: the branch was not pushed or GitHub refused it"
        };
        return held(state, &context.item_id, row, previous, reason.to_owned()).await;
    };

    let client = match github_client() {
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

/// Why the Merge button cannot be pressed on this attempt now; `None` when it
/// can. It reads the ledger, the pull request and, for an open one, CI on the
/// head commit; it never opens a pull request or merges.
pub async fn disabled_reason(state: &AppState, attempt_id: &str) -> Option<String> {
    let pool = state.repo.pool();
    let Ok(Some(context)) = ledger::attempt_context(pool, attempt_id).await else {
        return Some("The attempt could not be read".to_owned());
    };
    let previous = ledger::get_merge(pool, attempt_id).await.ok().flatten();
    if previous.as_ref().is_some_and(|row| row.merged_at.is_some()) {
        return Some(ALREADY_MERGED.to_owned());
    }
    let attempt_state: Option<String> =
        sqlx::query_scalar("SELECT state FROM execution_attempts WHERE id = ?")
            .bind(attempt_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
    let git = context
        .actual_execution
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .map(|v| v["git"].clone())
        .unwrap_or(Value::Null);
    if attempt_state.as_deref() != Some("succeeded")
        || git["pushed"] != true
        || git["head_commit"].as_str().is_none()
    {
        return Some("The run has not finished with a pushed branch to merge".to_owned());
    }
    let Ok(Some(pr)) = pull_requests::get_for_attempt(pool, attempt_id).await else {
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
        return (!linked).then(|| NOT_LINKED.to_owned());
    };
    match pr.state.as_str() {
        "open" => {}
        "merged" => return Some(ALREADY_MERGED.to_owned()),
        other => return Some(format!("The pull request is {other}")),
    }
    if let Some(reason) = previous
        .and_then(|row| row.held_reason)
        .filter(|reason| reason.starts_with(TOKEN_REFUSED))
    {
        return Some(reason);
    }
    let project = context.project_id.parse().unwrap_or_default();
    if github_token_for_project(state, project).await.is_none() {
        return Some(NO_TOKEN.to_owned());
    }
    // CI is read when the button is pressed, never to draw it: a list of
    // attempts would otherwise call GitHub on every refresh.
    None
}

/// The button as the attempts list draws it: enabled when nothing local stops
/// a merge, with what held the last press as its reason; disabled with why.
pub async fn button(state: &AppState, attempt_id: &str) -> (bool, Option<String>) {
    if let Some(reason) = disabled_reason(state, attempt_id).await {
        return (false, Some(reason));
    }
    let last_hold = ledger::get_merge(state.repo.pool(), attempt_id)
        .await
        .ok()
        .flatten()
        .and_then(|row| row.held_reason);
    (true, last_hold)
}

/// An item entered a Done status. When its project merges on Done, the latest
/// succeeded attempt that pushed a branch is merged with the `done` trigger,
/// unless a person did not make the move (it came over MCP): then only the
/// item is told. CI still running leaves a held row the tick retries.
pub async fn on_done(state: &AppState, item_id: &str, project_id: &str, moved_over_mcp: bool) {
    let pool = state.repo.pool();
    let wanted = ledger::get_settings(pool, project_id)
        .await
        .ok()
        .flatten()
        .is_some_and(|settings| settings.merge_on_done);
    if !wanted {
        return;
    }
    let attempt_id: Option<String> = sqlx::query_scalar(
        "SELECT a.id FROM execution_attempts a
         JOIN execution_requests r ON r.id = a.request_id
         WHERE r.item_id = ? AND a.state = 'succeeded'
           AND json_extract(a.actual_execution, '$.git.pushed') = 1
           AND json_extract(a.actual_execution, '$.git.head_commit') IS NOT NULL
         ORDER BY a.created_at DESC, a.rowid DESC LIMIT 1",
    )
    .bind(item_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let Some(attempt_id) = attempt_id else {
        return;
    };
    if moved_over_mcp {
        tell(
            state,
            item_id,
            "Moved to Done over MCP: not merged; use Merge".to_owned(),
        )
        .await;
        return;
    }
    let _ = attempt(state, &attempt_id, Trigger::Done, FactsExtra::default()).await;
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
