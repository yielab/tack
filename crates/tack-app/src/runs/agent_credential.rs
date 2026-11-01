//! An agent's credential (ADR 0034, decision 2): issued with scopes, its secret shown once and
//! kept only as a hash, rotated, revoked, and recognized on a request. It is the pattern of a
//! runner's credential (`enroll.rs`): ninety days, and no way to read the secret back.

use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use tack_core::principal::{
    AGENT_CREDENTIAL_LIFETIME_DAYS, AgentCredential, CredentialState, Scope, credential_problem,
};
use tack_db::Repository;
use uuid::Uuid;

use crate::AppError;

/// What every agent secret starts with, so a bearer token tells its own kind.
pub const SECRET_PREFIX: &str = "tack_agent_";

/// `last_used_at` is written at most this often, so a busy agent does not write on every call.
const TOUCH_EVERY_SECONDS: i64 = 60;

/// A credential and the one time its secret is in hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issued {
    pub credential: AgentCredential,
    /// Shown once; Tack keeps only its hash.
    pub secret: String,
}

/// The SHA-256 hex digest the store keeps instead of the secret.
pub fn hash_secret(secret: &str) -> String {
    hex::encode(Sha256::digest(secret.as_bytes()))
}

/// `tack_agent_` and 64 random hex digits.
fn new_secret() -> String {
    format!(
        "{SECRET_PREFIX}{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

/// Makes a credential for `name` holding `scopes`, valid ninety days. A name that is empty, the
/// operator's, or held by another credential not revoked is refused; so is a scope that names
/// no project and no capability, or a project that does not exist.
pub async fn issue(
    repo: &Repository,
    now: DateTime<Utc>,
    name: &str,
    scopes: Vec<Scope>,
) -> Result<Issued, AppError> {
    let name = name.trim();
    if let Some(problem) = credential_problem(name, &scopes) {
        return Err(AppError::Invalid(problem));
    }
    for scope in &scopes {
        if scope.all_projects {
            continue;
        }
        for project in &scope.projects {
            if repo.get_project(*project).await?.is_none() {
                return Err(AppError::Invalid(format!(
                    "project {project} does not exist"
                )));
            }
        }
    }
    let secret = new_secret();
    let credential = repo
        .create_agent_credential(
            name,
            &hash_secret(&secret),
            &scopes,
            now,
            now + Duration::days(AGENT_CREDENTIAL_LIFETIME_DAYS),
        )
        .await?;
    Ok(Issued { credential, secret })
}

/// Gives the credential a new secret and ninety days from `now`; the old secret stops working
/// at once. A credential that is revoked, or that does not exist, is `NotFound`.
pub async fn rotate(repo: &Repository, now: DateTime<Utc>, id: Uuid) -> Result<Issued, AppError> {
    let secret = new_secret();
    let credential = repo
        .rotate_agent_credential(
            id,
            &hash_secret(&secret),
            now + Duration::days(AGENT_CREDENTIAL_LIFETIME_DAYS),
        )
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Issued { credential, secret })
}

/// Revokes the credential for good; revoking twice is the same answer.
pub async fn revoke(
    repo: &Repository,
    now: DateTime<Utc>,
    id: Uuid,
) -> Result<AgentCredential, AppError> {
    repo.revoke_agent_credential(id, now)
        .await?
        .ok_or(AppError::NotFound)
}

/// The credential a bearer secret belongs to, when it is active; `None` for an unknown,
/// expired or revoked one (the three look the same to the caller). Notes the use.
pub async fn authenticate(
    repo: &Repository,
    now: DateTime<Utc>,
    secret: &str,
) -> Result<Option<AgentCredential>, AppError> {
    if !secret.starts_with(SECRET_PREFIX) {
        return Ok(None);
    }
    let Some(credential) = repo
        .find_agent_credential_by_hash(&hash_secret(secret))
        .await?
    else {
        return Ok(None);
    };
    if credential.state(now) != CredentialState::Active {
        return Ok(None);
    }
    let stale = credential
        .last_used_at
        .is_none_or(|used| now - used >= Duration::seconds(TOUCH_EVERY_SECONDS));
    if stale {
        repo.touch_agent_credential(credential.id, now).await?;
    }
    Ok(Some(credential))
}
