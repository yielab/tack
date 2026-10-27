use chrono::{DateTime, Duration, Utc};
use tack_db::Repository;
use tack_db::repo::execution::RedeemEnrollmentResult;

use crate::AppError;

/// How long an issued or rotated runner credential stays valid before the runner must call
/// refresh with a rotation.
pub const CREDENTIAL_LIFETIME_DAYS: i64 = 90;

/// What a runner reports about itself when it enrolls, already validated and encoded.
pub struct Enrollment<'a> {
    pub token_hash: &'a str,
    pub credential_hash: &'a str,
    pub runner_version: &'a str,
    pub runner_name: &'a str,
    pub labels_json: &'a str,
    pub total_capacity: i64,
    pub available_capacity: i64,
    pub capability_snapshot_json: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Enrolled {
    /// The token was good and is now spent; the runner holds the credential until
    /// `credential_expires_at`.
    Runner {
        runner_id: String,
        credential_expires_at: DateTime<Utc>,
        server_time: DateTime<Utc>,
    },
    /// The token is unknown, expired, revoked or already used.
    InvalidToken,
}

/// Spends a single-use enrollment token and activates the runner it was issued for, giving it
/// the credential whose hash is passed in. A name or other unique value already held by
/// another runner is `AppError::Store(StoreError::Conflict)`.
pub async fn enroll(
    repo: &Repository,
    now: DateTime<Utc>,
    enrollment: Enrollment<'_>,
) -> Result<Enrolled, AppError> {
    let server_time = now;
    let credential_expires_at = server_time + Duration::days(CREDENTIAL_LIFETIME_DAYS);
    let redeemed = repo
        .redeem_enrollment_token(
            enrollment.token_hash,
            enrollment.credential_hash,
            credential_expires_at,
            enrollment.runner_version,
            enrollment.runner_name,
            enrollment.labels_json,
            enrollment.total_capacity,
            enrollment.available_capacity,
            enrollment.capability_snapshot_json,
            1,
            now,
        )
        .await?;
    Ok(match redeemed {
        RedeemEnrollmentResult::Redeemed(runner_id) => Enrolled::Runner {
            runner_id,
            credential_expires_at,
            server_time,
        },
        RedeemEnrollmentResult::InvalidOrExpired => Enrolled::InvalidToken,
    })
}
