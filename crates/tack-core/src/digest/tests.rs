use super::*;
use chrono::TimeZone;

fn at(day: u32, hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, day, hour, 0, 0).unwrap()
}

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

fn status(name: &str, category: StatusCategory) -> StatusFact {
    StatusFact {
        name: name.into(),
        category,
    }
}

fn workflow() -> Vec<StatusFact> {
    vec![
        status("Backlog", StatusCategory::Todo),
        status("In Progress", StatusCategory::InProgress),
        status("In Review", StatusCategory::InProgress),
        status("Done", StatusCategory::Done),
    ]
}

fn item(n: u128, title: &str, status: &str, updated_at: DateTime<Utc>) -> ItemFact {
    ItemFact {
        id: id(n),
        title: title.into(),
        status: status.into(),
        updated_at,
    }
}

fn empty_project(n: u128, name: &str) -> ProjectFacts {
    ProjectFacts {
        id: id(n),
        name: name.into(),
        statuses: workflow(),
        counts: Vec::new(),
        open_items: Vec::new(),
        blocks: Vec::new(),
        waiting_on_person: Vec::new(),
        failed_runs: Vec::new(),
        unclaimed: None,
        tokens: TokenFacts::default(),
        change_sets: Vec::new(),
    }
}

fn input(projects: Vec<ProjectFacts>) -> DigestInput {
    DigestInput {
        since: at(20, 0),
        now: at(30, 12),
        projects,
        team_tasks: Vec::new(),
    }
}

fn team_task(team: &str, task: &str, state: ActivityState) -> TeamTaskFact {
    TeamTaskFact {
        harness: "docket".into(),
        team: team.into(),
        task_ref: task.into(),
        project: None,
        item: None,
        title: None,
        state,
        waits_on: None,
        step: None,
        changed_at: None,
    }
}

/// A board that uses every section: the same names a person would see on "My SaaS".
fn my_saas() -> DigestInput {
    let mut web = empty_project(1, "My SaaS");
    web.counts = vec![
        ("Backlog".into(), 6),
        ("In Progress".into(), 3),
        ("In Review".into(), 1),
        ("Done".into(), 12),
        ("Archived".into(), 2),
    ];
    web.open_items = vec![
        item(10, "Design the sign-up flow", "Backlog", at(2, 9)),
        item(11, "Build the sign-up form", "In Progress", at(3, 9)),
        item(12, "Send the welcome email", "In Progress", at(29, 9)),
        item(13, "Add billing webhooks", "In Progress", at(10, 9)),
        item(14, "Review the billing page", "In Review", at(30, 8)),
        item(15, "Write the launch post", "Backlog", at(1, 9)),
        item(16, "Fix the invoice rounding", "Backlog", at(25, 9)),
    ];
    // 10 blocks 11, 11 blocks 12 and 13; 15 blocks 16; 99 is done and holds nothing up.
    web.blocks = vec![
        (id(10), id(11)),
        (id(11), id(12)),
        (id(11), id(13)),
        (id(15), id(16)),
        (id(99), id(12)),
    ];
    web.waiting_on_person = vec![
        WaitFact {
            item: id(14),
            title: "Review the billing page".into(),
            kind: WaitKind::Review,
            since: at(30, 8),
        },
        WaitFact {
            item: id(13),
            title: "Add billing webhooks".into(),
            kind: WaitKind::Decision,
            since: at(28, 15),
        },
    ];
    web.failed_runs = vec![
        FailedRunFact {
            run: "run-old".into(),
            item: id(11),
            title: "Build the sign-up form".into(),
            failed_at: at(5, 9),
            reason: Some("the build failed".into()),
        },
        FailedRunFact {
            run: "run-new".into(),
            item: id(12),
            title: "Send the welcome email".into(),
            failed_at: at(29, 18),
            reason: None,
        },
    ];
    web.unclaimed = Some(vec![
        UnclaimedFact {
            item: id(16),
            title: "Fix the invoice rounding".into(),
            code: "no_profile".into(),
            text: "No one is chosen to do it".into(),
        },
        UnclaimedFact {
            item: id(11),
            title: "Build the sign-up form".into(),
            code: "blocked".into(),
            text: "Blocked by 1 unfinished task".into(),
        },
        UnclaimedFact {
            item: id(15),
            title: "Write the launch post".into(),
            code: "no_profile".into(),
            text: "No one is chosen to do it".into(),
        },
    ]);
    web.tokens = TokenFacts {
        used_since: 1_200_000,
        used_today: 300_000,
        reserved_today: 100_000,
        limit_per_day: Some(1_000_000),
        per_run_limit: Some(2_000_000),
    };
    web.change_sets = vec![
        ChangeSetFact {
            id: id(201),
            principal: "agent:planner".into(),
            reason: "Split the billing work".into(),
            operations: 4,
            since: at(29, 10),
        },
        ChangeSetFact {
            id: id(200),
            principal: "agent:planner".into(),
            reason: "Add the launch checklist".into(),
            operations: 2,
            since: at(27, 10),
        },
    ];

    let mut docs = empty_project(2, "My SaaS docs");
    docs.counts = vec![("Backlog".into(), 2)];
    docs.open_items = vec![
        item(30, "Write the install page", "Backlog", at(1, 9)),
        item(31, "Write the API reference", "Backlog", at(1, 9)),
    ];

    let mut tasks = vec![
        team_task("writers", "draft-launch-post", ActivityState::Queued),
        team_task("builders", "billing-webhooks", ActivityState::Running),
        team_task("builders", "sign-up-form", ActivityState::Queued),
        team_task("builders", "invoice-rounding", ActivityState::Waiting),
        team_task("builders", "old-job", ActivityState::Done),
    ];
    tasks[1].project = Some(id(1));
    tasks[1].item = Some(id(13));
    tasks[1].title = Some("Add billing webhooks".into());
    tasks[1].step = Some(StepFact {
        name: "implement".into(),
        role: Some("lead".into()),
        since: Some(at(30, 9)),
    });
    tasks[1].changed_at = Some(at(30, 9));
    tasks[3].waits_on = Some(WaitsOn::Answer);
    tasks[3].changed_at = Some(at(30, 10));
    DigestInput {
        team_tasks: tasks,
        ..input(vec![docs, web])
    }
}

