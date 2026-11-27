//! One write of a project's plan: everything a checked project file asks for, written on one
//! `BEGIN IMMEDIATE` transaction, so a failure in any section leaves the board as it was. The
//! caller (`tack-api`'s outline handler) checked the file and mapped it to a [`ProjectWrite`]; this
//! writes it, section by section, through the repositories' own connection-level functions.

use std::collections::HashMap;

use chrono::Utc;
use sqlx::SqliteConnection;
use tack_core::CoreError;
use tack_core::hierarchy::Hierarchy;
use tack_core::item_types::ItemTypeDef;
use tack_core::models::{
    CreateCustomField, CreateProject, CreateRole, CreateSprint, Item, ItemSource, SprintStatus,
    UpdateCustomField, UpdateProject, UpdateRole, UpdateSprint,
};
use tack_core::outline::ResolvedOutline;
use tack_core::outline::bundle::ProfileRow;
use uuid::Uuid;

use super::Repository;
use super::autonomy::{Settings, put_autonomy_settings_in};
use super::custom_fields::{create_field_in, set_field_value_in, update_field_in};
use super::execution::write_agent_profile_in;
use super::item_types::{CatalogError, set_item_types_in};
use super::items::{ItemError, create_outline_items_in};
use super::projects::{create_project_in, update_project_in};
use super::roles::{assign_role_to_item_in, create_role_in, update_role_in};
use super::sprints::{create_sprint_in, get_sprint_in, update_sprint_in, update_sprint_status_in};
use crate::StoreError;

/// The project a write lands in.
pub enum Target {
    /// A project the file creates, in this workspace.
    New {
        workspace_id: Uuid,
        project: CreateProject,
    },
    Existing(Uuid),
}

/// A field the file creates, or one the board has and the file changes.
pub enum FieldWrite {
    Create(CreateCustomField),
    Update { id: Uuid, change: UpdateCustomField },
}

/// A role the file creates (`existing` is `None`) or one the board has, and the profile (an id)
/// it names.
pub struct RoleWrite {
    pub existing: Option<Uuid>,
    pub role: CreateRole,
    pub profile: Option<String>,
}

/// A sprint the file creates (`existing` is `None`) or one the board has. What the file leaves
/// out of an existing sprint keeps its stored value. `close` moves it to closed afterwards.
pub struct SprintWrite {
    pub existing: Option<Uuid>,
    pub sprint: CreateSprint,
    pub close: bool,
}

/// Everything one checked file writes, in the order it is written: the project, its settings,
/// the type catalog, fields, profiles, roles, sprints, automation, then the items with their
/// roles and field values. The three maps hold the board's own rows by name; the write adds
/// what it creates, and the items name their sprint, roles and fields through them.
pub struct ProjectWrite {
    pub target: Target,
    pub update: Option<UpdateProject>,
    pub types: Option<Vec<ItemTypeDef>>,
    pub fields: Vec<FieldWrite>,
    pub profiles: Vec<ProfileRow>,
    pub roles: Vec<RoleWrite>,
    pub sprints: Vec<SprintWrite>,
    /// The project's own automation keys.
    pub automation: Option<UpdateProject>,
    /// The autonomy settings the project ends with.
    pub autonomy: Option<Settings>,
    pub items: ResolvedOutline,
    pub source: ItemSource,
    pub field_ids: HashMap<String, Uuid>,
    pub role_ids: HashMap<String, Uuid>,
    pub sprint_ids: HashMap<String, Uuid>,
}

/// What a write made.
pub struct Written {
    pub project_id: Uuid,
    /// The items in written order: parents before their children.
    pub items: Vec<Item>,
    pub dependencies: usize,
}

/// What stops a write; the board is as it was.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// The store failed in this section of the file.
    #[error("the {section} section could not be written")]
    Section {
        section: &'static str,
        #[source]
        source: StoreError,
    },
    /// A rule of the board refused what the file asked.
    #[error(transparent)]
    Refused(#[from] CoreError),
    /// The transaction could not begin or end.
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl From<sqlx::Error> for WriteError {
    fn from(error: sqlx::Error) -> Self {
        WriteError::Store(error.into())
    }
}

/// Names the section a failed store call belonged to.
fn at<E: Into<StoreError>>(section: &'static str) -> impl Fn(E) -> WriteError {
    move |source| WriteError::Section {
        section,
        source: source.into(),
    }
}

impl Repository {
    /// Writes a checked project file whole: either every section lands, or none does.
    pub async fn write_project_file(&self, write: ProjectWrite) -> Result<Written, WriteError> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let written = write_in(&mut tx, write).await?;
        tx.commit().await?;
        Ok(written)
    }
}

