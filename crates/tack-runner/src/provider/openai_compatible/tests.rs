use super::*;
use crate::secrets::SecretStore;

fn config(base_url: Option<&str>) -> ProviderConfig {
    ProviderConfig {
        enabled: true,
        secret: String::new(),
        base_url: base_url.map(str::to_owned),
    }
}

/// `(base_url as configured, server root it names)`: a trailing slash or
/// a `/v1` the operator copied from their server's own docs must not be
/// doubled by the `/v1` this provider appends.
#[test]
fn server_root_strips_a_trailing_slash_or_v1() {
    for (configured, root) in [
        ("http://127.0.0.1:8081", "http://127.0.0.1:8081"),
        ("http://127.0.0.1:8081/", "http://127.0.0.1:8081"),
        ("http://127.0.0.1:8081/v1", "http://127.0.0.1:8081"),
        ("http://127.0.0.1:8081/v1/", "http://127.0.0.1:8081"),
        ("  http://127.0.0.1:8081/v1 ", "http://127.0.0.1:8081"),
        ("https://router.example/openai/v1", "https://router.example/openai"),
    ] {
        assert_eq!(server_root(configured), root, "{configured}");
    }
}

#[test]
fn serves_only_the_chat_completions_wire() {
    let provider = OpenAiCompatible;
    let config = config(Some("http://127.0.0.1:8081"));
    assert!(!provider.serves(Wire::AnthropicMessages));
    assert!(provider.endpoint(Wire::AnthropicMessages, &config).is_none());
    assert!(!provider.serves(Wire::OpenAiResponses));
    assert!(provider.endpoint(Wire::OpenAiResponses, &config).is_none());
    assert!(provider.serves(Wire::OpenAiChatCompletions));
    let endpoint = provider
        .endpoint(Wire::OpenAiChatCompletions, &config)
        .expect("endpoint present");
    assert_eq!(endpoint.base_url, "http://127.0.0.1:8081/v1");
    assert_eq!(endpoint.credential_env_var, "OPENAI_API_KEY");
}

#[test]
fn endpoint_appends_v1_exactly_once() {
    let endpoint = OpenAiCompatible
        .endpoint(
            Wire::OpenAiChatCompletions,
            &config(Some("http://127.0.0.1:8081/v1")),
        )
        .expect("endpoint present");
    assert_eq!(endpoint.base_url, "http://127.0.0.1:8081/v1");
}

#[test]
fn endpoint_is_absent_without_a_base_url() {
    let provider = OpenAiCompatible;
    assert!(provider.serves(Wire::OpenAiChatCompletions));
    assert!(
        provider
            .endpoint(Wire::OpenAiChatCompletions, &config(None))
            .is_none()
    );
    assert!(
        provider
            .endpoint(Wire::OpenAiChatCompletions, &config(Some("  ")))
            .is_none()
    );
}

#[test]
fn a_credential_is_optional() {
    assert!(!OpenAiCompatible.requires_credential());
}

/// A llama.cpp `/v1/models` body: the OpenAI list shape plus the server's
/// own `meta` object, and no pricing or modality anywhere.
const LLAMA_CPP_BODY: &str = r#"{
    "object": "list",
    "data": [
        {
            "id": "qwen2.5-coder-7b-instruct-q4_k_m.gguf",
            "object": "model",
            "created": 1759000000,
            "owned_by": "llamacpp",
            "meta": {
                "vocab_type": 2,
                "n_vocab": 152064,
                "n_ctx_train": 32768,
                "n_embd": 3584,
                "n_params": 7615616512,
                "size": 4683073536
            }
        }
    ]
}"#;

#[test]
fn parses_the_openai_list_shape_with_every_metadata_unset() {
    let entries = parse_catalog(LLAMA_CPP_BODY.as_bytes()).expect("valid catalog body");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "qwen2.5-coder-7b-instruct-q4_k_m.gguf");
    assert_eq!(entries[0].context_window, None);
    assert!(entries[0].price.is_none());
    assert!(entries[0].modality.is_none());
}

#[test]
fn catalog_parse_rejects_a_non_json_body() {
    assert!(parse_catalog(b"not json").is_err());
}

/// Matches a request carrying no `Authorization` header at all — the
/// absence a server that checks no credential must see, not an empty
/// bearer token.
struct NoAuthorization;

impl wiremock::Match for NoAuthorization {
    fn matches(&self, request: &wiremock::Request) -> bool {
        !request.headers.contains_key("authorization")
    }
}

async fn mount_catalog<M: wiremock::Match + Send + Sync + 'static>(
    server: &wiremock::MockServer,
    auth: M,
) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v1/models"))
        .and(auth)
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(LLAMA_CPP_BODY))
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn catalog_fetch_sends_no_authorization_without_a_credential() {
    let server = wiremock::MockServer::start().await;
    mount_catalog(&server, NoAuthorization).await;

    let entries = OpenAiCompatible
        .fetch_catalog(&config(Some(&server.uri())), None)
        .await
        .expect("the loopback server answers");
    assert_eq!(entries.len(), 1);
    server.verify().await;
}

/// The configured `base_url` already ends in `/v1`, and the catalog is
/// still fetched from `/v1/models` — never `/v1/v1/models`.
#[tokio::test]
async fn catalog_fetch_sends_a_bearer_credential_and_never_doubles_v1() {
    let server = wiremock::MockServer::start().await;
    mount_catalog(
        &server,
        wiremock::matchers::header("authorization", "Bearer local-key"),
    )
    .await;
    let dir = tempfile::tempdir().expect("temporary directory");
    let secrets = SecretStore::file(dir.path().join("secrets.json"));
    secrets.set("local", "local-key").expect("seed secret");
    let secret = secrets.resolve("local").expect("resolve secret");

    let entries = OpenAiCompatible
        .fetch_catalog(
            &config(Some(&format!("{}/v1", server.uri()))),
            Some(&secret),
        )
        .await
        .expect("the loopback server answers");
    assert_eq!(entries[0].id, "qwen2.5-coder-7b-instruct-q4_k_m.gguf");
    server.verify().await;
}

#[tokio::test]
async fn catalog_fetch_reports_a_rejecting_status() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v1/models"))
        .respond_with(wiremock::ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let error = OpenAiCompatible
        .fetch_catalog(&config(Some(&server.uri())), None)
        .await
        .expect_err("a 401 is not a catalog");
    assert!(matches!(error, CatalogFetchError::Status(401)), "{error:?}");
}

#[tokio::test]
async fn catalog_fetch_without_a_base_url_is_a_transport_error() {
    let error = OpenAiCompatible
        .fetch_catalog(&config(None), None)
        .await
        .expect_err("nothing to fetch from");
    assert!(matches!(error, CatalogFetchError::Transport), "{error:?}");
}
