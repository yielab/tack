use super::*;
use std::fs;
use std::path::Path;

/// Working directory that removes itself when the returned guard drops,
/// so a failed assertion leaves nothing behind either.
fn workdir(tag: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(tag)
        .tempdir()
        .expect("temporary directory")
}

#[test]
fn configured_log_path_creates_dir_and_splits_into_dir_and_name() {
    let dir_guard = workdir("log-target");
    let nested = dir_guard.path().join("logs").join("tack.log");
    assert!(!nested.parent().unwrap().exists());

    let (dir, name) = log_file_target(nested.to_str().unwrap()).expect("a usable target");

    assert_eq!(dir, nested.parent().unwrap());
    assert_eq!(name, "tack.log");
    assert!(
        dir.is_dir(),
        "the log directory must be created, not assumed"
    );
}

#[test]
fn a_bare_file_name_logs_beside_the_working_directory() {
    let (dir, name) = log_file_target("tack.log").expect("a usable target");
    assert_eq!(dir, Path::new("."));
    assert_eq!(name, "tack.log");
}

#[test]
fn a_path_naming_no_file_is_refused_rather_than_guessed() {
    assert!(log_file_target("/").is_none());
    assert!(log_file_target("..").is_none());
}

/// The whole point of the setting: a line logged through the subscriber built
/// from a config carrying a log path has to land in that file. Asserting that a
/// layer was constructed would have passed for the entire time this setting
/// silently did nothing. The subscriber is scoped to this thread: the global
/// slot goes to whichever in-process server test starts first.
#[test]
fn a_configured_log_file_receives_the_lines_that_are_logged() {
    let dir_guard = workdir("log-write");
    let path = dir_guard.path().join("logs").join("tack.log");

    let config = AppConfig {
        log_file: Some(path.to_string_lossy().into_owned()),
        log_level: "info".into(),
        log_json: false,
        ..AppConfig::default()
    };
    tracing::subscriber::with_default(tracing_subscriber_for(&config), || {
        tracing::error!(marker = "written-to-the-file", "log file smoke line");
    });

    let written = fs::read_to_string(&path).expect("the log file must exist");
    assert!(
        written.contains("written-to-the-file"),
        "the configured log file got no line; it holds: {written:?}"
    );
}

/// A line from another Tack crate (the runner's transport) is kept by the default filter: the
/// filter lists the `tack` prefix, not each crate, so a new crate cannot be silent again.
#[test]
fn the_default_filter_keeps_the_runner_crates_lines() {
    let dir_guard = workdir("log-runner-target");
    let path = dir_guard.path().join("tack.log");
    let config = AppConfig {
        log_file: Some(path.to_string_lossy().into_owned()),
        log_level: "info".into(),
        log_json: false,
        ..AppConfig::default()
    };
    tracing::subscriber::with_default(tracing_subscriber_for(&config), || {
        tracing::info!(target: "tack_runner::transport", marker = "from-the-runner", "runner line");
    });
    let written = fs::read_to_string(&path).expect("the log file must exist");
    assert!(
        written.contains("from-the-runner"),
        "the runner's line was filtered out: {written:?}"
    );
}

/// The per-request trace (`tower_http` logs each request and response at `debug`) stays out of an
/// `info` log, where a runner's polls would bury every other line; a failed request is `error` and
/// stays in.
#[test]
fn the_default_filter_keeps_the_per_request_trace_at_the_configured_level() {
    let dir_guard = workdir("log-tower-level");
    let path = dir_guard.path().join("tack.log");
    let config = AppConfig {
        log_file: Some(path.to_string_lossy().into_owned()),
        log_level: "info".into(),
        log_json: false,
        ..AppConfig::default()
    };
    tracing::subscriber::with_default(tracing_subscriber_for(&config), || {
        tracing::debug!(target: "tower_http::trace::on_request", marker = "each-poll", "started");
        tracing::error!(target: "tower_http::trace::on_failure", marker = "a-5xx", "failed");
    });
    let written = fs::read_to_string(&path).expect("the log file must exist");
    assert!(
        !written.contains("each-poll"),
        "a request's debug trace reached an info log: {written:?}"
    );
    assert!(
        written.contains("a-5xx"),
        "a failed request was filtered out: {written:?}"
    );
}

