//! What feeds the inbox (ADR 0035, decision 3): the events the bus carries open and close
//! notifications, an event the bus lost is repaired from the store, and the reconcile makes the
//! needs-you rows say what the store says, once.

use std::sync::Arc;
use std::time::Duration;

use tack_app::background::{Loops, STOP_GRACE};
use tack_app::change_sets::{Mode, Proposal, propose, reject};
use tack_app::events::{DomainEvent, EventBus, Fact, RecordedEvents, RunRef};
use tack_app::notifications::Inbox;
use tack_app::notifications::subscriber::{
    Reconciled, drain, reconcile, reconcile_memory, spawn_every,
};
use tack_app::ports::DomainEvents;
use tack_app::runs::answered_elsewhere::answered_elsewhere;
use tack_core::change_set::{CreateOp, Operation, Origin};
use tack_core::models::CreateRole;
use tack_core::notification::{Key, Kind, Level, Notice};
use tack_core::principal::{Capability, Principal, Scope};
use tack_protocol::Clock;
use tack_protocol::execution::{AttemptId, ExecutionState};
use tack_test_support::{FixedClock, poll_until, seed};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::common::{self, RUNNER_ID, Running, World, enqueue, running_attempt, world};

struct Fixture {
    world: World,
    inbox: Inbox,
    project: Uuid,
    item: Uuid,
}

async fn fixture() -> Fixture {
    let world = world().await;
    let item = Uuid::parse_str(&world.item_id).unwrap();
    let project = world.repo.get_item(item).await.unwrap().unwrap().project_id;
    let inbox = Inbox::new(
        world.repo.clone(),
        Arc::new(FixedClock(world.clock.now())),
        Vec::new(),
    );
    Fixture {
        world,
        inbox,
        project,
        item,
    }
}

impl Fixture {
    fn run(&self, request: &str) -> RunRef {
        RunRef {
            request_id: request.into(),
            attempt_id: format!("attempt-of-{request}"),
            item_id: self.item,
        }
    }

    fn said(&self, fact: Fact) -> DomainEvent {
        DomainEvent::new(self.project, &Principal::runner(RUNNER_ID), fact)
    }

    async fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self
            .inbox
            .open_notifications(100)
            .await
            .unwrap()
            .into_iter()
            .map(|n| n.key.to_string())
            .collect();
        keys.sort();
        keys
    }

    /// A running attempt that asked the question `decision`.
    async fn asking(&self, key: &str, decision: &str) -> Running {
        let run = running_attempt(&self.world, key).await;
        seed::pending_decision(
            &self.world.repo,
            &self.world.clock,
            RUNNER_ID,
            &run.attempt_id,
            decision,
            &[],
            None,
        )
        .await;
        run
    }

    async fn reconcile(&self) -> Reconciled {
        reconcile(&self.world.repo, &self.inbox, &self.world.clock)
            .await
            .expect("reconcile")
    }
}

#[tokio::test]
async fn the_events_of_a_question_open_and_close_its_notification() {
    let fx = fixture().await;
    let bus = EventBus::default();
    let mut receiver = bus.subscribe();
    let run = fx.run("req-1");

    bus.emit(fx.said(Fact::DecisionOpened {
        run: run.clone(),
        decision_id: "dec-1".into(),
    }));
    bus.emit(fx.said(Fact::DecisionOpened {
        run: run.clone(),
        decision_id: "dec-1".into(),
    }));
    bus.emit(fx.said(Fact::RunFinished { run: run.clone() }));
    assert!(!drain(&fx.world.repo, &fx.inbox, &mut receiver).await);
    assert_eq!(
        fx.keys().await,
        ["decision:dec-1", "finished:req-1"],
        "the repeat opened nothing more"
    );

    // Answered at docket's own tool or in Tack: whoever answers closes it.
    bus.emit(DomainEvent::new(
        fx.project,
        &Principal::operator("local"),
        Fact::DecisionResolved {
            run,
            decision_id: "dec-1".into(),
        },
    ));
    drain(&fx.world.repo, &fx.inbox, &mut receiver).await;
    assert_eq!(fx.keys().await, ["finished:req-1"]);
}

