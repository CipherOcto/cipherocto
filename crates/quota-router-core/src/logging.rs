// Structured Logging Module (RFC-0905)
//
// Provides NDJSON structured logging with PII redaction, async buffered writes,
// and configurable log sampling.

use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use tokio::sync::mpsc;

// ============================================================================
// Log Level
// ============================================================================

/// Log severity levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Debug => write!(f, "debug"),
            Self::Info => write!(f, "info"),
            Self::Warn => write!(f, "warn"),
            Self::Error => write!(f, "error"),
        }
    }
}

// ============================================================================
// Log Event
// ============================================================================

/// Structured log event (NDJSON format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    /// ISO 8601 timestamp
    pub timestamp: String,
    /// Log level
    pub level: LogLevel,
    /// Component that generated the event
    pub component: String,
    /// Event type
    pub event: String,
    /// Trace ID (W3C trace context)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    /// Span ID (W3C trace context)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
    /// Request ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// Provider name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Model name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Request status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Latency in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<f64>,
    /// Input token count
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u32>,
    /// Output token count
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u32>,
    /// Error message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ============================================================================
// PII Redaction
// ============================================================================

/// Headers that must never be logged
const NEVER_LOG_HEADERS: &[&str] = &["authorization", "x-api-key", "cookie", "set-cookie"];

/// Redact PII from a string value
pub fn redact_pii(value: &str) -> String {
    // Redact API keys (sk-... or bearer tokens)
    if value.starts_with("sk-") || value.starts_with("Bearer ") {
        if value.len() > 8 {
            return format!("{}...{}", &value[..4], &value[value.len() - 4..]);
        }
        return "[REDACTED]".to_string();
    }

    // Redact email addresses
    if value.contains('@') && value.contains('.') {
        let parts: Vec<&str> = value.split('@').collect();
        if parts.len() == 2 && parts[1].contains('.') {
            return format!("{}@{}", &parts[0][..1.min(parts[0].len())], parts[1]);
        }
    }

    value.to_string()
}

/// Check if a header should be logged
pub fn should_log_header(header_name: &str) -> bool {
    !NEVER_LOG_HEADERS
        .iter()
        .any(|h| h.eq_ignore_ascii_case(header_name))
}

// ============================================================================
// Async Buffered Writer
// ============================================================================

/// Async buffered log writer
pub struct LogWriter {
    sender: mpsc::Sender<LogEvent>,
    dropped: std::sync::atomic::AtomicU64,
}

impl LogWriter {
    /// Create a new log writer with the given buffer capacity
    pub fn new(buffer_size: usize) -> Self {
        let (sender, mut receiver) = mpsc::channel::<LogEvent>(buffer_size);

        tokio::spawn(async move {
            let mut stdout = io::stdout();
            while let Some(event) = receiver.recv().await {
                match serde_json::to_string(&event) {
                    Ok(mut json) => {
                        json.push('\n');
                        let _ = stdout.write_all(json.as_bytes());
                    }
                    Err(_) => continue,
                }
            }
        });

        Self {
            sender,
            dropped: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Emit a log event (non-blocking)
    pub async fn emit(&self, event: LogEvent) {
        match self.sender.try_send(event) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.dropped
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                // Channel closed, nothing to do
            }
        }
    }

    /// Get number of dropped events
    pub fn dropped_count(&self) -> u64 {
        self.dropped.load(std::sync::atomic::Ordering::Relaxed)
    }
}

// ============================================================================
// Log Sampler
// ============================================================================

/// Log sampling strategy
pub struct LogSampler {
    sample_rate: f64,
    always_log_errors: bool,
}

impl LogSampler {
    /// Create a new sampler with the given rate (0.0 to 1.0)
    pub fn new(sample_rate: f64, always_log_errors: bool) -> Self {
        Self {
            sample_rate: sample_rate.clamp(0.0, 1.0),
            always_log_errors,
        }
    }

    /// Check if this event should be logged
    pub fn should_sample(&self, event: &LogEvent) -> bool {
        // Always log errors if configured
        if self.always_log_errors && event.level >= LogLevel::Error {
            return true;
        }

        // Sample based on rate
        if self.sample_rate >= 1.0 {
            return true;
        }
        if self.sample_rate <= 0.0 {
            return false;
        }

        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen::<f64>() < self.sample_rate
    }
}

