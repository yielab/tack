use chrono::{Local, TimeZone, Utc};
use serde_json::json;

use super::{
    DocketAdapter, OptionLabels, RunInfo, escalation_progress, escalation_question, wait_line,
    wire::{Escalation, TaskView, TraceEvent},
};
use crate::config::DocketConfig;

fn escalation(record: serde_json::Value) -> Escalation {
    serde_json::from_value(record).expect("an escalation as docket sends it")
}

fn option(id: &str) -> serde_json::Value {
    json!({"id": id, "label": id, "description": "", "risks": [], "estimatedTokens": null})
}

#[test]
fn a_change_to_its_own_check_is_asked_in_dockets_words_and_answered_with_a_reason() {
    let record = json!({
        "id": "esc-1", "kind": "check_changed", "state": "pending", "seq": 1, "role": "implementer",
        "prompt": "The change edits what checks it: package.json. Accept it after reading it?",
        "options": [option("approve_once"), option("deny")],
        "recommendation": null, "requestedSchema": null, "expiresAt": null,
        "affordances": {"options": ["approve_once", "deny"], "acceptsText": false},
    });
    let (question, pending) = escalation_question(&escalation(record), None).expect("a question");
    assert_eq!(question.vendor_id, "esc-1");
    assert_eq!(question.kind, "check_changed");
    assert_eq!(
        question.prompt,
        "The change edits what checks it: package.json. Accept it after reading it?"
    );
    let ids: Vec<_> = question
        .options
        .iter()
        .map(|o| o.option_id.as_str())
        .collect();
    assert_eq!(ids, ["approve_once", "deny"]);
    assert_eq!(pending.property, "reason");
    assert!(!pending.required && !pending.by_text);
}

#[test]
fn a_question_that_only_declines_but_takes_text_is_offered_an_answer_first() {
    let record = json!({
        "id": "esc-2", "kind": "input", "state": "pending", "seq": 2, "prompt": "Which one?",
        "options": [option("decline")], "recommendation": null,
        "requestedSchema": {"type": "object", "properties": {"answer": {"type": "string"}},
                            "required": ["answer"]},
        "affordances": {"options": ["decline"], "acceptsText": true},
    });
    let (question, pending) = escalation_question(&escalation(record), None).expect("a question");
    let ids: Vec<_> = question
        .options
        .iter()
        .map(|o| o.option_id.as_str())
        .collect();
    assert_eq!(ids, ["accept", "decline"]);
    assert!(pending.by_text && pending.required);
    assert_eq!(pending.options, ["decline"]);
    assert_eq!(pending.property, "answer");
}

#[test]
fn a_kind_tack_does_not_know_is_not_guessed_and_is_named() {
    let record = json!({
        "id": "esc-3", "kind": "teleport", "state": "pending", "seq": 3, "prompt": "Beam up?",
        "options": [option("approve_once"), option("deny")], "recommendation": null,
        "requestedSchema": null, "expiresAt": null,
        "affordances": {"options": ["approve_once", "deny"], "acceptsText": false},
    });
    let Err(sentence) = escalation_question(&escalation(record), None) else {
        panic!("a kind Tack does not know is not turned into an approval");
    };
    assert!(sentence.contains("\"teleport\""), "{sentence}");
    assert!(sentence.contains("answer it in docket"), "{sentence}");
}

fn event(line: serde_json::Value) -> TraceEvent {
    TraceEvent::parse(&line.to_string()).expect("a trace event as docket writes it")
}

#[test]
fn a_resolved_escalation_names_the_label_the_person_picked_and_no_id() {
    let mut labels = OptionLabels::new();
    labels.insert(
        ("esc-1".to_owned(), "approve_once".to_owned()),
        "Approve once".to_owned(),
    );
    let resolved = event(json!({
        "event_type": "escalation_resolved",
        "payload": {"task": "t", "escalation": "esc-1", "optionId": "approve_once"},
    }));
    let line = escalation_progress(&resolved, &labels).expect("a line");
    assert_eq!(line.text, "a person answered: Approve once");

    // An option whose label Tack never read is not worded by its id.
    let unread = event(json!({
        "event_type": "escalation_resolved",
        "payload": {"task": "t", "escalation": "esc-9", "optionId": "approve_task"},
    }));
    let line = escalation_progress(&unread, &labels).expect("a line");
    assert_eq!(line.text, "a person answered");
}

