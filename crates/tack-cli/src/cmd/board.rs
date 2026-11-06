use clap::Subcommand;
use serde_json::json;
use std::path::PathBuf;

use tack_cli::client::TackClient;
use tack_cli::config::{self, Config};
use tack_cli::git;
use tack_cli::vocab;

use super::common::*;
use super::entities::cmd_template_create_from;

#[derive(Subcommand)]
pub(crate) enum ProjectCommands {
    /// Show how far Tack may merge this project's changes on its own
    Autonomy {
        /// Project ID
        project: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
}

pub(crate) fn cmd_config(
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
    println!("Saved to ~/.tackrc");
    println!("  base_url: {base_url}");
    if token.is_some() {
        println!("  token:    (set)");
    }
    Ok(())
}

pub(crate) fn cmd_init(
    client: &TackClient,
    name: String,
    project_type: String,
    description: Option<String>,
    template: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    if let Some(template) = template {
        let id = resolve_template(client, &template)?;
        return cmd_template_create_from(client, id, name, description, as_json);
    }
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

/// A template's id from what the person typed: a UUID as it is, else a name
/// matched against `GET /templates`, ignoring case and reading `-` as a space.
fn resolve_template(client: &TackClient, wanted: &str) -> anyhow::Result<String> {
    let is_uuid = wanted.len() == 36
        && wanted.chars().enumerate().all(|(i, c)| {
            matches!(i, 8 | 13 | 18 | 23) == (c == '-') && (c == '-' || c.is_ascii_hexdigit())
        });
    if is_uuid {
        return Ok(wanted.to_string());
    }
    let key = wanted.replace('-', " ").to_lowercase();
    let templates = client.get("/templates")?;
    let templates = templates.as_array().cloned().unwrap_or_default();
    let named = |t: &serde_json::Value| t["name"].as_str().map(str::to_owned);
    for t in &templates {
        if let (Some(name), Some(id)) = (named(t), t["id"].as_str())
            && name.replace('-', " ").to_lowercase() == key
        {
            return Ok(id.to_string());
        }
    }
    let names: Vec<String> = templates.iter().filter_map(named).collect();
    anyhow::bail!(
        "no template named \"{wanted}\". Templates: {}",
        names.join(", ")
    )
}

pub(crate) fn cmd_export(
    client: &TackClient,
    project: &str,
    format: Option<String>,
    bundle: bool,
    output: Option<PathBuf>,
) -> anyhow::Result<()> {
    let format = format.unwrap_or_else(|| if bundle { "yaml" } else { "json" }.to_string());
    let mut path = format!("/projects/{project}/export?format={}", urlenc(&format));
    if bundle {
        path.push_str("&scope=bundle");
    }
    let bytes = client.get_bytes(&path)?;
    match output {
        Some(out) => {
            std::fs::write(&out, &bytes)
                .map_err(|e| anyhow::anyhow!("Cannot write {}: {e}", out.display()))?;
            println!("Exported to {} ({} bytes)", out.display(), bytes.len());
        }
        None => {
            use std::io::Write;
            std::io::stdout().write_all(&bytes)?;
        }
    }
    Ok(())
}

/// Where `tack import` puts the items: an existing project, or a new one.
pub(crate) struct ImportTarget {
    project: Option<String>,
    name: Option<String>,
    project_type: Option<String>,
}

fn cmd_import(
    client: &TackClient,
    file: &std::path::Path,
    target: ImportTarget,
    dry_run: bool,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = std::fs::read_to_string(file)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", file.display()))?;
    let extension = file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let (content_type, is_csv) = match extension.as_str() {
        "csv" => ("text/csv", true),
        "yaml" | "yml" => ("application/x-yaml", false),
        _ => ("application/json", false),
    };
    let route = if is_csv { "import-csv" } else { "outline" };
    let default_name = file.file_stem().map(|s| s.to_string_lossy().into_owned());

    let mut query: Vec<(&str, &str)> = Vec::new();
    if dry_run {
        query.push(("dry_run", "true"));
    }
    let path = match &target.project {
        Some(project) => format!("/projects/{project}/{route}"),
        None => {
            let name = target.name.as_deref().or(if is_csv {
                default_name.as_deref()
            } else {
                None
            });
            if let Some(name) = name {
                query.push(("name", name));
            }
            if let Some(project_type) = target.project_type.as_deref() {
                query.push(("project_type", project_type));
            }
            format!("/projects/{route}")
        }
    };
    let resp = client.post_text(&path, &query, content_type, body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }

    if let Some(id) = resp["project"]["id"].as_str() {
        let name = resp["project"]["name"].as_str().unwrap_or("?");
        println!("Created project: {name} ({})", &id[..8.min(id.len())]);
        println!("  id:   {id}");
    }
    for section in resp["sections"].as_array().into_iter().flatten() {
        let count = |key: &str| section[key].as_u64().unwrap_or(0);
        println!(
            "{}: {} new, {} updated, {} not mentioned",
            section_title(section["section"].as_str().unwrap_or("?")),
            count("created"),
            count("updated"),
            count("not_mentioned")
        );
        for warning in section["warnings"].as_array().into_iter().flatten() {
            println!("  warning: {}", warning.as_str().unwrap_or("?"));
        }
    }
    let created = resp["created"].as_u64().unwrap_or(0);
    let dependencies = resp["dependencies"].as_u64().unwrap_or(0);
    let verb = if dry_run { "Would create" } else { "Created" };
    let items_word = if created == 1 { "item" } else { "items" };
    let deps_word = if dependencies == 1 {
        "dependency"
    } else {
        "dependencies"
    };
    println!("{verb} {created} {items_word} and {dependencies} {deps_word}");
    for item in resp["items"].as_array().into_iter().flatten() {
        println!(
            "  - {} [{}]",
            item["title"].as_str().unwrap_or("?"),
            item["status"].as_str().unwrap_or("?")
        );
    }
    for warning in resp["warnings"].as_array().into_iter().flatten() {
        println!("  warning: {}", warning.as_str().unwrap_or("?"));
    }
    Ok(())
}

/// A section's name as a person reads it: "roles" -> "Roles".
fn section_title(section: &str) -> String {
    let mut chars = section.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

pub(crate) fn cmd_project_autonomy(
    client: &TackClient,
    project: &str,
    as_json: bool,
) -> anyhow::Result<()> {
    let a = client.get(&format!("/projects/{project}/autonomy"))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&a)?);
        return Ok(());
    }
    let list = |v: &serde_json::Value| {
        let items: Vec<&str> = v
            .as_array()
            .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
            .unwrap_or_default();
        if items.is_empty() {
            "-".to_owned()
        } else {
            items.join(", ")
        }
    };
    let limit = |v: &serde_json::Value| v.as_f64().map_or("-".to_owned(), |n| n.to_string());
    println!(
        "Level:               {}",
        a["level"].as_str().unwrap_or("?")
    );
    println!(
        "Merge on Done:       {}",
        a["merge_on_done"].as_bool().unwrap_or(false)
    );
    println!("Required stages:     {}", list(&a["verify_require"]));
    println!(
        "Signing key:         {}",
        if a["attestation_public_key"].is_string() {
            "set"
        } else {
            "none"
        }
    );
    println!("Loop cap:            {}", a["verify_loop_max_attempts"]);
    let b = &a["budgets"];
    println!(
        "Budgets per day:     tokens {}, auto merges {}, review queue {}",
        limit(&b["tokens_per_day"]),
        limit(&b["auto_merges_per_day"]),
        limit(&b["queue_items_max"])
    );
    println!("Domains:");
    let empty = vec![];
    for d in a["domains"].as_array().unwrap_or(&empty) {
        println!(
            "  {}  {}  paths: {}  types: {}",
            d["level"].as_str().unwrap_or("?"),
            d["name"].as_str().unwrap_or("?"),
            list(&d["path_globs"]),
            list(&d["item_types"])
        );
    }
    Ok(())
}

