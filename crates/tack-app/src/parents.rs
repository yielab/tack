//! Where an item may go in the tree: the parents the rule of `tack_core::hierarchy` admits, for
//! an item that exists (a move) or for one about to be made (a type). Every picker reads this, so
//! no screen works the rule out itself.

use tack_core::hierarchy::{Forest, Placement, admit_parent};
use tack_db::queries::items::{TreeRow, tree_of_project};
use tack_db::{Repository, StoreError};
use uuid::Uuid;

use crate::AppError;

/// An item that can be a parent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParentChoice {
    pub id: Uuid,
    pub title: String,
    /// The key of its type in the project's catalog.
    pub item_type: String,
}

/// Whose parent is being chosen.
#[derive(Debug, Clone)]
pub enum Subject {
    /// An item that exists: the choices leave out the item and everything under it.
    Item(Uuid),
    /// An item about to be made, by the key of its type.
    NewOfType(String),
}

/// The project's items in tree order, and the parents of all of them as a [`Forest`].
pub(crate) async fn read_tree(
    repo: &Repository,
    project_id: Uuid,
) -> Result<(Forest, Vec<TreeRow>), StoreError> {
    let rows = tree_of_project(repo, project_id).await?;
    let mut forest = Forest::new(project_id);
    for row in &rows {
        forest.insert(row.id, row.entry.clone());
    }
    Ok((forest, rows))
}

/// Every item of the project that may be the parent of `subject`, in tree order.
pub async fn admitted(
    repo: &Repository,
    project_id: Uuid,
    subject: Subject,
) -> Result<Vec<ParentChoice>, AppError> {
    let project = repo
        .get_project(project_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let (forest, rows) = read_tree(repo, project_id).await?;
    let placement = match subject {
        Subject::Item(id) => forest.placement(id).ok_or(AppError::NotFound)?,
        Subject::NewOfType(key) => {
            let kind = project
                .item_types
                .iter()
                .find(|kind| kind.key == key)
                .ok_or(AppError::NotFound)?;
            Placement {
                item: Uuid::nil(),
                title: String::new(),
                type_name: kind.name.clone(),
                level: kind.level,
                project: project_id,
                subtree_height: 0,
            }
        }
    };
    Ok(rows
        .into_iter()
        .filter(|row| !row.hidden)
        .filter(|row| {
            forest
                .target(row.id)
                .is_some_and(|target| admit_parent(&placement, Some(&target)).is_ok())
        })
        .map(|row| ParentChoice {
            id: row.id,
            title: row.entry.title,
            item_type: row.item_type,
        })
        .collect())
}
