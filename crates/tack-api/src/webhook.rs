use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Fire-and-forget outbound webhook client.
///
/// Cloning is cheap — the inner `reqwest::Client` shares a connection pool.
#[derive(Clone)]
pub struct WebhookClient {
    url: String,
    secret: Option<String>,
    client: reqwest::Client,
}

impl WebhookClient {
    pub fn new(url: String, secret: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self {
            url,
            secret,
            client,
        }
    }

    fn sign(&self, body: &[u8]) -> Option<String> {
        let secret = self.secret.as_deref()?;
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).ok()?;
        mac.update(body);
        Some(format!(
            "sha256={}",
            hex::encode(mac.finalize().into_bytes())
        ))
    }

    /// Spawn a background task that POSTs `payload` to the configured URL.
    /// Errors are logged but never propagate to the caller.
    pub fn fire(&self, event: &str, payload: serde_json::Value) {
        let url = self.url.clone();
        let client = self.client.clone();
        let event = event.to_string();

        let body = match serde_json::to_vec(&payload) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(event=%event, error=%e, "webhook: failed to serialize payload");
                return;
            }
        };
        let sig = self.sign(&body);

        tokio::spawn(async move {
            let mut builder = client
                .post(&url)
                .header("Content-Type", "application/json")
                .header("X-Tack-Event", &event)
                .body(body);

            if let Some(sig) = sig {
                builder = builder.header("X-Tack-Signature", sig);
            }

            match builder.send().await {
                Ok(r) if r.status().is_success() => {
                    tracing::debug!(event=%event, status=%r.status(), "webhook delivered");
                }
                Ok(r) => {
                    tracing::warn!(event=%event, status=%r.status(), "webhook non-2xx response");
                }
                Err(e) => {
                    tracing::warn!(event=%event, error=%e, "webhook delivery failed");
                }
            }
        });
    }
}
