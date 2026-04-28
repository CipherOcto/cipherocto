# RFC-0920 (Economics): Unified Python SDK — Dual-Mode LiteLLM/any-llm Compatibility

## Status

Draft (v1.19 — 2026-04-28)

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu

## Summary

Define a unified Python SDK via PyO3 that supports **both** LiteLLM-style API (separate `provider` param) and any-llm-style API (`provider:model` format), enabling drop-in replacement for both LiteLLM and any-llm users. This RFC supersedes RFC-0908 and extends RFC-0917 Phase 3 to provide a single SDK that works in both deployment modes.

## Dependencies

**Requires:**

- RFC-0917: Dual-Mode Query Router (Accepted)
- RFC-0904: Real-Time Cost Tracking (for `InsufficientFundsError`)
- RFC-0913: stoolap-pubsub-cache-invalidation (Required for `cache_responses` via stoolap semantic cache)

**Optional:**

- RFC-0902: Multi-Provider Routing and Load Balancing (Required for Router class)
- RFC-0903: Virtual API Key System
- RFC-0909: Deterministic Quota Accounting
- RFC-0910: Pricing Table Registry

**⚠️ MODE GATE ≠ INTERFACE (per RFC-0917):**
Both HTTP proxy and Python SDK exist in ALL modes (litellm-mode, any-llm-mode, full).
Mode gate controls provider strategy (reqwest vs PyO3), NOT interface availability.

## Motivation

### The Problem

RFC-0908 (Python SDK PyO3 Bindings) describes a LiteLLM-compatible SDK with `provider` as a separate parameter. RFC-0917 Phase 3 describes any-llm-mode with `provider:model` format. Both modes must be supported for full compatibility:

- **LiteLLM users** expect `completion(provider="openai", model="gpt-4o", messages=[...])`
- **any-llm users** expect `completion(model="openai:gpt-4o", messages=[...])`

The current `quota-router-pyo3` crate only implements any-llm-style (per RFC-0917 Phase 3), and even that is a mock stub without real provider integrations.

### Why Needed

- **Dual compatibility**: Users of both LiteLLM and any-llm can migrate to quota-router
- **Maximum adoption**: Neither ecosystem has to change their API patterns
- **Enterprise flexibility**: Deployments choose LiteLLM-mode or any-llm-mode or both
- **RFC-0908 supersession**: RFC-0908 is LiteLLM-only; this RFC covers both

## Design Goals

| Goal | Target                         | Metric                                                                            |
| ---- | ------------------------------ | --------------------------------------------------------------------------------- |
| G1   | <10ms function call overhead   | Latency (PyO3 call into Rust)                                                     |
| G2   | 100% LiteLLM API compatibility | Test coverage against LiteLLM test suite                                          |
| G3   | 100% any-llm API compatibility | Test coverage against any-llm test suite                                          |
| G4   | pip installable                | PyPI package                                                                      |
| G5   | Type hints                     | mypy pass                                                                         |
| G6   | Both API styles work           | `completion(provider="openai", ...)` AND `completion(model="openai:gpt-4o", ...)` |

## Specification

### ⚠️ CRITICAL INVARIANT — Mode Gate ≠ Interface

**Per RFC-0917, this is mathematically always true:**

```
For ALL modes (litellm-mode, any-llm-mode, full):
    HTTP proxy interface EXISTS ✅
    Python SDK interface EXISTS ✅

Mode gate controls ONLY: what library calls providers (reqwest vs PyO3)
Mode gate does NOT control: which interfaces exist
```

**Never forget:**
- `litellm-mode` DOES NOT mean "HTTP proxy only"
- `any-llm-mode` DOES NOT mean "Python SDK only"
- Both interfaces exist in ALL modes
- Mode selects provider strategy (reqwest vs PyO3), not interface availability

### Crate Architecture

`quota-router-pyo3` is the **Python SDK crate** that wraps `quota-router-core` via PyO3:

```
┌─────────────────────────────────────────────────────────────────┐
│              quota-router-pyo3 (Python SDK)                      │
│  • Registers completion(), acompletion(), set_api_key(), etc.     │
│  • Calls Rust core via PyO3 (extern crate)                       │
│  • Provider resolution (provider:model parsing)                    │
│  • Python-level Router class (selects deployment, calls completion)│
│  • Exception mapping (Python → unified types)                    │
└─────────────────────────────────────────────────────────────────┘
                              │ PyO3
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│              quota-router-core (Rust core)                       │
│  • KeyMiddleware    — API key validation                        │
│  • Balance         — OCTO-W spend tracking                      │
│  • StoolapKeyStorage — Persistence (RFC-0912/0914)            │
│  • KeyCache (L1)   — In-memory key cache with TTL             │
│  • RateLimiter     — TokenBucket RPM/TPM enforcement            │
│  • Router          — Index-based selection (proxy server)        │
│  • FallbackExecutor — Retry with backoff                       │
│  • Provider        — Provider config (endpoint, rpm, tpm, weight) │
│  • PricingRegistry — Token pricing (RFC-0910)                   │
└─────────────────────────────────────────────────────────────────┘
```

**Python-to-Rust component mapping:**

| Python API | Rust Core Component | Notes |
|------------|---------------------|-------|
| `set_api_key(provider, key)` | `KeyMiddleware::validate_key()` + `StoolapKeyStorage` | Validates then persists |
| `get_budget_status()` | `Balance` + `StoolapKeyStorage` | Returns OCTO-W spend |
| `completion()` | `KeyMiddleware` → Provider call | Key validation → actual call |
| `Router.route()` (at Python level) | None | Python-level deployment selection |
| `num_retries` | `FallbackExecutor` (in proxy) | Retry via proxy, not Python SDK |
| `cache_responses` | `KeyCache` + `StoolapKeyStorage` | Semantic cache (RFC-0913) |
| Rate limiting | `RateLimiter` | TokenBucket enforcement |
| Exception mapping | `RouterError` → Python | PyO3 exception translation |

**Two modes (feature flags) control provider integration — interface availability is NOT mode-gated:**

| Mode | Provider Strategy | HTTP Proxy? | Python SDK? |
|------|-----------------|:------------:|:------------:|
| `litellm-mode` | reqwest HTTP (Rust) | ✅ Yes (reqwest-based) | ✅ Yes |
| `any-llm-mode` | PyO3 → Python SDK | ✅ Yes (via PyO3 bridge) | ✅ Yes |
| `full` | Both | ✅ Yes (both reqwest + embedded PyO3) | ✅ Yes |

**Mode gate controls HOW (reqwest vs PyO3), NOT WHETHER (proxy vs SDK).**

**Per RFC-0917 §Scope: HTTP Proxy Server is "(always)" — it exists in all modes.**

In `any-llm-mode`, the HTTP proxy delegates to Python SDK providers via PyO3 bridge. This is architecturally supported because any-llm-mode already compiles the PyO3 bridge — the HTTP proxy can use it to call Python SDKs.

**Key insight**: Mode determines which HTTP/client layer is compiled. The Python SDK (`quota-router-pyo3`) is always the Python interface — it wraps the Rust core regardless of mode.

### Runtime Mode Selection (full builds)

**Severity: Critical**

In `full` builds (both reqwest and PyO3 compiled), the active mode can be selected at **runtime** via environment variable:

```python
import os

def get_deployment_mode() -> str:
    """
    Returns the active runtime mode.

    Precedence:
    1. QUOTA_ROUTER_MODE environment variable (if set to litellm-mode, any-llm-mode, or full)
    2. Compile-time embedded mode (from Cargo feature flags)

    Examples:
        QUOTA_ROUTER_MODE=any-llm-mode  # Force Python SDK delegation
        QUOTA_ROUTER_MODE=litellm-mode   # Force reqwest HTTP
        QUOTA_ROUTER_MODE=full           # Use compile-time default (both available)

    Validation: If QUOTA_ROUTER_MODE is set to a mode not compiled in the binary,
    the function returns the compile-time embedded mode and logs a warning.
    For example, if QUOTA_ROUTER_MODE=any-llm-mode but the binary only has litellm-mode
    compiled, it falls back to litellm-mode with a warning.
    """
    env_mode = os.environ.get("QUOTA_ROUTER_MODE")
    if env_mode in ("litellm-mode", "any-llm-mode", "full"):
        # Validate against compile-time capabilities
        compiled_modes = _get_compiled_modes()  # Returns set of compiled modes
        if env_mode in compiled_modes or env_mode == "full":
            return env_mode
        else:
            # Requested mode not compiled in — fall back with warning
            warnings.warn(
                f"QUOTA_ROUTER_MODE={env_mode} not compiled in this binary. "
                f"Compiled modes: {compiled_modes}. Falling back to {_EMBEDDED_MODE}."
            )
    return _EMBEDDED_MODE  # Compile-time mode
```

**For pip-installed wheels:** Set `QUOTA_ROUTER_MODE` to switch between LiteLLM-compatible (reqwest) and any-llm-compatible (PyO3) behavior without reinstalling.

**Scope note:** `QUOTA_ROUTER_MODE` affects only the **Python SDK** interface's provider strategy. The HTTP proxy in a `full` build uses a separate configuration (e.g., in `config.yaml`) to determine its provider strategy. The proxy can also be forced to a specific strategy at startup, independent of the SDK's runtime mode.

### Dual-Mode API Conventions

**⚠️ Mode ≠ Interface reminder:** Both HTTP proxy and Python SDK exist in ALL modes. The mode selects provider strategy (reqwest vs PyO3), not which interface is available.

The SDK operates in two API conventions (not feature flags — both work in all modes):

```
┌─────────────────────────────────────────────────────────────────┐
│                    quota_router Python SDK                       │
│                                                                 │
│  ┌─────────────────────┐    ┌─────────────────────┐            │
│  │   LiteLLM Convention │    │  any-llm Convention  │            │
│  │   provider param    │    │  provider:model      │            │
│  │                     │    │                     │            │
│  │ completion(        │    │ completion(         │            │
│  │   provider="openai",│    │   model="openai:...",│           │
│  │   model="gpt-4o",  │    │   messages=[...]   │            │
│  │   messages=[...]   │    │ )                   │            │
│  │ )                   │    │                     │            │
│  └─────────────────────┘    └─────────────────────┘            │
│                                                                 │
│  Both conventions work regardless of mode (litellm/any-llm/full) │
└─────────────────────────────────────────────────────────────────┘
```

**Convention determines:**
1. How provider is specified (explicit param vs embedded in model string)
2. Default provider when not specified

### API Style 1: LiteLLM-Compatible

```python
# LiteLLM style — provider as separate parameter
from quota_router import completion, acompletion

# Sync
response = completion(
    provider="openai",      # Required in LiteLLM mode, optional in any-llm mode
    model="gpt-4o",
    messages=[{"role": "user", "content": "Hello"}],
    temperature=0.7,
    max_tokens=100,
    api_key="sk-...",      # Optional — falls back to env var
    api_base="https://...", # Optional — provider default
)

# Async
response = await acompletion(
    provider="anthropic",
    model="claude-3-5-sonnet",
    messages=[{"role": "user", "content": "Hello"}],
)
```

### API Style 2: any-llm-Compatible

```python
# any-llm style — provider embedded in model string
from quota_router import completion, acompletion

# Sync
response = completion(
    model="openai:gpt-4o",           # Provider:model format
    messages=[{"role": "user", "content": "Hello"}],
)

# Async
response = await acompletion(
    model="anthropic:claude-3-5-sonnet",
    messages=[{"role": "user", "content": "Hello"}],
)

# Must call set_api_key() first in any-llm mode
from quota_router import set_api_key
set_api_key("anthropic", "sk-ant-...")
```

**Class-based API (any-llm style):** any-llm has `AnyLLM` class with instance methods (`anyllm.completion()`, `anyllm.acompletion()`). The RFC speccs only the **functional API** (`completion()`, `acompletion()`). Class-based API is **NOT in scope** for Phase 1 — only the module-level functions.

### Unified Function Signature