#[test]
fn an_escalation_docket_decided_alone_says_so_without_its_policy_or_level() {
    let decided = event(json!({
        "event_type": "escalation_decided",
        "payload": {"task": "t", "escalation": "esc-1", "optionId": "deny",
                    "policy": "deny", "level": "L5", "by": "policy"},
    }));
    let line = escalation_progress(&decided, &OptionLabels::new()).expect("a line");
    assert_eq!(line.text, "docket decided this on its own");
}

#[test]
fn an_event_type_tack_does_not_follow_is_not_a_line() {
    let other = event(json!({"event_type": "llm_call", "payload": {"task": "t"}}));
    assert!(escalation_progress(&other, &OptionLabels::new()).is_none());
    assert_eq!(other.task(), None);
}

/// A moment on the person's own clock, so the lines below read the same in any time zone.
fn local(day: u32, hour: u32, minute: u32) -> chrono::DateTime<Utc> {
    Local
        .with_ymd_and_hms(2026, 10, day, hour, minute, 0)
        .single()
        .expect("a local time")
        .with_timezone(&Utc)
}

#[test]
fn a_wait_is_one_line_with_its_time_and_asks_nobody() {
    let today = wait_line(Some(local(30, 14, 0)), local(30, 9, 5));
    assert_eq!(today, "The team waits until 14:00. Nobody needs to answer.");
    let later = wait_line(Some(local(31, 8, 30)), local(30, 9, 5));
    assert_eq!(
        later,
        "The team waits until 2026-10-31 08:30. Nobody needs to answer."
    );
    // A wait with no due time (a probe) still says nobody is asked.
    assert_eq!(
        wait_line(None, local(30, 9, 5)),
        "The team waits. Nobody needs to answer."
    );
    // Not a question: the kind is not one Tack puts to a person.
    let record = json!({
        "id": "esc-w", "kind": "wait", "state": "pending", "seq": 4, "policy": "machine",
        "prompt": "waiting", "options": [option("resume")], "recommendation": null,
        "requestedSchema": null, "expiresAt": "2026-10-30T14:00:00Z",
        "affordances": {"options": ["resume"], "acceptsText": false},
    });
    assert!(escalation_question(&escalation(record), None).is_err());
}

fn outward_approval() -> serde_json::Value {
    json!({
        "id": "esc-4", "kind": "approval", "state": "pending", "seq": 4, "role": "actor",
        "outward": true, "policy": "person",
        "prompt": "send: an email to a customer (the call is cut here)",
        "options": [option("approve_once"), option("deny")],
        "recommendation": null, "requestedSchema": null, "expiresAt": null,
        "affordances": {"options": ["approve_once", "deny"], "acceptsText": false},
    })
}

#[test]
fn an_outward_approval_with_its_call_carries_the_call_and_the_mark() {
    let call = "send_email to=ana@example.com\nsubject: Your order\n\nHello, Ana.";
    let (question, _) =
        escalation_question(&escalation(outward_approval()), Some(call)).expect("a question");
    assert_eq!(question.metadata["call"], json!(call));
    assert_eq!(question.metadata["outward"], json!(true));
    // The prompt stays docket's redacted, cut one: it is what the timeline line is made of.
    assert_eq!(
        question.prompt,
        "send: an email to a customer (the call is cut here)"
    );
    assert!(!question.prompt.contains("Hello, Ana"));
}

#[test]
fn an_outward_approval_without_its_call_is_not_asked() {
    for call in [None, Some(""), Some("  \n")] {
        let Err(sentence) = escalation_question(&escalation(outward_approval()), call) else {
            panic!("an approval whose call is not shown is not put to a person");
        };
        assert_eq!(
            sentence,
            "docket did not show the call that would leave; answer it in docket, where it is shown"
        );
    }
    // An ordinary approval needs no call and carries none.
    let mut ordinary = outward_approval();
    ordinary["outward"] = json!(false);
    let (question, _) = escalation_question(&escalation(ordinary), Some("ignored")).expect("asked");
    assert!(question.metadata.get("call").is_none());
    assert!(question.metadata.get("outward").is_none());
}

