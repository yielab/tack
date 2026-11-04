//! What a board announces on its event bus, read from the port: a person's edit, an agent's
//! change set and a run's finish each produce exactly their events, once, with the project and
//! the principal that caused them.

use crate::common;
use crate::handlers::mrp::{
    accept_and_start, completion_with_pushed_branch, headers_ref, operator_headers,
    request_and_claim, setup_with_bus,
};
use axum::http::StatusCode;
use serde_json::{Value, json};
use tack_app::events::{DomainEvent, EventBus, Fact};
use tack_core::principal::PrincipalKind;
use tokio::sync::broadcast::Receiver;
use uuid::Uuid;

fn drain(events: &mut Receiver<DomainEvent>) -> Vec<DomainEvent> {
    std::iter::from_fn(|| events.try_recv().ok()).collect()
}

async fn operator(app: &axum::Router, method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    let (status, body, _) =
        common::send_with_raw(app, method, uri, body, &operator_headers()).await;
    (status, body)
}

struct Board {
    app: axum::Router,
    bus: EventBus,
    project: Uuid,
    item: String,
    _root: tempfile::TempDir,
}

async fn board() -> Board {
    let root = tempfile::tempdir().unwrap();
    let (app, repo, item, _ws, bus) = setup_with_bus(root.path(), None).await;
    let project = repo
        .get_item(item.parse().unwrap())
        .await
        .unwrap()
        .expect("item")
        .project_id;
    Board {
        app,
        bus,
        project,
        item,
        _root: root,
    }
}

#[tokio::test]
async fn a_persons_patch_is_one_move_and_a_plain_edit_is_one_update() {
    let board = board().await;
    let mut events = board.bus.subscribe();
    let uri = format!("/api/items/{}", board.item);

    let (status, _) = operator(&board.app, "PATCH", &uri, json!({"status": "In Progress"})).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = operator(&board.app, "PATCH", &uri, json!({"title": "Renamed"})).await;
    assert_eq!(status, StatusCode::OK);

    let heard = drain(&mut events);
    assert_eq!(heard.len(), 2, "{heard:?}");
    let item_id: Uuid = board.item.parse().unwrap();
    assert_eq!(
        heard[0].fact,
        Fact::ItemMoved {
            item_id,
            from: "To Do".into(),
            to: "In Progress".into()
        }
    );
    assert_eq!(
        heard[1].fact,
        Fact::ItemUpdated {
            item_id,
            status: "In Progress".into()
        }
    );
    for event in &heard {
        assert_eq!(event.project_id, board.project);
        assert_eq!(event.principal.kind, PrincipalKind::Operator);
    }
}

