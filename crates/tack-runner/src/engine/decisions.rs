//! The questions a running harness asks, carried out to the operator as
//! decisions and answered back.

use tokio::sync::mpsc;

use super::{
    AnsweredElsewhere, DecisionState, HarnessAdapter, POLL_FROM_THE_START, PendingDecision,
    Question, RunnerEngine,
};
use crate::client::{
    DecisionAnswer, DecisionAnsweredElsewhereReport, DecisionCreateReport, DecisionPollReport,
    PullProtocol, Recommendation, RunnerSession, Timestamp, journal::AttemptJournal,
    workspace::WorktreeProvisioner,
};

/// Posts `answer` back to the harness, if anything is still listening. Best
/// effort: a dropped receiver means the run has already moved on without it.
async fn send_answer(
    decisions: &mut Option<(mpsc::Receiver<Question>, mpsc::Sender<DecisionAnswer>)>,
    answer: DecisionAnswer,
) {
    if let Some((_, answers_tx)) = decisions.as_mut() {
        let _ = answers_tx.send(answer).await;
    }
}

/// The answer nobody gave, marked as such (`unanswered`) so an adapter that must not read it as a
/// refusal can tell it from one. By convention every grammar orders a question's
/// options with its safe default last (`decision.create.request.json`'s own
/// example: `allow_once` then `deny`), so this is always that one.
fn deny_answer(question: &Question) -> DecisionAnswer {
    match question.options.last() {
        Some(option) => DecisionAnswer {
            option_id: Some(option.option_id.clone()),
            text: None,
            unanswered: true,
        },
        None => DecisionAnswer {
            option_id: None,
            text: None,
            unanswered: true,
        },
    }
}

