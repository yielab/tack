//! The inbox keeps a situation open once, closes it by its key, counts what still asks something
//! of a person, hands each channel what is due, and forgets what is settled.

use chrono::{Duration, Utc};
use tack_core::notification::{Key, Kind, Level, Notice};
use tack_db::repo::notifications::Listing;
use tack_test_support::{create_test_workspace, make_project, setup_test_db};
use uuid::Uuid;

fn notice(project: Uuid, kind: Kind, level: Level, key: Key) -> Notice {
    Notice {
        kind,
        level,
        key,
        project_id: project,
        item_id: None,
        request_id: None,
        title: "A run is asking a question".into(),
        cause: "runner:local".into(),
    }
}

fn question(project: Uuid, id: &str) -> Notice {
    notice(project, Kind::Decision, Level::NeedsYou, Key::decision(id))
}

fn webhook() -> Vec<String> {
    vec!["webhook".to_owned()]
}

#[tokio::test]
async fn a_situation_is_open_once_and_again_after_it_closes() {
    let repo = setup_test_db().await;
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await.id;
    let now = Utc::now();

    let first = repo
        .insert_open_notification(&question(project, "d-1"), &[], now)
        .await
        .expect("insert")
        .expect("opened");
    assert_eq!(first.key, Key::decision("d-1"));
    assert_eq!(first.level, Level::NeedsYou);
    assert_eq!(first.kind, Kind::Decision);
    assert!(first.read_at.is_none() && first.resolved_at.is_none());
    assert!(
        repo.insert_open_notification(&question(project, "d-1"), &[], now)
            .await
            .expect("again")
            .is_none(),
        "the same situation is not opened twice"
    );
    assert_eq!(repo.count_open_notifications().await.unwrap(), 1);

    let closed = repo
        .close_notification_by_key(&Key::decision("d-1"), now)
        .await
        .expect("close");
    assert_eq!(closed, Some(first.id));
    assert_eq!(
        repo.close_notification_by_key(&Key::decision("d-1"), now)
            .await
            .unwrap(),
        None,
        "nothing is open to close the second time"
    );
    assert_eq!(repo.count_open_notifications().await.unwrap(), 0);

    let second = repo
        .insert_open_notification(&question(project, "d-1"), &[], now)
        .await
        .expect("reopen")
        .expect("the key is free once it is closed");
    assert_ne!(second.id, first.id);
    assert_eq!(
        repo.list_notifications(Listing::All, 10)
            .await
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn reading_closes_an_info_row_but_not_a_question() {
    let repo = setup_test_db().await;
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await.id;
    let now = Utc::now();
    let ask = repo
        .insert_open_notification(&question(project, "d-1"), &[], now)
        .await
        .unwrap()
        .unwrap();
    let info = repo
        .insert_open_notification(
            &notice(
                project,
                Kind::RunFinished,
                Level::Info,
                Key::finished("req-1"),
            ),
            &[],
            now + Duration::seconds(1),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(repo.count_open_notifications().await.unwrap(), 2);
    let open = repo.list_notifications(Listing::Open, 10).await.unwrap();
    assert_eq!(
        open.iter().map(|n| n.id).collect::<Vec<_>>(),
        vec![info.id, ask.id],
        "newest first"
    );

    assert!(repo.mark_notification_read(info.id, now).await.unwrap());
    assert!(
        !repo.mark_notification_read(info.id, now).await.unwrap(),
        "read once"
    );
    assert_eq!(repo.count_open_notifications().await.unwrap(), 1);

    let marked = repo.mark_all_notifications_read(now).await.unwrap();
    assert_eq!(marked, vec![ask.id], "only what was unread");
    assert_eq!(
        repo.count_open_notifications().await.unwrap(),
        1,
        "a question read is still waiting for its answer"
    );
    assert_eq!(
        repo.list_notifications(Listing::Open, 10)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        repo.list_notifications(Listing::All, 1)
            .await
            .unwrap()
            .len(),
        1
    );

    // A read info row keeps its key: the same finish is not told again.
    assert!(
        repo.insert_open_notification(
            &notice(
                project,
                Kind::RunFinished,
                Level::Info,
                Key::finished("req-1")
            ),
            &[],
            now,
        )
        .await
        .unwrap()
        .is_none()
    );
}

#[tokio::test]
async fn each_channel_is_handed_what_is_due_and_what_failed_waits_for_its_time() {
    let repo = setup_test_db().await;
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await.id;
    let now = Utc::now();
    let opened = repo
        .insert_open_notification(&question(project, "d-1"), &webhook(), now)
        .await
        .unwrap()
        .unwrap();
    // Opened while no channel is configured: nothing to deliver.
    repo.insert_open_notification(&question(project, "d-2"), &[], now)
        .await
        .unwrap();

    let due = repo.deliveries_due(now, 10).await.unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].notification.id, opened.id);
    assert_eq!((due[0].channel.as_str(), due[0].attempts), ("webhook", 0));

    let retry = now + Duration::seconds(30);
    repo.delivery_failed(opened.id, "webhook", Some(retry), "status 500")
        .await
        .unwrap();
    assert!(repo.deliveries_due(now, 10).await.unwrap().is_empty());
    let due = repo.deliveries_due(retry, 10).await.unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].attempts, 1);

    repo.delivery_failed(opened.id, "webhook", None, "status 500")
        .await
        .unwrap();
    assert!(
        repo.deliveries_due(retry + Duration::days(1), 10)
            .await
            .unwrap()
            .is_empty(),
        "a delivery with no next time has given up"
    );
}

