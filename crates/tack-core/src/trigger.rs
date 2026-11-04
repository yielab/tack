//! What wakes a profile (ADR 0034, decision 8), as pure values and the rules that bound it. A
//! [`Trigger`] is either an event (a kind of thing that happened on a board, a [`Filter`]) or a
//! schedule (weekdays and a time). Events that arrive close together wait for a quiet moment
//! and then start one run that reads all of them; a trigger has a cap of runs per day; and an
//! event a profile's own principal caused never starts that profile. Nothing here touches the
//! store, a clock or HTTP.

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Datelike, Duration, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::thread::ThreadScope;

/// The longest quiet period a trigger may ask for, in seconds.
pub const MAX_QUIET_SECONDS: u32 = 3_600;

/// The most runs one trigger starts in a day.
pub const MAX_DAILY_CAP: u32 = 100;

/// A burst that never goes quiet starts its run anyway after this long, in seconds.
pub const MAX_WAIT_SECONDS: i64 = 3_600;

/// The most events one run lists; the rest are counted.
pub const MAX_LISTED: usize = 50;

/// The longest name, in characters.
pub const NAME_CHARS: usize = 100;

/// Events nobody read are forgotten after this many days.
pub const KEEP_DAYS: i64 = 7;

/// The kinds of thing a trigger can wait for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum EventKind {
    ItemCreated,
    ItemUpdated,
    ItemMoved,
    ItemDeleted,
    RunStarted,
    RunFinished,
    RunFailed,
    RunWaiting,
    DecisionOpened,
    DecisionResolved,
    /// A change set has waited for a person longer than the filter's `after_seconds`. Time
    /// caused it, not a principal: Tack announces it once per change set.
    ChangeSetWaiting,
    /// An agent proposed a change set that waits for a person.
    ChangeSetProposed,
    /// A change set no longer waits: it was applied or rejected.
    ChangeSetSettled,
}

impl EventKind {
    pub const ALL: [EventKind; 13] = [
        EventKind::ItemCreated,
        EventKind::ItemUpdated,
        EventKind::ItemMoved,
        EventKind::ItemDeleted,
        EventKind::RunStarted,
        EventKind::RunFinished,
        EventKind::RunFailed,
        EventKind::RunWaiting,
        EventKind::DecisionOpened,
        EventKind::DecisionResolved,
        EventKind::ChangeSetWaiting,
        EventKind::ChangeSetProposed,
        EventKind::ChangeSetSettled,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            EventKind::ItemCreated => "item_created",
            EventKind::ItemUpdated => "item_updated",
            EventKind::ItemMoved => "item_moved",
            EventKind::ItemDeleted => "item_deleted",
            EventKind::RunStarted => "run_started",
            EventKind::RunFinished => "run_finished",
            EventKind::RunFailed => "run_failed",
            EventKind::RunWaiting => "run_waiting",
            EventKind::DecisionOpened => "decision_opened",
            EventKind::DecisionResolved => "decision_resolved",
            EventKind::ChangeSetWaiting => "change_set_waiting",
            EventKind::ChangeSetProposed => "change_set_proposed",
            EventKind::ChangeSetSettled => "change_set_settled",
        }
    }

    /// What a person reads in a list of events.
    pub fn words(self) -> &'static str {
        match self {
            EventKind::ItemCreated => "an item was created",
            EventKind::ItemUpdated => "an item was edited",
            EventKind::ItemMoved => "an item moved",
            EventKind::ItemDeleted => "an item was deleted",
            EventKind::RunStarted => "a run started",
            EventKind::RunFinished => "a run finished",
            EventKind::RunFailed => "a run failed",
            EventKind::RunWaiting => "a run is waiting for a person",
            EventKind::DecisionOpened => "a decision was opened",
            EventKind::DecisionResolved => "a decision was answered",
            EventKind::ChangeSetWaiting => "a change set is waiting for a person",
            EventKind::ChangeSetProposed => "a change set was proposed",
            EventKind::ChangeSetSettled => "a change set was applied or rejected",
        }
    }

    fn about_items(self) -> bool {
        matches!(
            self,
            EventKind::ItemCreated
                | EventKind::ItemUpdated
                | EventKind::ItemMoved
                | EventKind::ItemDeleted
        )
    }
}

