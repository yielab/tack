//! OpenAPI contract tests.
//!
//! 1. **Drift gate** — the committed `docs/openapi.json` must byte-for-byte equal
//!    the spec generated from the annotated handlers + DTOs. Regenerate with:
//!
//!    ```sh
//!    UPDATE_OPENAPI=1 cargo nextest run --workspace -E 'binary(openapi_contract)'
//!    ```
//!
//! 2. **Served endpoint** — `GET /api/openapi.json` returns a valid, non-empty
//!    OpenAPI document, and is reachable without an API token.

mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use tack_api::config::AppConfig;
use tack_api::openapi::ApiDoc;
use tower::ServiceExt;
use utoipa::OpenApi;

/// Path to the committed spec, relative to this crate (`crates/tack-api`).
fn committed_spec_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/openapi.json")
}

/// The spec as it should appear on disk: pretty-printed JSON with a trailing
/// newline (so the file is POSIX-clean and `git diff` friendly).
fn generated_spec() -> String {
    let mut s = serde_json::to_string_pretty(&ApiDoc::openapi()).expect("serialize OpenAPI");
    s.push('\n');
    s
}

#[test]
fn openapi_spec_matches_committed_file() {
    let path = committed_spec_path();
    let generated = generated_spec();

    if std::env::var_os("UPDATE_OPENAPI").is_some() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create docs dir");
        }
        std::fs::write(&path, &generated).expect("write docs/openapi.json");
        eprintln!("Regenerated {}", path.display());
        return;
    }

    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "could not read {} ({e}).\nGenerate it with: UPDATE_OPENAPI=1 cargo nextest run --workspace -E 'binary(openapi_contract)'",
            path.display()
        )
    });

    assert_eq!(
        committed, generated,
        "\n\ndocs/openapi.json is out of date with the API code.\n\
         Regenerate it with:\n    UPDATE_OPENAPI=1 cargo nextest run --workspace -E 'binary(openapi_contract)'\n"
    );
}

#[test]
fn generated_spec_is_well_formed() {
    let doc = ApiDoc::openapi();
    assert_eq!(doc.info.title, "Tack API");
    // Count via the serialized JSON to stay independent of utoipa's internal
    // PathItem representation.
    let raw = serde_json::to_value(&doc).unwrap();
    let paths = raw["paths"].as_object().expect("paths object");
    // Sanity floor: the surface is dozens of endpoints; guard against an empty
    // or accidentally-truncated document. Paths collapse multiple methods, so
    // also count the individual operations.
    assert!(
        paths.len() >= 40,
        "expected >= 40 paths, found {}",
        paths.len()
    );
    let methods = ["get", "post", "put", "patch", "delete"];
    let operations: usize = paths
        .values()
        .map(|item| {
            item.as_object()
                .map(|o| methods.iter().filter(|m| o.contains_key(**m)).count())
                .unwrap_or(0)
        })
        .sum();
    assert!(
        operations >= 60,
        "expected >= 60 documented operations, found {operations}"
    );
    let schemas = doc
        .components
        .as_ref()
        .expect("components present")
        .schemas
        .len();
    assert!(
        schemas >= 30,
        "expected >= 30 component schemas, found {schemas}"
    );
}

/// Item optimistic concurrency is a browser-visible wire contract. Keep the
/// source annotations honest even while the generated artifacts are updated by
/// their designated owner: a future handler edit must not silently omit the
/// header or 412 semantics from `ApiDoc`.
#[test]
fn item_conditional_patch_contract_documents_etags_and_precondition_failure() {
    let raw = serde_json::to_value(ApiDoc::openapi()).expect("serialize OpenAPI");
    let item = &raw["paths"]["/api/items/{id}"];

    let get_ok = &item["get"]["responses"]["200"];
    assert!(
        get_ok["headers"]["ETag"].is_object(),
        "GET item 200 must document its ETag response header"
    );

    let patch = &item["patch"];
    let if_match = patch["parameters"]
        .as_array()
        .expect("PATCH item parameters")
        .iter()
        .find(|parameter| parameter["name"] == "If-Match" && parameter["in"] == "header");
    assert!(
        if_match.is_some(),
        "PATCH item must document the optional If-Match request header"
    );

    let patch_ok = &patch["responses"]["200"];
    assert!(
        patch_ok["headers"]["ETag"].is_object(),
        "PATCH item 200 must document the ETag for its returned snapshot"
    );
    assert_eq!(
        patch["responses"]["412"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/ErrorEnvelope",
        "PATCH item 412 must use the standard error envelope"
    );
}

#[tokio::test]
async fn openapi_json_endpoint_serves_valid_spec() {
    let (app, _) = common::test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = axum::body::to_bytes(res.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    // Validate the served document's structure. (We check the JSON shape rather
    // than round-tripping through utoipa's own `Deserialize`, whose impls are
    // intentionally partial — it is a serializer first.)
    let raw: serde_json::Value = serde_json::from_slice(&body).expect("valid JSON");
    assert_eq!(raw["openapi"], "3.1.0");
    assert_eq!(raw["info"]["title"], "Tack API");
    let paths = raw["paths"].as_object().expect("paths object");
    assert!(!paths.is_empty(), "spec must document at least one path");
    assert!(
        raw["components"]["schemas"]
            .as_object()
            .is_some_and(|s| !s.is_empty()),
        "spec must define component schemas"
    );
}

#[tokio::test]
async fn openapi_json_is_public_even_with_token_configured() {
    // The schema is public documentation: readable without the Bearer token.
    let config = AppConfig {
        api_token: Some("s3cr3t".to_string()),
        ..AppConfig::default()
    };
    let (app, _) = common::test_app_with_config(config).await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