#[tokio::test]
async fn a_delivery_taken_is_not_due_again() {
    let repo = setup_test_db().await;
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await.id;
    let now = Utc::now();
    let opened = repo
        .insert_open_notification(&question(project, "d-1"), &webhook(), now)
        .await
        .unwrap()
        .unwrap();
    repo.delivery_done(opened.id, "webhook", now).await.unwrap();
    assert!(
        repo.deliveries_due(now + Duration::days(1), 10)
            .await
            .unwrap()
            .is_empty()
    );
    let (attempts, delivered): (i64, Option<String>) = sqlx::query_as(
        "SELECT attempts, delivered_at FROM notification_deliveries WHERE notification_id = ?",
    )
    .bind(opened.id.to_string())
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(attempts, 1);
    assert!(delivered.is_some());
}

#[tokio::test]
async fn a_channel_reports_what_waits_what_gave_up_and_the_last_error() {
    let repo = setup_test_db().await;
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await.id;
    let now = Utc::now();
    assert!(repo.channel_statuses().await.unwrap().is_empty());

    let mut ids = Vec::new();
    for (i, id) in ["d-1", "d-2", "d-3"].into_iter().enumerate() {
        let opened = repo
            .insert_open_notification(
                &question(project, id),
                &webhook(),
                now + Duration::seconds(i as i64),
            )
            .await
            .unwrap()
            .unwrap();
        ids.push(opened.id);
    }
    repo.delivery_done(ids[0], "webhook", now).await.unwrap();
    repo.delivery_failed(ids[1], "webhook", None, "status 404")
        .await
        .unwrap();

    let statuses = repo.channel_statuses().await.unwrap();
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].channel, "webhook");
    assert_eq!((statuses[0].pending, statuses[0].failed), (1, 1));
    assert_eq!(statuses[0].last_error.as_deref(), Some("status 404"));
}

#[tokio::test]
async fn what_is_settled_is_forgotten_and_what_still_waits_is_not() {
    let repo = setup_test_db().await;
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await.id;
    let old = Utc::now() - Duration::days(40);
    let cutoff = Utc::now() - Duration::days(30);

    // Resolved long ago.
    repo.insert_open_notification(&question(project, "answered"), &webhook(), old)
        .await
        .unwrap();
    repo.close_notification_by_key(&Key::decision("answered"), old)
        .await
        .unwrap();
    // An info row read long ago.
    let read = repo
        .insert_open_notification(
            &notice(
                project,
                Kind::RunFinished,
                Level::Info,
                Key::finished("r-1"),
            ),
            &[],
            old,
        )
        .await
        .unwrap()
        .unwrap();
    repo.mark_notification_read(read.id, old).await.unwrap();
    // A question read long ago and never answered, and an info row nobody read.
    let waiting = repo
        .insert_open_notification(&question(project, "waiting"), &[], old)
        .await
        .unwrap()
        .unwrap();
    repo.mark_notification_read(waiting.id, old).await.unwrap();
    repo.insert_open_notification(
        &notice(
            project,
            Kind::RunFinished,
            Level::Info,
            Key::finished("r-2"),
        ),
        &[],
        old,
    )
    .await
    .unwrap();

    assert_eq!(repo.purge_settled_notifications(cutoff).await.unwrap(), 2);
    let kept = repo.list_notifications(Listing::All, 10).await.unwrap();
    assert_eq!(kept.len(), 2);
    assert!(kept.iter().any(|n| n.id == waiting.id));
    let deliveries: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notification_deliveries")
        .fetch_one(repo.pool())
        .await
        .unwrap();
    assert_eq!(deliveries, 0, "deliveries go with their notification");
}