impl fmt::Display for EventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for EventKind {
    type Err = ();
    fn from_str(text: &str) -> Result<Self, ()> {
        EventKind::ALL
            .into_iter()
            .find(|kind| kind.as_str() == text)
            .ok_or(())
    }
}

/// What narrows an event trigger. Every field that is set must match.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Filter {
    /// For an item event: the status the item is in after the event.
    #[serde(default)]
    pub status: Option<String>,
    /// For `change_set_waiting`: how long the change set waits before it counts, in seconds.
    #[serde(default)]
    pub after_seconds: Option<u32>,
}

/// Weekdays and a time of day. `days` are ISO weekdays (1 Monday to 7 Sunday); the time is
/// the minute of the day in the zone `utc_offset_minutes` east of UTC names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Schedule {
    pub days: Vec<u8>,
    pub minute_of_day: u16,
    #[serde(default)]
    pub utc_offset_minutes: i32,
}

/// What wakes the profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum Source {
    Event {
        event: EventKind,
        #[serde(default)]
        filter: Filter,
    },
    Schedule(Schedule),
}

/// A rule that starts a run of a profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Trigger {
    pub id: Uuid,
    pub name: String,
    /// The project the runs happen in and, with a credential, the projects whose events count.
    pub scope: ThreadScope,
    /// The agent profile that runs.
    pub profile: String,
    pub source: Source,
    /// How long events must stay quiet before the run starts, in seconds.
    pub quiet_seconds: u32,
    /// The most runs in a day (24 hours).
    pub daily_cap: u32,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub last_fired_at: Option<DateTime<Utc>>,
}

/// The principal label a trigger's runs are signed with.
pub fn run_principal(trigger: Uuid) -> String {
    format!("trigger:{trigger}")
}

/// Why a trigger cannot be accepted, or `None` when it can.
pub fn problem(name: &str, source: &Source, quiet_seconds: u32, daily_cap: u32) -> Option<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > NAME_CHARS {
        return Some(format!("the name takes 1 to {NAME_CHARS} characters"));
    }
    if quiet_seconds > MAX_QUIET_SECONDS {
        return Some(format!(
            "the quiet period is at most {MAX_QUIET_SECONDS} seconds"
        ));
    }
    if daily_cap == 0 || daily_cap > MAX_DAILY_CAP {
        return Some(format!("the daily cap is 1 to {MAX_DAILY_CAP} runs"));
    }
    match source {
        Source::Event { event, filter } => {
            if filter.status.is_some() && !event.about_items() {
                return Some("a status filter fits only events about items".into());
            }
            if filter.after_seconds.is_some() && *event != EventKind::ChangeSetWaiting {
                return Some("`after_seconds` fits only change_set_waiting".into());
            }
            if *event == EventKind::ChangeSetWaiting && filter.after_seconds.is_none() {
                return Some("say how long a change set waits before it counts".into());
            }
            None
        }
        Source::Schedule(schedule) => {
            if schedule.days.is_empty() {
                return Some("choose at least one weekday".into());
            }
            if schedule.days.iter().any(|day| !(1..=7).contains(day)) {
                return Some("weekdays are 1 (Monday) to 7 (Sunday)".into());
            }
            if schedule.minute_of_day >= 24 * 60 {
                return Some("the time of day is 00:00 to 23:59".into());
            }
            if !(-14 * 60..=14 * 60).contains(&schedule.utc_offset_minutes) {
                return Some("the time zone is within 14 hours of UTC".into());
            }
            None
        }
    }
}

/// Something that happened, reduced to what a trigger looks at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Happened {
    pub kind: EventKind,
    pub project: Uuid,
    /// The principal label that caused it (`operator:local`, `agent:<id>`, `trigger:<id>`).
    pub cause: String,
    pub at: DateTime<Utc>,
    /// For an item event: the status after it.
    pub status: Option<String>,
    /// The item, run, decision or change set it is about, as text.
    pub subject: String,
    /// One line saying what, for the run's input.
    pub detail: String,
}

impl Filter {
    pub fn admits(&self, happened: &Happened) -> bool {
        self.status
            .as_ref()
            .is_none_or(|wanted| happened.status.as_ref() == Some(wanted))
    }
}

