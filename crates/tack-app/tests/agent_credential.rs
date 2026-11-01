//! The rules of an agent credential: the secret is shown once and only its hash is kept, it
//! lives ninety days, rotation replaces the secret at once, revocation is for good.

use crate::common::world;
use chrono::Duration;
use tack_app::AppError;
use tack_app::runs::agent_credential::{
    SECRET_PREFIX, authenticate, hash_secret, issue, revoke, rotate,
};
use tack_core::principal::{AGENT_CREDENTIAL_LIFETIME_DAYS, Capability, CredentialState, Scope};
use tack_protocol::Clock;

fn read_everywhere() -> Vec<Scope> {
    vec![Scope {
        all_projects: true,
        projects: vec![],
        capabilities: vec![Capability::Read],
    }]
}

#[tokio::test]
async fn the_secret_is_shown_once_and_only_its_hash_is_stored() {
    let world = world().await;
    let now = world.clock.now();
    let issued = issue(&world.repo, now, "Reviewer", read_everywhere())
        .await
        .expect("issue");
    assert!(issued.secret.starts_with(SECRET_PREFIX));
    assert_eq!(
        issued.credential.expires_at - now,
        Duration::days(AGENT_CREDENTIAL_LIFETIME_DAYS)
    );

    // The store holds the hash and nothing from which the secret could be read back.
    let by_hash = world
        .repo
        .find_agent_credential_by_hash(&hash_secret(&issued.secret))
        .await
        .expect("lookup");
    assert_eq!(by_hash.map(|c| c.id), Some(issued.credential.id));
    assert!(
        world
            .repo
            .find_agent_credential_by_hash(&issued.secret)
            .await
            .expect("lookup by the raw secret")
            .is_none()
    );
    let listed = world.repo.list_agent_credentials().await.expect("list");
    assert!(!format!("{listed:?}").contains(&issued.secret));
}

#[tokio::test]
async fn a_credential_authenticates_until_it_expires() {
    let world = world().await;
    let now = world.clock.now();
    let issued = issue(&world.repo, now, "Reviewer", read_everywhere())
        .await
        .expect("issue");

    let found = authenticate(&world.repo, now, &issued.secret)
        .await
        .expect("auth")
        .expect("active");
    assert_eq!(found.id, issued.credential.id);
    let stored = world
        .repo
        .get_agent_credential(found.id)
        .await
        .expect("get")
        .expect("row");
    assert_eq!(stored.last_used_at, Some(now));

    assert!(
        authenticate(&world.repo, now, "tack_agent_wrong")
            .await
            .expect("auth")
            .is_none()
    );
    assert!(
        authenticate(&world.repo, now, "not-an-agent-secret")
            .await
            .expect("auth")
            .is_none()
    );
    let after = now + Duration::days(AGENT_CREDENTIAL_LIFETIME_DAYS + 1);
    assert!(
        authenticate(&world.repo, after, &issued.secret)
            .await
            .expect("auth")
            .is_none(),
        "an expired credential"
    );
    assert_eq!(stored.state(after), CredentialState::Expired);
}

#[tokio::test]
async fn rotation_replaces_the_secret_and_renews_the_expiry() {
    let world = world().await;
    let now = world.clock.now();
    let issued = issue(&world.repo, now, "Reviewer", read_everywhere())
        .await
        .expect("issue");
    let later = now + Duration::days(60);
    let rotated = rotate(&world.repo, later, issued.credential.id)
        .await
        .expect("rotate");
    assert_ne!(rotated.secret, issued.secret);
    assert_eq!(rotated.credential.id, issued.credential.id);
    assert_eq!(
        rotated.credential.expires_at - later,
        Duration::days(AGENT_CREDENTIAL_LIFETIME_DAYS)
    );
    assert!(
        authenticate(&world.repo, later, &issued.secret)
            .await
            .expect("auth")
            .is_none(),
        "the old secret stops working at once"
    );
    assert!(
        authenticate(&world.repo, later, &rotated.secret)
            .await
            .expect("auth")
            .is_some()
    );
}

#[tokio::test]
async fn revocation_is_for_good() {
    let world = world().await;
    let now = world.clock.now();
    let issued = issue(&world.repo, now, "Reviewer", read_everywhere())
        .await
        .expect("issue");
    let revoked = revoke(&world.repo, now, issued.credential.id)
        .await
        .expect("revoke");
    assert_eq!(revoked.state(now), CredentialState::Revoked);
    // Revoking again keeps the first time.
    let again = revoke(&world.repo, now + Duration::hours(1), issued.credential.id)
        .await
        .expect("revoke again");
    assert_eq!(again.revoked_at, revoked.revoked_at);
    assert!(
        authenticate(&world.repo, now, &issued.secret)
            .await
            .expect("auth")
            .is_none()
    );
    assert!(matches!(
        rotate(&world.repo, now, issued.credential.id).await,
        Err(AppError::NotFound)
    ));
    assert!(matches!(
        revoke(&world.repo, now, uuid::Uuid::new_v4()).await,
        Err(AppError::NotFound)
    ));
}

#[tokio::test]
async fn a_bad_name_scope_or_project_is_refused() {
    let world = world().await;
    let now = world.clock.now();
    for (name, scopes) in [
        ("", read_everywhere()),
        ("Operator", read_everywhere()),
        ("Reviewer", vec![]),
        (
            "Reviewer",
            vec![Scope {
                all_projects: false,
                projects: vec![uuid::Uuid::new_v4()],
                capabilities: vec![Capability::Read],
            }],
        ),
    ] {
        assert!(
            matches!(
                issue(&world.repo, now, name, scopes).await,
                Err(AppError::Invalid(_))
            ),
            "{name:?}"
        );
    }
}
