mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use tack_api::config::AppConfig;
use tower::ServiceExt;

const API_TOKEN: &str = "secret-token";
const SPLIT_ORIGIN: &str = "https://app.example.test";

fn protected_config() -> AppConfig {
    AppConfig {
        api_token: Some(API_TOKEN.into()),
        allowed_origins: vec![SPLIT_ORIGIN.into()],
        ..AppConfig::default()
    }
}

#[tokio::test]
async fn suffix_lookalikes_stay_behind_the_bearer_gate() {
    let (app, _) = common::test_app_with_config(protected_config()).await;
    for uri in [
        "/api/projects/health",
        "/api/projects/alexa",
        "/api/projects/openapi.json",
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{uri}");
    }

    let health = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);
}

#[tokio::test]
async fn csp_disallows_executable_content() {
    let (app, _) = common::test_app().await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let csp = response
        .headers()
        .get(header::CONTENT_SECURITY_POLICY)
        .and_then(|value| value.to_str().ok())
        .expect("CSP response header");
    assert!(csp.contains("script-src 'self'"));
    assert!(csp.contains("object-src 'none'"));
}
