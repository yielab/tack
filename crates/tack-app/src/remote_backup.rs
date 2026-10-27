//! Remote cloud backup — S3-compatible object storage (Cloudflare R2, Backblaze B2, AWS S3,
//! MinIO, or any other S3-compatible endpoint).
//!
//! # Bundle format
//! Each backup is a `tack-backup-<UTC-timestamp>.tar.zst` archive containing:
//! - `database.db`      — VACUUM INTO SQLite snapshot
//! - `attachments/…`    — copy of TACK_STORAGE_DIR (omitted when empty / dir absent)
//! - `manifest.json`    — metadata (format_version, timestamps, migration count, sha256, etc.)
//!
//! A sidecar `<archive>.manifest.json` is stored alongside each bundle so `list()` can
//! return metadata without downloading the full archive.

use std::io::{Cursor, Read as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use object_store::ObjectStore;
// object_store 0.13 moved the put/get/delete convenience methods to this extension trait.
use object_store::ObjectStoreExt;
use object_store::aws::AmazonS3Builder;
use object_store::path::Path as OsPath;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tack_db::queries::backup::item_count;
use tack_db::queries::migrations::applied_migration_count;
use tack_db::repo::backup::scrub_snapshot_secrets;
use tack_db::{Repository, StoreError};
use tack_protocol::Clock;
use thiserror::Error;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::background::{Loops, Start};

// ── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("remote backup is not configured (set TACK_BACKUP_BUCKET, _ACCESS_KEY, _SECRET_KEY)")]
    NotConfigured,
    #[error("backup requires a file-based database (not in-memory)")]
    InMemoryDb,
    #[error("object store error: {0}")]
    ObjectStore(#[from] object_store::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Store(#[from] StoreError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("zstd error: {0}")]
    Zstd(String),
    #[error(
        "restore rejected: snapshot migration_version ({snapshot}) is ahead of the running binary ({local}); upgrade Tack before restoring"
    )]
    SchemaTooNew { snapshot: u32, local: u32 },
    #[error("bundle is corrupt or unrecognised format")]
    CorruptBundle,
    #[error(
        "restore rejected: unsupported bundle format_version {0} (this binary understands version 1)"
    )]
    UnsupportedFormat(u32),
    #[error(
        "restore rejected: database integrity check failed (manifest sha256 does not match the extracted database)"
    )]
    IntegrityMismatch,
    #[error("restore rejected: bundle contains an unsafe path '{0}' (path traversal attempt)")]
    UnsafePath(String),
    #[error(
        "upload rejected: another device uploaded newer work (remote generation {remote_generation} ≥ local {local_generation}) — restore first or force"
    )]
    GenerationConflict {
        local_generation: u64,
        remote_generation: u64,
        /// The remote head manifest that would be clobbered (for the 409 body).
        remote: Box<BackupManifest>,
    },
    #[error(
        "restore rejected: local generation ({local_generation}) is ahead of the snapshot ({snapshot_generation}); this device has newer work — force to overwrite"
    )]
    RestoreWouldLoseWork {
        local_generation: u64,
        snapshot_generation: u64,
    },
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
    /// Monotonic sync generation this snapshot represents. Bumped once per
    /// successful backup; used for cross-device conflict detection.
    /// Defaults to 0 for older sidecars that predate the field.
    #[serde(default)]
    pub generation: u64,
}

// ── Settings ──────────────────────────────────────────────────────────────────

/// Where a backup goes and what it covers: the object-store destination, how many bundles to
/// keep, and the local files a bundle is made from.
#[derive(Debug, Clone)]
pub struct BackupConfig {
    pub endpoint: Option<String>,
    pub bucket: Option<String>,
    pub region: String,
    pub access_key: Option<String>,
    /// Never logged.
    pub secret_key: Option<String>,
    pub prefix: String,
    /// How many bundles stay in the bucket after each upload.
    pub retention: usize,
    /// The database file, `None` for an in-memory database (which cannot be backed up).
    pub db_file: Option<PathBuf>,
    /// The attachments directory a bundle carries along with the database.
    pub storage_dir: String,
}

