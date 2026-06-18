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
}
