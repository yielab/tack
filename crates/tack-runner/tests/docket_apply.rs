//! Applying a repository's team through docket (`RunnerOperation::ApplyTeam`), against the
//! in-process fake docket: the pod is made from its shape, then the team's folder is applied, and
//! a folder that would run commands waits for a person's code.

use std::path::Path;

use serde_json::json;
use tack_protocol::execution::OperationOutcome;
use tack_runner::{
    config::DocketConfig,
    harness::{HarnessProbe, docket::DocketAdapter},
};

use crate::common;
use crate::fake_docket::FakeDocket;

const TEAM: &str = "fab-support";

fn adapter(fake: &FakeDocket) -> (DocketAdapter, tempfile::TempDir) {
    let state = common::temp_dir("docket-apply-state");
    let adapter = DocketAdapter::new(
        DocketConfig {
            base_url: Some(fake.base_url.clone()),
            token_file: Some(fake.token_file()),
            ..DocketConfig::default()
        },
        state.path().to_path_buf(),
    );
    (adapter, state)
}

/// A checkout holding the team's folder.
fn checkout() -> tempfile::TempDir {
    let dir = common::temp_dir("docket-apply-checkout");
    std::fs::create_dir_all(dir.path().join(".docket/teams").join(TEAM)).expect("team folder");
    dir
}

async fn apply(
    adapter: &DocketAdapter,
    checkout: &Path,
    shape: &str,
    accept: Option<&str>,
) -> OperationOutcome {
    adapter
        .apply_team(TEAM, shape, &checkout.display().to_string(), accept)
        .await
}

async fn shaped() -> FakeDocket {
    let fake = FakeDocket::start().await;
    fake.with_pod_shape();
    fake
}

#[tokio::test]
async fn the_pod_is_made_from_its_shape_and_then_the_teams_folder_is_applied() {
    let fake = shaped().await;
    let (adapter, _state) = adapter(&fake);
    let checkout = checkout();

    let outcome = apply(&adapter, checkout.path(), "respond", None).await;

    assert!(
        matches!(&outcome, OperationOutcome::Done { team: None }),
        "{outcome:?}"
    );
    let posts: Vec<_> = fake
        .requests()
        .into_iter()
        .filter(|request| request.method == "POST")
        .collect();
    assert_eq!(posts.len(), 2, "the pod, then the apply: {posts:?}");
    assert_eq!(posts[0].path, "/pods");
    assert_eq!(
        posts[0].body,
        json!({
            "name": TEAM,
            "blueprint": "respond",
            "location": checkout.path().display().to_string(),
            "ensure": true,
        })
    );
    assert_eq!(posts[1].path, format!("/pods/{TEAM}/apply"));
    assert_eq!(
        posts[1].body,
        json!({"directory": checkout.path().join(".docket/teams").join(TEAM).display().to_string()}),
        "the team's own folder, and no code before docket asked for one"
    );
}

#[tokio::test]
async fn a_folder_that_runs_commands_waits_for_a_person_and_the_answer_is_sent_as_accept() {
    let fake = shaped().await;
    fake.require_apply_review(
        "3f9a1c7e2b40",
        &[("allow-commands", "allowCommands", "pytest, ruff")],
    );
    let (adapter, _state) = adapter(&fake);
    let checkout = checkout();

    let first = apply(&adapter, checkout.path(), "respond", None).await;
    let OperationOutcome::NeedsReview { code, commands } = first else {
        panic!("a review was expected, got {first:?}");
    };
    assert_eq!(code, "3f9a1c7e2b40");
    assert_eq!(commands, ["allow-commands allowCommands: pytest, ruff"]);
    assert_eq!(
        fake.requests_to("POST", &format!("/pods/{TEAM}/apply"))
            .len(),
        1,
        "docket is asked once until a person answers"
    );

    let second = apply(&adapter, checkout.path(), "respond", Some(&code)).await;
    assert!(
        matches!(second, OperationOutcome::Done { .. }),
        "{second:?}"
    );
    let applies = fake.requests_to("POST", &format!("/pods/{TEAM}/apply"));
    assert_eq!(applies.len(), 2);
    assert_eq!(applies[1].body["accept"], json!(code));
}

#[tokio::test]
async fn a_code_for_commands_that_changed_since_is_refused() {
    let fake = shaped().await;
    fake.require_apply_review("aaaa", &[("network", "network", "open")]);
    let (adapter, _state) = adapter(&fake);
    let checkout = checkout();

    let failed = apply(&adapter, checkout.path(), "build", Some("bbbb")).await;
    assert!(
        matches!(&failed, OperationOutcome::Refused { reason } if reason.contains("changed what runs")),
        "{failed:?}"
    );
}

#[tokio::test]
async fn a_team_of_a_folder_shape_applies_although_docket_records_no_codebase_for_it() {
    let fake = shaped().await;
    let (adapter, _state) = adapter(&fake);
    let checkout = checkout();

    let outcome = apply(&adapter, checkout.path(), "operate", None).await;

    assert!(
        matches!(outcome, OperationOutcome::Done { .. }),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn a_docket_without_podshape_is_not_asked_to_make_anything() {
    let fake = FakeDocket::start().await;
    let (adapter, _state) = adapter(&fake);
    let checkout = checkout();

    let outcome = apply(&adapter, checkout.path(), "respond", None).await;

    assert!(
        matches!(&outcome, OperationOutcome::Refused { reason } if reason.contains("podShape")),
        "{outcome:?}"
    );
    assert!(fake.requests_to("POST", "/pods").is_empty());
}

#[tokio::test]
async fn a_checkout_without_the_teams_folder_fails_before_docket_is_asked() {
    let fake = shaped().await;
    let (adapter, _state) = adapter(&fake);
    let empty = common::temp_dir("docket-apply-empty");

    let outcome = apply(&adapter, empty.path(), "respond", None).await;

    assert!(
        matches!(&outcome, OperationOutcome::Refused { reason } if reason.contains(".docket/teams/fab-support")),
        "{outcome:?}"
    );
    assert!(fake.requests_to("POST", "/pods").is_empty());
}

#[tokio::test]
async fn the_probe_lists_podshape_only_for_a_docket_that_lists_it() {
    let listed = shaped().await;
    assert!(
        adapter(&listed)
            .0
            .probe()
            .await
            .lists_api_feature("podShape")
    );
    let older = FakeDocket::start().await;
    assert!(
        !adapter(&older)
            .0
            .probe()
            .await
            .lists_api_feature("podShape")
    );
}
