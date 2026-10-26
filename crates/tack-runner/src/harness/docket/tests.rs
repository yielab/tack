use serde_json::json;

use super::escalation_question;

fn option(id: &str) -> serde_json::Value {
    json!({"id": id, "label": id, "description": "", "risks": [], "estimatedTokens": null})
}

#[test]
fn a_change_to_its_own_check_is_asked_in_dockets_words_and_answered_with_a_reason() {
    let record = json!({
        "id": "esc-1", "kind": "check_changed", "state": "pending", "role": "implementer",
        "prompt": "The change edits what checks it: package.json. Accept it after reading it?",
        "options": [option("approve_once"), option("deny")],
        "recommendation": null, "requestedSchema": null, "expiresAt": null,
        "affordances": {"options": ["approve_once", "deny"], "acceptsText": false},
    });
    let (question, pending) = escalation_question(&record).expect("a question");
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
        "id": "esc-2", "kind": "input", "state": "pending", "prompt": "Which one?",
        "options": [option("decline")], "recommendation": null,
        "requestedSchema": {"type": "object", "properties": {"answer": {"type": "string"}},
                            "required": ["answer"]},
        "affordances": {"options": ["decline"], "acceptsText": true},
    });
    let (question, pending) = escalation_question(&record).expect("a question");
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