```python
async def acompletion(
    model: str,                                    # "openai:gpt-4o" or just "gpt-4o"
    messages: List[Dict[str, str]],               # LiteLLM message format
    *,
    # Provider (LiteLLM style — separate param)
    provider: Optional[str] = None,               # "openai", "anthropic", etc.

    # API credentials (override env vars)
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    base_url: Optional[str] = None,  # Alias for api_base (LiteLLM compat)

    # Completion parameters
    temperature: Optional[float] = None,
    top_p: Optional[float] = None,
    max_tokens: Optional[int] = None,
    max_completion_tokens: Optional[int] = None,
    n: Optional[int] = None,
    stream: Optional[bool] = None,  # None = sync response; True = streaming; matches LiteLLM behavior
    stream_options: Optional[Dict] = None,
    timeout: Optional[Union[float, int]] = None,  # async uses float|int; sync uses float|str|httpx.Timeout (see sync completion)
    # Note: async and sync timeout types differ per LiteLLM:
    # - acompletion: timeout: Optional[Union[float, int]]
    # - completion: timeout: Optional[Union[float, str, httpx.Timeout]]
    stop: Optional[Union[str, List[str]]] = None,
    presence_penalty: Optional[float] = None,
    frequency_penalty: Optional[float] = None,
    logit_bias: Optional[Dict[int, float]] = None,
    user: Optional[str] = None,
    seed: Optional[int] = None,

    # Reasoning (Anthropic, OpenAI o1)
    reasoning_effort: Optional[str] = None,  # LiteLLM-style: "none", "minimal", "low", "medium", "high", "xhigh", "default", "auto"
    # Note: `reasoning_effort` (string enum) is different from `thinking` (structured Dict below).

    # Tools / Function calling
    tools: Optional[List[Dict]] = None,
    tool_choice: Optional[Union[str, Dict]] = None,
    parallel_tool_calls: Optional[bool] = None,

    # Legacy function calling (deprecated — use tools/tool_choice)
    # Kept for LiteLLM compatibility — PASSED THROUGH to provider SDK
    functions: Optional[List] = None,  # Deprecated: use tools; passed through as-is
    function_call: Optional[str] = None,  # Deprecated: use tool_choice; passed through as-is

    # Response format (structured output)
    response_format: Optional[Union[str, Dict, Type[Any]]] = None,

    # LiteLLM extras
    logprobs: Optional[bool] = None,
    top_logprobs: Optional[int] = None,
    session_label: Optional[str] = None,
    client_args: Optional[Dict] = None,
    extra_headers: Optional[Dict] = None,  # LiteLLM extra_headers (also in sync completion)
    deployment_id: Optional[str] = None,  # LiteLLM deployment selection
    verbosity: Optional[Literal["low", "medium", "high"]] = None,  # LiteLLM verbosity

    # Thinking parameter (Anthropic structured thinking budget — different from reasoning_effort)
    thinking: Optional[Dict] = None,  # any-llm/LiteLLM structured Dict: {"type": "enabled"|"auto", "budget_tokens": int}

    # LiteLLM session and validation
    shared_session: Optional[Any] = None,  # ClientSession for session management
    web_search_options: Optional[Dict] = None,  # OpenAI web search options
    enable_json_schema_validation: Optional[bool] = None,  # Per-request JSON schema validation override

    # Remaining kwargs passed to provider
    **kwargs,
) -> CompletionResponse:
    """
    Unified completion supporting both LiteLLM and any-llm calling conventions.

    Resolution order:
    1. Explicit provider param (LiteLLM style)
    2. Provider from model string "provider:model" (any-llm style)
    3. Default provider from deployment mode

    Examples:
        # LiteLLM style
        acompletion(provider="openai", model="gpt-4o", messages=[...])

        # any-llm style
        acompletion(model="openai:gpt-4o", messages=[...])

        # Hybrid (provider in param overrides model string)
        acompletion(provider="openai", model="anthropic:claude-3", messages=[...])
        # → Uses OpenAI, ignores model string prefix
    """
```

### Provider Resolution Algorithm

```python
# Default provider per mode (can be overridden via environment/config)
DEFAULT_PROVIDER_BY_MODE = {
    "litellm-mode": "openai",  # LiteLLM default
    "any-llm-mode": None,      # any-llm has no default — must use explicit provider
    "full": "openai",          # Full mode defaults to LiteLLM behavior
}

def resolve_provider(
    provider_param: Optional[str],
    model: str,
    deployment_mode: str,  # "litellm-mode" | "any-llm-mode" | "full"
) -> tuple[str, str]:
    """
    Returns (provider, model_name).

    Resolution priority:
    1. provider param if provided and non-empty
    2. Parse model string for "provider:model" format (colon delimiter ONLY)
    3. Use default provider for mode (litellm-mode/full: "openai"; any-llm-mode: raise)

    Note: Slash ("/") delimiter is NOT supported. Use colon (":") for provider:model format.
    Slash parsing was removed to avoid ambiguity with provider names that could match
    HuggingFace model paths (e.g., "mistralai/Mistral-7B").

    Raises:
        ValueError: If no provider can be determined (any-llm-mode behavior)
    """
    # 1. Explicit provider param wins
    if provider_param:
        return provider_param, model

    # 2. Parse model string (case-insensitive provider lookup)
    if ":" in model:
        provider, model_name = model.split(":", 1)
        provider_lower = provider.lower()
        if is_known_provider(provider_lower):
            # Ambiguity check: if model_name equals provider, warn
            if model_name.lower() == provider_lower:
                import warnings
                warnings.warn(
                    f"Ambiguous model string '{model}' — provider and model name are identical. "
                    f"Assuming provider='{provider_lower}', model='{model_name}'. "
                    f"To silence this, use explicit provider= parameter.",
                    UserWarning,
                    stacklevel=2,
                )
            return provider_lower, model_name

    # 3. Mode-aware fallback or error
    default_provider = DEFAULT_PROVIDER_BY_MODE.get(deployment_mode)
    if default_provider:
        return default_provider, model
    # any-llm-mode: no default, raise error (matches any-llm behavior)
    raise ValueError(
        f"Invalid model format '{model}'. Expected 'provider:model' format "
        f"or pass provider='<name>' parameter. Known providers: {', '.join(sorted(KNOWN_PROVIDERS))}"
    )
```

### Supported Providers (42)

Both modes support identical 42 providers (union of any-llm + missing providers):

```
openai, anthropic, mistral, ollama, gemini,
azure, azureopenai, azureanthropic, bedrock, cerebras,
cohere, dashscope, databricks, deepseek, fireworks,
gateway, groq, huggingface, inception, llama,
llamacpp, llamafile, lmstudio, minimax, moonshot,
mzai, nebius, openrouter, perplexity, platform,
portkey, sagemaker, sambanova, together, vertexai,
vertexaianthropic, vllm, voyage, watsonx, xai, zai,
deepinfra
```

**Gap vs any-llm**: any-llm has 39 providers; quota-router adds `deepinfra` (not in any-llm).

**Gap vs litellm**: litellm has 100+ providers. Missing from quota-router: `replicate`, `azure_ai`, `bedrock_mantle`, `anyscale`, `fireworks_ai`, `localai`, `manifest`, `mimechat`, `nlp_cloud`, `predibase`, `proai`, `qianfan`, `sagemaker_chat`, `together_ai`, `yandex`, `yi`, `zhipuai`, and many `openai_like` providers. These can be added as needed via the provider plugin system.

Matches LiteLLM exceptions + quota-router specifics:

```python
# quota_router/exceptions.py

# Error codes for programmatic handling (matching LiteLLM convention)
ERROR_CODES = {
    "AUTH_ERROR": "AuthenticationError",
    "RATE_LIMIT": "RateLimitError",
    "INVALID_REQUEST": "InvalidRequestError",
    "PROVIDER_ERROR": "ProviderError",
    "CONTENT_FILTER": "ContentFilterError",
    "MODEL_NOT_FOUND": "ModelNotFoundError",
    "CONTEXT_LENGTH": "ContextLengthExceededError",
    "MISSING_API_KEY": "MissingApiKeyError",
    "UNSUPPORTED_PROVIDER": "UnsupportedProviderError",
    "UNSUPPORTED_PARAM": "UnsupportedParameterError",
    "INSUFFICIENT_FUNDS": "InsufficientFundsError",
    "UPSTREAM_ERROR": "UpstreamProviderError",
    "GATEWAY_TIMEOUT": "GatewayTimeoutError",
    "LENGTH_FINISH": "LengthFinishReasonError",
    "CONTENT_FILTER_FINISH": "ContentFilterFinishReasonError",
    "BATCH_NOT_COMPLETE": "BatchNotCompleteError",
}

class QuotaRouterError(Exception):
    """Base exception for all quota-router errors."""
    code: str                          # Error code string
    provider: Optional[str] = None     # Provider name if applicable

    def __init__(self, message: str, code: str, provider: Optional[str] = None):
        super().__init__(message)
        self.code = code
        self.provider = provider

class AuthenticationError(QuotaRouterError):
    """Invalid or missing API key."""
    pass

class RateLimitError(QuotaRouterError):
    """Rate limit exceeded."""
    retry_after: Optional[float]

class InvalidRequestError(QuotaRouterError):
    """Malformed request parameters."""
    param: Optional[str]

class ProviderError(QuotaRouterError):
    """Provider-side error."""
    upstream_code: Optional[str]

class ContentFilterError(QuotaRouterError):
    """Content filtered by provider."""
    pass

class ModelNotFoundError(QuotaRouterError):
    """Unknown model identifier."""
    pass

class ContextLengthExceededError(QuotaRouterError):
    """Token limit exceeded."""
    max_tokens: Optional[int]
    received_tokens: Optional[int]

class MissingApiKeyError(QuotaRouterError):
    """No API key provided and none in environment."""
    provider: str
    env_var_name: str  # Which env var to set (matches any-llm)

class UnsupportedProviderError(QuotaRouterError):
    """Provider not supported."""
    provider_key: str  # The provider that was requested
    supported_providers: List[str]  # List of known providers

class UnsupportedParameterError(QuotaRouterError):
    """Parameter not supported by provider."""
    param: str
    provider: str

class InsufficientFundsError(QuotaRouterError):
    """OCTO-W balance insufficient."""
    current_balance: float
    required: float

class UpstreamProviderError(QuotaRouterError):
    """Provider returned an error."""
    status_code: Optional[int]

class GatewayTimeoutError(QuotaRouterError):
    """Provider gateway timeout."""
    pass

class LengthFinishReasonError(QuotaRouterError):
    """Response truncated due to length."""
    finish_reason: str

class ContentFilterFinishReasonError(QuotaRouterError):
    """Response filtered."""
    finish_reason: str

class BatchNotCompleteError(QuotaRouterError):
    """Batch job not yet complete."""
    batch_id: str
    status: str

class AllModelsFailedError(QuotaRouterError):
    """All models failed in batch_completion_models() race."""
    models: List[str]
```

### Python-to-Rust Exception Mapping

**Severity: Important**

Python exceptions defined above map to Rust core `RouterError` variants for fallback logic:

| Python Exception          | Rust `RouterError`      | Trigger                           |
| ------------------------ | ---------------------- | --------------------------------- |
| `RateLimitError`          | `RouterError::RateLimit` | 429 response, "rate_limit" in msg |
| `AuthenticationError`     | `RouterError::AuthError` | 401/403, invalid API key          |
| `ContextLengthExceededError` | `RouterError::ContextWindowExceeded` | Token limit exceeded       |
| `ContentFilterError`      | `RouterError::ContentPolicyViolation` | Content policy violation  |
| `GatewayTimeoutError`     | `RouterError::Timeout`   | 504, timeout                      |
| `ProviderError`           | `RouterError::ProviderUnavailable` | Provider down/unreachable |
| `ModelNotFoundError`      | `RouterError::ProviderUnavailable` | Model not found           |

**Two exception sources:**

1. **Upstream provider errors** (regex mapping, `map_upstream_exception()`):
   - Caught from provider SDK responses
   - Mapped via `UNIFIED_EXCEPTION_PATTERNS` regex rules

2. **Rust core errors** (via PyO3 exception propagation):
   - `RouterError` variants translate to Python equivalents
   - Caught by fallback logic in `Router.completion()` / `Router.acompletion()`

**Implementation:**

```rust
// In PyO3 bindings — translating Rust RouterError to Python exception
match rust_error {
    RouterError::RateLimit => PyErr::new::<RateLimitError, _>(...),
    RouterError::AuthError => PyErr::new::<AuthenticationError, _>(...),
    RouterError::ContextWindowExceeded => PyErr::new::<ContextLengthExceededError, _>(...),
    RouterError::ContentPolicyViolation => PyErr::new::<ContentFilterError, _>(...),
    RouterError::Timeout => PyErr::new::<GatewayTimeoutError, _>(...),
    RouterError::ProviderUnavailable => PyErr::new::<ProviderError, _>(...),
    RouterError::Unknown => PyErr::new::<ProviderError, _>(...),
}
```

Reference: RFC-0902 §Fallback Mechanisms (Rust `RouterError` enum definition).

### Embedded API (LiteLLM-Compatibility Style)

For LiteLLM compatibility, the SDK can be configured with persistent API keys before use:

