use std::sync::Arc;

use chrono::Utc;
use tack_core::notification::{Key, Kind, Level};
use tack_test_support::{FixedClock, create_test_workspace, make_project, setup_test_db};
use tokio::sync::broadcast::error::TryRecvError;
use uuid::Uuid;

use super::*;

fn question(project: Uuid, id: &str) -> Notice {
    Notice {
        kind: Kind::Decision,
        level: Level::NeedsYou,
        key: Key::decision(id),
        project_id: project,
        item_id: None,
        request_id: None,
        title: "A run is asking a question".into(),
        cause: "runner:local".into(),
    }
}

async fn inbox() -> (Inbox, Uuid) {
    let repo = setup_test_db().await;
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await.id;
    let inbox = Inbox::new(
        repo,
        Arc::new(FixedClock(Utc::now())),
        vec!["webhook".into()],
    );
    (inbox, project)
}

#[tokio::test]
async fn opening_a_situation_twice_writes_and_tells_once() {
    let (inbox, project) = inbox().await;
    let mut listener = inbox.subscribe();

    let first = inbox.open(&question(project, "d-1")).await.unwrap();
    let again = inbox.open(&question(project, "d-1")).await.unwrap();

    let opened = first.expect("the first call opens it");
    assert!(again.is_none(), "the second finds it open");
    assert_eq!(listener.try_recv(), Ok(Change::Opened(opened.clone())));
    assert_eq!(listener.try_recv(), Err(TryRecvError::Empty));
    assert_eq!(inbox.open_count().await.unwrap(), 1);
    assert_eq!(inbox.open_notifications(10).await.unwrap(), vec![opened]);
}

#[tokio::test]
async fn closing_tells_the_notification_that_closed_and_only_when_one_was_open() {
    let (inbox, project) = inbox().await;
    let opened = inbox
        .open(&question(project, "d-1"))
        .await
        .unwrap()
        .unwrap();
    let mut listener = inbox.subscribe();

    assert!(inbox.close(&Key::decision("d-1")).await.unwrap());
    assert!(!inbox.close(&Key::decision("d-1")).await.unwrap());
    assert!(!inbox.close(&Key::decision("never")).await.unwrap());

    assert_eq!(
        listener.try_recv(),
        Ok(Change::Closed {
            id: opened.id,
            key: Key::decision("d-1")
        })
    );
    assert_eq!(listener.try_recv(), Err(TryRecvError::Empty));
    assert_eq!(inbox.open_count().await.unwrap(), 0);
}

#[tokio::test]
async fn reading_tells_each_notification_read_once() {
    let (inbox, project) = inbox().await;
    let one = inbox
        .open(&question(project, "d-1"))
        .await
        .unwrap()
        .unwrap();
    let two = inbox
        .open(&question(project, "d-2"))
        .await
        .unwrap()
        .unwrap();
    let mut listener = inbox.subscribe();

    assert!(inbox.read(one.id).await.unwrap());
    assert!(!inbox.read(one.id).await.unwrap());
    assert_eq!(inbox.read_all().await.unwrap(), 1);
    assert_eq!(inbox.read_all().await.unwrap(), 0);

    assert_eq!(listener.try_recv(), Ok(Change::Read { id: one.id }));
    assert_eq!(listener.try_recv(), Ok(Change::Read { id: two.id }));
    assert_eq!(listener.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn a_new_notification_is_queued_for_each_channel() {
    let (inbox, project) = inbox().await;
    let opened = inbox
        .open(&question(project, "d-1"))
        .await
        .unwrap()
        .unwrap();

    let due = inbox.repo.deliveries_due(Utc::now(), 10).await.unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].notification.id, opened.id);
    assert_eq!(due[0].channel, "webhook");
}

#[test]
fn a_change_is_sent_with_its_type() {
    let closed = Change::Closed {
        id: Uuid::nil(),
        key: Key::decision("d-1"),
    };
    assert_eq!(
        serde_json::to_value(&closed).unwrap(),
        serde_json::json!({
            "type": "closed",
            "id": "00000000-0000-0000-0000-000000000000",
            "key": "decision:d-1"
        })
    );
    let read = serde_json::to_value(Change::Read { id: Uuid::nil() }).unwrap();
    assert_eq!(read["type"], "read");
}
