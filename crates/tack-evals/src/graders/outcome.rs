//! Did the work come out right: the hidden tests, the run's end state, the files it touched,
//! the plan it wrote.

use serde_json::Value;

use super::{CheckResult, GradeInput};

/// The agent's tree passed the exam it never saw.
pub fn hidden_tests(input: &GradeInput<'_>) -> CheckResult {
    const NAME: &str = "outcome.hidden_tests";
    let Some(exam) = &input.record.exam else {
        return CheckResult::error(NAME, "the trial recorded no exam");
    };
    if exam.passed {
        return CheckResult::pass(NAME, "the hidden tests pass on the agent's tree");
    }
    let how = if exam.timed_out {
        "timed out".to_owned()
    } else {
        format!(
            "exit {}",
            exam.exit_code.map_or("?".to_owned(), |c| c.to_string())
        )
    };
    CheckResult::fail(NAME, format!("the hidden tests fail ({how})"))
}

/// The run ended in the state the task wants (`succeeded`, not `failed` or `cancelled`).
pub fn run_state(args: &Value, input: &GradeInput<'_>) -> CheckResult {
    const NAME: &str = "outcome.run_state";
    let Some(wanted) = args.as_str() else {
        return CheckResult::error(NAME, "the argument is the state wanted, e.g. `succeeded`");
    };
    match input.record.run_state.as_deref() {
        Some(state) if state == wanted => CheckResult::pass(NAME, format!("the run is {state}")),
        Some(state) => CheckResult::fail(NAME, format!("the run is {state}, wanted {wanted}")),
        None => CheckResult::fail(NAME, "the run has no state"),
    }
}

/// The patch touches exactly what the task allows: only `only`, and at least `changed`. A path
/// ending in `/` stands for everything under it.
pub fn files(args: &Value, input: &GradeInput<'_>) -> CheckResult {
    const NAME: &str = "outcome.files";
    let list = |key: &str| -> Result<Option<Vec<String>>, String> {
        match args.get(key) {
            None => Ok(None),
            Some(Value::Array(items)) => items
                .iter()
                .map(|item| item.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
                .map(Some)
                .ok_or_else(|| format!("`{key}` is a list of paths")),
            Some(_) => Err(format!("`{key}` is a list of paths")),
        }
    };
    let (only, changed) = match (list("only"), list("changed")) {
        (Ok(only), Ok(changed)) => (only, changed),
        (Err(why), _) | (_, Err(why)) => return CheckResult::error(NAME, why),
    };
    if only.is_none() && changed.is_none() {
        return CheckResult::error(NAME, "name `only`, `changed` or both");
    }
    let Some(frozen) = input.frozen else {
        return CheckResult::error(NAME, "the frozen run is missing");
    };
    // No patch is no change, which `only` allows and `changed` does not.
    // Running the tests leaves bytecode caches; they are not the agent's work.
    let touched: Vec<String> = frozen
        .changed_files()
        .unwrap_or_default()
        .into_iter()
        .filter(|path| {
            !path.split('/').any(|part| part == "__pycache__") && !path.ends_with(".pyc")
        })
        .collect();
    let matches = |pattern: &str, path: &str| {
        if let Some(prefix) = pattern.strip_suffix('/') {
            path.starts_with(&format!("{prefix}/"))
        } else {
            path == pattern
        }
    };
    if let Some(only) = only {
        let stray: Vec<&String> = touched
            .iter()
            .filter(|path| !only.iter().any(|pattern| matches(pattern, path)))
            .collect();
        if !stray.is_empty() {
            return CheckResult::fail(
                NAME,
                format!("changed outside the allowed files: {stray:?}"),
            );
        }
    }
    if let Some(changed) = changed {
        let missing: Vec<&String> = changed
            .iter()
            .filter(|pattern| !touched.iter().any(|path| matches(pattern, path)))
            .collect();
        if !missing.is_empty() {
            return CheckResult::fail(NAME, format!("did not change {missing:?}"));
        }
    }
    CheckResult::pass(NAME, format!("changed {touched:?}"))
}

/// The plan the run staged is well formed: `plan-v1`, a number of subtasks in range, each with
/// a title and at least one acceptance check, and dependencies that point at a subtask.
pub fn plan(args: &Value, input: &GradeInput<'_>) -> CheckResult {
    const NAME: &str = "outcome.plan";
    let bound = |key: &str| args.get(key).and_then(Value::as_u64).map(|n| n as usize);
    let (min, max) = (bound("min").unwrap_or(1), bound("max"));
    let Some(frozen) = input.frozen else {
        return CheckResult::error(NAME, "the frozen run is missing");
    };
    let Some(bytes) = frozen.artifact("tack-plan.json") else {
        return CheckResult::fail(NAME, "the run staged no tack-plan.json");
    };
    let Ok(plan) = serde_json::from_slice::<Value>(&bytes) else {
        return CheckResult::fail(NAME, "tack-plan.json is not JSON");
    };
    if plan["v"] != "1" {
        return CheckResult::fail(NAME, "tack-plan.json is not plan-v1 (`\"v\": \"1\"`)");
    }
    let Some(subtasks) = plan["subtasks"].as_array() else {
        return CheckResult::fail(NAME, "the plan has no `subtasks` list");
    };
    let count = subtasks.len();
    if count < min || max.is_some_and(|max| count > max) {
        let range = max.map_or_else(
            || format!("at least {min}"),
            |max| format!("{min} to {max}"),
        );
        return CheckResult::fail(
            NAME,
            format!("the plan has {count} subtasks, wanted {range}"),
        );
    }
    for (index, subtask) in subtasks.iter().enumerate() {
        let titled = subtask["title"]
            .as_str()
            .is_some_and(|t| !t.trim().is_empty());
        let checked = subtask["acceptance"].as_array().is_some_and(|a| {
            !a.is_empty()
                && a.iter()
                    .all(|c| c.as_str().is_some_and(|c| !c.trim().is_empty()))
        });
        if !titled || !checked {
            return CheckResult::fail(
                NAME,
                format!("subtask {index} needs a title and at least one acceptance check"),
            );
        }
        let dangling = subtask["depends_on"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|dep| dep.as_u64().is_none_or(|d| d as usize >= count));
        if dangling {
            return CheckResult::fail(
                NAME,
                format!("subtask {index} depends on a subtask that does not exist"),
            );
        }
    }
    CheckResult::pass(
        NAME,
        format!("a plan of {count} subtasks, each with a check"),
    )
}