```python
from quota_router import set_api_key, get_budget_status

# Set API key for a provider (LiteLLM-style persistence)
# Note: any-llm has NO set_api_key() — any-llm passes keys per-call or via constructor.
# set_api_key() is a LiteLLM-compatible feature added by quota-router.
set_api_key("anthropic", "sk-ant-...")
set_api_key("openai", "sk-...")

# Check budget status
budget = get_budget_status()
print(f"OCTO-W Balance: {budget['balance']}")

# Get metrics
metrics = get_metrics()
print(f"Total spend: {metrics['total_spend']}")
```

**any-llm key handling vs quota-router:**

| Approach | any-llm | quota-router |
| -------- | -------- | ------------ |
| Per-call | `completion(model="...", api_key="sk-...")` | `completion(model="...", api_key="sk-...")` ✓ |
| Constructor | `AnyLLM(api_key="sk-...")` | N/A |
| Persistent | **No** `set_api_key()` | `set_api_key()` (LiteLLM compat) |

#### set_api_key() — Storage Clarification

**Severity: Important**

The `set_api_key()` function has **two implementation modes**:

| Mode    | Storage              | Budget Enforcement |
| ------- | -------------------- | ------------------ |
| any-llm | In-memory (HashMap)  | None               |
| full    | `StoolapKeyStorage`  | RFC-0904 enforced  |

**any-llm-mode implementation:**

```rust
// quota-router-pyo3/src/sdk.rs (current)
static API_KEYS: Lazy<Mutex<HashMap<String, String>>>  // In-memory only

pub fn set_api_key(provider: String, api_key: String) -> ... {
    // Format validation, then stores in local HashMap
    // Does NOT persist to StoolapKeyStorage in any-llm-mode
}
```

**full-mode implementation:**

```rust
// When both PyO3 and reqwest are compiled (full build):
// set_api_key() → KeyMiddleware::validate_key() + StoolapKeyStorage
// Keys persist across requests via stoolap WAL
```

**Key insight:** In single-mode `any-llm-mode`, keys are in-memory only (session-scoped). In `full` mode, keys are stored in `StoolapKeyStorage` and budget enforcement (RFC-0904) applies.

**Important — `get_budget_status()` behavior by mode:**

| Mode    | get_budget_status() returns | Notes |
| ------- | -------------------------- | ----- |
| any-llm | Estimated from in-memory tracking | No persistence; estimate only |
| full    | Real Balance from StoolapKeyStorage | Persisted, accurate |

In any-llm-mode, `get_budget_status()` tracks spend in-memory per-session using `Balance` struct (RFC-0904) but does NOT persist across restarts. In `full` mode, `Balance` data persists via stoolap WAL.

#### get_budget_status() — Balance Reference

**Severity: Important**

`get_budget_status()` returns OCTO-W spend data from Rust `Balance` struct:

```rust
// quota-router-core/src/balance.rs
pub struct Balance {
    pub key_id: String,
    pub team_id: String,
    pub current_spend: Decimal,
    pub budget_limit: Option<Decimal>,
    pub last_updated: DateTime<Utc>,
}
```

**Python return type:**

```python
@dataclass
class BudgetStatus:
    balance: float           # Current OCTO-W balance
    total_spend: float      # Cumulative spend
    budget_limit: Optional[float]  # Cap if set
    last_updated: str       # ISO 8601 timestamp
    key_id: Optional[str]   # For which key (if tracked)

def get_budget_status(provider: Optional[str] = None) -> BudgetStatus:
    """
    Returns OCTO-W budget status from Rust Balance + StoolapKeyStorage.

    ⚠️ WARNING: In any-llm-mode, budget tracking is in-memory only and will reset
    on process restart. For production budget enforcement, use 'full' mode with
    stoolap persistence (RFC-0904).

    | Mode    | Behavior | Notes |
    | ------- | -------- | ----- |
    | any-llm | Estimated spend from current session only | No persistence |
    | full    | Persisted, accurate balance from stoolap | durable across restarts |

    Args:
        provider: Optional provider name to get per-provider budget status

    Returns:
        BudgetStatus with balance, total_spend, budget_limit, last_updated
    """
```

**Reference:** RFC-0904 (Real-Time Cost Tracking) for Balance struct definition.

### LiteLLM Router Class

For LiteLLM compatibility, the Router class is included:

```python
from quota_router import Router

router = Router(
    model_list=[
        {"model_name": "gpt-4o", "litellm_params": {"provider": "openai"}},
        {"model_name": "claude-3", "litellm_params": {"provider": "anthropic"}},
    ],
    routing_strategy="least-busy",
    cache=True,
)

# Use as sync or async
response = router.completion(
    model="gpt-4o",
    messages=[{"role": "user", "content": "Hello"}],
)
```

### list_models() Signature

`list_models()` is available as both a **standalone function** and an **instance method on Router** for LiteLLM compatibility:

```python
# Standalone function
from quota_router import list_models
models = list_models(provider="openai")

# Router instance method (LiteLLM style)
from quota_router import Router
router = Router(model_list=[...])
models = router.list_models()  # Returns models for all deployments
models = router.list_models(provider="openai")  # Filter by provider
```

**Specification:**

```python
def list_models(
    provider: Optional[str] = None,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict] = None,
) -> List[Model]:
    """
    List available models for a provider.

    Args:
        provider: Provider name (e.g., "openai", "anthropic"). If None, lists all
                  providers' models.
        api_key: Override API key for this call.
        api_base: Override base URL for this call.
        client_args: Additional provider-specific arguments.

    Returns:
        List of Model objects with fields: id, name, provider, created, description.

    Raises:
        MissingApiKeyError: If no API key available.
        ProviderError: If provider API call fails.
    """

# Router instance method
class Router:
    def list_models(
        self,
        provider: Optional[str] = None,
    ) -> List[Model]:
        """List models from this router's model_list deployments."""
```

### Model Response Type

````python
@dataclass
class Model:
    id: str           # Full model ID (e.g., "gpt-4o")
    name: str         # Display name
    provider: str      # Provider name (e.g., "openai")
    created: Optional[int]  # Unix timestamp
    description: Optional[str]
    supports: Optional[Dict[str, bool]]  # Feature support flags

### CompletionResponse Type

```python
@dataclass
class CompletionResponse:
    id: str                    # Unique response ID
    provider: str              # Provider used (e.g., "openai")
    model: str                 # Model used (e.g., "gpt-4o")
    object: str = "chat.completion"  # OpenAI-compatible object type
    created: int              # Unix timestamp
    choices: List[Choice]     # Response choices
    usage: Optional[Usage]   # Token usage statistics

@dataclass
class Choice:
    index: int
    message: Message         # Response message
    finish_reason: str        # "stop", "length", "content_filter", etc.

@dataclass
class Message:
    role: str                 # "assistant"
    content: str              # Response content

@dataclass
class Usage:
    prompt_tokens: int
    completion_tokens: int
    total_tokens: int
````

This matches OpenAI's chat completion response format for maximum compatibility.

### session_label Handling

`session_label: Optional[str] = None` is used for **metrics grouping and tracing**. It is:

- Passed to the router's metrics system for correlation
- **NOT** passed to provider SDKs (providers don't understand it)
- Useful for grouping requests by user session or feature area

### client_args Schema

`client_args: Optional[Dict] = None` provides **provider-specific overrides**:

```python
client_args: {
    "timeout": 30.0,           # Request timeout in seconds
    "max_retries": 3,         # Retry count
    "connection_pool_size": 10, # Connection pool size
    # Provider-specific options passed through to SDK
}
```

If `client_args` conflicts with `api_key` or `api_base`, `client_args` takes precedence for provider SDK initialization.

### Gap Analysis vs Reference Implementations

This section documents gaps between RFC-0920 and the reference implementations (any-llm, litellm), with **actual specifications** for each gap.

#### Missing Completion Parameters

**Parameters present in litellm but not yet specced in RFC-0920:**

| Parameter                       | Type                            | Description                                | Spec Location           |
| ------------------------------- | ------------------------------- | ------------------------------------------ | ----------------------- |
| `timeout`                       | `float \| str \| httpx.Timeout` | Request timeout (sync completion only); acompletion uses `float \| int` per LiteLLM | §Timeout Parameter      |
| `thinking`                      | `dict`                          | Anthropic extended thinking budget         | §Thinking Parameter     |
| `model_list`                    | `list`                          | Alternative model configuration            | §Model List             |
| `extra_headers`                 | `dict`                          | Additional headers to pass to provider     | §Extra Headers          |
| `base_url`                      | `str`                           | Alias for `api_base`                       | §Base URL Alias         |
| `api_version`                   | `str`                           | API version for Azure-style providers      | §API Version            |
| `web_search_options`            | `dict`                          | Web search for supported providers         | §Web Search Options     |
| `modalities`                    | `list`                          | Audio output modalities                    | §Modalities             |
| `audio`                         | `dict`                          | Audio parameters                           | §Audio Parameters       |
| `prediction`                    | `dict`                          | Prediction content for o1 models           | §Prediction             |
| `shared_session`                | `ClientSession`                 | Shared httpx session                       | §Shared Session         |
| `enable_json_schema_validation` | `bool`                          | Validate response vs schema                | §JSON Schema Validation |

**Parameters present in any-llm but not yet specced in RFC-0920:**

| Parameter                | Type          | Description                        | Spec Location        |
| ------------------------ | ------------- | ---------------------------------- | -------------------- |
| `system`                 | `str \| list` | System message(s) for messages API | §System Parameter    |
| `top_k`                  | `int`         | Top-k sampling for Anthropic       | §Top K               |
| `truncation`             | `str`         | Cohere truncation strategy         | §Truncation          |
| `service_tier`           | `str`         | Azure OpenAI service tier          | §Service Tier        |
| `background`             | `bool`        | Run request in background          | §Background Requests |
| `safety_identifier`      | `str`         | Content safety category            | §Safety Identifier   |
| `prompt_cache_key`       | `str`         | Prompt caching key                 | §Prompt Cache        |
| `prompt_cache_retention` | `str`         | Prompt cache TTL                   | §Prompt Cache        |
| `conversation`           | `str`         | Conversation ID for continuity     | §Conversation        |

#### Sync Streaming — Async Iterator Bridge

**Severity: High**

When `completion(model="...", messages=[...], stream=True)` is called synchronously (not async), and the underlying provider SDK is async-only, the SDK must bridge the async stream to a sync iterator.

This applies to **any-llm-mode** (PyO3 Python SDK calls) where providers may be async-first.

**Specification:**

```python
# quota_router/streaming.py

import asyncio
import queue
import threading
from typing import AsyncIterator, Iterator, TypeVar

T = TypeVar("T")

def async_iter_to_sync_iter(
    async_iter: AsyncIterator[T],
    timeout: float = 60.0,
) -> Iterator[T]:
    """
    Bridge an async iterator to a sync iterator using a background thread.

    Used when sync completion() is called but the underlying provider SDK
    is async-only. The background thread drives the async iterator and
    yields items to a synchronous queue.

    Args:
        async_iter: AsyncIterator to consume
        timeout: Max seconds between items before StopIteration

    Yields:
        Items from the async iterator

    Raises:
        Exception: Any exception raised by the async iterator

    Note:
        The background thread is daemon=True — it will not prevent
        the main process from exiting.
    """
    q: queue.Queue[T | type(StopIteration)] = queue.Queue(maxsize=1)
    exception_store = [None]  # Mutate to share exception

    def consume_async() -> None:
        try:
            loop = asyncio.new_event_loop()
            asyncio.set_event_loop(loop)
            async def run() -> None:
                try:
                    async for item in async_iter:
                        q.put(item, timeout=timeout)
                except StopAsyncIteration:
                    q.put(StopIteration, timeout=timeout)
            loop.run_until_complete(run())
        except Exception as e:  # noqa: BLE001
            exception_store[0] = e
            q.put(StopIteration, timeout=timeout)

    thread = threading.Thread(target=consume_async, daemon=True)
    thread.start()

    while True:
        item = q.get(timeout=timeout * 2)
        if isinstance(item, type(StopIteration)):
            if exception_store[0] is not None:
                raise exception_store[0]
            break
        yield item
