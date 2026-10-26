use std::collections::{HashMap, HashSet, VecDeque};

use tracing::{debug, instrument};
use uuid::Uuid;

use crate::error::CoreError;
use crate::models::DependencyType;
use crate::workflow::WorkflowConfig;

/// Lightweight edge representation for the dependency graph.
#[derive(Debug, Clone)]
pub struct DependencyEdge {
    pub source: Uuid,
    pub target: Uuid,
    pub dep_type: DependencyType,
}

/// Dependency graph for cycle detection and blocked-item computation.
#[derive(Debug, Default)]
pub struct DependencyGraph {
    /// adjacency list: item -> items it blocks
    edges: HashMap<Uuid, Vec<(Uuid, DependencyType)>>,
    /// reverse adjacency: item -> items that block it
    reverse_edges: HashMap<Uuid, Vec<(Uuid, DependencyType)>>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the graph from a list of edges.
    pub fn from_edges(edges: &[DependencyEdge]) -> Self {
        let mut graph = Self::new();
        for edge in edges {
            graph.add_edge(edge.source, edge.target, edge.dep_type.clone());
        }
        graph
    }

    /// Add a directed edge (source blocks target).
    pub fn add_edge(&mut self, source: Uuid, target: Uuid, dep_type: DependencyType) {
        self.edges
            .entry(source)
            .or_default()
            .push((target, dep_type.clone()));
        self.reverse_edges
            .entry(target)
            .or_default()
            .push((source, dep_type));
    }

    /// Check if adding an edge would create a cycle (DFS-based).
    #[instrument(skip(self), fields(source = %source, target = %target))]
    pub fn would_create_cycle(&self, source: Uuid, target: Uuid) -> bool {
        // If adding source->target, check if target can reach source
        let mut visited = HashSet::new();
        let mut stack = VecDeque::new();
        stack.push_back(target);

        while let Some(node) = stack.pop_back() {
            if node == source {
                debug!("Cycle detected: {target} can reach {source}");
                return true;
            }
            if visited.insert(node)
                && let Some(neighbors) = self.edges.get(&node)
            {
                for (neighbor, _) in neighbors {
                    stack.push_back(*neighbor);
                }
            }
        }

        false
    }

    /// Get all items that directly block the given item.
    pub fn blockers_of(&self, item_id: Uuid) -> Vec<(Uuid, DependencyType)> {
        self.reverse_edges
            .get(&item_id)
            .cloned()
            .unwrap_or_default()
    }

    /// The items that block `item` and are not yet Done. A blocker whose status is
    /// unknown (`status_of` answers `None`) counts as unfinished.
    pub fn unfinished_blockers(
        &self,
        item: Uuid,
        status_of: &dyn Fn(Uuid) -> Option<String>,
        workflow: &WorkflowConfig,
    ) -> Vec<Uuid> {
        self.blockers_of(item)
            .into_iter()
            .map(|(blocker, _)| blocker)
            .filter(|blocker| !status_of(*blocker).is_some_and(|s| workflow.is_done_status(&s)))
            .collect()
    }

    /// Validate that adding an edge won't create a cycle, returning an error if it would.
    pub fn validate_new_edge(&self, source: Uuid, target: Uuid) -> Result<(), CoreError> {
        if source == target {
            return Err(CoreError::DependencyCycle(source));
        }
        if self.would_create_cycle(source, target) {
            return Err(CoreError::DependencyCycle(target));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "dependency/tests.rs"]
mod tests;
