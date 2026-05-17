//! Langfuse callback target — HTTP API integration.
//!
//! Per RFC-0947: 3 attempts with exponential backoff (1s, 2s, 4s).

use super::{CallbackError, CallbackEvent, CallbackTarget};
use async_trait::async_trait;
use std::time::Duration;

const DEFAULT_LANGFUSE_HOST: &str = "https://cloud.langfuse.com";

/// Langfuse observability platform callback target.
pub struct LangfuseTarget {
    public_key: String,
    secret_key: String,
    host: String,
    client: reqwest::Client,
    timeout: Duration,
}

impl LangfuseTarget {
    pub fn new(
        public_key: String,
        secret_key: String,
        host: Option<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            public_key,
            secret_key,
            host: host.unwrap_or_else(|| DEFAULT_LANGFUSE_HOST.to_string()),
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

        for attempt in 0..3 {
            match self.send_once(payload).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    if attempt < 2 {
                        tokio::time::sleep(delays[attempt]).await;
                    } else {
                        return Err(e);
                    }
                }
            }
        }
        unreachable!()
    }

    async fn send_once(&self, payload: &[u8]) -> Result<(), CallbackError> {
        let url = format!("{}/api/public/ingestion", self.host);

        let resp = self
            .client
            .post(&url)
            .timeout(self.timeout)
            .basic_auth(&self.public_key, Some(&self.secret_key))
            .header("Content-Type", "application/json")
            .body(payload.to_vec())
            .send()
            .await
            .map_err(|e| CallbackError::TargetUnreachable(format!("Langfuse send failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(CallbackError::TargetError {
                status: resp.status().as_u16(),
                message: format!("Langfuse returned {}", resp.status()),
            });
        }

        Ok(())
    }
}

#[async_trait]
impl CallbackTarget for LangfuseTarget {
    async fn fire(&self, event: &CallbackEvent) -> Result<(), CallbackError> {
        let payload = serde_json::to_vec(event)
            .map_err(|e| CallbackError::SerializationError(e.to_string()))?;
        self.send_with_retry(&payload).await
    }

    fn name(&self) -> &str {
        "langfuse"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_langfuse_target_name() {
        let target = LangfuseTarget::new(
            "pk".to_string(),
            "sk".to_string(),
            None,
            Duration::from_secs(5),
        );
        assert_eq!(target.name(), "langfuse");
    }

    #[test]
    fn test_langfuse_default_host() {
        let target = LangfuseTarget::new(
            "pk".to_string(),
            "sk".to_string(),
            None,
            Duration::from_secs(5),
        );
        assert_eq!(target.host, DEFAULT_LANGFUSE_HOST);
    }

    #[test]
    fn test_langfuse_custom_host() {
        let target = LangfuseTarget::new(
            "pk".to_string(),
            "sk".to_string(),
            Some("https://custom.langfuse.io".to_string()),
            Duration::from_secs(5),
        );
        assert_eq!(target.host, "https://custom.langfuse.io");
    }
}