// ============================================================================
// Structured Logger
// ============================================================================

/// Main structured logger combining writer, sampler, and redaction
pub struct StructuredLogger {
    writer: LogWriter,
    sampler: LogSampler,
    min_level: LogLevel,
}

impl StructuredLogger {
    /// Create a new structured logger
    pub fn new(
        buffer_size: usize,
        sample_rate: f64,
        always_log_errors: bool,
        min_level: LogLevel,
    ) -> Self {
        Self {
            writer: LogWriter::new(buffer_size),
            sampler: LogSampler::new(sample_rate, always_log_errors),
            min_level,
        }
    }

    /// Log an event (applies sampling, level filtering, and PII redaction)
    pub async fn log(&self, mut event: LogEvent) {
        // Filter by level
        if event.level < self.min_level {
            return;
        }

        // Apply sampling
        if !self.sampler.should_sample(&event) {
            return;
        }

        // Redact PII in error messages
        if let Some(ref err) = event.error {
            event.error = Some(redact_pii(err));
        }

        // Set timestamp if not set
        if event.timestamp.is_empty() {
            event.timestamp = chrono::Utc::now().to_rfc3339();
        }

        self.writer.emit(event).await;
    }

    /// Get number of dropped events
    pub fn dropped_count(&self) -> u64 {
        self.writer.dropped_count()
    }

    /// Log a request started event
    pub async fn request_started(&self, request_id: &str, provider: &str, model: &str) {
        self.log(LogEvent {
            timestamp: String::new(),
            level: LogLevel::Debug,
            component: "proxy".to_string(),
            event: "request_started".to_string(),
            trace_id: None,
            span_id: None,
            request_id: Some(request_id.to_string()),
            provider: Some(provider.to_string()),
            model: Some(model.to_string()),
            status: None,
            latency_ms: None,
            input_tokens: None,
            output_tokens: None,
            error: None,
        })
        .await;
    }

    /// Log a request completed event
    pub async fn request_completed(
        &self,
        request_id: &str,
        provider: &str,
        model: &str,
        latency_ms: f64,
        input_tokens: u32,
        output_tokens: u32,
    ) {
        self.log(LogEvent {
            timestamp: String::new(),
            level: LogLevel::Info,
            component: "proxy".to_string(),
            event: "request_completed".to_string(),
            trace_id: None,
            span_id: None,
            request_id: Some(request_id.to_string()),
            provider: Some(provider.to_string()),
            model: Some(model.to_string()),
            status: Some("success".to_string()),
            latency_ms: Some(latency_ms),
            input_tokens: Some(input_tokens),
            output_tokens: Some(output_tokens),
            error: None,
        })
        .await;
    }

    /// Log a request failed event
    pub async fn request_failed(&self, request_id: &str, error: &str, latency_ms: f64) {
        self.log(LogEvent {
            timestamp: String::new(),
            level: LogLevel::Error,
            component: "proxy".to_string(),
            event: "request_failed".to_string(),
            trace_id: None,
            span_id: None,
            request_id: Some(request_id.to_string()),
            provider: None,
            model: None,
            status: Some("error".to_string()),
            latency_ms: Some(latency_ms),
            input_tokens: None,
            output_tokens: None,
            error: Some(error.to_string()),
        })
        .await;
    }

    /// Log a rate limit hit event
    pub async fn rate_limit_hit(&self, request_id: &str, provider: &str, model: &str) {
        self.log(LogEvent {
            timestamp: String::new(),
            level: LogLevel::Warn,
            component: "proxy".to_string(),
            event: "rate_limit_hit".to_string(),
            trace_id: None,
            span_id: None,
            request_id: Some(request_id.to_string()),
            provider: Some(provider.to_string()),
            model: Some(model.to_string()),
            status: None,
            latency_ms: None,
            input_tokens: None,
            output_tokens: None,
            error: None,
        })
        .await;
    }

    /// Log a cache hit event
    pub async fn cache_hit(&self, request_id: &str, _key: &str) {
        self.log(LogEvent {
            timestamp: String::new(),
            level: LogLevel::Debug,
            component: "cache".to_string(),
            event: "cache_hit".to_string(),
            trace_id: None,
            span_id: None,
            request_id: Some(request_id.to_string()),
            provider: None,
            model: None,
            status: None,
            latency_ms: None,
            input_tokens: None,
            output_tokens: None,
            error: None,
        })
        .await;
    }

