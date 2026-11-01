//! The rule behind a run's start: the item moves to the project's `on_start_status`, unless there
//! is none, the item is already there, or it is done.

use crate::common::{World, running_attempt, world};
use tack_app::runs::move_on_start::move_item;
use tack_core::models::UpdateItem;
use tack_test_support::{seed, update_item};
use uuid::Uuid;

async fn project_of(world: &World) -> String {
    world
        .repo
        .get_item(Uuid::parse_str(&world.item_id).unwrap())
        .await
        .unwrap()
        .expect("item")
        .project_id
        .to_string()
}

async fn item_status(world: &World) -> String {
    world
        .repo
        .get_item(Uuid::parse_str(&world.item_id).unwrap())
        .await
        .unwrap()
        .expect("item")
        .status
}

async fn set_status(world: &World, status: &str) {
    update_item(
        &world.repo,
        Uuid::parse_str(&world.item_id).unwrap(),
        UpdateItem {
            status: Some(status.to_string()),
            ..Default::default()
        },
    )
    .await;
}

#[tokio::test]
async fn a_new_project_moves_the_item_to_the_workflows_first_in_progress_status() {
    let world = world().await;
    let run = running_attempt(&world, "start-default").await;
    let before = item_status(&world).await;

    let moved = move_item(&world.repo, &run.attempt_id)
        .await
        .expect("the item moved");

    assert_eq!(moved.old_status, before);
    assert_eq!(moved.new_status, "In Progress");
    assert_eq!(item_status(&world).await, "In Progress");
}

#[tokio::test]
async fn a_project_that_does_not_move_leaves_the_item_where_it_is() {
    let world = world().await;
    let project_id = project_of(&world).await;
    seed::project_on_start_status(&world.repo, &project_id, None).await;
    let run = running_attempt(&world, "start-null").await;
    let before = item_status(&world).await;

    assert_eq!(move_item(&world.repo, &run.attempt_id).await, None);
    assert_eq!(item_status(&world).await, before);
}

#[tokio::test]
async fn the_status_is_found_in_any_case_and_one_the_workflow_lacks_moves_nothing() {
    for (setting, expected) in [("in progress", Some("In Progress")), ("Nowhere", None)] {
        let world = world().await;
        let project_id = project_of(&world).await;
        seed::project_on_start_status(&world.repo, &project_id, Some(setting)).await;
        let run = running_attempt(&world, "start-case").await;
        let before = item_status(&world).await;

        let moved = move_item(&world.repo, &run.attempt_id).await;

        assert_eq!(moved.map(|m| m.new_status), expected.map(str::to_string));
        assert_eq!(item_status(&world).await, expected.unwrap_or(&before));
    }
}

#[tokio::test]
async fn an_item_already_in_the_status_is_not_moved_again() {
    let world = world().await;
    set_status(&world, "In Progress").await;
    let run = running_attempt(&world, "start-same").await;

    assert_eq!(move_item(&world.repo, &run.attempt_id).await, None);
    assert_eq!(item_status(&world).await, "In Progress");
}

#[tokio::test]
async fn a_done_item_is_not_reopened_by_a_run() {
    let world = world().await;
    set_status(&world, "Done").await;
    let run = running_attempt(&world, "start-done").await;

    assert_eq!(move_item(&world.repo, &run.attempt_id).await, None);
    assert_eq!(item_status(&world).await, "Done");
}
