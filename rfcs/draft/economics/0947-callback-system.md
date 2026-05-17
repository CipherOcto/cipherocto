# RFC-0947 (Economics): Callback System

## Status

Draft

## Authors

- Author: @cipherocto

## Maintainers

- Maintainer: @cipherocto

## Summary

Define a callback system for quota-router that enables logging, tracing, and third-party integrations (Langfuse, Datadog, webhooks, custom callbacks) for all LLM requests and responses. Provides parity with LiteLLM's `success_callback` and `failure_callback` interfaces.

## Dependencies

**Requires:**

- RFC-0905 (Economics): Observability and Logging
- RFC-0903 (Economics): Virtual API Key System

**Optional:**

- RFC-0904 (Economics): Real-Time Cost Tracking (for spend callbacks)
- RFC-0913 (Economics): Stoolap Pub/Sub (for distributed callback delivery)

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | <1ms overhead | Callback registration latency |
| G2 | Non-blocking | Callback execution must not block request path |
| G3 | LiteLLM parity | success_callback, failure_callback, custom callbacks |
| G4 | Extensible | Easy to add new callback targets |

## Motivation

### Problem

quota-router has no mechanism for external systems to receive request/response events. Users need:

1. **Logging** — Send request metadata to logging systems (Datadog, CloudWatch)
2. **Tracing** — Integrate with observability platforms (Langfuse, Arize, Phoenix)
3. **Webhooks** — Notify external systems on specific events (budget exhaustion, rate limits)
4. **Custom logic** — Execute user-defined callbacks for analytics, billing, compliance

### LiteLLM Compatibility

LiteLLM provides:
- `litellm.success_callback = ["langfuse", "s3", "custom"]`
- `litellm.failure_callback = ["langfuse", "sentry"]`
- `litellm.callbacks = [CustomCallback()]`

quota-router must match this interface for drop-in replacement.

## Specification

### Callback Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CallbackType {
    /// Request completed successfully
    Success,
    /// Request failed (error, timeout, rate limit)
    Failure,
    /// Request started (pre-flight)
    Start,
    /// Request completed (post-flight, includes both success and failure)
    End,
}
```

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
    /// Custom callback function (Python SDK only)
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

### Callback Data Model

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackEvent {
    /// Unique event ID
    pub event_id: String,
    /// Callback type
    pub callback_type: CallbackType,
    /// Timestamp (UTC)
    pub timestamp: DateTime<Utc>,
    /// Request metadata
    pub request: CallbackRequest,
    /// Response metadata (None for Start callbacks)
    pub response: Option<CallbackResponse>,
    /// Error details (Failure callbacks only)
    pub error: Option<CallbackError>,
    /// Virtual key metadata (if applicable)
    pub key_metadata: Option<KeyMetadata>,
    /// Timing information
    pub timing: CallbackTiming,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub stream: bool,
    pub provider: String,
    pub key_id: Option<String>,
    pub team_id: Option<String>,
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackResponse {
    pub id: String,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
    pub latency_ms: u64,
    pub provider: String,
    pub cached: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackError {
    pub error_type: String,
    pub message: String,
    pub status_code: Option<u16>,
    pub provider: Option<String>,
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

### Configuration

```yaml
# In config.yaml
callbacks:
  # Global callbacks applied to all requests
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
    /// Channel for async callback delivery
    tx: mpsc::Sender<CallbackEvent>,
    /// Background worker
    worker: JoinHandle<()>,
}