    /// Log a cache miss event
    pub async fn cache_miss(&self, request_id: &str, _key: &str) {
        self.log(LogEvent {
            timestamp: String::new(),
            level: LogLevel::Debug,
            component: "cache".to_string(),
            event: "cache_miss".to_string(),
            trace_id: None,
            span_id: None,
            request_id: Some(request_id.to_string()),
            provider: None,
            model: None,
            status: None,
            latency_ms: None,
            input_tokens: None,
            output_tokens: None,
            error: None,
        })
        .await;
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_ordering() {
        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Error);
    }

    #[test]
    fn test_redact_api_key() {
        let redacted = redact_pii("sk-1234567890abcdef");
        assert!(redacted.contains("..."));
        assert!(!redacted.contains("1234567890abcdef"));
    }

    #[test]
    fn test_redact_email() {
        let redacted = redact_pii("user@example.com");
        assert!(redacted.contains("@"));
        assert!(!redacted.contains("user@"));
    }

    #[test]
    fn test_should_log_header() {
        assert!(should_log_header("content-type"));
        assert!(should_log_header("x-request-id"));
        assert!(!should_log_header("authorization"));
        assert!(!should_log_header("x-api-key"));
        assert!(!should_log_header("cookie"));
    }

    #[test]
    fn test_sampler_always_on() {
        let sampler = LogSampler::new(1.0, true);
        let event = LogEvent {
            timestamp: String::new(),
            level: LogLevel::Info,
            component: "test".to_string(),
            event: "test".to_string(),
            trace_id: None,
            span_id: None,
            request_id: None,
            provider: None,
            model: None,
            status: None,
            latency_ms: None,
            input_tokens: None,
            output_tokens: None,
            error: None,
        };
        assert!(sampler.should_sample(&event));
    }

    #[test]
    fn test_sampler_always_off() {
        let sampler = LogSampler::new(0.0, true);
        let event = LogEvent {
            timestamp: String::new(),
            level: LogLevel::Info,
            component: "test".to_string(),
            event: "test".to_string(),
            trace_id: None,
            span_id: None,
            request_id: None,
            provider: None,
            model: None,
            status: None,
            latency_ms: None,
            input_tokens: None,
            output_tokens: None,
            error: None,
        };
        assert!(!sampler.should_sample(&event));
    }

    #[test]
    fn test_sampler_always_log_errors() {
        let sampler = LogSampler::new(0.0, true);
        let event = LogEvent {
            timestamp: String::new(),
            level: LogLevel::Error,
            component: "test".to_string(),
            event: "test".to_string(),
            trace_id: None,
            span_id: None,
            request_id: None,
            provider: None,
            model: None,
            status: None,
            latency_ms: None,
            input_tokens: None,
            output_tokens: None,
            error: None,
        };
        // Errors should be logged even with 0.0 sample rate when always_log_errors=true
        assert!(sampler.should_sample(&event));
    }

    #[test]
    fn test_log_event_serialization() {
        let event = LogEvent {
            timestamp: "2026-05-17T10:30:00.000Z".to_string(),
            level: LogLevel::Info,
            component: "proxy".to_string(),
            event: "request_completed".to_string(),
            trace_id: Some("abc123".to_string()),
            span_id: Some("def456".to_string()),
            request_id: Some("req-789".to_string()),
            provider: Some("openai".to_string()),
            model: Some("gpt-4o".to_string()),
            status: Some("success".to_string()),
            latency_ms: Some(150.0),
            input_tokens: Some(500),
            output_tokens: Some(200),
            error: None,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"level\":\"info\""));
        assert!(json.contains("\"event\":\"request_completed\""));
        assert!(json.contains("\"provider\":\"openai\""));
    }

    #[test]
    fn test_log_event_skip_none() {
        let event = LogEvent {
            timestamp: "2026-05-17T10:30:00.000Z".to_string(),
            level: LogLevel::Info,
            component: "proxy".to_string(),
            event: "request_started".to_string(),
            trace_id: None,
            span_id: None,
            request_id: None,
            provider: None,
            model: None,
            status: None,
            latency_ms: None,
            input_tokens: None,
            output_tokens: None,
            error: None,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(!json.contains("trace_id"));
        assert!(!json.contains("span_id"));
        assert!(!json.contains("request_id"));
    }
}
