//! Remote cloud backup — S3-compatible object storage (Cloudflare R2, Backblaze B2, AWS S3,
//! MinIO, or any other S3-compatible endpoint).
//!
//! # Bundle format
//! Each backup is a `flexpm-backup-<UTC-timestamp>.tar.zst` archive containing:
//! - `database.db`      — VACUUM INTO SQLite snapshot
//! - `attachments/…`    — copy of FLEXPM_STORAGE_DIR (omitted when empty / dir absent)
//! - `manifest.json`    — metadata (format_version, timestamps, migration count, sha256, etc.)
//!
//! A sidecar `<archive>.manifest.json` is stored alongside each bundle so `list()` can
//! return metadata without downloading the full archive.

use std::io::{Cursor, Read as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use object_store::ObjectStore;
use object_store::aws::AmazonS3Builder;
use object_store::path::Path as OsPath;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use thiserror::Error;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::config::AppConfig;

// ── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("remote backup is not configured (set FLEXPM_BACKUP_BUCKET, _ACCESS_KEY, _SECRET_KEY)")]
    NotConfigured,
    #[error("backup requires a file-based database (not in-memory)")]
    InMemoryDb,
    #[error("object store error: {0}")]
    ObjectStore(#[from] object_store::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("zstd error: {0}")]
    Zstd(String),
    #[error("restore rejected: snapshot migration_version ({snapshot}) is ahead of the running binary ({local}); upgrade FlexPM before restoring")]
    SchemaTooNew { snapshot: u32, local: u32 },
    #[error("bundle is corrupt or unrecognised format")]
    CorruptBundle,
}

// ── Manifest ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    /// Always 1 for this implementation.
    pub format_version: u32,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
    /// Number of rows in the `_migrations` table at backup time.
    pub migration_version: u32,
    /// Hex-encoded SHA-256 of the raw `database.db` bytes.
    pub db_sha256: String,
    /// Stable install identifier (UUID v4, generated once per install).
    pub install_id: String,
    /// Total number of items in the database at backup time.
    pub item_count: u64,
    /// Object key of the `.tar.zst` archive in the bucket.
    pub object_key: String,
    /// Approximate bundle size in bytes.
    pub bundle_size_bytes: u64,
}

// ── Store construction ────────────────────────────────────────────────────────

/// Build an S3-compatible `ObjectStore` from application config.
pub fn store_from_config(cfg: &AppConfig) -> Result<Arc<dyn ObjectStore>, BackupError> {
    if !cfg.remote_backup_enabled() {
        return Err(BackupError::NotConfigured);
    }

    let bucket = cfg.backup_bucket.as_deref().unwrap();
    let access_key = cfg.backup_access_key.as_deref().unwrap();
    let secret_key = cfg.backup_secret_key.as_deref().unwrap();

    let mut builder = AmazonS3Builder::new()
        .with_bucket_name(bucket)
        .with_region(&cfg.backup_region)
        .with_access_key_id(access_key)
        .with_secret_access_key(secret_key);

    if let Some(endpoint) = &cfg.backup_endpoint {
        builder = builder.with_endpoint(endpoint);
        // Allow plain HTTP for local MinIO / test setups
        if endpoint.starts_with("http://") {
            builder = builder.with_allow_http(true);
        }
    }

    let store = builder.build()?;
    Ok(Arc::new(store))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Query the current migration version (count of applied rows in `_migrations`).
async fn migration_version(pool: &SqlitePool) -> Result<u32, BackupError> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _migrations")
        .fetch_one(pool)
        .await?;
    Ok(count as u32)
}

/// Query the total item count.
async fn item_count(pool: &SqlitePool) -> Result<u64, BackupError> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM items")
        .fetch_one(pool)
        .await?;
    Ok(count as u64)
}

/// Get or create a stable install ID stored in the `app_meta` table.
pub async fn install_id(pool: &SqlitePool) -> Result<String, BackupError> {
    // Create the table if it doesn't exist (idempotent).
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS app_meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL)",
    )
    .execute(pool)
    .await?;

    let existing: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_meta WHERE key = 'install_id'")
            .fetch_optional(pool)
            .await?;

    if let Some(id) = existing {
        return Ok(id);
    }

    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO app_meta (key, value) VALUES ('install_id', ?)")
        .bind(&id)
        .execute(pool)
        .await?;
    Ok(id)
}

