use serde_json::json;

use super::held_approval_question;

#[test]
fn a_change_to_its_own_check_asks_about_the_files_it_edits() {
    let task = json!({
        "status": "waiting_approval",
        "approvalToken": "apr-9",
        "checkChanged": ["package.json", "eslint.config.mjs"],
        "reason": "the change edits what checks it: package.json, eslint.config.mjs (token=apr-9)",
    });
    let question = held_approval_question(&task, "apr-9", None);
    assert_eq!(question.vendor_id, "apr-9");
    assert_eq!(
        question.prompt,
        "The change edits what checks it: package.json, eslint.config.mjs. \
         Accept it after reading those files?"
    );
    let ids: Vec<_> = question
        .options
        .iter()
        .map(|o| o.option_id.as_str())
        .collect();
    assert!(ids.contains(&"deny"), "{ids:?}");
}

#[test]
fn a_held_approval_without_changed_files_still_asks_about_the_tool() {
    let task = json!({"status": "waiting_approval", "approvalToken": "apr-1", "checkChanged": []});
    let record = json!({"context": {"tool": "bash"}, "action": "ls"});
    let question = held_approval_question(&task, "apr-1", Some(&record));
    assert_eq!(question.prompt, "Allow bash? docket's policy: ls");
}
