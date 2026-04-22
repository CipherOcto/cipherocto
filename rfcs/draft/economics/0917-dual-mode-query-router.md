# RFC-0917 (Economics): Dual-Mode Query Router — LiteLLM-Compatible Proxy + any-llm-Style SDK

## Status

Draft (v2 — Revised with feature-gated dual-mode architecture)

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu

## Summary

Define a dual-mode query router that operates under Rust feature gates: **LiteLLM Mode** (proxy gateway with OpenAI-compatible HTTP endpoints) and **any-llm Mode** (direct SDK calls via PyO3). Both modes share the same Rust router core (RFC-0902), provider abstraction layer, and stoolap persistence (RFC-0903-B1/C1), but expose different interfaces for different user segments. LiteLLM Mode targets enterprises needing centralized key management; any-llm Mode targets Python developers wanting drop-in SDK replacement without proxy deployment.

## Dependencies

**Requires:**

- RFC-0902: Multi-Provider Routing and Load Balancing (Accepted)
- RFC-0903: Virtual API Key System (Final)
- RFC-0903-B1: Schema Amendments (spend_ledger BLOB)
- RFC-0903-C1: Extended Schema Amendments (api_keys/teams BLOB)

**Optional:**

- RFC-0904: Real-Time Cost Tracking
- RFC-0906: Response Caching
- RFC-0907: Configuration Management
- RFC-0908: Python SDK and PyO3 Bindings
- RFC-0909: Deterministic Quota Accounting
- RFC-0910: Pricing Table Registry

## Motivation

### Research Foundation

Based on `docs/research/any-llm-vs-litellm-comparison.md`:

**LiteLLM** is a mature production gateway used by Stripe, Google, Netflix. It reimplements provider HTTP clients internally and exposes an OpenAI-compatible proxy with full enterprise features (virtual keys, budgets, rate limiting, 100+ providers).

**any-llm** is a lean correctness-first SDK that delegates to official provider SDKs. It has no router, no fallback, but maximum protocol correctness and simpler maintenance.

**CipherOcto Opportunity:** Adopt LiteLLM's interface contracts for enterprise compatibility while using any-llm's SDK-first approach for provider correctness — all in Rust with stoolap persistence.

### The Dual-Mode Concept

Two user segments, two interaction patterns:

| User Segment | Deployment | Example | This RFC |
|--------------|------------|---------|----------|
| **Python Developer** | No proxy | `pip install quota-router` → `import quota_router as litellm` | **any-llm Mode** |
| **Enterprise DevOps** | HTTP proxy | Deploy gateway, call `/v1/chat/completions` with API key | **LiteLLM Mode** |

Both modes share the same Rust router and provider layer — the distinction is purely interface-level:

```mermaid
flowchart TB
    subgraph SharedCore["Shared: quota-router-core"]
        RC1[Router Engine<br/>RFC-0902]
        RC2[Provider Abstraction<br/>trait LLMProvider]
        RC3[stoolap Storage<br/>RFC-0903-B1/C1]
        RC4[OCTO-W Balance]
    end
    
    subgraph LiteLLMMode["LiteLLM Mode (Proxy)"]
        LM1[HTTP Server<br/>hyper/axum]
        LM2[Auth Middleware]
        LM3[OpenAI Endpoints<br/>/v1/chat/completions]
    end
    
    subgraph AnyLLMMode["any-llm Mode (SDK)"]
        AM1[Python SDK<br/>PyO3 Bindings]
        AM2[completion()<br/>acompletion()]
    end
    
    LM1 --> RC1
    LM2 --> LM1
    LM3 --> LM1
    AM1 --> RC1
    AM2 --> AM1
    RC1 --> RC2
    RC2 --> RC3
    RC3 --> RC4
```

### Rust Feature Gates

The dual-mode architecture is enforced via Cargo feature gates:

```toml
# Cargo.toml (quota-router-core)
[features]
default = ["any-llm-mode"]  # SDK mode by default
any-llm-mode = []            # Direct provider calls via PyO3
litellm-mode = []            # HTTP proxy gateway with OpenAI compat
full = ["any-llm-mode", "litellm-mode", "enterprise"]  # Both modes + all features
enterprise = []             # Virtual keys, budgets, rate limiting
```

**Feature interaction:**

| Feature | Enables | Use Case |
|---------|---------|----------|
| `any-llm-mode` | PyO3 bindings for direct SDK calls | Python developers |
| `litellm-mode` | HTTP server + OpenAI endpoints | Enterprise proxy |
| `full` | Both modes simultaneously | Single binary, both deployment styles |
| `enterprise` | Virtual keys, budgets, rate limiting | Production deployments |