impl BackupConfig {
    /// True when the three fields a destination cannot do without are set.
    pub fn enabled(&self) -> bool {
        self.bucket.is_some() && self.access_key.is_some() && self.secret_key.is_some()
    }
}

/// The destination saved from the UI (`app_meta`, key `backup_config`). Every field is
/// optional so a partially configured destination round-trips.
#[derive(Debug, Default, Deserialize)]
struct SavedDestination {
    #[serde(default)]
    endpoint: Option<String>,
    #[serde(default)]
    bucket: Option<String>,
    #[serde(default)]
    region: Option<String>,
    #[serde(default)]
    access_key: Option<String>,
    #[serde(default)]
    secret_key: Option<String>,
    #[serde(default)]
    prefix: Option<String>,
    #[serde(default)]
    retention: Option<usize>,
}

const SAVED_DESTINATION_KEY: &str = "backup_config";

/// The settings a backup should actually use: `base` (what the environment configured) with
/// any destination saved from the UI laid over it. A blank saved string leaves the base value
/// alone; an unreadable saved value counts as none.
pub async fn effective_config(repo: &Repository, base: &BackupConfig) -> BackupConfig {
    let saved: SavedDestination =
        tack_db::queries::app_meta::app_meta_value(repo, SAVED_DESTINATION_KEY)
            .await
            .ok()
            .flatten()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();

    let mut cfg = base.clone();
    let nonempty = |o: Option<String>| o.filter(|v| !v.trim().is_empty());

    if let Some(v) = nonempty(saved.endpoint) {
        cfg.endpoint = Some(v);
    }
    if let Some(v) = nonempty(saved.bucket) {
        cfg.bucket = Some(v);
    }
    if let Some(v) = nonempty(saved.region) {
        cfg.region = v;
    }
    if let Some(v) = nonempty(saved.access_key) {
        cfg.access_key = Some(v);
    }
    if let Some(v) = nonempty(saved.secret_key) {
        cfg.secret_key = Some(v);
    }
    if let Some(v) = nonempty(saved.prefix) {
        cfg.prefix = v;
    }
    if let Some(rt) = saved.retention {
        cfg.retention = rt;
    }
    cfg
}

// ── Store construction ────────────────────────────────────────────────────────

/// Build an S3-compatible `ObjectStore` from the backup settings.
pub fn store_from_config(cfg: &BackupConfig) -> Result<Arc<dyn ObjectStore>, BackupError> {
    if !cfg.enabled() {
        return Err(BackupError::NotConfigured);
    }

    let bucket = cfg.bucket.as_deref().unwrap();
    let access_key = cfg.access_key.as_deref().unwrap();
    let secret_key = cfg.secret_key.as_deref().unwrap();

    let mut builder = AmazonS3Builder::new()
        .with_bucket_name(bucket)
        .with_region(&cfg.region)
        .with_access_key_id(access_key)
        .with_secret_access_key(secret_key);

    if let Some(endpoint) = &cfg.endpoint {
        builder = builder.with_endpoint(endpoint);
        // Allow plain HTTP for local MinIO / test setups
        if endpoint.starts_with("http://") {
            builder = builder.with_allow_http(true);
        }
    }

    let store = builder.build()?;
    Ok(Arc::new(store))
}

/// The remote head: the manifest with the highest generation (ties broken by
/// newest `created_at`). `None` when the bucket has no backups yet.
pub async fn remote_head(
    store: &dyn ObjectStore,
    prefix: &str,
) -> Result<Option<BackupManifest>, BackupError> {
    let manifests = list(store, prefix).await?;
    Ok(manifests.into_iter().max_by(|a, b| {
        a.generation
            .cmp(&b.generation)
            .then(a.created_at.cmp(&b.created_at))
    }))
}

/// Conflict guard for uploads. Returns `Some(remote_head)` when uploading at
/// `prospective_generation` would clobber newer remote work from *another*
/// install — i.e. the remote head's generation is `>= prospective_generation`
/// and it belongs to a different install. `None` means it is safe to upload.
pub async fn upload_conflict(
    store: &dyn ObjectStore,
    prefix: &str,
    prospective_generation: u64,
    local_install: &str,
) -> Result<Option<BackupManifest>, BackupError> {
    let head = remote_head(store, prefix).await?;
    Ok(head.filter(|h| h.generation >= prospective_generation && h.install_id != local_install))
}

