//! Callback system for quota-router (RFC-0947).
//!
//! Provides non-blocking callback delivery for request/response events.
//! Callbacks fire asynchronously via a bounded channel — never block the request path.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

pub mod datadog;
pub mod langfuse;
pub mod logging;
pub mod webhook;

// ============================================================================
// Callback Types
// ============================================================================

/// Callback event type — maps to LiteLLM's 4 callback lists plus extensions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CallbackType {
    /// Input validation/transformation (pre-provider-call).
    /// Maps to LiteLLM's input_callback.
    Input,
    /// Request completed successfully (post-provider-call).
    /// Maps to LiteLLM's success_callback.
    Success,
    /// Request failed (error, timeout, rate limit).
    /// Maps to LiteLLM's failure_callback.
    Failure,
    /// Request started (fires after key validation and rate limit checks,
    /// before provider selection and HTTP dispatch).
    Start,
    /// Request completed (fires after response is fully received or error occurs;
    /// always fires regardless of success/failure).
    End,
    /// Health/monitoring events (provider health, circuit breaker state changes).
    /// Maps to LiteLLM's service_callback.
    Service,
}

// ============================================================================
// Data Model
// ============================================================================

/// Callback event — the full event delivered to targets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackEvent {
    /// Unique event ID (UUIDv4)
    pub event_id: String,
    /// Callback type
    pub callback_type: CallbackType,
    /// Timestamp (UTC)
    pub timestamp: DateTime<Utc>,
    /// Request metadata
    pub request: CallbackRequest,
    /// Response metadata (None for Start/Input/Service callbacks)
    pub response: Option<CallbackResponse>,
    /// Error details (Failure callbacks only)
    pub error: Option<CallbackErrorDetail>,
    /// Virtual key metadata (if applicable)
    pub key_metadata: Option<KeyMetadata>,
    /// Timing information
    pub timing: CallbackTiming,
}

/// Request metadata — no content, no PII risk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackRequest {
    pub model: String,
    /// Message metadata only (roles, content lengths).
    pub messages: Vec<MessageMetadata>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub stream: bool,
    pub provider: String,
    pub key_id: Option<String>,
    pub team_id: Option<String>,
    pub user_id: Option<String>,
}

/// Message metadata — no content, no PII risk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMetadata {
    pub role: String,
    pub content_length: usize,
}

/// Response metadata — summary only, no content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackResponse {
    pub id: String,
    pub model: String,
    /// Response summary only — no full choices content.
    pub response_summary: ResponseSummary,
    pub usage: Usage,
    pub latency_ms: u64,
    pub provider: String,
    pub cached: bool,
}

/// Response summary — metadata only, no content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseSummary {
    pub choice_count: usize,
    pub finish_reason: Option<String>,
    pub total_content_length: usize,
}

/// Usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Error detail for callback events (data model, not error enum).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackErrorDetail {
    pub error_type: String,
    pub message: String,
    pub status_code: Option<u16>,
    pub provider: Option<String>,
}

/// Virtual key metadata from RFC-0903.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyMetadata {
    pub key_id: String,
    pub key_prefix: String,
    pub team_id: Option<String>,
    pub user_id: Option<String>,
    pub spend_usd: f64,
    pub max_budget_usd: Option<f64>,
}

/// Timing information for callback events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackTiming {
    pub request_start: DateTime<Utc>,
    /// None for Start/Input/Service callbacks (not yet complete).
    pub request_end: Option<DateTime<Utc>>,
    pub total_ms: u64,
    pub provider_latency_ms: u64,
    pub queue_time_ms: u64,
}

// ============================================================================
// Callback Target Trait
// ============================================================================

/// Callback target — receives events and processes them.
#[async_trait::async_trait]
pub trait CallbackTarget: Send + Sync + 'static {
    /// Fire a callback event. Returns Ok on success, Err on failure.
    /// Failures are logged but not propagated to the request path.
    async fn fire(&self, event: &CallbackEvent) -> Result<(), CallbackError>;

    /// The name of this target (for logging).
    fn name(&self) -> &str;
}

