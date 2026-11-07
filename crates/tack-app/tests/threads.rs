//! A thread's turns: each message starts a run of the thread's profile that reads the earlier
//! turns, the reply is posted when the run ends with the change sets the turn made, and the
//! caps stop a thread.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use tack_app::AppError;
use tack_app::ports::Artifacts;
use tack_app::threads::{self, OpenThread, Post};
use tack_core::change_set::{ChangeSetStatus, CreateOp, Operation, Origin};
use tack_core::models::UpdateProject;
use tack_core::principal::Principal;
use tack_core::thread::{MAX_TURNS, RefKind, ThreadScope, ThreadStatus, TurnState};
use tack_db::repo::change_sets::NewChangeSet;
use tack_db::{Repository, queries};
use tack_protocol::SystemClock;
use tack_protocol::conversation::{REPLY_MEDIA_TYPE, TurnNote};
use uuid::Uuid;

use tack_test_support::seed;

use crate::support::{Board, add_profile, board, set_project};

#[derive(Default)]
struct Replies(Mutex<HashMap<String, Vec<u8>>>);

#[async_trait]
impl Artifacts for Replies {
    async fn read(&self, reference: &str) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(reference).cloned()
    }
}

async fn threaded() -> (Board, Uuid) {
    let board = board().await;
    add_profile(
        &board.repo,
        "profile-limited",
        r#"{"mode":"safe"}"#,
        r#"{"budgets":{"tokens":777}}"#,
    )
    .await;
    set_project(
        &board.repo,
        board.project_id,
        UpdateProject {
            default_harness: Some(Some("codex".into())),
            ..UpdateProject::default()
        },
    )
    .await;
    let thread = threads::open(
        &board.repo,
        &SystemClock,
        OpenThread {
            scope: ThreadScope::Project {
                project: board.project_id,
            },
            profile: "profile-limited".into(),
            title: "What blocks the launch?".into(),
        },
    )
    .await
    .expect("thread")
    .id;
    (board, thread)
}

async fn say(board: &Board, thread: Uuid, body: &str) -> Result<threads::Posted, AppError> {
    threads::post(
        &board.repo,
        &SystemClock,
        Post {
            thread,
            principal: &Principal::operator("local"),
            body: body.into(),
            refs: Vec::new(),
        },
    )
    .await
}

/// The runner ended the run: succeeded, with `reply` staged as the reply artifact.
async fn finish(repo: &Repository, replies: &Replies, request_id: &str, reply: Option<&str>) {
    let attempt = format!("att-{request_id}");
    let now = Utc::now().to_rfc3339();
    seed::statement(
        repo,
        "UPDATE execution_requests SET state = 'succeeded' WHERE id = ?",
        &[request_id],
    )
    .await;
    seed::statement(
        repo,
        "INSERT INTO execution_attempts (id, request_id, attempt_number, runner_id, fencing_token,
           lease_issued_at, lease_expires_at, created_at, updated_at)
         VALUES (?, ?, 1, 'runner-active', 1, ?, ?, ?, ?)",
        &[&attempt, request_id, &now, &now, &now, &now],
    )
    .await;
    if let Some(reply) = reply {
        let reference = format!("ref-{request_id}");
        seed::statement(
            repo,
            "INSERT INTO execution_artifacts (id, attempt_id, artifact_id, kind, name, media_type,
               size_bytes, sha256, content_reference, created_at)
             VALUES (?, ?, 'reply-1', 'reply', 'tack-reply.md', ?, 1, 'x', ?, ?)",
            &[
                &format!("row-{request_id}"),
                &attempt,
                REPLY_MEDIA_TYPE,
                &reference,
                &now,
            ],
        )
        .await;
        replies
            .0
            .lock()
            .unwrap()
            .insert(reference, reply.as_bytes().to_vec());
    }
}

async fn run_snapshot(repo: &Repository, request_id: &str) -> Value {
    let raw = queries::execution_requests::request_snapshot(repo, request_id)
        .await
        .unwrap()
        .unwrap();
    serde_json::from_str(&raw).unwrap()
}

