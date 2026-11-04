//! What wakes a profile (ADR 0034, decision 8): a burst of events starts one run that lists
//! them all, an event the profile's own principal caused starts nothing, the daily cap and
//! the schedule bound the runs.

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use tack_app::events::{DomainEvent, Fact, RunRef};
use tack_app::runs::agent_credential::issue;
use tack_app::triggers::{self, OpenTrigger};
use tack_core::change_set::{CreateOp, Operation, Origin};
use tack_core::principal::{Capability, Principal, Scope};
use tack_core::thread::ThreadScope;
use tack_core::trigger::{EventKind, Filter, Schedule, Source, Trigger};
use tack_db::repo::change_sets::NewChangeSet;
use tack_db::{Repository, queries};
use tack_protocol::Clock;
use tack_test_support::{ControllableClock, seed};
use uuid::Uuid;

use crate::support::{Board, board, set_project};
use tack_core::models::UpdateProject;

struct FakeClock(ControllableClock);

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        self.0.now()
    }
}

fn clock() -> FakeClock {
    FakeClock(ControllableClock::new(Utc::now()))
}

async fn board_with_harness() -> Board {
    let board = board().await;
    set_project(
        &board.repo,
        board.project_id,
        UpdateProject {
            default_harness: Some(Some("codex".into())),
            ..UpdateProject::default()
        },
    )
    .await;
    board
}

fn failed_run(board: &Board, n: usize, by: &Principal) -> DomainEvent {
    DomainEvent::new(
        board.project_id,
        by,
        Fact::RunFailed {
            run: RunRef {
                request_id: format!("run-{n}"),
                attempt_id: format!("att-{n}"),
                item_id: board.item_id,
            },
        },
    )
}

fn on_failed_runs() -> Source {
    Source::Event {
        event: EventKind::RunFailed,
        filter: Filter::default(),
    }
}

async fn trigger(
    board: &Board,
    clock: &FakeClock,
    name: &str,
    source: Source,
    cap: u32,
    credential: Option<Uuid>,
) -> Trigger {
    let scope = match credential {
        Some(credential) => ThreadScope::Credential {
            credential,
            home: board.project_id,
        },
        None => ThreadScope::Project {
            project: board.project_id,
        },
    };
    triggers::create(
        &board.repo,
        clock,
        OpenTrigger {
            scope,
            profile: "profile-c1".into(),
            name: name.into(),
            source,
            quiet_seconds: 60,
            daily_cap: cap,
            enabled: true,
        },
    )
    .await
    .expect("trigger")
}

async fn manager_credential(board: &Board, clock: &FakeClock) -> Uuid {
    issue(
        &board.repo,
        clock.now(),
        "Manager",
        vec![Scope {
            all_projects: true,
            projects: vec![],
            capabilities: vec![Capability::Read, Capability::Propose],
        }],
    )
    .await
    .expect("credential")
    .credential
    .id
}

