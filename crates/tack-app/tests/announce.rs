//! What a run says to the board's listeners, read from the port: each fact once, with the
//! project and the principal that caused it, and nothing for a report, a question or an answer
//! that was only repeated.

use crate::common;
use tack_protocol::Clock;
use tack_protocol::execution::{AttemptId, CompletionId, DecisionOption, ExecutionState, RunnerId};

use common::{RUNNER_ID, Running, running_attempt, world};
use serde_json::{Value, json};
use tack_app::events::{DomainEvent, Fact, RecordedEvents, RunRef};
use tack_app::runs::announce;
use tack_app::runs::complete::{Completed, CompletionReport, complete};
use tack_app::runs::create_decision::{DecisionCreated, DecisionRequest, create_decision};
use tack_app::runs::resolve_decision::resolve_decision;
use tack_core::decision::DecisionResolvedBy;
use tack_core::principal::Principal;
use tack_protocol::mrp::MergeReadinessPack;
use tack_test_support::completion::{actual_execution, terminal_reason_of, usage_not_measured};
use tack_test_support::seed;
use uuid::Uuid;

async fn project_of(world: &common::World) -> Uuid {
    world
        .repo
        .get_item(Uuid::parse_str(&world.item_id).unwrap())
        .await
        .unwrap()
        .expect("item")
        .project_id
}

fn run_of(world: &common::World, run: &Running) -> RunRef {
    RunRef {
        request_id: run.request_id.clone(),
        attempt_id: run.attempt_id.clone(),
        item_id: Uuid::parse_str(&world.item_id).unwrap(),
    }
}

async fn ask(world: &common::World, run: &Running, decision_id: &str) -> DecisionCreated {
    create_decision(
        &world.repo,
        world.clock.now(),
        DecisionRequest {
            runner_id: &RunnerId::new(RUNNER_ID),
            attempt_id: &AttemptId::new(&run.attempt_id),
            fencing_token: run.fence,
            decision_id,
            kind: "tool_permission",
            prompt: "Allow it?",
            options: &[DecisionOption {
                option_id: "allow".into(),
                label: "Allow".into(),
                description: None,
                risks: None,
                estimated_tokens: None,
            }],
            metadata: &serde_json::Map::new(),
            recommendation: None,
            expires_at: None,
        },
    )
    .await
    .expect("create")
}

async fn end(
    world: &common::World,
    run: &Running,
    completion_id: &str,
    terminal_state: ExecutionState,
    terminal_reason: Value,
) -> Completed {
    async fn no_pack(_reference: String) -> Option<MergeReadinessPack> {
        None
    }
    complete(
        &world.repo,
        world.clock.now(),
        CompletionReport {
            runner_id: &RunnerId::new(RUNNER_ID),
            attempt_id: &AttemptId::new(&run.attempt_id),
            fencing_token: run.fence,
            completion_id: &CompletionId::new(completion_id),
            final_event_checkpoint: None,
            terminal_state,
            terminal_reason: &terminal_reason_of(terminal_reason),
            actual_execution: &actual_execution(),
            usage: &usage_not_measured(),
        },
        no_pack,
    )
    .await
    .expect("complete")
}

#[tokio::test]
async fn a_question_opens_a_decision_and_parks_the_run_once() {
    let world = world().await;
    let run = running_attempt(&world, "announce-ask").await;
    let (project, runner) = (project_of(&world).await, Principal::runner(RUNNER_ID));
    let events = RecordedEvents::default();
    let attempt = AttemptId::new(&run.attempt_id);

    for _ in 0..2 {
        let created = ask(&world, &run, "dec-1").await;
        announce::decision_opened(&world.repo, &events, &runner, &attempt, "dec-1", &created).await;
    }
    let second = ask(&world, &run, "dec-2").await;
    announce::decision_opened(&world.repo, &events, &runner, &attempt, "dec-2", &second).await;

    let reference = run_of(&world, &run);
    let said = |fact| DomainEvent::new(project, &runner, fact);
    assert_eq!(
        events.events(),
        vec![
            said(Fact::DecisionOpened {
                run: reference.clone(),
                decision_id: "dec-1".into()
            }),
            said(Fact::RunWaiting {
                run: reference.clone()
            }),
            said(Fact::DecisionOpened {
                run: reference,
                decision_id: "dec-2".into()
            }),
        ],
        "the repeat said nothing and the second question did not park a run already parked"
    );
}

