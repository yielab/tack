//! Change sets (ADR 0034, decisions 3 and 4): checked whole, applied in one transaction, decided
//! operation by operation by the project's rung and the credential's scopes, and answered again
//! for the same idempotency key.

use tack_app::AppError;
use tack_app::change_sets::{Application, Mode, Proposal, Proposed, apply, propose, reject};
use tack_app::events::{DomainEvent, Fact, RecordedEvents};
use tack_core::change_set::{
    AgentsOnBoard, ChangeSetStatus, CreateOp, ItemRef, OpState, Operation, Origin, RungLevel,
};
use tack_core::models::{Priority, Project, UpdateProject};
use tack_core::principal::{Capability, Principal, Scope};
use tack_db::Repository;
use tack_protocol::SystemClock;
use tack_test_support::{create_test_workspace, make_item, make_project, setup_test_db};
use uuid::Uuid;

struct Board {
    repo: Repository,
    events: RecordedEvents,
    project: Project,
    item: Uuid,
}

async fn board(rung: AgentsOnBoard) -> Board {
    let repo = setup_test_db().await;
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await;
    let item = make_item(&repo, &project).await.id;
    let project = set_rung(&repo, project.id, rung).await;
    Board {
        repo,
        events: RecordedEvents::default(),
        project,
        item,
    }
}

async fn set_rung(repo: &Repository, project: Uuid, rung: AgentsOnBoard) -> Project {
    repo.update_project(
        project,
        UpdateProject {
            agents_on_board: Some(rung),
            ..UpdateProject::default()
        },
    )
    .await
    .expect("update")
    .expect("project")
}

fn rung(level: RungLevel, per_set: u32, per_day: u32) -> AgentsOnBoard {
    AgentsOnBoard {
        level,
        creates_per_change_set: per_set,
        creates_per_day: per_day,
    }
}

fn agent(project: Uuid, capabilities: &[Capability]) -> Principal {
    Principal::agent(
        "credential-1",
        vec![Scope {
            all_projects: false,
            projects: vec![project],
            capabilities: capabilities.to_vec(),
        }],
    )
}

fn create(title: &str) -> Operation {
    Operation::Create(CreateOp {
        title: title.into(),
        description: None,
        acceptance: vec!["it works".into()],
        depends_on: Vec::new(),
        item_type: None,
        role: None,
        team: None,
        profile_id: None,
        parent: None,
    })
}

fn proposal<'a>(
    board: &Board,
    principal: &'a Principal,
    key: Option<&str>,
    operations: Vec<Operation>,
) -> Proposal<'a> {
    Proposal {
        project: board.project.id,
        principal,
        origin: Origin::Mcp,
        origin_ref: None,
        reason: "The launch needs it".into(),
        idempotency_key: key.map(str::to_owned),
        anchor: None,
        artifact_id: None,
        operations,
        mode: Mode::Rung,
    }
}

/// Two creates, a priority change and a comment on the first create.
fn mixed(item: Uuid) -> Vec<Operation> {
    vec![
        create("first"),
        create("second"),
        Operation::Priority {
            item: ItemRef::Existing(item),
            priority: Priority::High,
        },
        Operation::Comment {
            item: ItemRef::New(0),
            text: "about the first".into(),
        },
    ]
}

async fn titles(repo: &Repository, project: Uuid) -> Vec<String> {
    let mut found: Vec<String> = repo
        .get_item_tree(project)
        .await
        .expect("tree")
        .into_iter()
        .map(|item| item.title)
        .collect();
    found.sort();
    found
}

/// What the board's listeners were told of a change set, in order.
fn told(board: &Board) -> Vec<Fact> {
    board
        .events
        .events()
        .into_iter()
        .map(|DomainEvent { fact, .. }| fact)
        .collect()
}

fn proposed_fact(id: Uuid) -> Fact {
    Fact::ChangeSetProposed {
        change_set_id: id.to_string(),
        item_id: None,
    }
}

