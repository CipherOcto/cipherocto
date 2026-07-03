//! Logging callback target — integration with RFC-0905 structured logging.
//!
//! Per RFC-0947: No retry (best effort).

use super::{CallbackError, CallbackEvent, CallbackTarget};
use async_trait::async_trait;

/// Structured logging callback target.
///
/// Delivers callback events to the structured logging system (RFC-0905).
/// No retry — best effort delivery.
pub struct LoggingTarget {
    level: LogLevel,
}

/// Log level for the logging target.
#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LoggingTarget {
    pub fn new(level: LogLevel) -> Self {
        Self { level }
    }
}

#[async_trait]
impl CallbackTarget for LoggingTarget {
    async fn fire(&self, event: &CallbackEvent) -> Result<(), CallbackError> {
        let json = serde_json::to_string(event)
            .map_err(|e| CallbackError::SerializationError(e.to_string()))?;

        match self.level {
            LogLevel::Debug => tracing::debug!("[callback] {}", json),
            LogLevel::Info => tracing::info!("[callback] {}", json),
            LogLevel::Warn => tracing::warn!("[callback] {}", json),
            LogLevel::Error => tracing::error!("[callback] {}", json),
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "logging"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::callbacks::*;

    #[test]
    fn test_logging_target_name() {
        let target = LoggingTarget::new(LogLevel::Info);
        assert_eq!(target.name(), "logging");
    }

    #[test]
    fn test_log_level_variants() {
        let _debug = LoggingTarget::new(LogLevel::Debug);
        let _info = LoggingTarget::new(LogLevel::Info);
        let _warn = LoggingTarget::new(LogLevel::Warn);
        let _error = LoggingTarget::new(LogLevel::Error);
    }

    #[tokio::test]
    async fn test_fire_logs_event() {
        let target = LoggingTarget::new(LogLevel::Info);
        let event = CallbackEvent {
            event_id: "test-123".into(),
            callback_type: CallbackType::Success,
            timestamp: chrono::Utc::now(),
            request: CallbackRequest {
                model: "gpt-4o".into(),
                messages: vec![],
                max_tokens: Some(100),
                temperature: Some(0.7),
                user_id: None,
                stream: false,
                provider: "openai".into(),
                key_id: None,
                team_id: None,
            },
            response: Some(CallbackResponse {
                id: "resp-1".into(),
                model: "gpt-4o".into(),
                response_summary: ResponseSummary {
                    choice_count: 1,
                    finish_reason: Some("stop".into()),
                    total_content_length: 10,
                },
                usage: Usage {
                    prompt_tokens: 10,
                    completion_tokens: 20,
                    total_tokens: 30,
                },
                latency_ms: 150,
                provider: "openai".into(),
                cached: false,
            }),
            error: None,
            key_metadata: None,
            timing: CallbackTiming {
                request_start: chrono::Utc::now(),
                request_end: Some(chrono::Utc::now()),
                total_ms: 150,
                provider_latency_ms: 120,
                queue_time_ms: 0,
            },
        };
        let result = target.fire(&event).await;
        assert!(result.is_ok());
    }
}
