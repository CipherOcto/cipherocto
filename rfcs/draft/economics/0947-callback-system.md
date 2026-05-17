# RFC-0947 (Economics): Callback System

## Status

Draft (v2 — Round 1 adversarial review fixes)

## Authors

- Author: @cipherocto

## Maintainers

- Maintainer: @cipherocto

## Summary

Define a callback system for quota-router that enables logging, tracing, and third-party integrations (Langfuse, Datadog, webhooks, custom callbacks) for all LLM requests and responses. Provides parity with LiteLLM's four callback lists: `input_callback`, `success_callback`, `failure_callback`, and `service_callback`.

## Dependencies

**Requires:**

- RFC-0905 (Economics): Observability and Logging
- RFC-0903 (Economics): Virtual API Key System

**Optional:**

- RFC-0904 (Economics): Real-Time Cost Tracking (for spend callbacks)
- RFC-0913 (Economics): Stoolap Pub/Sub (for distributed callback delivery — adds durability; without it, callbacks are fire-and-forget in-memory)

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | <1ms overhead | Callback dispatch latency (non-blocking send to channel) |
| G2 | Non-blocking | Callback execution must not block request path |
| G3 | LiteLLM parity | input_callback, success_callback, failure_callback, service_callback |
| G4 | Extensible | Easy to add new callback targets |

## Motivation

### Problem

quota-router has no mechanism for external systems to receive request/response events. Users need:

1. **Logging** — Send request metadata to logging systems (Datadog, CloudWatch)
2. **Tracing** — Integrate with observability platforms (Langfuse, Arize, Phoenix)
3. **Webhooks** — Notify external systems on specific events (budget exhaustion, rate limits)
4. **Custom logic** — Execute user-defined callbacks for analytics, billing, compliance

### LiteLLM Compatibility

LiteLLM provides four callback lists:
- `litellm.input_callback` — Fires before provider call; supports input validation, transformation, and rejection
- `litellm.success_callback` — Fires after successful completion
- `litellm.failure_callback` — Fires after failure (error, timeout, rate limit)
- `litellm.service_callback` — Fires for health/monitoring events (provider health, circuit breaker)
- `litellm.callbacks = [CustomCallback()]` — Custom callback class instances

quota-router must match all four lists for drop-in replacement.

## Specification

### Callback Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CallbackType {
    /// Input validation/transformation (pre-provider-call).
    /// Maps to LiteLLM's input_callback.
    /// Supports: input validation, transformation, rejection (return error to abort request).
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
```

**Start callback timing:** Fires after key validation (`validate_key()`) and rate limit checks, but before provider selection and the outgoing HTTP request to the provider. This ensures `key_id`, `team_id`, and `provider` metadata are available.

### Callback Targets

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CallbackTarget {
    /// Langfuse observability platform
    Langfuse {
        public_key: String,
        secret_key: String,
        host: Option<String>,
    },
    /// Datadog logging
    Datadog {
        api_key: String,
        site: Option<String>,
    },
    /// Custom webhook URL
    Webhook {
        url: String,
        secret: Option<String>,
        headers: HashMap<String, String>,
    },
    /// Custom callback function (Python SDK only — not available via HTTP proxy).
    /// For HTTP proxy path, use Webhook target instead.
    Custom {
        module: String,
        function: String,
    },
    /// Structured logging (RFC-0905)
    Logging {
        level: LogLevel,
    },
}
```

**Limitation:** `CallbackTarget::Custom` is only available via the Python SDK. The HTTP proxy path cannot invoke Python callback functions. Use `Webhook` targets for HTTP proxy integrations.

### Callback Data Model