/// A run started without a person ticking boxes (a thread's, as an auto-started task's) asks
/// before each action unless every machine reports the agent sandboxed. docket's probe says
/// `none` when it reports no jail or does not say, and `harness` when it reports one.
#[tokio::test]
async fn a_docket_run_asks_unless_docket_reported_a_jail() {
    for (sandbox, approvals) in [("none", json!("ask")), ("harness", Value::Null)] {
        let (board, thread) = threaded().await;
        set_project(
            &board.repo,
            board.project_id,
            UpdateProject {
                default_harness: Some(Some("docket".into())),
                ..UpdateProject::default()
            },
        )
        .await;
        let snapshot = json!({"harnesses": [{
            "harness_kind": "docket",
            "installed_version": "0.3.0",
            "probed_at": "2026-11-07T00:00:00Z",
            "sandbox": sandbox,
        }]});
        seed::agent_runner_capabilities(&board.repo, "runner-active", &snapshot.to_string()).await;
        let turn = say(&board, thread, "go")
            .await
            .expect("posted")
            .turn
            .expect("a run");
        let request = run_snapshot(&board.repo, &turn.request_id).await;
        assert_eq!(
            request["permission_policy"]["approvals"], approvals,
            "{sandbox}: {request}"
        );
    }
}

#[tokio::test]
async fn three_turns_reach_the_profile_with_the_earlier_turns() {
    let (board, thread) = threaded().await;
    let replies = Replies::default();
    let mut previous: Vec<String> = Vec::new();
    for (n, question) in ["first question", "second question", "third question"]
        .into_iter()
        .enumerate()
    {
        let posted = say(&board, thread, question).await.expect("posted");
        let turn = posted.turn.expect("a turn started");
        assert_eq!(turn.position as usize, n + 1);
        let snapshot = run_snapshot(&board.repo, &turn.request_id).await;
        let task = snapshot["task"]["description"].as_str().unwrap().to_owned();
        assert!(task.contains(question), "{task}");
        for earlier in &previous {
            assert!(
                task.contains(earlier),
                "turn {} lacks {earlier}: {task}",
                n + 1
            );
        }
        assert!(task.contains("Board digest"), "{task}");
        let note = TurnNote::of(&snapshot["metadata"]).expect("the turn is marked");
        assert_eq!(
            (note.turn as usize, note.message.as_str()),
            (n + 1, question)
        );
        assert_eq!(
            snapshot["created_by"]["subject_id"],
            format!("thread:{thread}")
        );
        // The profile's own token limit applies to the turn.
        assert_eq!(snapshot["budgets"]["tokens"], 777);

        finish(
            &board.repo,
            &replies,
            &turn.request_id,
            Some(&format!("answer {}", n + 1)),
        )
        .await;
        assert_eq!(
            threads::settle(&board.repo, &SystemClock, &replies)
                .await
                .unwrap(),
            1
        );
        previous.push(question.to_owned());
        previous.push(format!("answer {}", n + 1));
    }
    let messages = board.repo.list_thread_messages(thread).await.unwrap();
    assert_eq!(messages.len(), 6);
    assert_eq!(messages[5].principal, format!("thread:{thread}"));
    let turns = board.repo.list_thread_turns(thread).await.unwrap();
    assert!(turns.iter().all(|turn| turn.state == TurnState::Replied));
    // Nothing of the thread is on the board.
    let listed = board
        .repo
        .list_items(board.project_id, &Default::default())
        .await
        .unwrap();
    assert_eq!(listed.len(), 1, "only the board's own item");
}

#[tokio::test]
async fn a_turn_must_answer_before_the_next_message_and_a_reply_is_posted_once() {
    let (board, thread) = threaded().await;
    let replies = Replies::default();
    let turn = say(&board, thread, "hello").await.unwrap().turn.unwrap();
    let busy = say(&board, thread, "hello again").await.unwrap_err();
    assert!(matches!(busy, AppError::Conflict(_)), "{busy}");
    finish(&board.repo, &replies, &turn.request_id, Some("hi")).await;
    assert_eq!(
        threads::settle(&board.repo, &SystemClock, &replies)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        threads::settle(&board.repo, &SystemClock, &replies)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        board.repo.list_thread_messages(thread).await.unwrap().len(),
        2
    );
}