## Scope

### In Scope

#### Feature-Gated Components

| Component | Feature Gate | Description |
|-----------|-------------|-------------|
| HTTP Server | `litellm-mode` | FastAPI-less Rust HTTP server via `hyper`/`axum` |
| OpenAI Endpoints | `litellm-mode` | `/v1/chat/completions`, `/v1/embeddings`, `/v1/models` |
| PyO3 Bindings | `any-llm-mode` | Direct `completion()` call from Python |
| Provider SDK Bridge | `any-llm-mode` | PyO3 → Rust → official provider SDKs |
| Shared Router | (none) | RFC-0902 router, always available |
| Shared Storage | (none) | stoolap persistence via RFC-0903-B1/C1 schema |

#### Provider Integration Strategy

Follow any-llm's insight: **use official provider SDKs where available** (delegation approach), with HTTP fallback for providers lacking Python SDKs.

| Provider | Implementation | Approach |
|----------|----------------|----------|
| OpenAI | `reqwest` | HTTP forwarding |
| Anthropic | `reqwest` | HTTP forwarding |
| Mistral | Python SDK via PyO3 | Official SDK |
| Ollama | `reqwest` | HTTP forwarding |
| Google (Gemini) | `reqwest` | HTTP forwarding |
| Azure OpenAI | `reqwest` | HTTP forwarding |
| AWS Bedrock | `reqwest` | HTTP forwarding |

#### LiteLLM Mode (Proxy Gateway)

```mermaid
sequenceDiagram
    participant Client as OpenAI SDK Client
    participant Gateway as quota-router HTTP Server
    participant Auth as Auth Middleware
    participant Router as Rust Router (RFC-0902)
    participant Provider as LLM Provider
    participant Storage as stoolap (RFC-0903)
    
    Client->>Gateway: POST /v1/chat/completions<br/>Authorization: Bearer sk-...
    Gateway->>Auth: Validate API key
    Auth->>Storage: Check key + budget<br/>Record spend event
    Storage-->>Auth: OK / Insufficient balance
    Auth->>Router: Route request<br/>Check rate limits
    Router->>Provider: Forward request
    Provider-->>Router: LLM Response
    Router->>Storage: Record usage<br/>Update spend ledger
    Router-->>Gateway: OpenAI-formatted response
    Gateway-->>Client: HTTP 200 + response
```

#### any-llm Mode (SDK)

```mermaid
sequenceDiagram
    participant Python as Python App
    participant SDK as quota_router Python SDK
    participant PyO3 as PyO3 Bindings
    participant Router as Rust Router
    participant Provider as LLM Provider
    participant Storage as stoolap
    
    Python->>SDK: import quota_router as litellm<br/>litellm.completion(model="...", messages=[...])
    SDK->>PyO3: Call completion()
    PyO3->>Router: route_and_forward(request)
    Router->>Storage: Check budget (async)
    Router->>Provider: Forward to provider
    Provider-->>Router: Response
    Router->>Storage: Record usage
    Router-->>PyO3: CompletionResponse
    PyO3-->>SDK: Py<PyAny> (Python dict)
    SDK-->>Python: ModelResponse
```

### Out of Scope

- Implementing all 100+ LiteLLM providers from scratch
- LiteLLM Python SDK compatibility (only LiteLLM interface contract)
- Cloud-hosted SaaS deployment
- Non-Python language bindings

## Specification

### Feature Gate Architecture

```rust
// quota-router-core/src/lib.rs

#[cfg(feature = "litellm-mode")]
pub mod gateway;      // HTTP server, OpenAI endpoints

#[cfg(feature = "any-llm-mode")]
pub mod py_bridge;   // PyO3 bindings for SDK mode

pub mod router;       // RFC-0902 router (always available)
pub mod providers;    // Provider abstraction layer (always available)
pub mod storage;     // stoolap storage (always available)
```

### Provider Abstraction Layer