/// `tack types <project>`: the project's item types, with their level and how many items use each.
pub(crate) fn cmd_types(client: &TackClient, project: &str, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get(&format!("/projects/{project}/types"))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let empty = vec![];
    let types = resp.as_array().unwrap_or(&empty);
    print_table_header(&["KEY", "NAME", "LEVEL", "COLOR", "ITEMS", "DEFAULT"]);
    for t in types {
        let items = t["items"].as_u64().unwrap_or(0).to_string();
        print_table_row(&[
            t["key"].as_str().unwrap_or("?"),
            t["name"].as_str().unwrap_or("?"),
            match t["level"].as_str() {
                Some("group") => "holds work",
                Some("work") => "work",
                _ => "?",
            },
            t["color"].as_str().unwrap_or("?"),
            &items,
            if t["default_work"].as_bool().unwrap_or(false) {
                "yes"
            } else {
                ""
            },
        ]);
    }
    Ok(())
}

pub(crate) fn cmd_projects(client: &TackClient, as_json: bool) -> anyhow::Result<()> {
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
pub(crate) fn cmd_add(
    client: &TackClient,
    title: String,
    project: String,
    item_type: Option<String>,
    priority: String,
    parent: Option<String>,
    assignee: Option<String>,
    sprint: Option<String>,
    recurrence: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let mut body = json!({
        "title": title,
        "priority": priority,
    });
    if let Some(t) = item_type {
        body["item_type"] = json!(t);
    }
    if let Some(p) = parent {
        body["parent_id"] = json!(p);
    }
    if let Some(a) = assignee {
        body["assignee"] = json!(a);
    }
    if let Some(s) = sprint {
        body["sprint_id"] = json!(s);
    }
    if let Some(r) = recurrence {
        body["recurrence"] = json!(r);
    }
    let resp = client.post(&format!("/projects/{project}/items"), &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let id = resp["id"].as_str().unwrap_or("?");
    // The type the server gave it, by the name the project calls it.
    let names = vocab::type_names(client, &project);
    let key = resp["item_type"].as_str().unwrap_or("");
    let type_label = vocab::term(&names, key);
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

pub(crate) fn cmd_list(
    client: &TackClient,
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
    // The list endpoint returns a `{ data, total, page, per_page }` envelope;
    // unwrap the `data` array (tolerating a bare array for forward/back compat).
    let items_val = resp.get("data").cloned().unwrap_or(resp);
    if as_json {
        println!("{}", serde_json::to_string_pretty(&items_val)?);
        return Ok(());
    }
    let empty = vec![];
    let items = items_val.as_array().unwrap_or(&empty);
    if items.is_empty() {
        println!("No items found.");
        return Ok(());
    }
    let v = vocab::type_names(client, &project);
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

/// The request `tack move` sends: the status, the parent (an item, or `null` for the top), or both.
fn move_body(
    status: Option<&str>,
    under: Option<&str>,
    top: bool,
) -> anyhow::Result<serde_json::Value> {
    let mut body = json!({});
    if let Some(status) = status {
        body["status"] = json!(status);
    }
    if let Some(parent) = under {
        body["parent_id"] = json!(parent);
    } else if top {
        body["parent_id"] = serde_json::Value::Null;
    }
    if body.as_object().is_some_and(|fields| fields.is_empty()) {
        anyhow::bail!("give a status, --under <item> or --top");
    }
    Ok(body)
}

pub(crate) fn cmd_move(
    client: &TackClient,
    id: String,
    status: Option<String>,
    under: Option<String>,
    top: bool,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = move_body(status.as_deref(), under.as_deref(), top)?;
    let resp = client.patch(&format!("/items/{id}"), &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let short = &id[..8.min(id.len())];
    if let Some(status) = &status {
        println!("Moved {short}: → {status}");
    }
    if let Some(parent) = &under {
        println!("Moved {short}: under {}", &parent[..8.min(parent.len())]);
    } else if top {
        println!("Moved {short}: to the top");
    }
    Ok(())
}

pub(crate) fn cmd_branch(
    client: &TackClient,
    id: String,
    checkout: bool,
    prefix: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let resp = client.get(&format!("/items/{id}"))?;
    let item = resp.get("item").unwrap_or(&resp);

    let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let item_type = item
        .get("item_type")
        .and_then(|v| v.as_str())
        .unwrap_or("task");
    // The server echoes the canonical id; fall back to the user-supplied one.
    let item_id = item.get("id").and_then(|v| v.as_str()).unwrap_or(&id);

    let branch = git::branch_name(item_type, item_id, title, prefix.as_deref());

    if checkout {
        let status = std::process::Command::new("git")
            .args(["checkout", "-b", &branch])
            .status()
            .map_err(|e| anyhow::anyhow!("failed to run git: {e}"))?;
        if !status.success() {
            anyhow::bail!("git checkout -b {branch} failed");
        }
    }

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "branch": branch,
                "item_id": item_id,
                "checked_out": checkout,
            }))?
        );
        return Ok(());
    }

    if checkout {
        println!("Switched to a new branch '{branch}'");
    } else {
        // Print the command so it can be eval'd or copy-pasted; the bare branch
        // name goes to stderr context-free for scripting via the last word.
        println!("git checkout -b {branch}");
    }
    Ok(())
}