#[tokio::test]
async fn an_answer_is_announced_once_with_the_person_who_gave_it() {
    let world = world().await;
    let run = running_attempt(&world, "announce-answer").await;
    let project = project_of(&world).await;
    let (person, runner) = (Principal::operator("local"), Principal::runner(RUNNER_ID));
    let events = RecordedEvents::default();
    let attempt = AttemptId::new(&run.attempt_id);
    ask(&world, &run, "dec-1").await;

    for _ in 0..2 {
        let outcome = resolve_decision(
            &world.repo,
            world.clock.now(),
            &attempt,
            "dec-1",
            &json!({"option_id": "allow", "text": null}),
            &DecisionResolvedBy::operator(person.label()),
        )
        .await
        .expect("answer");
        announce::decision_resolved(&world.repo, &events, &person, &attempt, "dec-1", &outcome)
            .await;
    }

    assert_eq!(
        events.events(),
        vec![DomainEvent::new(
            project,
            &person,
            Fact::DecisionResolved {
                run: run_of(&world, &run),
                decision_id: "dec-1".into()
            }
        )]
    );
    assert_ne!(person, runner);
}

#[tokio::test]
async fn a_started_run_and_the_move_it_caused_are_two_events_with_the_runners_principal() {
    let world = world().await;
    let run = running_attempt(&world, "announce-start").await;
    let (project, runner) = (project_of(&world).await, Principal::runner(RUNNER_ID));
    let events = RecordedEvents::default();
    let moved = tack_app::runs::complete::ItemMoved {
        project_id: project,
        item_id: Uuid::parse_str(&world.item_id).unwrap(),
        old_status: "Todo".into(),
        new_status: "In Progress".into(),
    };

    announce::run_started(
        &world.repo,
        &events,
        &runner,
        &AttemptId::new(&run.attempt_id),
        Some(&moved),
    )
    .await;

    let item_id = moved.item_id;
    assert_eq!(
        events.events(),
        vec![
            DomainEvent::new(
                project,
                &runner,
                Fact::RunStarted {
                    run: run_of(&world, &run)
                }
            ),
            DomainEvent::new(
                project,
                &runner,
                Fact::ItemMoved {
                    item_id,
                    from: "Todo".into(),
                    to: "In Progress".into()
                }
            ),
        ]
    );
}

#[tokio::test]
async fn a_success_finishes_a_failure_fails_and_a_replay_or_a_cancellation_says_nothing() {
    for (state, expected) in [
        (ExecutionState::Succeeded, Some("finished")),
        (ExecutionState::Failed, Some("failed")),
        (ExecutionState::Cancelled, None),
    ] {
        let world = world().await;
        let run = running_attempt(&world, "announce-end").await;
        let (project, runner) = (project_of(&world).await, Principal::runner(RUNNER_ID));
        let events = RecordedEvents::default();
        let attempt = AttemptId::new(&run.attempt_id);

        for _ in 0..2 {
            let completed = end(&world, &run, "done-1", state, json!({})).await;
            announce::run_ended(&world.repo, &events, &runner, &attempt, &completed).await;
        }

        let reference = run_of(&world, &run);
        let expected = expected.map(|word| {
            DomainEvent::new(
                project,
                &runner,
                match word {
                    "finished" => Fact::RunFinished { run: reference },
                    _ => Fact::RunFailed { run: reference },
                },
            )
        });
        assert_eq!(events.events(), expected.into_iter().collect::<Vec<_>>());
    }
}

#[tokio::test]
async fn a_success_that_moves_the_item_says_so_after_the_run_finished() {
    let world = world().await;
    let project = project_of(&world).await;
    seed::project_on_finish_status(&world.repo, &project.to_string(), "In Progress").await;
    let run = running_attempt(&world, "announce-move").await;
    let runner = Principal::runner(RUNNER_ID);
    let events = RecordedEvents::default();

    let completed = end(
        &world,
        &run,
        "done-1",
        ExecutionState::Succeeded,
        json!({"artifacts": [{"kind": "patch", "size_bytes": 12}]}),
    )
    .await;
    announce::run_ended(
        &world.repo,
        &events,
        &runner,
        &AttemptId::new(&run.attempt_id),
        &completed,
    )
    .await;

    let facts: Vec<Fact> = events.events().into_iter().map(|e| e.fact).collect();
    assert!(matches!(facts[0], Fact::RunFinished { .. }));
    assert!(matches!(&facts[1], Fact::ItemMoved { to, .. } if to == "In Progress"));
    assert_eq!(facts.len(), 2);
}