```

**Sync streaming return type** (any-llm-mode):

```python
def completion(
    model: str,
    messages: List[Dict],
    *,
    # Provider
    provider: Optional[str] = None,
    # API credentials
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    # Completion params (matching LiteLLM sync completion)
    temperature: Optional[float] = None,
    top_p: Optional[float] = None,
    max_tokens: Optional[int] = None,
    max_completion_tokens: Optional[int] = None,
    n: Optional[int] = None,
    stream: bool = False,
    stream_options: Optional[Dict] = None,
    stop: Optional[Union[str, List[str]]] = None,
    presence_penalty: Optional[float] = None,
    frequency_penalty: Optional[float] = None,
    logit_bias: Optional[Dict[int, float]] = None,
    user: Optional[str] = None,
    seed: Optional[int] = None,
    timeout: Optional[Union[float, str, httpx.Timeout]] = None,  # sync uses str/httpx.Timeout
    # LiteLLM sync-specific params
    reasoning_effort: Optional[str] = None,  # LiteLLM-style: "none", "minimal", "low", "medium", "high", "xhigh", "default", "auto"
    functions: Optional[List] = None,  # Legacy — use tools
    function_call: Optional[str] = None,  # Legacy — use tool_choice
    tools: Optional[List[Dict]] = None,
    tool_choice: Optional[Union[str, Dict]] = None,
    parallel_tool_calls: Optional[bool] = None,
    logprobs: Optional[bool] = None,
    top_logprobs: Optional[int] = None,
    extra_headers: Optional[Dict] = None,
    base_url: Optional[str] = None,  # Alias for api_base
    api_version: Optional[str] = None,
    api_type: Optional[str] = None,  # LiteLLM api_type (e.g., "azure")
    model_list: Optional[list] = None,  # Per-call model configuration (see below)
    """
When provided, the completion call selects a deployment from this list for the
current call only, ignoring any global Router configuration. Each dict follows
the deployment format: {"model_name": "...", "api_base": "...", "api_key": "...", "rpm": N, "tpm": N}.
If the requested model is not in the list, raises ModelNotFoundError.
This parameter does NOT modify the Router's stored deployment list.

Empty list (model_list=[]): Raises ValueError — an empty list explicitly passed
is treated as a validation error, not as "no list provided" (use model_list=None
to fall back to default provider resolution).
"""
    deployment_id: Optional[str] = None,
    safety_identifier: Optional[str] = None,
    service_tier: Optional[str] = None,
    # Response format (structured output)
    response_format: Optional[Union[str, Dict, Type[Any]]] = None,  # Pydantic BaseModel types supported
    # LiteLLM session and validation
    shared_session: Optional[Any] = None,  # ClientSession for session management
    web_search_options: Optional[Dict] = None,  # OpenAI web search options
    enable_json_schema_validation: Optional[bool] = None,  # Per-request JSON schema validation override

    # Note: `thinking` (structured Dict) and `reasoning_effort` (string enum) are separate parameters in LiteLLM, not aliases
    **kwargs,
) -> Union[CompletionResponse, Iterator[ChatCompletionChunk]]:
    """
    When stream=True in any-llm-mode, returns Iterator[ChatCompletionChunk].
    The iterator is created by bridging the async provider stream via
    async_iter_to_sync_iter().
    """
```

#### Real SSE Streaming — Provider-Specific Parsing

**Severity: Important**

The current `quota-router-pyo3/src/streaming.rs` implementation is a **mock** that splits content by whitespace. Real streaming requires SSE parsing and transformation per provider format.

**Streaming execution path (Router → completion → stream):**

```
router.completion(stream=True)
  → _select_deployment(model)          # Select deployment
  → completion(stream=True)            # Module-level completion with stream=True
  → Provider SDK's stream=True call     # Via PyO3 in any-llm, via reqwest in litellm
  → SSE parsing (provider-specific)    # OpenAI pass-through, Anthropic transform, etc.
  → Iterator[ChatCompletionChunk]      # Normalized chunks returned
```

**Note:** When `Router.completion(stream=True)` is called, the Router:
1. Selects deployment via `_select_deployment()`
2. Calls `completion(stream=True)` with selected deployment params
3. The module-level completion handles provider-specific streaming
4. Router does NOT directly call provider SDKs — it goes through completion() as normal

The streaming spec below covers the SSE transformation layer inside `completion()`.

**Streaming behavior by mode:**

| Mode | Provider Call | Stream Return Type | Implementation |
|------|-------------|-------------------|----------------|
| `litellm-mode` | reqwest (Rust sync) | `Iterator[ChatCompletionChunk]` | Rust iterator exposed via PyO3 |
| `any-llm-mode` | Python SDK (async) | `Iterator[ChatCompletionChunk]` | `async_iter_to_sync_iter()` bridge |
| `full` | Based on runtime mode | `Iterator[ChatCompletionChunk]` | Same as above based on `QUOTA_ROUTER_MODE` |

**For `acompletion(stream=True)`:**
- In `litellm-mode`: Rust async stream → Python async iterator via PyO3 async support
- In `any-llm-mode`: Python async SDK stream → `async_iter_to_sync_iter()` bridge → sync iterator
- In `full` mode: Uses whichever mode is active via `QUOTA_ROUTER_MODE`

**Current mock implementation (replace):**

```rust
// quota-router-pyo3/src/streaming.rs (CURRENT — MOCK)
pub fn create_chunk_list(model: String, content: String) -> Vec<ChatCompletionChunk> {
    content.split_whitespace().map(|word| ...).collect()
}
```

**Real streaming implementation:**

```python
# quota_router/streaming.py

import sse
from typing import Iterator, Optional

class SSEParser:
    """
    Provider-specific SSE parsing for streaming responses.

    Each provider has a different SSE format. This class normalizes
    to OpenAI SSE format for compatibility.
    """

    @staticmethod
    def parse_openai_sse(chunk: bytes) -> Optional[ChatCompletionChunk]:
        """OpenAI SSE: pass-through (already normalized)."""
        # data: {"id":"...","choices":[{"delta":{"content":"..."}}]}
        # Parse and yield ChatCompletionChunk
        pass

    @staticmethod
    def parse_anthropic_sse(chunk: bytes) -> Optional[ChatCompletionChunk]:
        """Anthropic event-stream: transform to OpenAI SSE."""
        # event: message_delta
        # data: {"usage":{"output_tokens":123},"delta":{"text":"..."}}
        # Transform to OpenAI format: {"choices":[{"delta":{"content":"..."}}]}
        pass

    @staticmethod
    def parse_mistral_sse(chunk: bytes) -> Optional[ChatCompletionChunk]:
        """Mistral: OpenAI SSE pass-through."""
        pass

    @staticmethod
    def parse_ollama_sse(chunk: bytes) -> Optional[ChatCompletionChunk]:
        """Ollama: SSE with custom format."""
        # data: {"model":"llama3","done":false,"message":{"role":"assistant","content":"..."}}
        # Transform to OpenAI SSE
        pass

async def _stream_provider_response(
    provider: str,
    model: str,
    messages: List[Dict],
    **kwargs,
) -> AsyncIterator[ChatCompletionChunk]:
    """
    Call provider SDK with stream=True, parse SSE, yield normalized chunks.
    """
    if provider == "openai":
        async for chunk in openai_sdk.chat.completions.stream(model=model, messages=messages, **kwargs):
            yield SSEParser.parse_openai_sse(chunk)
    elif provider == "anthropic":
        async for event in anthropic_sdk.messages.stream(model=model, messages=messages, **kwargs):
            yield SSEParser.parse_anthropic_sse(event)
    # ... other providers

async def _stream_sync_bridge(
    provider: str,
    model: str,
    messages: List[Dict],
    **kwargs,
) -> Iterator[ChatCompletionChunk]:
    """
    Bridge async streaming to sync iterator using async_iter_to_sync_iter().
    """
    async_iter = _stream_provider_response(provider, model, messages, **kwargs)
    yield from async_iter_to_sync_iter(async_iter)
```

**SSE transformation table:**

| Provider  | Native SSE Format           | Normalized To     | Transform Function                |
| --------- | --------------------------- | ----------------- | --------------------------------- |
| OpenAI    | OpenAI SSE                  | Pass-through      | `parse_openai_sse()`              |
| Anthropic | `event: message_delta`       | OpenAI SSE        | `parse_anthropic_sse()`            |
| Mistral   | OpenAI SSE                  | Pass-through      | `parse_mistral_sse()`              |
| Ollama    | Custom JSON lines           | OpenAI SSE        | `parse_ollama_sse()`              |
| Gemini    | Provider-specific           | OpenAI SSE        | Provider-specific                 |
| Groq      | OpenAI SSE                  | Pass-through      | `parse_openai_sse()`              |

**Note:** LiteLLM mode (HTTP proxy) uses Rust-side SSE transformation. Any-llm mode uses Python-side SSE transformation per above.

#### acompletion() Streaming — AsyncIterator Return Type

**Severity: Important**

**Specification:**

```python
async def acompletion(
    model: str,
    messages: List[Dict],
    *,
    stream: Optional[bool] = None,
    stream_options: Optional[Dict] = None,
    timeout: Optional[Union[float, int]] = None,  # Common for streaming to avoid hanging
    response_format: Optional[Union[str, Dict, Type[Any]]] = None,  # Structured output
    **kwargs,
) -> Union[CompletionResponse, AsyncIterator[ChatCompletionChunk]]:
    """
    When stream=True, returns an AsyncIterator[ChatCompletionChunk].
    The caller uses `async for chunk in result:` to consume chunks.

    Reference: LiteLLM's CustomStreamWrapper.__aiter__/__anext__
    (litellm/litellm_core_utils/streaming_handler.py lines 2017-2075)

    Example:
        result = await acompletion(model="gpt-4o", messages=[...], stream=True)
        # result is AsyncIterator[ChatCompletionChunk]
        async for chunk in result:
            print(chunk.delta.content, end="")
    """
```

**AsyncIterator implementation pattern (per LiteLLM CustomStreamWrapper):**

```python
class ChatCompletionChunkIterator:
    """Async iterator for streaming chunks — mimics LiteLLM CustomStreamWrapper."""

    def __init__(self, provider: str, model: str, messages: List[Dict], **kwargs):
        self.provider = provider
        self.model = model
        self.messages = messages
        self.kwargs = kwargs
        self._stream = None

    def __aiter__(self) -> "ChatCompletionChunkIterator":
        return self

    async def __anext__(self) -> ChatCompletionChunk:
        """Yield chunks from the provider's async stream."""
        if self._stream is None:
            self._stream = await self._create_stream()
        try:
            async for chunk in self._stream:
                # Transform to normalized ChatCompletionChunk
                yield self._transform_chunk(chunk)
        except StopAsyncIteration:
            raise StopAsyncIteration

    async def _create_stream(self) -> AsyncIterator:
        """Create the async stream from the provider SDK."""
        if self.provider == "openai":
            from openai import AsyncOpenAI
            client = AsyncOpenAI()
            stream = await client.chat.completions.create(
                model=self.model,
                messages=self.messages,
                stream=True,
                **self.kwargs,
            )
            return stream
        elif self.provider == "anthropic":
            from anthropic import AsyncAnthropic
            client = AsyncAnthropic()
            stream = await client.messages.stream(
                model=self.model,
                messages=self.messages,
                **self.kwargs,
            )
            return stream
        # ... other providers

    def _transform_chunk(self, chunk) -> ChatCompletionChunk:
        """Provider-specific chunk normalization."""
        # Provider-specific SSE parsing happens here
        pass
```

**Note on SSE parsing:** Phase 1 uses `async_iter_to_sync_iter()` bridge for **sync** streaming with sync providers. For **async** streaming (acompletion with stream=True), the async iterator is returned directly. SSE parsing (F3: provider-specific SSE transformation) is **NOT** part of Phase 1 — Phase 1 returns raw chunks from provider SDKs. F3 covers implementing proper SSE parsing for non-OpenAI-SSE providers.

**Bridge function clarification:** quota-router uses `async_iter_to_sync_iter()` (takes AsyncIterator, drives via background thread). any-llm uses `async_coro_to_sync_iter()` (takes coroutine, runs it, yields results). These are different functions for different use cases:
- `async_coro_to_sync_iter`: coroutine → sync iterator (any-llm pattern)
- `async_iter_to_sync_iter`: AsyncIterator → sync iterator (RFC pattern)

#### In-Memory Batch Completion

**Severity: High**

`batch_completion()` submits multiple completion requests in parallel threads, returning a list of responses in input order. Distinct from file-based Batch API.

**LiteLLM vs any-llm:** LiteLLM has ThreadPoolExecutor-based `batch_completion()` (in-memory parallel). any-llm has **NO** in-memory batch — only file-based batch via `create_batch()` / `acreate_batch()`. quota-router implements in-memory batch for LiteLLM compatibility; any-llm users must use the file-based batch API.

**Specification:**

```python
# quota_router/batch.py

from concurrent.futures import ThreadPoolExecutor, as_completed
from typing import List, Dict, Optional, Union