```rust
// providers/mod.rs

/// Unified provider interface for all LLM providers
pub trait LLMProvider: Send + Sync {
    /// Provider identifier (e.g., "openai", "anthropic")
    fn name(&self) -> &str;
    
    /// All models this provider supports
    fn supported_models(&self) -> Vec<&str>;
    
    /// Check if a model is supported
    fn supports_model(&self, model: &str) -> bool {
        self.supported_models().iter().any(|m| *m == model)
    }
    
    /// Execute completion request
    async fn completion(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError>;
    
    /// Execute embedding request
    async fn embedding(
        &self,
        request: &EmbeddingRequest,
    ) -> Result<EmbeddingResponse, ProviderError>;
    
    /// Routing weight for load balancing
    fn routing_weight(&self) -> u32;
}

/// Provider implementations
#[cfg(feature = "any-llm-mode")]
pub mod py_providers;  // Python SDK bridges via PyO3

pub mod openai;        // reqwest-based OpenAI
pub mod anthropic;    // reqwest-based Anthropic
pub mod mistral;      // reqwest fallback
pub mod ollama;       // reqwest for local
```

### LiteLLM Mode: HTTP Gateway

#### Endpoints (OpenAI-Compatible)

```yaml
# Required for LiteLLM compatibility
POST /v1/chat/completions   # Chat completions
POST /v1/embeddings          # Embeddings
GET  /v1/models              # List available models
GET  /v1/models/{model}       # Get model info
GET  /health                  # Health check

# quota-router specific (enterprise)
POST /admin/keys             # Key management
POST /admin/budgets           # Budget management
GET  /metrics                 # Prometheus metrics
```

#### Request Flow

```rust
// gateway/src/chat.rs
async fn chat_completions(
    req: ChatCompletionRequest,
    auth_header: Authorization,
) -> Result<ChatCompletionResponse, GatewayError> {
    // 1. Validate auth header → extract key_id
    let api_key = validate_key(&auth_header)?;
    
    // 2. Check budget via storage
    check_budget(&api_key.key_id, &req.model)?;
    
    // 3. Route via shared router
    let mut router = Router::new(config.clone());
    let response = router.route_and_forward(req).await?;
    
    // 4. Record usage in storage
    record_spend(&api_key.key_id, &response).await?;
    
    Ok(response)
}
```

### any-llm Mode: SDK Bindings

#### Python SDK Interface

```python
# quota_router/__init__.py
from quota_router.completion import completion, acompletion
from quota_router.embedding import embedding, aembedding
from quota_router.exceptions import (
    AuthenticationError,
    RateLimitError,
    BudgetExceededError,
    ProviderError,
)

__version__ = "0.1.0"

# LiteLLM compatibility alias
import sys
litellm = sys.modules[__name__]
```

#### PyO3 Bridge

```rust
// py_bridge/src/completion.rs

#[pyfunction]
#[pyo3(name = "completion")]
pub async fn completion(
    py: Python,
    model: String,
    messages: Vec<PyMessage>,
    // LiteLLM-compatible params (all optional)
    temperature: Option<f64>,
    max_tokens: Option<i32>,
    top_p: Option<f64>,
    stream: Option<bool>,
    **kwargs: PyDict,
) -> PyResult<Py<PyAny>> {
    // Parse model string (provider:model or model only)
    let (provider, model_name) = parse_model_string(&model)?;
    
    // Build request
    let request = CompletionRequest {
        model: model_name,
        provider,
        messages: messages.into(),
        temperature,
        max_tokens,
        // ... other params
    };
    
    // Route via shared router (no HTTP involved)
    let response = Router::global()
        .route_and_forward(request)
        .await
        .map_err(|e| PyErr::from(e))?;
    
    // Return as Python dict (OpenAI format)
    Python::with_gil(|py| response.to_dict(py))
}
```

### Shared Router (RFC-0902 Extension)

```rust
// router/src/lib.rs

pub struct Router {
    config: RouterConfig,
    providers: HashMap<String, Vec<ProviderWithState>>,
    provider_impls: HashMap<String, Arc<dyn LLMProvider>>,
}

impl Router {
    /// Route request to appropriate provider
    /// Uses strategy from RFC-0902 (simple-shuffle, least-busy, latency-based, etc.)
    pub async fn route_and_forward(
        &mut self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, RouterError> {
        // 1. Select provider based on routing strategy
        let provider_idx = self.route(&request.model)?;
        
        // 2. Get provider implementation
        let provider = self.provider_impls
            .get(&request.provider)
            .ok_or(RouterError::UnknownProvider)?;
        
        // 3. Forward request
        let response = provider.completion(&request).await?;
        
        // 4. Update provider state (latency, usage)
        self.update_provider_state(provider_idx, &response);
        
        Ok(response)
    }
}
```

### stoolap Storage (RFC-0903-B1/C1)