fn settled_fact(id: Uuid) -> Fact {
    Fact::ChangeSetSettled {
        change_set_id: id.to_string(),
    }
}

fn states(proposed: &Proposed) -> Vec<OpState> {
    proposed.change_set.ops.iter().map(|op| op.state).collect()
}

#[tokio::test]
async fn at_propose_every_change_set_waits_and_nothing_is_written() {
    let board = board(AgentsOnBoard::default()).await;
    let principal = agent(board.project.id, &[Capability::Act]);
    let proposed = propose(
        &board.repo,
        &SystemClock,
        &board.events,
        proposal(&board, &principal, None, mixed(board.item)),
    )
    .await
    .expect("propose");
    assert!(proposed.applied_now.is_empty());
    assert_eq!(proposed.change_set.status, ChangeSetStatus::Waiting);
    assert!(states(&proposed).iter().all(|s| *s == OpState::Waiting));
    assert_eq!(titles(&board.repo, board.project.id).await, ["Test Item"]);
    assert_eq!(
        told(&board),
        [proposed_fact(proposed.change_set.id)],
        "a change set that waits is announced as proposed"
    );
    assert_eq!(board.events.events()[0].principal, principal);
}

#[tokio::test]
async fn at_act_creates_wait_and_the_rest_applies() {
    let board = board(rung(RungLevel::Act, 5, 5)).await;
    let principal = agent(board.project.id, &[Capability::Act]);
    let proposed = propose(
        &board.repo,
        &SystemClock,
        &board.events,
        proposal(&board, &principal, None, mixed(board.item)),
    )
    .await
    .expect("propose");
    // The priority change applies; the comment needs the first create, so it waits with it.
    assert_eq!(proposed.applied_now, [2]);
    assert_eq!(
        states(&proposed),
        [
            OpState::Waiting,
            OpState::Waiting,
            OpState::Applied,
            OpState::Waiting
        ]
    );
    assert_eq!(proposed.change_set.status, ChangeSetStatus::Partly);
    let item = board.repo.get_item(board.item).await.unwrap().unwrap();
    assert_eq!(item.priority, Priority::High);
    assert_eq!(titles(&board.repo, board.project.id).await, ["Test Item"]);
    assert_eq!(
        told(&board),
        [proposed_fact(proposed.change_set.id)],
        "what the rung let through does not hide what still waits"
    );
}

#[tokio::test]
async fn at_create_creates_apply_within_the_caps_and_wait_beyond_them() {
    let board = board(rung(RungLevel::Create, 1, 3)).await;
    let principal = agent(board.project.id, &[Capability::Act]);
    let first = propose(
        &board.repo,
        &SystemClock,
        &board.events,
        proposal(&board, &principal, None, mixed(board.item)),
    )
    .await
    .expect("propose");
    // One create per change set: the first lands with its comment, the second waits.
    assert_eq!(first.applied_now, [0, 2, 3]);
    assert_eq!(first.change_set.status, ChangeSetStatus::Partly);
    assert_eq!(
        titles(&board.repo, board.project.id).await,
        ["Test Item", "first"]
    );
    let created = first.change_set.ops[0]
        .result
        .as_ref()
        .unwrap()
        .item
        .unwrap();
    let comments = board.repo.list_comments(created).await.unwrap();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].principal.as_deref(), Some("agent:credential-1"));

    // The day's cap is 3: two more single creates land, the third finds the day used up.
    for (n, lands) in [(1, true), (2, true), (3, false)] {
        let proposed = propose(
            &board.repo,
            &SystemClock,
            &board.events,
            proposal(&board, &principal, None, vec![create(&format!("more {n}"))]),
        )
        .await
        .expect("propose");
        // The first set used one of the day's three.
        assert_eq!(proposed.applied_now.len() == 1, lands, "create {n}");
    }
}

