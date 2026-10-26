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

#[test]
fn unfinished_blockers_ignores_a_done_blocker() {
    let mut graph = DependencyGraph::new();
    graph.add_edge(id(1), id(3), DependencyType::Blocks);
    graph.add_edge(id(2), id(3), DependencyType::Blocks);
    let workflow = crate::workflow::simple_workflow();
    let done = workflow.find_first_done_status().unwrap().to_owned();
    let status_of = |item: Uuid| (item == id(1)).then(|| done.clone());
    // id(1) is Done; id(2) has no known status and so still blocks.
    assert_eq!(
        graph.unfinished_blockers(id(3), &status_of, &workflow),
        vec![id(2)]
    );
    assert!(
        graph
            .unfinished_blockers(id(1), &status_of, &workflow)
            .is_empty()
    );
}
