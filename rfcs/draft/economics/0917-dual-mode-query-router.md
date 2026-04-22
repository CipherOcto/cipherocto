# RFC-0917 (Economics): Dual-Mode Query Router — LiteLLM-Style HTTP Forwarding + any-llm-Style SDK Delegation

## Status

Draft (v2.6 — Round 6: fix remaining issues; streaming spec complete; provider-list parsing; RetryPolicy; dual sync/async API; idempotency keys)

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu

## Summary

Define a dual-mode query router that operates under Rust feature gates: **LiteLLM Mode** (native Rust HTTP forwarding to provider REST APIs, like LiteLLM's custom HTTP clients) and **any-llm Mode** (Python SDK delegation via PyO3 to official provider SDKs, like any-llm's delegation approach). The modes differ in **how providers are called**, not in which interface is exposed. Both modes can serve clients via HTTP proxy (OpenAI-compatible endpoints) and via Python SDK (`pip install`). Enterprise features (virtual keys, budgets, rate limiting, Prometheus, RFC-0903/0904/0909/0910) are available in **both** modes. The mode gate controls whether providers are called via native Rust HTTP (`reqwest`) or via official Python SDKs through PyO3.

## Motivation

### Research Foundation

Based on `docs/research/any-llm-vs-litellm-comparison.md`:

**LiteLLM** (BerriAI) is a mature production gateway used by Stripe, Google, Netflix. Its defining characteristic is **reimplementing provider HTTP clients internally** — it does NOT delegate to official provider SDKs. It exposes both a Python SDK and an HTTP proxy, with full enterprise features.

**any-llm** (Mozilla AI) is a lean correctness-first SDK that **delegates to official provider SDKs** (Anthropic SDK, OpenAI SDK, etc.). It has no router, no fallback, but maximum protocol correctness via SDK delegation. It exposes a Python SDK with an optional FastAPI gateway.

