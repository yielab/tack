//! Self-provisions a one-time enrollment token for `tack serve --with-runner`'s
//! zero-touch local case (ADR 0058). Only mints the token; redemption still
//! goes over real runner-v1 HTTP.

use std::path::Path;

use tack_api::handlers::runner_admin;
use tack_runner::EnrollmentCredential;

/// `tack_runner::transport`'s private session filename, duplicated here.
const SESSION_FILE_NAME: &str = "session.json";

/// Whether a durable session from a prior enrollment already exists on disk.
pub fn has_stored_session(state_dir: &Path) -> bool {
    state_dir.join(SESSION_FILE_NAME).is_file()
}

/// Whether the on-disk session names a runner id with no row in `database_url`
/// — its database was replaced, not merely unreachable; unidentifiable → `false`.
pub async fn stored_session_orphaned(state_dir: &Path, database_url: &str) -> anyhow::Result<bool> {
    let Some(runner_id) = tack_runner::client::persisted_session_runner_id(state_dir) else {
        return Ok(false);
    };
    let exists = runner_admin::local_runner_id_exists(database_url, &runner_id)
        .await
        .map_err(|err| anyhow::anyhow!("checking the stored session's runner id failed: {err}"))?;
    Ok(!exists)
}

/// Placeholder for `enrollment_credential` when a stored session already
/// exists, since `build_runtime` requires *some* credential up front. Never
/// transmitted: `establish_session` tries the stored session's `refresh`
/// first and only reads this on rejection, where failing loudly is correct.
pub(crate) fn stored_session_placeholder() -> EnrollmentCredential {
    EnrollmentCredential::new("stored-session-on-disk-no-token-needed")
}

/// Self-provisions a runner and returns its one-time token, redacted and
/// never logged; the caller redeems it over loopback HTTP like any runner.
pub async fn self_provision(database_url: &str) -> anyhow::Result<EnrollmentCredential> {
    let response = runner_admin::provision_local_runner(database_url)
        .await
        .map_err(|err| anyhow::anyhow!("self-provisioning a local runner failed: {err}"))?;
    tracing::info!(
        runner_id = %response.runner_id,
        token_id = %response.token_id,
        expires_at = %response.expires_at,
        "self-provisioned a local runner for the embedded runner to redeem"
    );
    Ok(EnrollmentCredential::new(response.enrollment_token))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_stored_session_is_false_for_an_empty_directory() {
        let guard = tempfile::tempdir().expect("temporary directory");
        let dir = guard.path();

        assert!(!has_stored_session(dir));

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn has_stored_session_is_true_once_session_json_exists() {
        let guard = tempfile::tempdir().expect("temporary directory");
        let dir = guard.path();
        std::fs::write(dir.join("session.json"), "{}").unwrap();

        assert!(has_stored_session(dir));

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn has_stored_session_is_false_when_state_dir_is_absent() {
        let guard = tempfile::tempdir().expect("temporary directory");
        // A path under the guard that was never created: `has_stored_session`
        // must answer for a state directory that does not exist at all.
        let dir = guard.path().join("absent");

        assert!(!has_stored_session(&dir));
    }

    /// The two branches `stored_session_orphaned` settles without ever
    /// reaching the database at all: nothing on disk to name a runner id for
    /// in the first place. Neither is "the wrong database" — there is
    /// nothing here to positively pin on one — so both report `false`
    /// rather than guessing, and a real (unreachable) `database_url` proves
    /// neither branch tries to open it.
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
        std::fs::write(dir.join(SESSION_FILE_NAME), b"not json").expect("write malformed session");

        assert!(
            !stored_session_orphaned(dir, "not-a-real-database-url")
                .await
                .expect(
                    "a session this function cannot even identify a runner id for must never fail"
                )
        );
    }
}
