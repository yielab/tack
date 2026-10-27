//! Self-provisions a one-time enrollment token for `tack serve --with-runner`'s zero-touch local
//! case. Only mints the token; redemption still goes over real runner-v1 HTTP.

use std::path::Path;

use tack_api::handlers::runner_admin;
use tack_app::local_runner::host_name;
use tack_runner::EnrollmentCredential;

/// Whether the on-disk session names a runner id with no row in `database_url`: its database was
/// replaced, not merely unreachable. Unidentifiable is `false`.
pub async fn stored_session_orphaned(state_dir: &Path, database_url: &str) -> anyhow::Result<bool> {
    let Some(runner_id) = tack_runner::client::persisted_session_runner_id(state_dir) else {
        return Ok(false);
    };
    let exists = runner_admin::local_runner_id_exists(database_url, &runner_id)
        .await
        .map_err(|err| anyhow::anyhow!("checking the stored session's runner id failed: {err}"))?;
    Ok(!exists)
}

/// Names the runner the stored session belongs to after this computer, when it still has the
/// generated name an earlier version gave it; a runner a person named is left alone. Best
/// effort: a failure is logged, never a reason not to start.
pub async fn name_stored_runner_after_this_computer(state_dir: &Path, database_url: &str) {
    let Some(runner_id) = tack_runner::client::persisted_session_runner_id(state_dir) else {
        return;
    };
    match runner_admin::name_local_runner_after_host(database_url, &runner_id, &host_name()).await {
        Ok(true) => tracing::info!(%runner_id, "named the embedded runner after this computer"),
        Ok(false) => {}
        Err(error) => {
            tracing::warn!(%error, "could not name the embedded runner after this computer");
        }
    }
}

/// Self-provisions a runner and returns its one-time token, redacted and never logged; the caller
/// redeems it over loopback HTTP like any runner.
pub async fn self_provision(database_url: &str) -> anyhow::Result<EnrollmentCredential> {
    let response = runner_admin::provision_local_runner(database_url, &host_name())
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
#[path = "local_enrollment/tests.rs"]
mod tests;