```rust
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackRequest {
    pub model: String,
    /// Message metadata only (roles, content lengths). Full message content
    /// is NOT included to prevent PII leakage to third-party targets.
    pub messages: Vec<MessageMetadata>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub stream: bool,
    pub provider: String,
    pub key_id: Option<String>,
    pub team_id: Option<String>,
    pub user_id: Option<String>,
}

/// Message metadata — no content, no PII risk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMetadata {
    pub role: String,
    pub content_length: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackResponse {
    pub id: String,
    pub model: String,
    /// Response summary only — no full choices content.
    /// Prevents PII leakage to third-party targets.
    pub response_summary: ResponseSummary,
    pub usage: Usage,
    pub latency_ms: u64,
    pub provider: String,
    pub cached: bool,
}

/// Response summary — metadata only, no content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseSummary {
    pub choice_count: usize,
    pub finish_reason: Option<String>,
    pub total_content_length: usize,
}

/// Error detail for callback events (data model, not error enum)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackErrorDetail {
    pub error_type: String,
    pub message: String,
    pub status_code: Option<u16>,
    pub provider: Option<String>,
}

/// Virtual key metadata from RFC-0903
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyMetadata {
    pub key_id: String,
    pub key_prefix: String,
    pub team_id: Option<String>,
    pub user_id: Option<String>,
    pub spend_usd: f64,
    pub max_budget_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackTiming {
    pub request_start: DateTime<Utc>,
    pub request_end: DateTime<Utc>,
    pub total_ms: u64,
    pub provider_latency_ms: u64,
    pub queue_time_ms: u64,
}
```

**Message types:** `MessageMetadata` and `ResponseSummary` are custom types defined by this RFC (not imported from `types.rs` or `shared_types.rs`) to ensure no full content is ever sent to third-party callback targets.

### Configuration

```yaml
# In config.yaml
callbacks:
  # Bounded channel capacity (default: 10000)
  channel_capacity: 10000

  # Global callbacks applied to all requests
  input:
    - type: logging
      level: debug
  success:
    - type: langfuse
      public_key: "${LANGFUSE_PUBLIC_KEY}"
      secret_key: "${LANGFUSE_SECRET_KEY}"
    - type: webhook
      url: "https://example.com/webhook"
      secret: "${WEBHOOK_SECRET}"
  failure:
    - type: langfuse
      public_key: "${LANGFUSE_PUBLIC_KEY}"
      secret_key: "${LANGFUSE_SECRET_KEY}"
    - type: logging
      level: error
  start:
    - type: logging
      level: debug
  end:
    - type: datadog
      api_key: "${DATADOG_API_KEY}"
  service:
    - type: logging
      level: warn

  # Per-key overrides
  key_overrides:
    "key-123":
      success:
        - type: webhook
          url: "https://analytics.example.com/track"
```

### Execution Model

```rust
/// Callback executor — non-blocking, async
pub struct CallbackExecutor {
    /// Registered callbacks by type
    callbacks: HashMap<CallbackType, Vec<CallbackTarget>>,
    /// HTTP client for webhook/langfuse/datadog calls
    client: reqwest::Client,
    /// Channel for async callback delivery (bounded, configurable capacity)
    tx: mpsc::Sender<CallbackEvent>,
    /// Background worker
    worker: JoinHandle<()>,
}

impl CallbackExecutor {
    /// Create executor with configurable channel capacity
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(capacity);
        // ...
    }

    /// Fire a callback event (non-blocking).
    /// Returns Err if channel is full — event is dropped, not retried.
    pub async fn fire(&self, event: CallbackEvent) -> Result<()> {
        self.tx.try_send(event)?;
        Ok(())
    }

    /// Background worker processes events
    async fn worker_loop(mut rx: mpsc::Receiver<CallbackEvent>) {
        while let Some(event) = rx.recv().await {
            // Dispatch to all registered targets for this event type
            // Execute in parallel, log failures but don't propagate
        }
    }
}
```

**Channel overflow behavior:** When the bounded channel is full, `fire()` returns an error and the event is dropped. This is intentional — callbacks must never backpressure the request path. A `callback_dropped_total` metric (RFC-0905) tracks dropped events. Overflow does NOT trigger retry — retry applies only to failed deliveries after successful channel send.

### Streaming Callback Semantics

For streaming requests (`stream: true`):
- **Start** callback: Fires at stream open (same timing as non-streaming)
- **Input** callback: Fires before stream request is sent (same as non-streaming)
- **Success** callback: Fires when stream completes successfully (after last chunk)
- **Failure** callback: Fires if stream errors mid-way
- **End** callback: Fires when stream closes (success or failure)
- **Response** contains aggregated usage and total latency, not per-chunk data
- **No per-chunk callbacks** — callbacks fire once per request, not per SSE event

### LiteLLM Interface Parity

