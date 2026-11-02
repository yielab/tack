//! The tools that read the board as a whole: the digest, the teams and what they are doing, and
//! a project's roles.

use serde_json::{Value, json};

use super::projection::{card_fence_id, fence_field};
use super::tool::{Kind, Spec, Tool, call, check_id, opt_str, require_id, require_str, urlencode};
use crate::client::TackClient;

/// How many of a team's tasks `team_activity` lists; the rest are counted.
const ACTIVITY_TASKS: usize = 20;

pub static DIGEST: Tool = Tool {
    name: "digest",
    spec: Spec {
        description: "A bounded summary of the projects you may read: items per status, what \
             waits on a person and since when, blocked chains, tasks no team will pick up and \
             why, stale items, runs that failed since `since`, tokens against the limits, each \
             team's current work and queue, and the change sets waiting. Every list shows a \
             fixed number of rows and says how many more there are. Read it first; ask for \
             detail after.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "projects": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Project UUIDs to describe; omit for every project you may read"
                    },
                    "since": { "type": "string", "description": "RFC3339; runs that failed before it are left out. Default: the last 24 hours" }
                }
            })
        },
    },
    kind: Kind::Read,
    handler: digest,
};

fn digest(client: &TackClient, args: &Value) -> Result<Value, String> {
    let mut query = Vec::new();
    if let Some(projects) = args.get("projects").filter(|value| !value.is_null()) {
        let ids = projects
            .as_array()
            .ok_or("projects must be an array of project ids")?;
        let ids: Vec<String> = ids
            .iter()
            .map(|id| check_id("projects", id.as_str().unwrap_or_default().to_string()))
            .collect::<Result<_, _>>()?;
        if !ids.is_empty() {
            query.push(format!("projects={}", ids.join(",")));
        }
    }
    if let Some(since) = opt_str(args, "since") {
        query.push(format!("since={}", urlencode(&since)));
    }
    let suffix = if query.is_empty() {
        String::new()
    } else {
        format!("?{}", query.join("&"))
    };
    let mut digest = call(client.get(&format!("/digest{suffix}")))?;
    // Two rows carry text somebody else wrote: a failed run's reason and a change set's.
    let fence = card_fence_id();
    for project in digest["projects"]["rows"]
        .as_array_mut()
        .into_iter()
        .flatten()
    {
        for section in ["failed_runs", "change_sets"] {
            for row in project[section]["rows"]
                .as_array_mut()
                .into_iter()
                .flatten()
            {
                fence_field(row, "reason", &fence);
            }
        }
    }
    Ok(digest)
}

pub static TEAM_ACTIVITY: Tool = Tool {
    name: "team_activity",
    spec: Spec {
        description: "What one team is doing: the tasks it holds, the ones still going first, \
             with each task's state, what it waits on and the step it is in. Work the board did \
             not start (a person or a routine gave it to the team directly) is included, with \
             `on_board` absent. With an agent credential you see only the tasks of the projects \
             it may read, and the work the board did not start only if it reads every project.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "team": { "type": "string", "description": "The team's name, as list_teams gives it" },
                    "harness": { "type": "string", "description": "Which agent's team, when two report a team of that name" }
                },
                "required": ["team"]
            })
        },
    },
    kind: Kind::Read,
    handler: team_activity,
};

fn team_activity(client: &TackClient, args: &Value) -> Result<Value, String> {
    let team = require_str(args, "team")?;
    let suffix = opt_str(args, "harness")
        .map(|harness| format!("?harness={}", urlencode(&harness)))
        .unwrap_or_default();
    let activity = call(client.get(&format!("/teams/{}/activity{suffix}", urlencode(&team))))?;
    let all = activity["tasks"].as_array().cloned().unwrap_or_default();
    let tasks: Vec<Value> = all.iter().take(ACTIVITY_TASKS).map(compact_task).collect();
    let mut compact = json!({
        "team": activity["team"],
        "harness": activity["harness"],
        "supported": activity["supported"],
        "reported_at": activity["reported_at"],
        "tasks": tasks,
        "count": all.len(),
        "more": all.len().saturating_sub(ACTIVITY_TASKS),
    });
    if !activity["reason"].is_null() {
        compact["reason"] = activity["reason"].clone();
    }
    Ok(compact)
}

/// One task of a team: where it is and what it waits on, without the steps' own words.
fn compact_task(task: &Value) -> Value {
    let mut row = json!({
        "task": task["ref"],
        "state": task["state"],
        "finished_steps": task["finished_steps"].as_array().map_or(0, Vec::len),
    });
    for key in ["on_board", "waits_on", "changed_at"] {
        if !task[key].is_null() {
            row[key] = task[key].clone();
        }
    }
    if let Some(step) = task["current_step"].as_object() {
        let mut current = json!({ "name": step.get("name") });
        for key in ["role", "since"] {
            if let Some(value) = step.get(key) {
                current[key] = value.clone();
            }
        }
        row["step"] = current;
    }
    row
}

pub static LIST_TEAMS: Tool = Tool {
    name: "list_teams",
    spec: Spec {
        description: "List the teams agents report: who is in each and what they can do, whether \
             it runs to the end or asks a person, its queue, and the agent profiles that name \
             it (the ids to give a task with who_does_it).",
        input_schema: || json!({ "type": "object", "properties": {} }),
    },
    kind: Kind::Read,
    handler: list_teams,
};

fn list_teams(client: &TackClient, _args: &Value) -> Result<Value, String> {
    let list = call(client.get("/teams"))?;
    let teams: Vec<Value> = list["teams"]
        .as_array()
        .map(|teams| teams.iter().map(compact_team).collect())
        .unwrap_or_default();
    Ok(json!({ "count": teams.len(), "teams": teams }))
}

fn compact_team(team: &Value) -> Value {
    let mut row = json!({
        "name": team["name"],
        "harness": team["harness"],
        "reported": team["reported"],
        "profiles": team["profiles"],
    });
    let report = &team["report"];
    for key in ["decides", "queue", "works_in", "network", "shape"] {
        if !report[key].is_null() {
            row[key] = report[key].clone();
        }
    }
    if let Some(members) = report["members"].as_array() {
        row["members"] = json!(members);
    }
    if let Some(steps) = report["steps"].as_array() {
        let names: Vec<&Value> = steps.iter().map(|step| &step["name"]).collect();
        row["steps"] = json!(names);
    }
    row
}

pub static LIST_ROLES: Tool = Tool {
    name: "list_roles",
    spec: Spec {
        description: "List a project's roles and the agent profile each one runs with. A task \
             with a role is done by that profile unless it names another.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": { "project_id": { "type": "string", "description": "Project UUID" } },
                "required": ["project_id"]
            })
        },
    },
    kind: Kind::Read,
    handler: list_roles,
};

fn list_roles(client: &TackClient, args: &Value) -> Result<Value, String> {
    let project_id = require_id(args, "project_id")?;
    let roles = call(client.get(&format!("/projects/{project_id}/roles")))?;
    let rows: Vec<Value> = roles
        .as_array()
        .map(|roles| {
            roles
                .iter()
                .map(|role| {
                    json!({
                        "id": role["id"],
                        "name": role["name"],
                        "agent_profile_id": role["agent_profile_id"],
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(json!({ "roles": rows, "count": rows.len() }))
}
