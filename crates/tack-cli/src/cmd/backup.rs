use serde_json::json;

use tack_cli::client::TackClient;

pub(crate) fn cmd_backup(
    client: &TackClient,
    path: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    let bytes = client.get_bytes("/backup")?;
    let out = path.unwrap_or_else(|| std::path::PathBuf::from("tack-backup.db"));
    std::fs::write(&out, &bytes)
        .map_err(|e| anyhow::anyhow!("Cannot write {}: {e}", out.display()))?;
    println!("Backup saved: {} ({} bytes)", out.display(), bytes.len());
    Ok(())
}

pub(crate) fn cmd_restore(
    client: &TackClient,
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

pub(crate) fn cmd_backup_remote(client: &TackClient, as_json: bool) -> anyhow::Result<()> {
    let resp = client.post("/backup/remote", &json!({}))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let key = resp["object_key"].as_str().unwrap_or("?");
    let bytes = resp["bundle_size_bytes"].as_u64().unwrap_or(0);
    let created = resp["created_at"].as_str().unwrap_or("-");
    println!("Remote backup uploaded:");
    println!("  Key:     {key}");
    println!("  Size:    {} bytes", bytes);
    println!("  Created: {created}");
    Ok(())
}

pub(crate) fn cmd_list_remote_backups(client: &TackClient, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get("/backup/remote")?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let empty = vec![];
    let backups = resp.as_array().unwrap_or(&empty);
    if backups.is_empty() {
        println!("No remote backups found.");
        return Ok(());
    }
    println!(
        "{:<32} {:>12} {:>8} Key",
        "Created", "Size (bytes)", "Items"
    );
    println!("{}", "-".repeat(90));
    for b in backups {
        let raw_date = b["created_at"].as_str().unwrap_or("-");
        let created = &raw_date[..19.min(raw_date.len())];
        let size = b["bundle_size_bytes"].as_u64().unwrap_or(0);
        let items = b["item_count"].as_u64().unwrap_or(0);
        let key = b["object_key"].as_str().unwrap_or("-");
        println!("{:<32} {:>12} {:>8} {}", created, size, items, key);
    }
    Ok(())
}

pub(crate) fn cmd_restore_remote(
    client: &TackClient,
    key: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = match key {
        Some(k) => json!({ "key": k }),
        None => json!({}),
    };
    let resp = client.post("/backup/remote/restore", &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let msg = resp["message"].as_str().unwrap_or("Restore staged.");
    println!("{msg}");
    Ok(())
}

pub(crate) fn run_backup(
    client: &TackClient,
    path: Option<std::path::PathBuf>,
    remote: bool,
    json: bool,
) -> anyhow::Result<()> {
    if remote {
        cmd_backup_remote(client, json)
    } else {
        cmd_backup(client, path)
    }
}

pub(crate) fn run_restore(
    client: &TackClient,
    path: Option<std::path::PathBuf>,
    remote: bool,
    key: Option<String>,
    json: bool,
) -> anyhow::Result<()> {
    if remote {
        cmd_restore_remote(client, key, json)
    } else {
        let p =
            path.ok_or_else(|| anyhow::anyhow!("'path' is required when not using --remote"))?;
        cmd_restore(client, p, json)
    }
}