/// Create a VACUUM INTO snapshot and return the raw bytes of the DB file.
async fn snapshot_db(pool: &SqlitePool, db_path: &Path) -> Result<Vec<u8>, BackupError> {
    // Checkpoint WAL into the main file first.
    sqlx::query("PRAGMA wal_checkpoint(FULL)")
        .execute(pool)
        .await?;

    let temp = std::env::temp_dir().join(format!("flexpm-snap-{}.db", Uuid::new_v4()));
    let temp_str = temp.to_string_lossy().replace('\'', "''");

    sqlx::query(&format!("VACUUM INTO '{temp_str}'"))
        .execute(pool)
        .await?;

    let bytes = tokio::fs::read(&temp).await?;
    let _ = tokio::fs::remove_file(&temp).await;
    debug!(db = %db_path.display(), bytes = bytes.len(), "DB snapshot complete");
    Ok(bytes)
}

// ── Bundle creation ───────────────────────────────────────────────────────────

/// Build the full `.tar.zst` bundle in memory and return it together with its manifest.
pub async fn create_bundle(
    pool: &SqlitePool,
    cfg: &AppConfig,
) -> Result<(Vec<u8>, BackupManifest), BackupError> {
    let db_path = cfg
        .db_file_path()
        .ok_or(BackupError::InMemoryDb)?;

    let db_bytes = snapshot_db(pool, &db_path).await?;
    let db_sha256 = hex::encode(Sha256::digest(&db_bytes));

    let mig_version = migration_version(pool).await?;
    let items = item_count(pool).await?;
    let install = install_id(pool).await?;
    let created_at = Utc::now().to_rfc3339();

    // Build tar in memory, then compress.
    let tar_bytes = build_tar(&db_bytes, &cfg.storage_dir, &created_at, mig_version, items, &install, &db_sha256)?;

    let compressed = zstd::encode_all(Cursor::new(&tar_bytes), 3)
        .map_err(|e| BackupError::Zstd(e.to_string()))?;

    let ts = created_at.replace(':', "-").replace('+', "Z");
    let object_key = format!("{}/flexpm-backup-{}.tar.zst", cfg.backup_prefix, ts);

    let manifest = BackupManifest {
        format_version: 1,
        created_at,
        migration_version: mig_version,
        db_sha256,
        install_id: install,
        item_count: items,
        object_key,
        bundle_size_bytes: compressed.len() as u64,
    };

    Ok((compressed, manifest))
}

fn build_tar(
    db_bytes: &[u8],
    storage_dir: &str,
    created_at: &str,
    mig_version: u32,
    items: u64,
    install: &str,
    db_sha256: &str,
) -> Result<Vec<u8>, BackupError> {
    let mut ar = tar::Builder::new(Vec::new());

    // database.db
    let mut header = tar::Header::new_gnu();
    header.set_size(db_bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    ar.append_data(&mut header, "database.db", Cursor::new(db_bytes))?;

    // attachments/ — walk storage_dir recursively if it exists
    let storage = Path::new(storage_dir);
    if storage.is_dir() {
        append_dir_recursive(&mut ar, storage, storage)?;
    }

    // manifest.json
    let manifest_json = serde_json::to_vec(&serde_json::json!({
        "format_version": 1,
        "created_at": created_at,
        "migration_version": mig_version,
        "db_sha256": db_sha256,
        "install_id": install,
        "item_count": items,
    }))?;
    let mut mh = tar::Header::new_gnu();
    mh.set_size(manifest_json.len() as u64);
    mh.set_mode(0o644);
    mh.set_cksum();
    ar.append_data(&mut mh, "manifest.json", Cursor::new(&manifest_json))?;

    ar.finish()?;
    Ok(ar.into_inner()?)
}

fn append_dir_recursive(
    ar: &mut tar::Builder<Vec<u8>>,
    base: &Path,
    dir: &Path,
) -> Result<(), BackupError> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(base).unwrap_or(&path);
        let tar_path = PathBuf::from("attachments").join(rel);

        if path.is_dir() {
            append_dir_recursive(ar, base, &path)?;
        } else {
            let data = std::fs::read(&path)?;
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            ar.append_data(&mut header, &tar_path, Cursor::new(data))?;
        }
    }
    Ok(())
}

