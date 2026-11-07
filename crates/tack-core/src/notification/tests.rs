use super::*;
use crate::event::RunRef;
use crate::principal::Principal;
use chrono::TimeZone;

fn project() -> Uuid {
    Uuid::from_u128(1)
}

fn run() -> RunRef {
    RunRef {
        request_id: "req-1".into(),
        attempt_id: "att-1".into(),
        item_id: Uuid::from_u128(7),
    }
}

fn by(principal: &Principal, fact: Fact) -> DomainEvent {
    DomainEvent::new(project(), principal, fact)
}

fn runner() -> Principal {
    Principal::runner("local")
}

fn item(due: Option<DateTime<Utc>>) -> Item {
    serde_json::from_value(serde_json::json!({
        "id": "0192f5a0-7c1e-7d3a-9b2e-5f6a7b8c9d0e",
        "project_id": "0192f5a0-0000-7000-8000-000000000001",
        "parent_id": null, "title": "Ship it", "description": null,
        "item_type": "task", "status": "To Do", "priority": "medium",
        "estimate": null, "estimate_unit": "story_points", "tags": [],
        "sort_order": 0, "sprint_id": null, "assignee": null, "due_date": due,
        "started_at": null, "completed_at": null,
        "created_at": "2026-10-05T10:00:00Z", "updated_at": "2026-10-05T10:00:00Z"
    }))
    .expect("an Item")
}

#[test]
fn a_question_asked_needs_you_and_names_its_decision() {
    let n = notice(&by(
        &runner(),
        Fact::DecisionOpened {
            run: run(),
            decision_id: "d-1".into(),
        },
    ))
    .expect("a notice");
    assert_eq!((n.kind, n.level), (Kind::Decision, Level::NeedsYou));
    assert_eq!(n.key, Key::decision("d-1"));
    assert_eq!(n.key.as_str(), "decision:d-1");
    assert_eq!(n.item_id, Some(Uuid::from_u128(7)));
    assert_eq!(n.request_id.as_deref(), Some("req-1"));
    assert_eq!(n.project_id, project());
    assert_eq!(n.cause, "runner:local");
}

#[test]
fn a_run_that_failed_needs_you_under_its_request() {
    let n = notice(&by(&runner(), Fact::RunFailed { run: run() })).expect("a notice");
    assert_eq!((n.kind, n.level), (Kind::RunFailed, Level::NeedsYou));
    assert_eq!(n.key.as_str(), "run:req-1");
}

#[test]
fn a_change_set_proposed_needs_you_with_or_without_an_item() {
    let proposed = |item_id| {
        notice(&by(
            &Principal::agent("cred", Vec::new()),
            Fact::ChangeSetProposed {
                change_set_id: "cs-1".into(),
                item_id,
            },
        ))
        .expect("a notice")
    };
    let n = proposed(Some(Uuid::from_u128(9)));
    assert_eq!((n.kind, n.level), (Kind::ChangeSet, Level::NeedsYou));
    assert_eq!(n.key.as_str(), "change_set:cs-1");
    assert_eq!(n.item_id, Some(Uuid::from_u128(9)));
    assert_eq!(proposed(None).item_id, None);
}

#[test]
fn a_run_that_finished_is_information_under_its_own_key() {
    let n = notice(&by(&runner(), Fact::RunFinished { run: run() })).expect("a notice");
    assert_eq!((n.kind, n.level), (Kind::RunFinished, Level::Info));
    assert_eq!(n.key.as_str(), "finished:req-1");
    assert_ne!(
        n.key,
        notice(&by(&runner(), Fact::RunFailed { run: run() }))
            .unwrap()
            .key,
        "a failure and a finish of one request are different situations"
    );
}

#[test]
fn the_children_of_a_parent_finishing_is_information_under_the_parent() {
    let parent = Uuid::from_u128(11);
    let n = notice(&by(&runner(), Fact::ChildrenFinished { parent_id: parent })).expect("a notice");
    assert_eq!((n.kind, n.level), (Kind::ChildrenFinished, Level::Info));
    assert_eq!(n.key, Key::children(parent));
    assert_eq!(n.key.as_str(), format!("children:{parent}"));
    assert_eq!(n.kind.as_str(), "children_finished");
    assert_eq!((n.item_id, n.request_id), (Some(parent), None));
    assert_eq!(
        notice(&by(
            &Principal::operator("local"),
            Fact::ChildrenFinished { parent_id: parent }
        )),
        None,
        "what the operator caused never notifies the operator"
    );
}

