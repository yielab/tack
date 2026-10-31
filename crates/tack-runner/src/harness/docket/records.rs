//! What a team wrote in one task, as the run keeps it: for each record set the team declares, the
//! rows whose `_task` is this task. Every other row stays in docket, and nothing here writes.

use serde_json::{Value, json};

use super::{
    client::Client,
    wire::{RecordList, RecordRows},
};
use crate::evidence::{PlanSubtask, PlanV1};

/// Most rows kept per set; a set with more says it was cut.
const ROWS_KEPT: u32 = 50;

/// Whether `text` is made only of the characters docket gives its ids: letters, digits, `-` and
/// `_`. The statement below holds both inside SQL, so nothing else is allowed in.
fn is_id(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The one statement that reads a task's rows of a set, or `None` when either name is not an id.
fn statement(set: &str, task: &str) -> Option<String> {
    (is_id(set) && is_id(task)).then(|| format!("SELECT * FROM {set} WHERE _task = '{task}'"))
}

/// The run's `records`: `[{set, columns, rows, cut}]`, the sets with no row of the task left out.
/// Empty when the team keeps no records or docket cannot say (an older one has no such route).
pub(super) async fn kept(client: &Client, pod: &str, task: &str) -> Vec<Value> {
    let Some(sets) = declared(client, pod).await else {
        return Vec::new();
    };
    let mut kept = Vec::new();
    for set in sets {
        let Some(sql) = statement(&set, task) else {
            tracing::warn!(set, task, "a record set or task id is not an id; skipped");
            continue;
        };
        let reply = match client.record_rows(pod, &set, ROWS_KEPT, &sql).await {
            Ok(reply) => reply,
            Err(why) => {
                tracing::warn!(why, set, "docket could not be reached to read a record set");
                continue;
            }
        };
        if reply.status != 200 {
            tracing::warn!(
                set,
                status = reply.status,
                why = reply.error(),
                "docket refused to read a record set"
            );
            continue;
        }
        let Ok(read) = reply.read::<RecordRows>().inspect_err(|why| {
            tracing::warn!(why, set, "docket's rows of a record set could not be read")
        }) else {
            continue;
        };
        if read.rows.is_empty() {
            continue;
        }
        let cut = match &read.cut {
            Value::Bool(cut) => *cut,
            Value::Number(count) => count.as_u64().unwrap_or(0) > 0,
            _ => false,
        };
        kept.push(json!({"set": set, "columns": read.columns, "rows": read.rows, "cut": cut}));
    }
    kept
}

/// The record set a team writes its proposed items in, one row for each item.
const PROPOSALS: &str = "tack_proposals";

/// The plan-v1 document the team's rows of `tack_proposals` make, or the reason they make none
/// (it names the row's `ref`). `None` when the team wrote no such row in this task. `records` is
/// what [`kept`] returned.
pub(super) fn proposal(records: &[Value]) -> Option<Result<Value, String>> {
    let rows = records
        .iter()
        .find(|set| set["set"] == PROPOSALS)?
        .get("rows")?
        .as_array()?;
    Some(plan(rows))
}

/// A column's text, trimmed; `None` when the row leaves it empty. A number is its digits.
fn text(row: &Value, column: &str) -> Option<String> {
    let text = match &row[column] {
        Value::String(text) => text.trim().to_owned(),
        Value::Number(number) => number.to_string(),
        _ => return None,
    };
    (!text.is_empty()).then_some(text)
}

/// The rows, in order, as one plan: a `ref` becomes the row's index, `parent` and `depends_on`
/// name other rows by their `ref`, and `acceptance` is one sentence a line.
fn plan(rows: &[Value]) -> Result<Value, String> {
    let mut refs: Vec<String> = Vec::with_capacity(rows.len());
    for (at, row) in rows.iter().enumerate() {
        let Some(name) = text(row, "ref") else {
            return Err(format!("row {} has no ref", at + 1));
        };
        if refs.contains(&name) {
            return Err(format!("'{name}': two rows have this ref"));
        }
        refs.push(name);
    }
    let index_of = |name: &str| refs.iter().position(|found| found == name);
    let mut subtasks = Vec::with_capacity(rows.len());
    for (at, row) in rows.iter().enumerate() {
        let name = &refs[at];
        let Some(title) = text(row, "title") else {
            return Err(format!("'{name}': the row has no title"));
        };
        let parent = match text(row, "parent") {
            None => None,
            Some(parent) => match index_of(&parent).filter(|index| *index < at) {
                Some(index) => Some(index as u32),
                None => {
                    return Err(format!(
                        "'{name}': parent '{parent}' is not the ref of an earlier row"
                    ));
                }
            },
        };
        let mut depends_on: Vec<u32> = Vec::new();
        let wanted = text(row, "depends_on").unwrap_or_default();
        for blocker in wanted.split(|c: char| c == ',' || c.is_whitespace()) {
            if blocker.is_empty() {
                continue;
            }
            match index_of(blocker).filter(|index| *index != at) {
                Some(index) if depends_on.contains(&(index as u32)) => {}
                Some(index) => depends_on.push(index as u32),
                None => {
                    return Err(format!(
                        "'{name}': depends_on '{blocker}' is not the ref of another row"
                    ));
                }
            }
        }
        subtasks.push(PlanSubtask {
            title,
            description: text(row, "description").unwrap_or_default(),
            acceptance: text(row, "acceptance")
                .unwrap_or_default()
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect(),
            depends_on,
            estimate: None,
            item_type: text(row, "type"),
            role: text(row, "role"),
            team: text(row, "team"),
            parent,
        });
    }
    let plan = PlanV1 {
        v: "1".to_owned(),
        subtasks,
    };
    serde_json::to_value(plan).map_err(|why| why.to_string())
}

/// The names of the sets the team declares, or `None` when docket does not say.
async fn declared(client: &Client, pod: &str) -> Option<Vec<String>> {
    let reply = client.records(pod).await.ok().filter(|r| r.status == 200)?;
    let list: RecordList = reply
        .read()
        .inspect_err(|why| tracing::warn!(why, "docket's record sets could not be read"))
        .ok()?;
    Some(list.records.into_iter().map(|set| set.name).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_statement_is_built_only_from_ids() {
        assert_eq!(
            statement("leads", "task-00_a").as_deref(),
            Some("SELECT * FROM leads WHERE _task = 'task-00_a'")
        );
        for bad in ["", "a b", "x'; DROP TABLE y; --", "a.b", "ñ", "a;b"] {
            assert_eq!(statement(bad, "task-1"), None, "{bad:?}");
            assert_eq!(statement("leads", bad), None, "{bad:?}");
        }
    }
}