pub(crate) fn cmd_start(client: &TackClient, id: String, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get(&format!("/items/{id}"))?;
    let item = resp.get("item").unwrap_or(&resp);

    let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let item_type = item
        .get("item_type")
        .and_then(|v| v.as_str())
        .unwrap_or("task");
    // The server echoes the canonical id; fall back to the user-supplied one.
    let item_id = item
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or(&id)
        .to_string();
    let project_id = item
        .get("project_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("item {item_id} has no project_id"))?;

    // Find the first (lowest `order`) in-progress-category status in the
    // project's workflow, the same way `find_first_done_status` in
    // tack-core picks a status by category rather than by name.
    let project = client.get(&format!("/projects/{project_id}"))?;
    let target_status = project["workflow"]["statuses"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| s["category"].as_str() == Some("in_progress"))
        .min_by_key(|s| s["order"].as_i64().unwrap_or(i64::MAX))
        .and_then(|s| s["name"].as_str())
        .ok_or_else(|| anyhow::anyhow!("project {project_id} has no in-progress status"))?
        .to_string();

    // A status change the workflow refuses surfaces here as the server's
    // error (via `client.patch`'s `?`) and returns before any branch is
    // created.
    let body = json!({ "status": target_status });
    let patch_resp = client.patch(&format!("/items/{item_id}"), &body)?;
    let status = patch_resp
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or(&target_status)
        .to_string();

    let branch = git::branch_name(item_type, &item_id, title, None);

    let checkout_status = std::process::Command::new("git")
        .args(["checkout", "-b", &branch])
        .status()
        .map_err(|e| anyhow::anyhow!("failed to run git: {e}"))?;
    if !checkout_status.success() {
        anyhow::bail!("git checkout -b {branch} failed");
    }

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "item_id": item_id,
                "status": status,
                "branch": branch,
            }))?
        );
        return Ok(());
    }

    println!("Started {}: → {status}", &item_id[..8.min(item_id.len())]);
    println!("Switched to a new branch '{branch}'");
    Ok(())
}

