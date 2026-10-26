use clap::Subcommand;
use serde_json::json;

use tack_cli::client::TackClient;

use tack_cli::vocab;

use super::common::*;

#[derive(Subcommand)]
pub(crate) enum SprintAction {
    /// Create a new sprint
    Create {
        /// Project ID
        #[arg(short = 'p', long)]
        project: String,
        /// Sprint name
        name: String,
        /// Sprint goal
        #[arg(short, long)]
        goal: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Start a sprint (sets status to active)
    Start {
        /// Sprint ID
        id: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Move sprint to review
    Review {
        /// Sprint ID
        id: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Close a sprint
    Close {
        /// Sprint ID
        id: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// List sprints in a project
    List {
        /// Project ID
        #[arg(short = 'p', long)]
        project: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum TemplateAction {
    /// List available templates
    List {
        /// Filter by project type (software, web, mobile, construction, personal, homework, maintenance, legal, research, event, custom)
        #[arg(short = 't', long)]
        project_type: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Show details of a template
    Show {
        /// Template ID
        id: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Create a new project from a template
    CreateFrom {
        /// Template ID
        id: String,
        /// New project name
        name: String,
        /// Optional description
        #[arg(short, long)]
        description: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Save a project's roles, profiles, automation, workflow and vocabulary as a template
    Save {
        /// Project ID
        project: String,
        /// The template's name
        #[arg(long)]
        name: String,
        /// Optional description
        #[arg(short, long)]
        description: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum RoleAction {
    /// List roles defined in a project
    List {
        /// Project ID
        #[arg(short = 'p', long)]
        project: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Create a new role in a project
    Create {
        /// Role name
        name: String,
        /// Project ID
        #[arg(short = 'p', long)]
        project: String,
        /// Hex color (e.g. #4A90D9)
        #[arg(long)]
        color: Option<String>,
        /// Agent profile ID a task with this role runs with
        #[arg(long)]
        profile: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Set or clear the agent profile a role runs with
    SetProfile {
        /// Role ID
        role: String,
        /// Agent profile ID, or "none" to clear
        profile: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Delete a role
    Delete {
        /// Role ID
        id: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Assign a role to an item
    Assign {
        /// Item ID
        item: String,
        /// Role ID
        role: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Remove a role assignment from an item
    Unassign {
        /// Item ID
        item: String,
        /// Role ID
        role: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum CommentAction {
    /// List comments on an item
    List {
        /// Item ID
        item: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Add a comment to an item
    Add {
        /// Item ID
        item: String,
        /// Comment text
        content: String,
        /// Author name
        #[arg(short, long)]
        author: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum FieldAction {
    /// List custom field definitions for a project
    List {
        /// Project ID
        #[arg(short = 'p', long)]
        project: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Create a custom field definition in a project
    Create {
        /// Field name
        name: String,
        /// Project ID
        #[arg(short = 'p', long)]
        project: String,
        /// Field type: text, long_text, number, date, boolean, select, multi_select, url, email
        #[arg(short = 't', long)]
        field_type: String,
        /// Mark the field as required
        #[arg(long)]
        required: bool,
        /// Comma-separated options (for select / multi_select types)
        #[arg(long)]
        options: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Delete a custom field definition
    Delete {
        /// Field ID
        id: String,
    },
    /// List all custom field values set on an item
    Values {
        /// Item ID
        item: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Set a custom field value on an item
    Set {
        /// Item ID
        item: String,
        /// Field ID
        field: String,
        /// Value (parsed as JSON if possible, otherwise treated as a string)
        value: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Remove a custom field value from an item
    Unset {
        /// Item ID
        item: String,
        /// Field ID
        field: String,
    },
}

fn cmd_template_save(
    client: &TackClient,
    project: &str,
    name: String,
    description: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = json!({ "name": name, "description": description });
    let resp = client.post(&format!("/projects/{project}/save-as-template"), &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let id = resp["id"].as_str().unwrap_or("?");
    println!("Saved template: {name} ({})", &id[..8.min(id.len())]);
    println!("  id: {id}");
    Ok(())
}

fn cmd_sprint_create(
    client: &TackClient,
    project: String,
    name: String,
    goal: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = json!({ "name": name, "goal": goal });
    let resp = client.post(&format!("/projects/{project}/sprints"), &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let id = resp["id"].as_str().unwrap_or("?");
    // Translate "sprint" via project vocabulary.
    let v = vocab::fetch(client, &project);
    let sprint_label = vocab::term(&v, "sprint");
    println!(
        "Created {sprint_label}: {} ({})",
        name,
        &id[..8.min(id.len())]
    );
    println!("  id:     {id}");
    println!("  status: planning");
    Ok(())
}

fn cmd_sprint_status(
    client: &TackClient,
    id: String,
    status: &str,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = json!({ "status": status });
    let resp = client.patch(&format!("/sprints/{id}/status"), &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    println!("Sprint {}: → {status}", &id[..8.min(id.len())]);
    Ok(())
}

fn cmd_sprint_list(client: &TackClient, project: String, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get(&format!("/projects/{project}/sprints"))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let empty = vec![];
    let sprints = resp.as_array().unwrap_or(&empty);
    if sprints.is_empty() {
        println!("No sprints found.");
        return Ok(());
    }
    print_table_header(&["ID", "NAME", "STATUS", "START", "END"]);
    for s in sprints {
        print_table_row(&[
            short_id(s["id"].as_str()),
            s["name"].as_str().unwrap_or("?"),
            s["status"].as_str().unwrap_or("?"),
            short_date(s["start_date"].as_str()),
            short_date(s["end_date"].as_str()),
        ]);
    }
    Ok(())
}

pub(crate) fn cmd_template(client: &TackClient, action: TemplateAction) -> anyhow::Result<()> {
    match action {
        TemplateAction::List { project_type, json } => {
            cmd_template_list(client, project_type, json)
        }
        TemplateAction::Show { id, json } => cmd_template_show(client, id, json),
        TemplateAction::CreateFrom {
            id,
            name,
            description,
            json,
        } => cmd_template_create_from(client, id, name, description, json),
        TemplateAction::Save {
            project,
            name,
            description,
            json,
        } => cmd_template_save(client, &project, name, description, json),
    }
}

fn cmd_template_list(
    client: &TackClient,
    project_type: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let path = match &project_type {
        Some(pt) => format!("/templates?project_type={}", urlenc(pt)),
        None => "/templates".to_string(),
    };
    let resp = client.get(&path)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let empty = vec![];
    let templates = resp.as_array().unwrap_or(&empty);
    if templates.is_empty() {
        println!("No templates found.");
        return Ok(());
    }
    print_table_header(&["ID", "NAME", "TYPE", "BUILTIN"]);
    for t in templates {
        let builtin = if t["is_builtin"].as_bool().unwrap_or(false) {
            "yes"
        } else {
            "no"
        };
        print_table_row(&[
            short_id(t["id"].as_str()),
            t["name"].as_str().unwrap_or("?"),
            t["project_type"].as_str().unwrap_or("?"),
            builtin,
        ]);
    }
    Ok(())
}

fn cmd_template_show(client: &TackClient, id: String, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get(&format!("/templates/{id}"))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let name = resp["name"].as_str().unwrap_or("?");
    let desc = resp["description"].as_str().unwrap_or("(no description)");
    let ptype = resp["project_type"].as_str().unwrap_or("?");
    let builtin = if resp["is_builtin"].as_bool().unwrap_or(false) {
        " [built-in]"
    } else {
        ""
    };
    println!("{name}{builtin}");
    println!("  type:        {ptype}");
    println!("  description: {desc}");

    // Workflow statuses
    if let Some(statuses) = resp["workflow"]["statuses"].as_array() {
        let names: Vec<&str> = statuses.iter().filter_map(|s| s["name"].as_str()).collect();
        println!("  statuses:    {}", names.join(" → "));
    }

    // Vocabulary overrides
    if let Some(vocab) = resp["vocabulary"].as_object() {
        let overrides: Vec<String> = vocab
            .iter()
            .map(|(k, v)| format!("{k}={}", v.as_str().unwrap_or("?")))
            .collect();
        if !overrides.is_empty() {
            println!("  vocabulary:  {}", overrides.join(", "));
        }
    }

    // Custom fields
    if let Some(fields) = resp["custom_fields"].as_array()
        && !fields.is_empty()
    {
        let field_names: Vec<&str> = fields.iter().filter_map(|f| f["name"].as_str()).collect();
        println!("  fields:      {}", field_names.join(", "));
    }

    // Boards
    if let Some(boards) = resp["default_boards"].as_array()
        && !boards.is_empty()
    {
        let board_names: Vec<&str> = boards.iter().filter_map(|b| b["name"].as_str()).collect();
        println!("  boards:      {}", board_names.join(", "));
    }

    println!("  id:          {id}");
    Ok(())
}

pub(crate) fn cmd_template_create_from(
    client: &TackClient,
    template_id: String,
    name: String,
    description: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = json!({ "name": name, "description": description });
    let resp = client.post(&format!("/projects/from-template/{template_id}"), &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let id = resp["id"].as_str().unwrap_or("?");
    let ptype = resp["project_type"].as_str().unwrap_or("?");
    println!("Created project: {} ({})", name, &id[..8.min(id.len())]);
    println!("  type:     {ptype}");
    println!("  template: {}", &template_id[..8.min(template_id.len())]);
    println!("  id:       {id}");
    Ok(())
}

// ─── Role commands ────────────────────────────────────────────────────────────

fn cmd_role_list(client: &TackClient, project: String, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get(&format!("/projects/{project}/roles"))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let empty = vec![];
    let roles = resp.as_array().unwrap_or(&empty);
    if roles.is_empty() {
        println!("No roles defined.");
        return Ok(());
    }
    print_table_header(&["ID", "NAME", "COLOR"]);
    for r in roles {
        print_table_row(&[
            short_id(r["id"].as_str()),
            r["name"].as_str().unwrap_or("?"),
            r["color"].as_str().unwrap_or("-"),
        ]);
    }
    Ok(())
}

pub(crate) fn cmd_role(client: &TackClient, action: RoleAction) -> anyhow::Result<()> {
    match action {
        RoleAction::List { project, json } => cmd_role_list(client, project, json),
        RoleAction::Create {
            name,
            project,
            color,
            profile,
            json,
        } => cmd_role_create(client, name, project, color, profile, json),
        RoleAction::SetProfile {
            role,
            profile,
            json,
        } => cmd_role_set_profile(client, role, profile, json),
        RoleAction::Delete { id, json } => cmd_role_delete(client, id, json),
        RoleAction::Assign { item, role, json } => cmd_role_assign(client, item, role, json),
        RoleAction::Unassign { item, role, json } => cmd_role_unassign(client, item, role, json),
    }
}

fn cmd_role_create(
    client: &TackClient,
    name: String,
    project: String,
    color: Option<String>,
    profile: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = json!({ "name": name, "color": color });
    let resp = client.post(&format!("/projects/{project}/roles"), &body)?;
    let id = resp["id"].as_str().unwrap_or("?").to_owned();
    // The create body carries no profile; a second call sets it.
    let resp = match profile {
        Some(profile) => client.patch(
            &format!("/roles/{id}"),
            &json!({ "agent_profile_id": profile }),
        )?,
        None => resp,
    };
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    println!("Created role: {} ({})", name, &id[..8.min(id.len())]);
    println!("  id:    {id}");
    Ok(())
}

fn cmd_role_set_profile(
    client: &TackClient,
    role: String,
    profile: String,
    as_json: bool,
) -> anyhow::Result<()> {
    let profile = (profile != "none").then_some(profile);
    let resp = client.patch(
        &format!("/roles/{role}"),
        &json!({ "agent_profile_id": profile }),
    )?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    match resp["agent_profile_id"].as_str() {
        Some(id) => println!(
            "Role {} runs with agent profile {id}",
            &role[..8.min(role.len())]
        ),
        None => println!("Role {} has no agent profile", &role[..8.min(role.len())]),
    }
    Ok(())
}

fn cmd_role_delete(client: &TackClient, id: String, as_json: bool) -> anyhow::Result<()> {
    let resp = client.delete_json(&format!("/roles/{id}"))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    println!("Deleted role: {}", &id[..8.min(id.len())]);
    Ok(())
}

fn cmd_role_assign(
    client: &TackClient,
    item: String,
    role: String,
    as_json: bool,
) -> anyhow::Result<()> {
    let resp = client.put_empty(&format!("/items/{item}/roles/{role}"))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    println!(
        "Assigned role {} to item {}",
        &role[..8.min(role.len())],
        &item[..8.min(item.len())]
    );
    Ok(())
}

fn cmd_role_unassign(
    client: &TackClient,
    item: String,
    role: String,
    as_json: bool,
) -> anyhow::Result<()> {
    let resp = client.delete_json(&format!("/items/{item}/roles/{role}"))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    println!(
        "Removed role {} from item {}",
        &role[..8.min(role.len())],
        &item[..8.min(item.len())]
    );
    Ok(())
}

// ─── Comment commands ─────────────────────────────────────────────────────────

fn cmd_comment_list(client: &TackClient, item: String, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get(&format!("/items/{item}/comments"))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let empty = vec![];
    let comments = resp.as_array().unwrap_or(&empty);
    if comments.is_empty() {
        println!("No comments.");
        return Ok(());
    }
    for c in comments {
        let author = c["author"].as_str().unwrap_or("anonymous");
        let created = short_date(c["created_at"].as_str());
        let content = c["content"].as_str().unwrap_or("?");
        println!("[{created}] {author}: {content}");
    }
    Ok(())
}

fn cmd_comment_add(
    client: &TackClient,
    item: String,
    content: String,
    author: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = json!({ "content": content, "author": author });
    let resp = client.post(&format!("/items/{item}/comments"), &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let id = resp["id"].as_str().unwrap_or("?");
    println!("Comment added ({})", &id[..8.min(id.len())]);
    Ok(())
}

// ─── Custom field commands ────────────────────────────────────────────────────

fn cmd_field_list(client: &TackClient, project: String, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get(&format!("/projects/{project}/custom-fields"))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let empty = vec![];
    let fields = resp.as_array().unwrap_or(&empty);
    if fields.is_empty() {
        println!("No custom fields defined.");
        return Ok(());
    }
    print_table_header(&["ID", "NAME", "TYPE", "REQUIRED"]);
    for f in fields {
        let required = if f["required"].as_bool().unwrap_or(false) {
            "yes"
        } else {
            "no"
        };
        print_table_row(&[
            short_id(f["id"].as_str()),
            f["name"].as_str().unwrap_or("?"),
            f["field_type"].as_str().unwrap_or("?"),
            required,
        ]);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_field_create(
    client: &TackClient,
    name: String,
    project: String,
    field_type: String,
    required: bool,
    options: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let opts: Option<Vec<&str>> = options
        .as_deref()
        .map(|s| s.split(',').map(str::trim).collect());
    let body = json!({
        "name": name,
        "field_type": field_type,
        "required": required,
        "options": opts,
    });
    let resp = client.post(&format!("/projects/{project}/custom-fields"), &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let id = resp["id"].as_str().unwrap_or("?");
    println!(
        "Created field: {} ({}) [{}]",
        name,
        field_type,
        &id[..8.min(id.len())]
    );
    println!("  id: {id}");
    Ok(())
}

fn cmd_field_delete(client: &TackClient, id: String) -> anyhow::Result<()> {
    client.delete(&format!("/custom-fields/{id}"))?;
    println!("Deleted field: {}", &id[..8.min(id.len())]);
    Ok(())
}

fn cmd_field_values(client: &TackClient, item: String, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get(&format!("/items/{item}/custom-fields"))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let empty = vec![];
    let values = resp.as_array().unwrap_or(&empty);
    if values.is_empty() {
        println!("No custom field values.");
        return Ok(());
    }
    for v in values {
        let field_id = short_id(v["field_id"].as_str());
        let value = &v["value"];
        println!("  {field_id}  {value}");
    }
    Ok(())
}

fn cmd_field_set(
    client: &TackClient,
    item: String,
    field: String,
    value: String,
    as_json: bool,
) -> anyhow::Result<()> {
    // Parse as JSON if possible; fall back to a JSON string.
    let json_value: serde_json::Value =
        serde_json::from_str(&value).unwrap_or_else(|_| serde_json::Value::String(value.clone()));
    let resp = client.put_json(&format!("/items/{item}/custom-fields/{field}"), &json_value)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    println!(
        "Set field {} on item {}",
        &field[..8.min(field.len())],
        &item[..8.min(item.len())]
    );
    Ok(())
}

fn cmd_field_unset(client: &TackClient, item: String, field: String) -> anyhow::Result<()> {
    client.delete(&format!("/items/{item}/custom-fields/{field}"))?;
    println!(
        "Unset field {} on item {}",
        &field[..8.min(field.len())],
        &item[..8.min(item.len())]
    );
    Ok(())
}

pub(crate) fn run_comment(client: &TackClient, action: CommentAction) -> anyhow::Result<()> {
    match action {
        CommentAction::List { item, json } => cmd_comment_list(client, item, json),
        CommentAction::Add {
            item,
            content,
            author,
            json,
        } => cmd_comment_add(client, item, content, author, json),
    }
}

pub(crate) fn run_sprint(client: &TackClient, action: SprintAction) -> anyhow::Result<()> {
    match action {
        SprintAction::Create {
            project,
            name,
            goal,
            json,
        } => cmd_sprint_create(client, project, name, goal, json),
        SprintAction::Start { id, json } => cmd_sprint_status(client, id, "active", json),
        SprintAction::Review { id, json } => cmd_sprint_status(client, id, "review", json),
        SprintAction::Close { id, json } => cmd_sprint_status(client, id, "closed", json),
        SprintAction::List { project, json } => cmd_sprint_list(client, project, json),
    }
}

pub(crate) fn run_field(client: &TackClient, action: FieldAction) -> anyhow::Result<()> {
    match action {
        FieldAction::List { project, json } => cmd_field_list(client, project, json),
        FieldAction::Create {
            name,
            project,
            field_type,
            required,
            options,
            json,
        } => cmd_field_create(client, name, project, field_type, required, options, json),
        FieldAction::Delete { id } => cmd_field_delete(client, id),
        FieldAction::Values { item, json } => cmd_field_values(client, item, json),
        FieldAction::Set {
            item,
            field,
            value,
            json,
        } => cmd_field_set(client, item, field, value, json),
        FieldAction::Unset { item, field } => cmd_field_unset(client, item, field),
    }
}
