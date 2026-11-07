//! The daily pass that tells a person when the work got clearly worse.
//!
//! `tick` is one pass and returns how many notifications it opened; `spawn` adds the loop
//! `tack serve` runs. For each project it sets the success rate of the last seven days against the
//! 28 before it, cut by profile and agent, and opens one needs-you notification for
//! each cut whose rate fell (`tack_core::notification::quality_trend` says when). The notice's key
//! keeps a drop that persists from being announced again, and a pass that finds the cut recovered
//! closes it, so a later drop is announced anew.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Duration as Span, SecondsFormat, Utc};
use tack_core::notification::{Kind, Notice, Outcomes, quality_trend};
use tack_core::run_outcome::work_outcome;
use tack_db::Repository;
use tack_db::queries::metrics::{QualityAttempt, quality_attempts};
use tack_db::queries::projects::active_project_ids;
use tack_protocol::Clock;
use tack_protocol::execution::ExecutionState;
use uuid::Uuid;

use crate::AppError;
use crate::autonomy::MRP_MEDIA_TYPE;
use crate::background::{Loops, Start};
use crate::notifications::Inbox;

/// How often `tack serve` runs a pass.
pub const INTERVAL: Duration = Duration::from_secs(24 * 3600);

const RECENT: Span = Span::days(7);
const BEFORE: Span = Span::days(28);

/// How an attempt came out, when it counts as a run that ended after starting.
fn succeeded(attempt: &QualityAttempt) -> Option<bool> {
    let state: ExecutionState = attempt.state.parse().ok()?;
    let reason = attempt
        .terminal_reason
        .as_deref()
        .and_then(|raw| serde_json::from_str(raw).ok());
    work_outcome(state, reason.as_ref())
}

fn instant(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// The notices for the cuts of one project whose success fell.
async fn drops(
    repo: &Repository,
    project: Uuid,
    now: DateTime<Utc>,
) -> Result<Vec<Notice>, AppError> {
    let id = project.to_string();
    let since_recent = instant(now - RECENT);
    let all = quality_attempts(
        repo,
        &id,
        MRP_MEDIA_TYPE,
        Some(&instant(now - RECENT - BEFORE)),
    )
    .await?;
    let recent: HashSet<String> = quality_attempts(repo, &id, MRP_MEDIA_TYPE, Some(&since_recent))
        .await?
        .into_iter()
        .map(|attempt| attempt.attempt_id)
        .collect();
    // (cut, name) -> (recent, before)
    let mut cuts: BTreeMap<(&str, String), (Outcomes, Outcomes)> = BTreeMap::new();
    for attempt in &all {
        let Some(ok) = succeeded(attempt) else {
            continue;
        };
        let is_recent = recent.contains(&attempt.attempt_id);
        let named = [("profile", &attempt.profile), ("agent", &attempt.harness)];
        for (by, name) in named {
            let Some(name) = name else { continue };
            let (now_window, before_window) = cuts.entry((by, name.clone())).or_insert((
                Outcomes {
                    succeeded: 0,
                    ended: 0,
                },
                Outcomes {
                    succeeded: 0,
                    ended: 0,
                },
            ));
            let window = if is_recent { now_window } else { before_window };
            window.ended += 1;
            window.succeeded += u32::from(ok);
        }
    }
    Ok(cuts
        .into_iter()
        .filter_map(|((by, name), (recent, before))| {
            quality_trend(project, by, &name, recent, before)
        })
        .collect())
}

/// One pass: every cut of every project whose success fell is opened in `inbox`, unless it is open
/// already, and every open drop that no longer holds is closed. Returns how many were opened.
pub async fn tick(repo: &Repository, clock: &dyn Clock, inbox: &Inbox) -> Result<usize, AppError> {
    let now = clock.now();
    let open: Vec<_> = inbox
        .open_notifications(u32::MAX)
        .await?
        .into_iter()
        .filter(|notification| notification.kind == Kind::QualityTrend)
        .collect();
    let mut opened = 0;
    for id in active_project_ids(repo).await? {
        let Ok(project) = id.parse::<Uuid>() else {
            continue;
        };
        let notices = drops(repo, project, now).await?;
        for notice in &notices {
            if inbox.open(notice).await?.is_some() {
                opened += 1;
            }
        }
        for notification in open.iter().filter(|n| n.project_id == project) {
            if !notices.iter().any(|notice| notice.key == notification.key) {
                inbox.close(&notification.key).await?;
            }
        }
    }
    Ok(opened)
}

/// The loop: a pass every `interval`, the first at the start (it only reads, and a restart then
/// never postpones the check), a failed one logged.
pub fn spawn(
    loops: &mut Loops,
    repo: Repository,
    clock: Arc<dyn Clock>,
    inbox: Inbox,
    interval: Duration,
) {
    loops.spawn_tick("quality_trend", Start::Immediately, interval, move || {
        let (repo, clock, inbox) = (repo.clone(), clock.clone(), inbox.clone());
        async move {
            if let Err(error) = tick(&repo, clock.as_ref(), &inbox).await {
                tracing::warn!(%error, "Failed to check the quality trend");
            }
        }
    });
}
