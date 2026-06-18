use std::collections::{HashMap, HashSet, VecDeque};

use tracing::{debug, instrument};
use uuid::Uuid;

use crate::error::CoreError;
use crate::models::DependencyType;

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

    /// Get all items that the given item directly blocks.
    pub fn blocked_by(&self, item_id: Uuid) -> Vec<(Uuid, DependencyType)> {
        self.edges.get(&item_id).cloned().unwrap_or_default()
    }

    /// Get all transitively blocked items (BFS from source).
    pub fn all_downstream(&self, item_id: Uuid) -> HashSet<Uuid> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(item_id);

        while let Some(node) = queue.pop_front() {
            if let Some(neighbors) = self.edges.get(&node) {
                for (neighbor, _) in neighbors {
                    if visited.insert(*neighbor) {
                        queue.push_back(*neighbor);
                    }
                }
            }
        }

        visited
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
mod tests {
    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn test_no_cycle_simple_chain() {
        let mut graph = DependencyGraph::new();
        graph.add_edge(id(1), id(2), DependencyType::Blocks);
        graph.add_edge(id(2), id(3), DependencyType::Blocks);

        assert!(!graph.would_create_cycle(id(3), id(4)));
        assert!(graph.validate_new_edge(id(3), id(4)).is_ok());
    }

    #[test]
    fn test_cycle_detection() {
        let mut graph = DependencyGraph::new();
        graph.add_edge(id(1), id(2), DependencyType::Blocks);
        graph.add_edge(id(2), id(3), DependencyType::Blocks);

        // Adding 3->1 would create a cycle
        assert!(graph.would_create_cycle(id(3), id(1)));
        assert!(graph.validate_new_edge(id(3), id(1)).is_err());
    }

    #[test]
    fn test_self_reference_rejected() {
        let graph = DependencyGraph::new();
        assert!(graph.validate_new_edge(id(1), id(1)).is_err());
    }

    #[test]
    fn test_blockers_of() {
        let mut graph = DependencyGraph::new();
        graph.add_edge(id(1), id(3), DependencyType::Blocks);
        graph.add_edge(id(2), id(3), DependencyType::Blocks);

        let blockers = graph.blockers_of(id(3));
        assert_eq!(blockers.len(), 2);
    }

    #[test]
    fn test_all_downstream() {
        let mut graph = DependencyGraph::new();
        graph.add_edge(id(1), id(2), DependencyType::Blocks);
        graph.add_edge(id(2), id(3), DependencyType::Blocks);
        graph.add_edge(id(2), id(4), DependencyType::Blocks);

        let downstream = graph.all_downstream(id(1));
        assert!(downstream.contains(&id(2)));
        assert!(downstream.contains(&id(3)));
        assert!(downstream.contains(&id(4)));
        assert!(!downstream.contains(&id(1)));
    }

    #[test]
    fn test_from_edges() {
        let edges = vec![
            DependencyEdge {
                source: id(1),
                target: id(2),
                dep_type: DependencyType::Blocks,
            },
            DependencyEdge {
                source: id(2),
                target: id(3),
                dep_type: DependencyType::Blocks,
            },
        ];
        let graph = DependencyGraph::from_edges(&edges);
        assert!(!graph.would_create_cycle(id(3), id(4)));
        assert!(graph.would_create_cycle(id(3), id(1)));
    }
}
