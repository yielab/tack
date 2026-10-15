//! A task that repeats: when the next one is due, and what it copies.

use chrono::{Datelike, Duration, NaiveDate, NaiveTime, TimeZone, Utc};

use crate::models::{CreateItem, Item, Recurrence};

fn last_day_of_month(year: i32, month: u32) -> NaiveDate {
    let (y, m) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(y, m, 1)
        .and_then(|first| first.pred_opt())
        .expect("a month has a last day")
}

/// The end of the next period after `from`: a week on, the last day of the next
/// month, or the last day of the quarter after the one `from` is in.
pub fn next_due(from: NaiveDate, r: Recurrence) -> NaiveDate {
    match r {
        Recurrence::Weekly => from + Duration::days(7),
        Recurrence::Monthly => {
            let (y, m) = (from.year(), from.month());
            if m == 12 {
                last_day_of_month(y + 1, 1)
            } else {
                last_day_of_month(y, m + 1)
            }
        }
        Recurrence::Quarterly => {
            let quarter_end = (from.month() - 1) / 3 * 3 + 3;
            let months = quarter_end + 3;
            last_day_of_month(
                from.year() + (months - 1) as i32 / 12,
                (months - 1) % 12 + 1,
            )
        }
    }
}

/// The task that follows a closed one: its words and shape, due on `due`, with
/// no sprint and no assignee. The caller gives the repository the first to-do
/// status, since `CreateItem` carries none.
pub fn clone_for_next(item: &Item, due: NaiveDate) -> CreateItem {
    CreateItem {
        title: item.title.clone(),
        description: item.description.clone(),
        item_type: Some(item.item_type.clone()),
        parent_id: item.parent_id,
        priority: Some(item.priority.clone()),
        estimate: item.estimate,
        estimate_unit: Some(item.estimate_unit.clone()),
        tags: Some(item.tags.clone()),
        due_date: Some(Utc.from_utc_datetime(&due.and_time(NaiveTime::MIN))),
        sprint_id: None,
        assignee: None,
        recurrence: item.recurrence,
        recurrence_of: Some(item.id),
        source_artifact_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{EstimateUnit, Item, ItemSource, ItemType, Priority, Recurrence};
    use chrono::{NaiveDate, Utc};
    use uuid::Uuid;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn next_due_is_the_end_of_the_next_period() {
        use Recurrence::*;
        for (from, r, want) in [
            (d(2026, 10, 14), Weekly, d(2026, 10, 21)),
            (d(2026, 12, 28), Weekly, d(2027, 1, 4)),
            (d(2026, 1, 31), Monthly, d(2026, 2, 28)),
            (d(2027, 12, 5), Monthly, d(2028, 1, 31)),
            (d(2027, 1, 15), Monthly, d(2027, 2, 28)),
            (d(2028, 1, 10), Monthly, d(2028, 2, 29)),
            (d(2026, 2, 3), Quarterly, d(2026, 6, 30)),
            (d(2026, 3, 31), Quarterly, d(2026, 6, 30)),
            (d(2026, 11, 30), Quarterly, d(2027, 3, 31)),
        ] {
            assert_eq!(next_due(from, r), want, "{from} {r:?}");
        }
    }

    #[test]
    fn clone_for_next_copies_what_the_plan_lists_and_nothing_else() {
        let now = Utc::now();
        let item = Item {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            parent_id: Some(Uuid::new_v4()),
            title: "Pay rent".into(),
            description: Some("transfer".into()),
            item_type: ItemType::Bug,
            status: "Done".into(),
            priority: Priority::High,
            estimate: Some(2.0),
            estimate_unit: EstimateUnit::Hours,
            tags: vec!["home".into()],
            sort_order: 3,
            sprint_id: Some(Uuid::new_v4()),
            assignee: Some("ana".into()),
            due_date: Some(now),
            recurrence: Some(Recurrence::Monthly),
            recurrence_of: Some(Uuid::new_v4()),
            started_at: Some(now),
            completed_at: Some(now),
            source: ItemSource::Manual,
            source_artifact_id: Some("art".into()),
            run_settings: None,
            needs_review: true,
            merge_note: None,
            created_at: now,
            updated_at: now,
        };
        let due = d(2026, 11, 30);
        let next = clone_for_next(&item, due);
        assert_eq!(next.title, "Pay rent");
        assert_eq!(next.description.as_deref(), Some("transfer"));
        assert_eq!(next.item_type, Some(ItemType::Bug));
        assert_eq!(next.priority, Some(Priority::High));
        assert_eq!(next.tags, Some(vec!["home".to_string()]));
        assert_eq!(next.parent_id, item.parent_id);
        assert_eq!(next.estimate, Some(2.0));
        assert_eq!(next.estimate_unit, Some(EstimateUnit::Hours));
        assert_eq!(next.recurrence, Some(Recurrence::Monthly));
        assert_eq!(next.recurrence_of, Some(item.id));
        assert_eq!(
            next.due_date.map(|t| t.date_naive()),
            Some(due),
            "the due date is the one passed in"
        );
        assert_eq!(next.sprint_id, None);
        assert_eq!(next.assignee, None);
        assert_eq!(next.source_artifact_id, None);
    }
}