/// Whether restoring a snapshot at `snapshot_generation` onto a device at
/// `local_generation` would discard newer local work. `force` overrides it.
/// (28.2, restore direction.)
pub fn restore_conflicts(local_generation: u64, snapshot_generation: u64, force: bool) -> bool {
    !force && local_generation > snapshot_generation
}

/// Create a VACUUM INTO snapshot and return the raw bytes of the DB file.
async fn snapshot_db(repo: &Repository, db_path: &Path) -> Result<Vec<u8>, BackupError> {
    // A VACUUM INTO target the caller reads back and removes — not a test
    // artifact, so no `tempfile` guard.
    #[allow(clippy::disallowed_methods)]
    let temp = std::env::temp_dir().join(format!("tack-snap-{}.db", Uuid::new_v4()));

    repo.snapshot_into(&temp).await?;

    // From here the snapshot file exists and still holds every secret the live
    // database does, so no path may return without removing it: a `?` between
    // here and the removal would leave an unscrubbed copy of the whole
    // database in a shared temporary directory.
    let scrubbed: Result<Vec<u8>, BackupError> = async {
        scrub_snapshot_secrets(&temp).await?;
        Ok(tokio::fs::read(&temp).await?)
    }
    .await;
    let _ = tokio::fs::remove_file(&temp).await;

    let bytes = scrubbed?;
    debug!(db = %db_path.display(), bytes = bytes.len(), "DB snapshot complete");
    Ok(bytes)
}

// ── Bundle creation ───────────────────────────────────────────────────────────

/// Build the full `.tar.zst` bundle in memory and return it together with its manifest.
pub async fn create_bundle(
    repo: &Repository,
    clock: &dyn Clock,
    cfg: &BackupConfig,
) -> Result<(Vec<u8>, BackupManifest), BackupError> {
    let db_path = cfg.db_file.as_deref().ok_or(BackupError::InMemoryDb)?;

    let db_bytes = snapshot_db(repo, db_path).await?;
    let db_sha256 = hex::encode(Sha256::digest(&db_bytes));

    let mig_version = applied_migration_count(repo).await?;
    let items = item_count(repo).await?;
    let install = repo.install_id().await?;
    // The generation is expected to already be bumped/persisted by the caller
    // (so the DB snapshot above carries the same value the manifest records).
    let gen_val = repo.backup_generation().await?;
    let created_at = clock.now().to_rfc3339();

    // Build tar in memory, then compress.
    let tar_bytes = build_tar(
        &db_bytes,
        &cfg.storage_dir,
        &created_at,
        mig_version,
        items,
        &install,
        &db_sha256,
        gen_val,
    )?;

    // Bundles are not encrypted. Optional symmetric encryption would wrap
    // `tar_bytes` here, before or after zstd, from an in-memory or env
    // passphrase; it is left out to keep the binary-size budget and avoid a
    // crypto dependency. Until it exists, the bucket's own access control and
    // server-side encryption are the only protection a snapshot has.

    let compressed = zstd::encode_all(Cursor::new(&tar_bytes), 3)
        .map_err(|e| BackupError::Zstd(e.to_string()))?;

    let ts = created_at.replace(':', "-").replace('+', "Z");
    let object_key = format!("{}/tack-backup-{}.tar.zst", cfg.prefix, ts);

    let manifest = BackupManifest {
        format_version: 1,
        created_at,
        migration_version: mig_version,
        db_sha256,
        install_id: install,
        item_count: items,
        object_key,
        bundle_size_bytes: compressed.len() as u64,
        generation: gen_val,
    };

    Ok((compressed, manifest))
}

