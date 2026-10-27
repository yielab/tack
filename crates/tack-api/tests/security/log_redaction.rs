//! A credential never reaches a log, and the backup endpoint is validated.
//!
//! The import handlers receive a personal token or API key in their body; the
//! span around each call records only the non-secret fields. The backup
//! settings endpoint refuses an endpoint that points at the cloud-metadata
//! service, and a secret stored for one endpoint is dropped when the endpoint
//! moves.

use std::io::Write;
use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::fmt::format::FmtSpan;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::common;

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Capture {
    type Writer = Capture;
    fn make_writer(&'a self) -> Capture {
        self.clone()
    }
}

fn captured(capture: &Capture) -> String {
    String::from_utf8_lossy(&capture.0.lock().unwrap()).into_owned()
}

/// Debug-level logging scoped to the current thread for the guard's life.
fn capture_logs() -> (Capture, tracing::subscriber::DefaultGuard) {
    let capture = Capture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .with_ansi(false)
        .with_writer(capture.clone())
        .finish();
    (capture, tracing::subscriber::set_default(subscriber))
}

/// The GitHub import span records the repo, never the token, even when
/// GitHub answers 401.
#[tokio::test]
async fn a_github_token_never_reaches_a_log() {
    let gh = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(401))
        .mount(&gh)
        .await;
    let config = AppConfig {
        loops: tack_app::loop_config::LoopConfig {
            github_api_base: gh.uri(),
            ..Default::default()
        },
        ..AppConfig::default()
    };
    let (app, _) = common::test_app_with_config(config).await;
    let pid = common::create_project(&app, "P", "software").await;

    let (capture, _guard) = capture_logs();
    let (status, _) = common::send(
        &app,
        "POST",
        &format!("/api/projects/{pid}/import-github"),
        json!({"repo": "acme/widgets", "token": "ghp_TESTSECRET"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let log = captured(&capture);
    assert!(
        log.contains("import_github"),
        "the span was captured: {log}"
    );
    assert!(log.contains("acme/widgets"), "the repo is recorded: {log}");
    assert!(
        !log.contains("TESTSECRET"),
        "token leaked into the log: {log}"
    );
}

/// The Linear import span records the team, never the API key. The project
/// does not exist, so the call ends before any request leaves the machine.
#[tokio::test]
async fn a_linear_api_key_never_reaches_a_log() {
    let (app, _) = common::test_app().await;

    let (capture, _guard) = capture_logs();
    let (status, _) = common::send(
        &app,
        "POST",
        &format!("/api/projects/{}/import-linear", uuid::Uuid::new_v4()),
        json!({"api_key": "lin_api_TESTSECRET", "team_id": "ENG"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let log = captured(&capture);
    assert!(
        log.contains("import_linear"),
        "the span was captured: {log}"
    );
    assert!(log.contains("ENG"), "the team is recorded: {log}");
    assert!(
        !log.contains("TESTSECRET"),
        "api key leaked into the log: {log}"
    );
}

/// An endpoint on the cloud-metadata address is refused on the `endpoint` field.
#[tokio::test]
async fn backup_settings_refuse_a_metadata_endpoint() {
    let (app, _) = common::test_app().await;
    let (status, body) = common::send(
        &app,
        "PUT",
        "/api/settings/backup",
        json!({"endpoint": "http://169.254.169.254", "bucket": "b", "access_key": "k", "secret_key": "s"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["details"]["field"], "endpoint", "{body}");
}

/// Moving the endpoint with a blank secret drops the stored secret, says so,
/// and the next remote backup fails as unconfigured rather than using it.
#[tokio::test]
async fn a_new_backup_endpoint_with_a_blank_secret_clears_the_stored_secret() {
    let (app, _) = common::test_app().await;
    let put = |endpoint: &'static str, secret: &'static str| {
        let app = app.clone();
        async move {
            common::send(
                &app,
                "PUT",
                "/api/settings/backup",
                json!({"endpoint": endpoint, "bucket": "b", "access_key": "k", "secret_key": secret}),
                &[],
            )
            .await
        }
    };

    let (status, body) = put("http://127.0.0.1:9", "old-secret").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["secret_key_set"], true);
    assert!(body.get("secret_cleared").is_none(), "{body}");

    let (status, body) = put("http://127.0.0.1:10", "").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["secret_cleared"], true, "{body}");
    assert_eq!(body["secret_key_set"], false, "{body}");

    let (status, body) = common::send(&app, "POST", "/api/backup/remote", Value::Null, &[]).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
}