/// Whether `cause` is one of the principals the profile works under: the labels `own` lists.
/// An event a principal caused never starts a run of that same principal, so what the manager
/// itself applies, or what its own runs do, does not wake it.
pub fn is_echo(cause: &str, own: &[String]) -> bool {
    own.iter().any(|label| label == cause)
}

/// Whether `happened` belongs in `trigger`'s next run. `own` lists the labels the trigger's
/// profile works under (see [`is_echo`]); `reaches` says whether the event's project is one the
/// trigger's scope covers.
pub fn fires(trigger: &Trigger, happened: &Happened, own: &[String], reaches: bool) -> bool {
    if !trigger.enabled || !reaches || is_echo(&happened.cause, own) {
        return false;
    }
    match &trigger.source {
        Source::Event { event, filter } => *event == happened.kind && filter.admits(happened),
        Source::Schedule(_) => false,
    }
}

/// The labels a profile works under, from what Tack knows of it: the credentials its triggers
/// act as, and the triggers and threads that run it.
pub fn profile_labels(
    credentials: impl IntoIterator<Item = Uuid>,
    triggers: impl IntoIterator<Item = Uuid>,
    threads: impl IntoIterator<Item = Uuid>,
) -> Vec<String> {
    let mut labels = BTreeSet::new();
    for credential in credentials {
        labels.insert(format!("agent:{credential}"));
    }
    for trigger in triggers {
        labels.insert(run_principal(trigger));
    }
    for thread in threads {
        labels.insert(crate::thread::turn_principal(thread));
    }
    labels.into_iter().collect()
}

/// Whether a burst of events is ready to start its run: the newest is `quiet_seconds` old, or
/// the oldest has waited [`MAX_WAIT_SECONDS`].
pub fn burst_ready(
    oldest: DateTime<Utc>,
    newest: DateTime<Utc>,
    quiet_seconds: u32,
    now: DateTime<Utc>,
) -> bool {
    newest + Duration::seconds(i64::from(quiet_seconds)) <= now
        || oldest + Duration::seconds(MAX_WAIT_SECONDS) <= now
}

/// Whether the cap of runs for the day is used up.
pub fn cap_reached(daily_cap: u32, fired_in_last_day: usize) -> bool {
    fired_in_last_day >= daily_cap as usize
}

/// The latest moment up to `now` at which the schedule says to run.
pub fn latest_scheduled(schedule: &Schedule, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let offset = Duration::minutes(i64::from(schedule.utc_offset_minutes));
    let time = NaiveTime::from_hms_opt(
        u32::from(schedule.minute_of_day / 60),
        u32::from(schedule.minute_of_day % 60),
        0,
    )?;
    let today = (now + offset).date_naive();
    (0..=7).find_map(|back| {
        let date = today - Duration::days(back);
        let weekday = u8::try_from(date.weekday().number_from_monday()).ok()?;
        if !schedule.days.contains(&weekday) {
            return None;
        }
        let instant = date.and_time(time).and_utc() - offset;
        (instant <= now).then_some(instant)
    })
}

/// Whether the schedule has a moment after `since` (the last run, or the trigger's creation)
/// that has already come.
pub fn schedule_due(schedule: &Schedule, since: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    latest_scheduled(schedule, now).is_some_and(|moment| moment > since)
}

/// One event as the run's input lists it.
fn line(happened: &Happened) -> String {
    format!(
        "- {} {} ({}; project {}; by {}){}",
        happened.at.format("%Y-%m-%d %H:%M UTC"),
        happened.kind.words(),
        happened.subject,
        happened.project,
        happened.cause,
        if happened.detail.is_empty() {
            String::new()
        } else {
            format!(": {}", happened.detail)
        }
    )
}