pub(crate) fn cmd_open(
    client: &TackClient,
    base_url: &str,
    id: String,
    as_json: bool,
) -> anyhow::Result<()> {
    let resp = client.get(&format!("/items/{id}"))?;
    let item = resp.get("item").unwrap_or(&resp);

    let item_id = item.get("id").and_then(|v| v.as_str()).unwrap_or(&id);
    let project_id = item
        .get("project_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("item {item_id} has no project_id"))?;

    // The web UI's item detail drawer is deep-linkable via `?item=<id>` on
    // any of the work-surface lenses (see frontend/src/features/item-detail/
    // ItemDetailDrawer.tsx); `board` is the default lens.
    let url = format!(
        "{}/projects/{project_id}/board?item={item_id}",
        base_url.trim_end_matches('/')
    );

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "item_id": item_id,
                "url": url,
            }))?
        );
    } else {
        println!("{url}");
    }

    if let Some(browser) = tack_cli::config::browser_program() {
        let _ = std::process::Command::new(browser).arg(&url).status();
    }

    Ok(())
}

/// How `tack board` lays the work out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum BoardBy {
    /// One column per workflow status
    Status,
    /// One block per group (an epic, a building, a course), with how far its work is
    Parent,
}

/// Every item of the project that `node` selects, over all pages of the list.
fn fetch_items(
    client: &TackClient,
    project: &str,
    node: &str,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let mut items = Vec::new();
    for page in 1.. {
        let resp = client.get(&format!(
            "/projects/{project}/items?node={node}&per_page=500&page={page}"
        ))?;
        let batch = resp
            .get("data")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();
        let total = resp["total"].as_u64().unwrap_or(0) as usize;
        let empty = batch.is_empty();
        items.extend(batch);
        if empty || items.len() >= total {
            break;
        }
    }
    Ok(items)
}