```python
# Python SDK — matches LiteLLM interface
import quota_router

# Input callbacks (pre-call validation/transformation)
quota_router.input_callback = ["custom_validator"]

# Success callbacks
quota_router.success_callback = ["langfuse", "datadog"]

# Failure callbacks
quota_router.failure_callback = ["langfuse", "sentry"]

# Service callbacks (health/monitoring)
quota_router.service_callback = ["prometheus"]

# Custom callbacks
from quota_router.callbacks import MyCustomCallback
quota_router.callbacks = [MyCustomCallback()]

# Per-request callbacks
response = quota_router.completion(
    model="gpt-4",
    messages=[...],
    success_callback=["langfuse"],
)
```

### Error Handling

```rust
/// Callback errors are logged but never propagated to the caller.
/// A failing callback must never block or fail an LLM request.
pub enum CallbackError {
    /// Target unreachable (network error)
    TargetUnreachable { target: String, error: String },
    /// Target returned error response
    TargetError { target: String, status: u16, body: String },
    /// Serialization error
    SerializationError { error: String },
    /// Rate limited by target
    RateLimited { target: String, retry_after: Duration },
    /// Channel full (event dropped)
    ChannelFull { capacity: usize },
}
```

### Retry Policy

| Target | Retry | Backoff |
|--------|-------|---------|
| Webhook | 3 attempts | Exponential (1s, 2s, 4s) |
| Langfuse | 3 attempts | Exponential (1s, 2s, 4s) |
| Datadog | 3 attempts | Exponential (1s, 2s, 4s) |
| Logging | No retry | Best effort |
| Custom | No retry | Best effort |

Retry applies to failed HTTP deliveries only. Channel overflow (event dropped) does NOT trigger retry.

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| Callback dispatch | <1ms | Non-blocking send to channel |
| Callback execution | Async | Background worker, doesn't block request |
| Memory overhead | <10MB | Per 1K registered callbacks |
| Webhook latency | <100ms | Target response time |
| Channel capacity | 10000 | Configurable via `channel_capacity` |

## Security Considerations

| Threat | Impact | Mitigation |
|--------|--------|------------|
| Webhook URL injection | Medium | Validate URLs, allowlist patterns |
| Secret leakage | High | Use env vars, never log secrets |
| PII leakage to third parties | High | No full message/response content — metadata only |
| Callback flooding | Medium | Bounded channel (configurable), rate limit per target |
| SSRF via webhook | High | Block internal/private IP ranges |
| Replay attacks | Medium | Sign webhook payloads with HMAC |
| Custom callback panic | Medium | Catch panics, log error, continue processing |

### Webhook Payload Signing

```rust
/// HMAC-SHA256 signature for webhook payloads using the `hmac` crate (0.12)
fn sign_payload(payload: &[u8], secret: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(payload);
    let result = mac.finalize();
    format!("sha256={}", hex::encode(result.into_bytes()))
}

/// Webhook headers
/// X-Webhook-Signature: sha256=<hmac>
/// X-Webhook-Timestamp: <unix_timestamp>
/// X-Webhook-ID: <event_id>
```

## Adversarial Review

