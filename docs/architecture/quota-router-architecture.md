# Quota Router Architecture

> **Version:** 1.0.0
> **Date:** 2026-05-20
> **Status:** Revised (Round 3 — post adversarial review)
> **Crates:** `quota-router-core`, `quota-router-pyo3`

## Table of Contents

1. [System Overview](#1-system-overview)
2. [Crate Architecture](#2-crate-architecture)
3. [Dual-Mode Architecture](#3-dual-mode-architecture)
4. [Request Flow](#4-request-flow)
5. [Provider System](#5-provider-system)
6. [Module Architecture](#6-module-architecture)
7. [Data Types](#7-data-types)
8. [Error Handling](#8-error-handling)
9. [Configuration](#9-configuration)
10. [Deployment and Mode Selection](#10-deployment-and-mode-selection)
11. [Test Architecture](#11-test-architecture)

---

## 1. System Overview

Quota Router is a Rust-based AI API gateway that provides a unified interface for
routing requests to 44 LLM providers. It exposes two interfaces — an HTTP proxy
and a Python SDK — both available regardless of runtime mode (litellm or any-llm).

```mermaid
graph TB
    subgraph Clients["Client Layer"]
        C1[HTTP Clients]
        C2[Python SDK Users]
        C3[Litellm Drop-in]
        C4[Any-llm Drop-in]
    end

    subgraph Interfaces["Interface Layer"]
        I1[HTTP Proxy<br/>ProxyServer]
        I2[Python SDK<br/>PyO3 Binding]
    end

    subgraph Core["Core Layer<br/>quota-router-core"]
        direction TB
        M1[Mode Router<br/>mode.rs]
        M2[Config<br/>config.rs]
        M3[Router<br/>router.rs]
        M4[Fallback<br/>fallback.rs]
        M5[Rate Limit<br/>rate_limit.rs]
        M6[Balance<br/>balance.rs]
        M7[Cache<br/>cache.rs]
        M8[Callbacks<br/>callbacks]
        M9[Guardrails<br/>guardrails]
        M10[Prompts<br/>prompts]
        M11[Pre-call Checks<br/>pre_call_checks.rs]
    end

    subgraph Providers["Provider Layer"]
        direction TB
        P1[native_http<br/>12 Providers<br/>reqwest HTTP]
        P2[py_bridge<br/>42 Providers<br/>Python SDKs]
    end

    subgraph External["External Services"]
        E1[OpenAI API]
        E2[Anthropic API]
        E3[Google Gemini]
        E4[Azure OpenAI]
        E5[44 Unique Providers]
    end

    Clients --> Interfaces
    Interfaces --> Core
    Core --> Providers
    Providers --> External

    style Clients fill:#e3f2fd
    style Interfaces fill:#e8f5e9
    style Core fill:#fff3e0
    style Providers fill:#fce4ec
    style External fill:#f3e5f5
```

---

## 2. Crate Architecture

The project is split into two Rust crates with a clear dependency hierarchy:

```mermaid
graph LR
    subgraph PyO3["quota-router-pyo3"]
        direction TB
        L1[lib.rs<br/>Module Registration]
        L2[completion.rs<br/>Completion Functions]
        L3[router.rs<br/>Router Class]
        L4[exceptions.rs<br/>Python Exceptions]
        L5[model.rs<br/>Model Parsing]
        L6[batch.rs<br/>Batch Functions]
        L7[providers/<br/>Provider Wrappers]
        L8[types.rs<br/>Python Types]
        L9[sdk.rs<br/>SDK Utilities]
        L10[streaming.rs<br/>Streaming Support]
    end

    subgraph Core["quota-router-core"]
        direction TB
        C1[proxy.rs<br/>HTTP Proxy Server]
        C2[mode.rs<br/>Mode Router]
        C3[native_http/<br/>12 HTTP Providers]
        C4[py_bridge/<br/>42 Python Providers]
        C5[router.rs<br/>Routing Logic]
        C6[fallback.rs<br/>Fallback & Health]
        C7[config.rs<br/>Configuration]
        C8[shared_types.rs<br/>Shared Data Types]
    end

    PyO3 -->|depends on| Core

    style PyO3 fill:#e3f2fd
    style Core fill:#e8f5e9
```

### 2.1 Crate Responsibilities

| Crate | Purpose | Dependencies |
|-------|---------|--------------|
| `quota-router-core` | Core business logic, proxy server, provider implementations | tokio, reqwest, serde, pyo3 (optional) |
| `quota-router-pyo3` | Python SDK binding via PyO3 | quota-router-core, pyo3 |

### 2.2 Feature Gates

**Source:** `crates/quota-router-core/src/lib.rs` lines 18-85

27 modules are **always compiled** (no feature gate), including: admin, auth, balance, cache, callbacks, config, fallback, guardrails, health, key_rate_limiter, keys, logging, metrics, middleware, mode, pre_call_checks, pricing, prompts, providers, proxy, rate_limit, router, schema, secret_manager, storage, tracing, shared_types.

Feature-gated modules:
- `native_http` — `#[cfg(any(feature = "litellm-mode", feature = "full"))]` (line 47-48)
- `py_bridge` — `#[cfg(any(feature = "any-llm-mode", feature = "full"))]` (line 60-61)
- `python_sdk_entry` — `#[cfg(any(feature = "any-llm-mode", feature = "full"))]` (line 73-74)
- `model` + `types` — `#[cfg(any(feature = "any-llm-mode", feature = "full"))]` (lines 80-85)

```mermaid
graph TD
    subgraph Features["Feature Gates"]
        F1["litellm-mode"]
        F2["any-llm-mode"]
        F3["full"]
    end

    subgraph Always["Always Compiled (27 modules)"]
        A1[proxy.rs, mode.rs, config.rs]
        A2[router.rs, fallback.rs, admin.rs]
        A3[auth, balance, cache, callbacks]
        A4[guardrails, health, keys, metrics]
        A5[pricing, prompts, schema, storage]
    end

    subgraph Gated["Feature-Gated"]
        G1[native_http/]
        G2[py_bridge/]
        G3[python_sdk_entry]
        G4[model.rs, types.rs]
    end

    F1 -->|enables| G1
    F2 -->|enables| G2
    F2 -->|enables| G3
    F2 -->|enables| G4
    F3 -->|enables| G1
    F3 -->|enables| G2
    F3 -->|enables| G3

    style F1 fill:#e3f2fd
    style F2 fill:#e8f5e9
    style F3 fill:#fff3e0
    style Always fill:#fff3e0
    style Gated fill:#fce4ec
```

---

## 3. Dual-Mode Architecture

The mode gate controls HOW providers are called, not WHETHER an interface exists.
The HTTP proxy is always compiled. The Python SDK (`python_sdk_entry`) requires
`any-llm-mode` or `full` feature gates — in practice, `quota-router-pyo3` always
compiles with `full`, so both interfaces are available in the pip-installed package.

```mermaid
graph TB
    subgraph Input["Input Interfaces"]
        I1[HTTP Proxy Request]
        I2[Python SDK Call]
    end

    subgraph ModeRouter["Mode Router<br/>mode.rs"]
        MR{Selected Mode?}
    end

    subgraph LiteLLM["litellm-mode"]
        direction TB
        L1[HttpProviderFactory<br/>native_http/mod.rs]
        L2[reqwest HTTP Client]
        L3[Direct REST API Calls]
    end

    subgraph AnyLLM["any-llm-mode"]
        direction TB
        A1[PyBridgeProviderFactory<br/>py_bridge/mod.rs]
        A2[PyO3 Bridge]
        A3[Official Python SDKs]
    end

    subgraph Providers["Provider APIs"]
        P1[OpenAI API]
        P2[Anthropic API]
        P3[42 Providers]
    end

    Input --> ModeRouter
    MR -->|"litellm"| LiteLLM
    MR -->|"any-llm"| AnyLLM
    LiteLLM --> Providers
    AnyLLM --> Providers

    style Input fill:#e3f2fd
    style ModeRouter fill:#fff3e0
    style LiteLLM fill:#e8f5e9
    style AnyLLM fill:#fce4ec
    style Providers fill:#f3e5f5
```

### 3.1 Mode Selection

| Mode | Backend | Default | Use Case |
|------|---------|---------|----------|
| `litellm` | reqwest (native HTTP) | Yes (when both compiled) | Fast, no Python dependency |
| `any-llm` | PyO3 → Python SDKs | No | Full SDK compatibility |

**Mode selection in Python SDK:**
```python
import quota_router as qr

# Default mode (litellm - reqwest)
qr.completion(model="openai/gpt-4", messages=[...])

# Explicit mode selection
qr.completion(model="openai/gpt-4", messages=[...], _mode="litellm")
qr.completion(model="openai/gpt-4", messages=[...], _mode="any-llm")
```

---

## 4. Request Flow

### 4.1 HTTP Proxy Path (litellm-mode)

```mermaid
sequenceDiagram
    participant Client
    participant Proxy as ProxyServer<br/>proxy.rs
    participant Config as Config<br/>config.rs
    participant Router as Router<br/>router.rs
    participant Fallback as Fallback<br/>fallback.rs
    participant PreCheck as PreCallChecks<br/>pre_call_checks.rs
    participant Provider as HttpProvider<br/>native_http/
    participant API as LLM API

    Client->>Proxy: POST /v1/chat/completions
    Proxy->>Proxy: Parse request body
    Proxy->>Config: Lookup dispatch entry
    Config-->>Proxy: DispatchInfo

    Proxy->>Proxy: resolve_api_key()
    Proxy->>PreCheck: ContextWindowCheck
    PreCheck-->>Proxy: ContextWindowResult

    alt Context Exceeded
        Proxy->>Fallback: Get fallback models
        Fallback-->>Proxy: Fallback list
    end

    Proxy->>Router: Select provider
    Router-->>Proxy: Provider selection

    Proxy->>Provider: completion(request, api_key)
    Provider->>API: HTTP POST

    alt Success
        API-->>Provider: Response
        Provider-->>Proxy: ChatCompletion
        Proxy->>Fallback: record_success()
        Proxy-->>Client: 200 OK
    else 429/5xx
        API-->>Provider: Error
        Provider-->>Proxy: Error
        Proxy->>Fallback: record_failure()
        Proxy->>Fallback: Try fallback models
    end
```

### 4.2 Python SDK Path (litellm-mode)

```mermaid
sequenceDiagram
    participant User as Python User
    participant SDK as completion()<br/>completion.rs
    participant Mode as Mode Router<br/>mode.rs
    participant Factory as HttpProviderFactory<br/>native_http/mod.rs
    participant Provider as HttpProvider
    participant API as LLM API

    User->>SDK: qr.completion(model, messages, _mode="litellm")
    SDK->>SDK: ParsedModel::parse(model)
    SDK->>Mode: ProviderMode::from_str("litellm")
    Mode-->>SDK: ProviderMode::LiteLLM

    SDK->>Factory: create(provider_name)
    Factory-->>SDK: Box<dyn HttpProvider>

    SDK->>SDK: Build HttpCompletionRequest
    SDK->>Provider: completion(request, api_key)
    Provider->>API: HTTP POST (reqwest)

    API-->>Provider: Response
    Provider-->>SDK: ChatCompletion
    SDK->>SDK: Convert to Python dict
    SDK-->>User: Python dict
```

### 4.3 Python SDK Path (any-llm-mode)

```mermaid
sequenceDiagram
    participant User as Python User
    participant SDK as completion()<br/>completion.rs
    participant Mode as Mode Router<br/>mode.rs
    participant Factory as PyBridgeProviderFactory<br/>py_bridge/mod.rs
    participant Bridge as PyBridgeProvider
    participant PySDK as Python SDK<br/>(openai, anthropic, etc.)
    participant API as LLM API

    User->>SDK: qr.completion(model, messages, _mode="any-llm")
    SDK->>SDK: ParsedModel::parse(model)
    SDK->>Mode: ProviderMode::from_str("any-llm")
    Mode-->>SDK: ProviderMode::AnyLlm

    SDK->>Factory: create(provider_name)
    Factory-->>SDK: Box<dyn PyBridgeProvider>

    SDK->>Bridge: with_api_key(key).with_api_base(base)
    SDK->>Bridge: completion(model, messages)

    Bridge->>PySDK: client.chat.completions.create(...)
    PySDK->>API: HTTP Request

    API-->>PySDK: Response
    PySDK-->>Bridge: ChatCompletion object
    Bridge->>Bridge: Convert to ChatCompletion
    Bridge-->>SDK: ChatCompletion
    SDK->>SDK: Convert to Python dict
    SDK-->>User: Python dict
```

---

## 5. Provider System

### 5.1 Provider Architecture

```mermaid
graph TB
    subgraph Trait["Provider Trait Hierarchy"]
        T1["HttpProvider Trait<br/>(native_http)"]
        T2["PyBridgeProvider Trait<br/>(py_bridge)"]
    end

    subgraph NativeHTTP["native_http/ Providers<br/>(12 providers, reqwest)"]
        direction TB
        N1[openai.rs]
        N2[anthropic.rs]
        N3[mistral.rs]
        N4[groq.rs]
        N5[together.rs]
        N6[azure.rs]
        N7[databricks.rs]
        N8[perplexity.rs]
        N9[ollama.rs]
        N10[bedrock.rs]
        N11[gemini.rs]
        N12[replicate.rs]
    end

    subgraph PyBridge["py_bridge/ Providers<br/>(42 providers, Python SDKs)"]
        direction TB
        P1[openai.rs]
        P2[anthropic.rs]
        P3[mistral.rs, mistral_large.rs]
        P4[cohere.rs]
        P5[groq.rs]
        P6[gemini.rs]
        P7[bedrock.rs]
        P8[vertexai.rs]
        P9[34 more...]
    end

    subgraph Factory["Factory Pattern"]
        F1["HttpProviderFactory::create()"]
        F2["PyBridgeProviderFactory::create()"]
    end

    T1 --> NativeHTTP
    T2 --> PyBridge

    F1 --> T1
    F2 --> T2

    style Trait fill:#fff3e0
    style NativeHTTP fill:#e8f5e9
    style PyBridge fill:#e3f2fd
    style Factory fill:#fce4ec
```

### 5.2 HttpProvider Trait

**Source:** `crates/quota-router-core/src/native_http/mod.rs` lines 140-290

```rust
#[async_trait]
pub trait HttpProvider: Send + Sync {
    fn name(&self) -> &str;
    fn supported_models(&self) -> Vec<&str>;
    fn supports_model(&self, model: &str) -> bool { /* default */ }
    fn supports_streaming(&self) -> bool { false } // default

    async fn completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: Option<&str>,
    ) -> Result<HttpCompletionResponse, ProviderError>;

    // Default: returns UnsupportedModel error
    async fn streaming_completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: Option<&str>,
    ) -> Result<StreamingResponse, ProviderError>;

    async fn embedding(
        &self,
        request: &HttpEmbeddingRequest,
        api_key: Option<&str>,
    ) -> Result<HttpEmbeddingResponse, ProviderError>;

    // OpenAI Responses API (default: UnsupportedModel)
    async fn get_response(&self, ...) -> Result<HttpResponseObject, ProviderError>;
    async fn delete_response(&self, ...) -> Result<HttpDeletedObject, ProviderError>;
    async fn create_response(&self, ...) -> Result<HttpResponseObject, ProviderError>;

    // OpenAI Batch API (default: UnsupportedModel)
    async fn batch_create(&self, ...) -> Result<HttpBatchObject, ProviderError>;
    async fn batch_retrieve(&self, ...) -> Result<HttpBatchObject, ProviderError>;
    async fn batch_cancel(&self, ...) -> Result<HttpBatchObject, ProviderError>;
    async fn batch_list(&self, ...) -> Result<HttpBatchListResponse, ProviderError>;
    async fn batch_results(&self, ...) -> Result<HttpBatchResultsResponse, ProviderError>;

    // Model listing (default: UnsupportedModel)
    async fn list_models(&self, ...) -> Result<HttpListModelsResponse, ProviderError>;

    fn routing_weight(&self) -> u32 { 1 } // default
}
```

### 5.3 PyBridgeProvider Trait

**Source:** `crates/quota-router-core/src/py_bridge/openai.rs` lines 235-262

```rust
pub trait PyBridgeProvider: Send + Sync + 'static {
    fn name(&self) -> &str;

    fn completion(
        &self,
        model: &str,
        messages: &[crate::types::Message],
    ) -> Result<crate::types::ChatCompletion, PyBridgeError>;

    // Default: returns "Streaming not supported" error
    fn streaming_completion(
        &self,
        model: &str,
        messages: &[crate::types::Message],
    ) -> Result<tokio::sync::mpsc::Receiver<Result<PyBridgeChunk, PyBridgeError>>, PyBridgeError>;

    fn with_api_key(self: Box<Self>, key: String) -> Box<dyn PyBridgeProvider>;
    fn with_api_base(self: Box<Self>, base: String) -> Box<dyn PyBridgeProvider>;
}
```

### 5.4 Shared Request/Response Types

```mermaid
classDiagram
    class HttpCompletionRequest {
        +model: String
        +messages: Vec~Message~
        +stream: Option~bool~
        +temperature: Option~f32~
        +max_tokens: Option~u32~
        +top_p: Option~f32~
        +stop: Option~Vec~String~~
        +n: Option~u32~
        +presence_penalty: Option~f32~
        +frequency_penalty: Option~f32~
        +user: Option~String~
        +api_base: Option~String~
        +tools: Option~Vec~Tool~~
        +tool_choice: Option~ToolChoice~
        +response_format: Option~ResponseFormat~
        +seed: Option~i64~
        +logprobs: Option~bool~
        +top_logprobs: Option~usize~
        +parallel_tool_calls: Option~bool~
        +prompt_id: Option~String~
        +prompt_variables: Option~HashMap~
        +provider_params: Option~Value~
        +timeout: Option~f64~
    }

    class HttpCompletionResponse {
        +id: String
        +object: String
        +created: u64
        +model: String
        +choices: Vec~Choice~
        +usage: Usage
        +metadata: Option~Value~
    }

    class ChatCompletion {
        +id: String
        +object: String
        +created: u64
        +model: String
        +choices: Vec~Choice~
        +usage: Usage
    }

    HttpCompletionRequest --> HttpCompletionResponse
    ChatCompletion --> Choice
    ChatCompletion --> Usage
```

---

## 6. Module Architecture

### 6.1 Core Module Dependency Graph

```mermaid
graph TD
    subgraph Entry["Entry Points"]
        E1[proxy.rs<br/>HTTP Proxy]
        E2[python_sdk_entry/<br/>Python SDK]
    end

    subgraph Routing["Routing Layer"]
        R1[mode.rs<br/>Mode Selection]
        R2[router.rs<br/>Provider Routing]
        R3[config.rs<br/>Configuration]
        R4[pre_call_checks.rs<br/>Context Window]
    end

    subgraph Reliability["Reliability Layer"]
        RL1[fallback.rs<br/>Fallback & Health]
        RL2[rate_limit.rs<br/>Rate Limiting]
        RL3[key_rate_limiter.rs<br/>Key Rate Limiting]
    end

    subgraph Enterprise["Enterprise Features"]
        EN1[cache.rs<br/>Response Cache]
        EN2[callbacks/<br/>Event Hooks]
        EN3[guardrails/<br/>Content Filters]
        EN4[prompts/<br/>Prompt Registry]
        EN5[pricing.rs<br/>Cost Tracking]
    end

    subgraph Security["Security Layer"]
        S1[auth/<br/>Authentication]
        S2[keys/<br/>API Keys]
        S3[secret_manager.rs<br/>Secrets]
    end

    subgraph Storage["Storage Layer"]
        ST1[storage.rs<br/>Storage Trait]
        ST2[health.rs<br/>Health Checks]
    end

    subgraph Providers["Provider Layer"]
        P1[native_http/<br/>HTTP Providers]
        P2[py_bridge/<br/>Python Providers]
    end

    E1 --> R1
    E2 --> R1
    R1 --> P1
    R1 --> P2
    E1 --> R2
    R2 --> R3
    R2 --> R4
    R4 --> RL1
    E1 --> RL2
    E1 --> EN1
    E1 --> EN2
    E1 --> EN3
    E1 --> EN4
    E1 --> S1
    S1 --> S2
    S2 --> ST1

    style Entry fill:#e3f2fd
    style Routing fill:#e8f5e9
    style Reliability fill:#fff3e0
    style Enterprise fill:#fce4ec
    style Security fill:#f3e5f5
    style Storage fill:#ffebee
    style Providers fill:#e0f7fa
```

### 6.2 Module Descriptions

| Module | File | Purpose |
|--------|------|---------|
| **admin** | `admin.rs` | Admin API endpoints |
| **auth** | `auth/` | Authentication (API keys, SSO, JWT) |
| **balance** | `balance.rs` | Balance tracking for OCTO-W budgets |
| **cache** | `cache.rs` | Response caching |
| **callbacks** | `callbacks/` | Event hooks for logging, metrics |
| **config** | `config.rs` | Configuration loading, dispatch map, model groups |
| **fallback** | `fallback.rs` | Fallback chains, health tracking, circuit breaking |
| **guardrails** | `guardrails/` | Content filtering, safety checks |
| **health** | `health.rs` | Health check endpoints |
| **key_rate_limiter** | `key_rate_limiter.rs` | Per-key rate limiting |
| **keys** | `keys/` | Virtual API key management |
| **logging** | `logging.rs` | Structured logging setup |
| **metrics** | `metrics.rs` | Prometheus metrics collection |
| **middleware** | `middleware.rs` | HTTP middleware (auth, logging, rate limit) |
| **mode** | `mode.rs` | Mode selection (litellm vs any-llm), default mode |
| **model** | `model.rs` | Model parsing and validation (any-llm-mode/full only) |
| **pre_call_checks** | `pre_call_checks.rs` | Context window validation, pre-flight checks |
| **pricing** | `pricing.rs` | Cost calculation, budget tracking |
| **prompts** | `prompts/` | Prompt template management |
| **providers** | `providers.rs` | Provider registry and trait definitions |
| **proxy** | `proxy.rs` | HTTP proxy server, request handling, endpoint routing |
| **py_bridge** | `py_bridge/` | 42 providers using Python SDKs |
| **python_sdk_entry** | `python_sdk_entry/` | Python SDK entry point (PyO3 module) |
| **rate_limit** | `rate_limit.rs` | Rate limiting per provider/model |
| **router** | `router.rs` | Provider routing strategies, load balancing |
| **schema** | `schema.rs` | JSON schema validation |
| **secret_manager** | `secret_manager.rs` | Secret storage and retrieval |
| **shared_types** | `shared_types.rs` | Types shared between crates (Message, Choice, Usage) |
| **storage** | `storage.rs` | Storage trait, persistence abstraction |
| **tracing** | `tracing.rs` | Distributed tracing setup |
| **types** | `types.rs` | Per-crate types (ChatCompletion, etc.) (any-llm-mode/full only) |
| **native_http** | `native_http/` | 12 providers using reqwest HTTP |

---

## 7. Data Types

### 7.1 Type Hierarchy

```mermaid
classDiagram
    class Message {
        +role: String
        +content: Option~String~
        +name: Option~String~
        +tool_calls: Option~Vec~ToolCall~~
        +tool_call_id: Option~String~
        +function_call: Option~FunctionCall~
    }

    class Choice {
        +index: u32
        +message: Message
        +finish_reason: String
        +logprobs: Option~LogProbs~
    }

    class Usage {
        +prompt_tokens: u32
        +completion_tokens: u32
        +total_tokens: u32
    }

    class ChatCompletion {
        +id: String
        +object: String
        +created: u64
        +model: String
        +choices: Vec~Choice~
        +usage: Usage
        +to_dict(py) Py~PyAny~
    }

    class DispatchInfo {
        +deployment_id: String
        +provider: String
        +model: String
        +api_key: Option~String~
        +api_base: Option~String~
        +rpm: u32
        +tpm: u64
        +model_group: Option~String~
        +metadata: Option~HashMap~String~~String~~
        +max_retries: Option~u32~
    }

    class ProviderMode {
        <<enum>>
        LiteLLM
        AnyLlm
    }

    ChatCompletion --> Choice
    ChatCompletion --> Usage
    Choice --> Message
```

### 7.2 Shared Types vs Crate-Specific Types

```mermaid
graph LR
    subgraph Shared["shared_types.rs<br/>(shared between crates)"]
        S1[Message]
        S2[Choice]
        S3[Usage]
    end

    subgraph Types["types.rs<br/>(per-crate)"]
        T1[ChatCompletion]
    end

    subgraph Core["native_http types"]
        C1[HttpMessage]
        C2[HttpCompletionRequest]
        C3[HttpCompletionResponse]
        C4[HttpEmbeddingRequest]
    end

    subgraph PyO3["pyo3 types"]
        P1[PyMessage]
        P2[PyChatCompletion]
    end

    S1 <--> C1
    T1 <--> C3
    S1 <--> P1
    T1 <--> P2

    style Shared fill:#e8f5e9
    style Types fill:#fff9c4
    style Core fill:#e3f2fd
    style PyO3 fill:#fce4ec
```

---

## 8. Error Handling

### 8.1 Error Hierarchy

```mermaid
classDiagram
    class QuotaRouterError {
        <<Python Exception>>
        +message: String
        +llm_provider: Option~String~
    }

    class AuthenticationError {
        <<401>>
    }

    class RateLimitError {
        <<429>>
        +retry_after: Option~String~
    }

    class InvalidRequestError {
        <<400>>
    }

    class ProviderError {
        <<500>>
    }

    class ModelNotFoundError {
        <<404>>
    }

    class ContextLengthExceededError {
        <<Context>>
    }

    class ContentFilterError {
        <<Safety>>
    }

    class MissingApiKeyError {
        <<Auth>>
    }

    class UnsupportedProviderError {
        <<Config>>
    }

    class UnsupportedParameterError {
        <<Config>>
    }

    class InsufficientFundsError {
        <<Budget>>
    }

    class UpstreamProviderError {
        <<502>>
    }

    class GatewayTimeoutError {
        <<504>>
    }

    class LengthFinishReasonError {
        <<Finish>>
    }

    class ContentFilterFinishReasonError {
        <<Finish>>
    }

    class BatchNotCompleteError {
        <<Batch>>
    }

    class AllModelsFailedError {
        <<Router>>
    }

    class BatchPartialFailureError {
        <<Batch>>
    }

    QuotaRouterError <|-- AuthenticationError
    QuotaRouterError <|-- RateLimitError
    QuotaRouterError <|-- InvalidRequestError
    QuotaRouterError <|-- ProviderError
    QuotaRouterError <|-- ModelNotFoundError
    QuotaRouterError <|-- ContextLengthExceededError
    QuotaRouterError <|-- ContentFilterError
    QuotaRouterError <|-- MissingApiKeyError
    QuotaRouterError <|-- UnsupportedProviderError
    QuotaRouterError <|-- UnsupportedParameterError
    QuotaRouterError <|-- InsufficientFundsError
    QuotaRouterError <|-- UpstreamProviderError
    QuotaRouterError <|-- GatewayTimeoutError
    QuotaRouterError <|-- LengthFinishReasonError
    QuotaRouterError <|-- ContentFilterFinishReasonError
    QuotaRouterError <|-- BatchNotCompleteError
    QuotaRouterError <|-- AllModelsFailedError
    QuotaRouterError <|-- BatchPartialFailureError
```

### 8.2 Error Mapping

```mermaid
graph LR
    subgraph HTTP["HTTP Status"]
        H1[401]
        H2[429]
        H3[400]
        H4[404]
        H5[500]
        H6[504]
    end

    subgraph Provider["ProviderError"]
        PE1[AuthError]
        PE2[RateLimit]
        PE3[InvalidResponse]
        PE4[UnsupportedModel]
        PE5[Network]
    end

    subgraph Python["Python Exceptions"]
        PY1[AuthenticationError]
        PY2[RateLimitError]
        PY3[InvalidRequestError]
        PY4[ModelNotFoundError]
        PY5[ProviderError]
        PY6[GatewayTimeoutError]
    end

    H1 --> PE1 --> PY1
    H2 --> PE2 --> PY2
    H3 --> PE3 --> PY3
    H4 --> PE3 --> PY4
    H5 --> PE3 --> PY5
    H6 --> PE3 --> PY6

    style HTTP fill:#fce4ec
    style Provider fill:#fff3e0
    style Python fill:#e3f2fd
```

### 8.3 Dual QuotaRouterError

There are two distinct `QuotaRouterError` types in the codebase:

1. **PyO3 Python exception** (`crates/quota-router-pyo3/src/exceptions.rs`) — the hierarchy shown above, used for Python-facing errors
2. **Rust `thiserror` enum** (`crates/quota-router-core/src/keys/errors.rs`) — wraps domain-specific Rust errors (`KeyError`, `BudgetError`, `RouterError`, `RegistryError`, `StorageError`, `ProviderError`)

These are separate types with different variant sets. The PyO3 hierarchy is what Python users interact with; the `thiserror` enum is used internally by the Rust core.

### 8.4 LiteLLM-Compatible Aliases

| Quota Router Name | LiteLLM/AnyLLM Alias |
|-------------------|---------------|
| `InsufficientFundsError` | `BudgetExceededError` |
| `UpstreamProviderError` | `ServiceUnavailableError` |
| `GatewayTimeoutError` | `APIConnectionError`, `Timeout` |
| `QuotaRouterError` | `APIError`, `AnyLLMError` |
| `ModelNotFoundError` | `NotFoundError` |
| `ContextLengthExceededError` | `ContextWindowExceededError` |
| `ContentFilterError` | `ContentPolicyViolationError` |

---

## 9. Configuration

### 9.1 Configuration Hierarchy

```mermaid
graph TD
    subgraph Config["Configuration Sources"]
        C1[config.yaml<br/>Main Config]
        C2[Environment Variables]
        C3[Runtime Overrides]
    end

    subgraph ConfigModule["config.rs"]
        CM1[RouterSettings]
        CM2[DeploymentConfig]
        CM3[GatewayConfig]
        CM4[AnyLlmProviderConfig]
    end

    subgraph Dispatch["Dispatch Map"]
        D1[DispatchInfo]
        D2[Model → Provider Mapping]
        D3[API Key Resolution]
    end

    Config --> ConfigModule
    ConfigModule --> Dispatch

    style Config fill:#e3f2fd
    style ConfigModule fill:#e8f5e9
    style Dispatch fill:#fff3e0
```

### 9.2 Dispatch Flow

```mermaid
sequenceDiagram
    participant Request
    participant Config as config.rs
    participant Dispatch as Dispatch Map
    participant Key as API Key Resolution

    Request->>Config: model: "openai/gpt-4"
    Config->>Dispatch: Lookup model/group

    alt Model Match
        Dispatch-->>Config: DispatchInfo
    else Model Group Match
        Dispatch-->>Config: DispatchInfo (via model_group)
    else Alias Match
        Dispatch-->>Config: DispatchInfo (via alias)
    end

    Config->>Key: resolve_api_key(provider, config_key)

    alt Config Key
        Key-->>Config: config_key
    else Environment Variable
        Key-->>Config: ENV_VAR key
    else No Key
        Key-->>Config: None (forward without auth)
    end

    Config-->>Request: DispatchInfo + API Key
```

---

## 10. Deployment and Mode Selection

Three feature gates produce three build configurations. Both deployments (binary and pip) can be built with any of the three — the choice determines which interfaces and provider backends are included.

**Important:** Deployment (binary vs pip) and runtime mode (litellm vs any-llm) are
**orthogonal** to the interfaces exposed. Both deployments expose BOTH interfaces
(HTTP proxy + library/binding call). The mode gate controls the **provider integration
backend** (reqwest vs PyO3), not the available interfaces.

### 10.1 Feature Gate Matrix (verified against lib.rs)

| Feature Gate | `proxy` (HTTP) | `python_sdk_entry` (SDK) | `native_http` (reqwest) | `py_bridge` (PyO3) |
|--------------|----------------|--------------------------|-------------------------|---------------------|
| `litellm-mode` | ✅ | ❌ | ✅ | ❌ |
| `any-llm-mode` | ✅ | ✅ | ❌ | ✅ |
| `full` | ✅ | ✅ | ✅ | ✅ |

**Source:** `crates/quota-router-core/src/lib.rs` lines 32-85

- `proxy` — no feature gate, always compiled (line 37)
- `native_http` — `#[cfg(any(feature = "litellm-mode", feature = "full"))]` (line 47)
- `py_bridge` — `#[cfg(any(feature = "any-llm-mode", feature = "full"))]` (line 60)
- `python_sdk_entry` — `#[cfg(any(feature = "any-llm-mode", feature = "full"))]` (line 73)

### 10.2 Build Configurations

Each feature gate produces a distinct build with specific interfaces and providers:

```mermaid
graph TB
    subgraph Litellm["litellm-mode Build"]
        direction TB
        L1[HTTP Proxy<br/>Always Available]
        L2[native_http<br/>12 reqwest Providers]
        L3[Mode Router<br/>lite-only]
        style Litellm fill:#e8f5e9
    end

    subgraph AnyLlm["any-llm-mode Build"]
        direction TB
        A1[HTTP Proxy<br/>Always Available]
        A2[Python SDK<br/>python_sdk_entry]
        A3[py_bridge<br/>42 PyO3 Providers]
        A4[Mode Router<br/>any-only]
        style AnyLlm fill:#e3f2fd
    end

    subgraph Full["full Build"]
        direction TB
        F1[HTTP Proxy]
        F2[Python SDK]
        F3[native_http<br/>12 reqwest Providers]
        F4[py_bridge<br/>42 PyO3 Providers]
        F5[Mode Router<br/>switches at runtime]
        style Full fill:#fff3e0
    end
```

#### Build Configuration Summary

| Build | HTTP Proxy | Python SDK | reqwest Providers | PyO3 Providers | Mode Selection |
|-------|-----------|------------|-------------------|----------------|----------------|
| `litellm-mode` | ✅ | ❌ | 12 | ❌ | Fixed at compile time |
| `any-llm-mode` | ✅ | ✅ | ❌ | 42 | Fixed at compile time |
| `full` | ✅ | ✅ | 12 | 42 | **Runtime switchable** |

**Key point:** The mode gate controls which provider backend is available, not which interfaces. The `litellm-mode` build still has HTTP proxy + library interface; `any-llm-mode` build still has HTTP proxy + library interface. Only the provider backends differ (12 reqwest vs 42 PyO3).

### 10.3 Runtime Mode Selection

When compiled with `full`, the mode router selects which provider backend
is used. The mode does NOT change which interfaces are available — those
are determined at compile time by feature gates.

```mermaid
graph TB
    subgraph Interfaces["Available Interfaces<br/>(determined by feature gates)"]
        I1[HTTP Proxy<br/>always compiled]
        I2[Python SDK<br/>any-llm-mode or full only]
    end

    subgraph Mode["Mode Router<br/>(runtime selection)"]
        M1{Selected Mode?}
    end

    subgraph LiteLLM["litellm (reqwest)"]
        L1[HttpProviderFactory]
        L2[native_http Providers]
    end

    subgraph AnyLLM["any-llm (PyO3)"]
        A1[PyBridgeProviderFactory]
        A2[py_bridge Providers]
    end

    subgraph Providers["Provider APIs"]
        P1[44 Unique LLM Providers]
    end

    Interfaces --> Mode
    M1 -->|litellm| LiteLLM
    M1 -->|any-llm| AnyLLM
    LiteLLM --> Providers
    AnyLLM --> Providers

    style Interfaces fill:#e3f2fd
    style Mode fill:#fff3e0
    style LiteLLM fill:#e8f5e9
    style AnyLLM fill:#fce4ec
    style Providers fill:#f3e5f5
```

---

## 11. Test Architecture

### 11.1 Test Layers

```mermaid
graph TB
    subgraph Tests["Test Pyramid"]
        direction TB
        T1["Unit Tests<br/>492 tests<br/>quota-router-core"]
        T2["PyO3 Unit Tests<br/>16 tests<br/>quota-router-pyo3"]
        T3["Rust E2E Tests<br/>15 tests<br/>proxy + real endpoint"]
        T4["Python E2E Tests<br/>25 tests<br/>SDK + real endpoint"]
        T5["Drop-in Tests<br/>75 tests<br/>litellm + any-llm compat"]
        T6["Extended SDK Tests<br/>68 tests<br/>extended_sdk + list_models"]
        T7["Anthropic E2E Tests<br/>18 tests<br/>Anthropic endpoint"]
        T8["Smoke Tests<br/>8 tests"]
    end

    T1 --> T3
    T2 --> T4
    T3 --> T4
    T4 --> T5
    T5 --> T6
    T6 --> T7
    T7 --> T8

    style T1 fill:#e8f5e9
    style T2 fill:#e8f5e9
    style T3 fill:#e3f2fd
    style T4 fill:#fff3e0
    style T5 fill:#fce4ec
    style T6 fill:#fce4ec
    style T7 fill:#f3e5f5
    style T8 fill:#f3e5f5
```

### 11.2 Test Coverage

| Test Type | Count | Coverage |
|-----------|-------|----------|
| Unit tests (core) | 492 | All modules (445 `#[test]` + 47 `#[tokio::test]`) |
| PyO3 unit tests | 16 | quota-router-pyo3 bindings |
| Rust E2E (proxy) | 15 | OpenAI endpoint via proxy |
| Python E2E (SDK) | 25 | OpenAI endpoint via SDK |
| Drop-in litellm | 35 | litellm compatibility |
| Drop-in any-llm | 40 | any-llm compatibility |
| Extended SDK | 38 | Extended SDK functions |
| List models | 30 | Model listing via SDK |
| Anthropic E2E | 18 | Anthropic endpoint (both modes) |
| Smoke tests | 8 | Basic integration checks |
| **Total** | **717** | |

### 11.3 Test Endpoints

| Endpoint | Auth | Used By |
|----------|------|---------|
| `opengateway.gitlawb.com/v1/xiaomi-mimo` | None | OpenAI e2e tests |
| `api.minimax.io/anthropic` | ANTHROPIC_AUTH_TOKEN | Anthropic e2e tests |

---

## Appendix A: Provider List

### Native HTTP Providers (litellm-mode)

| Provider | File | Streaming | Embeddings |
|----------|------|-----------|------------|
| OpenAI | `openai.rs` | Yes | Yes |
| Anthropic | `anthropic.rs` | Yes | No |
| Mistral | `mistral.rs` | Yes | Yes |
| Groq | `groq.rs` | Yes | Yes |
| Together | `together.rs` | Yes | Yes |
| Azure | `azure.rs` | Yes | Yes |
| Databricks | `databricks.rs` | Yes | Yes |
| Perplexity | `perplexity.rs` | Yes | Yes |
| Ollama | `ollama.rs` | Yes | Yes |
| Bedrock | `bedrock.rs` | Yes | No |
| Gemini | `gemini.rs` | Yes | Yes |
| Replicate | `replicate.rs` | Yes | No |

### PyBridge Providers (any-llm-mode)

42 providers total. Includes 10 of 12 native HTTP providers (excludes Databricks
and Perplexity) plus:
AI21, AI Foundry, Aleph Alpha, Cerebras, CloudflareAI, Cohere, Conjure,
DashScope, DeepInfra, DeepSeek, Fireworks, HuggingFace, Inception, Infere,
Level AI, LlamaCpp, Llamafile, LMStudio, MiniMax, Mistral Large, Moonshot,
Nebius, NVIDIA, OpenRouter, Portkey, Sagemaker, Sambanova, VertexAI, Voyage,
Watsonx, WorkersAI, XAI.

---

## Appendix B: API Surface

### Python SDK Functions

| Function | Mode | Status |
|----------|------|--------|
| `completion()` | Both | Implemented |
| `acompletion()` | Both | Implemented |
| `text_completion()` | Both | Implemented |
| `atext_completion()` | Both | Implemented |
| `embedding()` | litellm | Implemented (litellm only) |
| `aembedding()` | litellm | Implemented (litellm only) |
| `messages()` | litellm | Implemented (litellm only) |
| `amessages()` | litellm | Implemented (litellm only) |
| `responses()` | litellm | Implemented (litellm only) |
| `aresponses()` | litellm | Implemented (litellm only) |
| `batch_create()` | litellm | Implemented (litellm only) |
| `batch_retrieve()` | litellm | Implemented (litellm only) |
| `batch_cancel()` | litellm | Implemented (litellm only) |
| `batch_list()` | litellm | Implemented (litellm only) |
| `batch_results()` | litellm | Implemented (litellm only) |
| `list_models()` | litellm | Implemented (litellm only) |
| `alist_models()` | litellm | Implemented (litellm only) |
| `get_response()` | litellm | Implemented (litellm only) |
| `delete_response()` | litellm | Implemented (litellm only) |
| `abatch_create()` | litellm | Implemented (litellm only) |
| `abatch_retrieve()` | litellm | Implemented (litellm only) |
| `abatch_cancel()` | litellm | Implemented (litellm only) |
| `abatch_list()` | litellm | Implemented (litellm only) |
| `abatch_results()` | litellm | Implemented (litellm only) |
| `aget_response()` | litellm | Implemented (litellm only) |
| `adelete_response()` | litellm | Implemented (litellm only) |

### SDK Management Functions

| Function | Status |
|----------|--------|
| `set_api_key()` | Implemented |
| `get_budget_status()` | Implemented |
| `get_metrics()` | Implemented |
| `parse_model()` | Implemented |
| `parse_model_strict()` | Implemented |

### Provider Functions

| Function | Status |
|----------|--------|
| `get_supported_providers()` | Implemented |
| `is_provider_supported()` | Implemented |
| `get_provider_info()` | Implemented |

### Batch Completion Functions

| Function | Status |
|----------|--------|
| `batch_completion()` | Implemented |
| `batch_completion_models()` | Implemented |
| `batch_completion_models_all_responses()` | Implemented |

### Routing Strategies

**Source:** `crates/quota-router-core/src/router.rs` lines 10-44 (`RoutingStrategy` enum)

| Strategy | Description |
|----------|-------------|
| `simple-shuffle` | Random provider selection (default) |
| `round-robin` | Cyclic rotation across providers |
| `least-busy` | Select provider with fewest active requests |
| `latency-based` | Route to lowest-latency provider |
| `cost-based` | Route to cheapest provider per token |
| `usage-based` | Balance based on token usage distribution |
| `usage-based-v2` | Improved usage-based with smoothing |
| `weighted` | User-configured provider weights |

### Router Class

| Method | Status |
|--------|--------|
| `__init__()` / `new()` | Implemented |
| `completion()` | Implemented |
| `acompletion()` | Implemented |
| `list_models()` | Implemented |
| `get_metrics()` | Implemented |
| `get_stats()` | Implemented |
| `get_strategy()` | Implemented |
| `set_strategy()` | Implemented |
| `get_models()` | Implemented |
| `__len__()` | Implemented |
| `__repr__()` | Implemented |

---

*End of document*