/// The work of a project laid out as `tack board` shows it: columns by workflow status, or a
/// block per group. `statuses` are the workflow's, `groups` and `work` come from the item list.
fn board_view(
    by: BoardBy,
    statuses: &[serde_json::Value],
    groups: &[serde_json::Value],
    work: &[serde_json::Value],
) -> serde_json::Value {
    match by {
        BoardBy::Status => {
            let mut ordered: Vec<&serde_json::Value> = statuses.iter().collect();
            ordered.sort_by_key(|s| s["order"].as_i64().unwrap_or(i64::MAX));
            let mut columns: Vec<serde_json::Value> = ordered
                .iter()
                .map(|s| {
                    let name = s["name"].as_str().unwrap_or("?");
                    json!({
                        "status": name,
                        "wip_limit": s["wip_limit"],
                        "items": work.iter().filter(|i| i["status"] == name).collect::<Vec<_>>(),
                    })
                })
                .collect();
            // Work in a status the workflow no longer names still shows, after the others.
            let known: Vec<&str> = ordered.iter().filter_map(|s| s["name"].as_str()).collect();
            let mut strays: Vec<&str> = work
                .iter()
                .filter_map(|i| i["status"].as_str())
                .filter(|s| !known.contains(s))
                .collect();
            strays.dedup();
            for name in strays {
                columns.push(json!({
                    "status": name,
                    "wip_limit": null,
                    "items": work.iter().filter(|i| i["status"] == name).collect::<Vec<_>>(),
                }));
            }
            json!({ "columns": columns })
        }
        BoardBy::Parent => {
            let mut blocks: Vec<serde_json::Value> = groups
                .iter()
                .map(|g| {
                    json!({
                        "group": g,
                        "items": work.iter().filter(|i| i["parent_id"] == g["id"]).collect::<Vec<_>>(),
                    })
                })
                .collect();
            let held: Vec<&serde_json::Value> = groups.iter().map(|g| &g["id"]).collect();
            let loose: Vec<&serde_json::Value> = work
                .iter()
                .filter(|i| !held.contains(&&i["parent_id"]))
                .collect();
            if !loose.is_empty() {
                blocks.push(json!({ "group": null, "items": loose }));
            }
            json!({ "blocks": blocks })
        }
    }
}