#[tokio::test]
async fn a_failed_run_closes_when_the_same_request_starts_again() {
    let fx = fixture().await;
    let bus = EventBus::default();
    let mut receiver = bus.subscribe();
    let run = fx.run("req-1");

    bus.emit(fx.said(Fact::RunFailed { run: run.clone() }));
    drain(&fx.world.repo, &fx.inbox, &mut receiver).await;
    assert_eq!(fx.keys().await, ["run:req-1"]);

    bus.emit(fx.said(Fact::RunStarted { run }));
    drain(&fx.world.repo, &fx.inbox, &mut receiver).await;
    assert!(fx.keys().await.is_empty());
}

#[tokio::test]
async fn a_fact_about_a_hidden_item_opens_nothing() {
    let fx = fixture().await;
    seed::item_hidden(&fx.world.repo, &fx.world.item_id).await;
    let bus = EventBus::default();
    let mut receiver = bus.subscribe();

    bus.emit(fx.said(Fact::RunFailed {
        run: fx.run("req-1"),
    }));
    drain(&fx.world.repo, &fx.inbox, &mut receiver).await;

    assert!(fx.keys().await.is_empty());
}

#[tokio::test]
async fn a_receiver_that_fell_behind_asks_for_a_reconcile_that_repairs_what_it_lost() {
    let fx = fixture().await;
    fx.asking("lost", "dec-lost").await;
    let (sender, mut receiver) = broadcast::channel(2);
    for _ in 0..5 {
        sender
            .send(fx.said(Fact::RunStarted { run: fx.run("x") }))
            .unwrap();
    }

    let repair = drain(&fx.world.repo, &fx.inbox, &mut receiver).await;
    assert!(repair, "the receiver was told it lost events");
    assert!(
        fx.keys().await.is_empty(),
        "the lost question is not open yet"
    );

    fx.reconcile().await;
    assert_eq!(fx.keys().await, ["decision:dec-lost"]);
    assert!(
        !drain(&fx.world.repo, &fx.inbox, &mut receiver).await,
        "caught up, nothing more to repair"
    );
}

#[tokio::test]
async fn the_reconcile_opens_what_waits_closes_what_does_not_and_does_nothing_twice() {
    let fx = fixture().await;
    fx.asking("asked", "dec-1").await;
    let stale = Notice {
        kind: Kind::Decision,
        level: Level::NeedsYou,
        key: Key::decision("dec-gone"),
        project_id: fx.project,
        item_id: None,
        request_id: None,
        title: "A run is asking a question".into(),
        cause: "runner:local".into(),
    };
    fx.inbox.open(&stale).await.unwrap().expect("stale row");
    assert_eq!(fx.keys().await, ["decision:dec-gone"]);

    assert_eq!(
        fx.reconcile().await,
        Reconciled {
            opened: 1,
            closed: 1
        }
    );
    assert_eq!(fx.keys().await, ["decision:dec-1"]);
    assert_eq!(fx.reconcile().await, Reconciled::default());
    assert_eq!(fx.keys().await, ["decision:dec-1"]);
}

#[tokio::test]
async fn a_question_docket_closed_in_its_own_tool_stops_waiting_on_the_person() {
    let fx = fixture().await;
    let run = fx.asking("elsewhere", "dec-1").await;
    assert_eq!(fx.reconcile().await.opened, 1);
    assert_eq!(fx.keys().await, ["decision:dec-1"]);

    // No fact is announced on this road: only the store knows.
    answered_elsewhere(
        &fx.world.repo,
        fx.world.clock.now(),
        &AttemptId::new(&run.attempt_id),
        "dec-1",
        "docket",
    )
    .await
    .expect("closed");

    assert_eq!(
        fx.reconcile().await,
        Reconciled {
            opened: 0,
            closed: 1
        }
    );
    assert!(fx.keys().await.is_empty());
}

#[tokio::test]
async fn what_the_notification_says_names_the_item_it_is_about() {
    let fx = fixture().await;
    fx.asking("named", "dec-1").await;
    let title = fx
        .world
        .repo
        .get_item(fx.item)
        .await
        .unwrap()
        .unwrap()
        .title;

    fx.reconcile().await;

    let open = fx.inbox.open_notifications(10).await.unwrap();
    assert_eq!(
        open[0].title,
        format!("A run is asking a question: {title}"),
        "two questions of two items no longer read the same"
    );
}