#[test]
fn the_digest_of_the_fixture_board_is_stable_byte_for_byte() {
    let board = my_saas();
    let text = serde_json::to_string_pretty(&digest(&board)).unwrap();
    let expected = include_str!("my-saas.expected.json");
    assert_eq!(text.trim_end(), expected.trim_end());

    // Every list of the input in the opposite order says the same.
    let mut flipped = board.clone();
    flipped.projects.reverse();
    for project in &mut flipped.projects {
        project.counts.reverse();
        project.open_items.reverse();
        project.blocks.reverse();
        project.waiting_on_person.reverse();
        project.failed_runs.reverse();
        project.change_sets.reverse();
    }
    flipped.team_tasks.reverse();
    assert_eq!(
        serde_json::to_string_pretty(&digest(&flipped)).unwrap(),
        text
    );
}

#[test]
fn a_board_of_any_size_stays_under_the_size_bound() {
    // 60 projects of 1,500 open items, each with every section full and long texts.
    let long = "x".repeat(2_000);
    let projects = (0..60_u128)
        .map(|p| {
            let base = p * 100_000;
            let mut project = empty_project(base + 1, &format!("{long} {p}"));
            project.counts = (0..200)
                .map(|s| (format!("{long}{s}"), 7_u64))
                .chain([("In Progress".to_owned(), 1_500)])
                .collect();
            project.open_items = (0..1_500_u128)
                .map(|n| item(base + 10 + n, &long, "In Progress", at(1, 9)))
                .collect();
            project.blocks = (1..1_500_u128)
                .map(|n| (id(base + 10), id(base + 10 + n)))
                .collect();
            project.waiting_on_person = (0..300_u128)
                .map(|n| WaitFact {
                    item: id(base + 10 + n),
                    title: long.clone(),
                    kind: WaitKind::Decision,
                    since: at(2, 9),
                })
                .collect();
            project.failed_runs = (0..300_u128)
                .map(|n| FailedRunFact {
                    run: format!("{long}{n}"),
                    item: id(base + 10 + n),
                    title: long.clone(),
                    failed_at: at(29, 9),
                    reason: Some(long.clone()),
                })
                .collect();
            project.unclaimed = Some(
                (0..300_u128)
                    .map(|n| UnclaimedFact {
                        item: id(base + 10 + n),
                        title: long.clone(),
                        code: "wip".into(),
                        text: long.clone(),
                    })
                    .collect(),
            );
            project.change_sets = (0..300_u128)
                .map(|n| ChangeSetFact {
                    id: id(base + 50_000 + n),
                    principal: long.clone(),
                    reason: long.clone(),
                    operations: 3,
                    since: at(3, 9),
                })
                .collect();
            project
        })
        .collect();
    let mut board = input(projects);
    board.team_tasks = (0..60)
        .flat_map(|t| {
            (0..200).map(move |n| {
                let mut task = team_task(
                    &format!("{t:03}"),
                    &format!("{n}{}", "y".repeat(2_000)),
                    ActivityState::Queued,
                );
                task.step = Some(StepFact {
                    name: "z".repeat(2_000),
                    role: Some("z".repeat(2_000)),
                    since: None,
                });
                task
            })
        })
        .collect();

    let out = digest(&board);
    let bytes = serde_json::to_vec(&out).unwrap().len();
    assert!(
        bytes < 512 * 1024,
        "the digest of a very large board is {bytes} bytes, over the 512 KiB bound"
    );
    assert_eq!(out.projects.rows.len(), MAX_PROJECTS);
    assert_eq!(out.projects.more, 40);
    assert_eq!(out.teams.rows.len(), MAX_TEAMS);
    let project = &out.projects.rows[0];
    assert_eq!(project.waiting_on_person.total, 300);
    assert_eq!(project.waiting_on_person.rows.len(), ROWS);
    assert_eq!(project.waiting_on_person.more, 300 - ROWS as u64);
    assert!(project.items_by_status.rows.len() <= MAX_STATUSES);
    assert_eq!(project.items_total, 200 * 7 + 1_500);
}