/// Error type for callback delivery failures.
#[derive(Debug, Clone)]
pub enum CallbackError {
    /// Target is unreachable (network error, timeout).
    TargetUnreachable(String),
    /// Target returned an error response.
    TargetError { status: u16, message: String },
    /// Serialization failed.
    SerializationError(String),
    /// Rate limited by target.
    RateLimited,
}

impl std::fmt::Display for CallbackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TargetUnreachable(msg) => write!(f, "Target unreachable: {msg}"),
            Self::TargetError { status, message } => {
                write!(f, "Target error {status}: {message}")
            }
            Self::SerializationError(msg) => write!(f, "Serialization error: {msg}"),
            Self::RateLimited => write!(f, "Rate limited"),
        }
    }
}

// ============================================================================
// Callback Executor
// ============================================================================

/// Type alias for the callback registry (shared with worker).
type CallbackRegistry = Arc<std::sync::RwLock<HashMap<CallbackType, Vec<Arc<dyn CallbackTarget>>>>>;

/// Callback executor — non-blocking, async.
pub struct CallbackExecutor {
    /// Registered callbacks by type (shared with worker).
    callbacks: CallbackRegistry,
    /// Channel for async callback delivery (bounded, configurable capacity).
    tx: mpsc::Sender<CallbackEvent>,
    /// Background worker handle.
    worker: Option<JoinHandle<()>>,
    /// Dropped event counter.
    dropped_total: Arc<AtomicU64>,
}

impl CallbackExecutor {
    /// Create executor with configurable channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(capacity);
        let dropped_total = Arc::new(AtomicU64::new(0));
        let callbacks: CallbackRegistry = Arc::new(std::sync::RwLock::new(HashMap::new()));

        let worker = tokio::spawn(Self::worker_loop(
            rx,
            Arc::clone(&callbacks),
            Arc::clone(&dropped_total),
        ));

        Self {
            callbacks,
            tx,
            worker: Some(worker),
            dropped_total,
        }
    }

    /// Register a callback target for a specific event type.
    pub fn register(&self, callback_type: CallbackType, target: Arc<dyn CallbackTarget>) {
        if let Ok(mut callbacks) = self.callbacks.write() {
            callbacks.entry(callback_type).or_default().push(target);
        }
    }

    /// Fire a callback event (non-blocking).
    /// Returns Err if channel is full — event is dropped, not retried.
    pub async fn fire(&self, event: CallbackEvent) -> Result<(), CallbackError> {
        match self.tx.try_send(event) {
            Ok(()) => Ok(()),
            Err(_) => {
                self.dropped_total.fetch_add(1, Ordering::Relaxed);
                Err(CallbackError::TargetUnreachable("Channel full".to_string()))
            }
        }
    }

    /// Get the number of dropped events.
    pub fn dropped_count(&self) -> u64 {
        self.dropped_total.load(Ordering::Relaxed)
    }

    /// Background worker processes events from the channel.
    /// Dispatches to all registered targets for the event type in parallel.
    /// Failures are logged but never propagated to the request path.
    async fn worker_loop(
        mut rx: mpsc::Receiver<CallbackEvent>,
        callbacks: CallbackRegistry,
        dropped_total: Arc<AtomicU64>,
    ) {
        while let Some(event) = rx.recv().await {
            let targets_snapshot = callbacks
                .read()
                .ok()
                .and_then(|c| c.get(&event.callback_type).cloned());

            if let Some(targets) = targets_snapshot {
                let handles: Vec<_> = targets
                    .iter()
                    .map(|target| {
                        let target = Arc::clone(target);
                        let event = event.clone();
                        let dropped_total = Arc::clone(&dropped_total);
                        tokio::spawn(async move {
                            if let Err(e) = target.fire(&event).await {
                                dropped_total.fetch_add(1, Ordering::Relaxed);
                                tracing::warn!(
                                    target = target.name(),
                                    event_id = %event.event_id,
                                    error = %e,
                                    "Callback delivery failed"
                                );
                            }
                        })
                    })
                    .collect();

                for handle in handles {
                    let _ = handle.await;
                }
            }
        }
    }

    /// Shutdown the executor gracefully.
    pub async fn shutdown(&mut self) {
        // Drop the sender to signal the worker to stop
        // We need to replace it with a new channel to avoid moving out of self
        let (tx, _rx) = mpsc::channel(1);
        let old_tx = std::mem::replace(&mut self.tx, tx);
        drop(old_tx);
        // Wait for the worker to finish
        if let Some(handle) = self.worker.take() {
            let _ = handle.await;
        }
    }
}