// ── Upload ────────────────────────────────────────────────────────────────────

/// Upload a bundle + sidecar manifest to the object store.
pub async fn upload(
    store: &dyn ObjectStore,
    manifest: &BackupManifest,
    bundle: Vec<u8>,
) -> Result<(), BackupError> {
    let bundle_key = OsPath::from(manifest.object_key.clone());
    let sidecar_key = OsPath::from(format!("{}.manifest.json", manifest.object_key));

    let bundle_payload = object_store::PutPayload::from(bundle);
    store.put(&bundle_key, bundle_payload).await?;
    info!(key = %manifest.object_key, bytes = manifest.bundle_size_bytes, "Uploaded backup bundle");

    let sidecar_bytes = serde_json::to_vec(manifest)?;
    let sidecar_payload = object_store::PutPayload::from(sidecar_bytes);
    store.put(&sidecar_key, sidecar_payload).await?;
    debug!(key = %sidecar_key, "Uploaded sidecar manifest");

    Ok(())
}

// ── List ──────────────────────────────────────────────────────────────────────

/// List remote backups newest-first by reading sidecar manifests.
pub async fn list(
    store: &dyn ObjectStore,
    prefix: &str,
) -> Result<Vec<BackupManifest>, BackupError> {
    use futures::StreamExt;

    let prefix_path = OsPath::from(format!("{}/", prefix));
    let mut stream = store.list(Some(&prefix_path));
    let mut manifests: Vec<BackupManifest> = Vec::new();

    while let Some(item) = stream.next().await {
        let meta = item?;
        let key = meta.location.to_string();
        if !key.ends_with(".manifest.json") {
            continue;
        }
        let bytes = store.get(&meta.location).await?.bytes().await?;
        match serde_json::from_slice::<BackupManifest>(&bytes) {
            Ok(m) => manifests.push(m),
            Err(e) => warn!(key = %key, error = %e, "Could not parse sidecar manifest, skipping"),
        }
    }

    // Sort newest first
    manifests.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(manifests)
}

// ── Download ─────────────────────────────────────────────────────────────────

/// Download a bundle by object key and return its raw bytes.
pub async fn download(store: &dyn ObjectStore, key: &str) -> Result<Vec<u8>, BackupError> {
    let path = OsPath::from(key);
    let bytes = store.get(&path).await?.bytes().await?;
    Ok(bytes.to_vec())
}

// ── Prune ─────────────────────────────────────────────────────────────────────

/// Delete oldest backups, keeping the newest `keep` bundles. Returns number deleted.
pub async fn prune(
    store: &dyn ObjectStore,
    prefix: &str,
    keep: usize,
) -> Result<usize, BackupError> {
    let mut manifests = list(store, prefix).await?;
    if manifests.len() <= keep {
        return Ok(0);
    }

    // `list` returns newest-first; keep the first `keep`, delete the rest.
    let to_delete = manifests.split_off(keep);
    let mut deleted = 0;

    for m in &to_delete {
        let bundle_key = OsPath::from(m.object_key.clone());
        let sidecar_key = OsPath::from(format!("{}.manifest.json", m.object_key));
        if let Err(e) = store.delete(&bundle_key).await {
            warn!(key = %m.object_key, error = %e, "Failed to delete old bundle");
        } else {
            deleted += 1;
        }
        let _ = store.delete(&sidecar_key).await;
    }

    info!(deleted, kept = keep, "Pruned old remote backups");
    Ok(deleted)
}

// ── Restore helpers ───────────────────────────────────────────────────────────

/// A file extracted from a bundle, ready to be written to disk.
struct ExtractedFile {
    dest: PathBuf,
    data: Vec<u8>,
}