#[tokio::test]
async fn create_with_no_caps_behaves_as_act() {
    let board = board(rung(RungLevel::Create, 0, 0)).await;
    let principal = agent(board.project.id, &[Capability::Act]);
    let proposed = propose(
        &board.repo,
        &SystemClock,
        &board.events,
        proposal(&board, &principal, None, mixed(board.item)),
    )
    .await
    .expect("propose");
    assert_eq!(proposed.applied_now, [2]);
}

#[tokio::test]
async fn the_stricter_of_the_rung_and_the_credential_wins() {
    let board = board(rung(RungLevel::Create, 5, 5)).await;
    let proposer = agent(board.project.id, &[Capability::Propose]);
    let proposed = propose(
        &board.repo,
        &SystemClock,
        &board.events,
        proposal(&board, &proposer, None, mixed(board.item)),
    )
    .await
    .expect("propose");
    assert!(proposed.applied_now.is_empty());

    let reader = agent(board.project.id, &[Capability::Read]);
    let refused = propose(
        &board.repo,
        &SystemClock,
        &board.events,
        proposal(&board, &reader, None, mixed(board.item)),
    )
    .await;
    assert!(matches!(refused, Err(AppError::Invalid(_))));

    set_rung(&board.repo, board.project.id, rung(RungLevel::Read, 0, 0)).await;
    let actor = agent(board.project.id, &[Capability::Act]);
    let refused = propose(
        &board.repo,
        &SystemClock,
        &board.events,
        proposal(&board, &actor, None, mixed(board.item)),
    )
    .await;
    assert!(matches!(refused, Err(AppError::Invalid(text)) if text.contains("read and talk only")));
}

#[tokio::test]
async fn a_repeated_key_returns_the_first_answer() {
    let board = board(rung(RungLevel::Act, 5, 5)).await;
    let principal = agent(board.project.id, &[Capability::Act]);
    let first = propose(
        &board.repo,
        &SystemClock,
        &board.events,
        proposal(&board, &principal, Some("k-1"), mixed(board.item)),
    )
    .await
    .expect("propose");
    assert!(first.made);
    // Same key, different operations: still the first change set, and nothing new is written.
    let again = propose(
        &board.repo,
        &SystemClock,
        &board.events,
        proposal(&board, &principal, Some("k-1"), vec![create("other")]),
    )
    .await
    .expect("propose again");
    assert!(!again.made);
    assert_eq!(again.change_set.id, first.change_set.id);
    assert_eq!(again.change_set.ops.len(), 4);
    let all = board
        .repo
        .list_change_sets(board.project.id, &[], 10)
        .await
        .unwrap();
    assert_eq!(all.len(), 1);
}

