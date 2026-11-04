//! The rule behind the hourly due-soon pass: an incomplete item due within the next hour is
//! opened in the inbox, once for the day it falls due; one due later, already past, or without a
//! date is not.

use std::sync::Arc;

use chrono::{DateTime, Duration, TimeZone, Utc};
use tack_app::due_soon;
use tack_app::notifications::Inbox;
use tack_core::models::CreateItem;
use tack_core::notification::{Kind, Level};
use tack_db::Repository;
use tack_protocol::Clock;
use tack_test_support::{ControllableClock, create_test_workspace, make_project, setup_test_db};

struct FakeClock(ControllableClock);

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        self.0.now()
    }
}

fn inbox(repo: &Repository, now: DateTime<Utc>) -> Inbox {
    Inbox::new(
        repo.clone(),
        Arc::new(tack_test_support::FixedClock(now)),
        Vec::new(),
    )
}

async fn item_due(
    repo: &Repository,
    project: &tack_core::models::Project,
    title: &str,
    due: Option<DateTime<Utc>>,
) {
    let status = project.workflow.initial_status().unwrap().to_string();
    repo.create_item(
        project.id,
        &status,
        CreateItem {
            recurrence: None,
            recurrence_of: None,
            title: title.into(),
            description: None,
            item_type: None,
            parent_id: None,
            priority: None,
            estimate: None,
            estimate_unit: None,
            tags: None,
            due_date: due,
            sprint_id: None,
            source_artifact_id: None,
            assignee: None,
        },
    )
    .await
    .expect("create item");
}

#[tokio::test]
async fn announces_only_the_items_due_within_the_next_hour() {
    let repo = setup_test_db().await;
    let now = Utc.with_ymd_and_hms(2026, 8, 12, 12, 0, 0).unwrap();
    let clock = FakeClock(ControllableClock::new(now));
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await;
    item_due(&repo, &project, "soon", Some(now + Duration::minutes(30))).await;
    item_due(&repo, &project, "later", Some(now + Duration::hours(2))).await;
    item_due(&repo, &project, "past", Some(now - Duration::minutes(5))).await;
    item_due(&repo, &project, "undated", None).await;

    let inbox = inbox(&repo, now);
    let announced = due_soon::tick(&repo, &clock, &inbox).await.expect("tick");

    assert_eq!(announced, 1);
    let open = inbox.open_notifications(10).await.unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].kind, Kind::DueSoon);
    assert_eq!(open[0].level, Level::Info);
    assert_eq!(open[0].project_id, project.id);
    assert!(open[0].title.contains("soon"));

    // The same pass again, or the next one within the day, announces nothing more.
    assert_eq!(due_soon::tick(&repo, &clock, &inbox).await.unwrap(), 0);
    assert_eq!(inbox.open_count().await.unwrap(), 1);
}

#[tokio::test]
async fn a_later_pass_announces_what_has_come_into_the_window() {
    let repo = setup_test_db().await;
    let now = Utc.with_ymd_and_hms(2026, 8, 12, 12, 0, 0).unwrap();
    let clock = FakeClock(ControllableClock::new(now));
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await;
    item_due(&repo, &project, "later", Some(now + Duration::hours(2))).await;

    let inbox = inbox(&repo, now);
    assert_eq!(due_soon::tick(&repo, &clock, &inbox).await.unwrap(), 0);
    clock.0.advance(Duration::minutes(90));
    assert_eq!(due_soon::tick(&repo, &clock, &inbox).await.unwrap(), 1);
}
