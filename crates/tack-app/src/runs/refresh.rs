use chrono::{DateTime, Duration, Utc};
use tack_db::repo::execution::{CredentialRotationResult, RunnerCapabilityUpdate};
use tack_db::{Repository, StoreError};
use tack_protocol::Clock;

use super::enroll::CREDENTIAL_LIFETIME_DAYS;
use crate::AppError;

/// What a runner reports when it refreshes, already validated and encoded.
pub struct Refresh<'a> {
    pub runner_id: &'a str,
    /// Hash of the credential the request was authenticated with.
    pub presented_credential_hash: &'a str,
    pub runner_version: &'a str,
    pub labels_json: &'a str,
    pub capability_snapshot_json: &'a str,
    /// Hash of a fresh credential, when the runner asked to rotate.
    pub new_credential_hash: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refreshed {
    /// The capabilities were stored; `credential_expires_at` is the expiry of the credential
    /// now in force (the new one after a rotation).
    Accepted {
        credential_expires_at: DateTime<Utc>,
        accepted_at: DateTime<Utc>,
    },
    /// Another rotation replaced the credential before this one committed.
    CredentialChanged,
    /// The runner is no longer active.
    Revoked,
}

/// Stores a runner's reported capabilities and, when asked, rotates its credential first. The
/// rotation is a compare-and-set on the credential the request authenticated with, so two
/// concurrent rotations cannot both win. A value another runner already holds is
/// `AppError::Store(StoreError::Conflict)`.
pub async fn refresh(
    repo: &Repository,
    clock: &dyn Clock,
    refresh: Refresh<'_>,
) -> Result<Refreshed, AppError> {
    let accepted_at = clock.now();
    let rotated_expiry = match refresh.new_credential_hash {
        Some(new_hash) => {
            let expires = accepted_at + Duration::days(CREDENTIAL_LIFETIME_DAYS);
            let rotation = repo
                .rotate_runner_credential(
                    refresh.runner_id,
                    refresh.presented_credential_hash,
                    new_hash,
                    expires,
                    clock,
                )
                .await
                .map_err(StoreError::from)?;
            match rotation {
                CredentialRotationResult::Rotated(_) => Some(expires),
                CredentialRotationResult::HashMismatch => return Ok(Refreshed::CredentialChanged),
            }
        }
        None => None,
    };
    let update = repo
        .update_runner_capabilities(
            refresh.runner_id,
            refresh.runner_version,
            refresh.labels_json,
            refresh.capability_snapshot_json,
            clock,
        )
        .await?;
    let RunnerCapabilityUpdate::Updated {
        credential_expires_at,
    } = update
    else {
        return Ok(Refreshed::Revoked);
    };
    let credential_expires_at = rotated_expiry
        .or(credential_expires_at)
        .unwrap_or(accepted_at);
    Ok(Refreshed::Accepted {
        credential_expires_at,
        accepted_at,
    })
}