#[tokio::test]
async fn the_loop_reconciles_on_a_clock_so_an_ending_no_fact_announced_is_still_seen() {
    let fx = fixture().await;
    let run = fx.asking("clocked", "dec-1").await;
    let mut loops = Loops::new();
    // A pass every 20 ms: a reconcile comes every fifteenth, 300 ms, as thirty seconds in production.
    spawn_every(
        &mut loops,
        fx.world.repo.clone(),
        Arc::new(FixedClock(fx.world.clock.now())),
        fx.inbox.clone(),
        Arc::new(EventBus::default()),
        Duration::from_millis(20),
    );
    let opened = poll_until(Duration::from_secs(5), async || {
        (fx.keys().await == ["decision:dec-1"]).then_some(())
    })
    .await;
    assert!(opened.is_some(), "the first pass opened it");

    answered_elsewhere(
        &fx.world.repo,
        fx.world.clock.now(),
        &AttemptId::new(&run.attempt_id),
        "dec-1",
        "docket",
    )
    .await
    .expect("closed");
    let closed = poll_until(Duration::from_secs(5), async || {
        fx.keys().await.is_empty().then_some(())
    })
    .await;
    loops.stop(STOP_GRACE).await;
    assert!(
        closed.is_some(),
        "a pass on the clock closed it, with no event and no reconnect"
    );
}

#[tokio::test]
async fn a_failed_run_stays_open_until_a_later_request_follows_it() {
    let fx = fixture().await;
    let failed = enqueue(&fx.world, "failed").await;
    seed::execution_request_state(&fx.world.repo, &failed, ExecutionState::Failed).await;
    seed::execution_request_created_at(&fx.world.repo, &failed, "2026-08-06T11:00:00Z").await;
    seed::execution_request_updated_at(&fx.world.repo, &failed, "2026-08-06T11:30:00Z").await;

    assert_eq!(fx.reconcile().await.opened, 1);
    assert_eq!(fx.keys().await, [format!("run:{failed}")]);

    // Run again under a new request id: no event names the old one.
    let again = enqueue(&fx.world, "again").await;
    seed::execution_request_created_at(&fx.world.repo, &again, "2026-08-06T11:45:00Z").await;
    assert_eq!(
        fx.reconcile().await,
        Reconciled {
            opened: 0,
            closed: 1
        }
    );
    assert!(fx.keys().await.is_empty());
}

#[tokio::test]
async fn an_old_failure_is_not_news_to_the_reconcile() {
    let fx = fixture().await;
    let failed = enqueue(&fx.world, "old").await;
    seed::execution_request_state(&fx.world.repo, &failed, ExecutionState::Failed).await;
    seed::execution_request_created_at(&fx.world.repo, &failed, "2026-06-01T10:00:00Z").await;

    assert_eq!(fx.reconcile().await, Reconciled::default());
}

#[tokio::test]
async fn a_change_set_waits_in_the_inbox_until_it_is_settled() {
    let fx = fixture().await;
    let events = RecordedEvents::default();
    let proposer = Principal::agent(
        "credential-1",
        vec![Scope {
            all_projects: false,
            projects: vec![fx.project],
            capabilities: vec![Capability::Propose],
        }],
    );
    let proposed = propose(
        &fx.world.repo,
        &fx.world.clock,
        &events,
        Proposal {
            project: fx.project,
            principal: &proposer,
            origin: Origin::Mcp,
            origin_ref: None,
            reason: "Because".into(),
            idempotency_key: None,
            anchor: None,
            artifact_id: None,
            operations: vec![Operation::Create(CreateOp {
                title: "New".into(),
                description: None,
                acceptance: vec!["it works".into()],
                depends_on: Vec::new(),
                item_type: None,
                role: None,
                team: None,
                profile_id: None,
                parent: None,
            })],
            mode: Mode::Rung,
        },
    )
    .await
    .expect("propose");
    let id = proposed.change_set.id;

    assert_eq!(fx.reconcile().await.opened, 1);
    assert_eq!(
        fx.keys().await,
        [Key::change_set(&id.to_string()).to_string()]
    );

    reject(
        &fx.world.repo,
        &fx.world.clock,
        &events,
        id,
        &Principal::operator("local"),
        "No",
    )
    .await
    .expect("reject");
    assert_eq!(fx.reconcile().await.closed, 1);
    assert!(fx.keys().await.is_empty());
}

