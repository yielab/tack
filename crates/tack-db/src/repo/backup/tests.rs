use super::*;

use sqlx::sqlite::SqliteJournalMode;

/// A WAL-mode database with `rows` migration rows, all of them still only in the `-wal`
/// (auto-checkpoint off), and a copy of both files in `dir`; the live connection stays open so
/// the copy is taken with the `-wal` unmerged.
async fn db_with_rows_only_in_the_wal(dir: &Path, rows: i64) -> std::path::PathBuf {
    let source = dir.join("source.db");
    let mut conn = SqliteConnectOptions::new()
        .filename(&source)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .connect()
        .await
        .unwrap();
    sqlx::query("PRAGMA wal_autocheckpoint = 0")
        .execute(&mut conn)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE _migrations (version INTEGER PRIMARY KEY)")
        .execute(&mut conn)
        .await
        .unwrap();
    // Fold the table into the main file so only the rows below live in the `-wal`.
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&mut conn)
        .await
        .unwrap();
    for version in 0..rows {
        sqlx::query("INSERT INTO _migrations (version) VALUES (?)")
            .bind(version)
            .execute(&mut conn)
            .await
            .unwrap();
    }

    let copy = dir.join("uploaded.db");
    std::fs::copy(&source, &copy).unwrap();
    std::fs::copy(dir.join("source.db-wal"), dir.join("uploaded.db-wal")).unwrap();
    conn.close().await.ok();
    copy
}

/// An uploaded file is read as the main file alone: rows that exist only in a `-wal` planted
/// beside it are not read, so a crafted `-wal` cannot reach SQLite's WAL reader.
#[tokio::test]
async fn a_wal_beside_an_uploaded_file_is_not_read() {
    let dir = tempfile::tempdir().unwrap();
    let uploaded = db_with_rows_only_in_the_wal(dir.path(), 3).await;
    assert!(
        dir.path().join("uploaded.db-wal").metadata().unwrap().len() > 0,
        "the fixture must leave rows in the -wal"
    );

    let count = migration_count_of_file(&uploaded).await.unwrap();

    assert_eq!(count, 0, "the planted -wal was read");
}

/// A `-wal` that is not a WAL at all (the shape a crafted one starts as) neither fails the read
/// nor changes its answer.
#[tokio::test]
async fn a_garbage_wal_beside_an_uploaded_file_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let uploaded = db_with_rows_only_in_the_wal(dir.path(), 3).await;
    std::fs::write(dir.path().join("uploaded.db-wal"), vec![0xAB; 4096]).unwrap();

    let count = migration_count_of_file(&uploaded).await.unwrap();

    assert_eq!(count, 0);
}