#[tokio::test]
async fn a_cycle_a_missing_status_and_a_full_column_are_refused_whole_with_every_reason() {
    let board = board(AgentsOnBoard::default()).await;
    let workspace = board.project.workspace_id;
    let _ = workspace;
    // "In Review" holds three items and its limit is three.
    let mut workflow = board.project.workflow.clone();
    for status in workflow.statuses.iter_mut() {
        if status.name == "In Review" {
            status.wip_limit = Some(1);
        }
    }
    board
        .repo
        .update_project(
            board.project.id,
            UpdateProject {
                workflow: Some(workflow),
                ..UpdateProject::default()
            },
        )
        .await
        .unwrap();
    let occupant = make_item(&board.repo, &board.project).await;
    tack_test_support::update_item(
        &board.repo,
        occupant.id,
        tack_core::models::UpdateItem {
            status: Some("In Review".into()),
            ..Default::default()
        },
    )
    .await;
    let other = make_item(&board.repo, &board.project).await;
    let operations = vec![
        Operation::Depend {
            blocker: ItemRef::Existing(board.item),
            blocked: ItemRef::Existing(other.id),
        },
        Operation::Depend {
            blocker: ItemRef::Existing(other.id),
            blocked: ItemRef::Existing(board.item),
        },
        Operation::Move {
            item: ItemRef::Existing(board.item),
            status: "Nowhere".into(),
        },
        Operation::Move {
            item: ItemRef::Existing(other.id),
            status: "In Review".into(),
        },
    ];
    let principal = agent(board.project.id, &[Capability::Propose]);
    let refused = propose(
        &board.repo,
        &SystemClock,
        &board.events,
        proposal(&board, &principal, Some("k"), operations),
    )
    .await;
    let Err(AppError::Invalid(text)) = refused else {
        panic!("expected a refusal, got {refused:?}");
    };
    assert!(text.contains("3 reasons"), "{text}");
    assert!(
        text.contains("operation 1") && text.contains("loop"),
        "{text}"
    );
    assert!(
        text.contains("operation 2") && text.contains("Nowhere"),
        "{text}"
    );
    assert!(
        text.contains("operation 3") && text.contains("WIP"),
        "{text}"
    );
    // Refused whole: nothing is kept, not even a waiting change set, and the key is free.
    assert!(
        board
            .repo
            .list_change_sets(board.project.id, &[], 10)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        board
            .repo
            .get_change_set_by_key(board.project.id, "k")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn a_person_applies_part_of_a_change_set_with_an_edit_and_rejects_the_rest() {
    let board = board(AgentsOnBoard::default()).await;
    let principal = agent(board.project.id, &[Capability::Propose]);
    let operator = Principal::operator("local");
    let proposed = propose(
        &board.repo,
        &SystemClock,
        &board.events,
        proposal(&board, &principal, None, mixed(board.item)),
    )
    .await
    .expect("propose");
    let id = proposed.change_set.id;
    assert_eq!(told(&board), [proposed_fact(id)]);

    // Applying a comment without the create it names is refused, and the set is as it was.
    let refused = apply(
        &board.repo,
        &SystemClock,
        &board.events,
        id,
        &operator,
        Application {
            operations: Some(vec![3]),
            edits: Vec::new(),
        },
    )
    .await;
    assert!(matches!(refused, Err(AppError::Invalid(text)) if text.contains("not applied")));

    // Create the first (with its title edited) and the comment; leave the second and the priority.
    let edited = apply(
        &board.repo,
        &SystemClock,
        &board.events,
        id,
        &operator,
        Application {
            operations: Some(vec![0, 3]),
            edits: vec![(0, create("first, edited"))],
        },
    )
    .await
    .expect("apply part");
    assert_eq!(edited.status, ChangeSetStatus::Partly);
    assert_eq!(edited.decided_by.as_deref(), Some("operator:local"));
    assert_eq!(
        told(&board),
        [proposed_fact(id)],
        "part of it still waits, so it is not settled"
    );
    assert_eq!(
        titles(&board.repo, board.project.id).await,
        ["Test Item", "first, edited"]
    );
    // The writes carry the proposer, whoever applied them.
    let made = edited.ops[0].result.as_ref().unwrap().item.unwrap();
    let card = board
        .repo
        .get_item_snapshot(made)
        .await
        .unwrap()
        .unwrap()
        .item;
    assert_eq!(card.created_by.as_deref(), Some("agent"));

    // The applied operations cannot be applied twice.
    let twice = apply(
        &board.repo,
        &SystemClock,
        &board.events,
        id,
        &operator,
        Application {
            operations: Some(vec![0]),
            edits: Vec::new(),
        },
    )
    .await;
    assert!(matches!(twice, Err(AppError::Conflict(_))));

    let rejected = reject(
        &board.repo,
        &SystemClock,
        &board.events,
        id,
        &operator,
        "Not this sprint",
    )
    .await
    .expect("reject");
    assert_eq!(rejected.status, ChangeSetStatus::Partly);
    assert_eq!(rejected.rejection.as_deref(), Some("Not this sprint"));
    assert_eq!(told(&board), [proposed_fact(id), settled_fact(id)]);
    assert_eq!(
        rejected.ops.iter().map(|op| op.state).collect::<Vec<_>>(),
        [
            OpState::Applied,
            OpState::Rejected,
            OpState::Rejected,
            OpState::Applied
        ]
    );
    // Nothing waits any more.
    let nothing = reject(
        &board.repo,
        &SystemClock,
        &board.events,
        id,
        &operator,
        "again",
    )
    .await;
    assert!(matches!(nothing, Err(AppError::Conflict(_))));
    let item = board.repo.get_item(board.item).await.unwrap().unwrap();
    assert_ne!(item.priority, Priority::High);
}

#[tokio::test]
async fn a_change_set_gone_stale_is_refused_whole_when_applied() {
    let board = board(AgentsOnBoard::default()).await;
    let principal = agent(board.project.id, &[Capability::Propose]);
    let operator = Principal::operator("local");
    let proposed = propose(
        &board.repo,
        &SystemClock,
        &board.events,
        proposal(
            &board,
            &principal,
            None,
            vec![
                create("made"),
                Operation::Move {
                    item: ItemRef::Existing(board.item),
                    status: "Done".into(),
                },
            ],
        ),
    )
    .await
    .expect("propose");
    // The item is deleted before a person gets to it.
    board.repo.delete_item(board.item).await.unwrap();
    let refused = apply(
        &board.repo,
        &SystemClock,
        &board.events,
        proposed.change_set.id,
        &operator,
        Application::default(),
    )
    .await;
    assert!(
        matches!(refused, Err(AppError::Invalid(text)) if text.contains("not in this project"))
    );
    // The create before it did not land either.
    assert!(titles(&board.repo, board.project.id).await.is_empty());
    let kept = board
        .repo
        .get_change_set(proposed.change_set.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(kept.status, ChangeSetStatus::Waiting);
}

#[tokio::test]
async fn a_persons_plan_applies_at_once_or_not_at_all() {
    let board = board(AgentsOnBoard::default()).await;
    let operator = Principal::operator("local");
    let mut plan = proposal(&board, &operator, None, vec![create("a"), create("b")]);
    plan.mode = Mode::Now;
    plan.origin = Origin::Run;
    let proposed = propose(&board.repo, &SystemClock, &board.events, plan)
        .await
        .expect("plan");
    assert_eq!(proposed.change_set.status, ChangeSetStatus::Applied);
    assert_eq!(
        titles(&board.repo, board.project.id).await,
        ["Test Item", "a", "b"]
    );
    assert!(
        told(&board).is_empty(),
        "a set that never waited is not announced"
    );
    // The daily count of the rung is not spent by a person's own plan.
    assert_eq!(
        board
            .repo
            .auto_creates_since(board.project.id, chrono::DateTime::UNIX_EPOCH)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn applying_the_last_of_what_waits_settles_the_change_set() {
    let board = board(AgentsOnBoard::default()).await;
    let principal = agent(board.project.id, &[Capability::Propose]);
    let operator = Principal::operator("local");
    let proposed = propose(
        &board.repo,
        &SystemClock,
        &board.events,
        proposal(&board, &principal, None, vec![create("a"), create("b")]),
    )
    .await
    .expect("propose");
    let id = proposed.change_set.id;

    apply(
        &board.repo,
        &SystemClock,
        &board.events,
        id,
        &operator,
        Application {
            operations: Some(vec![0]),
            edits: Vec::new(),
        },
    )
    .await
    .expect("apply the first");
    assert_eq!(told(&board), [proposed_fact(id)]);

    apply(
        &board.repo,
        &SystemClock,
        &board.events,
        id,
        &operator,
        Application::default(),
    )
    .await
    .expect("apply the rest");
    assert_eq!(told(&board), [proposed_fact(id), settled_fact(id)]);
    assert_eq!(board.events.events()[1].principal, operator);
}