async fn input_of(repo: &Repository, trigger: &Trigger) -> String {
    let request = repo
        .last_trigger_request(trigger.id)
        .await
        .unwrap()
        .expect("a run");
    let raw = queries::execution_requests::request_snapshot(repo, &request)
        .await
        .unwrap()
        .unwrap();
    let snapshot: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        snapshot["created_by"]["subject_id"],
        format!("trigger:{}", trigger.id)
    );
    snapshot["task"]["description"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn a_burst_of_ten_failed_runs_starts_one_run_listing_all_ten() {
    let board = board_with_harness().await;
    let clock = clock();
    let trigger = trigger(&board, &clock, "Failed runs", on_failed_runs(), 5, None).await;
    let runner = Principal::runner("runner-active");
    for n in 0..10 {
        let kept = triggers::observe(&board.repo, &clock, &failed_run(&board, n, &runner))
            .await
            .unwrap();
        assert_eq!(kept, 1);
        clock.0.advance(Duration::seconds(2));
    }
    // Still inside the quiet window: nothing yet.
    assert_eq!(triggers::tick(&board.repo, &clock).await.unwrap(), 0);
    clock.0.advance(Duration::seconds(61));
    assert_eq!(triggers::tick(&board.repo, &clock).await.unwrap(), 1);
    let input = input_of(&board.repo, &trigger).await;
    assert!(input.contains("10 events"), "{input}");
    assert_eq!(input.matches("a run failed").count(), 10, "{input}");
    assert!(input.contains("Board digest"), "{input}");
    // The events were taken: the next pass has nothing to start.
    clock.0.advance(Duration::minutes(5));
    assert_eq!(triggers::tick(&board.repo, &clock).await.unwrap(), 0);
    // And the run is not on the board.
    let listed = board
        .repo
        .list_items(board.project_id, &Default::default())
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
}

#[tokio::test]
async fn what_the_manager_itself_did_does_not_wake_it() {
    let board = board_with_harness().await;
    let clock = clock();
    let credential = manager_credential(&board, &clock).await;
    trigger(
        &board,
        &clock,
        "Moves",
        Source::Event {
            event: EventKind::ItemMoved,
            filter: Filter::default(),
        },
        5,
        Some(credential),
    )
    .await;
    let moved = |by: &Principal| {
        DomainEvent::new(
            board.project_id,
            by,
            Fact::ItemMoved {
                item_id: board.item_id,
                from: "Backlog".into(),
                to: "Ready".into(),
            },
        )
    };
    let manager = Principal::agent(credential.to_string(), vec![]);
    assert_eq!(
        triggers::observe(&board.repo, &clock, &moved(&manager))
            .await
            .unwrap(),
        0,
        "a change set the manager applied"
    );
    // A person's change does wake it.
    assert_eq!(
        triggers::observe(&board.repo, &clock, &moved(&Principal::operator("local")))
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn the_daily_cap_stops_a_second_run() {
    let board = board_with_harness().await;
    let clock = clock();
    let trigger = trigger(&board, &clock, "Failed runs", on_failed_runs(), 1, None).await;
    let runner = Principal::runner("runner-active");
    triggers::observe(&board.repo, &clock, &failed_run(&board, 1, &runner))
        .await
        .unwrap();
    clock.0.advance(Duration::seconds(61));
    assert_eq!(triggers::tick(&board.repo, &clock).await.unwrap(), 1);
    // The first run ends; a second burst is ready, but the cap is used up.
    let request = board
        .repo
        .last_trigger_request(trigger.id)
        .await
        .unwrap()
        .unwrap();
    seed::statement(
        &board.repo,
        "UPDATE execution_requests SET state = 'succeeded' WHERE id = ?",
        &[&request],
    )
    .await;
    triggers::observe(&board.repo, &clock, &failed_run(&board, 2, &runner))
        .await
        .unwrap();
    clock.0.advance(Duration::seconds(61));
    assert_eq!(triggers::tick(&board.repo, &clock).await.unwrap(), 0);
    // A day later it starts, with the event that waited.
    clock.0.advance(Duration::hours(24));
    assert_eq!(triggers::tick(&board.repo, &clock).await.unwrap(), 1);
    assert!(input_of(&board.repo, &trigger).await.contains("1 event"));
}

#[tokio::test]
async fn a_run_still_going_is_not_started_again() {
    let board = board_with_harness().await;
    let clock = clock();
    trigger(&board, &clock, "Failed runs", on_failed_runs(), 5, None).await;
    let runner = Principal::runner("runner-active");
    triggers::observe(&board.repo, &clock, &failed_run(&board, 1, &runner))
        .await
        .unwrap();
    clock.0.advance(Duration::seconds(61));
    assert_eq!(triggers::tick(&board.repo, &clock).await.unwrap(), 1);
    triggers::observe(&board.repo, &clock, &failed_run(&board, 2, &runner))
        .await
        .unwrap();
    clock.0.advance(Duration::seconds(61));
    assert_eq!(triggers::tick(&board.repo, &clock).await.unwrap(), 0);
}

#[tokio::test]
async fn a_schedule_starts_a_run_once_per_moment_with_the_digest() {
    let board = board_with_harness().await;
    let clock = clock();
    clock.0.set(
        DateTime::parse_from_rfc3339("2026-11-02T07:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
    );
    let trigger = trigger(
        &board,
        &clock,
        "Stand-up",
        Source::Schedule(Schedule {
            days: vec![1, 2, 3, 4, 5],
            minute_of_day: 9 * 60,
            utc_offset_minutes: 0,
        }),
        1,
        None,
    )
    .await;
    assert_eq!(triggers::tick(&board.repo, &clock).await.unwrap(), 0);
    clock.0.set(
        DateTime::parse_from_rfc3339("2026-11-02T09:00:30Z")
            .unwrap()
            .with_timezone(&Utc),
    );
    assert_eq!(triggers::tick(&board.repo, &clock).await.unwrap(), 1);
    let input = input_of(&board.repo, &trigger).await;
    assert!(input.contains("no event started you"), "{input}");
    assert!(input.contains("Board digest"), "{input}");
    clock.0.advance(Duration::minutes(10));
    assert_eq!(triggers::tick(&board.repo, &clock).await.unwrap(), 0);
}

#[tokio::test]
async fn a_change_set_waiting_too_long_is_noticed_once() {
    let board = board_with_harness().await;
    let clock = clock();
    let trigger = trigger(
        &board,
        &clock,
        "Waiting",
        Source::Event {
            event: EventKind::ChangeSetWaiting,
            filter: Filter {
                status: None,
                after_seconds: Some(3_600),
            },
        },
        5,
        None,
    )
    .await;
    board
        .repo
        .insert_change_set(&NewChangeSet {
            project: board.project_id,
            principal: "agent:manager",
            origin: Origin::Thread,
            origin_ref: None,
            reason: "tidy the board",
            idempotency_key: None,
            anchor: None,
            artifact_id: None,
            operations: vec![Operation::Create(CreateOp {
                title: "Half".into(),
                description: None,
                acceptance: vec!["done".into()],
                depends_on: Vec::new(),
                item_type: None,
                role: None,
                team: None,
                profile_id: None,
                parent: None,
            })],
            now: clock.now(),
        })
        .await
        .unwrap();
    clock.0.advance(Duration::minutes(30));
    assert_eq!(triggers::tick(&board.repo, &clock).await.unwrap(), 0);
    clock.0.advance(Duration::minutes(40));
    // Noticed now; its quiet period starts from the notice.
    assert_eq!(triggers::tick(&board.repo, &clock).await.unwrap(), 0);
    clock.0.advance(Duration::seconds(61));
    assert_eq!(triggers::tick(&board.repo, &clock).await.unwrap(), 1);
    assert!(
        input_of(&board.repo, &trigger)
            .await
            .contains("a change set is waiting")
    );
    // Not noticed a second time.
    clock.0.advance(Duration::hours(2));
    assert_eq!(triggers::tick(&board.repo, &clock).await.unwrap(), 0);
    assert!(
        board
            .repo
            .pending_trigger_events(trigger.id)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_trigger_must_name_a_profile_that_exists_and_say_how_it_waits() {
    let board = board_with_harness().await;
    let clock = clock();
    let bad = |profile: &str, source: Source| OpenTrigger {
        scope: ThreadScope::Project {
            project: board.project_id,
        },
        profile: profile.into(),
        name: "x".into(),
        source,
        quiet_seconds: 0,
        daily_cap: 1,
        enabled: true,
    };
    assert!(
        triggers::create(&board.repo, &clock, bad("missing", on_failed_runs()))
            .await
            .is_err()
    );
    let waiting_without_time = Source::Event {
        event: EventKind::ChangeSetWaiting,
        filter: Filter::default(),
    };
    assert!(
        triggers::create(&board.repo, &clock, bad("profile-c1", waiting_without_time))
            .await
            .is_err()
    );
    let made = triggers::create(&board.repo, &clock, bad("profile-c1", on_failed_runs()))
        .await
        .unwrap();
    let edited = triggers::edit(
        &board.repo,
        made.id,
        triggers::EditTrigger {
            quiet_seconds: Some(120),
            enabled: Some(false),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!((edited.quiet_seconds, edited.enabled), (120, false));
    triggers::delete(&board.repo, made.id).await.unwrap();
    assert!(board.repo.get_trigger(made.id).await.unwrap().is_none());
}

#[tokio::test]
async fn a_change_set_proposed_or_settled_is_an_event_a_trigger_can_wait_for() {
    let board = board_with_harness().await;
    let clock = clock();
    let proposed = |event| Source::Event {
        event,
        filter: Filter::default(),
    };
    trigger(
        &board,
        &clock,
        "Proposed",
        proposed(EventKind::ChangeSetProposed),
        5,
        None,
    )
    .await;
    trigger(
        &board,
        &clock,
        "Settled",
        proposed(EventKind::ChangeSetSettled),
        5,
        None,
    )
    .await;
    let agent = Principal::agent("credential-9", Vec::new());
    let said = |fact| DomainEvent::new(board.project_id, &agent, fact);

    let kept = triggers::observe(
        &board.repo,
        &clock,
        &said(Fact::ChangeSetProposed {
            change_set_id: "set-1".into(),
            item_id: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(kept, 1, "only the trigger for proposals keeps it");

    let kept = triggers::observe(
        &board.repo,
        &clock,
        &said(Fact::ChangeSetSettled {
            change_set_id: "set-1".into(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(kept, 1, "only the trigger for settled sets keeps it");
}
