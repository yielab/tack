//! A thread keeps its messages in order, numbers its turns, settles each turn once, and closes
//! when it has been idle; its hidden item is on no list.

use chrono::{Duration, Utc};
use tack_core::models::ItemFilter;
use tack_core::thread::{Ref, RefKind, ThreadScope, ThreadStatus, TurnState};
use tack_db::repo::threads::NewThread;
use tack_test_support::{create_test_workspace, make_project, setup_test_db};

#[tokio::test]
async fn messages_and_turns_keep_their_order_and_a_turn_settles_once() {
    let repo = setup_test_db().await;
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await;
    let now = Utc::now();
    let thread = repo
        .create_thread(&NewThread {
            scope: ThreadScope::Project {
                project: project.id,
            },
            profile: "profile-1",
            title: "What blocks the launch?",
            anchor_status: "backlog",
            now,
        })
        .await
        .expect("thread");
    assert_eq!(thread.status, ThreadStatus::Open);

    let reference = Ref {
        kind: RefKind::ChangeSet,
        id: "cs-1".into(),
    };
    let first = repo
        .append_thread_message(thread.id, "operator:local", "hello", &[], now)
        .await
        .expect("first");
    repo.append_thread_message(
        thread.id,
        "thread:x",
        "hi",
        std::slice::from_ref(&reference),
        now,
    )
    .await
    .expect("second");
    let messages = repo.list_thread_messages(thread.id).await.expect("list");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].body, "hello");
    assert_eq!(messages[1].refs, vec![reference]);

    let turn = repo
        .start_thread_turn(thread.id, first.id, "req-1", now)
        .await
        .expect("turn");
    assert_eq!(turn.position, 1);
    assert_eq!(repo.get_thread(thread.id).await.unwrap().unwrap().turns, 1);
    assert_eq!(repo.running_thread_turns().await.unwrap().len(), 1);
    assert!(
        repo.settle_thread_turn(thread.id, 1, TurnState::Replied, now)
            .await
            .unwrap()
    );
    assert!(
        !repo
            .settle_thread_turn(thread.id, 1, TurnState::Failed, now)
            .await
            .unwrap(),
        "a settled turn is not settled again"
    );
    assert!(repo.running_thread_turns().await.unwrap().is_empty());
}

#[tokio::test]
async fn the_anchor_item_is_on_no_list_and_an_idle_thread_closes() {
    let repo = setup_test_db().await;
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await;
    let now = Utc::now();
    let thread = repo
        .create_thread(&NewThread {
            scope: ThreadScope::Project {
                project: project.id,
            },
            profile: "profile-1",
            title: "t",
            anchor_status: "backlog",
            now: now - Duration::days(8),
        })
        .await
        .expect("thread");
    let anchor = repo
        .thread_anchor(thread.id)
        .await
        .unwrap()
        .expect("anchor");
    assert!(repo.get_item(anchor).await.unwrap().is_some());
    let listed = repo
        .list_items(project.id, &ItemFilter::default())
        .await
        .unwrap();
    assert!(listed.iter().all(|card| card.id != anchor));
    assert_eq!(
        repo.count_items(project.id, &ItemFilter::default())
            .await
            .unwrap(),
        0
    );

    let closed = repo
        .close_idle_threads(now - Duration::days(7))
        .await
        .unwrap();
    assert_eq!(closed, vec![thread.id]);
    let kept = repo.get_thread(thread.id).await.unwrap().unwrap();
    assert_eq!(kept.status, ThreadStatus::Closed);
    assert_eq!(kept.closed_reason.as_deref(), Some("idle"));
    assert!(!repo.close_thread(thread.id, "closed").await.unwrap());
}
