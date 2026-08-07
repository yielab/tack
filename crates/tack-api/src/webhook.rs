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
            // Webhook URLs are operator-configured, but a redirect response is
            // controlled by the remote peer. Do not let it retarget a signed
            // payload (or its HMAC) to an unvalidated destination.
            .redirect(reqwest::redirect::Policy::none())
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

    async fn deliver(
        client: &reqwest::Client,
        url: &str,
        event: &str,
        body: Vec<u8>,
        sig: Option<String>,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let mut builder = client
            .post(url)
            .header("Content-Type", "application/json")
            .header("X-Tack-Event", event)
            .body(body);

        if let Some(sig) = sig {
            builder = builder.header("X-Tack-Signature", sig);
        }

        builder.send().await
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
            match Self::deliver(&client, &url, &event, body, sig).await {
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

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn redirect_does_not_retarget_signed_webhook_to_private_destination() {
        let origin = MockServer::start().await;
        let private_destination = MockServer::start().await;
        let redirect_target = format!("{}/instance-metadata", private_destination.uri());

        Mock::given(method("POST"))
            .and(path("/hook"))
            .respond_with(ResponseTemplate::new(302).insert_header("Location", redirect_target))
            .mount(&origin)
            .await;

        let webhook =
            WebhookClient::new(format!("{}/hook", origin.uri()), Some("signing-key".into()));
        let body = br#"{\"sensitive\":true}"#.to_vec();
        let response = WebhookClient::deliver(
            &webhook.client,
            &webhook.url,
            "item.updated",
            body.clone(),
            webhook.sign(&body),
        )
        .await
        .expect("origin response should be returned without following it");

        assert_eq!(response.status(), reqwest::StatusCode::FOUND);
        assert!(
            private_destination
                .received_requests()
                .await
                .expect("inspect private destination")
                .is_empty(),
            "a redirect must never deliver the signed payload or HMAC to a private target"
        );
    }
}