/// What a run is told: why it started, every event since the last run (up to
/// [`MAX_LISTED`]), then the board's digest.
pub fn run_input(trigger: &Trigger, events: &[Happened], digest_json: &str) -> String {
    let why = match &trigger.source {
        Source::Event { .. } => format!(
            "You were started by the trigger \"{}\" because of {} event{} on the board, listed \
             below, oldest first.",
            trigger.name,
            events.len(),
            if events.len() == 1 { "" } else { "s" }
        ),
        Source::Schedule(_) => format!(
            "You were started by the schedule \"{}\"; no event started you, so read the digest.",
            trigger.name
        ),
    };
    let mut listed: Vec<String> = events.iter().take(MAX_LISTED).map(line).collect();
    if events.len() > MAX_LISTED {
        listed.push(format!("- and {} more", events.len() - MAX_LISTED));
    }
    let events_text = if listed.is_empty() {
        String::new()
    } else {
        format!("\nEvents:\n{}\n", listed.join("\n"))
    };
    format!(
        "{why}\n{events_text}\nBoard digest (computed, not written by a model; ask for detail \
         through your tools rather than guessing):\n{digest_json}\n\nAnything you want changed on \
         the board goes through a change set, which a person applies unless the project lets you \
         act. When you propose one, give it the origin `trigger` and the origin_ref `{id}`.\n",
        id = trigger.id
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn trigger(source: Source) -> Trigger {
        Trigger {
            id: Uuid::from_u128(1),
            name: "Failed runs".into(),
            scope: ThreadScope::Project {
                project: Uuid::from_u128(9),
            },
            profile: "manager".into(),
            source,
            quiet_seconds: 60,
            daily_cap: 5,
            enabled: true,
            created_at: at("2026-11-01T00:00:00Z"),
            last_fired_at: None,
        }
    }

    fn failed(cause: &str) -> Happened {
        Happened {
            kind: EventKind::RunFailed,
            project: Uuid::from_u128(9),
            cause: cause.into(),
            at: at("2026-11-02T10:00:00Z"),
            status: None,
            subject: "run r1".into(),
            detail: String::new(),
        }
    }

    fn on_failure() -> Trigger {
        trigger(Source::Event {
            event: EventKind::RunFailed,
            filter: Filter::default(),
        })
    }

    #[test]
    fn an_event_of_the_kind_fires_and_another_does_not() {
        let own = vec![];
        assert!(fires(&on_failure(), &failed("runner:r"), &own, true));
        let mut finished = failed("runner:r");
        finished.kind = EventKind::RunFinished;
        assert!(!fires(&on_failure(), &finished, &own, true));
    }

    #[test]
    fn an_event_the_profile_caused_never_fires_it() {
        let credential = Uuid::from_u128(5);
        let thread = Uuid::from_u128(6);
        let other_trigger = Uuid::from_u128(7);
        let own = profile_labels([credential], [other_trigger], [thread]);
        for cause in [
            format!("agent:{credential}"),
            format!("thread:{thread}"),
            format!("trigger:{other_trigger}"),
        ] {
            assert!(
                !fires(&on_failure(), &failed(&cause), &own, true),
                "{cause}"
            );
        }
        assert!(fires(&on_failure(), &failed("operator:local"), &own, true));
        assert!(fires(
            &on_failure(),
            &failed("agent:someone-else"),
            &own,
            true
        ));
    }

    #[test]
    fn a_disabled_trigger_and_a_project_out_of_reach_do_not_fire() {
        let mut off = on_failure();
        off.enabled = false;
        assert!(!fires(&off, &failed("runner:r"), &[], true));
        assert!(!fires(&on_failure(), &failed("runner:r"), &[], false));
    }

    #[test]
    fn a_status_filter_keeps_only_items_in_that_status() {
        let mut moved = failed("operator:local");
        moved.kind = EventKind::ItemMoved;
        moved.status = Some("In Review".into());
        let wants = |status: &str| {
            trigger(Source::Event {
                event: EventKind::ItemMoved,
                filter: Filter {
                    status: Some(status.into()),
                    after_seconds: None,
                },
            })
        };
        assert!(fires(&wants("In Review"), &moved, &[], true));
        assert!(!fires(&wants("Done"), &moved, &[], true));
    }

    #[test]
    fn a_burst_waits_for_quiet_but_not_forever() {
        let first = at("2026-11-02T10:00:00Z");
        let last = at("2026-11-02T10:00:30Z");
        assert!(!burst_ready(first, last, 60, at("2026-11-02T10:01:00Z")));
        assert!(burst_ready(first, last, 60, at("2026-11-02T10:01:30Z")));
        // Events keep coming: the oldest has waited an hour.
        let newest = at("2026-11-02T11:00:00Z");
        assert!(burst_ready(first, newest, 60, at("2026-11-02T11:00:10Z")));
    }

    #[test]
    fn the_daily_cap_counts_runs() {
        assert!(!cap_reached(2, 1));
        assert!(cap_reached(2, 2));
    }

    #[test]
    fn a_schedule_is_due_once_per_moment() {
        // Monday to Friday at 09:00 in a zone one hour east of UTC: 08:00 UTC.
        let schedule = Schedule {
            days: vec![1, 2, 3, 4, 5],
            minute_of_day: 9 * 60,
            utc_offset_minutes: 60,
        };
        // 2026-11-02 is a Monday.
        let created = at("2026-11-02T07:00:00Z");
        assert!(!schedule_due(
            &schedule,
            created,
            at("2026-11-02T07:59:00Z")
        ));
        assert!(schedule_due(&schedule, created, at("2026-11-02T08:00:00Z")));
        let fired = at("2026-11-02T08:00:05Z");
        assert!(!schedule_due(&schedule, fired, at("2026-11-02T20:00:00Z")));
        // Saturday and Sunday are skipped; Monday comes round again.
        let friday = at("2026-11-06T08:00:05Z");
        assert!(!schedule_due(&schedule, friday, at("2026-11-07T20:00:00Z")));
        assert_eq!(
            latest_scheduled(&schedule, at("2026-11-08T20:00:00Z")),
            Some(at("2026-11-06T08:00:00Z"))
        );
        assert!(schedule_due(&schedule, fired, at("2026-11-03T08:00:00Z")));
    }

    #[test]
    fn a_schedule_made_after_its_time_waits_for_the_next() {
        let schedule = Schedule {
            days: vec![1],
            minute_of_day: 9 * 60,
            utc_offset_minutes: 0,
        };
        let created = at("2026-11-02T14:00:00Z");
        assert!(!schedule_due(
            &schedule,
            created,
            at("2026-11-02T15:00:00Z")
        ));
        assert!(schedule_due(&schedule, created, at("2026-11-09T09:00:00Z")));
    }

    #[test]
    fn what_cannot_be_accepted_says_why() {
        let event = Source::Event {
            event: EventKind::RunFailed,
            filter: Filter::default(),
        };
        assert!(problem("Failed runs", &event, 60, 5).is_none());
        assert!(problem("  ", &event, 60, 5).is_some());
        assert!(problem("x", &event, MAX_QUIET_SECONDS + 1, 5).is_some());
        assert!(problem("x", &event, 60, 0).is_some());
        let waiting = Source::Event {
            event: EventKind::ChangeSetWaiting,
            filter: Filter::default(),
        };
        assert!(problem("x", &waiting, 60, 5).is_some());
        let status_on_run = Source::Event {
            event: EventKind::RunFailed,
            filter: Filter {
                status: Some("Done".into()),
                after_seconds: None,
            },
        };
        assert!(problem("x", &status_on_run, 60, 5).is_some());
        let no_days = Source::Schedule(Schedule {
            days: vec![],
            minute_of_day: 0,
            utc_offset_minutes: 0,
        });
        assert!(problem("x", &no_days, 0, 1).is_some());
    }

    #[test]
    fn the_input_lists_every_event_up_to_the_bound() {
        let events: Vec<Happened> = (0..60).map(|_| failed("runner:r")).collect();
        let text = run_input(&on_failure(), &events, "{}");
        assert!(text.contains("60 events"));
        assert_eq!(text.matches("a run failed").count(), MAX_LISTED);
        assert!(text.contains("- and 10 more"));
    }

    #[test]
    fn a_schedule_run_says_no_event_started_it() {
        let scheduled = trigger(Source::Schedule(Schedule {
            days: vec![1],
            minute_of_day: 540,
            utc_offset_minutes: 0,
        }));
        let text = run_input(&scheduled, &[], "{}");
        assert!(text.contains("no event started you"));
        assert!(!text.contains("Events:"));
    }

    #[test]
    fn kinds_round_trip_through_their_names() {
        for kind in EventKind::ALL {
            assert_eq!(kind.as_str().parse::<EventKind>(), Ok(kind));
        }
    }
}
