//! A project's plan is written on one transaction: a section that fails takes the sections
//! before it back, and the repository functions the write is made of keep their own rules.

use std::collections::HashMap;

use tack_core::models::{
    CreateCustomField, CreateItem, CreateRole, CreateSprint, CustomFieldType, ItemSource,
    UpdateProject, UpdateRole,
};
use tack_core::outline::ResolvedOutline;
use tack_db::StoreError;
use tack_db::repo::project_file::{
    FieldWrite, ProjectWrite, RoleWrite, SprintWrite, Target, WriteError,
};
use tack_test_support::{create_test_workspace, make_project, setup_test_db};

fn write_to(project: uuid::Uuid) -> ProjectWrite {
    ProjectWrite {
        target: Target::Existing(project),
        update: None,
        types: None,
        fields: Vec::new(),
        profiles: Vec::new(),
        roles: Vec::new(),
        sprints: Vec::new(),
        automation: None,
        autonomy: None,
        items: ResolvedOutline {
            items: Vec::new(),
            blocks: Vec::new(),
            warnings: Vec::new(),
        },
        source: ItemSource::Outline,
        field_ids: HashMap::new(),
        role_ids: HashMap::new(),
        sprint_ids: HashMap::new(),
    }
}

fn role(name: &str, profile: Option<&str>, existing: Option<uuid::Uuid>) -> RoleWrite {
    RoleWrite {
        existing,
        role: CreateRole {
            key: None,
            name: name.into(),
            color: None,
            icon: None,
        },
        profile: profile.map(str::to_owned),
    }
}

#[tokio::test]
async fn a_write_whose_last_role_fails_leaves_the_project_as_it_was() {
    let repo = setup_test_db().await;
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await;
    let dev = repo
        .create_role(
            project.id,
            CreateRole {
                key: None,
                name: "Dev".into(),
                color: None,
                icon: None,
            },
        )
        .await
        .unwrap();

    let mut write = write_to(project.id);
    write.update = Some(UpdateProject {
        name: Some("Renamed".into()),
        ..Default::default()
    });
    write.fields = vec![FieldWrite::Create(CreateCustomField {
        key: None,
        name: "Plan".into(),
        field_type: CustomFieldType::Text,
        description: None,
        required: None,
        default_value: None,
        options: None,
        validation: None,
    })];
    // The last role names a profile the board does not have: the database refuses it.
    write.roles = vec![
        role("Ops", None, None),
        role("Dev", Some("ap_missing"), Some(dev.id)),
    ];
    write.sprints = vec![SprintWrite {
        existing: None,
        sprint: CreateSprint {
            key: None,
            name: "Week 1".into(),
            goal: None,
            start_date: None,
            end_date: None,
        },
        close: false,
    }];

    let failed = repo.write_project_file(write).await;
    assert!(
        matches!(
            failed,
            Err(WriteError::Section {
                section: "roles",
                ..
            })
        ),
        "{:?}",
        failed.as_ref().err()
    );

    let after = repo.get_project(project.id).await.unwrap().unwrap();
    assert_eq!(after.name, project.name);
    assert!(
        repo.list_fields_for_project(project.id)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(repo.list_sprints(project.id).await.unwrap().is_empty());
    let roles = repo.list_roles(project.id).await.unwrap();
    assert_eq!(
        roles.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        ["Dev"]
    );
    assert_eq!(roles[0].agent_profile_id, None);
    let untouched = UpdateRole {
        name: None,
        color: None,
        icon: None,
        agent_profile_id: None,
    };
    assert!(
        repo.update_role(dev.id, untouched).await.unwrap().is_some(),
        "the board's own role is still there"
    );
}

#[tokio::test]
async fn deleting_a_sprint_clears_the_sprint_of_its_items() {
    let repo = setup_test_db().await;
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await;
    let status = project.workflow.initial_status().unwrap().to_string();
    let sprint = repo
        .create_sprint(
            project.id,
            CreateSprint {
                key: None,
                name: "Week 1".into(),
                goal: None,
                start_date: None,
                end_date: None,
            },
        )
        .await
        .unwrap();
    let item = repo
        .create_item(
            project.id,
            &status,
            CreateItem {
                title: "In the sprint".into(),
                sprint_id: Some(sprint.id),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(item.sprint_id, Some(sprint.id));

    assert!(repo.delete_sprint(sprint.id).await.unwrap());
    assert!(!repo.delete_sprint(sprint.id).await.unwrap());
    let after = repo.get_item(item.id).await.unwrap().unwrap();
    assert_eq!(after.sprint_id, None);
}

#[tokio::test]
async fn a_built_in_profile_is_not_deleted() {
    let repo = setup_test_db().await;
    tack_db::repo::execution::seed_builtin_profiles(&repo, chrono::Utc::now())
        .await
        .unwrap();
    let rows = repo.list_agent_profile_rows().await.unwrap();
    let built_in = rows.iter().find(|r| r.builtin).expect("a built-in profile");

    // `delete_agent_profile_in` is the body of this call.
    let refused = repo.delete_agent_profile(&built_in.id).await;
    assert!(
        matches!(&refused, Err(StoreError::Conflict(why)) if why == "builtin_profile"),
        "{refused:?}"
    );
    let still = repo.list_agent_profile_rows().await.unwrap();
    assert!(still.iter().any(|r| r.id == built_in.id));
}