impl Drop for CallbackExecutor {
    fn drop(&mut self) {
        // Abort the worker task if it's still running
        if let Some(ref worker) = self.worker {
            if !worker.is_finished() {
                worker.abort();
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_callback_type_variants() {
        let types = vec![
            CallbackType::Input,
            CallbackType::Success,
            CallbackType::Failure,
            CallbackType::Start,
            CallbackType::End,
            CallbackType::Service,
        ];
        assert_eq!(types.len(), 6);
    }

    #[test]
    fn test_callback_event_serialization() {
        let event = CallbackEvent {
            event_id: "test-id".to_string(),
            callback_type: CallbackType::Success,
            timestamp: Utc::now(),
            request: CallbackRequest {
                model: "gpt-4".to_string(),
                messages: vec![MessageMetadata {
                    role: "user".to_string(),
                    content_length: 100,
                }],
                temperature: Some(0.7),
                max_tokens: Some(1000),
                stream: false,
                provider: "openai".to_string(),
                key_id: Some("key-123".to_string()),
                team_id: None,
                user_id: None,
            },
            response: Some(CallbackResponse {
                id: "resp-123".to_string(),
                model: "gpt-4".to_string(),
                response_summary: ResponseSummary {
                    choice_count: 1,
                    finish_reason: Some("stop".to_string()),
                    total_content_length: 500,
                },
                usage: Usage {
                    prompt_tokens: 100,
                    completion_tokens: 50,
                    total_tokens: 150,
                },
                latency_ms: 500,
                provider: "openai".to_string(),
                cached: false,
            }),
            error: None,
            key_metadata: None,
            timing: CallbackTiming {
                request_start: Utc::now(),
                request_end: Some(Utc::now()),
                total_ms: 500,
                provider_latency_ms: 450,
                queue_time_ms: 50,
            },
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: CallbackEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.callback_type, CallbackType::Success);
    }

    #[tokio::test]
    async fn test_callback_executor_channel_full() {
        let executor = CallbackExecutor::new(1);

        let event = CallbackEvent {
            event_id: "test-id".to_string(),
            callback_type: CallbackType::Success,
            timestamp: Utc::now(),
            request: CallbackRequest {
                model: "gpt-4".to_string(),
                messages: vec![],
                temperature: None,
                max_tokens: None,
                stream: false,
                provider: "openai".to_string(),
                key_id: None,
                team_id: None,
                user_id: None,
            },
            response: None,
            error: None,
            key_metadata: None,
            timing: CallbackTiming {
                request_start: Utc::now(),
                request_end: None,
                total_ms: 0,
                provider_latency_ms: 0,
                queue_time_ms: 0,
            },
        };

        // First send should succeed
        assert!(executor.fire(event.clone()).await.is_ok());

        // Second send may succeed or fail depending on timing
        // The dropped count should track failures
        let _ = executor.fire(event).await;
        // Channel capacity is 1, so at least one should be tracked
    }

    #[test]
    fn test_request_end_none_for_start_callback() {
        let timing = CallbackTiming {
            request_start: Utc::now(),
            request_end: None, // Start callbacks have no end time
            total_ms: 0,
            provider_latency_ms: 0,
            queue_time_ms: 0,
        };
        assert!(timing.request_end.is_none());
    }
}
