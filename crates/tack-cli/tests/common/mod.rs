//! Shared fixture for this crate's real-subprocess tests.

/// Binds an ephemeral port and immediately drops the listener so a spawned
/// subprocess can bind it next — the same small, accepted race window this
/// crate's real-subprocess tests have always carried.
pub fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    listener.local_addr().expect("local addr").port()
}

/// The operator token every spawned server in these tests is started with.
#[allow(dead_code)] // only the embedded-runner suites start a server with a token
pub const API_TOKEN: &str = "test-operator-token";

/// A blocking client that sends [`API_TOKEN`] as a Bearer credential.
#[allow(dead_code)] // only the embedded-runner suites call it
pub fn authed_client() -> reqwest::blocking::Client {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        format!("Bearer {API_TOKEN}").parse().expect("header value"),
    );
    reqwest::blocking::Client::builder()
        .default_headers(headers)
        .build()
        .expect("client")
}
