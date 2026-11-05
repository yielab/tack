//! The tree rule over HTTP: moving an item with `parent_id`, the parents a picker is offered,
//! and a change set that would break the tree.

use crate::common::{send, test_app_with_config};
use axum::Router;
use axum::http::StatusCode;
use serde_json::{Value, json};
use tack_api::config::AppConfig;

const OPERATOR: &str = "Bearer operator-token";

async fn call(app: &Router, method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(app, method, uri, body, &[("authorization", OPERATOR)]).await
}

struct Board {
    app: Router,
    project: String,
}

impl Board {
    async fn new() -> Board {
        let (app, _) = test_app_with_config(AppConfig {
            api_token: Some("operator-token".into()),
            ..AppConfig::default()
        })
        .await;
        let (_, project) = call(
            &app,
            "POST",
            "/api/projects",
            json!({"name": "My SaaS", "project_type": "software"}),
        )
        .await;
        let project = project["id"].as_str().unwrap().to_owned();
        Board { app, project }
    }

    async fn item(&self, title: &str, item_type: &str, parent: Option<&str>) -> String {
        let (status, item) = call(
            &self.app,
            "POST",
            &format!("/api/projects/{}/items", self.project),
            json!({"title": title, "item_type": item_type, "parent_id": parent}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{item}");
        item["id"].as_str().unwrap().to_owned()
    }

    async fn move_under(&self, item: &str, parent: Value) -> (StatusCode, Value) {
        call(
            &self.app,
            "PATCH",
            &format!("/api/items/{item}"),
            json!({"parent_id": parent}),
        )
        .await
    }

    async fn parents(&self, query: &str) -> Vec<String> {
        let (status, list) = call(
            &self.app,
            "GET",
            &format!("/api/projects/{}/parents?{query}", self.project),
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{list}");
        list.as_array()
            .unwrap()
            .iter()
            .map(|p| p["title"].as_str().unwrap().to_owned())
            .collect()
    }
}

#[tokio::test]
async fn patching_parent_id_moves_an_item_and_null_puts_it_at_the_top() {
    let b = Board::new().await;
    let epic = b.item("Checkout", "epic", None).await;
    let task = b.item("Pay", "task", None).await;

    let (status, moved) = b.move_under(&task, json!(epic)).await;
    assert_eq!(status, StatusCode::OK, "{moved}");
    assert_eq!(moved["parent_id"], epic.as_str());

    let (status, top) = b.move_under(&task, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{top}");
    assert_eq!(top["parent_id"], Value::Null);
}

#[tokio::test]
async fn each_refusal_of_the_tree_answers_its_own_code_and_sentence() {
    let b = Board::new().await;
    let epic = b.item("Checkout", "epic", None).await;
    let task = b.item("Pay", "task", Some(&epic)).await;

    let (status, refused) = b.move_under(&epic, json!(task)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert_eq!(refused["error"]["code"], "parent_loop");
    assert_eq!(
        refused["error"]["message"],
        "That would put Checkout inside itself."
    );

    let other = b.item("Receipt", "task", None).await;
    let (status, refused) = b.move_under(&epic, json!(other)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert_eq!(refused["error"]["code"], "group_under_work");
    assert_eq!(
        refused["error"]["message"],
        "An epic cannot go under a task."
    );

    // Five levels: Pay is on the second, so three more fit and the next does not.
    let mut parent = task.clone();
    for level in 3..=5 {
        parent = b.item(&format!("L{level}"), "task", Some(&parent)).await;
    }
    let (status, refused) = call(
        &b.app,
        "POST",
        &format!("/api/projects/{}/items", b.project),
        json!({"title": "L6", "parent_id": parent}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert_eq!(refused["error"]["code"], "tree_too_deep");
    assert_eq!(refused["error"]["message"], "Five levels is the most.");

    // A parent of another project.
    let (_, elsewhere) = call(
        &b.app,
        "POST",
        "/api/projects",
        json!({"name": "Other", "project_type": "software"}),
    )
    .await;
    let foreign = call(
        &b.app,
        "POST",
        &format!("/api/projects/{}/items", elsewhere["id"].as_str().unwrap()),
        json!({"title": "Elsewhere", "item_type": "epic"}),
    )
    .await
    .1["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (status, refused) = b.move_under(&other, json!(foreign)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert_eq!(refused["error"]["code"], "parent_in_other_project");
}

#[tokio::test]
async fn a_group_is_not_planned_into_a_sprint() {
    let b = Board::new().await;
    let epic = b.item("Checkout", "epic", None).await;
    let (_, sprint) = call(
        &b.app,
        "POST",
        &format!("/api/projects/{}/sprints", b.project),
        json!({"name": "S1"}),
    )
    .await;
    let (status, refused) = call(
        &b.app,
        "PATCH",
        &format!("/api/items/{epic}"),
        json!({"sprint_id": sprint["id"]}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert_eq!(refused["error"]["code"], "group_in_sprint");
    assert_eq!(
        refused["error"]["message"],
        "An epic does not go in a sprint: its items do."
    );
}

#[tokio::test]
async fn a_change_set_that_breaks_the_tree_is_refused_whole() {
    let b = Board::new().await;
    let epic = b.item("Checkout", "epic", None).await;
    let task = b.item("Pay", "task", Some(&epic)).await;
    let other = b.item("Receipt", "task", None).await;
    let uri = format!("/api/projects/{}/change-sets", b.project);

    let (status, refused) = call(
        &b.app,
        "POST",
        &uri,
        json!({
            "reason": "Reorganise",
            "operations": [
                {"op": "priority", "item": other, "priority": "high"},
                {"op": "parent", "item": epic, "parent": task},
                {"op": "parent", "item": epic, "parent": other},
            ],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    let message = refused["error"]["message"].as_str().unwrap();
    assert!(message.contains("2 reasons"), "{message}");
    assert!(message.contains("inside itself"), "{message}");
    assert!(
        message.contains("An epic cannot go under a task"),
        "{message}"
    );
    // Nothing was kept, not even the priority that was fine.
    let (_, item) = call(&b.app, "GET", &format!("/api/items/{other}"), Value::Null).await;
    assert_eq!(item["item"]["priority"], "medium");
    let (_, listed) = call(&b.app, "GET", &uri, Value::Null).await;
    assert_eq!(listed, json!([]));
}

#[tokio::test]
async fn a_change_set_moves_an_item_under_a_parent() {
    let b = Board::new().await;
    let epic = b.item("Checkout", "epic", None).await;
    let task = b.item("Pay", "task", None).await;
    let (status, done) = call(
        &b.app,
        "POST",
        &format!("/api/projects/{}/change-sets", b.project),
        json!({
            "reason": "Pay belongs to Checkout",
            "operations": [{"op": "parent", "item": task, "parent": epic}],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{done}");
    // The default rung leaves it for a person, who applies it.
    let id = done["change_set"]["id"].as_str().unwrap();
    let (status, applied) = call(
        &b.app,
        "POST",
        &format!("/api/change-sets/{id}/apply"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    let (_, item) = call(&b.app, "GET", &format!("/api/items/{task}"), Value::Null).await;
    assert_eq!(item["item"]["parent_id"], epic.as_str());
}

#[tokio::test]
async fn the_parents_offered_for_a_task_leave_out_its_own_subtree() {
    let b = Board::new().await;
    let epic = b.item("Checkout", "epic", None).await;
    let task = b.item("Pay", "task", Some(&epic)).await;
    b.item("Receipt", "task", Some(&task)).await;
    b.item("Refund", "task", None).await;

    let offered = b.parents(&format!("item={task}")).await;
    assert_eq!(offered, vec!["Checkout", "Refund"]);
}

#[tokio::test]
async fn the_parents_offered_for_a_new_epic_are_only_groups() {
    let b = Board::new().await;
    b.item("Checkout", "epic", None).await;
    b.item("Pay", "task", None).await;

    assert_eq!(b.parents("type=epic").await, vec!["Checkout"]);
    assert_eq!(b.parents("type=task").await, vec!["Checkout", "Pay"]);

    let (status, _) = call(
        &b.app,
        "GET",
        &format!("/api/projects/{}/parents", b.project),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call(
        &b.app,
        "GET",
        &format!("/api/projects/{}/parents?type=saga", b.project),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
