//! The tools that read and create projects.

use serde_json::{Value, json};

use super::changes;
use super::projection::{project_list, project_summary};
use super::tool::{Kind, Spec, Tool, call, opt_str, require_id, require_str, require_u64};
use crate::client::TackClient;

pub static LIST_PROJECTS: Tool = Tool {
    name: "list_projects",
    spec: Spec {
        description: "List all projects.",
        input_schema: || {
            json!({
                "type": "object", "properties": {}
            })
        },
    },
    kind: Kind::Read,
    handler: list_projects,
};

fn list_projects(client: &TackClient, _args: &Value) -> Result<Value, String> {
    let projects = call(client.get("/projects"))?;
    Ok(project_list(&projects))
}

pub static GET_AUTONOMY: Tool = Tool {
    name: "get_autonomy",
    spec: Spec {
        description: "Read how far Tack may merge a project's changes on its own: level, domains, required verification stages, budgets. Read-only.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": { "project_id": { "type": "string", "description": "Project UUID" } },
                "required": ["project_id"]
            })
        },
    },
    kind: Kind::Read,
    handler: get_autonomy,
};

fn get_autonomy(client: &TackClient, args: &Value) -> Result<Value, String> {
    let project_id = require_id(args, "project_id")?;
    call(client.get(&format!("/projects/{project_id}/autonomy")))
}

pub static CREATE_PROJECT: Tool = Tool {
    name: "create_project",
    spec: Spec {
        description: "Create an empty project. Returns its id and its workflow's status names.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "project_type": { "type": "string", "description": "custom (default), software, web, mobile, construction, personal, homework, maintenance, legal, research, event — sets the workflow and vocabulary" },
                    "description": { "type": "string" }
                },
                "required": ["name"]
            })
        },
    },
    kind: Kind::Write,
    handler: create_project,
};

fn create_project(client: &TackClient, args: &Value) -> Result<Value, String> {
    let name = require_str(args, "name")?;
    let body = json!({
        "name": name,
        "project_type": opt_str(args, "project_type")
            .map(|t| t.to_lowercase())
            .unwrap_or_else(|| "custom".into()),
        "description": opt_str(args, "description"),
    });
    let project = call(client.post("/projects", &body))?;
    Ok(project_summary(&project))
}

pub static CREATE_OUTLINE: Tool = Tool {
    name: "create_outline",
    spec: Spec {
        description: "Create many items in one call, with their hierarchy and dependencies: \
             in a new project (pass project) or an existing one (pass project_id). \
             Every item is checked before anything is written, and every problem \
             is reported at once; nothing is written unless all of it can be. \
             Give an item a ref and name it from other items' parent or \
             depends_on (an exact title works too). Order does not matter. \
             Use dry_run first to see what would be created and any warnings. \
             The file may also carry fields, profiles, roles, sprints and \
             automation, as in Tack's project file (Settings → Data → File format). With \
             an agent credential this proposes a change set: pass project_id, and no \
             assignee, estimate or tags, project file parts or dry_run. A reason may be given. \
             It never removes anything; a person keeps the plan in the repository with `tack folder`.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string", "description": "Add to this project (mutually exclusive with project)" },
                    "project": {
                        "type": "object",
                        "description": "Create this project: {name, project_type?, description?}; project_type as in create_project",
                        "properties": {
                            "name": { "type": "string" },
                            "project_type": { "type": "string" },
                            "description": { "type": "string" }
                        },
                        "required": ["name"]
                    },
                    "items": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "ref": { "type": "string", "description": "A name for this item, unique in the outline" },
                                "title": { "type": "string" },
                                "description": { "type": "string" },
                                "type": { "type": "string", "description": "A key or name of the project's types; omitted, its default work type; a word the project lacks is added as a work type" },
                                "status": { "type": "string", "description": "A workflow status (any case); default the first one" },
                                "priority": { "type": "string", "description": "critical, high, medium (default), low, none" },
                                "assignee": { "type": "string" },
                                "estimate": { "type": "number" },
                                "due_date": { "type": "string", "description": "YYYY-MM-DD" },
                                "tags": { "type": "array", "items": { "type": "string" } },
                                "parent": { "type": "string", "description": "Parent's ref or title; with project_id, also an existing item's id" },
                                "depends_on": { "type": "array", "items": { "type": "string" }, "description": "Refs or titles of items that must finish first" }
                            },
                            "required": ["title"]
                        }
                    },
                    "fields": { "type": "array", "description": "Custom fields the project's items carry" },
                    "profiles": { "type": "array", "description": "Agent profiles the project uses" },
                    "roles": { "type": "array", "description": "Roles and the profile each one uses" },
                    "sprints": { "type": "array", "description": "Sprints" },
                    "types": { "type": "array", "description": "The project's item types (key, name, level group or work, color, default_work); a person's credential only" },
                    "automation": { "type": "object", "description": "The project's automation settings" },
                    "dry_run": { "type": "boolean", "description": "Check and report without writing" },
                    "reason": { "type": "string", "description": "Why, for the person who reads the change set; used when you hold an agent credential" }
                },
                "required": ["items"]
            })
        },
    },
    kind: Kind::Write,
    handler: create_outline,
};

