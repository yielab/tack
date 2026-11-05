//! The type catalog: a new project gets its preset, an item's type must be in it, a hidden item
//! takes the default work type, and deleting a used type moves its items first.

use tack_core::CoreError;
use tack_core::item_types::{CreateItemType, Level, TypeColor, UpdateItemType, preset};
use tack_core::models::{CreateItem, ProjectType};
use tack_db::StoreError;
use tack_db::queries;
use tack_db::repo::item_types::ItemTypeError;
use tack_test_support::{create_test_workspace, make_project, setup_test_db};

fn item(title: &str, item_type: Option<&str>) -> CreateItem {
    CreateItem {
        title: title.into(),
        item_type: item_type.map(str::to_owned),
        ..Default::default()
    }
}

#[tokio::test]
async fn a_new_project_has_the_preset_of_its_type_and_serves_it() {
    let repo = setup_test_db().await;
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await;
    assert_eq!(project.item_types, preset(&ProjectType::Software));
    let read = repo.get_project(project.id).await.unwrap().unwrap();
    assert_eq!(read.item_types, project.item_types);
    let listed = repo.list_projects(workspace).await.unwrap();
    assert_eq!(listed[0].item_types, project.item_types);
}

#[tokio::test]
async fn an_item_without_a_type_takes_the_default_and_one_the_catalog_lacks_is_refused() {
    let repo = setup_test_db().await;
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await;
    let status = project.workflow.initial_status().unwrap().to_string();

    let plain = repo
        .create_item(project.id, &status, item("plain", None))
        .await
        .unwrap();
    assert_eq!(plain.item_type, "task");
    let bug = repo
        .create_item(project.id, &status, item("bug", Some("bug")))
        .await
        .unwrap();
    assert_eq!(bug.item_type, "bug");

    let refused = repo
        .create_item(project.id, &status, item("odd", Some("saga")))
        .await;
    assert!(matches!(
        refused,
        Err(StoreError::NotFound {
            entity: "item type"
        })
    ));

    let change = tack_core::models::UpdateItem {
        item_type: Some("saga".into()),
        ..Default::default()
    };
    let outcome = repo
        .update_item_atomically(plain.id, change, &project.workflow, None)
        .await;
    assert!(matches!(
        outcome,
        Err(StoreError::NotFound {
            entity: "item type"
        })
    ));
}

#[tokio::test]
async fn merge_then_delete_moves_every_item_in_one_step() {
    let repo = setup_test_db().await;
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await;
    let status = project.workflow.initial_status().unwrap().to_string();
    for title in ["a", "b"] {
        repo.create_item(project.id, &status, item(title, Some("bug")))
            .await
            .unwrap();
    }

    let refused = repo.delete_item_type(project.id, "bug", None).await;
    assert!(matches!(
        refused,
        Err(ItemTypeError::Refused(CoreError::TypeInUse {
            items: 2,
            ..
        }))
    ));
    let used = queries::item_types::in_use(&repo, project.id)
        .await
        .unwrap();
    assert_eq!(used.iter().find(|t| t.def.key == "bug").unwrap().items, 2);

    let after = repo
        .delete_item_type(project.id, "bug", Some("feature"))
        .await
        .unwrap();
    assert!(after.iter().all(|t| t.key != "bug"));
    let used = queries::item_types::in_use(&repo, project.id)
        .await
        .unwrap();
    assert_eq!(
        used.iter().find(|t| t.def.key == "feature").unwrap().items,
        2
    );
    assert!(used.iter().all(|t| t.def.key != "bug"));
}

#[tokio::test]
async fn create_and_update_keep_one_default_work_type() {
    let repo = setup_test_db().await;
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await;

    let after = repo
        .create_item_type(
            project.id,
            &CreateItemType {
                key: "spike".into(),
                name: "Spike".into(),
                level: Level::Work,
                color: TypeColor::Accent,
            },
        )
        .await
        .unwrap();
    assert_eq!(after.last().unwrap().key, "spike");

    let after = repo
        .update_item_type(
            project.id,
            "spike",
            &UpdateItemType {
                default_work: Some(true),
                name: Some("Research spike".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(after.iter().filter(|t| t.default_work).count(), 1);
    assert!(
        after
            .iter()
            .find(|t| t.key == "spike")
            .unwrap()
            .default_work
    );

    // The default is read back for a hidden item.
    repo.create_thread(&tack_db::repo::threads::NewThread {
        scope: tack_core::thread::ThreadScope::Project {
            project: project.id,
        },
        profile: "p",
        title: "t",
        anchor_status: "backlog",
        now: chrono::Utc::now(),
    })
    .await
    .unwrap();
    let anchor: String =
        sqlx::query_scalar("SELECT item_type FROM items WHERE project_id = ? AND hidden = 1")
            .bind(project.id.to_string())
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(anchor, "spike");

    let missing = repo
        .update_item_type(project.id, "ghost", &UpdateItemType::default())
        .await;
    assert!(matches!(
        missing,
        Err(ItemTypeError::Store(StoreError::NotFound {
            entity: "item type"
        }))
    ));
}

#[tokio::test]
async fn a_level_changes_only_while_no_item_uses_the_type() {
    let repo = setup_test_db().await;
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await;
    let status = project.workflow.initial_status().unwrap().to_string();
    repo.create_item(project.id, &status, item("e", Some("epic")))
        .await
        .unwrap();
    let to_work = UpdateItemType {
        level: Some(Level::Work),
        ..Default::default()
    };
    assert!(matches!(
        repo.update_item_type(project.id, "epic", &to_work).await,
        Err(ItemTypeError::Refused(CoreError::TypeInUse { .. }))
    ));
    let to_group = UpdateItemType {
        level: Some(Level::Group),
        ..Default::default()
    };
    assert!(
        repo.update_item_type(project.id, "feature", &to_group)
            .await
            .is_ok()
    );
}