#[tokio::test]
async fn the_last_open_run_among_a_parents_children_opens_one_info_notification_for_the_parent() {
    use crate::common::FakeClock;
    use tack_app::runs::announce;
    use tack_app::runs::complete::{CompletionReport, complete};
    use tack_core::models::CreateItem;
    use tack_protocol::execution::{AttemptId, CompletionId, RunnerId};
    use tack_test_support::completion::{actual_execution, terminal_reason_of, usage_not_measured};

    let fx = fixture().await;
    let status_before = fx
        .world
        .repo
        .get_item(fx.item)
        .await
        .unwrap()
        .unwrap()
        .status;
    let bus = EventBus::default();
    let mut receiver = bus.subscribe();
    let runner = Principal::runner(RUNNER_ID);
    let child = |title: &'static str| {
        let (repo, clock) = (fx.world.repo.clone(), fx.world.clock.0.clone());
        async move {
            let item = repo
                .create_item(
                    fx.project,
                    "To Do",
                    CreateItem {
                        title: title.into(),
                        parent_id: Some(fx.item),
                        ..Default::default()
                    },
                )
                .await
                .unwrap();
            World {
                repo,
                clock: FakeClock(clock),
                item_id: item.id.to_string(),
            }
        }
    };
    let (first, second) = (child("first").await, child("second").await);
    let runs = [
        (running_attempt(&first, "kid-a").await, "done-a"),
        (running_attempt(&second, "kid-b").await, "done-b"),
    ];

    for (index, (run, completion)) in runs.iter().enumerate() {
        async fn no_pack(_reference: String) -> Option<tack_protocol::mrp::MergeReadinessPack> {
            None
        }
        let attempt = AttemptId::new(&run.attempt_id);
        let completed = complete(
            &fx.world.repo,
            fx.world.clock.now(),
            CompletionReport {
                runner_id: &RunnerId::new(RUNNER_ID),
                attempt_id: &attempt,
                fencing_token: run.fence,
                completion_id: &CompletionId::new(*completion),
                final_event_checkpoint: None,
                terminal_state: ExecutionState::Succeeded,
                terminal_reason: &terminal_reason_of(serde_json::json!({})),
                actual_execution: &actual_execution(),
                usage: &usage_not_measured(),
            },
            no_pack,
        )
        .await
        .unwrap();
        announce::run_ended(&fx.world.repo, &bus, &runner, &attempt, &completed).await;
        drain(&fx.world.repo, &fx.inbox, &mut receiver).await;
        let parent_told = fx
            .keys()
            .await
            .contains(&Key::children(fx.item).to_string());
        assert_eq!(parent_told, index == 1, "told after run {index}");
    }

    let opened = fx.inbox.open_notifications(100).await.unwrap();
    let told = opened
        .iter()
        .find(|n| n.kind == Kind::ChildrenFinished)
        .expect("the parent was told");
    assert_eq!((told.level, told.item_id), (Level::Info, Some(fx.item)));
    let status_after = fx
        .world
        .repo
        .get_item(fx.item)
        .await
        .unwrap()
        .unwrap()
        .status;
    assert_eq!(status_after, status_before, "the parent did not move");
}

/// Memory entries as `(id, subject)`.
type Entries<'a> = &'a [(&'a str, &'a str)];

/// What a runner's report holds when a docket team has these memory entries (pending, accepted);
/// `None` is a team whose report carries none.
fn docket_report(world: &World, teams: &[(&str, Option<(Entries, Entries)>)]) -> String {
    let entry = |(id, subject): &(&str, &str)| {
        serde_json::json!({
            "id": id, "kind": "pitfall", "subject": subject, "note": "a note",
            "reason": "awaits operator approval", "stale": false,
            "provenance": {"task": "task-1"}
        })
    };
    let teams: Vec<_> = teams
        .iter()
        .map(|(name, memory)| {
            let mut team = serde_json::json!({"name": name, "violations": []});
            if let Some((pending, accepted)) = memory {
                team["memory"] = serde_json::json!({
                    "pending": pending.iter().map(entry).collect::<Vec<_>>(),
                    "accepted": accepted.iter().map(entry).collect::<Vec<_>>(),
                });
            }
            team
        })
        .collect();
    let mut snapshot = common::capabilities(world.clock.now(), 2, 2);
    snapshot["harnesses"] = serde_json::json!([{
        "harness_kind": "docket", "installed_version": "0.3.0b1", "probe_error": null,
        "probed_at": world.clock.now().to_rfc3339(), "requires_model": false, "teams": teams,
    }]);
    snapshot.to_string()
}