def batch_completion(
    model: str,
    messages: List[List[Dict]],
    *,
    # Completion params (subset of acompletion)
    provider: Optional[str] = None,
    temperature: Optional[float] = None,
    top_p: Optional[float] = None,
    max_tokens: Optional[int] = None,
    n: Optional[int] = None,
    timeout: Optional[int] = 600,
    max_workers: int = 100,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    **kwargs,
) -> List[CompletionResponse]:
    """
    Submit multiple completion requests in parallel.

    Args:
        model: Model identifier (e.g., "openai:gpt-4o")
        messages: List of message lists, one per request
        provider: Provider name (optional if model has prefix)
        temperature: Sampling temperature
        top_p: Nucleus sampling
        max_tokens: Max tokens per response
        n: Number of completions per request
        timeout: Request timeout in seconds
        max_workers: Max parallel threads
        api_key: Override API key
        api_base: Override base URL

    Returns:
        List[CompletionResponse] in same order as messages input

    Raises:
        BatchPartialFailureError: If some requests fail (partial results returned)

    Note:
        Uses ThreadPoolExecutor internally. For async batch, use
        abatch_completion() with asyncio.gather().

        **GIL consideration:** In any-llm mode, Python SDK calls hold the GIL
        and ThreadPoolExecutor provides no parallelism. Prefer abatch_completion()
        (asyncio.gather) for any-llm mode. For litellm mode (Rust reqwest),
        threads are fine since Rust releases the GIL during HTTP calls.
    """
    if not messages:
        return []

    results: List[Optional[CompletionResponse]] = [None] * len(messages)
    errors: List[Optional[Exception]] = [None] * len(messages)

    def submit_one(idx: int, msgs: List[Dict]) -> None:
        try:
            # Call completion (sync) for each message set
            result = completion(
                model=model,
                messages=msgs,
                provider=provider,
                temperature=temperature,
                top_p=top_p,
                max_tokens=max_tokens,
                n=n,
                timeout=timeout,
                api_key=api_key,
                api_base=api_base,
                **kwargs,
            )
            results[idx] = result
        except Exception as e:  # noqa: BLE001
            errors[idx] = e

    with ThreadPoolExecutor(max_workers=max_workers) as executor:
        futures = [
            executor.submit(submit_one, i, msgs)
            for i, msgs in enumerate(messages)
        ]
        for future in as_completed(futures):
            pass  # Exceptions stored in errors list

    # Check for any errors
    failed_count = sum(1 for e in errors if e is not None)
    if failed_count > 0:
        # Return partial results + raise on completion
        raise BatchPartialFailureError(
            message=f"{failed_count}/{len(messages)} requests failed",
            successful=[r for r in results if r is not None],
            failed=[(i, e) for i, e in enumerate(errors) if e is not None],
        )

    return results  # type: ignore[return-value]
```

**Async variant:**

```python
async def abatch_completion(
    model: str,
    messages: List[List[Dict]],
    *,
    provider: Optional[str] = None,
    temperature: Optional[float] = None,
    max_tokens: Optional[int] = None,
    n: Optional[int] = None,
    max_workers: int = 100,
    **kwargs,
) -> List[CompletionResponse]:
    """
    Async version: gather responses concurrently using asyncio.
    """
    import asyncio

    async def submit_one(msgs: List[Dict]) -> CompletionResponse:
        return await acompletion(
            model=model,
            messages=msgs,
            provider=provider,
            temperature=temperature,
            max_tokens=max_tokens,
            n=n,
            **kwargs,
        )

    return await asyncio.gather(*[submit_one(msgs) for msgs in messages])
```

#### batch_completion_models() — Race Multiple Models (LiteLLM Compatible)

**Severity: Important**

`batch_completion_models()` sends the **same request** to **multiple models concurrently** and returns the **first response** (race condition). Distinct from `batch_completion()` which sends **many messages** to **one model**.

**Reference:** LiteLLM's `batch_completion_models` (`litellm/batch_completion/main.py` lines 128-211).

**Specification:**

```python
def batch_completion_models(
    *args,
    messages: List[Dict],
    models: Union[str, List[str]],  # One or more models to race
    **kwargs,
) -> CompletionResponse:
    """
    Send a request to multiple language models concurrently and return
    the response from the FIRST model that responds.

    Args:
        *args: Variable-length positional args (passed to completion)
        messages: The message list to send to ALL models
        models: Single model name (str) or list of model names to race
        **kwargs: Passed to completion() for each model

    Returns:
        CompletionResponse from the first model to respond

    Note:
        Uses ThreadPoolExecutor with wait(FIRST_COMPLETED) — returns
        as soon as any model responds. Other requests are cancelled.
        Distinct from batch_completion() which sends many messages to
        one model and returns ALL results.
    """
    if isinstance(models, str):
        models = [models]

    # Remove conflicting kwargs
    kwargs.pop("model", None)
    kwargs.pop("models", None)

    futures = {}
    with ThreadPoolExecutor(max_workers=len(models)) as executor:
        for model in models:
            futures[model] = executor.submit(
                completion, *args, model=model, messages=messages, **kwargs
            )

        # Wait for first completion (FIRST_COMPLETED)
        done, _ = wait(futures.values(), return_when=FIRST_COMPLETED)
        for future in done:
            try:
                return future.result()
            except Exception:
                # First model failed — continue waiting for others
                continue

    raise AllModelsFailedError(
        f"All {len(models)} models failed: {[m for m in models]}"
    )
```

**Also available:** `batch_completion_models_all_responses()` — returns ALL responses (not just first), as a list.

**Async variant:**

```python
async def abatch_completion_models(
    *args,
    messages: List[Dict],
    models: Union[str, List[str]],
    **kwargs,
) -> CompletionResponse:
    """
    Async version of batch_completion_models().
    Sends same request to multiple models concurrently, returns FIRST response.
    """
    if isinstance(models, str):
        models = [models]

    kwargs.pop("model", None)
    kwargs.pop("models", None)

    async def submit_one(model_name: str) -> CompletionResponse:
        return await acompletion(model=model_name, messages=messages, **kwargs)

    results = await asyncio.gather(*[submit_one(m) for m in models], return_exceptions=True)
    for result in results:
        if not isinstance(result, Exception):
            return result

    raise AllModelsFailedError(
        f"All {len(models)} models failed: {[m for m in models]}"
    )
```

**Key difference from `batch_completion`:**

| Function | Messages | Models | Returns |
| -------- | -------- | ------ | ------- |
| `batch_completion()` | Many message sets | One model | ALL results (List[CompletionResponse]) |
| `batch_completion_models()` | One message set | Many models | FIRST response (CompletionResponse) |
| `batch_completion_models_all_responses()` | One message set | Many models | ALL responses (List[CompletionResponse]) |

#### Router — Load Balancing Strategies

**Severity: High**

Router dispatches to multiple model deployments using configurable strategies.

**Important:** The Python Router is a **Python-level class** that calls the Python `completion()` function. It does **NOT** wrap the Rust core `Router`. The Rust `Router` (`quota-router-core/src/router.rs`) is for the proxy server's multi-deployment index selection — it is separate from the Python SDK's routing layer.

**Architecture:**

```
Python Router (this spec)
  └── Calls Python completion() function
        └── PyO3 → Rust core (KeyMiddleware, Balance, Storage)

Rust Router (quota-router-core/src/router.rs)
  └── Used by ProxyServer for index-based deployment selection
  └── NOT used by Python SDK
```

The Python Router's `model_list` contains LiteLLM-style deployment configs (`{"model_name": "gpt-4o", "litellm_params": {"provider": "openai", "api_key": "...", "api_base": "..."}}`). The Router selects a deployment, then calls `completion(provider=..., model=...)` with that deployment's params.

**Specification:**

```python
# quota_router/routing.py
# Python-level router that composes completion()

from typing import List, Dict, Optional
import random
import time

class Router:
    """
    Python-level router for multi-deployment load balancing.

    Selects a deployment from model_list using a routing strategy,
    then calls the Python completion() function with that deployment's params.

    Routing strategies (from RFC-0902):
        "simple-shuffle"     — Weighted random (rpm/tpm/weight) — recommended for production
        "round-robin"        — Sequential rotation
        "least-busy"          — Fewest active requests
        "latency-based-routing" — Lowest rolling average latency
        "cost-based-routing"  — Lowest cost per token (requires RFC-0904)
        "usage-based-routing" — Lowest cumulative usage
        "usage-based-routing-v2" — Usage weighted by recency
        "weighted"            — Explicit per-provider weights (distinct from simple-shuffle)

    Reference: RFC-0902 §Routing Strategies
    """

    def __init__(
        self,
        model_list: List[Dict],
        routing_strategy: str = "simple-shuffle",
        cache_responses: bool = False,  # stoolap semantic cache (RFC-0913)
        fallbacks: Optional[List[Dict]] = None,  # model -> [fallback_models]
        content_policy_fallbacks: Optional[Dict[str, str]] = None,  # model -> fallback_model
        context_window_fallbacks: Optional[Dict[str, str]] = None,  # model -> larger_context_model
        num_retries: Optional[int] = 3,
        timeout: Optional[float] = None,
        logger_fn: Optional[callable] = None,  # RFC-0905 logger
        **kwargs,
    ):
        """
        Initialize Router with model deployments.

        Args:
            model_list: List of {"model_name": "...", "litellm_params": {...}}
                Example: {"model_name": "gpt-4o", "litellm_params": {"provider": "openai", "api_key": "...", "rpm_limit": 1000}}
            routing_strategy: RFC-0902 routing strategy (string)
            cache_responses: Enable stoolap semantic cache (RFC-0913)
            fallbacks: List of {"model": "gpt-4o", "fallback_models": ["gpt-3.5-turbo", "claude-3"]}
            content_policy_fallbacks: Content policy error mapping
            context_window_fallbacks: Context window error mapping
            num_retries: Number of retries on failure (default 3)
            timeout: Default request timeout
            logger_fn: Optional callback for observability (RFC-0905)

        Note:
            This is a Python-level router. It does NOT wrap the Rust core Router.
            The Rust Router (quota-router-core/src/router.rs) is for the proxy server.
        """
        self.model_list = model_list
        self.routing_strategy = routing_strategy
        self.cache_responses = cache_responses
        self.fallbacks = fallbacks or []
        self.content_policy_fallbacks = content_policy_fallbacks or {}
        self.context_window_fallbacks = context_window_fallbacks or {}
        self.num_retries = num_retries
        self.timeout = timeout
        self.logger_fn = logger_fn

        # Runtime state per deployment
        self._deployments = []  # Flat list of (model_name, litellm_params)
        self._round_robin_index = 0
        self._active_requests = {}  # deployment_idx -> count
        self._latencies = {}  # deployment_idx -> list of latencies_us
        self._total_spend = {}  # deployment_idx -> float

        # Group by model_name
        self._by_model: Dict[str, List[int]] = {}  # model_name -> [deployment_idx]
        for i, item in enumerate(model_list):
            model_name = item["model_name"]
            self._deployments.append((model_name, item.get("litellm_params", {})))
            self._by_model.setdefault(model_name, []).append(i)
            self._active_requests[i] = 0
            self._latencies[i] = []
            self._total_spend[i] = 0.0

    def _select_deployment(self, model: str) -> int:
        """Select deployment index using routing strategy.

        Args:
            model: The **model_name** (not model_group) — must match the key in self._by_model.
                   This is the value from model_list[].model_name (e.g., "gpt-4o", "claude-3-opus").
                   Not a model group — model groups are not used at this layer.
        """
        indices = self._by_model.get(model, [])
        if not indices:
            raise ModelNotFoundError(f"No deployments found for model: {model}")

        strategy = self.routing_strategy
        if strategy == "round-robin":
            idx = self._round_robin_index % len(indices)
            self._round_robin_index += 1
            return indices[idx]
        elif strategy == "least-busy":
            return min(indices, key=lambda i: self._active_requests[i])
        elif strategy == "latency-based-routing":
            return min(indices, key=lambda i: self._avg_latency(i))
        elif strategy == "cost-based-routing":
            # Requires RFC-0904 pricing — fallback to shuffle
            return random.choice(indices)
        elif strategy == "usage-based-routing":
            return min(indices, key=lambda i: self._total_spend[i])
        elif strategy == "weighted":
            # Weighted by explicit weights in litellm_params
            weights = [(self._deployments[i][1].get("weight", 1)) for i in indices]
            total = sum(weights)
            r = random.uniform(0, total)
            cumsum = 0
            for idx, w in zip(indices, weights):
                cumsum += w
                if r <= cumsum:
                    return idx
            return indices[-1]
        else:  # simple-shuffle or default
            return random.choice(indices)

    def _avg_latency(self, idx: int) -> float:
        lats = self._latencies[idx]
        if not lats:
            return float("inf")
        return sum(lats) / len(lats)

    def _record_request_start(self, idx: int):
        self._active_requests[idx] = self._active_requests.get(idx, 0) + 1

    def _record_request_end(self, idx: int, latency_ms: float, tokens: int):
        self._active_requests[idx] = max(0, self._active_requests.get(idx, 1) - 1)
        self._latencies[idx].append(int(latency_ms * 1000))  # Store as microseconds
        if len(self._latencies[idx]) > 100:
            self._latencies[idx] = self._latencies[idx][-100:]

    def completion(
        self,
        model: str,
        messages: List[Dict],
        **kwargs,
    ) -> CompletionResponse:
        """
        Route to a deployment and call the module-level completion() function.

        Note: This calls `from quota_router import completion` (module-level),
        NOT self.completion() (recursive loop would occur).
        """
        from quota_router import completion as _module_completion

        deployment_idx = self._select_deployment(model)
        model_name, params = self._deployments[deployment_idx]

        # Merge deployment params with call kwargs (call kwargs take precedence)
        call_kwargs = {**params, **kwargs}
        if self.timeout:
            call_kwargs.setdefault("timeout", self.timeout)

        last_error = None
        for attempt in range(self.num_retries + 1):
            try:
                self._record_request_start(deployment_idx)
                start = time.time()
                result = _module_completion(model=model_name, messages=messages, **call_kwargs)
                latency_ms = (time.time() - start) * 1000
                self._record_request_end(deployment_idx, latency_ms, result.get("usage", {}).get("total_tokens", 0))
                if self.logger_fn:
                    self.logger_fn({"model": model, "deployment": model_name, "latency_ms": latency_ms})
                return result
            except ContextLengthExceededError as e:
                # Try context_window fallback
                # `model` = original input (e.g., "gpt-4o"), `model_name` = current model being attempted
                fallback = self.context_window_fallbacks.get(model)
                if fallback:
                    model_name = fallback  # Overwrite current model attempt with fallback
                    continue
                raise
            except ContentFilterError as e:
                # Try content_policy fallback
                fallback = self.content_policy_fallbacks.get(model)
                if fallback:
                    model_name = fallback
                    continue
                raise
            except (RateLimitError, GatewayTimeoutError, UpstreamProviderError) as e:
                # DO NOT retry here — Rust core (FallbackExecutor) handles HTTP-level retry
                # The Router only handles deployment-level fallback (switching to different model)
                # Routing to a different deployment on error is handled via fallback lists below
                # Check generic fallbacks list for this model
                if self.fallbacks:
                    fallback_list = self.fallbacks.get(model, [])
                    if fallback_list:
                        # Pick first fallback from list
                        model_name = fallback_list[0]
                        continue
                raise
            except Exception as e:
                last_error = e
                raise

        raise last_error

    async def acompletion(
        self,
        model: str,
        messages: List[Dict],
        **kwargs,
    ) -> CompletionResponse:
        """Async route and call the module-level acompletion() function.

        Note: This calls `from quota_router import acompletion` (module-level),
        NOT self.acompletion() (recursive loop would occur).
        """
        import asyncio
        from quota_router import acompletion as _module_acompletion

        deployment_idx = self._select_deployment(model)
        model_name, params = self._deployments[deployment_idx]
        call_kwargs = {**params, **kwargs}
        if self.timeout:
            call_kwargs.setdefault("timeout", self.timeout)

        last_error = None
        for attempt in range(self.num_retries + 1):
            try:
                self._record_request_start(deployment_idx)
                start = time.time()
                result = await _module_acompletion(model=model_name, messages=messages, **call_kwargs)
                latency_ms = (time.time() - start) * 1000
                self._record_request_end(deployment_idx, latency_ms, result.get("usage", {}).get("total_tokens", 0))
                if self.logger_fn:
                    self.logger_fn({"model": model, "deployment": model_name, "latency_ms": latency_ms})
                return result
            except ContextLengthExceededError as e:
                # Try context_window fallback
                # `model` = original input (e.g., "gpt-4o"), `model_name` = current model being attempted
                fallback = self.context_window_fallbacks.get(model)
                if fallback:
                    model_name = fallback  # Overwrite current model attempt with fallback
                    continue
                raise
            except ContentFilterError as e:
                # Try content_policy fallback
                fallback = self.content_policy_fallbacks.get(model)
                if fallback:
                    model_name = fallback
                    continue
                raise
            except (RateLimitError, GatewayTimeoutError, UpstreamProviderError) as e:
                # DO NOT retry here — Rust core (FallbackExecutor) handles HTTP-level retry
                # The Router only handles deployment-level fallback (switching to different model)
                # Check generic fallbacks list for this model
                if self.fallbacks:
                    fallback_list = self.fallbacks.get(model, [])
                    if fallback_list:
                        model_name = fallback_list[0]
                        continue
                raise
            except Exception as e:
                last_error = e
                raise

        raise last_error