fn create_outline(client: &TackClient, args: &Value) -> Result<Value, String> {
    if client.acts_as_agent() {
        return changes::create_outline(client, args);
    }
    if !args.get("items").is_some_and(Value::is_array) {
        return Err("missing required argument: items (an array)".into());
    }
    // The body is the whole file: every argument but the two that pick
    // the route.
    let mut file = args.as_object().cloned().unwrap_or_default();
    file.remove("project_id");
    file.remove("dry_run");
    let file = Value::Object(file);
    let suffix = if args.get("dry_run").and_then(Value::as_bool) == Some(true) {
        "?dry_run=true"
    } else {
        ""
    };
    let project_id = opt_str(args, "project_id")
        .map(|_| require_id(args, "project_id"))
        .transpose()?;
    let written = match (project_id, args.get("project")) {
        (Some(_), Some(_)) => Err("create_outline: pass project_id or project, not both".into()),
        (Some(pid), None) => call(client.post(&format!("/projects/{pid}/outline{suffix}"), &file)),
        (None, Some(project)) if project.is_object() => {
            call(client.post(&format!("/projects/outline{suffix}"), &file))
        }
        _ => Err(
            "create_outline: pass project_id to add to a project, or project \
                  {name, project_type?, description?} to create one"
                .into(),
        ),
    };
    written.map(|mut result| {
        if result["project"].is_object() {
            result["project"] = project_summary(&result["project"]);
        }
        result
    })
}

pub static PLAN_FOLDER: Tool = Tool {
    name: "plan_folder",
    spec: Spec {
        description: "Preview what a project folder would change on the board: pass the folder's files, or a revision number to go back to. \
             Returns the plan: changes, kept rows with their reason, conflicts, removals with their cost and warnings. \
             It never applies the plan; a person applies it with `tack folder`.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string", "description": "Project UUID" },
                    "files": { "type": "object", "description": "The folder's files: each path relative to .tack/ and its text (not with revision)" },
                    "revision": { "type": "number", "description": "A revision number to plan going back to (not with files)" },
                    "prefer": { "type": "string", "description": "file or board: whose value wins a conflict" }
                },
                "required": ["project_id"]
            })
        },
    },
    kind: Kind::Read,
    handler: plan_folder,
};

fn plan_folder(client: &TackClient, args: &Value) -> Result<Value, String> {
    let project_id = require_id(args, "project_id")?;
    let mut query: Vec<(&str, String)> = Vec::new();
    if let Some(prefer) = opt_str(args, "prefer") {
        if prefer != "file" && prefer != "board" {
            return Err("prefer must be file or board".into());
        }
        query.push(("prefer", prefer));
    }
    let body = match (args.get("files"), args.get("revision")) {
        (Some(_), Some(_)) => return Err("plan_folder: pass files or revision, not both".into()),
        (Some(files), None) if files.is_object() => json!({ "files": files }).to_string(),
        (None, Some(_)) => {
            query.push(("revision", require_u64(args, "revision")?.to_string()));
            String::new()
        }
        _ => {
            return Err("plan_folder: pass files (an object of path to text) or revision".into());
        }
    };
    let query: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();
    call(client.post_text(
        &format!("/projects/{project_id}/folder/plan"),
        &query,
        "application/json",
        body,
    ))
}