#[allow(clippy::too_many_arguments)]
fn build_tar(
    db_bytes: &[u8],
    storage_dir: &str,
    created_at: &str,
    mig_version: u32,
    items: u64,
    install: &str,
    db_sha256: &str,
    generation: u64,
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
        "generation": generation,
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

/// Bundles larger than this are streamed with `put_multipart` instead of a
/// single buffered PUT, so the whole compressed archive need not be re-buffered
/// as one request body inside the object-store client.
const MULTIPART_THRESHOLD: usize = 32 * 1024 * 1024;
/// Multipart chunk size (8 MiB — comfortably above S3's 5 MiB minimum part).
const MULTIPART_PART_SIZE: usize = 8 * 1024 * 1024;

/// Upload a bundle + sidecar manifest to the object store.
///
/// The bundle goes up first; a missing sidecar afterwards leaves an *orphan*
/// bundle, which [`prune`] reconciles (deletes) on its next run so a failed
/// sidecar PUT can never leak an invisible, unprunable object.
pub async fn upload(
    store: &dyn ObjectStore,
    manifest: &BackupManifest,
    bundle: Vec<u8>,
) -> Result<(), BackupError> {
    let bundle_key = OsPath::from(manifest.object_key.clone());
    let sidecar_key = OsPath::from(format!("{}.manifest.json", manifest.object_key));

    if bundle.len() > MULTIPART_THRESHOLD {
        let mut upload = store.put_multipart(&bundle_key).await?;
        for chunk in bundle.chunks(MULTIPART_PART_SIZE) {
            let payload = object_store::PutPayload::from(chunk.to_vec());
            upload.put_part(payload).await?;
        }
        upload.complete().await?;
        info!(key = %manifest.object_key, bytes = bundle.len(), "Uploaded backup bundle (multipart)");
    } else {
        let bundle_payload = object_store::PutPayload::from(bundle);
        store.put(&bundle_key, bundle_payload).await?;
        info!(key = %manifest.object_key, bytes = manifest.bundle_size_bytes, "Uploaded backup bundle");
    }

    let sidecar_bytes = serde_json::to_vec(manifest)?;
    let sidecar_payload = object_store::PutPayload::from(sidecar_bytes);
    store.put(&sidecar_key, sidecar_payload).await?;
    debug!(key = %sidecar_key, "Uploaded sidecar manifest");

    Ok(())
}

/// Full conflict-safe backup: read the local generation, reject if the remote
/// head is newer work from another install (unless `force`), bump + persist the
/// generation, snapshot, upload, and prune. On any failure after the bump the
/// generation is rolled back so a failed attempt never silently advances it.
///
/// Returns [`BackupError::GenerationConflict`] when another device is ahead.
pub async fn perform_backup(
    repo: &Repository,
    clock: &dyn Clock,
    cfg: &BackupConfig,
    store: &dyn ObjectStore,
    force: bool,
) -> Result<BackupManifest, BackupError> {
    let local_gen = repo.backup_generation().await?;
    let prospective = local_gen + 1;
    let my_install = repo.install_id().await?;

    if !force
        && let Some(remote) = upload_conflict(store, &cfg.prefix, prospective, &my_install).await?
    {
        return Err(BackupError::GenerationConflict {
            local_generation: local_gen,
            remote_generation: remote.generation,
            remote: Box::new(remote),
        });
    }

    // Persist the bump BEFORE snapshotting so the DB snapshot's app_meta carries
    // the same generation the manifest records (restores adopt it).
    repo.set_backup_generation(prospective).await?;

    let outcome = async {
        let (bundle, manifest) = create_bundle(repo, clock, cfg).await?;
        upload(store, &manifest, bundle).await?;
        prune(store, &cfg.prefix, cfg.retention).await?;
        Ok::<_, BackupError>(manifest)
    }
    .await;

    match outcome {
        Ok(m) => Ok(m),
        Err(e) => {
            let _ = repo.set_backup_generation(local_gen).await;
            Err(e)
        }
    }
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

/// Delete oldest backups, keeping the newest `keep` bundles, and reconcile any
/// orphaned bundles (a `.tar.zst` with no sidecar manifest — the fingerprint of
/// a backup whose sidecar PUT failed). Returns number of bundles deleted.
pub async fn prune(
    store: &dyn ObjectStore,
    prefix: &str,
    keep: usize,
) -> Result<usize, BackupError> {
    let mut manifests = list(store, prefix).await?;
    let mut deleted = 0;

    // ── Retention: `list` returns newest-first; delete everything past `keep`.
    if manifests.len() > keep {
        let to_delete = manifests.split_off(keep);
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
    }

    // ── Reconcile orphans: any bundle object without a matching sidecar.
    let orphans = orphan_bundles(store, prefix).await?;
    for key in orphans {
        if let Err(e) = store.delete(&OsPath::from(key.clone())).await {
            warn!(key = %key, error = %e, "Failed to delete orphaned bundle");
        } else {
            debug!(key = %key, "Reconciled orphaned bundle (no sidecar)");
            deleted += 1;
        }
    }

    info!(deleted, kept = keep, "Pruned old remote backups");
    Ok(deleted)
}

/// List bundle object keys (`*.tar.zst`) that have no `*.tar.zst.manifest.json`
/// sidecar — the leftover of a bundle upload whose sidecar PUT never landed.
async fn orphan_bundles(store: &dyn ObjectStore, prefix: &str) -> Result<Vec<String>, BackupError> {
    use futures::StreamExt;
    use std::collections::HashSet;

    let prefix_path = OsPath::from(format!("{}/", prefix));
    let mut stream = store.list(Some(&prefix_path));

    let mut bundles: Vec<String> = Vec::new();
    let mut sidecar_targets: HashSet<String> = HashSet::new();

    while let Some(item) = stream.next().await {
        let key = item?.location.to_string();
        if let Some(bundle) = key.strip_suffix(".manifest.json") {
            sidecar_targets.insert(bundle.to_string());
        } else if key.ends_with(".tar.zst") {
            bundles.push(key);
        }
    }

    Ok(bundles
        .into_iter()
        .filter(|b| !sidecar_targets.contains(b))
        .collect())
}

// ── Restore helpers ───────────────────────────────────────────────────────────

/// A file extracted from a bundle, ready to be written to disk.
#[derive(Debug)]
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
    // Reject unknown bundle formats before touching anything.
    if manifest.format_version != 1 {
        return Err(BackupError::UnsupportedFormat(manifest.format_version));
    }

    if manifest.migration_version > local_migration_version {
        return Err(BackupError::SchemaTooNew {
            snapshot: manifest.migration_version,
            local: local_migration_version,
        });
    }

    let restore_db_path = PathBuf::from(format!("{}.restore", db_path.to_string_lossy()));
    let restore_storage = format!("{}.restore", storage_dir);
    let restore_storage_path = PathBuf::from(&restore_storage);

    // Clear any leftovers from a previous (aborted) restore attempt so a new
    // bundle never merges into a stale staging tree.
    let _ = tokio::fs::remove_file(&restore_db_path).await;
    let _ = tokio::fs::remove_dir_all(&restore_storage_path).await;

    // All sync work (decompression + tar parsing) before any await points.
    let restore_storage_for_parse = restore_storage_path.clone();
    let (db_bytes, attachments) = tokio::task::spawn_blocking(move || {
        parse_bundle(&bundle_bytes, &restore_storage_for_parse)
    })
    .await
    .map_err(|e| BackupError::Io(std::io::Error::other(e.to_string())))??;

    // Integrity: the extracted database must hash to the manifest's db_sha256.
    // Guards against silent corruption/bit-rot and tampered bundles.
    let actual_sha = hex::encode(Sha256::digest(&db_bytes));
    if actual_sha != manifest.db_sha256 {
        return Err(BackupError::IntegrityMismatch);
    }

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

/// Verify a bundle without staging anything: checks `format_version`, that the
/// snapshot schema is not newer than the running binary, and that the extracted
/// database hashes to the manifest's `db_sha256`. Used by the "Verify" preview
/// so a restore can be validated before touching the live DB.
pub async fn verify_bundle(
    bundle_bytes: Vec<u8>,
    manifest: &BackupManifest,
    local_migration_version: u32,
) -> Result<(), BackupError> {
    if manifest.format_version != 1 {
        return Err(BackupError::UnsupportedFormat(manifest.format_version));
    }
    if manifest.migration_version > local_migration_version {
        return Err(BackupError::SchemaTooNew {
            snapshot: manifest.migration_version,
            local: local_migration_version,
        });
    }

    let expected_sha = manifest.db_sha256.clone();
    // Parse in a blocking task (decompress + tar walk). The staging path is a
    // throwaway — verify never writes.
    let db_bytes = tokio::task::spawn_blocking(move || {
        parse_bundle(&bundle_bytes, Path::new("/nonexistent-verify-staging"))
            .map(|(db, _attachments)| db)
    })
    .await
    .map_err(|e| BackupError::Io(std::io::Error::other(e.to_string())))??;

    let actual_sha = hex::encode(Sha256::digest(&db_bytes));
    if actual_sha != expected_sha {
        return Err(BackupError::IntegrityMismatch);
    }
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
            // Refuse path-traversal / absolute entries so a crafted bundle can
            // never escape the staging dir (mirrors tar::Entry::unpack_in).
            use std::path::Component;
            let unsafe_component = rel.components().any(|c| {
                matches!(
                    c,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            });
            if unsafe_component {
                return Err(BackupError::UnsafePath(name.into_owned()));
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

// ── The scheduled backup ──────────────────────────────────────────────────────

/// What one scheduled pass did.
#[derive(Debug)]
pub enum Outcome {
    /// No destination is configured yet (a UI-only configuration may not be saved).
    NotConfigured,
    /// A bundle went up.
    Uploaded(Box<BackupManifest>),
    /// Another device holds newer work; nothing was uploaded.
    AnotherDeviceAhead { remote_generation: u64 },
}

/// Why a scheduled pass failed, and at which step.
#[derive(Debug, Error)]
pub enum TickError {
    #[error("{0}")]
    StoreInit(BackupError),
    #[error("{0}")]
    Backup(BackupError),
}

/// One scheduled remote backup. Reads the *effective* settings each pass so UI-saved ones
/// (bucket/prefix/creds/retention) are honored, and runs the conflict-safe upload path: on a
/// cross-device conflict it skips rather than clobbering another device's newer work, and
/// never forces.
pub async fn tick(
    repo: &Repository,
    clock: &dyn Clock,
    base: &BackupConfig,
) -> Result<Outcome, TickError> {
    let cfg = effective_config(repo, base).await;
    if !cfg.enabled() {
        return Ok(Outcome::NotConfigured);
    }

    let store = store_from_config(&cfg).map_err(TickError::StoreInit)?;

    match perform_backup(repo, clock, &cfg, store.as_ref(), false).await {
        Ok(manifest) => Ok(Outcome::Uploaded(Box::new(manifest))),
        Err(BackupError::GenerationConflict {
            remote_generation, ..
        }) => Ok(Outcome::AnotherDeviceAhead { remote_generation }),
        Err(e) => Err(TickError::Backup(e)),
    }
}

/// The loop `tack serve` starts: run [`tick`] every `interval` and log what it did. The first
/// pass comes one `interval` after the start, not at it. `interval` must not be zero.
pub fn spawn(
    loops: &mut Loops,
    repo: Repository,
    clock: Arc<dyn Clock>,
    base: BackupConfig,
    interval: std::time::Duration,
) {
    let base = Arc::new(base);
    loops.spawn_tick("remote_backup", Start::AfterInterval, interval, move || {
        let (repo, clock, base) = (repo.clone(), clock.clone(), base.clone());
        async move {
            match tick(&repo, clock.as_ref(), &base).await {
                Ok(Outcome::NotConfigured) => {
                    // A UI-only config may not be set yet; nothing to do this tick.
                    debug!("Scheduled backup skipped: remote backup not configured");
                }
                Ok(Outcome::Uploaded(manifest)) => {
                    info!(key = %manifest.object_key, "Scheduled backup complete");
                }
                Ok(Outcome::AnotherDeviceAhead { remote_generation }) => {
                    warn!(
                        remote_generation,
                        "Scheduled backup skipped: another device has newer work (restore to resolve)"
                    );
                }
                Err(TickError::StoreInit(e)) => {
                    warn!(error = %e, "Scheduled backup store init failed");
                }
                Err(TickError::Backup(e)) => warn!(error = %e, "Scheduled backup failed"),
            }
        }
    });
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "remote_backup/tests.rs"]
mod tests;