/// The runner reports `snapshot` as its capabilities, as a refresh stores it.
async fn reports(world: &World, snapshot: &str) {
    tack_app::runs::refresh::refresh(
        &world.repo,
        world.clock.now(),
        tack_app::runs::refresh::Refresh {
            runner_id: RUNNER_ID,
            presented_credential_hash: common::CREDENTIAL_HASH,
            runner_version: "2.0.0",
            labels_json: "{}",
            capability_snapshot_json: snapshot,
            new_credential_hash: None,
        },
    )
    .await
    .expect("refresh");
}

/// A profile that names the docket team `team`.
async fn team_profile(world: &World, id: &str, team: &str) {
    let policy = serde_json::json!({"agent": "docket", "docket": {"team": team}}).to_string();
    tack_test_support::seed::create_agent_profile(
        &world.repo,
        tack_db::repo::execution::NewAgentProfile {
            id,
            name: id,
            instructions: "work",
            tool_policy: &policy,
            limits: "{}",
        },
        world.clock.now(),
    )
    .await
    .expect("profile");
}

/// A role of `project` whose profile is `profile`; the role's id.
async fn role_using(world: &World, project: Uuid, name: &str, profile: &str) -> Uuid {
    let role = world
        .repo
        .create_role(
            project,
            CreateRole {
                name: name.into(),
                color: None,
                icon: None,
            },
        )
        .await
        .expect("role");
    set_role_profile(world, role.id, Some(profile)).await;
    role.id
}

async fn set_role_profile(world: &World, role: Uuid, profile: Option<&str>) {
    world
        .repo
        .update_role(
            role,
            tack_core::models::UpdateRole {
                name: None,
                color: None,
                icon: None,
                agent_profile_id: Some(profile.map(str::to_owned)),
            },
        )
        .await
        .expect("update")
        .expect("role exists");
}

/// Another project on the same board.
async fn another_project(fx: &Fixture) -> Uuid {
    let workspace = fx
        .world
        .repo
        .get_project(fx.project)
        .await
        .unwrap()
        .unwrap()
        .workspace_id;
    tack_test_support::make_project(&fx.world.repo, workspace)
        .await
        .id
}

fn memory_key(project: Uuid, team: &str, id: &str) -> String {
    Key::memory(project, team, id).to_string()
}

#[tokio::test]
async fn a_pending_memory_entry_needs_you_in_the_project_that_uses_the_team_until_it_leaves_pending()
 {
    let fx = fixture().await;
    team_profile(&fx.world, "p-dev", "saas-dev").await;
    role_using(&fx.world, fx.project, "Builder", "p-dev").await;
    let pending: &[(&str, &str)] = &[("mem-1", "README.md"), ("mem-2", "docs/")];
    let accepted: &[(&str, &str)] = &[("mem-0", "src/")];
    reports(
        &fx.world,
        &docket_report(&fx.world, &[("saas-dev", Some((pending, accepted)))]),
    )
    .await;

    let done = reconcile(&fx.world.repo, &fx.inbox, &fx.world.clock)
        .await
        .unwrap();

    assert_eq!(
        done,
        Reconciled {
            opened: 2,
            closed: 0
        }
    );
    let mut wanted = vec![
        memory_key(fx.project, "saas-dev", "mem-1"),
        memory_key(fx.project, "saas-dev", "mem-2"),
    ];
    wanted.sort();
    assert_eq!(
        fx.keys().await,
        wanted,
        "one per pending entry, none for an accepted one"
    );
    let opened = fx.inbox.open_notifications(100).await.unwrap();
    let first = opened
        .iter()
        .find(|n| n.key == Key::memory(fx.project, "saas-dev", "mem-1"))
        .unwrap();
    assert_eq!(
        (first.kind, first.level, first.project_id),
        (Kind::Memory, Level::NeedsYou, fx.project)
    );
    assert_eq!(
        first.title,
        "Team saas-dev waits for you to approve what it learnt about README.md"
    );
    assert_eq!(first.cause, format!("runner:{RUNNER_ID}"));

    // Nothing twice.
    assert_eq!(
        reconcile_memory(&fx.world.repo, &fx.inbox).await.unwrap(),
        Reconciled::default()
    );

    // The operator approves mem-1 at docket's own CLI: the next report no longer lists it pending.
    let after_pending: &[(&str, &str)] = &[("mem-2", "docs/")];
    let after_accepted: &[(&str, &str)] = &[("mem-0", "src/"), ("mem-1", "README.md")];
    reports(
        &fx.world,
        &docket_report(
            &fx.world,
            &[("saas-dev", Some((after_pending, after_accepted)))],
        ),
    )
    .await;
    assert_eq!(
        reconcile_memory(&fx.world.repo, &fx.inbox).await.unwrap(),
        Reconciled {
            opened: 0,
            closed: 1
        }
    );
    assert_eq!(
        fx.keys().await,
        [memory_key(fx.project, "saas-dev", "mem-2")]
    );
}