#[test]
fn what_the_board_shows_already_is_no_notification() {
    let item_id = Uuid::from_u128(3);
    for fact in [
        Fact::ItemCreated {
            item_id,
            status: "To Do".into(),
        },
        Fact::ItemUpdated {
            item_id,
            status: "To Do".into(),
        },
        Fact::ItemMoved {
            item_id,
            from: "To Do".into(),
            to: "Done".into(),
        },
        Fact::ItemDeleted { item_id },
        Fact::RunStarted { run: run() },
        Fact::RunWaiting { run: run() },
        Fact::DecisionResolved {
            run: run(),
            decision_id: "d-1".into(),
        },
        Fact::ChangeSetSettled {
            change_set_id: "cs-1".into(),
        },
    ] {
        assert_eq!(notice(&by(&runner(), fact.clone())), None, "{fact:?}");
    }
}

#[test]
fn what_the_operator_caused_never_notifies_the_operator() {
    let operator = Principal::operator("local");
    for fact in [
        Fact::DecisionOpened {
            run: run(),
            decision_id: "d-1".into(),
        },
        Fact::RunFailed { run: run() },
        Fact::RunFinished { run: run() },
        Fact::ChangeSetProposed {
            change_set_id: "cs-1".into(),
            item_id: None,
        },
    ] {
        assert_eq!(notice(&by(&operator, fact.clone())), None, "{fact:?}");
    }
    // Tack's own loops and agents are not the person.
    assert!(notice(&by(&Principal::system(), Fact::RunFailed { run: run() })).is_some());
    assert!(
        notice(&by(
            &Principal::agent("cred", Vec::new()),
            Fact::RunFailed { run: run() }
        ))
        .is_some()
    );
}

#[test]
fn an_answer_closes_its_decision_whoever_gave_it() {
    let resolved = Fact::DecisionResolved {
        run: run(),
        decision_id: "d-1".into(),
    };
    for principal in [Principal::operator("local"), runner(), Principal::system()] {
        assert_eq!(
            closes(&by(&principal, resolved.clone())),
            Some(Key::decision("d-1"))
        );
    }
}

#[test]
fn a_run_that_starts_closes_the_failure_of_its_request() {
    assert_eq!(
        closes(&by(&runner(), Fact::RunStarted { run: run() })),
        Some(Key::run("req-1"))
    );
}

#[test]
fn a_change_set_applied_or_rejected_closes_its_proposal() {
    assert_eq!(
        closes(&by(
            &Principal::operator("local"),
            Fact::ChangeSetSettled {
                change_set_id: "cs-1".into()
            }
        )),
        Some(Key::change_set("cs-1"))
    );
}

#[test]
fn nothing_else_closes_anything() {
    for fact in [
        Fact::DecisionOpened {
            run: run(),
            decision_id: "d-1".into(),
        },
        Fact::RunFailed { run: run() },
        Fact::RunFinished { run: run() },
        Fact::RunWaiting { run: run() },
        Fact::ItemDeleted {
            item_id: Uuid::from_u128(3),
        },
        Fact::ChildrenFinished {
            parent_id: Uuid::from_u128(11),
        },
        Fact::ChangeSetProposed {
            change_set_id: "cs-1".into(),
            item_id: None,
        },
    ] {
        assert_eq!(closes(&by(&runner(), fact.clone())), None, "{fact:?}");
    }
}

#[test]
fn an_item_due_soon_is_information_once_for_the_day_it_falls_due() {
    let due = Utc.with_ymd_and_hms(2026, 11, 4, 9, 30, 0).unwrap();
    let now = Utc.with_ymd_and_hms(2026, 11, 4, 8, 45, 0).unwrap();
    let item = item(Some(due));
    let n = due_soon(&item, now);
    assert_eq!((n.kind, n.level), (Kind::DueSoon, Level::Info));
    assert_eq!(n.key.as_str(), format!("due:{}:2026-11-04", item.id));
    assert_eq!(n.item_id, Some(item.id));
    assert_eq!(n.project_id, item.project_id);
    assert!(n.title.contains("Ship it"));
    // A later pass in the same hour makes the same key.
    assert_eq!(
        due_soon(&item, now + chrono::Duration::minutes(20)).key,
        n.key
    );
}

#[test]
fn kinds_and_levels_have_the_names_the_store_and_the_api_use() {
    for (kind, name) in [
        (Kind::Decision, "decision"),
        (Kind::RunFailed, "run_failed"),
        (Kind::ChangeSet, "change_set"),
        (Kind::RunFinished, "run_finished"),
        (Kind::DueSoon, "due_soon"),
    ] {
        assert_eq!(kind.as_str(), name);
        assert_eq!(serde_json::to_value(kind).unwrap(), name);
    }
    assert_eq!(serde_json::to_value(Level::NeedsYou).unwrap(), "needs_you");
    assert_eq!(Level::Info.as_str(), "info");
}