**CipherOcto Opportunity:** The dual-mode distinction should mirror the architectural difference between the reference implementations:
- **LiteLLM Mode:** Native Rust HTTP forwarding (like LiteLLM's custom HTTP approach, but in Rust) — no Python SDK dependency for provider calls, protocol control, lightweight
- **any-llm Mode:** Python SDK delegation via PyO3 (like any-llm's SDK delegation approach) — maximum correctness via official SDKs, familiar Python API

Both modes expose identical interfaces (HTTP proxy + Python SDK) and identical enterprise features. The only difference is the **provider integration strategy**.

### The Dual-Mode Concept

The dual-mode architecture differentiates **how providers are called**, not which client interface is exposed:

| Dimension | LiteLLM Mode | any-llm Mode |
|-----------|--------------|--------------|
| Provider integration | Native Rust HTTP forwarding (`reqwest`) | Python SDK delegation (PyO3 → official SDKs) |
| Reference approach | LiteLLM's custom HTTP clients | any-llm's SDK delegation |
| Python dependency | None for provider calls | Official provider SDKs (Anthropic, OpenAI, etc.) |
| Protocol control | Full (custom HTTP implementation) | Delegated to SDK |
| Correctness guarantee | Via audit + test | Via official SDK |

**Both modes expose identical interfaces:**

| Interface | Availability | Description |
|-----------|-------------|-------------|
| HTTP proxy | Both modes | OpenAI-compatible endpoints (`/v1/chat/completions`) |
| Python SDK | Both modes | `pip install quota_router` → `completion()` |

**Both modes share identical enterprise features:**
- Virtual API keys (RFC-0903)
- Budget enforcement (RFC-0904)
- Rate limiting (RFC-0902)
- Deterministic quota accounting (RFC-0909)
- Pricing table registry (RFC-0910)
- Prometheus metrics
- OCTO-W balance (RFC-0900)
- stoolap persistence (RFC-0903-B1/C1)

The mode gate does NOT control interface (HTTP vs SDK) or enterprise features — it controls **how providers are called internally**.

### Architectural Diagram

```mermaid
flowchart TB
    subgraph Interface["Interface Layer (per-feature)"]
        direction TB
        HTTP[HTTP Proxy<br/>/v1/chat/completions<br/>litellm-mode OR full]
        SDK[Python SDK<br/>completion() / acompletion()<br/>any-llm-mode OR full]
    end

    subgraph LiteLLM["LiteLLM Mode (reqwest HTTP — litellm-mode OR full)"]
        direction TB
        LMR[Router] --> LMH[reqwest HTTP<br/>Native Rust → Provider REST APIs]
    end

    subgraph AnyLLM["any-llm Mode (Python SDK — any-llm-mode OR full)"]
        direction TB
        AMR[Router] --> AMP[PyO3 Bridge<br/>Python SDKs: Anthropic·OpenAI·Mistral·etc.]
    end

    subgraph Shared["Shared Core (always compiled)"]
        direction TB
        Enterprise[Enterprise: Keys·Budgets·Rate Limits·Metrics]
        Storage[stoolap RFC-0903-B1/C1]
        Router[RFC-0902 Router<br/>6 routing strategies]
    end

    Interface --> Shared
    Shared --> LiteLLM
    Shared --> AnyLLM

    classDef gate fill:#fff3cd
    classDef shared fill:#e1f5fe
    classDef interface fill:#f0fff0
```

**Key architectural point:** The `Shared` core is always compiled. The **interface** (HTTP proxy vs Python SDK) and the **provider strategy** (reqwest HTTP vs Python SDK) are selected by the feature gate. These are **mutually exclusive per mode** — `litellm-mode` gives you HTTP proxy + reqwest; `any-llm-mode` gives you Python SDK + PyO3 bridge; `full` gives you both interfaces and both strategies simultaneously.

**Mode gates:**

```toml
# Cargo.toml (quota-router-core)
[features]
default = ["full"]           # Both provider integration strategies + both interfaces
litellm-mode = ["hyper", "axum"]  # HTTP proxy + reqwest HTTP (no Python SDK)
any-llm-mode = ["py-o3"]    # Python SDK + PyO3 bridge (no HTTP proxy)
full = ["litellm-mode", "any-llm-mode"]  # Both interfaces AND both strategies
```

**What each mode builds:**

| Feature | `litellm-mode` | `any-llm-mode` | `full` |
|---------|---------------|----------------|-------|
| Native Rust HTTP (`reqwest`) | ✅ | ❌ | ✅ |
| Python SDK delegation (PyO3) | ❌ | ✅ | ✅ |
| HTTP proxy interface (`hyper`/`axum`) | ✅ | ❌ | ✅ |
| Python SDK interface (`py-o3`) | ❌ | ✅ | ✅ |
| Enterprise features | ✅ | ✅ | ✅ |
| stoolap storage | ✅ | ✅ | ✅ |

### Rust Feature Gates

The dual-mode architecture uses Cargo feature gates to select the **provider integration strategy**. Both modes also include HTTP proxy + Python SDK interfaces (not gated):

```toml
# Cargo.toml (quota-router-core)
[features]
default = ["full"]           # Both provider integration strategies + both interfaces
litellm-mode = ["hyper", "axum"]  # Native Rust HTTP forwarding (no Python SDK deps for providers)
any-llm-mode = ["py-o3"]    # Python SDK delegation via PyO3 (official provider SDKs)
full = ["litellm-mode", "any-llm-mode"]  # Both provider strategies

# Interfaces (always included, not gated):
# - HTTP proxy: hyper + axum (always compiled)
# - Python SDK: py-o3 (always compiled when any-llm-mode or full)
```

**What each feature controls (provider integration strategy, not interface):**

| Feature | Provider Integration | Python Provider SDKs |
|---------|--------------------|--------------------|
| `litellm-mode` | Native Rust HTTP (`reqwest`) to provider REST APIs | ❌ None |
| `any-llm-mode` | Python SDK delegation via PyO3 (Anthropic, OpenAI, Mistral, etc.) | ✅ Via PyO3 |
| `full` (default) | Both strategies simultaneously | Both |

**Interfaces (compiled per feature flag, not shared):**

| Interface | `litellm-mode` | `any-llm-mode` | `full` |
|-----------|:--------------:|:---------------:|:------:|
| HTTP proxy (`/v1/chat/completions`) | ✅ | ❌ | ✅ |
| Python SDK (`pip install`) | ❌ | ✅ | ✅ |

**Note:** `hyper`/`axum` for the HTTP proxy and `pyo3` for the Python SDK are compiled **only** when the respective feature is enabled. The `litellm-mode` / `any-llm-mode` gate controls which interface is available AND which provider integration strategy is used. `full` is required for both interfaces to coexist in one binary.

## Scope

### In Scope

#### Feature-Gated Components

| Component | Feature Gate | Description |
|-----------|-------------|-------------|
| Native HTTP Forwarding | `litellm-mode` | `reqwest`-based HTTP calls to provider REST APIs (Rust, no Python SDK deps) |
| Python SDK Delegation | `any-llm-mode` | PyO3 bridge calling official Python SDKs (Anthropic, OpenAI, Mistral, etc.) |
| HTTP Proxy Server | (always with litellm-mode) | `hyper`/`axum` OpenAI-compatible proxy endpoints |
| Python SDK Interface | (always with any-llm-mode) | PyO3 bindings for `pip install` Python SDK |
| Shared Router | (none) | RFC-0902 router + all 6 routing strategies |
| Enterprise Features | (none) | Virtual keys, budgets, rate limiting, Prometheus, RFC-0903/0904/0909/0910 |
| stoolap Storage | (none) | RFC-0903-B1/C1 persistence |

#### Provider Integration Strategy

The dual-mode architecture differentiates **how providers are called**, not which interface is exposed:

**LiteLLM Mode — Native Rust HTTP Forwarding:**

```
Router → reqwest HTTP → Provider REST API (OpenAI, Anthropic, Mistral, etc.)
```

Like LiteLLM's approach: custom HTTP implementation in Rust for protocol control. No Python dependency for provider calls. Single HTTP stack (`reqwest`) for all providers.

| Provider | LiteLLM Mode Implementation | Notes |
|----------|--------------------------|-------|
| OpenAI | `reqwest` | REST API — chat completions, embeddings |
| Anthropic | `reqwest` | REST API — messages, embeddings |
| Mistral | `reqwest` | REST API — official Mistral API |
| Ollama | `reqwest` | REST API — local and remote Ollama |
| Google (Gemini) | `reqwest` | REST API — Vertex AI or maker suite |
| Azure OpenAI | `reqwest` | REST API — Azure-hosted models |
| AWS Bedrock | `reqwest` | REST API — Claude, Llama via Bedrock |

**any-llm Mode — Python SDK Delegation:**

```
Router → PyO3 Bridge → Official Python SDK (Anthropic, OpenAI, Mistral, etc.)
```

Like any-llm's approach: delegation to official provider SDKs for maximum HTTP transport correctness. The PyO3 bridge calls into Python SDKs that handle HTTP internally.

| Provider | any-llm Mode Implementation | Notes |
|----------|----------------------------|-------|
| OpenAI | `openai` Python SDK | Official OpenAI SDK |
| Anthropic | `anthropic` Python SDK | Official Anthropic SDK |
| Mistral | `mistralai` Python SDK | Official Mistral SDK |
| Ollama | `ollama` Python SDK | Official Ollama SDK |
| Google (Gemini) | `google-genai` Python SDK | Official Google SDK |
| Azure OpenAI | `openai` Python SDK (Azure endpoint) | Official SDK with Azure config |
| AWS Bedrock | `boto3` + `botocore` | Official AWS SDK |

> **Why two strategies?** LiteLLM Mode's native HTTP is lightweight (no Python dependency for providers). any-llm Mode's SDK delegation is correct by construction (official SDK owns HTTP transport). Both are available; the mode gate selects which is used.

#### LiteLLM Mode: Native HTTP Forwarding

LiteLLM Mode calls providers via native Rust HTTP (`reqwest`). Available interfaces: HTTP proxy and Python SDK.

**Via HTTP proxy:**
```mermaid
sequenceDiagram
    participant Client as HTTP Client<br/>(any language)
    participant Gateway as quota-router HTTP Proxy
    participant Auth as Auth Middleware
    participant Router as Rust Router
    participant HTTP as reqwest HTTP<br/>(LiteLLM Mode)
    participant Provider as LLM Provider
    participant Storage as stoolap

    Client->>Gateway: POST /v1/chat/completions<br/>Authorization: Bearer sk-...
    Gateway->>Auth: Validate API key (RFC-0903)
    Auth->>Storage: Check virtual key + budget (RFC-0904)
    Storage-->>Auth: OK / Budget exceeded
    Auth->>Router: Route + check rate limits (RFC-0902)
    Router->>HTTP: reqwest HTTP request
    HTTP->>Provider: Provider REST API
    Provider-->>HTTP: LLM Response
    HTTP-->>Router: Response
    Router->>Storage: Record spend (RFC-0909)
    Router-->>Gateway: OpenAI-formatted response
    Gateway-->>Client: HTTP 200
```

**Via Python SDK:**
```python
# LiteLLM Mode — Python SDK (PyO3) with native HTTP forwarding
from quota_router import completion

# Providers called via reqwest (native Rust HTTP), not Python SDKs
response = completion(model="openai/gpt-4o", messages=[...])
```

#### any-llm Mode: Python SDK Delegation

any-llm Mode calls providers via official Python SDKs through PyO3. Available interfaces: HTTP proxy and Python SDK.

**Via Python SDK:**
```python
# any-llm Mode — Python SDK with official SDK delegation
from quota_router import completion

# Providers called via official Python SDKs (Anthropic, OpenAI, etc.)
# through PyO3 bridge — not reqwest
response = completion(model="anthropic/claude-opus-4", messages=[...])
```

**Via HTTP proxy:**
```mermaid
sequenceDiagram
    participant Client as HTTP Client
    participant Gateway as quota-router HTTP Proxy
    participant Auth as Auth Middleware
    participant Router as Rust Router
    participant SDK as PyO3 Bridge<br/>(any-llm Mode)
    participant ProviderSDK as Official Python SDK<br/>(Anthropic·OpenAI·Mistral)
    participant Provider as Provider API
    participant Storage as stoolap

    Client->>Gateway: POST /v1/chat/completions<br/>Authorization: Bearer sk-...
    Gateway->>Auth: Validate API key (RFC-0903)
    Auth->>Storage: Check budget (RFC-0904)
    Storage-->>Auth: OK
    Auth->>Router: Route + check rate limits (RFC-0902)
    Router->>SDK: PyO3 call
    SDK->>ProviderSDK: Official SDK call
    ProviderSDK->>Provider: Provider API
    Provider-->>ProviderSDK: Response
    ProviderSDK-->>SDK: SDK Response
    SDK-->>Router: Normalized response
    Router->>Storage: Record spend (RFC-0909)
    Router-->>Gateway: OpenAI-formatted response
    Gateway-->>Client: HTTP 200
```

**Both interfaces in both modes enforce all enterprise features identically:** virtual keys (RFC-0903), budgets (RFC-0904), rate limits (RFC-0902), spend ledger (RFC-0909), Prometheus metrics.

### Out of Scope

- Implementing all 100+ LiteLLM providers from scratch
- LiteLLM Python SDK compatibility (only LiteLLM interface contract)
- Cloud-hosted SaaS deployment
- Non-Python language bindings

## Specification

### Feature Gate Architecture

```rust
// quota-router-core/src/lib.rs

// Provider integration strategies (mutually exclusive per provider call path):
#[cfg(feature = "litellm-mode")]
pub mod native_http;  // reqwest HTTP forwarding — LiteLLM Mode

#[cfg(feature = "any-llm-mode")]
pub mod py_bridge;    // PyO3 → official Python SDKs — any-llm Mode

// Interface layers (available in both modes):
pub mod gateway;      // HTTP proxy server (hyper/axum)
pub mod python_sdk;   // Python SDK bindings (PyO3)

// Shared core (always compiled):
pub mod router;       // RFC-0902 router
pub mod storage;      // stoolap storage
pub mod enterprise;    // Virtual keys, budgets, rate limiting, metrics
```

### Provider Abstraction Layer

**C2 Resolution:** The `LLMProvider` trait cannot be unified because `reqwest` HTTP and Python SDK delegation produce **different HTTP requests** for the same provider (e.g., Anthropic's SDK does automatic message format conversion that `reqwest` code cannot replicate). The trait must be **feature-gated** per integration strategy.

```rust
// providers/mod.rs

// =====================================================================
// LITEllm MODE: Native Rust HTTP via reqwest
// =====================================================================
#[cfg(feature = "litellm-mode")]
pub mod native_http {
    use async_trait::async_trait;

    /// Provider interface for LiteLLM Mode (reqwest HTTP forwarding)
    #[async_trait]
    pub trait HttpProvider: Send + Sync {
        fn name(&self) -> &str;
        fn supported_models(&self) -> Vec<&str>;
        fn supports_model(&self, model: &str) -> bool {
            self.supported_models().iter().any(|m| *m == model)
        }
        async fn completion(
            &self,
            request: &HttpCompletionRequest,
        ) -> Result<HttpCompletionResponse, ProviderError>;
        async fn embedding(
            &self,
            request: &HttpEmbeddingRequest,
        ) -> Result<HttpEmbeddingResponse, ProviderError>;
        fn routing_weight(&self) -> u32;
    }

    // reqwest-based provider implementations
    pub mod openai;        // Native Rust HTTP → OpenAI REST API
    pub mod anthropic;     // Native Rust HTTP → Anthropic REST API
    pub mod mistral;       // Native Rust HTTP → Mistral REST API
    pub mod ollama;        // Native Rust HTTP → Ollama REST API
    pub mod gemini;        // Native Rust HTTP → Google Gemini REST API
    pub mod azure;         // Native Rust HTTP → Azure OpenAI REST API
    pub mod bedrock;      // Native Rust HTTP → AWS Bedrock REST API
}

// =====================================================================
// ANY-LLM MODE: Python SDK delegation via PyO3
// =====================================================================
#[cfg(feature = "any-llm-mode")]
pub mod py_providers {
    use async_trait::async_trait;

    /// Provider interface for any-llm Mode (Python SDK delegation)
    /// Different from HttpProvider — wraps official Python SDKs, not REST APIs
    #[async_trait]
    pub trait SdkProvider: Send + Sync {
        fn name(&self) -> &str;
        fn supported_models(&self) -> Vec<&str>;
        fn supports_model(&self, model: &str) -> bool {
            self.supported_models().iter().any(|m| *m == model)
        }
        async fn completion(
            &self,
            request: &SdkCompletionRequest,
        ) -> Result<SdkCompletionResponse, ProviderError>;
        async fn embedding(
            &self,
            request: &SdkEmbeddingRequest,
        ) -> Result<SdkEmbeddingResponse, ProviderError>;
        fn routing_weight(&self) -> u32;
    }

    // Python SDK wrappers (called via PyO3)
    pub mod openai_sdk;      // wraps official openai Python SDK
    pub mod anthropic_sdk;   // wraps official anthropic Python SDK
    pub mod mistral_sdk;     // wraps official mistralai Python SDK
    pub mod ollama_sdk;      // wraps official ollama Python SDK
}

// =====================================================================
// FULL BUILD: Dynamic dispatch to either strategy
// =====================================================================
#[cfg(feature = "full")]
pub mod dynamic {
    use async_trait::async_trait;

    /// Unified provider handle for full builds (both strategies available)
    /// Routes to either HttpProvider or SdkProvider based on model/provider config
    pub enum ProviderHandle {
        Http(Box<dyn HttpProvider>),
        Sdk(Box<dyn SdkProvider>),
    }

    pub trait HttpProvider: Send + Sync { /* ... */ }
    pub trait SdkProvider: Send + Sync { /* ... */ }
}
```

**Key difference:** `HttpCompletionRequest` and `SdkCompletionRequest` are **different types**. The HTTP variant encodes the raw request parameters that `reqwest` sends to the REST API. The SDK variant encodes the parameters that the Python SDK accepts — which the SDK internally translates to the HTTP request. These translations differ (e.g., Anthropic SDK does message format conversion), so the request types cannot be unified.

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

// Router is shared at the gateway level, not created per-request
lazy_static::lazy_static! {
    static ref ROUTER: Arc<Router> = Router::new(config.clone());
}

async fn chat_completions(
    req: ChatCompletionRequest,
    auth_header: Authorization,
) -> Result<ChatCompletionResponse, GatewayError> {
    // 1. Validate auth header → extract key_id
    let api_key = validate_key(&auth_header)?;

    // 2. Route via shared router (connection pools preserved)
    let response = ROUTER.route_and_forward(req).await?;

    // 3. Record usage in storage (deduct from budget, record spend event)
    record_spend(&api_key.key_id, &response).await?;

    Ok(response)
}
```

**Optimizations applied (A6 + A7 FIXED):**
- **A6 FIXED:** Storage called twice (budget check in auth + record_spend after) — not three times. Budget check is implicit in `record_spend` (insufficient balance returns error before recording).
- **A7 FIXED:** Router shared via `Arc<Router>` at gateway level. Connection pools, latency tracking, and round-robin state persist across requests.

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

    // Route via shared router using global singleton with interior mutability
    // Router::global() returns Arc<Router>, so .route_and_forward() takes &self
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
    // Interior mutability for thread-safe shared state
    state: RwLock<RouterState>,
}

struct RouterState {
    // Provider connection pools, latency tracking, RPM/TPM counters
    connection_pools: HashMap<String, Pool>,
    latency_tracker: LatencyTracker,
    round_robin_index: usize,
}

impl Router {
    /// Route request to appropriate provider
    /// Uses strategy from RFC-0902 (simple-shuffle, least-busy, latency-based, etc.)
    ///
    /// Uses interior mutability (&self) so Router::global() singleton works safely.
    pub async fn route_and_forward(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, RouterError> {
        // 1. Select provider based on routing strategy
        let provider_idx = {
            let state = self.state.read().await;
            self.route_with_strategy(&state, &request.model)?
        };

        // 2. Get provider implementation
        let provider = self.provider_impls
            .get(&request.provider)
            .ok_or(RouterError::UnknownProvider)?;

        // 3. Forward request
        let response = provider.completion(&request).await?;

        // 4. Update provider state (latency, usage) via interior mutability
        self.update_provider_state(provider_idx, &response).await;

        Ok(response)
    }

    /// Shared global router for SDK mode (PyO3 bridge)
    pub fn global() -> Arc<Self> { /* ... */ }
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

    /// Get current OCTO-W balance for marketplace settlement
    pub async fn get_octo_w_balance(&self, key_id: &[u8; 16]) -> Result<u64, StorageError>;
}
```

### Mode Selection

Modes are determined at **compile time** via feature flags. Enterprise features are always included (built into the shared core):

```bash
# Build for Python SDK only (any-llm mode)
cargo build --features "any-llm-mode" --release

# Build for HTTP proxy only (LiteLLM mode)
cargo build --features "litellm-mode" --release

# Build with both interfaces (default)
cargo build --features "full" --release  # any-llm-mode + litellm-mode
```

**What gets compiled:**

| Build | HTTP Proxy (`:8000`) | Python SDK (`pip install`) | Enterprise Features |
|-------|---------------------|---------------------------|---------------------|
| `any-llm-mode` | ❌ | ✅ | ✅ (always) |
| `litellm-mode` | ✅ | ❌ | ✅ (always) |
| `full` (default) | ✅ | ✅ | ✅ (always) |

**Runtime configuration:**

```yaml
# config.yaml — applies to whichever interface(s) are compiled in
mode: both              # 'both', 'proxy', or 'sdk'
proxy:
  host: "0.0.0.0"
  port: 8000
  master_key: "${MASTER_KEY}"
sdk:
  default_provider: "openai"
  # SDK is always available when compiled in full mode
```

For `full` build (both interfaces):
- `mode: both` — HTTP server running + Python SDK importable
- `mode: proxy` — HTTP server only
- `mode: sdk` — HTTP server disabled, SDK only

### LiteLLM Compatibility Matrix

Based on `docs/research/any-llm-vs-litellm-comparison.md`. The dual-mode distinction is **provider integration strategy** (native HTTP vs SDK delegation) AND **which interface is compiled** (HTTP vs Python SDK).

| Feature | LiteLLM | this RFC (LiteLLM Mode) | any-llm | this RFC (any-llm Mode) |
|---------|---------|------------------------|---------|------------------------|
| Provider integration | Custom HTTP (Python) | Native Rust HTTP (`reqwest`) | Official SDKs | Python SDK delegation (PyO3) |
| OpenAI-compatible API (HTTP) | Yes | ✅ | No | ❌ (requires `full` build) |
| Python SDK (`pip install`) | Yes | ❌ (requires `full` build) | Yes | ✅ |
| Virtual API keys | Yes | ✅ (RFC-0903) | Basic | ✅ (RFC-0903) |
| Budget enforcement | Yes | ✅ (RFC-0904) | Yes | ✅ (RFC-0904) |
| Load balancing | Yes (6 strategies) | ✅ (RFC-0902) | No | ✅ (RFC-0902) |
| Fallback routing | Yes | ✅ (RFC-0902) | No | ✅ (RFC-0902) |
| 100+ providers | Yes | 10+ initially | 43 | 10+ initially |
| stoolap persistence | No | ✅ | No | ✅ |
| OCTO-W integration | No | ✅ | No | ✅ |
| Prometheus metrics | Yes | ✅ | Yes | ✅ |
| Streaming support | Yes | ✅ | Yes | ✅ |

**Interface parity:** Enterprise features are identical across both modes. The interfaces differ: LiteLLM Mode exposes HTTP proxy; any-llm Mode exposes Python SDK. The `full` build exposes both interfaces. The only difference in provider integration is how providers are called internally.

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
| G1 | HTTP proxy compatibility (LiteLLM Mode) | 90%+ endpoint compatibility |
| G2 | Python SDK compatibility (any-llm Mode) | 90%+ function signature match |
| G3 | Shared enterprise features | Identical feature set in both modes |
| G4 | HTTP forwarding correctness | REST API equivalence to official SDKs |
| G5 | Feature-gated build | Zero overhead for disabled interface |
| G6 | <10ms proxy latency | LiteLLM Mode gateway overhead |
| G7 | <50ms SDK call overhead | PyO3 boundary + router |
| G8 | Binary size | <15MB for single-mode, <25MB for `full` |
| G9 | Python wheel size | <10MB for any-llm-mode |

## Key Files to Modify

### Feature-Gated Structure

**Note:** Provider implementations are **mutually exclusive** per feature gate. The `providers/` tree is for LiteLLM Mode (reqwest HTTP), and the `py_bridge/` tree is for any-llm Mode (Python SDK delegation). These cannot coexist in a single-provider-call path — the mode gate determines which provider tree is active.

```
crates/quota-router-core/
├── src/
│   ├── lib.rs                 # Feature-gated module exports
│   ├── router.rs              # RFC-0902 router (always)
│   ├── providers/             # [feature = "litellm-mode"] ONLY — reqwest HTTP implementations
│   │   ├── mod.rs
│   │   ├── openai.rs          # reqwest OpenAI REST
│   │   ├── anthropic.rs       # reqwest Anthropic REST
│   │   ├── mistral.rs         # reqwest Mistral REST
│   │   ├── ollama.rs          # reqwest Ollama REST
│   │   ├── gemini.rs          # reqwest Google Gemini REST
│   │   ├── azure.rs           # reqwest Azure OpenAI REST
│   │   └── bedrock.rs         # reqwest AWS Bedrock REST
│   ├── py_bridge/              # [feature = "any-llm-mode"] ONLY — Python SDK wrappers
│   │   ├── mod.rs
│   │   ├── completion.rs      # PyO3 completion bridge
│   │   ├── embeddings.rs      # PyO3 embeddings bridge
│   │   ├── exceptions.rs      # LiteLLM-compatible exceptions
│   │   ├── providers/          # Python SDK wrappers (PyO3-callable)
│   │   │   ├── openai_sdk.rs   # wraps official openai Python SDK
│   │   │   ├── anthropic_sdk.rs # wraps official anthropic Python SDK
│   │   │   ├── mistral_sdk.rs  # wraps official mistralai Python SDK
│   │   │   └── ollama_sdk.rs   # wraps official ollama Python SDK
│   │   └── [feature = "any-llm-mode"]
│   ├── storage/               # stoolap storage (always)
│   │   └── mod.rs
│   └── gateway/               # HTTP server [feature = "litellm-mode" OR "full"]
│       ├── mod.rs
│       ├── chat.rs
│       ├── embeddings.rs
│       ├── auth.rs
│       └── admin.rs
```

**Mutual exclusivity note:** `providers/` (reqwest HTTP) and `py_bridge/` (Python SDK) are compiled mutually exclusively. A `litellm-mode` build uses `providers/`. An `any-llm-mode` build uses `py_bridge/`. A `full` build can include both but only one provider strategy is active per request (selected at routing time if both are available in `full`).

### Cargo Features

```toml
[features]
default = ["full"]           # Both provider integration strategies
litellm-mode = ["hyper", "axum"]  # Native Rust HTTP forwarding (reqwest)
any-llm-mode = ["py-o3"]    # Python SDK delegation via PyO3
full = ["litellm-mode", "any-llm-mode"]  # Both strategies

# Interface layers (always available when respective mode is enabled):
hyper = ["dep:hyper", "dep:hyper-util", "dep:axum"]
py-o3 = ["dep:pyo3", "dep:pyo3-ffi"]
```

**Enterprise features and interfaces (HTTP proxy + Python SDK) are always included.** The `litellm-mode` / `any-llm-mode` gates control which **provider integration strategy** is compiled in (native HTTP or Python SDK delegation).

## Implementation Phases

**Enterprise features are part of the shared core — implemented once, available to both modes.**

### Phase 1: Shared Core

- [ ] RFC-0902 router with all 6 routing strategies
- [ ] stoolap storage layer (RFC-0903-B1/C1 schema)
- [ ] Virtual API key validation (RFC-0903)
- [ ] Budget enforcement (RFC-0904)
- [ ] Rate limiting (RFC-0902)
- [ ] Deterministic quota accounting (RFC-0909)
- [ ] Prometheus metrics endpoint

### Phase 2: LiteLLM Mode — Native Rust HTTP Forwarding

- [ ] `native_http` module: reqwest HTTP forwarding to all provider REST APIs
- [ ] Provider implementations: OpenAI, Anthropic, Mistral, Ollama, Gemini, Azure, Bedrock
- [ ] HTTP proxy server (`hyper`/`axum`) on configurable host/port
- [ ] OpenAI-compatible endpoints (`/v1/chat/completions`, `/v1/embeddings`, `/v1/models`)
- [ ] Auth middleware (API key validation)
- [ ] Admin endpoints for key/budget management

### Phase 3: any-llm Mode — Python SDK Delegation

- [ ] PyO3 bridge module calling official Python SDKs
- [ ] Provider SDK integrations: `anthropic`, `openai`, `mistralai`, `ollama`, `google-genai`
- [ ] Python SDK interface (`pip install quota_router`)
- [ ] `completion()` / `acompletion()` / `embedding()` / `aembedding()`
- [ ] Streaming support (Python generator via PyO3)
- [ ] LiteLLM-compatible exception types
- [ ] `set_api_key()` — validates and registers key with storage
- [ ] `get_budget_status()` — returns current spend vs limit
- [ ] `get_metrics()` — returns Prometheus metrics dict
- [ ] Model string parsing (both `provider/model` and `provider:model` formats)

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

## Adversarial Review

### A1: PyO3 Cannot Bridge to Python SDKs from Rust

**Severity:** Critical (Architectural Contradiction)

**Finding:** The RFC originally stated Mistral uses "Python SDK via PyO3" (line 137), but PyO3 bridges go **from Rust to Python**, not from Rust to Python SDKs. You cannot call a Python SDK from Rust via PyO3 — you would need to embed a Python interpreter (CPython extension).

**Original contradiction:**
- Line 137: "Mistral | Python SDK via PyO3 | Official SDK"
- Line 249: `pub mod py_providers;  // Python SDK bridges via PyO3`
- Mermaid diagram: "Provider SDK Bridge | PyO3 → Rust → official provider SDKs"

If PyO3 bridges Python → Rust, calling a Python SDK from Rust requires embedding a Python interpreter.

**Resolution (FIXED):** Changed all providers to `reqwest` HTTP forwarding. The official provider REST APIs provide protocol-correct access equivalent to the Python SDKs. Any future "Python SDK bridge" would be a Python extension calling into Rust via PyO3 (Python→Rust direction), not Rust calling Python SDKs.

---

### A2: Feature Gate Location Mismatch

**Severity:** Critical (Implementation Blocker)

**Finding (ORIGINAL):** Feature gates were defined in `quota-router-core/Cargo.toml`, but `py_bridge` was placed in `quota-router-pyo3/` (a separate crate). Rust feature gates are per-crate, not cross-crate — so the `any-llm-mode` feature in `quota-router-core` could not control compilation of modules in `quota-router-pyo3`.

**Original problematic structure:**
```toml
# quota-router-core/Cargo.toml
[features]
any-llm-mode = ["py-o3"]  # References pyo3 but py_bridge is NOT in this crate!

# quota-router-pyo3/Cargo.toml  (SEPARATE CRATE)
pyo3 = { version = "0.21", features = ["extension-module", "experimental-async"] }
```

**Resolution (FIXED):** The `py_bridge` module now lives in `quota-router-core` as a module, not a separate crate:

```rust
// quota-router-core/src/lib.rs

#[cfg(feature = "any-llm-mode")]
pub mod py_bridge;  // Now in the SAME crate — feature gate works
```

The Cargo.toml for `quota-router-core` includes `pyo3` as a dependency gated by the `any-llm-mode` feature:

```toml
# quota-router-core/Cargo.toml
[features]
any-llm-mode = ["py-o3"]
py-o3 = ["dep:pyo3"]
```

The Python SDK wheel is built by placing `py_bridge` behind the `any-llm-mode` feature gate, ensuring feature-gated compilation works as intended.

---

### A3: Router `&mut self` Incompatible with Global Singleton

**Severity:** Critical (API Design Flaw)

**Finding (ORIGINAL):** The RFC's PyO3 bridge called `Router::global().route_and_forward(request)` but `route_and_forward` took `&mut self`. A global `Router` via `Arc<Mutex<>>` would require `&self`, not `&mut self`.

**Original problematic code:**
```rust
pub async fn route_and_forward(
    &mut self,  // <-- Mutex conflict with global singleton
    request: CompletionRequest,
) -> Result<CompletionResponse, RouterError>
```

**Resolution (FIXED):** Changed to interior mutability pattern with `&self`:

```rust
pub struct Router {
    config: RouterConfig,
    providers: HashMap<String, Arc<dyn LLMProvider>>,
    // Interior mutability for thread-safe shared state
    state: RwLock<RouterState>,
}

pub async fn route_and_forward(
    &self,  // <-- Now compatible with global Arc<Router> singleton
    request: CompletionRequest,
) -> Result<CompletionResponse, RouterError> {
    let provider_idx = {
        let state = self.state.read().await;
        self.route_with_strategy(&state, &request.model)?
    };
    // ...
}
```

`Router::global()` returns `Arc<Self>`, so calls use `&self` (immutable borrow). State that changes during routing (`round_robin_index`, `latency_tracker`, `connection_pools`) lives in `RouterState` protected by `RwLock`, allowing interior mutability within `&self`.

---

### A4: Streaming Not Specified

**Severity:** High (Missing Core Feature)

**Finding:** The RFC's LiteLLM compatibility table shows `stream: ✅` but does not specify how streaming works. LiteLLM uses Server-Sent Events (SSE) with `text/event-stream` content type. The RFC has no streaming specification.

**Missing:**
- SSE chunk format per provider
- How to handle provider-specific streaming differences (Anthropic uses different SSE format than OpenAI)
- How the PyO3 bridge handles streaming responses (yielding chunks vs returning complete response)
- Rate limiting interaction with streaming (per-token vs per-request)

**Resolution (FIXED):** Streaming specification below applies to LiteLLM Mode (HTTP proxy). any-llm Mode streaming is via Python SDK (see C3 fix).

#### Streaming Architecture

**HTTP Response:** `Content-Type: text/event-stream`

**SSE framing:** All chunks use the SSE `data:` prefix followed by JSON, terminated by `\n\n`.

#### LiteLLM Mode: HTTP Proxy Streaming (via tokio-tower SSE)

The HTTP proxy uses SSE for streaming responses (per LiteLLM compatibility requirement).

**SSE format (OpenAI-compatible):**
```
data: {"id":"chatcmpl-xxx","choices":[{"index":0,"delta":{"content":"token"},"finish_reason":null}]}\n\n
```

**Chunk termination:** `[DONE]` marker:
```
data: [DONE]\n\n
```

**Per-provider streaming differences:**

| Provider | SSE Format | Event Types | Notes |
|----------|-----------|-------------|-------|
| OpenAI | Standard SSE `data: {...}` | `delta.content` | Standard chat completions format |
| Anthropic | Standard SSE `data: {...}` | `type: content_block_delta`, `delta.type: text_delta` | SDK does message→content.blocks conversion |
| Mistral | Standard SSE | `delta.content` | OpenAI-compatible format |
| Ollama | Standard SSE | `delta.content` | OpenAI-compatible format |
| Gemini | Server-Sent Events | Provider-specific | Depends on API version |

**Anthropic SSE conversion:** The `reqwest`-based Anthropic implementation receives raw SSE events from the Anthropic API. These must be **transformed** to OpenAI-compatible SSE format before returning to the HTTP proxy client:

```rust
// anthropic.rs — SSE transformation for LiteLLM compatibility
async fn transform_anthropic_to_openai_sse(
    anthropic_stream: impl Stream<Item = SSEEvent>,
) -> impl Stream<Item = bytes::Bytes> {
    anthropic_stream.map(|event| {
        match event.event_type {
            "content_block_delta" => {
                let text = event.delta.text;  // Anthropic's text in delta
                let openai_chunk = format!(
                    r#"data: {{"choices":[{{"index":0,"delta":{{"content":"{}"}},"finish_reason":null}}]}}\n\n"#,
                    text
                );
                bytes::Bytes::from(openai_chunk)
            }
            "message_delta" => {
                // Final usage block
                let usage = format_usage(&event.usage);
                let done = r#"data: {"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#;
                bytes::Bytes::from(format!("{}\n\ndata: [DONE]\n\n", done))
            }
            _ => bytes::Bytes::new(),  // Skip other event types
        }
    })
}
```

**Rate limiting for streaming:** Per-request (not per-token). Budget is checked before streaming begins. If the first chunk would exceed budget, the request is rejected before any bytes are sent.

#### any-llm Mode: Python SDK Streaming

any-llm Mode uses the official Python SDKs' streaming APIs. Each provider SDK has its own streaming format internally — the PyO3 bridge receives Python objects and converts them to OpenAI-compatible format.

**Python SDK streaming interface:**

```python
# Python SDK (any-llm mode) — streaming response
def completion(model: str, messages: list, stream: bool = True, **kwargs):
    if stream:
        return streaming_response  # Python generator / async iterator
    else:
        return complete_response
```

**Streaming response types per provider:**

| Provider | SDK Streaming API | PyO3 Bridge Output |
|----------|------------------|-------------------|
| OpenAI | `openai.StreamingChatCompletion` | `Iterator[ChatCompletionChunk]` |
| Anthropic | `anthropic.MessageStream` | `AsyncIterator[Chunk]` → SSE via FastAPI |
| Mistral | `mistralai.StreamingChatCompletion` | `Iterator[ChatCompletionChunk]` |
| Ollama | `ollama.AsyncGenerate` | `AsyncIterator[GenerateResponse]` |

**Streaming via HTTP proxy (any-llm Mode, `full` build only):**

When any-llm Mode is compiled in a `full` build with the HTTP proxy enabled, streaming requires the PyO3 bridge to:
1. Call the Python SDK's streaming method
2. Receive Python chunk objects
3. Convert each chunk to OpenAI SSE format
4. Stream SSE bytes through the Rust HTTP response

This requires both `py-o3` (for calling Python SDKs) and `hyper` (for HTTP response streaming) simultaneously — only possible in `full` builds.

#### Per-Mode Streaming Availability

| Mode | Streaming via HTTP Proxy | Streaming via Python SDK |
|------|-------------------------|-------------------------|
| `litellm-mode` | ✅ SSE via tokio-tower | ❌ (no Python SDK) |
| `any-llm-mode` | ❌ (no hyper compiled) | ✅ Python generator |
| `full` | ✅ SSE (both bridges available) | ✅ Python generator |

**Note:** Streaming via HTTP proxy in any-llm Mode requires the `full` build (both hyper and py-o3 compiled).

---

### A5: Budget vs OCTO-W Relationship Ambiguous

**Severity:** High (Semantic Confusion)

**Finding (ORIGINAL):** The RFC conflated two distinct concepts:
- **RFC-0903 budgets:** Daily/weekly/monthly virtual limits per API key (`budget_limit` field in `api_keys` table — a *limit*, not a balance)
- **OCTO-W:** Token/currency balance for marketplace settlement (a *balance* from RFC-0900)

The confusion was in the storage interface:
```rust
pub async fn check_budget(&self, key_id: &[u8; 16], cost: u64) -> Result<(), BudgetError>;
pub async fn get_balance(&self, key_id: &[u8; 16]) -> Result<u64, StorageError>;  // OCTO-W
```

**Resolution (FIXED):** Separated into two distinct concepts with clear semantics:

**1. Budget Enforcement (per RFC-0903/0904):**
```rust
/// Check if adding `cost` would exceed the key's budget_limit
/// Uses the `budget_limit` from api_keys table (per RFC-0903)
pub async fn check_budget_limit(
    &self,
    key_id: &[u8; 16],
    cost: u64,
) -> Result<(), BudgetExceededError>;
```

**2. OCTO-W Balance (per RFC-0900/0902):**
```rust
/// Get current OCTO-W balance for marketplace settlement
/// OCTO-W is a separate balance from the budget limit
pub async fn get_octo_w_balance(&self, key_id: &[u8; 16]) -> Result<u64, StorageError>;

/// Deduct OCTO-W for pay-per-token marketplace calls
pub async fn deduct_octo_w(&self, key_id: &[u8; 16], amount: u64) -> Result<(), InsufficientBalanceError>;
```

**Two separate concepts:**
| Concept | Type | Source RFC | Field/Table |
|---------|------|------------|-------------|
| Budget limit | Limit (`$100/month`) | RFC-0903/0904 | `api_keys.budget_limit` |
| OCTO-W balance | Balance (tokens) | RFC-0900/0902 | `octo_w_ledger` |

---

### A6: Storage Called Twice in LiteLLM Mode

**Severity:** Medium (Inefficiency)

**Finding:** In LiteLLM Mode, the sequence diagram shows storage called twice:

```
Auth->>Storage: Check key + budget      [first call]
Router->>Storage: Record usage          [second call]
```

But the router also checks budget internally:

```
Router->>Storage: Check budget (async)  [third call]
```

**Three storage calls per request** in LiteLLM Mode:
1. Auth middleware checks budget
2. Router checks budget (redundant with #1)
3. Router records usage

**Resolution:** Collapse to two calls: (1) budget check in auth middleware before routing, (2) usage record after response. Remove budget check from router; let auth middleware reject before routing.

---

### A7: Connection Pooling Lost with Per-Request Router

**Severity:** Medium (Performance)

**Finding:** The HTTP request flow shows creating a new Router per request:

```rust
// RFC line 290
let mut router = Router::new(config.clone());
```

Creating a new `Router` per request loses:
- Provider connection pools (HTTP keepalive connections)
- Latency tracking state
- RPM/TPM counters
- Routing strategy state (round-robin index)

**Impact:** Every request establishes new HTTP connections to providers — significant latency overhead.

**Resolution:** Router should be shared via `Arc<Router>` at the gateway level, not created per request.

---

### A8: API Key Auth in any-llm Mode Not Specified

**Severity:** Medium (Security Gap)

**Finding:** LiteLLM Mode (Proxy) has auth middleware that validates API keys. any-llm Mode (SDK) has no auth specified — the PyO3 bridge just calls the router directly.

**Problem:** In any-llm Mode, if the SDK is deployed as a library in a user's Python app, API keys are passed directly to `completion()`:

```python
# any-llm mode
from quota_router import completion
response = completion(model="gpt-4", messages=[...], api_key="sk-...")
```

There's no auth enforcement — the SDK accepts any string as an API key.

**Resolution:** Either:
1. Require any-llm Mode to always go through a proxy (defeats the purpose)
2. Add API key validation in the PyO3 bridge layer
3. Clearly document that any-llm Mode is for trusted environments only

---

### A9: RFC Status Inconsistency

**Severity:** Low (Documentation)

**Finding:** RFC references non-Final RFCs without explicit status requirements:

| RFC | Referenced Status | Actual Status |
|-----|-------------------|---------------|
| RFC-0903-B1 | Required (line 25) | **Accepted** (just moved) |
| RFC-0903-C1 | Required (line 26) | **Accepted** (just moved) |
| RFC-0904 | Optional (line 30) | **Planned** |
| RFC-0909 | Optional (line 34) | **Final** |

**Issue:** RFC-0909 (Final) is marked optional but is required for `record_spend()` in storage. RFC-0904 (Planned) is needed for budget enforcement but is optional.

**Resolution:** Mark RFC-0909 as **Required** if storage `record_spend()` is part of the spec. Mark RFC-0904 as **Required** if budget enforcement is in scope for Phase 4.

---

### A10: PyO3 Experimental Async Flag

**Severity:** Low (Implementation Risk)

**Finding:** The RFC references `pyo3 = { version = "0.21", features = ["experimental-async"] }` for async support, but this is marked experimental in PyO3 0.21.

**Risk:** Experimental features may change behavior or have bugs. LiteLLM compatibility requires reliable async `acompletion()`.

**Resolution:** Consider using synchronous completion in PyO3 (run `tokio::runtime::Runtime` in the call) to avoid experimental async, or document this as an accepted risk.

---

### A11: Feature Gate Compilation Dependency

**Severity:** Low (Build System)

**Finding:** If `quota-router-pyo3` is distributed as a PyPI package, users install via `pip install quota-router-pyo3`. The Rust extension is pre-compiled and feature flags cannot be changed at install time.

**Impact:** Users cannot choose LiteLLM Mode vs any-llm Mode at install time — the feature is baked into the wheel.

**Resolution:** Document that:
1. `quota-router-core` with `any-llm-mode` is what gets compiled into the PyO3 wheel
2. LiteLLM Mode requires separate binary deployment (not pip-installable)
3. Or: distribute two separate wheels: `quota-router-sdk` and `quota-router-gateway`

---

### B1: HTTP Forwarding Not Central — Spec Says "Provider SDK delegation" but All Providers Use HTTP

**Severity:** High (Architectural Clarity)

**Finding:** The RFC's LiteLLM compatibility matrix claims "Provider SDK delegation: Partial ✅ (any-llm approach)" — implying delegation to official provider SDKs. But the actual provider table (lines 132-140) shows ALL providers using `reqwest` HTTP forwarding. No Python SDK is used in either mode.

**Contradiction:**
- Line 499: "Provider SDK delegation | Partial | ✅ (any-llm approach)"
- Lines 132-140: All 7 providers are `reqwest` HTTP forwarding
- Line 130: "Follow any-llm's insight: use official provider SDKs where available"

But there ARE NO provider SDKs in the implementation. The "any-llm approach" is the HTTP forwarding strategy (delegation via REST API equivalence), not actual SDK delegation.

**Impact:** The spec describes a capability that doesn't exist. Users expecting actual provider SDK delegation (like any-llm's official SDK approach) will be confused.

**Resolution:** Remove "Provider SDK delegation" from the matrix or change to "HTTP forwarding (REST API equivalence)". Clarify: "Both modes use reqwest HTTP forwarding to provider REST APIs. This provides protocol-correct access equivalent to official SDKs without the maintenance burden of SDK version management."

---

### B2: Enterprise Features Exposed Only to LiteLLM Mode — SDK Mode Has No Access

**Severity:** High (Feature Parity Gap)

**Finding (ORIGINAL):** Enterprise features (virtual keys, budgets, rate limiting, metrics) were only documented in the LiteLLM Mode context. any-llm Mode had no documented path to access these.

**Resolution (FIXED):** RFC rewritten to explicitly state that all enterprise features are in the shared core, available to both modes identically. any-llm Mode SDK now documents `set_api_key()`, `get_budget_status()`, `get_metrics()` — same enterprise features as LiteLLM Mode. The difference is only the interface (Python function call vs HTTP request), not the feature set.

---

### B3: Feature Gate `enterprise` Is Redundant with Mode Gates

**Severity:** Medium (Design Confusion)

**Finding (ORIGINAL):** `enterprise` was a separate feature gate from the mode gates, creating confusing 4-way build combinations where `any-llm-mode` alone had no enterprise features.

**Resolution (FIXED):** Removed the `enterprise` feature gate. Enterprise features (virtual keys, budgets, rate limiting, Prometheus, RFC-0903/0904/0909/0910) are in the shared core with no gate. Only `any-llm-mode` (Python SDK) and `litellm-mode` (HTTP proxy) are gated. Default (`full`) enables both interfaces.

---

### B4: HTTP Forwarding Is the Shared Core — but Not Named as Such

**Severity:** Medium (Architectural Clarity)

**Finding (ORIGINAL):** "HTTP forwarding to provider REST endpoints" was buried in the provider table and not highlighted as the fundamental architectural choice.

**Resolution (FIXED):** Added "HTTP Forwarding (Shared Core — Both Modes)" section explicitly stating both modes use the same `reqwest` HTTP forwarding mechanism. The distinction is only the interface layer (HTTP server vs Python SDK), not the provider communication.

---

### B5: `provider:model` Format Collision Between LiteLLM and any-llm Styles

**Severity:** Medium (UX Confusion)

**Finding:** The RFC supports multiple model string formats:

```python
# LiteLLM style (provider/model)
completion(model="openai/gpt-4o", ...)

# any-llm style (provider:model)
completion(model="openai:gpt-4o", ...)
```

But the slash (`/`) and colon (`:`) are both valid in model strings. If a provider actually uses either character in their model name (e.g., `anthropic/claude-opus-4-250624` — future versioned model), the parsing is ambiguous.

**Problem:** Which format takes priority? `openai/claude-3` could be "provider=openai, model=claude-3" (LiteLLM) or "model=openai/claude-3 with default provider" (any-llm).

**Resolution:** Define unambiguous parsing rules:
1. If string contains `:` → split on first `:` (any-llm style: `provider:model`)
2. If string contains `/` but no `:` → split on first `/` (LiteLLM style: `provider/model`)
3. If both `:` and `/` → reject as ambiguous

Also document that model names containing `:` or `/` are unsupported.

---

### B6: Dual-Mode Binary Size Not Specified

**Severity:** Low (Operations)

**Finding:** The RFC doesn't address the binary size implications of building with `full` (both modes + enterprise features). A binary with `hyper`, `axum`, `pyo3`, and all enterprise storage features could be 20+ MB.

**Missing:**
- Expected binary size for each feature combination
- Whether LiteLLM Mode can be deployed as a lean binary (without any-llm-mode)
- Whether any-llm-mode Python wheel size is documented

**Resolution:** Add binary size targets to Design Goals:

| G8 | Binary size | <15MB for liteLLM-mode only, <25MB for full |
| G9 | Python wheel size | <10MB for any-llm-mode |

---

### B7: Dual-Mode Configuration Conflicts Not Resolved

**Severity:** Low (Configuration)

**Finding:** The config.yaml examples show mutually exclusive `mode: proxy` vs `mode: sdk`. But if built with `full` (both modes), which config section takes precedence?

```yaml
# If both modes are compiled in, which wins?
mode: sdk  # or proxy?
litellm_mode:
  host: "0.0.0.0"
  port: 8000
anyllm_mode:
  default_provider: "openai"
```

**Resolution:** Document that `full` build enables BOTH HTTP server AND SDK. The config determines which is primary:
- `mode: both` — HTTP server running + SDK importable
- `mode: proxy` — HTTP server only (SDK import fails gracefully)
- `mode: sdk` — HTTP server disabled, SDK only

---

## Adversarial Review Round 2: HTTP Forwarding Emphasis + Enterprise for Both

### New Issues Summary

| ID | Severity | Issue |
|----|----------|-------|
| B1 | High | HTTP forwarding is core but "Provider SDK delegation" in matrix is misleading |
| B2 | High | Enterprise features (keys, budgets, rate limiting) only accessible in LiteLLM Mode |
| B3 | Medium | `enterprise` feature gate is redundant with mode gates — enterprise features should be in BOTH modes |
| B4 | Medium | HTTP forwarding as shared core not explicitly named |
| B5 | Medium | `provider/model` vs `provider:model` format collision ambiguous |
| B6 | Low | Binary size not specified |
| B7 | Low | Dual-mode config conflicts not resolved |

### Combined Status (All Issues)

| ID | Severity | Status | Issue |
|----|----------|--------|-------|
| A1 | Critical | **FIXED** | PyO3 cannot bridge to Python SDKs → all providers use reqwest |
| A2 | Critical | **FIXED** | Feature gate location → py_bridge in quota-router-core |
| A3 | Critical | **FIXED** | &mut self → &self with interior mutability |
| A4 | High | Open | Streaming not specified |
| A5 | High | **FIXED** | Budget vs OCTO-W semantics separated |
| A6 | Medium | **FIXED** | Storage 3x → 2x calls |
| A7 | Medium | **FIXED** | Per-request router → Arc<Router> shared |
| A8 | Medium | Open | any-llm Mode API key auth not specified |
| A9 | Low | Open | RFC status inconsistency in dependencies |
| A10 | Low | Open | PyO3 experimental async risk |
| A11 | Low | Open | Feature flags baked into wheel |
| B1 | High | **FIXED** | HTTP forwarding core — matrix fixed, "SDK delegation" → "HTTP forwarding" |
| B2 | High | **FIXED** | Enterprise features in both modes — shared core, not per-mode |
| B3 | Medium | **FIXED** | enterprise gate removed — features in shared core |
| B4 | Medium | **FIXED** | HTTP forwarding named as shared core |
| B5 | Medium | Open | provider/model vs provider:model format collision |
| B6 | Low | **FIXED** | Binary size added to Design Goals |
| B7 | Low | **FIXED** | Dual-mode config conflicts resolved |

---

## Adversarial Review Round 5: Deep Issues

### C1: Feature Gate Table Claims Both Modes Have Both Interfaces — But Gate Definitions Make This Impossible

**Severity:** Critical (Architectural Contradiction)

**Finding:** The RFC makes two contradictory claims about what feature gates produce:

**Claim 1 — Feature gate table (lines 141-152):**
| Interface | `litellm-mode` | `any-llm-mode` | `full` |
|-----------|:--------------:|:---------------:|:------:|
| HTTP proxy | ✅ | ✅ | ✅ |
| Python SDK | ✅ | ✅ | ✅ |

This table says `litellm-mode` alone produces both HTTP proxy AND Python SDK. Similarly for `any-llm-mode`.

**Claim 2 — Cargo features (lines 707-718):**
```toml
litellm-mode = ["hyper", "axum"]  # No py-o3!
any-llm-mode = ["py-o3"]          # No hyper!
```
This says `litellm-mode` only compiles `hyper`/`axum` and does NOT compile `py-o3`. `any-llm-mode` only compiles `py-o3` and does NOT compile `hyper`/`axum`.

**Contradiction:** If `litellm-mode` doesn't compile `py-o3`, the Python SDK (PyO3 bindings) cannot exist in `litellm-mode`. But the table says it does. Similarly, if `any-llm-mode` doesn't compile `hyper`/`axum`, the HTTP proxy cannot exist in `any-llm-mode`. But the table says it does.

**Root cause:** The RFC says "Both modes expose both interfaces" but the feature gates are defined as mutually exclusive per code path. The statement "interfaces always available when respective mode is enabled" is false — `hyper` is only compiled with `litellm-mode`, and `py-o3` is only compiled with `any-llm-mode`.

**Impact:** The entire claim of "interface parity between modes" is only true for `full` builds. For individual feature flags, you get one interface each.

**Resolution:** Either:
1. Make `hyper` and `py-o3` unconditional dependencies (always compiled), so both interfaces are always available
2. Change the table to accurately reflect per-flag capabilities:
   | Interface | `litellm-mode` | `any-llm-mode` | `full` |
   |-----------|:--------------:|:---------------:|:------:|
   | HTTP proxy | ✅ | ❌ | ✅ |
   | Python SDK | ❌ | ✅ | ✅ |
3. Or restructure feature gates so the interface availability truly matches the table

---

### C2: `LLMProvider` Trait Cannot Simultaneously Support Both Provider Integration Strategies

**Severity:** Critical (Architectural Contradiction)

**Finding:** The RFC defines a unified `LLMProvider` trait (lines 332-359) as the shared abstraction for all provider implementations:

```rust
pub trait LLMProvider: Send + Sync {
    async fn completion(&self, request: &CompletionRequest) -> Result<CompletionResponse, ProviderError>;
    async fn embedding(&self, request: &EmbeddingRequest) -> Result<EmbeddingResponse, ProviderError>;
    fn routing_weight(&self) -> u32;
}
```

But the mode distinction requires this trait to simultaneously support two fundamentally different HTTP call paths for the **same provider**:

| Provider | LiteLLM Mode | any-llm Mode |
|----------|-------------|--------------|
| OpenAI | `reqwest` (Rust HTTP) | `openai` Python SDK via PyO3 |
| Anthropic | `reqwest` (Rust HTTP) | `anthropic` Python SDK via PyO3 |

**Problem:** The unified `LLMProvider` trait hides the integration strategy difference. But the actual call — `provider.completion(&request)` — must produce the same HTTP request regardless of which strategy is compiled in. If `OpenAI` is compiled as `reqwest` in one build and `PyO3→openai SDK` in another, the same `LLMProvider::completion()` call must reach the same endpoint with the same body. This is only possible if:

1. Both implementations produce **byte-for-byte identical** HTTP requests, OR
2. The normalization layer transforms responses so the caller can't tell the difference

For OpenAI, both can produce correct requests. But for Anthropic, the SDK does automatic message format conversion (`OpenAI messages → Anthropic content.blocks`) that `reqwest` code cannot replicate without reimplementing the same conversion logic. The `LLMProvider` trait's `CompletionRequest` type must encode all the provider-specific translation logic — meaning the trait is NOT provider-agnostic; it must be specialized per integration strategy.

**Further problem:** The Provider implementations in `providers/mod.rs` (lines 361-368) show ALL providers as `reqwest`-based:
```rust
pub mod openai;        // reqwest-based OpenAI
pub mod anthropic;    // reqwest-based Anthropic
```
In any-llm mode, these would need to be different implementations wrapping Python SDKs via PyO3 — but they can't coexist with the `reqwest` versions.

**Resolution:** The `LLMProvider` trait must be feature-gated itself, with separate traits for `native_http` and `py_bridge` strategies. Or the trait must be parameterized by the integration strategy:

```rust
#[cfg(feature = "litellm-mode")]
pub trait LLMProvider: Send + Sync {
    async fn completion(&self, req: &HttpCompletionRequest) -> ...;
}

#[cfg(feature = "any-llm-mode")]
pub trait LLMProvider: Send + Sync {
    async fn completion(&self, req: &PyCompletionRequest) -> ...;
}
```

This fundamentally changes the "shared router" claim.

---

### C3: Streaming in any-llm Mode Via HTTP Proxy Is Not Specified

**Severity:** High (Missing Core Feature)

**Finding:** The LiteLLM compatibility matrix shows both modes support streaming. The LiteLLM Mode streaming sequence is clear (SSE via tokio-tower). But streaming in any-llm mode is underspecified in two ways:

**C3a: Streaming via Python SDK interface**

The Python SDK for any-llm mode calls through PyO3. The research confirms any-llm uses Python generators for streaming. But the RFC's `acompletion()` signature is:

```rust
pub async fn completion(..., stream: Option<bool>, ...) -> PyResult<Py<PyAny>>
```

Returning `Py<PyAny>` for a streaming response means returning a Python iterator/generator. The spec says "Return a Python generator (`PyIterator`) that yields chunks as they arrive." But the PyO3 signature is `async fn` — PyO3's async support is experimental and how an async Rust function yields to a Python generator while awaiting provider responses is unspecified.

**C3b: Streaming via HTTP proxy in any-llm mode**

The HTTP proxy sequence diagram (lines 267-292) shows streaming via the PyO3 bridge:

```
Router->>SDK: PyO3 call
SDK->>ProviderSDK: Official SDK call
ProviderSDK->>Provider: Provider API (streaming)
Provider-->>ProviderSDK: SSE stream
ProviderSDK-->>SDK: chunk
SDK-->>Router: chunk
Router-->>Gateway: SSE chunk
```

**Problem:** The official Python SDK streaming API (e.g., `AsyncAnthropic.messages.stream()`) yields Python objects, not HTTP bytes. Converting these to SSE format for the HTTP response requires:
1. Detecting streaming response in PyO3 bridge
2. Converting Python SDK chunk objects to OpenAI SSE format
3. Streaming those chunks back through the Rust router
4. Through the hyper response body

This is an entirely new code path not shown in the spec. The SSE conversion layer is missing.

**C3c: SSE format inconsistency**

The spec states (line 891):
```
OpenAI: data: {"choices":[{"delta":{"content":"token"}}]}\n\n
Anthropic: data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"token"}}\n\n
```

But these are **different JSON structures** — not just different field names. An HTTP proxy receiving an Anthropic SSE stream cannot naively forward OpenAI-format chunks. The conversion from Anthropic's event types to OpenAI's streaming format requires a transformation layer that doesn't exist in the spec.

**Resolution:** Specify the streaming architecture explicitly for each mode × interface combination. At minimum, 6 paths need specification: LiteLLM via HTTP (SSE), LiteLLM via Python (generator), any-llm via HTTP (SDK→SSE conversion), any-llm via Python (generator). Add SSE transformation to the LiteLLM Mode section.

---

### C4: `storage` Module Defines Budget Limit But RFC-0904 Is Not Final

**Severity:** High (Dependency)

**Finding:** The storage interface (lines 549-561) defines `check_budget_limit` referencing RFC-0903/0904 budget semantics. But RFC-0904 is in **Planned** status (not Accepted or Final). RFC-0909 (Final) depends on RFC-0904 for `record_spend()` which uses pricing. The dependency chain:

```
RFC-0917 → requires → RFC-0904 (Planned)
RFC-0917 → requires → RFC-0909 (Final) → requires → RFC-0904 (Planned)
```

If RFC-0904 changes (budget reset periods, per-key vs per-team, etc.), the storage interface becomes invalid.

**Missing:** A statement that the budget enforcement interface is provisional pending RFC-0904 acceptance.

---

### C5: `set_api_key()` Is Registration, Not Validation — Auth Model Still Broken

**Severity:** High (Security Gap)

**Finding:** The A8 resolution shows:

```python
set_api_key("sk-...")  # Registers with storage, enforces budget
```

**Problem:** Registration and validation are different operations:
- **Registration:** Associate a key hash with storage (creates a virtual key entry)
- **Validation:** Check if a presented key is valid and has budget remaining

The current SDK flow:
```python
set_api_key("sk-...")  # This just REGISTERS a key
response = completion(...)  # Router validates... but where?
```

If `set_api_key` only registers, then any string can be "registered" before use. The actual validation must happen during `completion()` — but the spec doesn't show where. Does the PyO3 bridge call `storage.validate_key()`? If so, when and with what key material?

**Security gap:** A malicious caller can call `set_api_key("any-random-string")` to register a key, then use it. There's no validation that the presented key matches an actual provider API key.

**Resolution:** The SDK operates in "trust the caller" mode — `set_api_key()` stores the key for use in provider calls. Validation happens at provider call time (option 2). The security model is:

1. **SDK deployment context:** The SDK is deployed in a trusted environment (user's server). The caller is authenticated to the SDK via their own auth system. `set_api_key()` stores the user's provider credentials locally.

2. **Virtual key vs provider key:** The SDK stores *provider* API keys (OpenAI, Anthropic, etc.), not virtual keys. Virtual keys (RFC-0903) are a LiteLLM Mode (HTTP proxy) concept where the proxy mediates access. In any-llm Mode (SDK), the client has direct provider credentials.

3. **Validation timing:** `set_api_key()` stores the key. The first `completion()` call uses it — if invalid, the provider returns an auth error which surfaces to the caller.

4. **Format validation only:** `set_api_key()` SHOULD validate key format before storing:
   - OpenAI keys: must start with `sk-` and be 48+ characters
   - Anthropic keys: must start with `sk-ant-` and be 48+ characters
   - Mistral keys: must start with `mistral-` or be a valid base64-like string
   - Other providers: validate format per provider's documented key format

```python
def set_api_key(key: str, provider: str = "openai"):
    """Store provider API key for subsequent calls.
    
    Validates key format matches the provider's expected format.
    Does NOT validate key with provider (first completion() call does that).
    """
    if provider == "openai" and not (key.startswith("sk-") and len(key) >= 48):
        raise ValueError("Invalid OpenAI key format")
    elif provider == "anthropic" and not (key.startswith("sk-ant-") and len(key) >= 48):
        raise ValueError("Invalid Anthropic key format")
    # ... other providers ...
    _storage.store_key(provider, key)
```

**Security note:** This is the same model as the official provider SDKs (OpenAI Python SDK, Anthropic Python SDK) — they store keys locally and validate at call time.

---

### C6: HTTP Proxy Streaming in any-llm Mode Requires PyO3 — But Hyper Isn't Compiled

**Severity:** High (Implementation Blocker)

**Finding:** The any-llm mode HTTP proxy streaming sequence (lines 267-292) requires the PyO3 bridge to convert Python SDK streaming responses to SSE. But `hyper` is only listed as a dependency of `litellm-mode` (not `any-llm-mode`).

**From Cargo features:**
```toml
any-llm-mode = ["py-o3"]    # Python SDK delegation via PyO3
litellm-mode = ["hyper", "axum"]  # HTTP proxy server
```

Building with `--features any-llm-mode` alone does NOT compile `hyper`/`axum`. Therefore the HTTP proxy cannot exist in any-llm-mode-only builds.

**Contradiction:** The spec's mode table shows HTTP proxy as available in `any-llm-mode`. The feature gate definition prevents this. And the streaming sequence for any-llm mode via HTTP proxy requires both `hyper` (for the SSE response) AND `py-o3` (for the SDK bridge) simultaneously — which only happens in `full` builds.

**Impact:** Streaming via HTTP proxy in any-llm mode is only possible with `full` builds. The table claiming HTTP proxy is available in `any-llm-mode` alone is false.

**Resolution (FIXED):** The feature gate tables have been corrected to accurately reflect per-flag capabilities:
- `litellm-mode`: HTTP proxy ✅, Python SDK ❌
- `any-llm-mode`: HTTP proxy ❌, Python SDK ✅
- `full`: HTTP proxy ✅, Python SDK ✅

Streaming via HTTP proxy in any-llm Mode is only available with `full` builds. The any-llm Mode's streaming is via the Python SDK interface only (Python generator yielding chunks). The HTTP proxy streaming scenario for any-llm Mode is not supported in single-mode builds — requires `full`.

---

### C7: Provider Implementations Listed in Wrong Module for any-llm Mode

**Severity:** Medium (Documentation Error)

**Finding:** The Feature-Gated Structure (lines 677-704) shows:

```
providers/
├── openai.rs      # reqwest OpenAI
├── anthropic.rs   # reqwest Anthropic
...
```

These are `reqwest`-based implementations for LiteLLM mode. But in any-llm mode, the providers are Python SDKs called via PyO3 — completely different code. The same directory structure cannot hold both simultaneously without conditional compilation (`#[cfg(feature = "litellm-mode")]` etc.).

**Problem:** The file tree implies a single set of provider implementations that work for both modes. This is false — any-llm mode providers are Python SDK wrappers in the PyO3 bridge, not Rust files in `providers/`.

**Resolution (FIXED):** The Feature-Gated Structure has been updated to show:
- `providers/` directory is feature-gated `[feature = "litellm-mode"]` only
- `py_bridge/providers/` subdirectory holds Python SDK wrappers, feature-gated `[feature = "any-llm-mode"]`
- These are mutually exclusive per build flag

The updated structure shows:
```
providers/             # [feature = "litellm-mode"] ONLY
py_bridge/providers/  # [feature = "any-llm-mode"] ONLY
```

---

### C8: B5 Parsing Rules Fail for Model Names With `/` or `:` — Silent Misrouting

**Severity:** Medium (Security/Misrouting Risk)

**Finding:** The B5 resolution defines parsing rules:

1. If `:` present → split on first `:` (any-llm style: `provider:model`)
2. If `/` present → split on first `/` (LiteLLM style: `provider/model`)
3. If both → reject as ambiguous

**Problem:** Real provider model names contain these characters:
- `openai/gpt-4o-0613` (slash for version/date)
- `anthropic/claude-opus-4-250624` (slash for model versioning)
- `mistral-small-latest` (no separator — bare model name)

Rule 2 would parse `openai/gpt-4o-0613` as `provider="openai"`, `model="gpt-4o-0613"`. But if a future OpenAI model is named `openai/gpt-4` (without versioning), rule 2 correctly splits it. However, rule 3 rejects anything with both — so `openai/gpt-4o:2024-06-13` would be rejected as ambiguous, even if intentionally formatted.

**Deeper problem:** The B5 rules don't account for provider-specific model naming conventions. Ollama uses `ollama/llama3.1:8b` (slash + colon). The rules would parse this as `provider="ollama"`, `model="llama3.1"` — losing the `:8b` tag.

**Resolution (FIXED):** Use **provider-list matching** (per litellm's approach). Only treat the first segment as a provider if it matches a known provider. This avoids misrouting when model names coincidentally contain `/` or `:`.

```rust
/// Known LLM providers (matched against first segment of model string)
const KNOWN_PROVIDERS: &[&str] = &[
    "openai", "anthropic", "mistral", "ollama",
    "gemini", "google", "azure", "bedrock",
    "openrouter", "cohere", "vertexai", "replicate",
];

/// Parse a model string, returning (provider, model_name).
///
/// Algorithm (per litellm's get_llm_provider_logic):
/// 1. Split on `:` first — if segment[0] is a known provider, use colon format
/// 2. Else split on `/` — if segment[0] is a known provider, use slash format
/// 3. Else no provider prefix — use default_provider (from config)
/// 4. Colon format takes precedence over slash if both are present AND provider matches
///
/// This correctly handles:
/// - `openai:gpt-4o` → provider="openai", model="gpt-4o"
/// - `ollama/llama3.1:8b` → provider="ollama", model="llama3.1:8b" (colon in model name)
/// - `gpt-4o` → provider="openai" (default), model="gpt-4o"
/// - `openai/gpt-4o-0613` → provider="openai", model="gpt-4o-0613"
///
/// Reject:
/// - Model strings with unknown provider prefixes (e.g., `unknown/gpt-4`)
/// - Ambiguous formats where neither delimiter's provider matches (both delimiters present)
fn parse_model_string(model: &str, default_provider: &str) -> Result<(&str, &str), ModelParseError> {
    let colon_idx = model.find(':');
    let slash_idx = model.find('/');

    // Try colon format first
    if let Some(idx) = colon_idx {
        let candidate = &model[..idx];
        if KNOWN_PROVIDERS.contains(&candidate) {
            let provider = candidate;
            let model_name = &model[idx + 1..];
            return Ok((provider, model_name));
        }
    }

    // Try slash format
    if let Some(idx) = slash_idx {
        let candidate = &model[..idx];
        if KNOWN_PROVIDERS.contains(&candidate) {
            let provider = candidate;
            let model_name = &model[idx + 1..];
            return Ok((provider, model_name));
        }
    }

    // No recognized provider prefix — use default
    Ok((default_provider, model))
}
```

**Per-provider model name conventions:**

| Provider | Model Format | Examples |
|----------|-------------|----------|
| OpenAI | `gpt-4o`, `gpt-4o-mini`, `gpt-4o-0613` | No prefix (default), version suffix with `-` |
| Anthropic | `claude-opus-4-250624`, `claude-sonnet-4` | Date suffixes, hyphen separator |
| Mistral | `mistral-small-latest`, `mistral-large-latest` | Hypenated, `-latest` suffix |
| Ollama | `llama3.1:8b`, `mistral:7b` | `model:size` format for local models |
| Gemini | `gemini-1.5-pro`, `gemini-2.0-flash` | Provider prefix usually via config |
| Azure | Via `api_base` config | No model prefix in model string |
| AWS Bedrock | `anthropic.claude-3-sonnet-20240229-v1:0` | Provider.model:version format |

**Note:** If a future provider uses `provider/model:tag` format (like Ollama), the slash-delimiter branch correctly captures `model:tag` as the full model name because the provider check passes on `ollama`.

---

### C9: idempotency_key — Missing Specification for Safe Retries

**Severity:** Medium (Reliability)

**Finding:** LiteLLM supports `idempotency_key` for safe request retries. The RFC does not mention idempotency anywhere. Without idempotency keys, retrying a failed request could result in duplicate charges if the provider processed the original request but the response was lost.

**Missing:**
- Whether idempotency keys are supported
- How they map to provider APIs (OpenAI supports idempotency, Anthropic does not)
- Whether retry behavior is provider-aware

**Resolution (FIXED):** Idempotency key support via pass-through header (per litellm's approach):

```rust
/// IdempotencyKey: passed through to providers that support it
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    pub fn new() -> Self { Self(uuid::Uuid::v4().to_string()) }
    pub fn from_str(s: &str) -> Self { Self(s.to_string()) }
    pub fn as_str(&self) -> &str { &self.0 }
}

/// Per-provider idempotency support:
/// - OpenAI: ✅ passes `Idempotency-Key` header to provider
/// - Anthropic: ❌ no idempotency support — retries handled locally
/// - Mistral: ✅ pass-through header
/// - Ollama: ❌ no idempotency support (local provider)
/// - Gemini: ❌ no idempotency support
/// - Azure: ✅ via OpenAI SDK (Azure OpenAI supports idempotency)
/// - AWS Bedrock: ❌ no idempotency support

/// Request options
pub struct RequestOptions {
    pub idempotency_key: Option<IdempotencyKey>,
    pub timeout: Duration,
    pub max_retries: u32,
}
```

**Retry with idempotency:** When retrying a failed request:
1. Same `idempotency_key` is reused across retry attempts
2. If provider returns 200 with a response (not error), idempotency guarantees same result
3. If provider returns 409 Conflict (duplicate), return the cached response
4. Storage records `idempotency_key` with each spend event for duplicate detection

**Storage duplicate detection:**
```rust
/// Record idempotency key with spend event
pub async fn record_spend_with_idempotency(
    &self,
    event: &SpendEvent,
    idempotency_key: Option<&IdempotencyKey>,
) -> Result<(), StorageError> {
    // Check for existing event with same idempotency key
    if let Some(key) = idempotency_key {
        if self.has_idempotent_event(key).await? {
            return Ok(());  // Duplicate — skip recording
        }
    }
    self.record_spend(event).await
}
```

**Note:** LiteLLM (the reference implementation) does NOT implement idempotency keys internally — it passes them through as headers when provided. This RFC follows the same approach.

---

### C10: Timeout and Retry Policy Not Specified

**Severity:** Medium (Operations)

**Finding:** The RFC mentions retries and timeouts in the context of RFC-0902 routing strategies but does not specify:

- Default timeout per provider (OpenAI: 60s? 120s?)
- Per-request timeout vs total timeout
- Retry conditions: which errors trigger retry (5xx? rate limit? network?)
- Max retries per request
- Timeout enforcement point: router, provider impl, or storage layer?

**Resolution (FIXED):** Full timeout and retry policy specification (per litellm's RetryPolicy):

```rust
/// Timeout configuration
#[derive(Clone)]
pub struct TimeoutConfig {
    /// Per-request timeout (default: 60 seconds)
    pub request_timeout: Duration,
    /// Max streaming duration (default: 300 seconds)
    pub stream_timeout: Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(60),
            stream_timeout: Duration::from_secs(300),
        }
    }
}

/// Retry policy (per litellm's RetryPolicy class)
#[derive(Clone)]
pub struct RetryPolicy {
    /// Retries on BadRequestError (default: 0)
    pub bad_request_error_retries: u32,
    /// Retries on AuthenticationError (default: 0)
    pub authentication_error_retries: u32,
    /// Retries on TimeoutError (default: 2)
    pub timeout_error_retries: u32,
    /// Retries on RateLimitError (default: 3)
    pub rate_limit_error_retries: u32,
    /// Retries on ContentPolicyViolationError (default: 0)
    pub content_policy_violation_retries: u32,
    /// Retries on InternalServerError (default: 2)
    pub internal_server_error_retries: u32,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            bad_request_error_retries: 0,
            authentication_error_retries: 0,
            timeout_error_retries: 2,
            rate_limit_error_retries: 3,
            content_policy_violation_retries: 0,
            internal_server_error_retries: 2,
        }
    }
}

/// Per-provider configuration
#[derive(Clone)]
pub struct ProviderConfig {
    pub timeout: TimeoutConfig,
    pub retry_policy: RetryPolicy,
    pub retry_overrides: Option<HashMap<String, RetryPolicy>>,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            timeout: TimeoutConfig::default(),
            retry_policy: RetryPolicy::default(),
            retry_overrides: None,
        }
    }
}

/// Global defaults
pub const DEFAULT_TIMEOUT_SECS: u64 = 60;
pub const DEFAULT_MAX_RETRIES: u32 = 2;
```

**Retry error classification:**

| Error Type | Retry? | Notes |
|------------|--------|-------|
| `400 BadRequestError` | ❌ | Don't retry invalid requests |
| `401 AuthenticationError` | ❌ | Don't retry auth failures |
| `408 TimeoutError` | ✅ (2 retries) | Network timeout, provider slow |
| `429 RateLimitError` | ✅ (3 retries) | With exponential backoff + `Retry-After` header |
| `413 ContentPolicyViolationError` | ❌ | Don't retry content policy violations |
| `500 InternalServerError` | ✅ (2 retries) | Provider internal error |
| `502 BadGatewayError` | ✅ (2 retries) | Upstream provider error |
| `503 ServiceUnavailableError` | ✅ (2 retries) | Provider temporarily unavailable |

**Timeout enforcement:** Timeout is enforced at the **provider implementation layer** (not router). The `reqwest` client has a built-in timeout. For streaming, a separate stream timeout tracks maximum time from first chunk to last chunk.

**Retry behavior:**
1. Retry with **exponential backoff**: `delay = base_delay * 2^attempt` (capped at 60s)
2. Respect `Retry-After` header from rate limit responses
3. Use same `idempotency_key` on retries (when provided)
4. Budget check happens **before** retries (don't waste budget on retries of already-rejected requests)

---

### C11: `get_balance` Still Returns OCTO-W Per A5 Fix — But Field Name Is Wrong

**Severity:** Medium (Stale Code)

**Finding:** After the A5 fix, the storage interface should separate `check_budget_limit` (budget enforcement) from OCTO-W operations. The spec defines:
```rust
pub async fn get_octo_w_balance(&self, key_id: &[u8; 16]) -> Result<u64, StorageError>;
```

But the actual code in the spec (lines 559-560) still shows:
```rust
pub async fn get_balance(&self, key_id: &[u8; 16]) -> Result<u64, StorageError>;
```

`get_balance` is ambiguous — is it OCTO-W or budget? The A5 fix was supposed to rename this to `get_octo_w_balance`. The trait in lines 549-554 still uses the old ambiguous name.

**Resolution:** Update all storage interface references to use the A5-resolved names.

---

### C12: PyO3 `async fn` Signature Is Incompatible With Synchronous Python Callers

**Severity:** Medium (Implementation Risk)

**Finding:** The PyO3 bridge defines:

```rust
#[pyfunction]
pub async fn completion(...) -> PyResult<Py<PyAny>>
```

This is an `async fn` exposed to Python. But PyO3's async support is experimental and requires the Python caller to be running within a Tokio runtime. The typical Python caller pattern:

```python
import asyncio
result = asyncio.run(quota_router.completion(...))  # Must run in event loop
```

This is not the "drop-in SDK replacement" experience — LiteLLM's SDK is synchronous. Users expecting `response = quota_router.completion(...)` (blocking call) cannot use an async function.

**Further:** The experimental `#[pyo3(async_features = "experimental-async")]` is required for `async fn` to work, but this may change in future PyO3 versions.

**Resolution (FIXED):** Follow any-llm's dual sync/async API pattern (per `any-llm/src/any_llm/api.py`):

**Dual API approach** (avoids experimental PyO3 async entirely):

```rust
// py_bridge/src/completion.rs

/// Synchronous completion (blocking) — PRIMARY interface
/// Users expect: response = quota_router.completion(model="gpt-4o", messages=[...])
#[pyfunction]
pub fn completion(
    model: String,
    messages: Vec<PyMessage>,
    stream: Option<bool>,
    // ... all other params
) -> PyResult<Py<PyAny>> {
    // Get or create Tokio runtime for this thread
    let rt = TOKIO_RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    });

    rt.blocking_async_fn(async {
        completion_impl(model, messages, stream, ...).await
    })
}

/// Asynchronous completion — for async Python callers
/// Users expect: response = await quota_router.acompletion(model="gpt-4o", messages=[...])
#[pyfunction]
pub async fn acompletion(
    model: String,
    messages: Vec<PyMessage>,
    stream: Option<bool>,
    // ... all other params
) -> PyResult<Py<PyAny>> {
    completion_impl(model, messages, stream, ...).await
}

// Internal implementation (not exposed to Python)
async fn completion_impl(
    model: String,
    messages: Vec<PyMessage>,
    stream: Option<bool>,
    // ...
) -> PyResult<Py<PyAny>> {
    // Route via shared router
    let response = Router::global()
        .route_and_forward(request).await
        .map_err(|e| PyErr::from(e))?;

    Python::with_gil(|py| response.to_dict(py))
}
```

**Thread-local runtime for sync calls:**
```rust
use std::cell::OnceCell;
thread_local! {
    static TOKIO_RUNTIME: tokio::runtime::Runtime = /* ... */;
}
```

**Streaming in sync mode:**
```rust
#[pyfunction]
pub fn completion(
    model: String,
    messages: Vec<PyMessage>,
    stream: Option<bool>,
) -> PyResult<Py<PyAny>> {
    if stream == Some(true) {
        // Return a Python generator for streaming
        Python::with_gil(|py| {
            PyIterator::new(py, StreamingIterator::new(request))
        })
    } else {
        // Blocking call, return complete response
        blocking_async_fn(async { completion_impl(...).await })
    }
}
```

**Benefits of this approach:**
1. **No experimental PyO3 async** — `#[pyfunction]` on sync `fn` is stable
2. **Drop-in replacement** — `response = quota_router.completion(...)` works like LiteLLM
3. **`acompletion()` for async callers** — `await quota_router.acompletion(...)` for asyncio users
4. **Streaming via generator** — Python callers get a Python iterator, not a Rust future

---

### C13: Feature Gate Mutual Exclusivity Contradicts Shared Core Diagram

**Severity:** Low (Architectural Clarity)

**Finding:** The Mermaid diagram (lines 68-95) shows:

```
LiteLLM --> Shared
AnyLLM --> Shared
```

Both modes feed into the same `Shared` core. But the feature gate architecture shows:

```rust
#[cfg(feature = "litellm-mode")]
pub mod native_http;   // LiteLLM Mode ONLY

#[cfg(feature = "any-llm-mode")]
pub mod py_bridge;     // any-llm Mode ONLY
```

These modules are feature-gated and cannot coexist in the same compilation unit. The diagram's "Shared" implies both can be compiled simultaneously and route through shared code. In reality, the provider call path is mutually exclusive — you can't have both `native_http` and `py_bridge` active simultaneously.

**Resolution:** Update diagram to show mutual exclusivity explicitly, or restructure so the shared router truly can accept both provider implementation types at runtime (via dynamic dispatch with feature-gated compilation of each implementation).

---

## Round 5 Issues Summary

| ID | Severity | Issue |
|----|----------|-------|
| C1 | Critical | Feature gate table claims both modes have both interfaces — but gate definitions make this impossible |
| C2 | Critical | `LLMProvider` trait cannot simultaneously support both integration strategies |
| C3 | High | Streaming in any-llm mode via HTTP proxy is not specified (SSE conversion missing) |
| C4 | High | `storage` module defines budget interface but RFC-0904 is Planned |
| C5 | High | `set_api_key()` is registration not validation — auth model still broken |
| C6 | High | HTTP proxy streaming in any-llm mode requires `hyper` — but `hyper` only compiled with `litellm-mode` |
| C7 | Medium | Provider implementations listed in wrong module for any-llm mode |
| C8 | Medium | B5 parsing rules fail silently for provider-specific model naming conventions |
| C9 | Medium | idempotency_key missing — safe retries not specified |
| C10 | Medium | Timeout and retry policy not specified |
| C11 | Medium | `get_balance` name stale — A5 fix not applied to code |
| C12 | Medium | PyO3 `async fn` incompatible with synchronous Python callers |
| C13 | Low | Feature gate mutual exclusivity contradicts shared core diagram |

**Round 6 Status:** C3, C8, C9, C10, C12, C13, A4, B5 all FIXED. C4 and A9 remain open (RFC-0904 Planned dependency). |

### Combined Status (All Issues Through Round 5)

| ID | Severity | Status | Issue |
|----|----------|--------|-------|
| A1 | Critical | **FIXED** | PyO3 cannot bridge to Python SDKs → all providers use reqwest |
| A2 | Critical | **FIXED** | Feature gate location → py_bridge in quota-router-core |
| A3 | Critical | **FIXED** | &mut self → &self with interior mutability |
| A4 | High | **FIXED** | Streaming fully specified: SSE format, per-provider differences, LiteLLM vs any-llm modes |
| A5 | High | **FIXED** | Budget vs OCTO-W semantics separated |
| A6 | Medium | **FIXED** | Storage 3x → 2x calls |
| A7 | Medium | **FIXED** | Per-request router → Arc<Router> shared |
| A8 | Medium | **FIXED** | `set_api_key()` auth — resolved by C5 fix (format validation + completion-time validation) |
| A9 | Low | Open | RFC-0904 (Planned) dependency — budget enforcement interface is provisional |
| A10 | Low | **FIXED** | PyO3 async — resolved by C12 (dual sync/async API, no experimental async needed) |
| A11 | Low | Open | Feature flags baked into wheel — known limitation, documented |
| B1 | High | **FIXED** | Provider SDK delegation → HTTP forwarding |
| B2 | High | **FIXED** | Enterprise features in both modes |
| B3 | Medium | **FIXED** | enterprise gate removed |
| B4 | Medium | **FIXED** | HTTP forwarding named as shared core |
| B5 | Medium | **FIXED** | Parsing rules — resolved by C8 (provider-list matching) |
| B6 | Low | **FIXED** | Binary size added |
| B7 | Low | **FIXED** | Config conflicts resolved |
| C1 | Critical | **FIXED** | Feature gate table corrected — litellm-mode=HTTP only, any-llm-mode=SDK only, full=both |
| C2 | Critical | **FIXED** | LLMProvider trait feature-gated — separate HttpProvider (reqwest) and SdkProvider (PyO3) traits |
| C3 | High | **FIXED** | Streaming spec complete — per-mode availability, SSE format, any-llm HTTP proxy requires full build |
| C4 | High | Open | RFC-0904 (Planned) dependency — storage interface provisional pending RFC-0904 acceptance |
| C5 | High | **FIXED** | set_api_key() is format validation + storage; actual provider validation at completion() time |
| C6 | High | **FIXED** | Tables corrected — HTTP proxy ❌ in any-llm-mode alone, ✅ only in full build |
| C7 | Medium | **FIXED** | Feature-Gated Structure updated — providers/ (litellm-mode), py_bridge/providers/ (any-llm-mode) |
| C8 | Medium | **FIXED** | Provider-list matching parsing algorithm — colon/slash only split if provider matches |
| C9 | Medium | **FIXED** | Idempotency key — pass-through header (OpenAI/Gemini), local retry for others |
| C10 | Medium | **FIXED** | RetryPolicy + TimeoutConfig fully specified (per litellm's approach) |
| C11 | Medium | **FIXED** | `get_balance` → `get_octo_w_balance` in storage interface |
| C12 | Medium | **FIXED** | Dual sync/async API — blocking sync fn + async fn, no experimental PyO3 async |
| C13 | Low | **FIXED** | Diagram updated — mutual exclusivity shown explicitly |

---

## Version History

| Version | Date       | Changes |
|---------|------------|---------|
| 2.6     | 2026-04-21 | Round 6: A4 streaming spec; C8 provider-list parsing; C9 idempotency keys; C10 RetryPolicy; C12 dual sync/async API; C13 diagram mutual exclusivity |
| 2.5     | 2026-04-21 | Round 5 fixes: C1 (feature gate table corrected), C2 (feature-gated provider traits), C5 (set_api_key format validation), C6/C7 (interface/module corrections) |
| 2.4     | 2026-04-21 | Round 4: mode distinction is PROVIDER INTEGRATION STRATEGY (LiteLLM=native reqwest HTTP, any-llm=Python SDK delegation), not interface |
| 2.3     | 2026-04-21 | Round 3: fix dual-mode misleading — enterprise in both modes, HTTP forwarding shared core |
| 2.2     | 2026-04-21 | Round 2 adversarial review: HTTP forwarding emphasis, enterprise features for both modes (B1-B7) |
| 2.1     | 2026-04-21 | Fix A1/A2/A3 (critical): PyO3→reqwest, py_bridge in-core, &self interior mutability |
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
