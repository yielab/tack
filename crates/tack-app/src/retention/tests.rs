use super::*;
use chrono::Utc;
use tack_test_support::FixedClock;
use tack_test_support::setup_test_db;

#[test]
fn the_default_window_is_ninety_days_in_batches_of_five_hundred() {
    let settings = Settings::default();
    assert_eq!(settings.retention_days, 90);
    assert_eq!(settings.batch_size, 500);
}

#[tokio::test]
async fn a_pass_over_an_empty_store_purges_nothing_and_does_not_fail() {
    let repo = setup_test_db().await;
    let clock = FixedClock(Utc::now());

    let outcome = tick(&repo, &clock, &Settings::default())
        .await
        .expect("an empty store is not an error");

    assert_eq!(outcome, Outcome::default());
}

#[tokio::test]
async fn a_pass_forgets_a_notification_resolved_a_month_ago_and_keeps_a_recent_one() {
    use tack_core::notification::{Key, Kind, Level, Notice};
    use tack_test_support::{create_test_workspace, make_project};

    let repo = setup_test_db().await;
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await.id;
    let now = Utc::now();
    let question = |id: &str| Notice {
        kind: Kind::Decision,
        level: Level::NeedsYou,
        key: Key::decision(id),
        project_id: project,
        item_id: None,
        request_id: None,
        title: "A run is asking a question".into(),
        cause: "runner:local".into(),
    };
    for (id, resolved) in [
        ("old", now - Duration::days(31)),
        ("new", now - Duration::days(1)),
    ] {
        repo.insert_open_notification(&question(id), &[], resolved)
            .await
            .unwrap();
        repo.close_notification_by_key(&Key::decision(id), resolved)
            .await
            .unwrap();
    }

    let outcome = tick(&repo, &FixedClock(now), &Settings::default())
        .await
        .unwrap();

    assert_eq!(outcome.notifications_forgotten, 1);
    let kept = repo
        .list_notifications(tack_db::repo::notifications::Listing::All, 10)
        .await
        .unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].key, Key::decision("new"));
}