pub(super) fn decision_id_for(record: &AttemptJournal, question: &Question) -> String {
    let material = format!(
        "{}:{}:{}",
        record.attempt_id.as_str(),
        record.fencing_token.0,
        question.vendor_id
    );
    format!(
        "dec_{}",
        material
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

/// The same secret scrub captured output gets, applied a second time at the
/// boundary this decision actually leaves the runner over: the resolved
/// prompt is the one secret this layer independently knows, regardless of
/// what a harness's own grammar already scrubbed against its full registered
/// secret set before the question ever reached this channel.
pub(super) fn scrub_with_prompt(text: &str, prompt: &str) -> String {
    if prompt.is_empty() || !text.contains(prompt) {
        text.to_owned()
    } else {
        text.replace(prompt, "[REDACTED]")
    }
}

fn scrub_metadata_with_prompt(
    metadata: serde_json::Map<String, serde_json::Value>,
    prompt: &str,
) -> serde_json::Map<String, serde_json::Value> {
    fn scrub_value(value: serde_json::Value, prompt: &str) -> serde_json::Value {
        match value {
            serde_json::Value::String(text) => {
                serde_json::Value::String(scrub_with_prompt(&text, prompt))
            }
            serde_json::Value::Array(items) => serde_json::Value::Array(
                items
                    .into_iter()
                    .map(|item| scrub_value(item, prompt))
                    .collect(),
            ),
            serde_json::Value::Object(map) => serde_json::Value::Object(
                map.into_iter()
                    .map(|(key, item)| (key, scrub_value(item, prompt)))
                    .collect(),
            ),
            other => other,
        }
    }
    match scrub_value(serde_json::Value::Object(metadata), prompt) {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    }
}

impl<P, A, W, C> RunnerEngine<P, A, W, C>
where
    P: PullProtocol,
    A: HarnessAdapter,
    W: WorktreeProvisioner,
    C: crate::Clock,
{
    /// Registers a newly asked question as a decision. A create that fails
    /// (no transport configured, or the call itself errors) is answered
    /// immediately as the question's deny option rather than left pending —
    /// there is nothing to poll.
    pub(super) async fn open_decision(
        &self,
        session: &RunnerSession,
        record: &AttemptJournal,
        state: &mut DecisionState<'_>,
        question: Question,
        prompt: &str,
    ) {
        let decision_id = decision_id_for(record, &question);
        let created = if state.known.contains(&decision_id) {
            true
        } else if let Some(data_protocol) = self.data_protocol.as_ref() {
            let report = DecisionCreateReport {
                attempt_id: record.attempt_id.clone(),
                fencing_token: record.fencing_token,
                decision_id: decision_id.clone(),
                kind: question.kind.clone(),
                prompt: scrub_with_prompt(&question.prompt, prompt),
                options: question.options.clone(),
                recommendation: question.recommendation.clone().map(|recommendation| {
                    Recommendation {
                        rationale: recommendation
                            .rationale
                            .map(|text| scrub_with_prompt(&text, prompt)),
                        ..recommendation
                    }
                }),
                expires_at: Timestamp::new(
                    state
                        .expires_at
                        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                ),
                metadata: scrub_metadata_with_prompt(question.metadata.clone(), prompt),
            };
            data_protocol.create_decision(session, report).await.is_ok()
        } else {
            false
        };
        if created {
            *state.pending = Some(PendingDecision {
                decision_id,
                question,
                after: Timestamp::new(POLL_FROM_THE_START.to_owned()),
            });
        } else {
            send_answer(state.decisions, deny_answer(&question)).await;
        }
    }

    /// Polls a pending decision once. Resolved answers it as given; a state
    /// this runner does not recognize as still-pending, or the attempt's own
    /// deadline passing, answers it as the question's deny option. A
    /// transport failure leaves it pending for the next tick — the deadline
    /// still bounds how long that can go on.
    pub(super) async fn advance_pending(
        &self,
        session: &RunnerSession,
        record: &AttemptJournal,
        state: &mut DecisionState<'_>,
    ) {
        let Some(open) = state.pending.as_mut() else {
            return;
        };
        if let Some(data_protocol) = self.data_protocol.as_ref() {
            let poll = DecisionPollReport {
                attempt_id: record.attempt_id.clone(),
                fencing_token: record.fencing_token,
                after: open.after.clone(),
            };
            match data_protocol.poll_decisions(session, poll).await {
                Err(error) => tracing::warn!(
                    %error,
                    attempt_id = record.attempt_id.as_str(),
                    "decision poll failed; the question stays pending until the next poll"
                ),
                Ok(response) => {
                    if let Some(next_after) = response.next_after.clone() {
                        open.after = next_after;
                    }
                    let resolved = response
                        .decisions
                        .iter()
                        .find(|decision| decision.decision_id == open.decision_id)
                        .cloned();
                    if let Some(resolved) = resolved {
                        if resolved.state == "resolved" {
                            let opened = state.pending.take().expect("just matched Some above");
                            // Whatever the board relays was given by someone: only the runner's
                            // own `deny_answer` stands for nobody.
                            let answer = resolved.answer.map_or_else(
                                || deny_answer(&opened.question),
                                |given| DecisionAnswer {
                                    unanswered: false,
                                    ..given
                                },
                            );
                            send_answer(state.decisions, answer).await;
                            return;
                        }
                        if resolved.state != "pending" {
                            let opened = state.pending.take().expect("just matched Some above");
                            send_answer(state.decisions, deny_answer(&opened.question)).await;
                            return;
                        }
                    }
                }
            }
        }
        if chrono::DateTime::<chrono::Utc>::from(self.clock.now()) >= state.expires_at {
            let opened = state.pending.take().expect("just matched Some above");
            send_answer(state.decisions, deny_answer(&opened.question)).await;
        }
    }

    /// The harness says the pending question was answered in its own tool. The board's decision
    /// for it is closed as answered elsewhere and the question stops being waited for, so the
    /// next one can be asked. A report about any other question is ignored. When the board
    /// cannot be told (a transport failure, a board without the operation) the question is still
    /// dropped: the run goes on, and the board's decision is closed when the attempt ends.
    pub(super) async fn close_answered_elsewhere(
        &self,
        session: &RunnerSession,
        record: &AttemptJournal,
        state: &mut DecisionState<'_>,
        told: AnsweredElsewhere,
    ) {
        let Some(open) = state
            .pending
            .take_if(|open| open.question.vendor_id == told.vendor_id)
        else {
            return;
        };
        let Some(data_protocol) = self.data_protocol.as_ref() else {
            return;
        };
        let report = DecisionAnsweredElsewhereReport {
            attempt_id: record.attempt_id.clone(),
            fencing_token: record.fencing_token,
            decision_id: open.decision_id,
            source: told.source,
        };
        if let Err(error) = data_protocol.answered_elsewhere(session, report).await {
            tracing::warn!(
                %error,
                attempt_id = record.attempt_id.as_str(),
                "the board could not be told a question was answered elsewhere; \
                 its decision stays open until the attempt ends"
            );
        }
    }
}
