//! What routing and auto-start read of the tree: the chain of an item with the profile and roles
//! on each link, the facts of a whole project, and the work waiting for someone to do it.

use tack_core::hierarchy::{Node, chain};
use tack_core::models::{CreateItem, CreateRole, Item, Project};
use tack_db::{Repository, queries};
use tack_test_support::{create_test_workspace, make_project, seed, setup_test_db};
use uuid::Uuid;

async fn make(
    repo: &Repository,
    project: &Project,
    title: &str,
    item_type: &str,
    parent: Option<Uuid>,
) -> Item {
    repo.create_item(
        project.id,
        &project.workflow.initial_status().unwrap(),
        CreateItem {
            title: title.into(),
            item_type: Some(item_type.into()),
            parent_id: parent,
            ..Default::default()
        },
    )
    .await
    .unwrap()
}

async fn role(repo: &Repository, project: &Project, name: &str, profile: &str) -> Uuid {
    let role = repo
        .create_role(
            project.id,
            CreateRole {
                name: name.into(),
                color: None,
                icon: None,
            },
        )
        .await
        .unwrap();
    seed::agent_profile_for_team(repo, profile, profile, "t").await;
    seed::role_agent_profile(repo, &role.id.to_string(), profile).await;
    role.id
}

async fn choose_profile(repo: &Repository, item: &Item, profile: &str) {
    sqlx::query("UPDATE items SET run_settings = json_object('agent_profile_id', ?) WHERE id = ?")
        .bind(profile)
        .bind(item.id.to_string())
        .execute(repo.pool())
        .await
        .unwrap();
}

async fn fixture() -> (Repository, Project) {
    let repo = setup_test_db().await;
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await;
    (repo, project)
}

async fn waiting(repo: &Repository, project: &Project) -> Vec<Uuid> {
    let status = project.workflow.initial_status().unwrap();
    let mut ids: Vec<Uuid> =
        queries::items::waiting_with_someone_to_do_it(repo, &project.id.to_string(), &status)
            .await
            .unwrap()
            .into_iter()
            .map(|row| row.id.parse().unwrap())
            .collect();
    ids.sort();
    ids
}

#[tokio::test]
async fn the_chain_is_the_item_then_its_ancestors_with_what_each_names() {
    let (repo, project) = fixture().await;
    let shop = make(&repo, &project, "Shop", "epic", None).await;
    let checkout = make(&repo, &project, "Checkout", "epic", Some(shop.id)).await;
    let pay = make(&repo, &project, "Pay", "task", Some(checkout.id)).await;
    let builder = role(&repo, &project, "Builder", "p-build").await;
    repo.assign_role_to_item(checkout.id, builder)
        .await
        .unwrap();
    choose_profile(&repo, &shop, "p-shop").await;

    let links = queries::items::routing_chain(&repo, pay.id).await.unwrap();
    let titles: Vec<&str> = links.iter().map(|l| l.title.as_str()).collect();
    assert_eq!(titles, ["Pay", "Checkout", "Shop"]);
    assert_eq!(links[0].run_settings, None);
    assert!(links[0].roles.is_empty());
    assert_eq!(
        links[1].roles,
        [("Builder".to_owned(), Some("p-build".to_owned()))]
    );
    assert_eq!(links[2].run_settings.as_deref(), Some("p-shop"));

    // The project-wide facts build the same chain in core.
    let facts = queries::items::routing_facts(&repo, project.id)
        .await
        .unwrap();
    assert_eq!(chain(pay.id, &facts), links);
    assert!(
        queries::items::routing_chain(&repo, Uuid::new_v4())
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn an_item_is_a_group_by_its_type_or_by_what_sits_under_it() {
    let (repo, project) = fixture().await;
    let epic = make(&repo, &project, "Checkout", "epic", None).await;
    let task = make(&repo, &project, "Pay", "task", None).await;
    let node = |item: &Item| {
        let (repo, id) = (repo.clone(), item.id);
        async move { queries::items::node_of(&repo, id).await.unwrap().unwrap() }
    };
    let found = node(&epic).await;
    assert_eq!(
        (found.node, found.type_name.as_str()),
        (Node::Group, "Epic")
    );
    assert_eq!(node(&task).await.node, Node::Work);
    make(&repo, &project, "Refund", "task", Some(task.id)).await;
    assert_eq!(node(&task).await.node, Node::Group);
    assert!(
        queries::items::node_of(&repo, Uuid::new_v4())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn auto_start_waits_for_work_never_for_a_group_and_looks_up_the_tree_for_someone() {
    let (repo, project) = fixture().await;
    let epic = make(&repo, &project, "Checkout", "epic", None).await;
    let pay = make(&repo, &project, "Pay", "task", Some(epic.id)).await;
    let refund = make(&repo, &project, "Refund", "task", Some(epic.id)).await;
    let alone = make(&repo, &project, "Alone", "task", None).await;
    let builder = role(&repo, &project, "Builder", "p-build").await;

    // Nobody is named anywhere: nothing waits.
    assert!(waiting(&repo, &project).await.is_empty());

    // The epic has the role, so its two tasks wait, and the epic itself does not.
    repo.assign_role_to_item(epic.id, builder).await.unwrap();
    let mut want = vec![pay.id, refund.id];
    want.sort();
    assert_eq!(waiting(&repo, &project).await, want);

    // A task with children is a group too: it leaves the list, its children stay.
    let sub = make(&repo, &project, "Sub", "task", Some(pay.id)).await;
    let mut want = vec![refund.id, sub.id];
    want.sort();
    assert_eq!(waiting(&repo, &project).await, want);

    // A task outside the epic needs someone of its own.
    assert!(!waiting(&repo, &project).await.contains(&alone.id));
    choose_profile(&repo, &alone, "p-solo").await;
    assert!(waiting(&repo, &project).await.contains(&alone.id));
}
