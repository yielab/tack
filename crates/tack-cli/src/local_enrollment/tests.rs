use super::*;

/// The two branches `stored_session_orphaned` settles without ever reaching the database:
/// nothing on disk to name a runner id for. Neither is "the wrong database", so both report
/// `false` rather than guessing, and an unreachable `database_url` proves neither tries to open
/// it.
#[tokio::test]
async fn stored_session_orphaned_is_false_with_nothing_on_disk() {
    let guard = tempfile::tempdir().expect("temporary directory");
    let dir = guard.path();

    assert!(
        !stored_session_orphaned(dir, "not-a-real-database-url")
            .await
            .expect("no session on disk must never fail, let alone reach a database")
    );
}

#[tokio::test]
async fn stored_session_orphaned_is_false_for_an_unparseable_session() {
    let guard = tempfile::tempdir().expect("temporary directory");
    let dir = guard.path();
    std::fs::write(dir.join("session.json"), b"not json").expect("write malformed session");

    assert!(
        !stored_session_orphaned(dir, "not-a-real-database-url")
            .await
            .expect("a session this function cannot even identify a runner id for must never fail")
    );
}