#[test]
fn items_are_counted_per_status_in_the_boards_order() {
    let out = digest(&my_saas());
    let web = &out.projects.rows[0];
    assert_eq!(web.name, "My SaaS");
    let rows: Vec<_> = web
        .items_by_status
        .rows
        .iter()
        .map(|row| (row.status.as_str(), row.count, row.done))
        .collect();
    // The workflow's statuses first, in its order; a status it does not list after them.
    assert_eq!(
        rows,
        vec![
            ("Backlog", 6, false),
            ("In Progress", 3, false),
            ("In Review", 1, false),
            ("Done", 12, true),
            ("Archived", 2, false),
        ]
    );
    assert_eq!(web.items_total, 24);
}

#[test]
fn what_waits_on_a_person_comes_with_since_when_the_longest_first() {
    let out = digest(&my_saas());
    let waits = &out.projects.rows[0].waiting_on_person;
    assert_eq!(waits.total, 2);
    assert_eq!(waits.rows[0].item.title, "Add billing webhooks");
    assert_eq!(waits.rows[0].kind, WaitKind::Decision);
    assert_eq!(waits.rows[0].since, at(28, 15));
    assert_eq!(waits.rows[1].kind, WaitKind::Review);
}

#[test]
fn a_blocked_chain_is_counted_from_the_item_that_holds_it_up() {
    let out = digest(&my_saas());
    let blocked = &out.projects.rows[0].blocked;
    // 11, 12, 13 and 16 wait; 12's other blocker is done and does not count.
    assert_eq!(blocked.items, 4);
    let chains: Vec<_> = blocked
        .chains
        .rows
        .iter()
        .map(|c| (c.blocker.id, c.blocks, c.depth))
        .collect();
    assert_eq!(chains, vec![(id(10), 3, 2), (id(15), 1, 1)]);
}

#[test]
fn a_dependency_loop_does_not_end_the_digest() {
    let mut project = empty_project(1, "My SaaS");
    project.open_items = vec![
        item(1, "A", "Backlog", at(1, 0)),
        item(2, "B", "Backlog", at(1, 0)),
        item(3, "C", "Backlog", at(1, 0)),
    ];
    project.blocks = vec![(id(1), id(2)), (id(2), id(3)), (id(3), id(2))];
    let out = digest(&input(vec![project]));
    let blocked = &out.projects.rows[0].blocked;
    assert_eq!(blocked.items, 2);
    assert_eq!(blocked.chains.rows[0].blocker.id, id(1));
    assert_eq!(blocked.chains.rows[0].blocks, 2);
}

#[test]
fn tasks_no_team_will_pick_up_say_why_and_keep_the_order_they_came_in() {
    let out = digest(&my_saas());
    let unclaimed = &out.projects.rows[0].unclaimed;
    assert!(unclaimed.auto_start);
    let reasons: Vec<_> = unclaimed
        .by_reason
        .iter()
        .map(|r| (r.code.as_str(), r.count))
        .collect();
    assert_eq!(reasons, vec![("blocked", 1), ("no_profile", 2)]);
    let first = &unclaimed.tasks.rows[0];
    assert_eq!(first.item.title, "Fix the invoice rounding");
    assert_eq!(first.why, "No one is chosen to do it");

    // A project that starts nothing by itself has nothing waiting for it.
    let docs = &out.projects.rows[1].unclaimed;
    assert!(!docs.auto_start);
    assert_eq!(docs.tasks.total, 0);
}

