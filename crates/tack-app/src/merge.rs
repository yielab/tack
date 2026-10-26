//! The merge, once: every trigger (the automatic tick, the Merge button, a move
//! to Done) calls [`attempt`]. It opens the pull request when missing, reads CI
//! on the head commit, asks `tack_core::autonomy::evaluate`, merges with `sha`
//! set to that head, and writes the attempt's `tack_merges` row and an activity
//! comment on the item whether it merged or not.

use chrono::SecondsFormat;
use serde_json::Value;
use tack_core::autonomy::{Check, CiState, Facts, Level, Trigger, evaluate};
use tack_db::repo::autonomy::MergeRow;
use tack_db::{Repository, queries};
use tack_protocol::Clock;
use tack_protocol::execution::ExecutionState;
use tack_protocol::mrp::MergeDecision;

use crate::host::Host;

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
    repo: &Repository,
    clock: &dyn Clock,
    host: &dyn Host,
    attempt_id: &str,
    trigger: Trigger,
    extra: FactsExtra,
) -> Result<String, String> {
    let context = match queries::autonomy::attempt_context(repo, attempt_id).await {
        Ok(Some(context)) => context,
        _ => return Err("The attempt could not be read".to_owned()),
    };
    let previous = queries::autonomy::merge_of_attempt(repo, attempt_id)
        .await
        .ok()
        .flatten();
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
        checked_at: clock.now().to_rfc3339_opts(SecondsFormat::Secs, true),
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
        return held(repo, &context.item_id, row, previous, hold.to_string()).await;
    }

    let project = context.project_id.parse().unwrap_or_default();
    let Some(token) = host.github_token(project).await else {
        return held(repo, &context.item_id, row, previous, NO_TOKEN.to_owned()).await;
    };
    let mut pr = queries::pull_requests::pull_request_of_attempt(repo, attempt_id)
        .await
        .ok()
        .flatten();
    if pr.is_none() {
        host.open_pull_request(attempt_id).await;
        pr = queries::pull_requests::pull_request_of_attempt(repo, attempt_id)
            .await
            .ok()
            .flatten();
    }
    let linked = match context.item_id.parse() {
        Ok(item) => repo.get_github_link(item).await.ok().flatten().is_some(),
        Err(_) => false,
    };
    let (Some(pr), Some(head)) = (pr, head) else {
        let reason = if !linked {
            NOT_LINKED
        } else {
            "No pull request could be opened: the branch was not pushed or GitHub refused it"
        };
        return held(repo, &context.item_id, row, previous, reason.to_owned()).await;
    };

    let client = match github_client() {
        Ok(client) => client,
        Err(error) => return held(repo, &context.item_id, row, previous, error.to_string()).await,
    };
    let ci = match host.ci_state(&client, &token, &pr.repo, &head).await {
        Ok(ci) => ci,
        Err(error) => {
            let reason = format!("Tack could not read CI on the head commit: {error}");
            return held(repo, &context.item_id, row, previous, reason).await;
        }
    };
    row.ci_state = Some(ci_text(ci).to_owned());
    facts.ci = ci;
    if let Err(hold) = evaluate(&facts) {
        return held(repo, &context.item_id, row, previous, hold.to_string()).await;
    }

    match host
        .merge_pull_request(&client, &token, &pr.repo, pr.number, &head)
        .await
    {
        Err(refused) => held(repo, &context.item_id, row, previous, refused).await,
        Ok(sha) => {
            let now = clock.now().to_rfc3339_opts(SecondsFormat::Secs, true);
            row.held_reason = None;
            row.merged_at = Some(now.clone());
            row.merge_sha = Some(sha.clone());
            if let Err(error) = repo.record_merge(&row).await {
                tracing::warn!(attempt_id, %error, "merge: merged but not recorded");
            }
            let _ = repo
                .record_pull_request_merged(&pr.repo, pr.number, &now, &sha)
                .await;
            let by = match (trigger, &row.domain) {
                (Trigger::Auto, Some(domain)) => {
                    format!("automatically (domain {domain}, {})", extra.level)
                }
                (Trigger::Auto, None) => format!("automatically ({})", extra.level),
                (Trigger::Button, _) => "with the Merge button".to_owned(),
                (Trigger::Done, _) => "when the task moved to Done".to_owned(),
            };
            tell(
                repo,
                &context.item_id,
                format!("Tack merged pull request #{} {by}.", pr.number),
            )
            .await;
            if trigger == Trigger::Auto {
                host.apply_mrp_accepted(&context.request_id).await;
            }
            Ok(sha)
        }
    }
}

