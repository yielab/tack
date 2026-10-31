use chrono::{Local, TimeZone, Utc};
use serde_json::json;

use super::{
    OptionLabels, escalation_progress, escalation_question, wait_line,
    wire::{Escalation, TraceEvent},
};

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