impl CallbackExecutor {
    /// Fire a callback event (non-blocking)
    pub async fn fire(&self, event: CallbackEvent) -> Result<()> {
        self.tx.send(event).await?;
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

### LiteLLM Interface Parity

```python
# Python SDK — matches LiteLLM interface
import quota_router

# Success callbacks
quota_router.success_callback = ["langfuse", "datadog"]

# Failure callbacks
quota_router.failure_callback = ["langfuse", "sentry"]

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
/// Callback errors are logged but never propagated to the caller
/// A failing callback must never block or fail an LLM request
pub enum CallbackError {
    /// Target unreachable (network error)
    TargetUnreachable { target: String, error: String },
    /// Target returned error response
    TargetError { target: String, status: u16, body: String },
    /// Serialization error
    SerializationError { error: String },
    /// Rate limited by target
    RateLimited { target: String, retry_after: Duration },
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

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| Callback registration | <1ms | One-time at startup |
| Callback dispatch | <1ms | Non-blocking send to channel |
| Callback execution | Async | Background worker, doesn't block request |
| Memory overhead | <10MB | Per 1K registered callbacks |
| Webhook latency | <100ms | Target response time |

## Security Considerations

| Threat | Impact | Mitigation |
|--------|--------|------------|
| Webhook URL injection | Medium | Validate URLs, allowlist patterns |
| Secret leakage | High | Use env vars, never log secrets |
| Callback flooding | Medium | Rate limit callback execution per target |
| SSRF via webhook | High | Block internal/private IP ranges |
| Replay attacks | Medium | Sign webhook payloads with HMAC |

### Webhook Payload Signing

```rust
/// HMAC-SHA256 signature for webhook payloads
fn sign_payload(payload: &[u8], secret: &str) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
    let signature = hmac::sign(&key, payload);
    format!("sha256={}", hex::encode(signature.as_ref()))
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
| Callback memory leak | High | Bounded channel (10K events), drop on overflow |
| Webhook SSRF | High | URL validation, block private IPs |
| Secret in logs | High | Redact all secret fields |
| Callback storm | Medium | Rate limit per target (100/min) |

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/callbacks/mod.rs` | New — callback types, executor, targets |
| `crates/quota-router-core/src/callbacks/langfuse.rs` | New — Langfuse integration |
| `crates/quota-router-core/src/callbacks/datadog.rs` | New — Datadog integration |
| `crates/quota-router-core/src/callbacks/webhook.rs` | New — Webhook delivery with HMAC signing |
| `crates/quota-router-core/src/config.rs` | Add CallbackConfig struct |
| `crates/quota-router-core/src/proxy.rs` | Fire callbacks at request start/end |
| `crates/quota-router-core/src/python_sdk/mod.rs` | Add Python callback support |
| `rfcs/accepted/economics/0947-callback-system.md` | Move to accepted |

## Implementation Phases

### Phase 1: Core Infrastructure

- [ ] Define CallbackEvent, CallbackTarget, CallbackExecutor types
- [ ] Implement async callback executor with bounded channel
- [ ] Add CallbackConfig to config.rs
- [ ] Fire callbacks in proxy.rs at request start/end

### Phase 2: Built-in Targets

- [ ] Implement Langfuse target (HTTP API)
- [ ] Implement Datadog target (HTTP API)
- [ ] Implement Webhook target with HMAC signing
- [ ] Implement Logging target (integration with RFC-0905)

### Phase 3: Python SDK Integration

- [ ] Add success_callback/failure_callback to Python SDK
- [ ] Support custom callback functions via PyO3
- [ ] Match LiteLLM callback interface

### Phase 4: Advanced Features

- [ ] Per-key callback overrides
- [ ] Callback rate limiting
- [ ] Callback retry with exponential backoff
- [ ] Callback metrics (fire count, failure count, latency)

## Future Work

- F1: Callback batching (group events, send in batches)
- F2: Callback filtering (only fire for specific models/providers)
- F3: Callback replay (replay missed events from WAL)
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

### Why LiteLLM Compatibility?

quota-router is positioned as a drop-in replacement for LiteLLM. Matching the callback interface (`success_callback`, `failure_callback`) means users can switch without code changes.

## Alternatives Considered

| Approach | Pros | Cons |
|----------|------|------|
| Synchronous callbacks | Simple | Blocks request path |
| Event bus (pub/sub) | Decoupled | Over-engineered for callbacks |
| Message queue (Redis) | Durable | External dependency |
| WAL-based callbacks | Durable, no external deps | Complex, higher latency |

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
}
```

## Version History

| Version | Date | Changes |
|---------|------|---------|
| v1 | 2026-05-17 | Initial draft |

## Related RFCs

- RFC-0905 (Economics): Observability and Logging
- RFC-0903 (Economics): Virtual API Key System
- RFC-0904 (Economics): Real-Time Cost Tracking
- RFC-0913 (Economics): Stoolap Pub/Sub

## Related Use Cases

- Enhanced Quota Router Gateway
- LiteLLM Drop-in Replacement