| Threat | Impact | Mitigation |
|--------|--------|------------|
| Callback blocks request path | Critical | Async execution, bounded channel |
| Callback memory leak | High | Bounded channel (configurable, default 10K), drop on overflow |
| PII leakage to third-party targets | High | MessageMetadata/ResponseSummary — no content |
| Webhook SSRF | High | URL validation, block private IPs |
| Secret in logs | High | Redact all secret fields |
| Callback storm | Medium | Rate limit per target (100/min) |
| Custom callback panic | Medium | Catch panics, log error, continue |

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/callbacks/mod.rs` | New — callback types, executor, targets |
| `crates/quota-router-core/src/callbacks/langfuse.rs` | New — Langfuse integration |
| `crates/quota-router-core/src/callbacks/datadog.rs` | New — Datadog integration |
| `crates/quota-router-core/src/callbacks/webhook.rs` | New — Webhook delivery with HMAC signing |
| `crates/quota-router-core/src/config.rs` | Add CallbackConfig struct with channel_capacity |
| `crates/quota-router-core/src/proxy.rs` | Fire callbacks at request start/end |
| `crates/quota-router-core/src/python_sdk/mod.rs` | Add Python callback support |

## Implementation Phases

### Phase 1: Core Infrastructure

- [ ] Define CallbackEvent, CallbackTarget, CallbackExecutor types
- [ ] Implement async callback executor with configurable bounded channel
- [ ] Add CallbackConfig to config.rs
- [ ] Fire callbacks in proxy.rs at request start/end

### Phase 2: Built-in Targets

- [ ] Implement Langfuse target (HTTP API)
- [ ] Implement Datadog target (HTTP API)
- [ ] Implement Webhook target with HMAC signing (using `hmac` + `sha2` crates)
- [ ] Implement Logging target (integration with RFC-0905)

### Phase 3: Python SDK Integration

- [ ] Add input_callback/success_callback/failure_callback/service_callback to Python SDK
- [ ] Support custom callback functions via PyO3
- [ ] Match LiteLLM callback interface

### Phase 4: Advanced Features

- [ ] Per-key callback overrides
- [ ] Callback rate limiting (100/min per target)
- [ ] Callback retry with exponential backoff
- [ ] Callback metrics (fire count, failure count, latency, dropped count)

## Future Work

- F1: Callback batching (group events, send in batches)
- F2: Callback filtering (per-team, per-model — add `team_overrides` and `model_filters` config)
- F3: Callback replay (replay missed events from WAL via RFC-0913)
- F4: Callback analytics dashboard

## Rationale

### Why Async Execution?

Callbacks must never block the request path. Using a bounded channel + background worker ensures:
1. Request latency is unaffected by callback execution
2. Callback failures don't propagate to callers
3. Backpressure is handled gracefully (drop events on overflow)

### Why Per-Target Retry?

Different targets have different reliability characteristics:
- Langfuse/Datadog: Reliable cloud services, retry makes sense
- Webhooks: May be unreliable, retry with backoff
- Logging: Best effort, no retry needed
- Custom: User's responsibility, no retry

### Why No Content in Callbacks?

Full message/response content contains PII. Enterprise compliance (GDPR, SOC2) requires that third-party targets receive metadata only. Users who need full content should use the Logging target (RFC-0905) which processes data locally.

### Why WAL is Optional?

WAL (RFC-0913) adds durability but also complexity and latency. For most use cases, fire-and-forget with bounded channel is sufficient. WAL adds replay capability for mission-critical audit trails.

## Alternatives Considered

| Approach | Pros | Cons |
|----------|------|------|
| Synchronous callbacks | Simple | Blocks request path |
| Event bus (pub/sub) | Decoupled | Over-engineered for callbacks |
| Message queue (Redis) | Durable | External dependency |
| WAL-based callbacks | Durable, replayable | Higher latency, added complexity (optional via RFC-0913) |

## Test Vectors

```rust
#[test]
fn test_callback_event_serialization() {
    let event = CallbackEvent {
        event_id: "evt_123".to_string(),
        callback_type: CallbackType::Success,
        timestamp: Utc::now(),
        request: CallbackRequest { /* ... */ },
        response: Some(CallbackResponse { /* ... */ }),
        error: None,
        key_metadata: None,
        timing: CallbackTiming { /* ... */ },
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"callback_type\":\"Success\""));
}

#[test]
fn test_webhook_signature() {
    let payload = b"test payload";
    let secret = "test_secret";
    let sig = sign_payload(payload, secret);
    assert!(sig.starts_with("sha256="));
    // Verify with known HMAC-SHA256 output
}

#[test]
fn test_no_content_in_response_summary() {
    let summary = ResponseSummary {
        choice_count: 1,
        finish_reason: Some("stop".to_string()),
        total_content_length: 42,
    };
    let json = serde_json::to_string(&summary).unwrap();
    assert!(!json.contains("content"));
    assert!(!json.contains("text"));
}
```

## Version History

| Version | Date | Changes |
|---------|------|---------|
| v1 | 2026-05-17 | Initial draft |
| v2 | 2026-05-17 | Round 1 fixes: added input/service callbacks, fixed CallbackError naming collision, removed PII from responses, added streaming semantics, fixed HMAC to use hmac crate, clarified channel capacity, added KeyMetadata, fixed G1 metric |

## Related RFCs

- RFC-0905 (Economics): Observability and Logging
- RFC-0903 (Economics): Virtual API Key System
- RFC-0904 (Economics): Real-Time Cost Tracking
- RFC-0913 (Economics): Stoolap Pub/Sub

## Related Use Cases

- Enhanced Quota Router Gateway
- LiteLLM Drop-in Replacement
