//! The rule behind the hourly `item.due_soon` webhook: an incomplete item due within the next
//! hour is announced once per pass; one due later, already past, or without a date is not.

use std::sync::Mutex;

use chrono::{DateTime, Duration, TimeZone, Utc};
use tack_app::due_soon::{self, DueSoonSink};
use tack_core::models::CreateItem;
use tack_db::Repository;
use tack_protocol::Clock;
use tack_test_support::{ControllableClock, create_test_workspace, make_project, setup_test_db};

struct FakeClock(ControllableClock);

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        self.0.now()
    }
}

#[derive(Default)]
struct Recorded(Mutex<Vec<(String, serde_json::Value)>>);

impl DueSoonSink for Recorded {
    fn fire(&self, event: &str, payload: serde_json::Value) {
        self.0.lock().unwrap().push((event.to_owned(), payload));
    }
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

    let sink = Recorded::default();
    let announced = due_soon::tick(&repo, &clock, &sink).await.expect("tick");

    assert_eq!(announced, 1);
    let fired = sink.0.lock().unwrap();
    assert_eq!(fired.len(), 1);
    let (event, payload) = &fired[0];
    assert_eq!(event, "item.due_soon");
    assert_eq!(payload["event"], "item.due_soon");
    assert_eq!(payload["timestamp"], now.to_rfc3339());
    assert_eq!(payload["project_id"], project.id.to_string());
    assert_eq!(payload["item"]["title"], "soon");
}

#[tokio::test]
async fn a_later_pass_announces_what_has_come_into_the_window() {
    let repo = setup_test_db().await;
    let now = Utc.with_ymd_and_hms(2026, 8, 12, 12, 0, 0).unwrap();
    let clock = FakeClock(ControllableClock::new(now));
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await;
    item_due(&repo, &project, "later", Some(now + Duration::hours(2))).await;

    let sink = Recorded::default();
    assert_eq!(due_soon::tick(&repo, &clock, &sink).await.unwrap(), 0);
    clock.0.advance(Duration::minutes(90));
    assert_eq!(due_soon::tick(&repo, &clock, &sink).await.unwrap(), 1);
}
