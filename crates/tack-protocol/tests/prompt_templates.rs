//! The fingerprint of the prompt templates changes with any line of any template.

use tack_protocol::execution::{
    PROMPT_TEMPLATE_SHA, PROMPT_TEMPLATES, TaskDocument, template_fingerprint,
};

/// The value recorded for the templates as they are. Changing a template fails the first test
/// until this file holds the new value: the change to what an agent reads is then in the diff.
const RECORDED: &str = include_str!("fixtures/prompt_template_sha.txt");

#[test]
fn template_fingerprint_matches_the_recorded_value() {
    assert_eq!(
        PROMPT_TEMPLATE_SHA.as_str(),
        RECORDED.trim(),
        "a prompt template changed: record the new fingerprint in tests/fixtures/prompt_template_sha.txt"
    );
}

#[test]
fn template_fingerprint_changes_when_one_line_of_a_template_changes() {
    for (index, template) in PROMPT_TEMPLATES.iter().enumerate() {
        let edited = template.replacen('\n', " \n", 1);
        let mut templates = PROMPT_TEMPLATES;
        templates[index] = &edited;
        assert_ne!(
            template_fingerprint(&templates),
            PROMPT_TEMPLATE_SHA.as_str(),
            "template {index} is not part of the fingerprint"
        );
    }
}

#[test]
fn template_fingerprint_tells_where_one_template_ends() {
    assert_ne!(
        template_fingerprint(&["ab", "c"]),
        template_fingerprint(&["a", "bc"])
    );
}

#[test]
fn compose_renders_only_the_fingerprinted_words() {
    let document = TaskDocument {
        instructions: String::new(),
        title: "T".into(),
        description: Some("D".into()),
        comments: vec!["C".into()],
        comments_left_out: 2,
        brief: None,
        definition_of_done: None,
        kind: "task".into(),
        source: "web".into(),
    };
    let rendered = document.item_section("abcd1234");
    for label in [
        "## The item",
        "Description:",
        "Comment:",
        "<<<card-text id=abcd1234>>>",
        "2 earlier comments are not included.",
    ] {
        assert!(rendered.contains(label), "{label} missing from {rendered}");
    }
}