/// How an attempt ends when docket's task has ended with `failure_kind`, given what Tack asked
/// the person: the question, when, and whether an answer went back to docket.
async fn ended(
    failure_kind: &str,
    reason: &str,
    asked: Option<(&str, i64, bool)>,
) -> serde_json::Value {
    let config = DocketConfig {
        // Nothing listens here: the records read after the end finds no docket and moves on.
        base_url: Some("http://127.0.0.1:1".to_owned()),
        ..DocketConfig::default()
    };
    let state = tempfile::tempdir().expect("a state directory");
    let adapter = DocketAdapter::new(config, state.path().to_path_buf());
    let info = RunInfo {
        pod: "my-saas".to_owned(),
        task: "task-1".to_owned(),
        attempt_id: "attempt-1".to_owned(),
        workspace: state.path().to_path_buf(),
        started_at: Utc::now(),
        timeout: std::time::Duration::from_secs(3600),
        refused: None,
        api: Some(6),
        in_repository: true,
        turn: false,
    };
    let task: TaskView = serde_json::from_value(json!({
        "status": "failed", "failureKind": failure_kind, "reason": reason, "hops": [],
    }))
    .expect("a task as docket sends it");
    let asked = asked.map(|(prompt, minutes_ago, answered)| {
        (
            prompt.to_owned(),
            Utc::now() - chrono::Duration::minutes(minutes_ago),
            answered,
        )
    });
    let (progress, _lines) = tokio::sync::mpsc::channel(4);
    adapter
        .finish(&info, &task, None, &progress, None, asked.as_ref())
        .await
        .terminal_reason
}

#[tokio::test]
async fn an_approval_that_expired_unanswered_ends_not_answered_with_its_question() {
    let reason = ended(
        "approval_denied",
        "approval denied",
        Some(("Push the migration to production?", 25, false)),
    )
    .await;
    assert_eq!(reason["code"], json!("not_answered"));
    let message = reason["message"].as_str().expect("a message");
    assert!(
        message.starts_with("Nobody answered 'Push the migration to production?' (asked "),
        "{message}"
    );
    assert!(
        message.ends_with("); the team stopped after 25 minutes."),
        "{message}"
    );
    assert!(!message.to_lowercase().contains("denied"), "{message}");
}

#[tokio::test]
async fn an_approval_docket_expired_ends_not_answered_with_its_text_after_the_question() {
    let expired = "approval expired: nobody answered by 2026-10-31T10:00:00Z";
    let with_question = ended(
        "approval_expired",
        expired,
        Some(("Push the migration to production?", 25, false)),
    )
    .await;
    assert_eq!(with_question["code"], json!("not_answered"));
    let message = with_question["message"].as_str().expect("a message");
    assert!(
        message.starts_with("Nobody answered 'Push the migration to production?' (asked "),
        "{message}"
    );
    assert!(
        message.ends_with(&format!("25 minutes. docket said: {expired}")),
        "{message}"
    );
    let without = ended("approval_expired", expired, None).await;
    assert_eq!(without["code"], json!("not_answered"));
    assert_eq!(
        without["message"],
        json!(format!(
            "Nobody answered the question before it expired. docket said: {expired}"
        ))
    );
}

#[tokio::test]
async fn an_approval_a_person_denied_ends_approval_denied() {
    let reason = ended(
        "approval_denied",
        "approval denied: not today",
        Some(("Push the migration to production?", 2, true)),
    )
    .await;
    assert_eq!(reason["code"], json!("approval_denied"));
    assert_eq!(reason["message"], json!("approval denied: not today"));
}

#[tokio::test]
async fn a_failure_code_tack_does_not_translate_shows_dockets_text_labelled() {
    let reason = ended("quota_exceeded", "the day's tokens are spent", None).await;
    assert_eq!(reason["code"], json!("quota_exceeded"));
    assert_eq!(
        reason["message"],
        json!("docket said: the day's tokens are spent")
    );
}