/// Why the Merge button cannot be pressed on this attempt now; `None` when it
/// can. It reads the ledger, the pull request and, for an open one, CI on the
/// head commit; it never opens a pull request or merges.
pub async fn disabled_reason(
    repo: &Repository,
    host: &dyn Host,
    attempt_id: &str,
) -> Option<String> {
    let Ok(Some(context)) = queries::autonomy::attempt_context(repo, attempt_id).await else {
        return Some("The attempt could not be read".to_owned());
    };
    let previous = queries::autonomy::merge_of_attempt(repo, attempt_id)
        .await
        .ok()
        .flatten();
    if previous.as_ref().is_some_and(|row| row.merged_at.is_some()) {
        return Some(ALREADY_MERGED.to_owned());
    }
    let attempt_state = queries::execution_attempts::attempt_state(repo, attempt_id)
        .await
        .ok()
        .flatten();
    let git = context
        .actual_execution
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .map(|v| v["git"].clone())
        .unwrap_or(Value::Null);
    if attempt_state != Some(ExecutionState::Succeeded) {
        return Some("The run has not succeeded".to_owned());
    }
    if git["pushed"] != true || git["head_commit"].as_str().is_none() {
        return Some("No branch was pushed".to_owned());
    }
    let Ok(Some(pr)) = queries::pull_requests::pull_request_of_attempt(repo, attempt_id).await
    else {
        let linked = match context.item_id.parse() {
            Ok(item) => repo.get_github_link(item).await.ok().flatten().is_some(),
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
    if host.github_token(project).await.is_none() {
        return Some(NO_TOKEN.to_owned());
    }
    // CI is read when the button is pressed, never to draw it: a list of
    // attempts would otherwise call GitHub on every refresh.
    None
}

/// The button as the attempts list draws it: enabled when nothing local stops
/// a merge, with what held the last press as its reason; disabled with why.
pub async fn button(
    repo: &Repository,
    host: &dyn Host,
    attempt_id: &str,
) -> (bool, Option<String>) {
    if let Some(reason) = disabled_reason(repo, host, attempt_id).await {
        return (false, Some(reason));
    }
    let last_hold = queries::autonomy::merge_of_attempt(repo, attempt_id)
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
pub async fn on_done(
    repo: &Repository,
    clock: &dyn Clock,
    host: &dyn Host,
    item_id: &str,
    project_id: &str,
    moved_over_mcp: bool,
) {
    let wanted = queries::autonomy::project_settings(repo, project_id)
        .await
        .ok()
        .flatten()
        .is_some_and(|settings| settings.merge_on_done);
    if !wanted {
        return;
    }
    let attempt_id = queries::execution_attempts::newest_pushed_attempt_of_item(repo, item_id)
        .await
        .ok()
        .flatten();
    let Some(attempt_id) = attempt_id else {
        return;
    };
    if moved_over_mcp {
        tell(
            repo,
            item_id,
            "Moved to Done over MCP: not merged; use Merge".to_owned(),
        )
        .await;
        return;
    }
    let _ = attempt(
        repo,
        clock,
        host,
        &attempt_id,
        Trigger::Done,
        FactsExtra::default(),
    )
    .await;
}

/// Record that the merge did not happen, and say so on the item when the reason
/// is new.
async fn held(
    repo: &Repository,
    item_id: &str,
    mut row: MergeRow,
    previous: Option<MergeRow>,
    reason: String,
) -> Result<String, String> {
    row.held_reason = Some(reason.clone());
    if let Err(error) = repo.record_merge(&row).await {
        tracing::warn!(attempt_id = %row.attempt_id, %error, "merge: hold not recorded");
    }
    if previous.and_then(|p| p.held_reason).as_deref() != Some(reason.as_str()) {
        tell(repo, item_id, format!("Tack did not merge: {reason}")).await;
    }
    Err(reason)
}

/// An activity entry on the item.
pub async fn tell(repo: &Repository, item_id: &str, text: String) {
    if let Ok(item) = item_id.parse() {
        let _ = repo
            .create_system_comment(item, text, tack_core::models::CommentType::System)
            .await;
    }
}