```rust
// storage/src/lib.rs

#[derive(Clone)]
pub struct KeyStorage {
    db: Arc<stoolap::Database>,
}

impl KeyStorage {
    /// Validate API key and return key metadata
    pub async fn validate_key(&self, key_hash: &[u8; 32]) -> Result<ApiKey, KeyError>;
    
    /// Check if key has sufficient budget for request
    pub async fn check_budget(&self, key_id: &[u8; 16], cost: u64) -> Result<(), BudgetError>;
    
    /// Record spend event to ledger (RFC-0909)
    pub async fn record_spend(&self, event: &SpendEvent) -> Result<(), StorageError>;
    
    /// Get current balance for OCTO-W
    pub async fn get_balance(&self, key_id: &[u8; 16]) -> Result<u64, StorageError>;
}
```

### Mode Selection

Modes are determined at **compile time** via feature flags:

```bash
# Build for Python SDK users (any-llm mode)
cargo build --features "any-llm-mode" --release

# Build for proxy gateway (LiteLLM mode)
cargo build --features "litellm-mode,enterprise" --release

# Build with both modes (dual mode)
cargo build --features "full" --release
```

Runtime configuration supplements compile-time flags:

```yaml
# config.yaml for LiteLLM mode
mode: proxy
litellm_mode:
  host: "0.0.0.0"
  port: 8000
  master_key: "${MASTER_KEY}"

# config.yaml for any-llm mode  
mode: sdk
anyllm_mode:
  # No host/port - direct calls only
  default_provider: "openai"
```

### LiteLLM Compatibility Matrix

Based on `docs/research/any-llm-vs-litellm-comparison.md` Table 24:

| Feature | LiteLLM | this RFC (LiteLLM Mode) | any-llm | this RFC (any-llm Mode) |
|---------|---------|------------------------|---------|------------------------|
| Drop-in OpenAI proxy | Yes | ✅ | No | N/A |
| Virtual API keys | Yes | ✅ (RFC-0903) | Basic | ✅ (RFC-0903) |
| Budget enforcement | Yes | ✅ (RFC-0904) | Yes | ✅ (RFC-0904) |
| Load balancing | Yes (6 strategies) | ✅ (RFC-0902) | No | ✅ (RFC-0902) |
| Fallback routing | Yes | ✅ (RFC-0902) | No | ✅ (RFC-0902) |
| Provider SDK delegation | Partial | ✅ (any-llm approach) | Yes | ✅ |
| 100+ providers | Yes | 10+ initially | 43 | 10+ initially |
| stoolap persistence | No | ✅ | No | ✅ |
| OCTO-W integration | No | ✅ | No | ✅ |

### Exception Parity

Both modes expose LiteLLM-compatible exceptions:

```python
# exceptions.py
class AuthenticationError(Exception): pass      # 401
class RateLimitError(Exception): pass           # 429
class BudgetExceededError(Exception): pass      # 402
class ProviderError(Exception): pass           # 502
class TimeoutError(Exception): pass             # 504
class InvalidRequestError(Exception): pass     # 400
class InternalError(Exception): pass           # 500
```

### Model String Formats

Both modes support multiple model string formats:

```python
# LiteLLM style (provider/model)
completion(model="openai/gpt-4o", messages=[...])
completion(model="anthropic/claude-opus-4", messages=[...])

# any-llm style (provider:model)  
completion(model="openai:gpt-4o", messages=[...])

# Explicit provider parameter (any-llm style)
completion(model="gpt-4o", provider="openai", messages=[...])

# Provider-embedded (mixed)
completion(model="mistral:mistral-small-latest", messages=[...])
```

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | LiteLLM proxy compatibility | 90%+ endpoint compatibility |
| G2 | any-llm SDK compatibility | 90%+ function signature match |
| G3 | Shared router | 100% RFC-0902 strategy support |
| G4 | stoolap persistence | RFC-0903-B1/C1 schema compliance |
| G5 | Feature-gated build | Zero overhead for disabled features |
| G6 | <10ms proxy latency | Gateway overhead |
| G7 | <50ms SDK call overhead | PyO3 boundary + router |

## Key Files to Modify

### Feature-Gated Structure