#[test]
fn only_items_in_progress_that_nothing_changed_for_two_weeks_are_stale() {
    let out = digest(&my_saas());
    let stale = &out.projects.rows[0].stale;
    // "Backlog" items are old by nature and "Add billing webhooks" was changed on the 10th,
    // twenty days before the digest.
    let titles: Vec<_> = stale.rows.iter().map(|s| s.item.title.as_str()).collect();
    assert_eq!(
        titles,
        vec!["Build the sign-up form", "Add billing webhooks"]
    );
    assert_eq!(stale.rows[0].last_change, at(3, 9));
}

#[test]
fn since_bounds_the_failed_runs() {
    let mut board = my_saas();
    let out = digest(&board);
    let runs = &out.projects.rows[0].failed_runs;
    assert_eq!(runs.total, 1);
    assert_eq!(runs.rows[0].run, "run-new");

    board.since = at(1, 0);
    let runs = &digest(&board).projects.rows[0].failed_runs;
    let ids: Vec<_> = runs.rows.iter().map(|r| r.run.as_str()).collect();
    assert_eq!(ids, vec!["run-new", "run-old"]);

    board.since = at(30, 0);
    assert_eq!(digest(&board).projects.rows[0].failed_runs.total, 0);
}

#[test]
fn tokens_are_set_against_the_days_limit() {
    let out = digest(&my_saas());
    let tokens = &out.projects.rows[0].tokens;
    assert_eq!(tokens.used_today, 300_000);
    assert_eq!(tokens.left_today, Some(600_000));
    assert_eq!(tokens.used_since, 1_200_000);
    assert_eq!(tokens.per_run_limit, Some(2_000_000));
    // No limit, no "left".
    assert_eq!(out.projects.rows[1].tokens.left_today, None);

    let mut over = my_saas();
    over.projects[1].tokens.used_today = 2_000_000;
    assert_eq!(digest(&over).projects.rows[0].tokens.left_today, Some(0));
}

#[test]
fn a_team_shows_what_it_is_doing_and_its_queue() {
    let out = digest(&my_saas());
    let teams: Vec<_> = out.teams.rows.iter().map(|t| t.team.as_str()).collect();
    assert_eq!(teams, vec!["builders", "writers"]);
    let builders = &out.teams.rows[0];
    assert_eq!(
        (
            builders.running,
            builders.waiting,
            builders.blocked,
            builders.queued
        ),
        (1, 1, 0, 1)
    );
    // The task being done is not work; the running one comes first, then the one waiting.
    let tasks: Vec<_> = builders
        .tasks
        .rows
        .iter()
        .map(|t| t.task.as_str())
        .collect();
    assert_eq!(
        tasks,
        vec!["billing-webhooks", "invoice-rounding", "sign-up-form"]
    );
    let running = &builders.tasks.rows[0];
    assert_eq!(running.item.as_ref().unwrap().title, "Add billing webhooks");
    assert_eq!(running.step.as_ref().unwrap().name, "implement");
    assert_eq!(builders.tasks.rows[1].waits_on, Some(WaitsOn::Answer));
    // Work the board did not start has no project.
    assert!(out.teams.rows[1].tasks.rows[0].project.is_none());
}

#[test]
fn change_sets_waiting_come_oldest_first() {
    let out = digest(&my_saas());
    let sets = &out.projects.rows[0].change_sets;
    assert_eq!(sets.total, 2);
    assert_eq!(sets.rows[0].id, id(200));
    assert_eq!(sets.rows[1].operations, 4);
}

#[test]
fn a_long_text_is_cut_and_says_so() {
    let mut project = empty_project(1, "My SaaS");
    project.waiting_on_person = vec![WaitFact {
        item: id(5),
        title: "t".repeat(500),
        kind: WaitKind::Decision,
        since: at(1, 0),
    }];
    let out = digest(&input(vec![project]));
    let title = &out.projects.rows[0].waiting_on_person.rows[0].item.title;
    assert_eq!(title.chars().count(), TITLE_CHARS);
    assert!(title.ends_with('…'));
}