```

**Note on `cache_responses`:** Uses **stoolap** (RFC-0913) semantic cache — NOT Redis. Stoolap is the sole persistence layer per RFC-0914. No `redis_url` parameter.

**Note on relationship to Rust Router:** The Rust core `Router` (`quota-router-core/src/router.rs`) is used by the proxy server (`ProxyServer` in `proxy.rs`) for index-based multi-deployment selection. The Python `Router` is a separate, Python-level implementation that achieves similar goals at the Python API layer. They are **not** the same class.

#### Retry Logic

**Severity: Medium**

Per-call retry on transient failure. The retry algorithm (exponential backoff, jitter, retry conditions) is implemented in the **Rust core** per RFC-0902 §Fallback Mechanisms. The Python layer only defines the `num_retries` parameter interface.

**Specification:**

```python
# Part of acompletion / completion / Router signature
num_retries: Optional[int] = None,  # Override HTTP-level retry count in Rust FallbackExecutor (default 3)

# Python layer passes num_retries to Rust core which handles:
# - Exponential backoff (backoff_multiplier)
# - Retry delay (retry_delay_ms)
# - Max backoff (max_backoff_ms)
# - Retry on: RateLimitError, GatewayTimeoutError, UpstreamProviderError
# Reference: RFC-0902 §Fallback Mechanisms, quota-router-core FallbackExecutor

# If None: uses FallbackExecutor default (max_retries: 3)
# If set: overrides max_retries in Rust core's fallback logic for this call
```

**Fallback types (from RFC-0902):**

| Type                       | Trigger                     | Description                                   |
| -------------------------- | --------------------------- | --------------------------------------------- |
| `fallbacks`                | All errors                  | Route to next model on failure                |
| `content_policy_fallbacks` | ContentPolicyViolationError | Map to provider with different content policy |
| `context_window_fallbacks` | ContextWindowExceededError  | Map to model with larger context              |

Reference: RFC-0902 §Fallback Mechanisms

#### Logger Function

**Severity: Low**

Custom logger callback for observability.

**Specification:**

```python
# Part of completion / acompletion / Router signature
logger_fn: Optional[Callable[[Dict], None]] = None

# Called on each request:
def log_request(request: Dict) -> None:
    """Logs request details. Does NOT block the request."""
    if logger_fn:
        try:
            logger_fn({
                "model": request["model"],
                "provider": request["provider"],
                "tokens_used": request.get("usage", {}),
                "latency_ms": request["latency_ms"],
                "status": request.get("status"),
                "error": request.get("error"),
            })
        except Exception:
            pass  # Never block on logger errors
```

#### Exception Regex Mapping

**Severity: Medium**

any-llm provides unified exception mapping via regex patterns on upstream error messages. quota-router supports this via `QUOTA_ROUTER_UNIFIED_EXCEPTIONS=1`.

**Specification:**

```python
# quota_router/exceptions.py

import os
import re

UNIFIED_EXCEPTION_PATTERNS: list[tuple[str, str, type[QuotaRouterError]]] = [
    # (regex, code, exception_type)
    (r"invalid_api_key", "AUTH_ERROR", AuthenticationError),
    (r"incorrect_api_key", "AUTH_ERROR", AuthenticationError),
    (r"api_key not found", "MISSING_API_KEY", MissingApiKeyError),
    (r"rate_limit", "RATE_LIMIT", RateLimitError),
    (r"429", "RATE_LIMIT", RateLimitError),
    (r"context_length", "CONTEXT_LENGTH", ContextLengthExceededError),
    (r"maximum context length", "CONTEXT_LENGTH", ContextLengthExceededError),
    (r"model_not_found", "MODEL_NOT_FOUND", ModelNotFoundError),
    (r"model .* not found", "MODEL_NOT_FOUND", ModelNotFoundError),
    (r"content_filter", "CONTENT_FILTER", ContentFilterError),
    (r"content filtered", "CONTENT_FILTER", ContentFilterError),
    (r"insufficient funds", "INSUFFICIENT_FUNDS", InsufficientFundsError),
    (r"budget exceeded", "INSUFFICIENT_FUNDS", InsufficientFundsError),
    (r"timeout", "GATEWAY_TIMEOUT", GatewayTimeoutError),
    (r"502", "UPSTREAM_ERROR", UpstreamProviderError),
    (r"503", "UPSTREAM_ERROR", UpstreamProviderError),
    (r"504", "GATEWAY_TIMEOUT", GatewayTimeoutError),
    (r"lengthfinishreason", "LENGTH_FINISH", LengthFinishReasonError),
    (r"contentfilterfinishreason", "CONTENT_FILTER_FINISH", ContentFilterFinishReasonError),
]

def map_upstream_exception(message: str, status_code: Optional[int] = None) -> QuotaRouterError:
    """
    Map an upstream provider exception to a quota-router exception.

    Enabled when QUOTA_ROUTER_UNIFIED_EXCEPTIONS=1 (default: off for liteLLM mode,
    on for any-llm mode).
    """
    for pattern, code, exc_type in UNIFIED_EXCEPTION_PATTERNS:
        if re.search(pattern, message, re.IGNORECASE):
            return exc_type(message, code)
    # Default: wrap as ProviderError
    return ProviderError(message, "UPSTREAM_ERROR", None)
```

#### Platform Provider (any-api Key Format)

**Severity: Medium**

any-llm supports `any-...` API keys that encode the provider internally. quota-router supports this via the `platform` pseudo-provider (listed in RFC-0917 Phase 3's 41 providers as `"platform"`).

**Verified consistency with RFC-0917 Phase 3:** The `platform` pseudo-provider matches RFC-0917 Phase 3's provider list (line 1008: `platform` among 41 providers). It is NOT a different platform integration — it is the same `any-...` key format mechanism.

**Specification:**

```python
# When set_api_key("platform", "any-ant-...") or api_key="any-ant-...":
# Parse the any-... key to extract the actual provider and key

ANY_KEY_PREFIX_RE = re.compile(r"^any-([a-z]+)-(.+)$")

def parse_platform_key(api_key: str) -> tuple[str, str]:
    """
    Parse any-api format key.

    Examples:
        "any-ant-sk-..." -> ("anthropic", "sk-...")
        "any-openai-sk-..." -> ("openai", "sk-...")
        "any-mistral-..." -> ("mistral", "...")

    Returns:
        (provider_name, underlying_api_key)

    Raises:
        ValueError: If not a valid any-... key
    """
    m = ANY_KEY_PREFIX_RE.match(api_key)
    if not m:
        raise ValueError(f"Invalid any-api format: {api_key}")
    return m.group(1), m.group(2)

# In set_api_key() or per-call api_key resolution:
if api_key.startswith("any-"):
    actual_provider, actual_key = parse_platform_key(api_key)
    _set_key_for_provider(actual_provider, actual_key)
    _platform_key_map[actual_provider] = "platform"  # Tag for metrics

# Note: any- key parsing works for BOTH set_api_key() AND per-call api_key= parameter.
# Before passing to provider SDK, any- prefixed keys are parsed to extract the actual provider.
```

**For per-call usage:**
```python
# Using any- key per-call (any-llm pattern):
completion(
    model="gpt-4o",
    messages=[...],
    api_key="any-openai-sk-..."  # Parsed to extract "openai" and "sk-..."
)
```

This ensures `any-` keys work regardless of how they're passed — via `set_api_key()` or per-call.

#### Timeout Parameter

**Severity: Medium**

**Specification:**

```python
# Part of completion / acompletion signature
timeout: Optional[Union[float, str, httpx.Timeout]] = None

# httpx.Timeout support:
# - float: total timeout in seconds
# - str: "30s", "1m", etc.
# - httpx.Timeout: explicit connect/read/write/timeouts

from httpx import Timeout

# Examples:
completion(model="gpt-4o", messages=[...], timeout=30.0)
completion(model="gpt-4o", messages=[...], timeout="60s")
completion(model="gpt-4o", messages=[...], timeout=Timeout(10.0, connect=5.0))

# Default: provider-specific, typically 60s
```

#### Thinking Parameter (Anthropic Extended Thinking)

**Severity: Medium**

**Specification:**

```python
# Part of completion / acompletion signature
thinking: Optional[Dict] = None

# Schema:
# {
#     "type": "enabled" | "auto",
#     "budget_tokens": int  # 1000-20000 for Claude 3.7
# }

