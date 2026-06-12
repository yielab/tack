use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use serde_json::json;

use flexpm_cli::client::FlexpmClient;
use flexpm_cli::config::{self, Config};
use flexpm_cli::vocab;

// ─── CLI structure ────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "flexpm",
    version,
    about = "FlexPM — Flexible Project Management CLI"
)]
struct Cli {
    /// FlexPM API base URL
    #[arg(long, env = "FLEXPM_API_URL")]
    api_url: Option<String>,

    /// Bearer token for authentication
    #[arg(long, env = "FLEXPM_API_TOKEN")]
    token: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new project
    Init {
        /// Project name
        name: String,
        /// Project type (software, web, mobile, construction, personal, homework, maintenance, custom)
        #[arg(short = 't', long, default_value = "software")]
        r#type: String,
        /// Optional description
        #[arg(short, long)]
        description: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// List projects
    Projects {
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Add a new item to a project
    Add {
        /// Item title
        title: String,
        /// Project ID
        #[arg(short = 'p', long)]
        project: String,
        /// Item type (task, epic, feature, bug, subtask, requirement)
        #[arg(short = 't', long, default_value = "task")]
        item_type: String,
        /// Priority (critical, high, medium, low)
        #[arg(long, default_value = "medium")]
        priority: String,
        /// Parent item ID
        #[arg(long)]
        parent: Option<String>,
        /// Assignee name
        #[arg(short, long)]
        assignee: Option<String>,
        /// Sprint ID
        #[arg(short, long)]
        sprint: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// List items in a project
    List {
        /// Project ID
        #[arg(short = 'p', long)]
        project: String,
        /// Filter by status
        #[arg(short, long)]
        status: Option<String>,
        /// Filter by item type
        #[arg(long)]
        item_type: Option<String>,
        /// Filter by assignee
        #[arg(short, long)]
        assignee: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Move an item to a new status
    Move {
        /// Item ID
        id: String,
        /// Target status
        status: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Show board view for a project
    Board {
        /// Project ID
        #[arg(short = 'p', long)]
        project: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Search items
    Search {
        /// Search query
        query: String,
        /// Project ID (optional; searches globally if omitted)
        #[arg(short = 'p', long)]
        project: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Sprint management
    Sprint {
        #[command(subcommand)]
        action: SprintAction,
    },

    /// Write ~/.flexpmrc with connection settings
    Config {
        /// API base URL to save
        #[arg(long)]
        url: Option<String>,
        /// Bearer token to save (omit to clear)
        #[arg(long)]
        token: Option<String>,
        /// Show current config without changing it
        #[arg(long)]
        show: bool,
    },

    /// Print shell completion script to stdout
    Completions {
        /// Target shell (bash, zsh, fish, powershell, elvish)
        shell: Shell,
    },

    /// Download a backup of the FlexPM database
    Backup {
        /// Where to save the backup (default: flexpm-backup.db)
        path: Option<std::path::PathBuf>,
    },

    /// Stage a backup file for restore on next server restart
    Restore {
        /// Path to the backup file produced by `flexpm backup`
        path: std::path::PathBuf,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Manage project templates
    Template {
        #[command(subcommand)]
        action: TemplateAction,
    },

    /// Manage project roles (specialties / disciplines)
    Role {
        #[command(subcommand)]
        action: RoleAction,
    },

    /// Manage item comments
    Comment {
        #[command(subcommand)]
        action: CommentAction,
    },

    /// Manage custom field definitions and item values
    Field {
        #[command(subcommand)]
        action: FieldAction,
    },
}

#[derive(Subcommand)]
enum SprintAction {
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
enum TemplateAction {
    /// List available templates
    List {
        /// Filter by project type (software, web, mobile, construction, personal, homework, maintenance, custom)
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
}

#[derive(Subcommand)]
enum RoleAction {
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
enum CommentAction {
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
enum FieldAction {
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

// ─── Entry point ─────────────────────────────────────────────────────────────

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Config and Completions don't need a live client.
    match &cli.command {
        Commands::Config { url, token, show } => {
            return cmd_config(
                url.as_deref(),
                token.as_deref(),
                *show,
                &cli.api_url,
                &cli.token,
            );
        }
        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            clap_complete::generate(*shell, &mut cmd, "flexpm", &mut std::io::stdout());
            return Ok(());
        }
        _ => {}
    }

    let config = Config::load(cli.api_url, cli.token);
    let client = FlexpmClient::new(&config)?;

    match cli.command {
        Commands::Init {
            name,
            r#type,
            description,
            json,
        } => cmd_init(&client, name, r#type, description, json),

        Commands::Projects { json } => cmd_projects(&client, json),

        Commands::Add {
            title,
            project,
            item_type,
            priority,
            parent,
            assignee,
            sprint,
            json,
        } => cmd_add(
            &client, title, project, item_type, priority, parent, assignee, sprint, json,
        ),

        Commands::List {
            project,
            status,
            item_type,
            assignee,
            json,
        } => cmd_list(&client, project, status, item_type, assignee, json),

        Commands::Move { id, status, json } => cmd_move(&client, id, status, json),

        Commands::Board { project, json } => cmd_board(&client, project, json),

        Commands::Search {
            query,
            project,
            json,
        } => cmd_search(&client, query, project, json),

        Commands::Backup { path } => cmd_backup(&client, path),

        Commands::Restore { path, json } => cmd_restore(&client, path, json),

        Commands::Template { action } => match action {
            TemplateAction::List { project_type, json } => {
                cmd_template_list(&client, project_type, json)
            }
            TemplateAction::Show { id, json } => cmd_template_show(&client, id, json),
            TemplateAction::CreateFrom {
                id,
                name,
                description,
                json,
            } => cmd_template_create_from(&client, id, name, description, json),
        },

        Commands::Role { action } => match action {
            RoleAction::List { project, json } => cmd_role_list(&client, project, json),
            RoleAction::Create {
                name,
                project,
                color,
                json,
            } => cmd_role_create(&client, name, project, color, json),
            RoleAction::Delete { id, json } => cmd_role_delete(&client, id, json),
            RoleAction::Assign { item, role, json } => cmd_role_assign(&client, item, role, json),
            RoleAction::Unassign { item, role, json } => {
                cmd_role_unassign(&client, item, role, json)
            }
        },

        Commands::Comment { action } => match action {
            CommentAction::List { item, json } => cmd_comment_list(&client, item, json),
            CommentAction::Add {
                item,
                content,
                author,
                json,
            } => cmd_comment_add(&client, item, content, author, json),
        },

        Commands::Sprint { action } => match action {
            SprintAction::Create {
                project,
                name,
                goal,
                json,
            } => cmd_sprint_create(&client, project, name, goal, json),
            SprintAction::Start { id, json } => cmd_sprint_status(&client, id, "active", json),
            SprintAction::Review { id, json } => cmd_sprint_status(&client, id, "review", json),
            SprintAction::Close { id, json } => cmd_sprint_status(&client, id, "closed", json),
            SprintAction::List { project, json } => cmd_sprint_list(&client, project, json),
        },

        Commands::Field { action } => match action {
            FieldAction::List { project, json } => cmd_field_list(&client, project, json),
            FieldAction::Create {
                name,
                project,
                field_type,
                required,
                options,
                json,
            } => cmd_field_create(&client, name, project, field_type, required, options, json),
            FieldAction::Delete { id } => cmd_field_delete(&client, id),
            FieldAction::Values { item, json } => cmd_field_values(&client, item, json),
            FieldAction::Set {
                item,
                field,
                value,
                json,
            } => cmd_field_set(&client, item, field, value, json),
            FieldAction::Unset { item, field } => cmd_field_unset(&client, item, field),
        },

        // Already handled above; unreachable but required for exhaustiveness.
        Commands::Config { .. } | Commands::Completions { .. } => unreachable!(),
    }
}

// ─── Command implementations ─────────────────────────────────────────────────

fn cmd_config(
    url: Option<&str>,
    token: Option<&str>,
    show: bool,
    cli_url: &Option<String>,
    cli_token: &Option<String>,
) -> anyhow::Result<()> {
    if show || (url.is_none() && token.is_none()) {
        let cfg = Config::load(cli_url.clone(), cli_token.clone());
        println!("base_url: {}", cfg.base_url);
        match &cfg.token {
            Some(t) => println!("token:    {}", "*".repeat(t.len().min(8))),
            None => println!("token:    (none)"),
        }
        return Ok(());
    }
    let base_url = url.unwrap_or("http://127.0.0.1:3210");
    config::save(base_url, token)?;
    println!("Saved to ~/.flexpmrc");
    println!("  base_url: {base_url}");
    if token.is_some() {
        println!("  token:    (set)");
    }
    Ok(())
}

fn cmd_init(
    client: &FlexpmClient,
    name: String,
    project_type: String,
    description: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = json!({
        "name": name,
        "project_type": project_type,
        "description": description,
    });
    let resp = client.post("/projects", &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let id = resp["id"].as_str().unwrap_or("?");
    println!("Created project: {} ({})", name, &id[..8.min(id.len())]);
    println!("  type: {project_type}");
    println!("  id:   {id}");
    Ok(())
}

fn cmd_projects(client: &FlexpmClient, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get("/projects")?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let empty = vec![];
    let projects = resp.as_array().unwrap_or(&empty);
    if projects.is_empty() {
        println!("No projects found.");
        return Ok(());
    }
    print_table_header(&["ID", "NAME", "TYPE", "UPDATED"]);
    for p in projects {
        print_table_row(&[
            short_id(p["id"].as_str()),
            p["name"].as_str().unwrap_or("?"),
            p["project_type"].as_str().unwrap_or("?"),
            short_date(p["updated_at"].as_str()),
        ]);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_add(
    client: &FlexpmClient,
    title: String,
    project: String,
    item_type: String,
    priority: String,
    parent: Option<String>,
    assignee: Option<String>,
    sprint: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let mut body = json!({
        "title": title,
        "item_type": item_type,
        "priority": priority,
    });
    if let Some(p) = parent {
        body["parent_id"] = json!(p);
    }
    if let Some(a) = assignee {
        body["assignee"] = json!(a);
    }
    if let Some(s) = sprint {
        body["sprint_id"] = json!(s);
    }
    let resp = client.post(&format!("/projects/{project}/items"), &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let id = resp["id"].as_str().unwrap_or("?");
    // Translate item_type through the project's vocabulary.
    let v = vocab::fetch(client, &project);
    let type_label = vocab::term(&v, &item_type);
    println!(
        "Created {}: {} ({})",
        type_label,
        title,
        &id[..8.min(id.len())]
    );
    println!("  priority: {priority}");
    println!("  id:       {id}");
    Ok(())
}

fn cmd_list(
    client: &FlexpmClient,
    project: String,
    status: Option<String>,
    item_type: Option<String>,
    assignee: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let mut params = vec![];
    if let Some(s) = &status {
        params.push(format!("status={}", urlenc(s)));
    }
    if let Some(t) = &item_type {
        params.push(format!("item_type={}", urlenc(t)));
    }
    if let Some(a) = &assignee {
        params.push(format!("assignee={}", urlenc(a)));
    }
    let qs = if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    };
    let resp = client.get(&format!("/projects/{project}/items{qs}"))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let empty = vec![];
    let items = resp.as_array().unwrap_or(&empty);
    if items.is_empty() {
        println!("No items found.");
        return Ok(());
    }
    let v = vocab::fetch(client, &project);
    print_table_header(&["ID", "TITLE", "TYPE", "STATUS", "PRIORITY", "ASSIGNEE"]);
    for item in items {
        let raw_type = item["item_type"].as_str().unwrap_or("task");
        print_table_row(&[
            short_id(item["id"].as_str()),
            item["title"].as_str().unwrap_or("?"),
            vocab::term(&v, raw_type),
            item["status"].as_str().unwrap_or("?"),
            item["priority"].as_str().unwrap_or("?"),
            item["assignee"].as_str().unwrap_or("-"),
        ]);
    }
    Ok(())
}

fn cmd_move(
    client: &FlexpmClient,
    id: String,
    status: String,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = json!({ "status": status });
    let resp = client.patch(&format!("/items/{id}"), &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    println!("Moved {}: → {status}", &id[..8.min(id.len())]);
    Ok(())
}

fn cmd_board(client: &FlexpmClient, project: String, as_json: bool) -> anyhow::Result<()> {
    let boards = client.get(&format!("/projects/{project}/boards"))?;
    let empty = vec![];
    let boards = boards.as_array().unwrap_or(&empty);

    let board_id = boards
        .iter()
        .find(|b| b["is_default"].as_bool().unwrap_or(false))
        .or_else(|| boards.first())
        .and_then(|b| b["id"].as_str())
        .ok_or_else(|| anyhow::anyhow!("No boards found for project {project}"))?;

    let view = client.get(&format!("/boards/{board_id}/view"))?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&view)?);
        return Ok(());
    }

    let v = vocab::fetch(client, &project);
    let board_name = view["name"].as_str().unwrap_or("Board");
    println!("── {board_name} ────────────────────────────");

    let cols = view["columns"]
        .as_array()
        .map(|a| a.as_slice())
        .unwrap_or(&[]);
    for col in cols {
        let name = col["name"].as_str().unwrap_or("?");
        let items = col["items"].as_array().map(|a| a.as_slice()).unwrap_or(&[]);
        let wip = col["wip_limit"].as_u64();
        let exceeded = col["wip_exceeded"].as_bool().unwrap_or(false);

        let wip_label = match wip {
            Some(limit) if exceeded => format!(" ({}/{limit} ⚠)", items.len()),
            Some(limit) => format!(" ({}/{})", items.len(), limit),
            None => format!(" ({})", items.len()),
        };

        println!("\n{name}{wip_label}");
        println!("{}", "─".repeat(name.len() + wip_label.len()));

        if items.is_empty() {
            println!("  (empty)");
        }
        for item in items {
            let title = item["title"].as_str().unwrap_or("?");
            let raw_type = item["item_type"].as_str().unwrap_or("task");
            let type_label = vocab::term(&v, raw_type);
            let assignee = item["assignee"].as_str();
            let suffix = match assignee {
                Some(a) => format!(" [{a}]"),
                None => String::new(),
            };
            println!("  · {title} ({type_label}){suffix}");
        }
    }
    Ok(())
}

fn cmd_search(
    client: &FlexpmClient,
    query: String,
    project: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let path = match &project {
        Some(pid) => format!("/projects/{pid}/search?q={}", urlenc(&query)),
        None => format!("/search?q={}", urlenc(&query)),
    };
    let resp = client.get(&path)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let empty = vec![];
    let items = resp.as_array().unwrap_or(&empty);
    if items.is_empty() {
        println!("No results for '{query}'.");
        return Ok(());
    }
    println!("{} result(s) for '{query}':", items.len());
    print_table_header(&["ID", "TITLE", "STATUS", "PROJECT"]);
    for item in items {
        print_table_row(&[
            short_id(item["id"].as_str()),
            item["title"].as_str().unwrap_or("?"),
            item["status"].as_str().unwrap_or("?"),
            short_id(item["project_id"].as_str()),
        ]);
    }
    Ok(())
}

fn cmd_sprint_create(
    client: &FlexpmClient,
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
    client: &FlexpmClient,
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

fn cmd_sprint_list(client: &FlexpmClient, project: String, as_json: bool) -> anyhow::Result<()> {
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

fn cmd_template_list(
    client: &FlexpmClient,
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

fn cmd_template_show(client: &FlexpmClient, id: String, as_json: bool) -> anyhow::Result<()> {
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

fn cmd_template_create_from(
    client: &FlexpmClient,
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

fn cmd_role_list(client: &FlexpmClient, project: String, as_json: bool) -> anyhow::Result<()> {
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

fn cmd_role_create(
    client: &FlexpmClient,
    name: String,
    project: String,
    color: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = json!({ "name": name, "color": color });
    let resp = client.post(&format!("/projects/{project}/roles"), &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let id = resp["id"].as_str().unwrap_or("?");
    println!("Created role: {} ({})", name, &id[..8.min(id.len())]);
    println!("  id:    {id}");
    Ok(())
}

fn cmd_role_delete(client: &FlexpmClient, id: String, as_json: bool) -> anyhow::Result<()> {
    let resp = client.delete_json(&format!("/roles/{id}"))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    println!("Deleted role: {}", &id[..8.min(id.len())]);
    Ok(())
}

fn cmd_role_assign(
    client: &FlexpmClient,
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
    client: &FlexpmClient,
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

fn cmd_comment_list(client: &FlexpmClient, item: String, as_json: bool) -> anyhow::Result<()> {
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
    client: &FlexpmClient,
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

fn cmd_field_list(client: &FlexpmClient, project: String, as_json: bool) -> anyhow::Result<()> {
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
    client: &FlexpmClient,
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

fn cmd_field_delete(client: &FlexpmClient, id: String) -> anyhow::Result<()> {
    client.delete(&format!("/custom-fields/{id}"))?;
    println!("Deleted field: {}", &id[..8.min(id.len())]);
    Ok(())
}

fn cmd_field_values(client: &FlexpmClient, item: String, as_json: bool) -> anyhow::Result<()> {
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
    client: &FlexpmClient,
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

fn cmd_field_unset(client: &FlexpmClient, item: String, field: String) -> anyhow::Result<()> {
    client.delete(&format!("/items/{item}/custom-fields/{field}"))?;
    println!(
        "Unset field {} on item {}",
        &field[..8.min(field.len())],
        &item[..8.min(item.len())]
    );
    Ok(())
}

// ─── Formatting helpers ───────────────────────────────────────────────────────

fn print_table_header(cols: &[&str]) {
    print_table_row(cols);
    let divider: Vec<String> = cols.iter().map(|c| "─".repeat(col_width(c))).collect();
    println!("{}", divider.join("  "));
}

fn print_table_row(cols: &[&str]) {
    let widths = [8, 32, 12, 16, 10, 12];
    let parts: Vec<String> = cols
        .iter()
        .enumerate()
        .map(|(i, &val)| {
            let w = widths.get(i).copied().unwrap_or(12);
            trunc_pad(val, w)
        })
        .collect();
    println!("{}", parts.join("  "));
}

fn col_width(header: &str) -> usize {
    match header {
        "ID" => 8,
        "NAME" | "TITLE" => 32,
        "TYPE" | "ITEM TYPE" => 12,
        "STATUS" => 16,
        "PRIORITY" => 10,
        "ASSIGNEE" => 12,
        _ => 12,
    }
}

fn trunc_pad(s: &str, width: usize) -> String {
    if s.len() > width {
        format!("{}…", &s[..width - 1])
    } else {
        format!("{:<width$}", s)
    }
}

fn short_id(id: Option<&str>) -> &str {
    id.map(|s| &s[..8.min(s.len())]).unwrap_or("?")
}

fn short_date(dt: Option<&str>) -> &str {
    dt.map(|s| &s[..10.min(s.len())]).unwrap_or("-")
}

fn cmd_backup(client: &FlexpmClient, path: Option<std::path::PathBuf>) -> anyhow::Result<()> {
    let bytes = client.get_bytes("/backup")?;
    let out = path.unwrap_or_else(|| std::path::PathBuf::from("flexpm-backup.db"));
    std::fs::write(&out, &bytes)
        .map_err(|e| anyhow::anyhow!("Cannot write {}: {e}", out.display()))?;
    println!("Backup saved: {} ({} bytes)", out.display(), bytes.len());
    Ok(())
}

fn cmd_restore(
    client: &FlexpmClient,
    path: std::path::PathBuf,
    as_json: bool,
) -> anyhow::Result<()> {
    let data =
        std::fs::read(&path).map_err(|e| anyhow::anyhow!("Cannot read {}: {e}", path.display()))?;
    let resp = client.post_bytes("/restore", data)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let msg = resp["message"].as_str().unwrap_or("Restore staged.");
    println!("{msg}");
    Ok(())
}

/// Minimal percent-encoding for query parameter values (spaces → %20, etc.)
fn urlenc(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => vec![c],
            ' ' => vec!['%', '2', '0'],
            _ => format!("%{:02X}", c as u32).chars().collect(),
        })
        .collect()
}