/// Extract DB and attachments from a bundle and stage them for next startup.
///
/// Writes:
/// - `<db_path>.restore`       — the database snapshot
/// - `<storage_dir>.restore/`  — the extracted attachment tree (if present in bundle)
pub async fn stage_restore(
    bundle_bytes: Vec<u8>,
    manifest: &BackupManifest,
    local_migration_version: u32,
    db_path: &Path,
    storage_dir: &str,
) -> Result<(), BackupError> {
    if manifest.migration_version > local_migration_version {
        return Err(BackupError::SchemaTooNew {
            snapshot: manifest.migration_version,
            local: local_migration_version,
        });
    }

    let restore_db_path = PathBuf::from(format!("{}.restore", db_path.to_string_lossy()));
    let restore_storage = format!("{}.restore", storage_dir);
    let restore_storage_path = PathBuf::from(&restore_storage);

    // All sync work (decompression + tar parsing) before any await points.
    let (db_bytes, attachments) = tokio::task::spawn_blocking(move || {
        parse_bundle(&bundle_bytes, &restore_storage_path)
    })
    .await
    .map_err(|e| BackupError::Io(std::io::Error::other(e.to_string())))??;

    // Async I/O: write the staged files.
    tokio::fs::write(&restore_db_path, &db_bytes).await?;

    for file in attachments {
        if let Some(parent) = file.dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&file.dest, &file.data).await?;
    }

    info!(
        db_restore = %restore_db_path.display(),
        storage_restore = %restore_storage,
        "Restore staged — restart the server to apply"
    );
    Ok(())
}