# Example:
acompletion(
    model="anthropic:claude-3-7-sonnet-20250620",
    messages=[...],
    thinking={
        "type": "enabled",
        "budget_tokens": 10000,
    },
)
# Maps to Anthropic API's thinking parameter
```

#### System Parameter (Anthropic Messages API)

**Severity: Medium**

**Specification:**

```python
# Part of messages() / amessages() signature
system: Optional[Union[str, List[Dict]]] = None

# Can be a string or list of content blocks:
# string: "You are a helpful assistant."
# list: [{"type": "text", "text": "..."}, {"type": "tool_use", ...}]

# Maps to Anthropic messages API system parameter
```

#### Additional Parameters (Low Priority — Phase 4)

These are documented here for completeness but specced in Phase 4:

| Parameter                       | Source  | Spec Location |
| ------------------------------- | ------- | ------------- |
| `top_k`                         | any-llm | Phase 4       |
| `truncation`                    | any-llm | Phase 4       |
| `service_tier`                  | any-llm | Phase 4       |
| `background`                    | any-llm | Phase 4       |
| `safety_identifier`             | litellm | Phase 3 (LiteLLM sig) |
| `prompt_cache_key`              | any-llm | Phase 4       |
| `prompt_cache_retention`        | any-llm | Phase 4       |
| `conversation`                  | any-llm | Phase 4       |
| `extra_headers`                 | litellm | Phase 3 (LiteLLM sync sig) |
| `base_url`                      | litellm | Phase 3 (LiteLLM sig)      |
| `api_version`                   | litellm | Phase 3 (LiteLLM sync sig) |
| `model_list`                    | litellm | Phase 3 (LiteLLM sync sig) |
| `web_search_options`            | litellm | Phase 4       |
| `modalities`                    | litellm | Phase 3 (LiteLLM sig)      |
| `audio`                         | litellm | Phase 3 (LiteLLM sig)      |
| `prediction`                    | litellm | Phase 3 (LiteLLM sig)      |
| `shared_session`                | litellm | Phase 4       |
| `enable_json_schema_validation` | litellm | Phase 4       |

### Batch API Signature

The batch API supports **both** LiteLLM style (`input_file_path`) and any-llm style (`input_file_id`):

```python
def batch_create(
    provider: str,
    input_file: Union[str, Path],     # Local file path (LiteLLM style)
    model: str,
    endpoint: str = "/v1/chat/completions",
    completion_window: str = "24h",
    metadata: Optional[Dict] = None,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
) -> BatchCreateResponse:
    """
    Create a batch job.

    Args:
        provider: Provider name (e.g., "openai")
        input_file: Path to JSONL file with requests, OR pre-existing file ID
                   (if string starts with "file-", treated as file_id; otherwise as path)
        model: Model to use
        endpoint: API endpoint (default: /v1/chat/completions)
        completion_window: Time window (default: "24h")
        metadata: Optional metadata dict
        api_key: Override API key
        api_base: Override base URL

    Returns:
        BatchCreateResponse with batch_id, status, etc.
    """

@dataclass
class BatchCreateResponse:
    batch_id: str           # e.g., "batch_abc123"
    status: str           # "validating", "in_progress", "completed", "failed"
    endpoint: str
    completion_window: str
    created_at: int
    expires_at: int
    metadata: Optional[Dict]

def batch_retrieve(
    batch_id: str,
    provider: str,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
) -> BatchRetrieveResponse:
    """Get batch job status and results."""

def batch_list(
    provider: str,
    after: Optional[str] = None,
    limit: int = 20,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
) -> List[BatchCreateResponse]:
    """List batch jobs for a provider."""

def batch_cancel(
    batch_id: str,
    provider: str,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
) -> BatchCreateResponse:
    """Cancel a batch job."""

def batch_results(
    batch_id: str,
    provider: str,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
) -> List[BatchResultItem]:
    """Retrieve batch results (after completion)."""
```

## Feature Gate Architecture

Per RFC-0917 §Rust Feature Gates, **the mode gate selects the provider integration strategy, NOT the interface. Both HTTP proxy and Python SDK are available in ALL modes:**

```toml
# Cargo.toml for quota-router-pyo3
[features]
litellm-mode = ["pyo3/extension-module"]   # Provider strategy: reqwest (native Rust HTTP)
any-llm-mode = ["pyo3/extension-module"]   # Provider strategy: PyO3 (official Python SDKs)
full = ["pyo3/extension-module"]            # Both provider strategies simultaneously

# Per RFC-0917 §Rust Feature Gates:
# The mode gate selects HOW providers are called (reqwest vs PyO3), NOT which interfaces exist.
# Both HTTP proxy AND Python SDK are ALWAYS available in all modes.
#
# | Interface       | litellm-mode | any-llm-mode | full |
# |-----------------|:------------:|:------------:|:----:|
# | HTTP proxy      |      ✅      |      ✅      |  ✅  |
# | Python SDK      |      ✅      |      ✅      |  ✅  |
#
# Mode controls only: what library is used to call providers
# - litellm-mode:  reqwest (native Rust HTTP) → direct to provider REST APIs
# - any-llm-mode:  PyO3 → official Python SDKs (Anthropic, OpenAI, Mistral, etc.)
```

**Example deployment scenarios:**
- `litellm-mode` build: Run as HTTP proxy (`ProxyServer` on port 8080) AND use Python SDK via `import quota_router`
- `any-llm-mode` build: Run as HTTP proxy AND use Python SDK
- `full` build: Both simultaneously

**⚠️ CRITICAL: Both interfaces exist in ALL modes.** The table below shows provider strategy per mode, NOT which interfaces exist.

### Deployment Mode Selection

Mode is selected at **build time** via Cargo feature flags. **Mode selects provider strategy (reqwest vs PyO3), NOT interface availability:**

| Installation                                   | Mode           | Provider Strategy | HTTP Proxy? | Python SDK? |
| ---------------------------------------------- | -------------- | ----------------- |:-----------:|:-----------:|
| `pip install quota-router` (from PyPI, wheels) | `full`         | Both (reqwest + PyO3) | ✅ | ✅ |
| `cargo build --features litellm-mode`          | `litellm-mode` | reqwest only      | ✅ | ✅ |
| `cargo build --features any-llm-mode`          | `any-llm-mode` | PyO3 only         | ✅ | ✅ |
| `cargo build --features full` (default)        | `full`         | Both              | ✅ | ✅ |

**Key insight:** Even `litellm-mode` builds have Python SDK available. Even `any-llm-mode` builds have HTTP proxy available. Mode controls HOW providers are called, not WHETHER an interface exists.

**Build-time mode selection:**
- Mode is selected at **compile time** via Cargo feature flags on `quota-router-core`
- `quota-router-pyo3` (Python SDK) does NOT have per-mode feature flags — it always wraps `quota-router-core`
- The mode affects which HTTP client is compiled into `quota-router-core`:
  - `litellm-mode` (default): compiles reqwest (Rust HTTP client) into core
  - `any-llm-mode`: compiles minimal core (no reqwest) — Python SDK calls providers via PyO3
  - `full`: compiles both reqwest and PyO3
- When building the Python SDK wheel, the build system selects the mode:
  ```bash
  # Build any-llm-mode Python SDK
  cargo build --package quota-router-pyo3 --features any-llm-mode --no-default-features

  # Build litellm-mode Python SDK
  cargo build --package quota-router-pyo3 --features litellm-mode
  ```
- The resulting `.so`/`.pyd` binary embeds the mode, readable via `get_deployment_mode()`

**Runtime detection:** The SDK exposes `quota_router.get_deployment_mode()`:

```python
import quota_router
mode = quota_router.get_deployment_mode()
# Returns: "litellm-mode" | "any-llm-mode" | "full"
```

**API style is independent of mode:** Both `provider=...` and `provider:model` calling conventions work in all modes.

**`get_deployment_mode()` implementation:** The mode is a compile-time constant baked into the PyO3 binary via Rust build metadata. At Python import time, the mode is read from an embedded constant (not runtime detection):

```rust
// In PyO3 module init
#[pymodule]
fn _quota_router(m: &Bound<PyModule>) -> PyResult<()> {
    #[cfg(feature = "litellm-mode")]
    m.add("__deployment_mode__", "litellm-mode")?;
    #[cfg(feature = "any-llm-mode")]
    m.add("__deployment_mode__", "any-llm-mode")?;
    #[cfg(feature = "full")]
    m.add("__deployment_mode__", "full")?;
    Ok(())
}

def get_deployment_mode() -> str:
    import quota_router
    return quota_router.__deployment_mode__
```

**Implementation note:** The mode string is embedded via `concat!(env!("CARGO_PKG_NAME"), "-", env!("PROFILE"))` or similar compile-time injection. Build scripts generate the constant at `cargo build` time.

## Package Structure

```
quota_router/
├── __init__.py           # Main exports, version
├── completion.py          # completion(), acompletion()
├── embedding.py           # embedding(), aembedding()
├── responses.py           # responses(), aresponses() — OpenAI Responses API
├── messages.py           # messages(), amessages() — Anthropic Messages API
├── batch.py              # batch_create(), batch_retrieve()
├── routing.py             # Router class
├── config.py             # Config handling
├── exceptions.py          # Exception hierarchy
├── models.py             # Model parsing utilities
├── streaming.py           # Streaming utilities
├── budget.py             # Budget management (set_api_key, get_budget_status)
└── metrics.py            # Metrics (get_metrics)

# LiteLLM compatibility — use explicit import alias at call site:
#   from quota_router import completion as litellm_completion
#   OR: import quota_router; litellm = quota_router  # simple alias, no sys.modules mutation
```

## Performance Targets

| Metric                 | Target | Notes                                          |
| ---------------------- | ------ | ---------------------------------------------- |
| Function call overhead | <10ms  | PyO3 → Rust call latency (matches RFC-0908 G1) |
| SDK import time        | <100ms | Cold import                                    |
| Memory per provider    | <10MB  | Cached Python client                           |

## Test Compatibility

### LiteLLM Test Compatibility

The SDK must pass LiteLLM's test suite for completion, embedding, and model listing.

```bash
# Run LiteLLM compatibility tests
pytest tests/test_litellm_compat.py -v