```
crates/quota-router-core/
├── src/
│   ├── lib.rs                 # Feature-gated module exports
│   ├── router.rs              # RFC-0902 router (always)
│   ├── providers/             # Provider abstraction (always)
│   │   ├── mod.rs
│   │   ├── openai.rs          # reqwest OpenAI
│   │   ├── anthropic.rs       # reqwest Anthropic
│   │   ├── ollama.rs          # reqwest Ollama
│   │   └── ...
│   ├── storage/               # stoolap storage (always)
│   │   └── mod.rs
│   ├── gateway/               # HTTP server
│   │   ├── mod.rs
│   │   ├── chat.rs
│   │   ├── embeddings.rs
│   │   ├── auth.rs
│   │   └── admin.rs
│   │   └── [feature = "litellm-mode"]
│   └── py_bridge/              # PyO3 bindings
│       ├── mod.rs
│       ├── completion.rs
│       └── exceptions.rs
│       └── [feature = "any-llm-mode"]
```

### Cargo Features

```toml
[features]
default = ["any-llm-mode"]
any-llm-mode = ["py-o3"]
litellm-mode = ["hyper", "axum", "tokio-tower"]
enterprise = ["any-llm-mode", "litellm-mode"]
full = ["enterprise"]

# Internal feature dependencies
py-o3 = ["dep:pyo3"]
hyper = ["dep:hyper", "dep:hyper-util"]
```

## Implementation Phases

### Phase 1: Shared Core

- [ ] Provider abstraction trait (`LLMProvider`)
- [ ] OpenAI provider implementation (`reqwest`)
- [ ] Anthropic provider implementation (`reqwest`)
- [ ] RFC-0902 router integration with providers
- [ ] stoolap storage layer (RFC-0903-B1/C1)

### Phase 2: any-llm Mode (SDK)

- [ ] PyO3 bridge module
- [ ] `completion()` binding that calls router directly
- [ ] LiteLLM-compatible exception types
- [ ] Model string parsing (provider:model format)
- [ ] Python SDK package structure

### Phase 3: LiteLLM Mode (Proxy)

- [ ] HTTP server module (`hyper`/`axum`)
- [ ] OpenAI-compatible endpoints (`/v1/chat/completions`, etc.)
- [ ] Auth middleware (API key validation)
- [ ] Admin endpoints for key/budget management
- [ ] Prometheus metrics endpoint

### Phase 4: Enterprise Features

- [ ] Virtual key management (per RFC-0903)
- [ ] Budget enforcement (per RFC-0904)
- [ ] Rate limiting (per RFC-0902)
- [ ] Usage tracking and analytics

## Alternatives Considered

### Alternative A: LiteLLM Python Fork

Fork LiteLLM and add Rust/stoolap integration.

**Rejected:** Maintenance burden, Python-only, complex merge conflicts.

### Alternative B: Pure HTTP (LiteLLM approach)

Reimplement all provider HTTP clients in Rust.

**Rejected:** Maintenance burden, protocol drift risk, violates any-llm's correctness insight.

### Alternative C: Single Feature Gate

Use single `proxy` feature instead of dual `litellm-mode`/`any-llm-mode`.

**Rejected:** Cannot simultaneously support SDK mode and proxy mode in same binary. User segments need both options available.

## Rationale

1. **Feature gates enforce compile-time separation** — no runtime branching overhead for disabled features
2. **any-llm approach for provider correctness** — official SDKs where available, reduces maintenance
3. **LiteLLM interface for ecosystem compatibility** — drop-in replacement for existing tooling
4. **Shared router ensures consistency** — both modes use same routing logic (RFC-0902)
5. **stoolap replaces Redis/PostgreSQL** — simpler deployment, deterministic storage

## Version History

| Version | Date       | Changes |
|---------|------------|---------|
| 2.0     | 2026-04-21 | Revised with Rust feature gates, dual-mode emphasis |
| 1.0     | 2026-04-21 | Initial draft |

## Related RFCs

- RFC-0902: Multi-Provider Routing and Load Balancing
- RFC-0903: Virtual API Key System
- RFC-0903-B1: Schema Amendments (spend_ledger BLOB)
- RFC-0903-C1: Extended Schema Amendments (api_keys/teams BLOB)
- RFC-0904: Real-Time Cost Tracking
- RFC-0906: Response Caching
- RFC-0907: Configuration Management
- RFC-0908: Python SDK and PyO3 Bindings
- RFC-0909: Deterministic Quota Accounting
- RFC-0910: Pricing Table Registry

## Related Use Cases

- Enhanced Quota Router Gateway

## Related Research

- `docs/research/any-llm-vs-litellm-comparison.md`
- `docs/research/litellm-analysis-and-quota-router-comparison.md`

---

**Submission Date:** 2026-04-21
**Last Updated:** 2026-04-21