/// Decompress and parse a bundle synchronously. Returns `(db_bytes, attachment_files)`.
fn parse_bundle(
    bundle_bytes: &[u8],
    restore_storage_path: &Path,
) -> Result<(Vec<u8>, Vec<ExtractedFile>), BackupError> {
    let decompressed = zstd::decode_all(Cursor::new(bundle_bytes))
        .map_err(|e| BackupError::Zstd(e.to_string()))?;

    let mut archive = tar::Archive::new(Cursor::new(decompressed));
    let mut db_bytes: Option<Vec<u8>> = None;
    let mut attachments: Vec<ExtractedFile> = Vec::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_path = entry.path()?.into_owned();
        let name = entry_path.to_string_lossy();

        if name == "database.db" {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            db_bytes = Some(buf);
        } else if name.starts_with("attachments/") {
            let rel = entry_path
                .strip_prefix("attachments/")
                .unwrap_or(&entry_path);
            if rel.as_os_str().is_empty() {
                continue;
            }
            let dest = restore_storage_path.join(rel);
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            attachments.push(ExtractedFile { dest, data: buf });
        }
    }

    let db = db_bytes.ok_or(BackupError::CorruptBundle)?;
    Ok((db, attachments))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use object_store::memory::InMemory;

    fn make_store() -> Arc<InMemory> {
        Arc::new(InMemory::new())
    }

    // ── manifest serialization ────────────────────────────────────────────────

    #[test]
    fn manifest_roundtrips_json() {
        let m = BackupManifest {
            format_version: 1,
            created_at: "2026-06-12T00:00:00+00:00".into(),
            migration_version: 16,
            db_sha256: "abc123".into(),
            install_id: "install-uuid".into(),
            item_count: 42,
            object_key: "flexpm/flexpm-backup-test.tar.zst".into(),
            bundle_size_bytes: 1024,
        };
        let json = serde_json::to_string(&m).unwrap();
        let m2: BackupManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(m.migration_version, m2.migration_version);
        assert_eq!(m.db_sha256, m2.db_sha256);
        assert_eq!(m.item_count, m2.item_count);
    }

    // ── build_tar round-trip ──────────────────────────────────────────────────

    #[test]
    fn build_tar_contains_database_and_manifest() {
        let db_bytes = b"SQLite format 3\x00fake_database_content";
        let tar_bytes = build_tar(
            db_bytes,
            "/nonexistent_storage_dir",
            "2026-06-12T00:00:00+00:00",
            16,
            5,
            "test-install-id",
            "deadbeef",
        )
        .unwrap();

        // Decompress would fail here (not compressed yet), check raw tar entries.
        let mut archive = tar::Archive::new(Cursor::new(&tar_bytes));
        let paths: Vec<String> = archive
            .entries()
            .unwrap()
            .map(|e| e.unwrap().path().unwrap().to_string_lossy().to_string())
            .collect();

        assert!(paths.contains(&"database.db".to_string()), "missing database.db in tar");
        assert!(paths.contains(&"manifest.json".to_string()), "missing manifest.json in tar");
    }

    // ── upload → list → download ──────────────────────────────────────────────

    #[tokio::test]
    async fn upload_list_download_roundtrip() {
        let store = make_store();
        let manifest = BackupManifest {
            format_version: 1,
            created_at: "2026-06-12T01:00:00+00:00".into(),
            migration_version: 16,
            db_sha256: "aa".into(),
            install_id: "id".into(),
            item_count: 1,
            object_key: "flexpm/flexpm-backup-test.tar.zst".into(),
            bundle_size_bytes: 4,
        };
        let bundle_data = b"test".to_vec();

        upload(store.as_ref(), &manifest, bundle_data.clone())
            .await
            .unwrap();

        let listed = list(store.as_ref(), "flexpm").await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].object_key, manifest.object_key);
        assert_eq!(listed[0].migration_version, 16);

        let downloaded = download(store.as_ref(), &manifest.object_key)
            .await
            .unwrap();
        assert_eq!(downloaded, bundle_data);
    }

    // ── prune keeps newest N ──────────────────────────────────────────────────

    #[tokio::test]
    async fn prune_keeps_newest() {
        let store = make_store();

        for i in 0..5u32 {
            let m = BackupManifest {
                format_version: 1,
                created_at: format!("2026-06-{:02}T00:00:00+00:00", i + 1),
                migration_version: 16,
                db_sha256: "x".into(),
                install_id: "id".into(),
                item_count: 0,
                object_key: format!("flexpm/flexpm-backup-{i:02}.tar.zst"),
                bundle_size_bytes: 1,
            };
            upload(store.as_ref(), &m, b"x".to_vec()).await.unwrap();
        }

        let deleted = prune(store.as_ref(), "flexpm", 3).await.unwrap();
        assert_eq!(deleted, 2);

        let remaining = list(store.as_ref(), "flexpm").await.unwrap();
        assert_eq!(remaining.len(), 3);
        // Newest 3 should be days 05, 04, 03 (sorted newest-first)
        assert!(remaining[0].created_at.contains("06-05"));
        assert!(remaining[2].created_at.contains("06-03"));
    }

    // ── version-guard rejects newer snapshots ─────────────────────────────────

    #[tokio::test]
    async fn stage_restore_rejects_newer_schema() {
        let manifest = BackupManifest {
            format_version: 1,
            created_at: "2026-06-12T00:00:00+00:00".into(),
            migration_version: 20, // ahead of local
            db_sha256: "x".into(),
            install_id: "id".into(),
            item_count: 0,
            object_key: "key".into(),
            bundle_size_bytes: 0,
        };

        let result = stage_restore(
            b"dummy".to_vec(),
            &manifest,
            16, // local has 16 migrations
            std::path::Path::new("/tmp/flexpm_test.db"),
            "/tmp/flexpm_storage_test",
        )
        .await;

        assert!(
            matches!(result, Err(BackupError::SchemaTooNew { snapshot: 20, local: 16 })),
            "expected SchemaTooNew error, got: {result:?}"
        );
    }

    // ── prune is a no-op when below retention ─────────────────────────────────

    #[tokio::test]
    async fn prune_noop_when_under_retention() {
        let store = make_store();
        let m = BackupManifest {
            format_version: 1,
            created_at: "2026-06-12T00:00:00+00:00".into(),
            migration_version: 16,
            db_sha256: "x".into(),
            install_id: "id".into(),
            item_count: 0,
            object_key: "flexpm/flexpm-backup-only.tar.zst".into(),
            bundle_size_bytes: 1,
        };
        upload(store.as_ref(), &m, b"x".to_vec()).await.unwrap();
        let deleted = prune(store.as_ref(), "flexpm", 10).await.unwrap();
        assert_eq!(deleted, 0);
    }
}