#[tokio::test]
async fn a_reply_refers_to_the_change_sets_its_turn_made() {
    let (board, thread) = threaded().await;
    let replies = Replies::default();
    let turn = say(&board, thread, "split the launch")
        .await
        .unwrap()
        .turn
        .unwrap();
    let (set, _) = board
        .repo
        .insert_change_set(&NewChangeSet {
            project: board.project_id,
            principal: "agent:manager",
            origin: Origin::Thread,
            origin_ref: Some(&thread.to_string()),
            reason: "the launch has two halves",
            idempotency_key: None,
            anchor: None,
            artifact_id: None,
            operations: vec![Operation::Create(CreateOp {
                title: "Half".into(),
                description: None,
                acceptance: vec!["done".into()],
                depends_on: Vec::new(),
                item_type: None,
                role: None,
                team: None,
                profile_id: None,
                parent: None,
            })],
            now: Utc::now(),
        })
        .await
        .unwrap();
    assert_eq!(set.status, ChangeSetStatus::Waiting);
    finish(
        &board.repo,
        &replies,
        &turn.request_id,
        Some("I propose two halves."),
    )
    .await;
    threads::settle(&board.repo, &SystemClock, &replies)
        .await
        .unwrap();
    let reply = board
        .repo
        .list_thread_messages(thread)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(reply.body, "I propose two halves.");
    assert_eq!(reply.refs.len(), 1);
    assert_eq!(reply.refs[0].kind, RefKind::ChangeSet);
    assert_eq!(reply.refs[0].id, set.id.to_string());
}

#[tokio::test]
async fn a_run_that_leaves_no_reply_is_said_in_the_thread() {
    let (board, thread) = threaded().await;
    let replies = Replies::default();
    let turn = say(&board, thread, "hello").await.unwrap().turn.unwrap();
    finish(&board.repo, &replies, &turn.request_id, None).await;
    threads::settle(&board.repo, &SystemClock, &replies)
        .await
        .unwrap();
    let last = board
        .repo
        .list_thread_messages(thread)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert!(
        last.body.contains("without leaving a reply"),
        "{}",
        last.body
    );
    let turns = board.repo.list_thread_turns(thread).await.unwrap();
    assert_eq!(turns[0].state, TurnState::Failed);
}

#[tokio::test]
async fn the_caps_stop_a_thread() {
    let (board, thread) = threaded().await;
    seed::statement(
        &board.repo,
        "UPDATE threads SET turns = ? WHERE id = ?",
        &[&MAX_TURNS.to_string(), &thread.to_string()],
    )
    .await;
    let stopped = say(&board, thread, "one more").await.unwrap_err();
    assert!(stopped.to_string().contains("turns"), "{stopped}");
    let kept = board.repo.get_thread(thread).await.unwrap().unwrap();
    assert_eq!(kept.status, ThreadStatus::Closed);
    assert_eq!(kept.closed_reason.as_deref(), Some("turn cap"));
}

#[tokio::test]
async fn a_thread_idle_for_seven_days_closes() {
    let (board, thread) = threaded().await;
    let old = (Utc::now() - Duration::days(8)).to_rfc3339();
    seed::statement(
        &board.repo,
        "UPDATE threads SET last_message_at = ? WHERE id = ?",
        &[&old, &thread.to_string()],
    )
    .await;
    // The loop closes it without anyone writing...
    let (other, quiet) = threaded().await;
    seed::statement(
        &other.repo,
        "UPDATE threads SET last_message_at = ? WHERE id = ?",
        &[&old, &quiet.to_string()],
    )
    .await;
    threads::tick(&other.repo, &SystemClock, &Replies::default())
        .await
        .unwrap();
    let closed = other.repo.get_thread(quiet).await.unwrap().unwrap();
    assert_eq!(closed.closed_reason.as_deref(), Some("idle"));
    // ...and a message to an idle thread is refused, and closes it.
    let refused = say(&board, thread, "still there?").await.unwrap_err();
    assert!(refused.to_string().contains("7 days"), "{refused}");
    let kept = board.repo.get_thread(thread).await.unwrap().unwrap();
    assert_eq!(kept.status, ThreadStatus::Closed);
}