#[test]
fn a_memory_entry_waiting_needs_you_under_its_team_and_entry() {
    let n = memory_waiting(
        project(),
        "saas-dev",
        "mem-b0ec5e2b1b",
        "README.md",
        "laptop",
    );
    assert_eq!((n.kind, n.level), (Kind::Memory, Level::NeedsYou));
    assert_eq!(n.key, Key::memory(project(), "saas-dev", "mem-b0ec5e2b1b"));
    assert_eq!(
        n.key.as_str(),
        format!("memory:{}:saas-dev:mem-b0ec5e2b1b", project())
    );
    assert_eq!(Kind::Memory.as_str(), "memory");
    assert_eq!(
        n.title,
        "Team saas-dev waits for you to approve what it learnt about README.md"
    );
    assert_eq!((n.item_id, n.request_id), (None, None));
    assert_eq!(n.project_id, project());
    assert_eq!(n.cause, "runner:laptop");
}

#[test]
fn a_memory_key_names_its_project_and_team_and_no_other_key_does() {
    assert_eq!(
        Key::memory(project(), "saas-dev", "mem-1").memory_use(),
        Some((project(), "saas-dev"))
    );
    assert_ne!(
        Key::memory(project(), "saas-dev", "mem-1"),
        Key::memory(Uuid::from_u128(2), "saas-dev", "mem-1"),
        "the same entry is one situation for each project"
    );
    assert_eq!(Key::decision("d-1").memory_use(), None);
    assert_eq!(Key::from_stored("memory:odd").memory_use(), None);
}

#[test]
fn a_long_subject_is_cut_in_the_title() {
    let long = "x".repeat(200);
    let n = memory_waiting(project(), "t", "mem-1", &long, "laptop");
    assert!(n.title.ends_with(&format!("{}...", "x".repeat(80))));
    assert!(n.title.chars().count() < 160);
}

fn outcomes(succeeded: u32, ended: u32) -> Outcomes {
    Outcomes { succeeded, ended }
}

#[test]
fn trend_wilson_matches_the_hand_values() {
    let (low, high) = wilson(outcomes(8, 10)).unwrap();
    assert!(
        (low - 0.4902).abs() < 1e-3 && (high - 0.9433).abs() < 1e-3,
        "{low} {high}"
    );
    assert_eq!(wilson(outcomes(0, 0)), None);
    let (low, high) = wilson(outcomes(0, 5)).unwrap();
    assert_eq!(low, 0.0);
    assert!(high > 0.4 && high < 0.5);
}

#[test]
fn trend_warns_only_when_the_intervals_do_not_overlap() {
    // 2 of 20 now against 63 of 70 before: far apart.
    let drop = quality_trend(
        project(),
        "profile",
        "Coder",
        outcomes(2, 20),
        outcomes(63, 70),
    )
    .expect("a drop");
    assert_eq!(
        (drop.kind, drop.level),
        (Kind::QualityTrend, Level::NeedsYou)
    );
    assert_eq!(drop.key, Key::quality_trend(project(), "profile", "Coder"));
    assert_eq!(drop.kind.as_str(), "quality_trend");
    assert_eq!(
        drop.title,
        "Success for profile \"Coder\" fell to 10% (2 of 20) in the last 7 days, from 90% (63 of 70) in the 28 before"
    );
    // A lower rate whose intervals still overlap, a sample too small to tell, no change and a rise: no notice.
    assert!(
        quality_trend(
            project(),
            "profile",
            "Coder",
            outcomes(6, 10),
            outcomes(63, 70)
        )
        .is_none()
    );
    assert!(
        quality_trend(
            project(),
            "profile",
            "Coder",
            outcomes(2, 4),
            outcomes(63, 70)
        )
        .is_none()
    );
    assert!(
        quality_trend(
            project(),
            "profile",
            "Coder",
            outcomes(63, 70),
            outcomes(63, 70)
        )
        .is_none()
    );
    assert!(
        quality_trend(
            project(),
            "profile",
            "Coder",
            outcomes(20, 20),
            outcomes(2, 20)
        )
        .is_none()
    );
    // Nothing to compare with.
    assert!(
        quality_trend(
            project(),
            "profile",
            "Coder",
            outcomes(0, 0),
            outcomes(63, 70)
        )
        .is_none()
    );
    assert!(
        quality_trend(
            project(),
            "profile",
            "Coder",
            outcomes(2, 20),
            outcomes(0, 0)
        )
        .is_none()
    );
}
