//! The tree of items has one rule, here (ADR 0036, decision 11): an item is a group when its
//! type holds work or when it has children, otherwise it is work; a group holds groups and work,
//! work holds work, a group never sits under work, the tree has at most [`MAX_DEPTH`] levels and
//! no loops, and a group is not planned into a sprint. Pure; the store reads the facts inside the
//! transaction that moves an item and asks [`ParentGate`], as it asks `StatusGate` about a status.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::CoreError;
use crate::item_types::Level;

/// The most levels a tree has: a top-level item is the first.
pub const MAX_DEPTH: usize = 5;

/// What an item is for the screens: a group holds items, work is done.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum Node {
    Group,
    Work,
}

/// Whether an item is a group or work: a group when its type holds work, or when it has children.
pub fn node(level: Level, has_children: bool) -> Node {
    if level == Level::Group || has_children {
        Node::Group
    } else {
        Node::Work
    }
}

/// The item that is being put under a parent, with what the rule needs to know of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    pub item: Uuid,
    pub title: String,
    /// The name its type has in the project ("Epic").
    pub type_name: String,
    pub level: Level,
    pub project: Uuid,
    /// How many levels lie below it: 0 for an item with no children.
    pub subtree_height: usize,
}

/// The item that would become the parent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub id: Uuid,
    pub type_name: String,
    pub level: Level,
    pub project: Uuid,
    /// Its parents, nearest first: empty when it is a top-level item.
    pub ancestors: Vec<Uuid>,
}

impl Target {
    /// The level the target sits at: 1 for a top-level item.
    pub fn depth(&self) -> usize {
        self.ancestors.len() + 1
    }
}

/// Whether `placement` may go under `parent`, or to the top when there is none. Moving to the top
/// never breaks a rule, so it is always admitted.
pub fn admit_parent(placement: &Placement, parent: Option<&Target>) -> Result<(), CoreError> {
    let Some(parent) = parent else {
        return Ok(());
    };
    if parent.project != placement.project {
        return Err(CoreError::ParentInOtherProject);
    }
    if parent.id == placement.item || parent.ancestors.contains(&placement.item) {
        return Err(CoreError::ParentLoop {
            title: placement.title.clone(),
        });
    }
    if placement.level == Level::Group && parent.level == Level::Work {
        return Err(CoreError::GroupUnderWork {
            group: placement.type_name.clone(),
            parent: parent.type_name.clone(),
        });
    }
    if parent.depth() + 1 + placement.subtree_height > MAX_DEPTH {
        return Err(CoreError::TreeTooDeep);
    }
    Ok(())
}

/// Whether an item of this `node` may be planned into a sprint: only work is.
/// `type_name` is the name its type has in the project.
pub fn admit_sprint(node: Node, type_name: &str) -> Result<(), CoreError> {
    match node {
        Node::Work => Ok(()),
        Node::Group => Err(CoreError::GroupInSprint {
            type_name: type_name.to_owned(),
        }),
    }
}

/// What a store asks, inside the transaction that gives an item a parent, to learn whether it is
/// allowed. The store supplies the facts it alone can read atomically; the decision is core's.
pub trait ParentGate: Sync {
    fn admit(&self, placement: &Placement, parent: Option<&Target>) -> Result<(), CoreError>;
}

/// The rule of this module, as a gate.
#[derive(Debug, Clone, Copy, Default)]
pub struct Hierarchy;

impl ParentGate for Hierarchy {
    fn admit(&self, placement: &Placement, parent: Option<&Target>) -> Result<(), CoreError> {
        admit_parent(placement, parent)
    }
}

/// One item of a [`Forest`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub parent: Option<Uuid>,
    pub title: String,
    pub type_name: String,
    pub level: Level,
}

/// The parents of a project's items, so a set of changes can be checked one after the other the
/// way the store would apply them, without a database.
#[derive(Debug, Clone, Default)]
pub struct Forest {
    project: Uuid,
    entries: HashMap<Uuid, Entry>,
}

impl Forest {
    pub fn new(project: Uuid) -> Self {
        Self {
            project,
            entries: HashMap::new(),
        }
    }

    pub fn insert(&mut self, id: Uuid, entry: Entry) {
        self.entries.insert(id, entry);
    }

    /// Whether `id` is an item of the project.
    pub fn contains(&self, id: Uuid) -> bool {
        self.entries.contains_key(&id)
    }

    /// Gives `id` the parent `parent`; the caller has admitted it.
    pub fn set_parent(&mut self, id: Uuid, parent: Option<Uuid>) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.parent = parent;
        }
    }

    /// The parents of `id`, nearest first. A loop in the stored parents stops the walk.
    fn ancestors(&self, id: Uuid) -> Vec<Uuid> {
        let mut found = Vec::new();
        let mut seen = HashSet::from([id]);
        let mut next = self.entries.get(&id).and_then(|e| e.parent);
        while let Some(parent) = next {
            if !seen.insert(parent) {
                break;
            }
            found.push(parent);
            next = self.entries.get(&parent).and_then(|e| e.parent);
        }
        found
    }

    /// How many levels lie below `id`.
    fn subtree_height(&self, id: Uuid) -> usize {
        let mut seen = HashSet::from([id]);
        let mut frontier = vec![id];
        let mut height = 0;
        loop {
            let next: Vec<Uuid> = self
                .entries
                .iter()
                .filter(|(child, entry)| {
                    entry.parent.is_some_and(|p| frontier.contains(&p)) && seen.insert(**child)
                })
                .map(|(child, _)| *child)
                .collect();
            if next.is_empty() {
                return height;
            }
            height += 1;
            frontier = next;
        }
    }

    /// `id` as an item to put under a parent.
    pub fn placement(&self, id: Uuid) -> Option<Placement> {
        let entry = self.entries.get(&id)?;
        Some(Placement {
            item: id,
            title: entry.title.clone(),
            type_name: entry.type_name.clone(),
            level: entry.level,
            project: self.project,
            subtree_height: self.subtree_height(id),
        })
    }

    /// `id` as a parent.
    pub fn target(&self, id: Uuid) -> Option<Target> {
        let entry = self.entries.get(&id)?;
        Some(Target {
            id,
            type_name: entry.type_name.clone(),
            level: entry.level,
            project: self.project,
            ancestors: self.ancestors(id),
        })
    }
}

#[cfg(test)]
#[path = "hierarchy/tests.rs"]
mod tests;
