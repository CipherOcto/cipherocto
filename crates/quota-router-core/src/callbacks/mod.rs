//! Callback system for quota-router (RFC-0947).
//!
//! Provides non-blocking callback delivery for request/response events.
//! Callbacks fire asynchronously via a bounded channel — never block the request path.

use chrono::{DateTime, Utc};
use prometheus::IntCounter;
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
    /// Optional Prometheus counter sink — set via `install_dropped_counter`.
    /// When the channel is full, both this counter and `dropped_total` are
    /// incremented so `callback_dropped_total` exposes the overflow metric.
    dropped_metric: Option<IntCounter>,
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
            dropped_metric: None,
        }
    }

    /// Wire the Prometheus counter sink for overflow tracking.
    /// After installation, every `fire()` that fails on full channel
    /// increments BOTH the local `dropped_total` AND the supplied counter.
    /// Returns the count of events already dropped but not yet reported
    /// to the metric (should be zero when installed at startup).
    pub fn install_dropped_counter(&mut self, counter: IntCounter) -> u64 {
        let prior = self.dropped_total.load(Ordering::Relaxed);
        if prior > 0 {
            counter.inc_by(prior);
        }
        self.dropped_metric = Some(counter);
        prior
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
                if let Some(c) = &self.dropped_metric {
                    c.inc();
                }
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
// Event Builders (RFC-0947 §End/Success/Failure wiring)
// ============================================================================

/// Build a `CallbackEvent` of type `End` — fires at request completion
/// (success OR failure path). Always paired with exactly one of
/// `Success` or `Failure`. `response` and `error` are mutually
/// exclusive: pass `Some(response)` for success, `Some(error)` for
/// failure, both `None` for an unexpected path.
pub fn build_end_event(
    request: CallbackRequest,
    response: Option<CallbackResponse>,
    error: Option<CallbackErrorDetail>,
    timing: CallbackTiming,
) -> CallbackEvent {
    CallbackEvent {
        event_id: uuid::Uuid::new_v4().to_string(),
        callback_type: CallbackType::End,
        timestamp: Utc::now(),
        request,
        response,
        error,
        key_metadata: None,
        timing,
    }
}

/// Build a `CallbackEvent` of type `Success` — fires after a
/// successful provider response (2xx status). The `response` carries
/// summary + usage + latency. Errors are always `None`.
pub fn build_success_event(
    request: CallbackRequest,
    response: CallbackResponse,
    timing: CallbackTiming,
) -> CallbackEvent {
    CallbackEvent {
        event_id: uuid::Uuid::new_v4().to_string(),
        callback_type: CallbackType::Success,
        timestamp: Utc::now(),
        request,
        response: Some(response),
        error: None,
        key_metadata: None,
        timing,
    }
}

/// Build a `CallbackEvent` of type `Failure` — fires after a provider
/// error (4xx/5xx status) or local proxy error. The `error` carries
/// the classified code + provider source. Response is always `None`.
pub fn build_failure_event(
    request: CallbackRequest,
    error: CallbackErrorDetail,
    timing: CallbackTiming,
) -> CallbackEvent {
    CallbackEvent {
        event_id: uuid::Uuid::new_v4().to_string(),
        callback_type: CallbackType::Failure,
        timestamp: Utc::now(),
        request,
        response: None,
        error: Some(error),
        key_metadata: None,
        timing,
    }
}

/// Fire `Success` + `End` in sequence (non-blocking). Convenience
/// wrapper used by `proxy.rs` at the success-return branch. Both
/// fires are best-effort — failures are swallowed (channel-full
/// drops are tracked by the executor's `dropped_total`).
pub async fn fire_end_success(
    executor: &CallbackExecutor,
    request: CallbackRequest,
    response: CallbackResponse,
    timing: CallbackTiming,
) {
    let _ = executor
        .fire(build_success_event(
            request.clone(),
            response.clone(),
            timing.clone(),
        ))
        .await;
    let _ = executor
        .fire(build_end_event(request, Some(response), None, timing))
        .await;
}

/// Fire `Failure` + `End` in sequence (non-blocking). Convenience
/// wrapper used by `proxy.rs` at the error-return branch (both
/// provider-error and local-error paths). Best-effort delivery.
pub async fn fire_end_failure(
    executor: &CallbackExecutor,
    request: CallbackRequest,
    error: CallbackErrorDetail,
    timing: CallbackTiming,
) {
    let _ = executor
        .fire(build_failure_event(
            request.clone(),
            error.clone(),
            timing.clone(),
        ))
        .await;
    let _ = executor
        .fire(build_end_event(request, None, Some(error), timing))
        .await;
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    struct MockCallbackTarget;

    #[async_trait::async_trait]
    impl CallbackTarget for MockCallbackTarget {
        async fn fire(&self, _event: &CallbackEvent) -> Result<(), CallbackError> {
            Ok(())
        }
        fn name(&self) -> &str {
            "mock"
        }
    }

    #[test]
    fn test_callback_type_variants() {
        let types = [
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

    #[tokio::test]
    async fn test_callback_executor_new() {
        let executor = CallbackExecutor::new(10);
        assert_eq!(executor.dropped_count(), 0);
    }

    #[tokio::test]
    async fn test_callback_executor_register() {
        let executor = CallbackExecutor::new(10);
        let target: Arc<dyn CallbackTarget> = Arc::new(MockCallbackTarget);
        executor.register(CallbackType::Success, target);
        // No panic = success
    }

    #[tokio::test]
    async fn test_callback_executor_fire_success() {
        let executor = CallbackExecutor::new(10);
        let event = make_test_event(CallbackType::Success);
        assert!(executor.fire(event).await.is_ok());
    }

    #[tokio::test]
    async fn test_callback_executor_dropped_count() {
        let executor = CallbackExecutor::new(1);
        let event = make_test_event(CallbackType::Success);
        // Fill the channel
        let _ = executor.fire(event.clone()).await;
        // This one should be dropped
        let _ = executor.fire(event.clone()).await;
        // Give time for processing
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(executor.dropped_count() >= 1);
    }

    #[tokio::test]
    async fn test_callback_executor_install_dropped_counter_wires_metric() {
        let counter = IntCounter::new("test_callback_dropped_total", "test").unwrap();
        let mut executor = CallbackExecutor::new(1);
        let replay = executor.install_dropped_counter(counter.clone());
        assert_eq!(replay, 0);
        let event = make_test_event(CallbackType::Success);
        // Fill channel + force at least one drop
        let _ = executor.fire(event.clone()).await;
        let _ = executor.fire(event.clone()).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        // The metric must reflect the same count as the local AtomicU64.
        assert_eq!(
            counter.get(),
            executor.dropped_count(),
            "metric must mirror local dropped_total after install"
        );
    }

    #[tokio::test]
    async fn test_callback_executor_install_dropped_counter_replays_prior() {
        let counter = IntCounter::new("test_callback_dropped_replay_total", "test").unwrap();
        let mut executor = CallbackExecutor::new(1);
        let event = make_test_event(CallbackType::Success);
        let _ = executor.fire(event.clone()).await;
        let _ = executor.fire(event.clone()).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let prior = executor.dropped_count();
        assert!(prior > 0, "test setup should produce at least one drop");
        let replay = executor.install_dropped_counter(counter.clone());
        assert_eq!(
            replay, prior,
            "install must report prior drops for catch-up"
        );
        assert_eq!(counter.get(), prior);
    }

    #[test]
    fn test_callback_type_debug() {
        let t = CallbackType::Input;
        let debug = format!("{:?}", t);
        assert!(debug.contains("Input"));
    }

    #[test]
    fn test_callback_type_serialization() {
        let types = [
            CallbackType::Input,
            CallbackType::Success,
            CallbackType::Failure,
            CallbackType::Start,
            CallbackType::End,
            CallbackType::Service,
        ];
        for t in &types {
            let json = serde_json::to_string(t).unwrap();
            let deserialized: CallbackType = serde_json::from_str(&json).unwrap();
            assert_eq!(*t, deserialized);
        }
    }

    fn make_test_event(callback_type: CallbackType) -> CallbackEvent {
        CallbackEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            callback_type,
            timestamp: Utc::now(),
            request: CallbackRequest {
                model: "gpt-4".into(),
                messages: vec![],
                temperature: None,
                max_tokens: None,
                stream: false,
                provider: "openai".into(),
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
        }
    }

    // === Builders for End/Success/Failure (RFC-0947 §End/Success/Failure wiring) ===

    fn make_test_request(stream: bool) -> CallbackRequest {
        CallbackRequest {
            model: "gpt-4".into(),
            messages: vec![MessageMetadata {
                role: "user".into(),
                content_length: 100,
            }],
            temperature: Some(0.7),
            max_tokens: Some(1000),
            stream,
            provider: "openai".into(),
            key_id: Some("key-1".into()),
            team_id: None,
            user_id: None,
        }
    }

    fn make_test_response() -> CallbackResponse {
        CallbackResponse {
            id: "resp-1".into(),
            model: "gpt-4".into(),
            response_summary: ResponseSummary {
                choice_count: 1,
                finish_reason: Some("stop".into()),
                total_content_length: 500,
            },
            usage: Usage {
                prompt_tokens: 100,
                completion_tokens: 50,
                total_tokens: 150,
            },
            latency_ms: 500,
            provider: "openai".into(),
            cached: false,
        }
    }

    fn make_test_error() -> CallbackErrorDetail {
        CallbackErrorDetail {
            error_type: "provider_error".into(),
            message: "upstream 500".into(),
            status_code: Some(500),
            provider: Some("openai".into()),
        }
    }

    fn make_test_timing_complete() -> CallbackTiming {
        CallbackTiming {
            request_start: Utc::now(),
            request_end: Some(Utc::now()),
            total_ms: 600,
            provider_latency_ms: 500,
            queue_time_ms: 100,
        }
    }

    #[test]
    fn test_build_end_event_success_path() {
        let timing = make_test_timing_complete();
        let event = build_end_event(
            make_test_request(false),
            Some(make_test_response()),
            None,
            timing.clone(),
        );
        assert_eq!(event.callback_type, CallbackType::End);
        assert!(event.response.is_some());
        assert!(event.error.is_none());
        assert_eq!(event.timing.total_ms, timing.total_ms);
        // End event has a non-None request_end (this is the terminal marker)
        assert!(event.timing.request_end.is_some());
    }

    #[test]
    fn test_build_end_event_failure_path() {
        let event = build_end_event(
            make_test_request(false),
            None,
            Some(make_test_error()),
            make_test_timing_complete(),
        );
        assert_eq!(event.callback_type, CallbackType::End);
        assert!(event.response.is_none());
        let err = event.error.expect("error must be present on failure path");
        assert_eq!(err.error_type, "provider_error");
        assert_eq!(err.status_code, Some(500));
    }

    #[test]
    fn test_build_success_event_carries_response() {
        let timing = make_test_timing_complete();
        let event = build_success_event(
            make_test_request(true),
            make_test_response(),
            timing.clone(),
        );
        assert_eq!(event.callback_type, CallbackType::Success);
        let resp = event.response.expect("success event must carry response");
        assert_eq!(resp.model, "gpt-4");
        assert_eq!(resp.usage.total_tokens, 150);
        assert_eq!(resp.latency_ms, 500);
        assert!(event.error.is_none());
        assert!(
            event.request.stream,
            "stream flag must propagate from request"
        );
    }

    #[test]
    fn test_build_failure_event_carries_error() {
        let event = build_failure_event(
            make_test_request(false),
            make_test_error(),
            make_test_timing_complete(),
        );
        assert_eq!(event.callback_type, CallbackType::Failure);
        assert!(event.response.is_none());
        let err = event.error.expect("failure event must carry error");
        assert_eq!(err.status_code, Some(500));
        assert_eq!(err.provider.as_deref(), Some("openai"));
    }

    #[tokio::test]
    async fn test_fire_end_success_emits_two_events() {
        let executor = CallbackExecutor::new(10);
        let target: Arc<dyn CallbackTarget> = Arc::new(MockCallbackTarget);
        executor.register(CallbackType::Success, target.clone());
        executor.register(CallbackType::End, target.clone());
        fire_end_success(
            &executor,
            make_test_request(false),
            make_test_response(),
            make_test_timing_complete(),
        )
        .await;
        // Give worker time to drain
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(
            executor.dropped_count(),
            0,
            "both events should fit channel"
        );
    }

    #[tokio::test]
    async fn test_fire_end_failure_emits_two_events() {
        let executor = CallbackExecutor::new(10);
        let target: Arc<dyn CallbackTarget> = Arc::new(MockCallbackTarget);
        executor.register(CallbackType::Failure, target.clone());
        executor.register(CallbackType::End, target.clone());
        fire_end_failure(
            &executor,
            make_test_request(false),
            make_test_error(),
            make_test_timing_complete(),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(
            executor.dropped_count(),
            0,
            "both events should fit channel"
        );
    }
}
