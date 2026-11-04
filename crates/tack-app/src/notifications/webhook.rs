//! The webhook channel: one signed POST of the notification to the URL the operator configured.
//! The receiver reads `X-Tack-Event` (`notification.<kind>`) and the body, and verifies
//! `X-Tack-Signature` (`sha256=<hex>`, the HMAC of the body under the shared secret) when a
//! secret is set.

use std::time::Duration;

use async_trait::async_trait;
use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use reqwest::StatusCode;
use sha2::Sha256;
use tack_core::notification::Notification;

use super::channel::{Channel, Undelivered};

type HmacSha256 = Hmac<Sha256>;

/// What the channel is called in the delivery rows.
pub const NAME: &str = "webhook";

const TIMEOUT: Duration = Duration::from_secs(10);

pub struct WebhookChannel {
    url: String,
    secret: Option<String>,
    client: reqwest::Client,
}

impl WebhookChannel {
    pub fn new(url: String, secret: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(TIMEOUT)
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

    async fn post(&self, event: &str, body: Vec<u8>) -> Result<reqwest::Response, reqwest::Error> {
        let mut request = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("X-Tack-Event", event);
        if let Some(signature) = self.sign(&body) {
            request = request.header("X-Tack-Signature", signature);
        }
        request.body(body).send().await
    }
}

#[async_trait]
impl Channel for WebhookChannel {
    fn name(&self) -> &'static str {
        NAME
    }

    async fn send(&self, notification: &Notification) -> Result<(), Undelivered> {
        let event = format!("notification.{}", notification.kind.as_str());
        let body = serde_json::to_vec(&serde_json::json!({
            "event": event,
            "notification": notification,
        }))
        .map_err(|e| Undelivered::Drop(format!("the body could not be written: {e}")))?;
        let response = self
            .post(&event, body)
            .await
            .map_err(|e| Undelivered::Retry(e.to_string()))?;
        let status = response.status();
        if status.is_success() {
            Ok(())
        } else if status.is_server_error()
            || status == StatusCode::REQUEST_TIMEOUT
            || status == StatusCode::TOO_MANY_REQUESTS
        {
            Err(Undelivered::Retry(format!("the webhook answered {status}")))
        } else {
            Err(Undelivered::Drop(format!("the webhook answered {status}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn webhook_redirect_never_delivers_signed_payload() {
        let origin = MockServer::start().await;
        let private_destination = MockServer::start().await;
        let redirect_target = format!("{}/instance-metadata", private_destination.uri());

        Mock::given(method("POST"))
            .and(path("/hook"))
            .respond_with(ResponseTemplate::new(302).insert_header("Location", redirect_target))
            .mount(&origin)
            .await;

        let webhook =
            WebhookChannel::new(format!("{}/hook", origin.uri()), Some("signing-key".into()));
        let response = webhook
            .post("notification.decision", br#"{"sensitive":true}"#.to_vec())
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
