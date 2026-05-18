//! Webhook callback target — generic HTTP POST with HMAC-SHA256 signing.
//!
//! Per RFC-0947: 3 attempts with exponential backoff (1s, 2s, 4s).

use super::{CallbackError, CallbackEvent, CallbackTarget};
use async_trait::async_trait;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::time::Duration;

type HmacSha256 = Hmac<Sha256>;

/// Sign a webhook payload with HMAC-SHA256.
fn sign_payload(payload: &[u8], secret: &str) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(payload);
    let result = mac.finalize();
    format!("sha256={}", hex::encode(result.into_bytes()))
}

/// Webhook callback target — delivers events via HTTP POST.
pub struct WebhookTarget {
    url: String,
    secret: Option<String>,
    headers: std::collections::HashMap<String, String>,
    client: reqwest::Client,
    timeout: Duration,
}

impl WebhookTarget {
    pub fn new(
        url: String,
        secret: Option<String>,
        headers: std::collections::HashMap<String, String>,
        timeout: Duration,
    ) -> Self {
        Self {
            url,
            secret,
            headers,
            client: reqwest::Client::new(),
            timeout,
        }
    }

    async fn send_with_retry(&self, payload: &[u8]) -> Result<(), CallbackError> {
        let delays = [
            Duration::from_secs(1),
            Duration::from_secs(2),
            Duration::from_secs(4),
        ];

        for (attempt, delay) in delays.iter().enumerate() {
            match self.send_once(payload).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    if attempt < 2 {
                        tokio::time::sleep(*delay).await;
                    } else {
                        return Err(e);
                    }
                }
            }
        }
        unreachable!()
    }

    async fn send_once(&self, payload: &[u8]) -> Result<(), CallbackError> {
        let mut req = self
            .client
            .post(&self.url)
            .timeout(self.timeout)
            .header("Content-Type", "application/json")
            .header(
                "X-Webhook-Timestamp",
                chrono::Utc::now().timestamp().to_string(),
            );

        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }

        if let Some(ref secret) = self.secret {
            let sig = sign_payload(payload, secret);
            req = req.header("X-Webhook-Signature", sig);
        }

        let resp =
            req.body(payload.to_vec()).send().await.map_err(|e| {
                CallbackError::TargetUnreachable(format!("Webhook send failed: {e}"))
            })?;

        if !resp.status().is_success() {
            return Err(CallbackError::TargetError {
                status: resp.status().as_u16(),
                message: format!("Webhook returned {}", resp.status()),
            });
        }

        Ok(())
    }
}

#[async_trait]
impl CallbackTarget for WebhookTarget {
    async fn fire(&self, event: &CallbackEvent) -> Result<(), CallbackError> {
        let payload = serde_json::to_vec(event)
            .map_err(|e| CallbackError::SerializationError(e.to_string()))?;
        self.send_with_retry(&payload).await
    }

    fn name(&self) -> &str {
        "webhook"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::callbacks::*;

    #[test]
    fn test_hmac_signing() {
        let payload = b"test payload";
        let secret = "my-secret";
        let sig = sign_payload(payload, secret);
        assert!(sig.starts_with("sha256="));
        // Deterministic
        assert_eq!(sig, sign_payload(payload, secret));
    }

    #[test]
    fn test_hmac_different_secrets() {
        let payload = b"test payload";
        let sig1 = sign_payload(payload, "secret1");
        let sig2 = sign_payload(payload, "secret2");
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_webhook_target_name() {
        let target = WebhookTarget::new(
            "https://example.com".to_string(),
            None,
            std::collections::HashMap::new(),
            Duration::from_secs(5),
        );
        assert_eq!(target.name(), "webhook");
    }
}
