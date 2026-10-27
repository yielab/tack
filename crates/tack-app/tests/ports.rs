//! The ports in `tack_app::ports` stand in for the HTTP layer: a use case or a loop pass runs
//! against fakes of them, with a real store and no router.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tack_app::github_sync::GitHub;
use tack_app::loop_config::{LoopConfig, MIN_BACKUP_INTERVAL_SECS};
use tack_app::ports::{Artifacts, BoardEvents, Ports, Verifier, VerifierError};
use tack_app::{auto_start, merge};
use tack_protocol::SystemClock;
use tack_test_support::setup_test_db;
use uuid::Uuid;

struct NoVerifier;

#[async_trait]
impl Verifier for NoVerifier {
    async fn verify_attestation(
        &self,
        _envelope: &[u8],
        _public_key_pem: &str,
        _commit: &str,
    ) -> Result<String, VerifierError> {
        Err(VerifierError::NotInstalled)
    }

    async fn pull_request_observed(&self, _repo: &str, _number: i64) {}
}

struct NoArtifacts;

#[async_trait]
impl Artifacts for NoArtifacts {
    async fn read(&self, _reference: &str) -> Option<Vec<u8>> {
        None
    }
}

#[derive(Default)]
struct RecordedEvents(Mutex<Vec<(Uuid, Uuid, String, String)>>);

impl BoardEvents for RecordedEvents {
    fn item_moved(&self, project_id: Uuid, item_id: Uuid, old_status: String, new_status: String) {
        self.0
            .lock()
            .unwrap()
            .push((project_id, item_id, old_status, new_status));
    }
}

fn fake_ports(events: Arc<RecordedEvents>) -> Ports {
    Ports {
        github: GitHub {
            github_token: None,
            api_base: "http://github.invalid".to_owned(),
        },
        runner: None,
        verifier: Arc::new(NoVerifier),
        artifacts: Arc::new(NoArtifacts),
        events,
    }
}

#[tokio::test]
async fn the_merge_button_answers_over_fake_ports() {
    let repo = setup_test_db().await;
    let ports = fake_ports(Arc::default());

    let (enabled, reason) = merge::button(&repo, &ports, "no-such-attempt").await;

    assert!(!enabled);
    assert_eq!(reason.as_deref(), Some("The attempt could not be read"));
}

#[tokio::test]
async fn an_auto_start_pass_over_fake_ports_with_nothing_to_start_says_nothing() {
    let repo = setup_test_db().await;
    let events = Arc::new(RecordedEvents::default());
    let ports = fake_ports(events.clone());

    let outcome = auto_start::tick(&repo, &SystemClock, &ports)
        .await
        .expect("a pass over an empty board");

    assert_eq!(outcome.started, 0);
    assert!(events.0.lock().unwrap().is_empty());
}

#[test]
fn an_empty_loop_configuration_is_the_default_one() {
    let parsed: LoopConfig = serde_json::from_str("{}").expect("every key has a default");
    assert_eq!(parsed, LoopConfig::default());
    assert_eq!(parsed.auto_start_seconds, 30);
    assert!(
        !parsed.execution_retention_enable,
        "deleting rows is opt-in"
    );
    assert!(parsed.execution_health_enable);
}

#[test]
fn the_backup_interval_has_a_floor_and_the_poll_needs_a_token() {
    let mut config = LoopConfig::default();
    assert_eq!(config.backup_interval(), None);
    config.backup_interval_secs = Some(5);
    assert_eq!(
        config.backup_interval().map(|interval| interval.as_secs()),
        Some(MIN_BACKUP_INTERVAL_SECS)
    );

    config.github_poll_seconds = 60;
    assert!(!config.github_poll_enabled());
    config.github_token = Some("token".to_owned());
    assert!(config.github_poll_enabled());
}
