//! Things a person asks of a runner's harness from the board (resume a team), which a runner
//! reaches the only way it reaches the board: it asks. A request is queued for one runner, handed
//! out on that runner's next poll and answered by the runner's result; the person's request waits
//! for that answer for a bounded time.
//!
//! The queue lives in memory because an operation is only worth doing while the person waits for
//! it: a board that restarts, or a runner that never polls, ends the wait with a sentence and
//! leaves nothing to run later. One board process holds it, so it is the process's, not a
//! part of any one router's state.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use tack_protocol::execution::{OperationOutcome, RunnerOperation, TeamOperation};
use tokio::sync::oneshot;
use uuid::Uuid;

/// Why a request got no answer from the runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unanswered {
    /// The runner did not poll and answer in time: it is busy, offline or an older version.
    TimedOut,
}

#[derive(Default)]
struct Queue {
    /// Waiting for the runner's next poll, by runner id.
    queued: HashMap<String, Vec<RunnerOperation>>,
    /// Handed out or queued, and still waited for: the runner it was sent to and where to answer.
    waiting: HashMap<String, (String, oneshot::Sender<OperationOutcome>)>,
}

#[derive(Default)]
pub struct RunnerOperations {
    queue: Mutex<Queue>,
}

impl RunnerOperations {
    fn locked(&self) -> std::sync::MutexGuard<'_, Queue> {
        self.queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Queues `operation` for `runner` and waits up to `within` for the runner's result.
    pub async fn ask(
        &self,
        runner: &str,
        operation: TeamOperation,
        within: Duration,
    ) -> Result<OperationOutcome, Unanswered> {
        let operation_id = format!("op_{}", Uuid::new_v4().simple());
        let (reply, answer) = oneshot::channel();
        {
            let mut queue = self.locked();
            queue
                .queued
                .entry(runner.to_owned())
                .or_default()
                .push(RunnerOperation {
                    operation_id: operation_id.clone(),
                    operation,
                });
            queue
                .waiting
                .insert(operation_id.clone(), (runner.to_owned(), reply));
        }
        let outcome = tokio::time::timeout(within, answer).await;
        if let Ok(Ok(outcome)) = outcome {
            return Ok(outcome);
        }
        // Not answered: drop what is left of it, so a late poll does not run what nobody waits for.
        let mut queue = self.locked();
        queue.waiting.remove(&operation_id);
        if let Some(waiting) = queue.queued.get_mut(runner) {
            waiting.retain(|queued| queued.operation_id != operation_id);
        }
        Err(Unanswered::TimedOut)
    }

    /// Everything queued for `runner`, once.
    pub fn take(&self, runner: &str) -> Vec<RunnerOperation> {
        self.locked().queued.remove(runner).unwrap_or_default()
    }

    /// The result of an operation `runner` was handed. `false` when nothing waits for it (the
    /// request gave up, or it was queued for another runner).
    pub fn finish(&self, runner: &str, operation_id: &str, outcome: OperationOutcome) -> bool {
        let mut queue = self.locked();
        match queue.waiting.get(operation_id) {
            Some((sent_to, _)) if sent_to == runner => {}
            _ => return false,
        }
        let Some((_, reply)) = queue.waiting.remove(operation_id) else {
            return false;
        };
        reply.send(outcome).is_ok()
    }
}

/// The board process's queue.
pub fn operations() -> &'static RunnerOperations {
    static QUEUE: OnceLock<RunnerOperations> = OnceLock::new();
    QUEUE.get_or_init(RunnerOperations::default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tack_protocol::execution::HarnessKind;

    fn resume() -> TeamOperation {
        TeamOperation::ResumeTeam {
            harness: HarnessKind::new("docket"),
            team: "saas-dev".to_owned(),
        }
    }

    #[tokio::test]
    async fn an_operation_goes_to_the_runner_it_was_queued_for_and_its_result_comes_back() {
        let queue = Arc::new(RunnerOperations::default());
        let asking = {
            let queue = Arc::clone(&queue);
            tokio::spawn(async move { queue.ask("r1", resume(), Duration::from_secs(5)).await })
        };
        let handed = loop {
            let handed = queue.take("r1");
            if !handed.is_empty() {
                break handed;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        };
        assert_eq!(handed.len(), 1);
        assert!(queue.take("r1").is_empty(), "handed out once");
        assert!(queue.take("r2").is_empty());
        let outcome = OperationOutcome::Refused {
            reason: "no".to_owned(),
        };
        assert!(!queue.finish("r2", &handed[0].operation_id, outcome.clone()));
        assert!(queue.finish("r1", &handed[0].operation_id, outcome.clone()));
        assert_eq!(asking.await.unwrap(), Ok(outcome));
    }

    #[tokio::test]
    async fn a_request_nobody_answers_ends_and_is_not_run_later() {
        let queue = RunnerOperations::default();
        let result = queue.ask("r1", resume(), Duration::from_millis(20)).await;
        assert_eq!(result, Err(Unanswered::TimedOut));
        assert!(queue.take("r1").is_empty());
    }
}
