//! Datadog callback target — HTTP API integration.
//!
//! Per RFC-0947: 3 attempts with exponential backoff (1s, 2s, 4s).

use super::{CallbackError, CallbackEvent, CallbackTarget};
use async_trait::async_trait;
use std::time::Duration;

const DEFAULT_DATADOG_SITE: &str = "datadoghq.com";

/// Datadog logging callback target.
pub struct DatadogTarget {
    api_key: String,
    site: String,
    client: reqwest::Client,
    timeout: Duration,
}

impl DatadogTarget {
    pub fn new(api_key: String, site: Option<String>, timeout: Duration) -> Self {
        Self {
            api_key,
            site: site.unwrap_or_else(|| DEFAULT_DATADOG_SITE.to_string()),
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
        let url = format!("https://http-intake.logs.{}/api/v2/logs", self.site);

        let resp = self
            .client
            .post(&url)
            .timeout(self.timeout)
            .header("Content-Type", "application/json")
            .header("DD-API-KEY", &self.api_key)
            .body(payload.to_vec())
            .send()
            .await
            .map_err(|e| CallbackError::TargetUnreachable(format!("Datadog send failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(CallbackError::TargetError {
                status: resp.status().as_u16(),
                message: format!("Datadog returned {}", resp.status()),
            });
        }

        Ok(())
    }
}

#[async_trait]
impl CallbackTarget for DatadogTarget {
    async fn fire(&self, event: &CallbackEvent) -> Result<(), CallbackError> {
        let payload = serde_json::to_vec(event)
            .map_err(|e| CallbackError::SerializationError(e.to_string()))?;
        self.send_with_retry(&payload).await
    }

    fn name(&self) -> &str {
        "datadog"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_datadog_target_name() {
        let target = DatadogTarget::new("api-key".to_string(), None, Duration::from_secs(5));
        assert_eq!(target.name(), "datadog");
    }

    #[test]
    fn test_datadog_default_site() {
        let target = DatadogTarget::new("api-key".to_string(), None, Duration::from_secs(5));
        assert_eq!(target.site, DEFAULT_DATADOG_SITE);
    }

    #[test]
    fn test_datadog_custom_site() {
        let target = DatadogTarget::new(
            "api-key".to_string(),
            Some("datadog.eu".to_string()),
            Duration::from_secs(5),
        );
        assert_eq!(target.site, "datadog.eu");
    }
}