#[tokio::test]
async fn two_projects_that_use_a_team_are_each_asked_and_a_project_that_does_not_is_not() {
    let fx = fixture().await;
    let second = another_project(&fx).await;
    let third = another_project(&fx).await;
    team_profile(&fx.world, "p-dev", "saas-dev").await;
    team_profile(&fx.world, "p-other", "other").await;
    role_using(&fx.world, fx.project, "Builder", "p-dev").await;
    // The second project uses the team as its default profile, not through a role.
    fx.world
        .repo
        .update_project(
            second,
            tack_core::models::UpdateProject {
                default_profile_id: Some(Some("p-dev".into())),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    // The third uses another team.
    role_using(&fx.world, third, "Builder", "p-other").await;
    let pending: &[(&str, &str)] = &[("mem-1", "README.md")];
    reports(
        &fx.world,
        &docket_report(
            &fx.world,
            &[("saas-dev", Some((pending, &[]))), ("other", None)],
        ),
    )
    .await;

    let done = reconcile_memory(&fx.world.repo, &fx.inbox).await.unwrap();

    assert_eq!(
        done,
        Reconciled {
            opened: 2,
            closed: 0
        }
    );
    let opened = fx.inbox.open_notifications(100).await.unwrap();
    let mut projects: Vec<Uuid> = opened.iter().map(|n| n.project_id).collect();
    projects.sort();
    let mut users = vec![fx.project, second];
    users.sort();
    assert_eq!(projects, users, "one for each project that uses the team");
    assert!(opened.iter().all(|n| n.project_id != third));
}

#[tokio::test]
async fn a_team_no_project_uses_opens_nothing() {
    let fx = fixture().await;
    let pending: &[(&str, &str)] = &[("mem-1", "README.md")];
    reports(
        &fx.world,
        &docket_report(&fx.world, &[("saas-dev", Some((pending, &[])))]),
    )
    .await;

    let done = reconcile(&fx.world.repo, &fx.inbox, &fx.world.clock)
        .await
        .unwrap();

    assert_eq!(done, Reconciled::default());
    assert!(fx.keys().await.is_empty());
}

#[tokio::test]
async fn changing_a_role_away_from_the_team_closes_that_projects_notification_only() {
    let fx = fixture().await;
    let second = another_project(&fx).await;
    team_profile(&fx.world, "p-dev", "saas-dev").await;
    team_profile(&fx.world, "p-other", "other").await;
    let role = role_using(&fx.world, fx.project, "Builder", "p-dev").await;
    role_using(&fx.world, second, "Builder", "p-dev").await;
    let pending: &[(&str, &str)] = &[("mem-1", "README.md")];
    reports(
        &fx.world,
        &docket_report(
            &fx.world,
            &[("saas-dev", Some((pending, &[]))), ("other", None)],
        ),
    )
    .await;
    reconcile_memory(&fx.world.repo, &fx.inbox).await.unwrap();
    assert_eq!(fx.keys().await.len(), 2);

    set_role_profile(&fx.world, role, Some("p-other")).await;

    assert_eq!(
        reconcile_memory(&fx.world.repo, &fx.inbox).await.unwrap(),
        Reconciled {
            opened: 0,
            closed: 1
        }
    );
    assert_eq!(
        fx.keys().await,
        [memory_key(second, "saas-dev", "mem-1")],
        "the project that still uses the team keeps its notification"
    );
}

#[tokio::test]
async fn a_team_that_does_not_list_the_feature_opens_nothing() {
    let fx = fixture().await;
    team_profile(&fx.world, "p-dev", "saas-dev").await;
    role_using(&fx.world, fx.project, "Builder", "p-dev").await;
    reports(&fx.world, &docket_report(&fx.world, &[("saas-dev", None)])).await;

    let done = reconcile(&fx.world.repo, &fx.inbox, &fx.world.clock)
        .await
        .unwrap();

    assert_eq!(done, Reconciled::default());
    assert!(fx.keys().await.is_empty());
}

#[tokio::test]
async fn a_report_without_memory_leaves_open_what_the_team_asked_but_a_vanished_team_closes_it() {
    let fx = fixture().await;
    team_profile(&fx.world, "p-dev", "saas-dev").await;
    role_using(&fx.world, fx.project, "Builder", "p-dev").await;
    let key = memory_key(fx.project, "saas-dev", "mem-1");
    let pending: &[(&str, &str)] = &[("mem-1", "README.md")];
    reports(
        &fx.world,
        &docket_report(&fx.world, &[("saas-dev", Some((pending, &[])))]),
    )
    .await;
    reconcile_memory(&fx.world.repo, &fx.inbox).await.unwrap();
    assert_eq!(fx.keys().await, std::slice::from_ref(&key));

    // The read of the team's memory failed this time: the report says nothing, so nothing closes.
    reports(&fx.world, &docket_report(&fx.world, &[("saas-dev", None)])).await;
    assert_eq!(
        reconcile_memory(&fx.world.repo, &fx.inbox).await.unwrap(),
        Reconciled::default()
    );
    assert_eq!(fx.keys().await, std::slice::from_ref(&key));

    // The team is no longer reported by any computer: what it asked is moot.
    reports(&fx.world, &docket_report(&fx.world, &[("other", None)])).await;
    assert_eq!(
        reconcile_memory(&fx.world.repo, &fx.inbox).await.unwrap(),
        Reconciled {
            opened: 0,
            closed: 1
        }
    );
}

#[tokio::test]
async fn the_full_reconcile_leaves_the_memory_notifications_to_the_teams_reports() {
    let fx = fixture().await;
    team_profile(&fx.world, "p-dev", "saas-dev").await;
    role_using(&fx.world, fx.project, "Builder", "p-dev").await;
    let pending: &[(&str, &str)] = &[("mem-1", "README.md")];
    reports(
        &fx.world,
        &docket_report(&fx.world, &[("saas-dev", Some((pending, &[])))]),
    )
    .await;
    reconcile(&fx.world.repo, &fx.inbox, &fx.world.clock)
        .await
        .unwrap();

    let again = reconcile(&fx.world.repo, &fx.inbox, &fx.world.clock)
        .await
        .unwrap();

    assert_eq!(again, Reconciled::default());
    assert_eq!(
        fx.keys().await,
        [memory_key(fx.project, "saas-dev", "mem-1")]
    );
}

#[tokio::test]
async fn the_webhook_carries_the_project_a_memory_notification_belongs_to() {
    use tack_app::notifications::channel::Channel;
    use tack_app::notifications::webhook::WebhookChannel;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let fx = fixture().await;
    let second = another_project(&fx).await;
    team_profile(&fx.world, "p-dev", "saas-dev").await;
    role_using(&fx.world, second, "Builder", "p-dev").await;
    let pending: &[(&str, &str)] = &[("mem-1", "README.md")];
    reports(
        &fx.world,
        &docket_report(&fx.world, &[("saas-dev", Some((pending, &[])))]),
    )
    .await;
    reconcile_memory(&fx.world.repo, &fx.inbox).await.unwrap();
    let notification = fx.inbox.open_notifications(10).await.unwrap().remove(0);

    let receiver = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&receiver)
        .await;
    WebhookChannel::new(format!("{}/hook", receiver.uri()), None)
        .send(&notification)
        .await
        .expect("delivered");

    let received = receiver.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&received[0].body).unwrap();
    assert_eq!(body["event"], "notification.memory");
    assert_eq!(body["notification"]["project_id"], second.to_string());
    assert_eq!(
        body["notification"]["key"],
        memory_key(second, "saas-dev", "mem-1")
    );
}