#[tokio::test]
async fn a_person_creating_and_deleting_an_item_is_one_event_each() {
    let board = board().await;
    let mut events = board.bus.subscribe();

    let (status, created) = operator(
        &board.app,
        "POST",
        &format!("/api/projects/{}/items", board.project),
        json!({"title": "New"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let id: Uuid = created["id"].as_str().unwrap().parse().unwrap();
    let (status, _) = operator(
        &board.app,
        "DELETE",
        &format!("/api/items/{id}"),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let facts: Vec<Fact> = drain(&mut events).into_iter().map(|e| e.fact).collect();
    assert!(
        matches!(&facts[..], [Fact::ItemCreated { item_id, .. }, Fact::ItemDeleted { item_id: gone }]
            if *item_id == id && *gone == id),
        "{facts:?}"
    );
}

#[tokio::test]
async fn an_agents_change_set_is_announced_when_it_applies_with_the_agents_principal() {
    let board = board().await;
    let (status, issued) = operator(
        &board.app,
        "POST",
        "/api/agent-credentials",
        json!({"name": "Planner", "scopes": [{"projects": [board.project], "capabilities": ["read", "propose"]}]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{issued}");
    let credential = issued["credential"]["id"].as_str().unwrap().to_owned();
    let secret = format!("Bearer {}", issued["secret"].as_str().unwrap());
    let mut events = board.bus.subscribe();

    let (status, proposed, _) = common::send_with_raw(
        &board.app,
        "POST",
        &format!("/api/projects/{}/change-sets", board.project),
        json!({
            "reason": "The launch needs it",
            "operations": [
                {"op": "create", "title": "Write the docs", "acceptance": ["it works"]},
                {"op": "priority", "item": board.item, "priority": "high"},
            ],
        }),
        &[("authorization", secret.as_str())],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{proposed}");
    let id = proposed["change_set"]["id"].as_str().unwrap();
    let said = drain(&mut events);
    assert_eq!(
        said.iter().map(|e| &e.fact).collect::<Vec<_>>(),
        [&Fact::ChangeSetProposed {
            change_set_id: id.to_owned(),
            item_id: None
        }],
        "a proposal that waits changes nothing on the board, and is announced as proposed"
    );
    assert_eq!(said[0].principal.id, credential);

    let (status, applied) = operator(
        &board.app,
        "POST",
        &format!("/api/change-sets/{id}/apply"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{applied}");

    let heard = drain(&mut events);
    assert_eq!(heard.len(), 3, "{heard:?}");
    assert!(matches!(heard[0].fact, Fact::ChangeSetSettled { .. }));
    assert!(matches!(heard[1].fact, Fact::ItemCreated { .. }));
    assert!(matches!(heard[2].fact, Fact::ItemUpdated { .. }));
    for event in &heard[1..] {
        assert_eq!(event.project_id, board.project);
        assert_eq!(event.principal.kind, PrincipalKind::Agent);
        assert_eq!(
            event.principal.id, credential,
            "the proposer, not the person applying"
        );
    }
}

#[tokio::test]
async fn a_run_that_starts_and_finishes_announces_each_once_with_the_runner() {
    let board = board().await;
    let attempt = request_and_claim(&board.app, &board.item, "events").await;
    let mut events = board.bus.subscribe();

    accept_and_start(&board.app, &attempt).await;
    let (status, body, _) = common::send_with_raw(
        &board.app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/completion", attempt.attempt_id),
        completion_with_pushed_branch(&attempt),
        &headers_ref(&attempt.auth),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, replayed, _) = common::send_with_raw(
        &board.app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/completion", attempt.attempt_id),
        completion_with_pushed_branch(&attempt),
        &headers_ref(&attempt.auth),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{replayed}");

    let heard = drain(&mut events);
    let facts: Vec<&Fact> = heard.iter().map(|e| &e.fact).collect();
    assert!(
        matches!(
            &facts[..],
            [Fact::RunStarted { .. }, Fact::RunFinished { .. }]
        ),
        "{facts:?}"
    );
    for event in &heard {
        assert_eq!(event.project_id, board.project);
        assert_eq!(event.principal.kind, PrincipalKind::Runner);
        assert_eq!(event.principal.id, attempt.runner_id);
    }
}

#[tokio::test]
async fn a_runners_question_opens_a_decision_and_parks_the_run_once_over_http() {
    let board = board().await;
    let attempt = request_and_claim(&board.app, &board.item, "ask").await;
    accept_and_start(&board.app, &attempt).await;
    let mut events = board.bus.subscribe();
    let question = json!({
        "protocol_version": 1, "runner_id": attempt.runner_id, "attempt_id": attempt.attempt_id,
        "fencing_token": attempt.fencing_token, "decision_id": "dec-1", "kind": "tool_permission",
        "prompt": "Allow?", "options": [{"option_id": "allow", "label": "Allow"}], "metadata": {},
    });

    for _ in 0..2 {
        let (status, body, _) = common::send_with_raw(
            &board.app,
            "POST",
            &format!("/api/runner/v1/attempts/{}/decisions", attempt.attempt_id),
            question.clone(),
            &headers_ref(&attempt.auth),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }

    let heard = drain(&mut events);
    let facts: Vec<&Fact> = heard.iter().map(|e| &e.fact).collect();
    assert!(
        matches!(
            &facts[..],
            [Fact::DecisionOpened { decision_id, .. }, Fact::RunWaiting { .. }] if decision_id == "dec-1"
        ),
        "{facts:?}"
    );
    for event in &heard {
        assert_eq!(event.project_id, board.project);
        assert_eq!(event.principal.kind, PrincipalKind::Runner);
    }
}