# Coverage target: 100% of LiteLLM API surface
```

### any-llm Test Compatibility

```bash
# Run any-llm compatibility tests
pytest tests/test_anyllm_compat.py -v
```

## Security Considerations

### API Key Trust Boundary

The SDK has **two incompatible API key handling modes** with different security properties:

| Aspect                 | `set_api_key()` (recommended)  | `api_key=...` per-call                  |
| ---------------------- | ------------------------------ | --------------------------------------- |
| Key storage            | Rust memory (enforceable)      | Goes directly to provider SDK           |
| Budget enforcement     | Enforceable (Rust holds key)   | **NOT enforceable** (SDK bypasses Rust) |
| Virtual key (RFC-0903) | Enforceable                    | **NOT enforceable**                     |
| Traceability           | Key identity → Rust → Provider | Key identity → Provider directly        |

**Warning:** When using `api_key="sk-..."` per-call parameter, the key goes directly to the provider SDK. The Rust core never sees it. This means:

- Budget enforcement (RFC-0904) is **bypassed**
- Virtual key validation (RFC-0903) is **bypassed**
- Spend recording uses the **default key**, not the per-call key

**Recommendation:** Use `set_api_key()` for budget-aware deployments. Use `api_key=...` only for one-off calls where budget tracking is not needed.

### General Security

1. **API key handling**: Keys stored in memory only, never persisted
2. **Environment variable fallback**: Standard env var pattern (`OPENAI_API_KEY`, etc.)
3. **Provider isolation**: Provider SDK calls are serialized through PyO3's GIL management (not parallel isolation)
4. **Input validation**: All parameters validated before passing to provider SDK

## Comparison with RFC-0908

| Aspect                  | RFC-0908    | RFC-0920       |
| ----------------------- | ----------- | -------------- |
| LiteLLM compatibility   | ✅ Yes      | ✅ Yes         |
| any-llm compatibility   | ❌ No       | ✅ Yes         |
| `provider` param        | ✅ Separate | ✅ Both styles |
| `provider:model` format | ❌ No       | ✅ Yes         |
| `set_api_key()` style   | ❌ No       | ✅ Yes         |
| Router class            | ✅ Yes      | ✅ Yes         |
| 41 providers            | Partial     | ✅ All 41      |

## Implementation Phases

### Phase 1: Core SDK (Foundation)

- [ ] PyO3 Rust core with unified `acompletion()` signature
- [ ] Provider resolution algorithm (both styles)
- [ ] Exception hierarchy with error codes
- [ ] **Replace mock with real PyO3 SDK calls (OpenAI + Anthropic)** — current `quota-router-pyo3` completion functions are mock stubs that echo messages. Phase 1 replaces these with real provider SDK calls.
- [ ] Basic test suite (OpenAI + Anthropic — these cover the two main patterns: provider= and provider:model)
  - **Note:** Phase 1 tests `completion()` / `acompletion()` directly, NOT through Router. Router is Phase 3.
- [ ] Async iterator bridge for sync streaming (`async_iter_to_sync_iter()`)

**Note:** Phase 1 MUST replace the current mock implementations with real provider SDK calls via PyO3. OpenAI and Anthropic are the priority providers because:
1. OpenAI covers the `provider=model` style (LiteLLM compatibility)
2. Anthropic covers the `provider:model` style (any-llm compatibility)

**Why Phase 1 if both interfaces exist in all modes?** The Python SDK interface **exists** in all modes (per RFC-0917 invariant), but the **implementation** is currently mock. Phase 1 replaces the mock with real SDK calls. This is implementation work, not interface work.

### Phase 2: Full Provider Coverage

- [ ] Anthropic provider integration (with `thinking` and `cache_control` support)
- [ ] Mistral provider integration
- [ ] All 42 providers (mock until real SDK available)
- [ ] Embedding API
- [ ] Model listing
- [ ] `timeout` parameter with httpx.Timeout support (DONE: specced in sync completion; async acompletion() uses float|int per LiteLLM)
- [ ] `extra_headers`, `base_url`, `api_version` parameters (specced above)

### Phase 3: Enterprise Features

- [ ] Router class with load balancing strategies (8 strategies specced)
- [ ] `batch_completion()` and `batch_completion_models()` (in-memory parallel batch — specced above)
- [ ] Batch API (file-based)
- [ ] Responses API
- [ ] Messages API (with `system`, `top_k`, `truncation` support)
- [ ] Budget/metrics APIs
- [ ] `cache_responses` support via **stoolap** semantic cache (RFC-0913) — NOT Redis
- [ ] `num_retries` per-call retry logic (specced above)
- [ ] `logger_fn` custom logger (specced above)
- [ ] Exception regex mapping mode (`QUOTA_ROUTER_UNIFIED_EXCEPTIONS=1` — specced above)
- [ ] Platform provider (any-api key format — specced above)
- [ ] `timeout` with httpx.Timeout support (specced above)
- [ ] `thinking` parameter for Anthropic extended thinking (specced above)

**Note:** `redis_url` is NOT applicable — stoolap (RFC-0912, RFC-0914) replaces Redis entirely as the persistence layer. Caching uses stoolap's WAL-based pub/sub semantic cache per RFC-0913.

### Phase 4: Full LiteLLM Compatibility (Future)

- [ ] Remaining litellm-only parameters: `modalities`, `audio`, `prediction`
- [ ] All litellm routing strategies (8 total: simple-shuffle, round-robin, least-busy, latency-based-routing, cost-based-routing, usage-based-routing, usage-based-routing-v2, weighted)
- [ ] Additional providers from litellm ecosystem as needed

## Key Files to Modify

| File                                         | Change                   |
| -------------------------------------------- | ------------------------ |
| `crates/quota-router-pyo3/src/lib.rs`        | Unified SDK exports      |
| `crates/quota-router-pyo3/src/completion.rs` | Dual-mode completion     |
| `crates/quota-router-pyo3/src/providers/`    | Provider implementations |
| `crates/quota-router-pyo3/src/exceptions.rs` | Exception hierarchy      |
| `crates/quota-router-pyo3/src/sdk.rs`        | set_api_key, budget APIs |

## Future Work

- F1: LangChain integration
- F2: LlamaIndex integration
- F3: Streaming SSE normalization — provider-specific SSE parsing for non-OpenAI-SSE providers (Anthropic, Mistral, etc.), distinct from Phase 1's `async_iter_to_sync_iter()` bridge which handles the sync/async conversion
- F4: Response caching (RFC-0906)

## Rationale

The dual-style approach maximizes adoption by meeting users where they are:

- LiteLLM users keep their `provider=` param pattern
- any-llm users keep their `provider:model` pattern
- Both can coexist in the same codebase

This is the only approach that achieves true drop-in replacement for both ecosystems.

## Version History

| Version | Date       | Changes |
| ------- | ---------- | ------- |
| 1.19    | 2026-04-28 | Fix external adversarial review round 3 (2026-04-28): High (repair async Router corrupted doc), CM-1 (sync Router now uses generic fallbacks list), CM-2 (QUOTA_ROUTER_MODE scope clarified — SDK only, proxy uses config.yaml), CM-3 (empty model_list raises ValueError), L1 (num_retries references FallbackExecutor HTTP retry count), L2 (get_budget_status duplicate docstring removed). |
| 1.18    | 2026-04-28 | Fix external adversarial review round 2 (2026-04-28): CC-1 (synchronized HTTP proxy availability with RFC-0917 — now in all modes), CC-2 (CRITICAL INVARIANT box aligned with RFC-0917), CH-1 (QUOTA_ROUTER_MODE validated against compile-time capabilities), CH-2 (Router no longer retries HTTP calls — Rust core FALLBACK_EXECUTOR handles retry, Router only handles model-level fallback), CM-2 (sync streaming now has model_list param). |
| 1.17    | 2026-04-28 | Fix external adversarial review (2026-04-28): C1 (add QUOTA_ROUTER_MODE runtime selection for full builds), C2 (HTTP proxy only in litellm-mode/full, not any-llm-mode), C4 (add streaming behavior table per mode), H1 (remove / parsing from resolve_provider), H2 (any- key parsing works per-call), H3 (add warning to get_budget_status), M1 (clarify async vs sync timeout types), M2 (document model_list per-call semantics), M4 (implement fallbacks parameter in Router), L4 (make reasoning_effort default explicit). |
| 1.16    | 2026-04-28 | Fix adversarial review v1.15 issue: I1 (response_format added to sync completion() signature, matching async and streaming specs). |
| 1.15    | 2026-04-28 | Fix adversarial review v1.14 issues: I1 (corrected sync completion note — thinking and reasoning_effort are separate params, not aliases), I2 (added timeout to streaming spec signature). |
| 1.12    | 2026-04-28 | Fix adversarial review v1.11 issues: I1 (enable_json_schema_validation added to both signatures), I2 (shared_session added to both signatures), I3 (web_search_options added to both signatures), L1 (streaming spec added response_format), L2 (reasoning_effort default changed from "auto" to None to match LiteLLM, with full enum values listed). |
| 1.11    | 2026-04-28 | Fix adversarial review v1.10 issues: I1 (thinking is structured Dict, not string alias for reasoning_effort), I4 (UnsupportedProviderError added provider_key + supported_providers attrs), I5 (Phase 2 timeout item marked DONE). |
| 1.10    | 2026-04-28 | Fix adversarial review v1.9 issues: I1 (reasoning_effort default changed to "auto" per any-llm), I2 (sync completion() added api_type), I3 (acompletion() added verbosity), I5 (MissingApiKeyError added env_var_name), I7 (sync completion() reasoning_effort default also "auto" + thinking alias noted), I8 (abatch_completion_models() async variant added), L2 (Phase 2 timeout item clarified as verifying async provider calls). |
| 1.9     | 2026-04-28 | Fix adversarial review v1.8 issues: C1 (mode-aware default provider — litellm-mode/full default to "openai", any-llm-mode raises), I1 (functions/function_call passed through to provider SDK), I2 (modalities, audio, prediction moved Phase 4→Phase 3 as they're in LiteLLM sig), I3/I4/I5/I6 (sync completion() now has explicit timeout, api_version, extra_headers, model_list), I7 (Router raises ModelNotFoundError if model not in model_list), I9 (Phase 1 "replace mock" scope clarified — implementation vs interface), I10 (thinking accepted as alias for reasoning_effort), I11 (Embedded API renamed from "any-llm style" to "LiteLLM-compatibility style"). |
| 1.8     | 2026-04-28 | Fix adversarial review v1.7 issues: C1 (get_deployment_mode() now uses cfg-based feature flag injection, not hardcoded string), C2 (acompletion() timeout now float\|int per LiteLLM, not str\|httpx.Timeout), C3 (MissingProviderError→ValueError; step 3 default provider removed since any-llm has none), I1 (class-based API clarified out of scope for Phase 1), I2 (async_coro_to_sync_iter vs async_iter_to_sync_iter clarified), I3 (AllModelsFailedError added to exception hierarchy), I4 (any-llm has no in-memory batch noted), I6 (set_api_key() is LiteLLM compat, any-llm has no equivalent), I8 (deployment_id added to unified signature), I9 (safety_identifier moved Phase 4→Phase 3 as it's in LiteLLM sig), I10 (functions/function_call legacy params added), L2 (Phase 1 tests completion() directly, not Router). |
| 1.7     | 2026-04-28 | Fix adversarial review v1.6 issues: I1 (mode selection for Python SDK build explained — feature flags on quota-router-core, not pyo3), I2 (batch_completion_models() specced with reference to LiteLLM impl), I4 (Phase 1 streaming vs F3 SSE parsing clarified — Phase 1 is raw chunks, F3 is proper SSE transformation), I6 (get_deployment_mode() cfg-based compile-time injection), I7 (Phase 4 "6 strategies" corrected to "8"), I8 (acompletion() fallback comment added), I9 (acompletion() streaming AsyncIterator return type spec'd with reference to LiteLLM CustomStreamWrapper). |
| 1.6     | 2026-04-27 | Fix adversarial review v1.5 issues: C1 (Router.completion() uses explicit import to avoid self-call infinite loop), I1 (get_budget_status behavior in any-llm vs full clarified), I2 (streaming execution path documented), I3 (platform provider RFC-0917 cross-reference verified), I4 (get_deployment_mode() implementation explained), I5 (_select_deployment model param is model_name not model_group), I6 (Phase 1 specifies OpenAI + Anthropic as first providers), L1 (sys.modules mutation removed, explicit import alias), L2 (Phase 3 "6 strategies" corrected to "8"), L3 (F3 streaming vs Phase 1 streaming clarified), L4 (async path now catches Exception like sync), L5 (model vs model_name variable semantics clarified), I7 (version history table aligned). |
| 1.5     | 2026-04-27 | Fix adversarial review v1.4 issues: I1 (corrected ProxyServer claim — RFC-0917 says both interfaces in all modes, mode controls provider strategy not interface), I2 (Python↔Rust exception mapping added with RouterError table), I3 (real SSE streaming spec added, replacing mock), I6 (set_api_key storage mode clarification), I7 (get_budget_status Balance reference), L3 (list_models now Router.list_models() method for LiteLLM compat). Feature gate section corrected per RFC-0917. |
| 1.4     | 2026-04-27 | Fix adversarial review v1.3 issues: C1 (Router now thin PyO3 wrapper delegating to RFC-0902 Rust core), C2 (num_retries now Python param only, references RFC-0902), C3 (added RFC-0913 to dependencies); I1/I2/I3/I4 (all 7 RFC-0902 strategies + fallback types now referenced); L1 (GIL note added to batch_completion), L3 (platform provider cross-ref to RFC-0917). |
| 1.3     | 2026-04-27 | Replace gap analysis with actual specifications: async_iter_to_sync_iter() bridge, batch_completion() with ThreadPoolExecutor, Router 6 strategies, retry logic, logger_fn, exception regex mapping (QUOTA_ROUTER_UNIFIED_EXCEPTIONS=1), platform provider (any-api key format), timeout httpx.Timeout, thinking, system params. Phase 3 updated with all specced items. |
| 1.2     | 2026-04-27 | Gap analysis vs any-llm/litellm: add missing completion params (timeout, thinking, system, etc.), streaming async bridge spec, batch_completion() spec, router strategies, exception mapping, platform provider. Phase 4 added for full LiteLLM compat. Provider count 41→42 (added deepinfra). Clarify redis_url=N/A (stoolap replaces Redis per RFC-0912/0914); cache_responses uses stoolap semantic cache per RFC-0913. |
| 1.1     | 2026-04-27 | Fix all adversarial review issues: C2 (security model docs), C3 (raise error not silent fallback), C4 (ambiguity detection), C5 (case-insensitive provider lookup); I1 (G1=<10ms), I2 (stream=None), I3 (list_models spec), I4 (typed CompletionResponse), I5 (session_label docs), I6 (client_args schema), I7 (error codes), I8 (GIL isolation); L1 (Phase 1 clarify), L2 (deployment mode), L3 (batch API), L4 (RFC-0904 required) |
| 1.0     | 2026-04-27 | Initial draft |

## Related RFCs

- RFC-0908: Python SDK PyO3 Bindings (Superseded)
- RFC-0917: Dual-Mode Query Router (Defines feature gates)

## Related Use Cases

- `docs/use-cases/enhanced-quota-router-gateway.md`
