//! What every read of an item says about it: `node`, `progress` on a group, the `node` and `under`
//! filters of the list, and the saved boards being gone.

use crate::common::{send, test_app};
use axum::Router;
use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

async fn item(app: &Router, project: &str, title: &str, item_type: &str, parent: Option<&str>) -> String {
    let (status, item) = send(
        app,
        "POST",
        &format!("/api/projects/{project}/items"),
        json!({"title": title, "item_type": item_type, "parent_id": parent}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{item}");
    item["id"].as_str().unwrap().to_owned()
}

async fn titles(app: &Router, uri: &str) -> Vec<String> {
    let (status, list) = send(app, "GET", uri, Value::Null, &[]).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    let mut titles: Vec<String> = list["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["title"].as_str().unwrap().to_owned())
        .collect();
    titles.sort();
    titles
}

#[tokio::test]
async fn node_work_leaves_the_epics_out_and_node_group_keeps_only_them() {
    let (app, _) = test_app().await;
    let pid = crate::common::create_project(&app, "P", "software").await.to_string();
    let epic = item(&app, &pid, "Checkout", "epic", None).await;
    item(&app, &pid, "Pay", "task", Some(&epic)).await;
    item(&app, &pid, "Lone", "task", None).await;

    let base = format!("/api/projects/{pid}/items");
    assert_eq!(titles(&app, &format!("{base}?node=work")).await, ["Lone", "Pay"]);
    assert_eq!(titles(&app, &format!("{base}?node=group")).await, ["Checkout"]);
    let (_, work) = send(&app, "GET", &format!("{base}?node=work"), Value::Null, &[]).await;
    assert_eq!(work["total"], 2, "the total follows the filter");
}

#[tokio::test]
async fn under_lists_the_subtree_at_any_depth() {
    let (app, _) = test_app().await;
    let pid = crate::common::create_project(&app, "P", "software").await.to_string();
    let epic = item(&app, &pid, "Checkout", "epic", None).await;
    let pay = item(&app, &pid, "Pay", "task", Some(&epic)).await;
    item(&app, &pid, "Receipt", "task", Some(&pay)).await;
    item(&app, &pid, "Lone", "task", None).await;

    let base = format!("/api/projects/{pid}/items");
    assert_eq!(
        titles(&app, &format!("{base}?under={epic}")).await,
        ["Pay", "Receipt"]
    );
    assert_eq!(
        titles(&app, &format!("{base}?under={epic}&node=work")).await,
        ["Receipt"]
    );
}

#[tokio::test]
async fn a_group_carries_its_node_and_the_progress_of_the_work_under_it() {
    let (app, _) = test_app().await;
    let pid = crate::common::create_project(&app, "P", "software").await.to_string();
    let epic = item(&app, &pid, "Checkout", "epic", None).await;
    let pay = item(&app, &pid, "Pay", "task", Some(&epic)).await;
    item(&app, &pid, "Receipt", "task", Some(&pay)).await;
    let (status, _) = send(
        &app,
        "PATCH",
        &format!("/api/items/{pay}"),
        json!({"estimate": 3.0}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, detail) = send(&app, "GET", &format!("/api/items/{epic}"), Value::Null, &[]).await;
    assert_eq!(detail["item"]["node"], "group");
    assert_eq!(detail["item"]["progress"]["total"], 1, "Pay holds Receipt, so it is no work");
    assert_eq!(detail["item"]["progress"]["done"], 0);

    let (_, listed) = send(
        &app,
        "GET",
        &format!("/api/projects/{pid}/items"),
        Value::Null,
        &[],
    )
    .await;
    let of = |id: &str| {
        listed["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(of(&epic)["progress"]["total"], 1);
    assert_eq!(of(&pay)["node"], "group");
    assert_eq!(of(&pay)["progress"]["total"], 1);
}

#[tokio::test]
async fn work_has_no_progress_and_a_created_item_says_what_it_is() {
    let (app, _) = test_app().await;
    let pid = crate::common::create_project(&app, "P", "software").await.to_string();
    let (_, epic) = send(
        &app,
        "POST",
        &format!("/api/projects/{pid}/items"),
        json!({"title": "Checkout", "item_type": "epic"}),
        &[],
    )
    .await;
    assert_eq!(epic["node"], "group");
    assert!(epic.get("progress").is_none(), "an epic with nothing under it has none");
    let (_, task) = send(
        &app,
        "POST",
        &format!("/api/projects/{pid}/items"),
        json!({"title": "Pay"}),
        &[],
    )
    .await;
    assert_eq!(task["node"], "work");
    assert!(task.get("progress").is_none());
}

#[tokio::test]
async fn the_saved_boards_are_gone() {
    let (app, _) = test_app().await;
    let pid = crate::common::create_project(&app, "P", "software").await.to_string();
    let board = Uuid::new_v4();
    for (method, uri) in [
        ("GET", format!("/api/boards/{board}")),
        ("GET", format!("/api/boards/{board}/view")),
        ("DELETE", format!("/api/boards/{board}")),
        ("POST", format!("/api/projects/{pid}/boards")),
        ("GET", format!("/api/projects/{pid}/boards")),
    ] {
        let (status, _) = send(&app, method, &uri, json!({"name": "B"}), &[]).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri}");
    }
}