/// The DB swap succeeds but the storage swap fails — the whole
/// operation must roll back so the original DB is intact and bootable.
#[test]
fn restore_swap_rolls_back_when_storage_swap_fails() {
    let dir_guard = workdir("storage-fail");
    let dir = dir_guard.path();
    let db_path = dir.join("tack.db");
    let restore_db = dir.join("tack.db.restore");
    fs::write(&db_path, b"ORIGINAL").unwrap();
    fs::write(&restore_db, b"RESTORED").unwrap();

    // A staging dir exists (so the storage step runs)…
    let storage_restore = dir.join("storage.restore");
    fs::create_dir_all(&storage_restore).unwrap();
    fs::write(storage_restore.join("a.bin"), b"attach").unwrap();

    // …but the live storage path sits under a MISSING parent, so promoting
    // the staging dir into place fails with ENOENT — after the DB swap.
    let storage_dir = dir.join("missing_parent").join("storage");

    let result = apply_restore_swap(&db_path, &restore_db, &storage_dir, &storage_restore, "TS");
    assert!(result.is_err(), "storage promotion should fail");

    // Rolled back: the ORIGINAL DB is back in place and bootable.
    assert_eq!(fs::read(&db_path).unwrap(), b"ORIGINAL");
    // The staged restore file is put back for a future attempt.
    assert_eq!(fs::read(&restore_db).unwrap(), b"RESTORED");
    // No stray .bak left behind for the DB.
    assert!(!Path::new(&format!("{}.bak-TS", db_path.to_string_lossy())).exists());

    fs::remove_dir_all(dir).ok();
}

/// Happy path: DB + storage both swap, and a timestamped .bak is kept.
#[test]
fn restore_swap_promotes_db_and_storage() {
    let dir_guard = workdir("ok");
    let dir = dir_guard.path();
    let db_path = dir.join("tack.db");
    let restore_db = dir.join("tack.db.restore");
    fs::write(&db_path, b"ORIGINAL").unwrap();
    fs::write(&restore_db, b"RESTORED").unwrap();

    let storage_dir = dir.join("storage");
    fs::create_dir_all(&storage_dir).unwrap();
    fs::write(storage_dir.join("old.bin"), b"old").unwrap();
    let storage_restore = dir.join("storage.restore");
    fs::create_dir_all(&storage_restore).unwrap();
    fs::write(storage_restore.join("new.bin"), b"new").unwrap();

    let baks = apply_restore_swap(&db_path, &restore_db, &storage_dir, &storage_restore, "TS")
        .expect("swap should succeed");

    assert_eq!(fs::read(&db_path).unwrap(), b"RESTORED");
    assert!(storage_dir.join("new.bin").exists());
    assert!(!storage_dir.join("old.bin").exists());
    assert!(Path::new(&baks.db_bak).exists());
    assert!(baks.storage_bak.is_some());
    // The staging inputs are consumed.
    assert!(!restore_db.exists());
    assert!(!storage_restore.exists());

    fs::remove_dir_all(dir).ok();
}

/// Stale `-wal`/`-shm` sidecars of the replaced DB are deleted so SQLite
/// cannot replay them onto the freshly restored database.
#[test]
fn restore_swap_deletes_stale_wal_shm() {
    let dir_guard = workdir("wal");
    let dir = dir_guard.path();
    let db_path = dir.join("tack.db");
    let restore_db = dir.join("tack.db.restore");
    fs::write(&db_path, b"ORIGINAL").unwrap();
    fs::write(&restore_db, b"RESTORED").unwrap();
    fs::write(dir.join("tack.db-wal"), b"waljunk").unwrap();
    fs::write(dir.join("tack.db-shm"), b"shmjunk").unwrap();

    // No storage staging dir → storage step is skipped.
    let storage_dir = dir.join("storage");
    let storage_restore = dir.join("storage.restore");

    apply_restore_swap(&db_path, &restore_db, &storage_dir, &storage_restore, "TS").unwrap();

    assert!(
        !dir.join("tack.db-wal").exists(),
        "stale -wal must be deleted"
    );
    assert!(
        !dir.join("tack.db-shm").exists(),
        "stale -shm must be deleted"
    );
    assert_eq!(fs::read(&db_path).unwrap(), b"RESTORED");

    fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn shutdown_waits_for_a_use_case_a_handler_started() {
    let tasks = TaskTracker::new();
    let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = finished.clone();
    tasks.spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    });

    wait_for_tasks(&tasks, Duration::from_secs(5)).await;

    assert!(
        finished.load(std::sync::atomic::Ordering::SeqCst),
        "the use case ran to its end before shutdown went on"
    );
}

#[tokio::test]
async fn shutdown_does_not_wait_forever_for_a_use_case_that_never_ends() {
    let tasks = TaskTracker::new();
    tasks.spawn(std::future::pending::<()>());

    let started = std::time::Instant::now();
    wait_for_tasks(&tasks, Duration::from_millis(100)).await;

    assert!(started.elapsed() < Duration::from_secs(5));
}
