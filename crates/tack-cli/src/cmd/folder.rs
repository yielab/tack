//! `tack folder`: the project folder, a project's plan kept in a repository.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use clap::Subcommand;
use serde_json::{Value, json};

use tack_cli::client::TackClient;

#[derive(Subcommand)]
pub(crate) enum FolderAction {
    /// Show what changed on the board since the project folder's last revision
    Status {
        /// Project ID or name
        project: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Write the project folder's files, and remove the item files the board no longer has
    Export {
        /// Project ID or name
        project: String,
        /// The folder to write (default .tack in the current directory)
        dir: Option<PathBuf>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Read the project folder, show the plan and apply it
    Import {
        /// Project ID or name
        project: String,
        /// The folder to read (default .tack in the current directory)
        dir: Option<PathBuf>,
        /// Show the plan and write nothing
        #[arg(long)]
        dry_run: bool,
        /// Apply a plan that removes things
        #[arg(long)]
        yes: bool,
        /// Settle a conflict: folder or board
        #[arg(long, value_parser = ["folder", "board"])]
        prefer: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// List the project folder's revisions, newest first
    History {
        /// Project ID or name
        project: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Go back to a revision: show the plan and apply it
    Revert {
        /// Project ID or name
        project: String,
        /// The revision's number
        revision: u64,
        /// Show the plan and write nothing
        #[arg(long)]
        dry_run: bool,
        /// Apply a plan that removes things
        #[arg(long)]
        yes: bool,
        /// Settle a conflict: folder or board
        #[arg(long, value_parser = ["folder", "board"])]
        prefer: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
}

pub(crate) fn run_folder(client: &TackClient, action: FolderAction) -> anyhow::Result<()> {
    match action {
        FolderAction::Status { project, json } => {
            let status = client.get(&format!("/projects/{project}/folder/status"))?;
            if json {
                return print_json(&status);
            }
            match status["last_revision"]["number"].as_u64() {
                Some(n) => println!(
                    "Last revision: #{n} ({}, {})",
                    status["last_revision"]["origin"].as_str().unwrap_or("?"),
                    status["last_revision"]["created_at"]
                        .as_str()
                        .unwrap_or("?")
                ),
                None => println!("No revision yet: export once to make the folder authoritative"),
            }
            if status["changes"].as_array().is_none_or(Vec::is_empty) {
                println!("The board matches the last revision");
            }
            print_plan(&status);
            Ok(())
        }
        FolderAction::Export { project, dir, json } => {
            let answer = client.get(&format!("/projects/{project}/folder"))?;
            let written = write_folder(&dir_or_default(dir), &answer["files"])?;
            if json {
                return print_json(&answer);
            }
            println!(
                "Wrote {} files; removed {} item files the board no longer has",
                written.0, written.1
            );
            Ok(())
        }
        FolderAction::Import {
            project,
            dir,
            dry_run,
            yes,
            prefer,
            json,
        } => {
            let files = read_folder(&dir_or_default(dir))?;
            let body = json!({ "files": files }).to_string();
            let prefer = prefer.map(|p| ("prefer", folder_is_file(p)));
            let query: Vec<_> = prefer.iter().map(|(k, v)| (*k, v.as_str())).collect();
            plan_and_apply(client, &project, &query, Some(body), dry_run, yes, json)
        }
        FolderAction::History { project, json } => {
            let revisions = client.get(&format!("/projects/{project}/folder/revisions"))?;
            if json {
                return print_json(&revisions);
            }
            for r in revisions.as_array().into_iter().flatten() {
                println!(
                    "#{} · {} · {} · {}",
                    r["number"],
                    r["origin"].as_str().unwrap_or("?"),
                    r["created_at"].as_str().unwrap_or("?"),
                    r["hash"].as_str().unwrap_or("?")
                );
            }
            Ok(())
        }
        FolderAction::Revert {
            project,
            revision,
            dry_run,
            yes,
            prefer,
            json,
        } => {
            let number = revision.to_string();
            let mut query = vec![("revision", number.as_str())];
            let prefer = prefer.map(folder_is_file);
            query.extend(prefer.as_deref().map(|p| ("prefer", p)));
            plan_and_apply(client, &project, &query, None, dry_run, yes, json)
        }
    }
}

/// Plans what `files` (or the revision in `query`) would do, prints it and, unless `dry_run`,
/// applies it with the plan's hash.
fn plan_and_apply(
    client: &TackClient,
    project: &str,
    query: &[(&str, &str)],
    body: Option<String>,
    dry_run: bool,
    yes: bool,
    as_json: bool,
) -> anyhow::Result<()> {
    let mut query: Vec<(&str, &str)> = query.to_vec();
    let post = |route: &str, query: &[(&str, &str)]| {
        client.post_text(
            &format!("/projects/{project}/folder/{route}"),
            query,
            "application/json",
            body.clone().unwrap_or_default(),
        )
    };
    let plan = post("plan", &query)?;
    if as_json {
        print_json(&plan)?;
    } else {
        print_plan(&plan);
    }
    if dry_run {
        return Ok(());
    }
    let conflicts = plan["conflicts"].as_array().map_or(0, Vec::len);
    if plan["applies"].as_bool() == Some(false) {
        anyhow::bail!("{conflicts} conflicts: add --prefer folder or --prefer board");
    }
    let removals = plan["removals"].as_array().map_or(0, Vec::len);
    if removals > 0 && !yes {
        anyhow::bail!("add --yes to apply the {removals} removals");
    }
    let hash = plan["plan_hash"].as_str().unwrap_or_default();
    query.push(("plan_hash", hash));
    let applied = post("apply", &query)?;
    if as_json {
        return print_json(&applied);
    }
    println!(
        "Applied revision #{}: {} changes, {} removed",
        applied["revision"]["number"],
        applied["changes"].as_array().map_or(0, Vec::len),
        applied["removals"].as_array().map_or(0, Vec::len)
    );
    Ok(())
}

/// The board's word for `--prefer folder`.
fn folder_is_file(prefer: String) -> String {
    if prefer == "folder" {
        "file".into()
    } else {
        prefer
    }
}

fn print_json(value: &Value) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

/// One plan, as `import`, `revert` and `status` show it: what changes, what is kept and why, the
/// conflicts with their three values, what is removed and what it costs, and the warnings.
fn print_plan(plan: &Value) {
    let text = |v: &Value| v.as_str().unwrap_or("?").to_owned();
    let mut kept = Vec::new();
    for change in plan["changes"].as_array().into_iter().flatten() {
        if change["verb"] == "keep" {
            kept.push(change);
            continue;
        }
        let head = format!(
            "{} {} {}",
            text(&change["verb"]),
            text(&change["section"]),
            text(&change["name"])
        );
        let fields = change["fields"].as_array().map_or(&[][..], Vec::as_slice);
        if fields.is_empty() {
            println!("{head}");
        }
        for field in fields {
            println!(
                "{head}: {} {} → {}",
                text(&field["path"]),
                field["from"],
                field["to"]
            );
        }
    }
    if !kept.is_empty() {
        println!("Kept:");
        for change in kept {
            println!(
                "  {} {}: {}",
                text(&change["section"]),
                text(&change["name"]),
                text(&change["reason"])
            );
        }
    }
    let conflicts = plan["conflicts"].as_array().map_or(&[][..], Vec::as_slice);
    if !conflicts.is_empty() {
        println!("Conflicts:");
        for c in conflicts {
            println!(
                "  {} {}: {}",
                text(&c["section"]),
                text(&c["name"]),
                text(&c["path"])
            );
            println!("    last revision: {}", c["base"]);
            println!("    board: {}", c["board"]);
            println!("    folder: {}", c["file"]);
        }
    }
    let removals = plan["removals"].as_array().map_or(&[][..], Vec::as_slice);
    if !removals.is_empty() {
        println!("Removed ({}):", removals.len());
        for r in removals {
            println!("  {} {}", text(&r["section"]), text(&r["name"]));
            for cost in r["cost"].as_array().into_iter().flatten() {
                println!("    {}", text(cost));
            }
        }
    }
    for warning in plan["warnings"].as_array().into_iter().flatten() {
        println!("warning: {}", text(warning));
    }
}

fn dir_or_default(dir: Option<PathBuf>) -> PathBuf {
    dir.unwrap_or_else(|| PathBuf::from(".tack"))
}

/// Every file under `dir`, by its path relative to it.
fn read_folder(dir: &Path) -> anyhow::Result<BTreeMap<String, String>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        let entries = std::fs::read_dir(&current)
            .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", current.display()))?;
        for entry in entries {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let text = std::fs::read_to_string(&path)
                .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;
            let relative = path.strip_prefix(dir)?;
            let name: Vec<_> = relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect();
            files.insert(name.join("/"), text);
        }
    }
    Ok(files)
}

/// Writes the answer's files under `dir` and removes the `items/*.md` it lacks. Returns how many
/// files it wrote and how many it removed.
fn write_folder(dir: &Path, files: &Value) -> anyhow::Result<(usize, usize)> {
    let files = files
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("the board answered with no files"))?;
    for (name, text) in files {
        let relative = Path::new(name);
        if !relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        {
            anyhow::bail!("the board named a file outside the folder: {name}");
        }
        let path = dir.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("cannot create {}: {e}", parent.display()))?;
        }
        std::fs::write(&path, text.as_str().unwrap_or_default())
            .map_err(|e| anyhow::anyhow!("cannot write {}: {e}", path.display()))?;
    }
    std::fs::create_dir_all(dir.join("items"))?;
    let mut removed = 0;
    for entry in std::fs::read_dir(dir.join("items"))? {
        let path = entry?.path();
        let name = format!(
            "items/{}",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        if path.extension().is_some_and(|e| e == "md") && !files.contains_key(&name) {
            std::fs::remove_file(&path)?;
            removed += 1;
        }
    }
    Ok((files.len(), removed))
}