/// A docket Planner or Researcher is told to propose through its team's `tack_proposals` records
/// and to write no file; any other profile gets the instructions as they are. The local agents get
/// the file sentence instead (`harness::tests`).
#[test]
fn a_docket_planner_or_researcher_is_told_to_propose_through_the_records_and_write_no_file() {
    let state = crate::harness::test_support::scratch("docket-plan-channel");
    let description = |kind: Option<&str>, with_task: bool| {
        let mut spec = crate::harness::test_support::spec("docket", state.path());
        let request = &mut spec.work.request;
        request.resolved_agent_profile.instructions = "Do the thing.".into();
        if let Some(kind) = kind {
            request
                .resolved_agent_profile
                .additional
                .insert("kind".into(), kind.into());
        }
        request.task = with_task.then(|| tack_protocol::execution::TaskDocument {
            instructions: "Do the thing.".into(),
            title: "A card".into(),
            description: None,
            comments: Vec::new(),
            comments_left_out: 0,
            brief: None,
            definition_of_done: None,
            kind: "task".into(),
            source: "human".into(),
        });
        let body = super::task::body(request, "attempt", 1, Some("main"), None);
        body["description"]
            .as_str()
            .expect("description")
            .to_owned()
    };
    for kind in ["planner", "researcher"] {
        for with_task in [true, false] {
            let text = description(Some(kind), with_task);
            assert!(
                text.starts_with("Do the thing.\n\nPropose items by calling rec__tack_proposals__add once per item,"),
                "{kind} {with_task}: {text}"
            );
            assert!(
                text.contains("Do not write a tack-plan.json file."),
                "{kind} {with_task}: {text}"
            );
            assert!(!text.contains("at the root of your work tree"), "{text}");
        }
    }
    for kind in [Some("implementer"), Some("reviewer"), None] {
        let text = description(kind, true);
        assert!(
            !text.contains("rec__tack_proposals__add"),
            "{kind:?}: {text}"
        );
    }
}

#[test]
fn a_409_to_a_cancel_is_a_finished_task_unless_docket_says_the_run_refused() {
    use tack_protocol::execution::CancelObservation::{AlreadyTerminal, Ambiguous, ProcessStopped};
    let ended = json!({"ok": false, "error": "task t is done; nothing to cancel", "code": "already_terminal"});
    let refused = json!({"ok": false, "error": "the run did not stop", "code": "not_cancellable"});
    let older = json!({"ok": false, "error": "task t is done; nothing to cancel"});
    assert_eq!(
        super::cancel_observation(200, &json!({"ok": true})),
        ProcessStopped
    );
    assert_eq!(super::cancel_observation(409, &ended), AlreadyTerminal);
    assert_eq!(super::cancel_observation(409, &older), AlreadyTerminal);
    assert_eq!(super::cancel_observation(409, &refused), Ambiguous);
    assert_eq!(super::cancel_observation(500, &ended), Ambiguous);
}

/// The input of a thread's turn asks a local agent to also write `tack-reply.md`; docket's agent
/// cannot write it and the reply is its team's last step, so the task it is given says that.
#[test]
fn a_docket_thread_turn_is_told_the_teams_last_step_answers_and_no_reply_file_is_written() {
    use tack_protocol::conversation::{REPLY_FILE, TURN_KEY, TurnNote};
    let state = crate::harness::test_support::scratch("docket-turn-input");
    let input = tack_core::thread::turn_input(&tack_core::thread::window(&[]), "{}", REPLY_FILE);
    assert!(
        input.contains(REPLY_FILE),
        "the local agent's input: {input}"
    );
    let description = |turn: bool| {
        let mut spec = crate::harness::test_support::spec("docket", state.path());
        let request = &mut spec.work.request;
        request.metadata = if turn {
            json!({TURN_KEY: TurnNote {
                id: "t-1".into(),
                turn: 1,
                max_turns: 10,
                message: "hello".into(),
            }.into_metadata()})
        } else {
            json!({})
        };
        request.task = Some(tack_protocol::execution::TaskDocument {
            instructions: "Do the thing.".into(),
            title: "A card".into(),
            description: Some(input.clone()),
            comments: Vec::new(),
            comments_left_out: 0,
            brief: None,
            definition_of_done: None,
            kind: "task".into(),
            source: "human".into(),
        });
        super::task::body(request, "attempt", 1, Some("main"), None)["description"]
            .as_str()
            .expect("description")
            .to_owned()
    };
    let turn = description(true);
    assert!(!turn.contains(REPLY_FILE), "{turn}");
    assert!(
        turn.contains("final message of your team's last step is your reply"),
        "{turn}"
    );
    assert!(turn.contains("Write no reply file."), "{turn}");
    // Nothing else of the input changes.
    assert!(turn.contains("Board digest:"), "{turn}");
    // A card that is not a turn is left as written.
    assert!(description(false).contains(REPLY_FILE));
}