async fn write_in(conn: &mut SqliteConnection, write: ProjectWrite) -> Result<Written, WriteError> {
    let ProjectWrite {
        target,
        update,
        types,
        fields,
        profiles,
        roles,
        sprints,
        automation,
        autonomy,
        mut items,
        source,
        mut field_ids,
        mut role_ids,
        mut sprint_ids,
    } = write;
    let project_id = match target {
        Target::New {
            workspace_id,
            project,
        } => {
            create_project_in(conn, workspace_id, project)
                .await
                .map_err(at("project"))?
                .id
        }
        Target::Existing(id) => id,
    };
    if let Some(update) = update {
        update_project_in(conn, project_id, update)
            .await
            .map_err(at("project"))?;
    }

    if let Some(types) = &types {
        set_item_types_in(conn, project_id, types)
            .await
            .map_err(|failure| match failure {
                CatalogError::Store(source) => at("types")(source),
                CatalogError::Refused(rule) => WriteError::Refused(rule),
            })?;
    }

    for row in fields {
        match row {
            FieldWrite::Create(field) => {
                let name = field.name.trim().to_string();
                let created = create_field_in(conn, project_id, field)
                    .await
                    .map_err(at("fields"))?;
                field_ids.insert(name, created.id);
            }
            FieldWrite::Update { id, change } => {
                update_field_in(conn, id, change)
                    .await
                    .map_err(at("fields"))?;
            }
        }
    }

    let now = Utc::now();
    for profile in &profiles {
        write_agent_profile_in(conn, profile, now)
            .await
            .map_err(at("profiles"))?;
    }

    for row in roles {
        let name = row.role.name.trim().to_string();
        let id = match row.existing {
            Some(id) => {
                let change = UpdateRole {
                    name: None,
                    color: row.role.color,
                    icon: row.role.icon,
                    agent_profile_id: row.profile.map(Some),
                };
                update_role_in(conn, id, change)
                    .await
                    .map_err(at("roles"))?;
                id
            }
            None => {
                let created = create_role_in(
                    conn,
                    project_id,
                    CreateRole {
                        name: name.clone(),
                        color: row.role.color,
                        icon: row.role.icon,
                    },
                )
                .await
                .map_err(at("roles"))?;
                if row.profile.is_some() {
                    let change = UpdateRole {
                        name: None,
                        color: None,
                        icon: None,
                        agent_profile_id: Some(row.profile),
                    };
                    update_role_in(conn, created.id, change)
                        .await
                        .map_err(at("roles"))?;
                }
                created.id
            }
        };
        role_ids.insert(name, id);
    }

    for row in sprints {
        let name = row.sprint.name.trim().to_string();
        let id = match row.existing {
            Some(id) => {
                let old = get_sprint_in(conn, id).await.map_err(at("sprints"))?;
                let change = UpdateSprint {
                    name: name.clone(),
                    goal: row
                        .sprint
                        .goal
                        .or_else(|| old.as_ref().and_then(|o| o.goal.clone())),
                    start_date: row
                        .sprint
                        .start_date
                        .or_else(|| old.as_ref().and_then(|o| o.start_date)),
                    end_date: row
                        .sprint
                        .end_date
                        .or_else(|| old.as_ref().and_then(|o| o.end_date)),
                };
                update_sprint_in(conn, id, change)
                    .await
                    .map_err(at("sprints"))?;
                id
            }
            None => {
                let sprint = CreateSprint {
                    name: name.clone(),
                    ..row.sprint
                };
                create_sprint_in(conn, project_id, sprint)
                    .await
                    .map_err(at("sprints"))?
                    .id
            }
        };
        if row.close {
            update_sprint_status_in(conn, id, SprintStatus::Closed)
                .await
                .map_err(at("sprints"))?;
        }
        sprint_ids.insert(name, id);
    }

    if let Some(automation) = automation {
        update_project_in(conn, project_id, automation)
            .await
            .map_err(at("automation"))?;
    }
    if let Some(settings) = &autonomy {
        let found = put_autonomy_settings_in(conn, &project_id.to_string(), settings)
            .await
            .map_err(at("automation"))?;
        if !found {
            return Err(at("automation")(StoreError::not_found("project")));
        }
    }

    let mut extras = Vec::with_capacity(items.items.len());
    for planned in &mut items.items {
        if let Some(sprint) = planned.sprint.as_deref() {
            planned.input.sprint_id = sprint_ids.get(sprint.trim()).copied();
        }
        extras.push((planned.roles.clone(), planned.fields.clone()));
    }
    let (created, dependencies) =
        create_outline_items_in(conn, project_id, items, source, &Hierarchy)
            .await
            .map_err(|failure| match failure {
                ItemError::Store(source) => at("items")(source),
                ItemError::Refused(rule) => WriteError::Refused(rule),
            })?;
    for (item, (roles, fields)) in created.iter().zip(extras) {
        for role in roles.iter().map(|r| r.trim()).filter(|r| !r.is_empty()) {
            if let Some(&role_id) = role_ids.get(role) {
                assign_role_to_item_in(conn, item.id, role_id)
                    .await
                    .map_err(at("roles"))?;
            }
        }
        for (name, value) in fields {
            if let Some(&field_id) = field_ids.get(name.trim()) {
                set_field_value_in(conn, item.id, field_id, value)
                    .await
                    .map_err(at("fields"))?;
            }
        }
    }
    Ok(Written {
        project_id,
        items: created,
        dependencies,
    })
}