/// "3 of 7 done", or "12 of 20 points" when the work has estimates, and who is waited for.
fn progress_line(progress: &serde_json::Value) -> String {
    if progress.is_null() {
        return "no work yet".to_string();
    }
    let number = |v: &serde_json::Value| {
        let n = v.as_f64().unwrap_or(0.0);
        if n.fract() == 0.0 {
            format!("{n:.0}")
        } else {
            format!("{n}")
        }
    };
    let mut line = if progress["estimate_total"].is_null() {
        format!(
            "{} of {} done",
            progress["done"].as_u64().unwrap_or(0),
            progress["total"].as_u64().unwrap_or(0)
        )
    } else {
        format!(
            "{} of {} points",
            number(&progress["estimate_done"]),
            number(&progress["estimate_total"])
        )
    };
    let needs_you = progress["needs_you"].as_u64().unwrap_or(0);
    if needs_you > 0 {
        line.push_str(&format!(" · {needs_you} need you"));
    }
    line
}

fn item_line(item: &serde_json::Value, types: &vocab::VocabMap, with_status: bool) -> String {
    let title = item["title"].as_str().unwrap_or("?");
    let type_label = vocab::term(types, item["item_type"].as_str().unwrap_or("task"));
    let status = match (with_status, item["status"].as_str()) {
        (true, Some(s)) => format!(" · {s}"),
        _ => String::new(),
    };
    let assignee = item["assignee"]
        .as_str()
        .map(|a| format!(" [{a}]"))
        .unwrap_or_default();
    format!("  · {title} ({type_label}){status}{assignee}")
}

/// [`board_view`] as the text `tack board` prints.
fn render_board(view: &serde_json::Value, types: &vocab::VocabMap) -> String {
    let mut out = String::new();
    let empty = vec![];
    for col in view["columns"].as_array().unwrap_or(&empty) {
        let name = col["status"].as_str().unwrap_or("?");
        let items = col["items"].as_array().unwrap_or(&empty);
        let label = match col["wip_limit"].as_u64() {
            Some(limit) if items.len() as u64 > limit => format!(" ({}/{limit} ⚠)", items.len()),
            Some(limit) => format!(" ({}/{limit})", items.len()),
            None => format!(" ({})", items.len()),
        };
        out.push_str(&format!("\n{name}{label}\n"));
        out.push_str(&format!("{}\n", "─".repeat(name.chars().count() + label.chars().count())));
        if items.is_empty() {
            out.push_str("  (empty)\n");
        }
        for item in items {
            out.push_str(&item_line(item, types, false));
            out.push('\n');
        }
    }
    for block in view["blocks"].as_array().unwrap_or(&empty) {
        let items = block["items"].as_array().unwrap_or(&empty);
        let heading = match block["group"].as_object() {
            Some(group) => {
                let title = group["title"].as_str().unwrap_or("?");
                let kind = vocab::term(types, group["item_type"].as_str().unwrap_or(""));
                format!(
                    "{title} ({kind}) · {}",
                    progress_line(group.get("progress").unwrap_or(&serde_json::Value::Null))
                )
            }
            None => "No parent".to_string(),
        };
        out.push_str(&format!("\n{heading}\n{}\n", "─".repeat(heading.chars().count())));
        if items.is_empty() {
            out.push_str("  (no work directly under it)\n");
        }
        for item in items {
            out.push_str(&item_line(item, types, true));
            out.push('\n');
        }
    }
    out
}

pub(crate) fn cmd_board(
    client: &TackClient,
    project: String,
    by: BoardBy,
    as_json: bool,
) -> anyhow::Result<()> {
    let work = fetch_items(client, &project, "work")?;
    let (statuses, groups) = match by {
        BoardBy::Status => {
            let p = client.get(&format!("/projects/{project}"))?;
            (p["workflow"]["statuses"].as_array().cloned().unwrap_or_default(), vec![])
        }
        BoardBy::Parent => (vec![], fetch_items(client, &project, "group")?),
    };
    let view = board_view(by, &statuses, &groups, &work);
    if as_json {
        println!("{}", serde_json::to_string_pretty(&view)?);
        return Ok(());
    }
    let types = vocab::type_names(client, &project);
    print!("{}", render_board(&view, &types));
    Ok(())
}

pub(crate) fn cmd_search(
    client: &TackClient,
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

pub(crate) fn run_import(
    client: &TackClient,
    file: &std::path::Path,
    project: Option<String>,
    name: Option<String>,
    project_type: Option<String>,
    dry_run: bool,
    json: bool,
) -> anyhow::Result<()> {
    cmd_import(
        client,
        file,
        ImportTarget {
            project,
            name,
            project_type,
        },
        dry_run,
        json,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types() -> vocab::VocabMap {
        vocab::VocabMap::from([("epic".into(), "Course".into()), ("task".into(), "Task".into())])
    }

    #[test]
    fn the_status_view_has_a_column_per_status_in_workflow_order() {
        let statuses = [
            json!({"name": "Done", "order": 2, "wip_limit": null}),
            json!({"name": "To Do", "order": 0, "wip_limit": null}),
            json!({"name": "Doing", "order": 1, "wip_limit": 1}),
        ];
        let work = [
            json!({"title": "A", "status": "Doing", "item_type": "task"}),
            json!({"title": "B", "status": "Doing", "item_type": "task"}),
            json!({"title": "C", "status": "Old", "item_type": "task"}),
        ];
        let view = board_view(BoardBy::Status, &statuses, &[], &work);
        let names: Vec<&str> = view["columns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["status"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["To Do", "Doing", "Done", "Old"]);
        let text = render_board(&view, &types());
        assert!(text.contains("Doing (2/1 ⚠)"), "{text}");
        assert!(text.contains("To Do (0)\n"), "{text}");
        assert!(text.contains("  · C (Task)"), "{text}");
    }

    #[test]
    fn the_parent_view_has_a_block_per_group_with_its_progress_and_one_for_work_with_no_parent() {
        let groups = [
            json!({"id": "g1", "title": "Algebra", "item_type": "epic",
                   "progress": {"total": 7, "done": 3, "estimate_total": null, "needs_you": 2}}),
            json!({"id": "g2", "title": "Geometry", "item_type": "epic",
                   "progress": {"total": 2, "done": 1, "estimate_total": 20.0, "estimate_done": 12.0, "needs_you": 0}}),
            json!({"id": "g3", "title": "Empty", "item_type": "epic"}),
        ];
        let work = [
            json!({"title": "A", "status": "Done", "item_type": "task", "parent_id": "g1"}),
            json!({"title": "Loose", "status": "To Do", "item_type": "task", "parent_id": null}),
        ];
        let text = render_board(&board_view(BoardBy::Parent, &[], &groups, &work), &types());
        assert!(text.contains("Algebra (Course) · 3 of 7 done · 2 need you"), "{text}");
        assert!(text.contains("Geometry (Course) · 12 of 20 points"), "{text}");
        assert!(text.contains("Empty (Course) · no work yet"), "{text}");
        assert!(text.contains("  · A (Task) · Done"), "{text}");
        assert!(text.contains("No parent\n"), "{text}");
        assert!(text.contains("  · Loose (Task) · To Do"), "{text}");
    }

    #[test]
    fn a_move_sends_the_status_the_parent_or_both() {
        assert_eq!(
            move_body(Some("Done"), None, false).unwrap(),
            json!({ "status": "Done" })
        );
        assert_eq!(
            move_body(None, Some("abc"), false).unwrap(),
            json!({ "parent_id": "abc" })
        );
        assert_eq!(
            move_body(Some("Done"), Some("abc"), false).unwrap(),
            json!({ "status": "Done", "parent_id": "abc" })
        );
    }

    #[test]
    fn top_sends_a_null_parent_and_a_move_with_nothing_to_do_is_refused() {
        assert_eq!(
            move_body(None, None, true).unwrap(),
            json!({ "parent_id": null })
        );
        assert!(move_body(None, None, false).is_err());
    }
}
