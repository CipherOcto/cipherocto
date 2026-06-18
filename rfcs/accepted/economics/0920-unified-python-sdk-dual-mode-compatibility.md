# RFC-0920 (Economics): Unified Python SDK — Dual-Mode LiteLLM/any-llm Compatibility

## Status

Accepted (v1.72 — 2026-05-21)

**ARCHITECTURAL CONSTRAINT: HTTP proxy is FOREVER in BOTH litellm-mode and any-llm-mode. See section below.**

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu

## Summary

Define a unified Python SDK via PyO3 that supports **both** LiteLLM-style API (separate `provider` param) and any-llm-style API (`provider:model` format), enabling drop-in replacement for both LiteLLM and any-llm users. This RFC supersedes RFC-0908 and extends RFC-0917 Phase 3 to provide a single SDK that works in both deployment modes.

**RFC-0920's SOLE ROLE:** RFC-0920 is a **THIN BINDING LAYER** — it defines the Python API surface and type marshaling only. ALL heavy lifting (routing, caching, concurrency, telemetry, rate limiting, spend tracking, decay math, fallback coordination, batch execution) is defined in RFC-0917 and implemented in `quota-router-core` (Rust). RFC-0920 does NOT define routing logic, state management, concurrency patterns, or any CPU/IO-intensive behavior. Any such content in this RFC is DEPRECATED and will be removed.

## Dependencies

**Requires:**

- RFC-0917: Dual-Mode Query Router (Accepted) — **definitive source for all heavy lifting**
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

**HTTP proxy architecture:** The HTTP proxy is a Rust binary (hyper/axum). It ALWAYS calls `quota-router-core` directly — it never goes through the PyO3 Python SDK bindings. The proxy's provider strategy is selected at compile time (litellm-mode = reqwest, any-llm-mode = PyO3 bridge) or at startup (full build via config.yaml).

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

### 🚨🚨🚨 ARCHITECTURAL CONSTRAINT — HTTP PROXY IN LITELLM-MODE AND FULL BUILDS 🚨🚨🚨

**THIS IS A FOUNDATIONAL SPEC RULE.**

```
╔═══════════════════════════════════════════════════════════════════════════════════════════╗
║                                                                                           ║
║   🔴 THE HTTP PROXY IS AVAILABLE IN litellm-mode AND full BUILDS 🔴                   ║
║                                                                                           ║
║   The HTTP proxy is a RUST BINARY (hyper/axum). It ALWAYS calls                          ║
║   quota-router-core DIRECTLY. It NEVER goes through PyO3 bindings.                        ║
║                                                                                           ║
║   BUILD-TIME CONSTRAINT (per RFC-0917 §Feature Gate Architecture):                         ║
║   - HTTP proxy IN litellm-mode  : ✅ YES (via reqwest, Rust native HTTP)                  ║
║   - HTTP proxy IN any-llm-mode  : ✅ YES (via PyO3 bridge, Rust core)                     ║
║   - HTTP proxy IN full build    : ✅ YES (both interfaces compiled in)                    ║
║                                                                                           ║
║   In any-llm-mode (the `pip install` package), users access quota-router via Python SDK.   ║
║   In litellm-mode (the gateway binary), users access quota-router via HTTP proxy.         ║
║   In full builds, both interfaces are available.                                         ║
║                                                                                           ║
╚═══════════════════════════════════════════════════════════════════════════════════════════╝
```

**In any-llm-mode, the HTTP proxy calls Rust core directly** — it never goes through the Python SDK bindings. The proxy may internally delegate to the PyO3 bridge for provider calls, but the proxy interface itself always speaks to Rust core. This is the correct performance-first architecture.

### 🚨🚨🚨 CRITICAL CONSTRAINT — RUST-OWNS-ALL-HEAVY-LIFTING 🚨🚨🚨

**THIS IS A CORE PROJECT-WIDE CONSTRAINT. APPLIES TO ALL LANGUAGES (Python, JS, Go, etc.).**

```
╔═══════════════════════════════════════════════════════════════════════════════════════════╗
║                                                                                           ║
║   🔴 RUST CORE OWNS ALL HEAVY LIFTING — ALL OTHER LANGUAGES ARE THIN BRIDGES 🔴          ║
║                                                                                           ║
║   Heavy lifting = routing, caching, concurrency, telemetry, rate limiting,              ║
║   spend tracking, decay math, fallback coordination, batch execution,                    ║
║   request hashing, validation, serialization, any CPU/IO-intensive work.                  ║
║                                                                                           ║
║   Python SDK (quota-router-pyo3) is a THIN PY03 BINDING LAYER ONLY.                      ║
║   - Python MUST NOT implement routing state, locks, decay math, spend history           ║
║   - Python MUST NOT implement caching, hashing, validation logic                        ║
║   - Python MUST NOT implement metric sampling or telemetry collection                    ║
║   - Python MUST NOT implement batch concurrency or worker pooling                       ║
║   - Python ONLY provides API surface, type marshaling, and exception translation        ║
║                                                                                           ║
║   All heavy processing is handled EXCLUSIVELY by quota-router-core (Rust).               ║
║   Python adds ONLY marshaling overhead (<2ms). All latency is Rust-core-bound.          ║
║                                                                                           ║
║   PHASE 1 = LiteLLM/any-llm API surface + thin PyO3 delegation stubs + signature parity  ║
║   PHASE 2 = Semantic cache integration, advanced telemetry (both in Rust core)          ║
║                                                                                           ║
║   ANY RFC THAT VIOLATES THIS CONSTRAINT IS ARCHITECTURALLY WRONG.                       ║
║                                                                                           ║
╚═══════════════════════════════════════════════════════════════════════════════════════════╝
```

### ⚠️ CRITICAL INVARIANT — Mode Gate ≠ Interface

**Per RFC-0917, this is mathematically always true:**

```
For ALL modes (litellm-mode, any-llm-mode, full):
    HTTP proxy interface EXISTS ✅
    Python SDK interface EXISTS ✅

Mode gate controls ONLY: what library calls providers (reqwest vs PyO3)
Mode gate does NOT control: which interfaces exist
```

**HTTP proxy always calls Rust core directly** — it never goes through PyO3 Python SDK bindings.

**Never forget:**
- `litellm-mode` DOES NOT mean "HTTP proxy only" — Python SDK is also available
- `any-llm-mode` DOES NOT mean "Python SDK only" — HTTP proxy is ALSO AVAILABLE (ALWAYS HAS BEEN)
- Both interfaces exist in ALL modes — THIS WILL NEVER CHANGE
- Mode selects provider strategy (reqwest vs PyO3), not interface availability
- **HTTP proxy is ALWAYS in BOTH modes — THIS IS NOT OPEN FOR DISCUSSION**

### Crate Architecture

`quota-router-pyo3` is the **Python SDK crate** — a THIN PY03 BINDING LAYER ONLY. It delegates ALL heavy lifting to `quota-router-core` (Rust):

**CRITICAL: RFC-0920 MUST NEVER DEFINE OR SPECIFY ANY HEAVY LIFTING.**

```
╔═══════════════════════════════════════════════════════════════════════════════════════════╗
║                                                                                           ║
║   🔴 RFC-0920 IS A THIN BINDING LAYER ONLY — NO HEAVY LIFTING 🔴                        ║
║                                                                                           ║
║   RFC-0920 DEFINES:                                                                      ║
║   • Python API surface (function signatures)                                            ║
║   • Type marshaling (Python ↔ Rust)                                                      ║
║   • Exception translation (RouterError → Python exceptions)                              ║
║   • Provider resolution (provider:model parsing)                                        ║
║                                                                                           ║
║   RFC-0920 MUST NEVER DEFINE:                                                            ║
║   • Routing strategies or state management (→ RFC-0917)                               ║
║   • Decay math, spend tracking, or latency tracking (→ RFC-0917)                      ║
║   • Batch concurrency or worker pooling (→ RFC-0917)                                   ║
║   • Caching, hashing, or validation logic (→ RFC-0917)                                 ║
║   • Telemetry or metric collection (→ RFC-0917)                                         ║
║                                                                                           ║
║   RFC-0917 IS THE DEFINITIVE SOURCE FOR ALL HEAVY LIFTING.                              ║
║   RFC-0920 REFS RFC-0917 ONLY — IT DOES NOT REIMPLEMENT OR EXTEND.                      ║
║                                                                                           ║
║   ⚠️  ANY CONTENT IN RFC-0920 THAT DEFINES HEAVY LIFTING IS:                           ║
║   (1) A BUG IN THIS RFC — NOT A FEATURE                                                 ║
║   (2) DEPRECATED AND SUBJECT TO REMOVAL WITHOUT NOTICE                                   ║
║                                                                                           ║
╚═══════════════════════════════════════════════════════════════════════════════════════════╝
```

```
┌─────────────────────────────────────────────────────────────────┐
│              quota-router-pyo3 (Python SDK) — THIN BINDING        │
│  • Registers completion(), acompletion(), set_api_key(), etc.     │
│  • Thin PyO3 calls into Rust core — NO heavy processing in Python │
│  • API surface & type marshaling ONLY                            │
│  • Provider resolution (provider:model parsing)                    │
│  • Exception mapping (Python → unified types)                    │
└─────────────────────────────────────────────────────────────────┘
                              │ PyO3
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│              quota-router-core (Rust core) — ALL HEAVY LIFTING   │
│  • KeyMiddleware    — API key validation                        │
│  • Balance         — OCTO-W spend tracking                      │
│  • StoolapKeyStorage — Persistence (RFC-0912/0914)            │
│  • KeyCache (L1)   — In-memory key cache with TTL             │
│  • RateLimiter     — TokenBucket RPM/TPM enforcement            │
│  • Router          — Routing strategies, state, decay math       │
│  • FallbackExecutor — Retry with backoff                       │
│  • Provider        — Provider config (endpoint, rpm, tpm, weight) │
│  • PricingRegistry — Token pricing (RFC-0910)                   │
│  • RouterHandle    — PyO3-exposed handle for Python SDK         │
└─────────────────────────────────────────────────────────────────┘
```

**Python-to-Rust component mapping:**

| Python API | Rust Core Component | Notes |
|------------|---------------------|-------|
| `set_api_key(provider, key)` | `KeyMiddleware::validate_key()` + `StoolapKeyStorage` | Validates then persists |
| `get_budget_status()` | `Balance` + `StoolapKeyStorage` | Returns OCTO-W spend |
| `completion()` | `RouterHandle.completion()` | Thin PyO3 delegation — all routing in Rust |
| `Router` class | `RustRouterHandle` | Thin PyO3 wrapper — no Python-side routing state |
| `cache_bypass` flag | `RouterHandle` | Forwarded to Rust cache/validation layer |
| `batch_completion()` | `RouterHandle.batch()` | Thin PyO3 delegation — Rust parallel executor |
| `num_retries` | `FallbackExecutor` | Rust handles retry, not Python |
| `cache_responses` | `KeyCache` + `StoolapKeyStorage` | Rust manages semantic cache (RFC-0913) |
| Rate limiting | `RateLimiter` | Rust TokenBucket enforcement |
| All routing state | `Router` (Rust core) | Python Router class is DEPRECATED stub |
| All telemetry | Rust Prometheus/OTLP | Python queries Rust via `get_metrics()` |
| Exception mapping | `RouterError` → Python | PyO3 exception translation |

**Two modes (feature flags) control provider integration — NOT interface availability:**

| Mode | Provider Strategy | HTTP Proxy? | Python SDK? |
|------|-----------------|:------------:|:------------:|
| `litellm-mode` | reqwest HTTP (Rust) | ✅ Yes (reqwest-based) | ✅ Yes |
| `any-llm-mode` | PyO3 → Python SDK | ✅ Yes (via PyO3 bridge) | ✅ Yes |
| `full` | Both | ✅ Yes (both reqwest + PyO3 bridge) | ✅ Yes |

**Mode gate controls HOW (reqwest vs PyO3), NOT WHETHER (proxy vs SDK).**

**HTTP proxy always calls quota-router-core directly** — it never goes through the PyO3 Python SDK bindings. In any-llm-mode, the proxy calls Rust core (which may internally delegate to Python SDKs via PyO3 bridge), but the proxy itself only ever speaks to Rust core. This is the correct performance-first architecture.

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

_EMBEDDED_MODE = getattr(quota_router, "__deployment_mode__", "full")

def _get_compiled_modes() -> set:
    """Returns the set of modes compiled into this binary.

    Populated at build time via Cargo feature flags.
    In a 'full' build, returns {"litellm-mode", "any-llm-mode", "full"}.
    In a single-mode build, returns just that mode.
    """
    # Injected at build time by py-o3 build.rs / Cargo.toml config
    return getattr(quota_router, "__compiled_modes__", {_EMBEDDED_MODE})
```

**⚠️ PyPI wheels are single-mode:** `pip install quota-router` installs an `any-llm-mode` wheel. `QUOTA_ROUTER_MODE` has no effect on PyPI wheels — feature flags are compile-time and cannot be changed at runtime. Attempting to set `QUOTA_ROUTER_MODE=litellm-mode` on a PyPI-installed SDK will be ignored.

**QUOTA_ROUTER_MODE runtime selection ONLY applies to `full` dev builds** (built via `cargo build --features full`). These builds include both reqwest and PyO3 provider strategies. However, QUOTA_ROUTER_MODE does NOT switch the provider strategy at runtime — it only changes the `__deployment_mode__` string label and internal routing hints. N16 fix: The mode label returned by `get_deployment_mode()` controls logging/observability, not the actual HTTP call path. In `full` builds, both reqwest and PyO3 are compiled in; the mode label is informational only.

### Dual-Mode API Conventions

**⚠️ Mode ≠ Interface reminder:** Both HTTP proxy and Python SDK exist in ALL modes. The HTTP proxy ALWAYS calls Rust core directly. Mode selects provider strategy (reqwest vs PyO3), not which interface is available.

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
    stream: bool = False,  # False = sync response; True = streaming; matches LiteLLM behavior
    stream_options: Optional[Dict] = None,
    raw_stream: bool = False,  # Phase 1: ignored (same as stream=True); Phase 3: marker for forcing raw chunks
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
    cache_bypass: bool = False,  # If True, skips KV cache lookup and top-level parameter validation.
                          # ⚠️ NOTE: Does NOT validate nested `messages` content. Malformed floats/objects
                          # in messages will be deferred to the provider SDK. Bypassing cache increases
                          # provider request volume. During provider instability or rate limiting, this
                          # amplifies fallback trigger rates and quota consumption. Monitor fallback
                          # metrics closely when cache_bypass=True. RECOMMENDED for >50k token payloads.

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

    Resolution priority (per RFC-0917 §C8):
    1. provider param if provided and non-empty
    2. Parse model string using provider-list matching:
       - Try colon: if segment before ":" is a known provider → colon format (provider:model)
       - Try slash: if segment before "/" is a known provider → slash format (provider/model)
       - If neither delimiter's prefix matches a known provider → use default
    3. Use default provider for mode (litellm-mode/full: "openai"; any-llm-mode: raise)

    Examples (per RFC-0917 §C8):
        "openai:gpt-4o" → provider="openai", model="gpt-4o" (colon match)
        "ollama/llama3.1:8b" → provider="ollama", model="llama3.1:8b" (slash match, colon in model name)
        "anthropic/claude-opus-4-250624" → provider="anthropic", model="claude-opus-4-250624" (slash match)
        "gpt-4o" → provider="openai" (default), model="gpt-4o"

    Graceful degradation: Model strings with unknown provider prefixes use default_provider
    (a warning is logged at WARN level for operator awareness).

    Raises:
        ValueError: If no provider can be determined (any-llm-mode behavior)
    """
    # 1. Explicit provider param wins (case-insensitive normalization)
    # M1 fix: explicit empty-string check — "" is falsy in Python but should raise
    if provider_param is not None and provider_param != "":
        return provider_param.lower(), model
    if provider_param == "":
        raise ValueError("provider parameter must be a non-empty string or None")

    # 2. Parse model string using provider-list matching (per RFC-0917 §C8)

    # Try colon format first
    if ":" in model:
        colon_candidate, _, model_name = model.partition(":")
        if is_known_provider(colon_candidate.lower()):
            provider = colon_candidate.lower()
            if model_name.lower() == provider:
                import warnings
                warnings.warn(
                    f"Ambiguous model string '{model}' — provider and model name are identical. "
                    f"Assuming provider='{provider}', model='{model_name}'. "
                    f"To silence this, use explicit provider= parameter.",
                    UserWarning,
                    stacklevel=2,
                )
            return provider, model_name

    # Try slash format
    if "/" in model:
        slash_candidate, _, model_name = model.partition("/")
        if is_known_provider(slash_candidate.lower()):
            provider = slash_candidate.lower()
            return provider, model_name

    # 3. Mode-aware fallback or error
    default_provider = DEFAULT_PROVIDER_BY_MODE.get(deployment_mode)
    if default_provider:
        return default_provider, model
    # any-llm-mode: no default, raise error (matches any-llm behavior)
    raise ValueError(
        f"Invalid model format '{model}'. Expected 'provider:model' format (any-llm) "
        f"or 'provider/model' format (LiteLLM), or pass provider='<name>' parameter. "
        f"Known providers: {', '.join(sorted(KNOWN_PROVIDERS))}"
    )
```

### Supported Providers (41)

`KNOWN_PROVIDERS` is the runtime registry of all supported provider names. It is derived from the provider plugin system (per RFC-0917 §Provider Integration Strategy) and used by `is_known_provider()` for case-insensitive lookup. The 42 providers listed below are the initial set at RFC-0920 acceptance; the registry is extensible via the provider plugin system.

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

**Gap vs any-llm**: any-llm has 39 providers; quota-router has 42 total (adds `deepinfra` + 2 others not in any-llm).

**Gap vs litellm**: litellm has 100+ providers. Missing from quota-router: `replicate`, `azure_ai`, `bedrock_mantle`, `anyscale`, `fireworks_ai`, `localai`, `manifest`, `mimechat`, `nlp_cloud`, `predibase`, `proai`, `qianfan`, `sagemaker_chat`, `together_ai`, `yandex`, `yi`, `zhipuai`, and many `openai_like` providers. These can be added as needed via the provider plugin system.

**SDK Entry Point Validation (NaN/Inf rejection):**

Before any processing, SDK entry points MUST validate top-level params and `messages` to reject NaN/Inf float values:

```python
import math

def _validate_no_nan_inf(params: Dict) -> None:
    """
    Validate top-level params contain no NaN/Inf float values.
    Raises InvalidRequestError if any float value is NaN or Infinity.
    M3 fix: Also validate messages list type (O(1) isinstance check).
    G1 note: Deep nested validation omitted — only top-level + messages checked.
    SECURITY NOTE: NaN in JSON is non-standard and causes downstream parse failures
    (Provider SDK may reject or misinterpret NaN-serialized requests).
    """
    # M3 fix: O(1) messages type check — NaN in tool call args, image data,
    # or structured content in messages is a caller error, not SDK validation scope.
    # Callers must sanitize deeply nested values before passing to SDK.
    for key, value in params.items():
        if isinstance(value, float):
            if math.isnan(value) or math.isinf(value):
                raise InvalidRequestError(
                    f"Invalid float value '{value}' for parameter '{key}'. "
                    f"NaN and Infinity are not permitted."
                )
```

**SDK entry point call sites:**
```python
def completion(model: str, messages: List[Dict], **kwargs):
    # M3 fix: validate messages (O(1) isinstance check) + kwargs
    if not isinstance(messages, list):
        raise InvalidRequestError(f"messages must be a list, got {type(messages).__name__}")
    _validate_no_nan_inf(kwargs)  # Validate before routing/cache
    # ... rest of execution

async def acompletion(model: str, messages: List[Dict], **kwargs):
    # M3 fix: validate messages (O(1) isinstance check) + kwargs
    if not isinstance(messages, list):
        raise InvalidRequestError(f"messages must be a list, got {type(messages).__name__}")
    _validate_no_nan_inf(kwargs)  # Validate before routing/cache
    # ... rest of execution
```

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
    """Base exception for all quota-router errors. Unified per RFC-0917 canonical definition."""
    code: str                           # Error code string
    status: int = 0                     # HTTP status code (0 = not applicable)
    provider: Optional[str] = None     # Provider name if applicable
    details: dict = {}                 # Structured details

    def __init__(self, message: str, code: str, status: int = 0,
                 provider: Optional[str] = None, details: Optional[dict] = None):
        super().__init__(message)
        self.code = code
        self.status = status
        self.provider = provider
        self.details = details or {}

class AuthenticationError(QuotaRouterError):
    """Invalid or missing API key."""
    pass

class RateLimitError(QuotaRouterError):
    """Rate limit exceeded."""
    retry_after: Optional[float] = None

    def __init__(self, message: str, code: str = "rate_limit_exceeded",
                 retry_after: Optional[float] = None, **kwargs):
        super().__init__(message, code, status=429, **kwargs)
        self.retry_after = retry_after

class InvalidRequestError(QuotaRouterError):
    """Malformed request parameters."""
    param: Optional[str] = None

    def __init__(self, message: str, code: str = "invalid_request",
                 param: Optional[str] = None, **kwargs):
        super().__init__(message, code, status=400, **kwargs)
        self.param = param

class ProviderError(QuotaRouterError):
    """Provider-side error."""
    upstream_code: Optional[str] = None

    def __init__(self, message: str, code: str = "provider_error",
                 upstream_code: Optional[str] = None, **kwargs):
        super().__init__(message, code, status=502, **kwargs)
        self.upstream_code = upstream_code

class ContentFilterError(QuotaRouterError):
    """Content filtered by provider."""
    pass

class ModelNotFoundError(QuotaRouterError):
    """Unknown model identifier."""
    pass

class ContextLengthExceededError(QuotaRouterError):
    """Token limit exceeded."""
    max_tokens: Optional[int] = None
    received_tokens: Optional[int] = None

    def __init__(self, message: str, code: str = "context_length_exceeded",
                 max_tokens: Optional[int] = None, received_tokens: Optional[int] = None,
                 **kwargs):
        super().__init__(message, code, status=400, **kwargs)
        self.max_tokens = max_tokens
        self.received_tokens = received_tokens

class MissingApiKeyError(QuotaRouterError):
    """No API key provided and none in environment."""
    provider: str = ""
    env_var_name: str = ""

    def __init__(self, message: str, code: str = "missing_api_key",
                 provider: str = "", env_var_name: str = "", **kwargs):
        super().__init__(message, code, status=401, **kwargs)
        self.provider = provider
        self.env_var_name = env_var_name

class UnsupportedProviderError(QuotaRouterError):
    """Provider not supported."""
    provider_key: str = ""
    supported_providers: List[str] = []

    def __init__(self, message: str, code: str = "unsupported_provider",
                 provider_key: str = "", supported_providers: Optional[List[str]] = None,
                 **kwargs):
        super().__init__(message, code, status=400, **kwargs)
        self.provider_key = provider_key
        self.supported_providers = supported_providers or []

class UnsupportedParameterError(QuotaRouterError):
    """Parameter not supported by provider."""
    param: str = ""
    provider: str = ""

    def __init__(self, message: str, code: str = "unsupported_parameter",
                 param: str = "", provider: str = "", **kwargs):
        super().__init__(message, code, status=400, **kwargs)
        self.param = param
        self.provider = provider

class InsufficientFundsError(QuotaRouterError):
    """OCTO-W balance insufficient. Use int μunits to avoid float precision loss (RFC-0904 G3)."""
    current_balance: int = 0  # μunits
    required: int = 0          # μunits

    def __init__(self, message: str, code: str = "insufficient_funds",
                 current_balance: int = 0, required: int = 0, **kwargs):
        super().__init__(message, code, status=402, **kwargs)
        self.current_balance = current_balance
        self.required = required

class UpstreamProviderError(QuotaRouterError):
    """Provider returned an error."""
    status_code: Optional[int] = None

    def __init__(self, message: str, code: str = "upstream_provider_error",
                 status_code: Optional[int] = None, **kwargs):
        super().__init__(message, code, status=502, **kwargs)
        self.status_code = status_code

class GatewayTimeoutError(QuotaRouterError):
    """Provider gateway timeout."""
    pass

class LengthFinishReasonError(QuotaRouterError):
    """Response truncated due to length."""
    finish_reason: str = ""

    def __init__(self, message: str, code: str = "length_finish_reason",
                 finish_reason: str = "", **kwargs):
        super().__init__(message, code, status=400, **kwargs)
        self.finish_reason = finish_reason

class ContentFilterFinishReasonError(QuotaRouterError):
    """Response filtered."""
    finish_reason: str = ""

    def __init__(self, message: str, code: str = "content_filter_finish_reason",
                 finish_reason: str = "", **kwargs):
        super().__init__(message, code, status=400, **kwargs)
        self.finish_reason = finish_reason

class BatchNotCompleteError(QuotaRouterError):
    """Batch job not yet complete."""
    batch_id: str = ""
    status: str = ""

    def __init__(self, message: str, code: str = "batch_not_complete",
                 batch_id: str = "", status: str = "", **kwargs):
        super().__init__(message, code, status=400, **kwargs)
        self.batch_id = batch_id
        self.status = status

class AllModelsFailedError(QuotaRouterError):
    """All models failed in batch_completion_models() race."""
    models: List[str] = field(default_factory=list)

    def __init__(self, message: str, code: str = "all_models_failed",
                 models: Optional[List[str]] = None, **kwargs):
        super().__init__(message, code, status=500, **kwargs)
        self.models = models or []

class BatchPartialFailureError(QuotaRouterError):
    """Some requests in batch failed, partial results returned."""
    successful: List[CompletionResponse] = field(default_factory=list)
    failed: List[Tuple[str, Exception]] = field(default_factory=list)

    def __init__(self, message: str, code: str = "batch_partial_failure",
                 successful: Optional[List[CompletionResponse]] = None,
                 failed: Optional[List[Tuple[str, Exception]]] = None,
                 **kwargs):
        super().__init__(message, code, status=207, **kwargs)
        self.successful = successful or []
        self.failed = failed or []
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
| `ModelNotFoundError`      | `RouterError::ModelNotFound` | Model not found (404)       |

**Note:** `RouterError::ModelNotFound` is a distinct variant from `RouterError::ProviderUnavailable`. A 404 response means the model is permanently unavailable (wrong model name, deprecated model), not that the provider is down. Conflating these causes incorrect retry behavior: `ModelNotFoundError` should NOT trigger retries or fallback — the model does not exist.

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
    RouterError::ModelNotFound => PyErr::new::<ModelNotFoundError, _>(...),
    RouterError::Unknown => PyErr::new::<ProviderError, _>(...),
}

**Normative enforcement:** The Rust `FallbackExecutor` implementation MUST explicitly handle `ModelNotFoundError` with zero retry attempts:

```rust
// In FallbackExecutor::execute()
match error {
    RouterError::ModelNotFound => return Err(e),  // NO retry, NO backoff
    RouterError::RateLimit => { /* retry with backoff */ }
    RouterError::Timeout => { /* retry with backoff */ }
    // ... other errors with retry logic
}
```

**ModelNotFoundError retry behavior:** Unlike transient errors (rate limit, timeout, provider down), `ModelNotFoundError` indicates a permanent failure (wrong model name, deprecated model, 404). The fallback executor must NOT retry on `ModelNotFoundError` — retries would waste resources on an unconditionally unavailable model.

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

Per RFC-0917 §A8, `set_api_key()` derives a budget identity via HMAC-SHA256 and stores it via `StoolapKeyStorage`. **Budget enforcement (RFC-0904) is active in ALL modes** (litellm-mode, any-llm-mode, full) — the "None" in the table below was incorrect and is now fixed.

| Mode    | Storage              | Budget Enforcement |
| ------- | -------------------- | ------------------ |
| any-llm | `StoolapKeyStorage`  | RFC-0904 enforced  |
| full    | `StoolapKeyStorage`  | RFC-0904 enforced  |

**Implementation (per RFC-0917 §A8):**

```rust
// set_api_key() → KeyMiddleware::validate_key() + StoolapKeyStorage
// 1. Format validation
// 2. key_id = HMAC-SHA256(server_secret, provider_key)[..16]
// 3. key_hash = HMAC-SHA256(server_secret, provider_key)
// 4. Persist to StoolapKeyStorage (WAL persisted, survives restarts)
// 5. All record_spend() calls use key_id for budget tracking
```

**Key insight:** In single-mode `any-llm-mode`, keys are persisted to `StoolapKeyStorage` (same as full mode). Budget enforcement applies in all modes. The previous "In-memory HashMap, no enforcement" description was incorrect.

**Important — `get_budget_status()` behavior by mode:**

| Mode    | get_budget_status() returns | Notes |
| ------- | -------------------------- | ----- |
| any-llm | Real Balance from StoolapKeyStorage | Persisted, accurate |
| full    | Real Balance from StoolapKeyStorage | Persisted, accurate |

Both modes return persisted balance data from `StoolapKeyStorage`. The previous "Estimated from in-memory tracking" description for any-llm-mode was incorrect.

#### get_budget_status() — Balance Reference

**Severity: Important**

`get_budget_status()` returns OCTO-W spend data from Rust `Balance` struct:

```rust
// quota-router-core/src/balance.rs
//
// All monetary fields are u64 micro-units (μunits) per RFC-0904 G3.
// Conversion: 1 μunit = 0.000001 USD
pub struct Balance {
    pub key_id: String,
    pub team_id: String,
    pub current_spend: u64,              // μunits
    pub budget_limit: Option<u64>,       // μunits (0 = unlimited)
    pub last_updated: DateTime<Utc>,
}
```

**Python return type:**

```python
@dataclass
class BudgetStatus:
    balance: int           # Current OCTO-W balance in μunits (micro-units)
    total_spend: int      # Cumulative spend in μunits
    budget_limit: Optional[int]  # Cap if set (μunits)
    last_updated: str       # ISO 8601 timestamp
    key_id: Optional[str]   # For which key (if tracked)

def get_budget_status(provider: Optional[str] = None) -> BudgetStatus:
    """
    Returns OCTO-W budget status from Rust Balance + StoolapKeyStorage.

    All monetary values are in **μunits** (micro-units) per RFC-0904 G3.
    Conversion: `balance_μunits / 1_000_000` = USD.
    Example: `balance = 1500000` means $1.50 USD.

    Multi-key semantics:
    - `get_budget_status()` (provider=None): Returns **aggregated** balance across all
      registered keys. Sums current_spend from all tracked key_ids.
    - `get_budget_status(provider="openai")`: Returns balance for the **specific provider's**
      default key (the key registered via set_api_key("openai", ...) or the first
      key matching that provider). Raises KeyNotFoundError if no key exists for provider.

    ⚠️ WARNING: In any-llm-mode, budget tracking uses HMAC-SHA256-derived key_id
    per RFC-0917 §Budget Identity in SDK Mode. Budget enforcement applies when
    using set_api_key(). Direct per-call api_key= parameter bypasses enforcement.

    | Mode    | Budget Identity | Enforcement |
    | ------- | -------------- | ----------- |
    | any-llm | HMAC-SHA256(provider_key) | Enforced via set_api_key() |
    | full    | HMAC-SHA256(provider_key) | Enforced + persisted |

    Args:
        provider: Optional provider name to get per-provider budget status.
                 If None, returns aggregated balance across all providers.

    Returns:
        BudgetStatus with balance, total_spend, budget_limit, last_updated (all in μunits)
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
    provider: str,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    **kwargs
) -> Sequence[Model]:
    """
    List available models for a provider.

    Args:
        provider: Provider name (e.g., "openai", "anthropic"). Required.
        api_key: Override API key for this call.
        api_base: Override base URL for this call.
        client_args: Additional provider-specific arguments.

    Returns:
        Sequence of Model objects with fields: id, name, provider, created, description.

    Raises:
        MissingApiKeyError: If no API key available.
        ProviderError: If provider API call fails.
    """

# Router instance method
class Router:
    def list_models(
        self,
        provider: Optional[str] = None,
    ) -> Sequence[Model]:
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
| `prompt_cache_key`       | `str`         | Prompt caching key                 | §Prompt Cache        |
| `prompt_cache_retention` | `str`         | Prompt cache TTL                   | §Prompt Cache        |
| `conversation`           | `str`         | Conversation ID for continuity     | §Conversation        |

Note: `safety_identifier` is NOT in this table — it IS specced in RFC-0920 (present in sync completion signature, Phase 3).

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

    ⚠️ All run_in_executor callbacks MUST use early binding for loop variables:
        - CORRECT: lambda i=item: q.put(i, timeout=timeout)
        - WRONG:   lambda: q.put(item, timeout=timeout)  # late binding = corrupt data
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
                        # Direct q.put() with timeout — daemon thread blocking IS the correct
                        # backpressure mechanism. Blocking this isolated thread pauses async
                        # iteration until the sync consumer drains the queue.
                        # IMPORTANT: Bind item at definition time using default arg (i=item)
                        # to prevent late-binding corruption in async loops.
                        q.put(item, timeout=timeout)
                finally:
                    # Always put sentinel — async for does NOT raise StopAsyncIteration
                    # when the iterator exits normally (it catches it internally).
                    q.put(StopIteration, timeout=timeout)
            loop.run_until_complete(run())
        except Exception as e:  # noqa: BLE001
            exception_store[0] = e
            try:
                q.put(StopIteration, timeout=timeout)
            except Exception:
                pass

    thread = threading.Thread(target=consume_async, daemon=True)
    thread.start()

    while True:
        item = q.get(timeout=timeout * 2)
        if item is StopIteration:
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
    raw_stream: bool = False,  # Phase 1: ignored (same as stream=True); Phase 3: marker for forcing raw chunks
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
current call only, using a **lightweight stateless ModelSelector** (not a full Router
instance) to preserve the `<10ms` function call overhead target (G1).

Execution path for module-level `completion(model_list=[...])`:
1. Parse model_list to extract deployment candidates
2. Apply `simple-shuffle` strategy (stateless **uniform** random selection — no round-robin state)
3. Return the selected deployment params

**Why simple-shuffle (uniform) for transient model_list:** A full `Router` instance per call adds ~2-5ms overhead (locks, dicts, deques initialization), violating G1. `simple-shuffle` selects uniformly at random without maintaining state — correct for stateless per-call use.

**Weight/RPM/TPM handling in transient model_list:** Transient `ModelSelector` uses **uniform random**, ignoring `weight`, `rpm`, `tpm` fields. For weighted per-call selection, use explicit `Router` class with `routing_strategy="weighted"`. `rpm`/`tpm` are enforced by Rust core rate limiter, not the Python layer.

For stateful strategies (round-robin, latency-based, cost-based), use the persistent `Router` class.

Each dict in model_list follows the deployment format:
{"model_name": "...", "api_base": "...", "api_key": "...", "rpm": N, "tpm": N}.
If the requested model is not in the list, raises ModelNotFoundError.
This parameter does NOT modify any global Router configuration.

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
    cache_bypass: bool = False,  # If True, skips KV cache lookup and top-level parameter validation.
                          # ⚠️ NOTE: Does NOT validate nested `messages` content. Malformed floats/objects
                          # in messages will be deferred to the provider SDK. Bypassing cache increases
                          # provider request volume. During provider instability or rate limiting, this
                          # amplifies fallback trigger rates and quota consumption. Monitor fallback
                          # metrics closely when cache_bypass=True. RECOMMENDED for >50k token payloads.

    # Note: `thinking` (structured Dict) and `reasoning_effort` (string enum) are separate parameters in LiteLLM, not aliases
    # Anthropic thinking param — REQUIRED for drop-in replacement (CRITICAL)
    thinking: Optional[Dict] = None,
    # Multi-modal output — REQUIRED for drop-in replacement (HIGH)
    modalities: Optional[List[str]] = None,  # e.g., ["text", "audio"]
    audio: Optional[Dict] = None,  # Audio output params
    prediction: Optional[Dict] = None,  # Predicted outputs (content prediction optimization)
    **kwargs,
) -> Union[CompletionResponse, Iterator[ChatCompletionChunk]]:
    """
    Route and dispatch completion requests.

    ⚠️ OPERATIONAL WARNING: cache_bypass=True disables exact-match deduplication
    and top-level validation in the Python SDK layer. In PyO3 builds (any-llm-mode, full),
    this flag is forwarded to the underlying provider SDK which may apply additional
    cache/validation semantics. Increases provider request volume and fallback trigger
    rates during instability. Monitor quota and fallback metrics closely.

    When stream=True in any-llm-mode, returns an iterator of chunks.

    **Phase 1 return type note:** In Phase 1, `stream=True` returns **raw provider-native
    chunks** (e.g., Anthropic `MessageStreamEvent`, OpenAI `ChatCompletionChunk` with SSE).
    Phase 3 (F3) will normalize all providers to `ChatCompletionChunk` format with SSE
    transformation.

    The return type is `Iterator[ChatCompletionChunk]` for OpenAI-compatible output in
    Phase 3. In Phase 1, consumers may receive provider-specific chunk types.
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

**Async Proxy + PyO3 GIL Integration (any-llm-mode):**

The HTTP proxy uses `hyper`/`axum` (async Rust) in all modes. In `any-llm-mode`, when the proxy must call Python provider SDKs via PyO3, use the appropriate integration pattern based on SDK type:

**Dual-path spec:**

| Python SDK Type | Integration Pattern | Rationale |
|----------------|---------------------|-----------|
| Sync SDKs (e.g., `requests`-based) | `tokio::task::spawn_blocking` | Blocking GIL-acquiring calls must not hold async executor thread |
| Async SDKs (e.g., `openai.AsyncOpenAI`, `anthropic.AsyncAnthropic`) | `pyo3-asyncio` or `tokio::task::spawn_local` + GIL-release | Async SDKs release GIL during network I/O; only marshaling needs GIL |

**For sync SDKs:**
```rust
async fn handle_completion(req: Request) -> Result<Response, HyperError> {
    let py_result = tokio::task::spawn_blocking(|| {
        Python::with_gil(|py| {
            call_python_sdk(py, &req)  // sync SDK call
        })
    }).await?;
    Ok(Response::new(Body::from(py_result)))
}
```

**For async SDKs (preferred):**
```rust
use pyo3_asyncio::tokio::into_future;

// Python side exposes async function:
// async def acall_completion(...): ... → AsyncIterator[ChatCompletionChunk]

async fn handle_completion(req: Request) -> Result<Response, HyperError> {
    // N5 fix: into_future (not into_async) — into_async was removed in pyo3-asyncio 0.16+
    let py_result = into_future(call_python_completion_async(req)).await?;
    Ok(Response::new(Body::from(py_result)))
}
```

**Why dual-path:** `spawn_blocking` for async SDKs adds thread-switch overhead and defeats async concurrency benefits. Async SDKs release the GIL during network I/O — using `spawn_blocking` serializes requests on the blocking thread pool.

**Provider SDK Type Registry:**

The proxy MUST have a compile-time or runtime registry mapping each provider to its SDK type. **M1 fix: Single source of truth.** Python and Rust registries MUST be generated from a shared config to prevent drift:

```yaml
# providers_sdk_types.yaml — shared config for Python + Rust generation
# CROSS-1 fix: This YAML is now synchronized with RFC-0917's canonical YAML.
# The canonical source is RFC-0917 §Provider SDK Type Registry.
# RFC-0920 uses the same providers_sdk_types.yaml file from the quota-router-core
# workspace, not a separate copy.
providers:
  openai: async
  anthropic: async
  mistral: async
  gemini: async
  ollama: sync
  deepinfra: async
  groq: async
default: sync
```

**Build-time generation (mandatory):**
- Python: `providers_sdk_types.yaml` → `providers.py` (dict at module level)
- Rust: `providers_sdk_types.yaml` → `providers.rs` (`HashMap<&str, &str>`)
- Both generated from the single `providers_sdk_types.yaml` in `quota-router-core/`

**CROSS-1/H2 fix: Rust output format mandate.** CI parity validation requires strict Rust syntax. Deviations from this format will break CI:
```rust
// REQUIRED FORMAT for CI parity validation — do not deviate
const PROVIDER_SDK_TYPES: &[(&str, &str)] = &[
    ("openai", "async"),
    ("anthropic", "async"),
    ("mistral", "async"),
    ("gemini", "async"),
    ("ollama", "sync"),
    ("deepinfra", "async"),
    ("groq", "async"),
    ("bedrock", "sync"),  // N18 fix: AWS boto3/botocore — sync SDK via spawn_blocking
    ("default", "sync"),
];
```
Regex used: `r'\(\s*"([\w.-]+)"\s*,\s*"([\w.-]+)"\s*\)'`. The `\s*` pattern safely matches spaces, tabs, and newlines — rustfmt formatting is allowed.

**H2 fix: CI validation command** (replaces git-dependent bash script — broken in shallow clones):

**Environment-agnostic Python schema validator** (replaces git metadata approach):
```python
#!/usr/bin/env python3
# ci/validate_registry_parity.py
"""Deterministic registry parity validation — no git metadata required.

NOTE: This validates build-time codegen for API compatibility ONLY.
Runtime routing, caching, and telemetry are owned by RFC-0902 Rust core (proxy)
or Python Router class (SDK Phase 1).

The Python Router class is specified as a Python-level component (NOT a Rust
delegation) per RFC-0920 Python Router specification. Rust delegation is Phase 2.
"""
from pathlib import Path
import sys, yaml, os, argparse

def find_repo_root(start: Path) -> Path:
    """H1 fix: Find repo root via .git/ or ci/ marker instead of subproject toml files.
    H3 fix: Correct operator precedence — and binds tighter than or requires parentheses.
    H3 fix: Use .is_dir() for .git to distinguish repo (dir) from submodule pointer (file).
    In monorepos, subprojects may have their own pyproject.toml/Cargo.toml.
    .git at repo root is a directory; submodule .git is a file."""
    for p in [start] + list(start.parents):
        if (p / ".git").is_dir() or (
            ((p / "pyproject.toml").exists() or (p / "Cargo.toml").exists())
            and (p / "ci").exists()
        ):
            return p
    raise FileNotFoundError("Repository root not found (missing .git/ or ci/ directory)")

def main():
    # H1 fix: Use CLI arg or marker-file search instead of hardcoded parent.parent
    # M2 fix: Use absolute() instead of resolve() for container/symlink safety
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-dir", type=Path, default=None)
    args = parser.parse_args()
    BASE_DIR = args.base_dir if args.base_dir else find_repo_root(Path(__file__).absolute())

    REQUIRED_FILES = ["providers_sdk_types.yaml", "providers.py", "providers.rs"]
    for fname in REQUIRED_FILES:
        fpath = BASE_DIR / fname
        if not fpath.exists():
            print(f"ERROR: Required file not found: {fpath}", file=sys.stderr)
            sys.exit(1)

    # Load expected from YAML
    with open(BASE_DIR / "providers_sdk_types.yaml") as f:
        expected = yaml.safe_load(f)

    # H1/L1 fix: Document pure-dict-literal codegen contract.
    # CI VALIDATOR CONTRACT: providers.py MUST export PROVIDER_SDK_TYPES as a pure dict literal.
    # Codegen templates MUST NOT use dict(), unpacking, dynamic expressions, OR inline comments
    # within the dict literal. Example: PROVIDER_SDK_TYPES = {"openai": "async", "default": "sync"}
    import ast
    with open(BASE_DIR / "providers.py") as f:
        tree = ast.parse(f.read())
    assignments = [
        node for node in tree.body
        if isinstance(node, ast.Assign)
        and any(isinstance(t, ast.Name) and t.id == "PROVIDER_SDK_TYPES" for t in node.targets)
    ]
    if len(assignments) != 1:
        print("ERROR: PROVIDER_SDK_TYPES must be assigned exactly once in providers.py.", file=sys.stderr)
        sys.exit(1)
    try:
        py_providers = ast.literal_eval(assignments[0].value)
    except (ValueError, TypeError, SyntaxError, RecursionError, MemoryError) as e:
        print(f"ERROR: PROVIDER_SDK_TYPES must be a pure dict literal (no dict(), unpacking, dynamic calls, or inline comments). Parse error: {e}", file=sys.stderr)
        sys.exit(1)

    # C2 fix: Whitespace-agnostic regex with optional trailing comma for rustfmt compatibility
    # L1 fix: Scope extraction ONLY to PROVIDER_SDK_TYPES constant block to prevent
    # test/doc false positives. Unscoped regex would match tuples in unit tests, comments.
    rust_registry = {}
    with open(BASE_DIR / "providers.rs") as f:
        content = f.read()
    import re
    const_match = re.search(
        r'const\s+PROVIDER_SDK_TYPES\s*:\s*&\[.*?\]\s*=\s*&\[(.*?)\];',
        content,
        re.DOTALL
    )
    if not const_match:
        print("ERROR: PROVIDER_SDK_TYPES constant block not found in providers.rs", file=sys.stderr)
        sys.exit(1)
    rust_block = const_match.group(1)
    for match in re.finditer(r'\(\s*"([\w.-]+)"\s*,\s*"([\w.-]+)"\s*,?\s*\)', rust_block):
        rust_registry[match.group(1)] = match.group(2)

    # Assert parity
    all_keys = set(expected.get("providers", {}).keys()) | set(py_providers.keys()) | set(rust_registry.keys())
    for key in all_keys:
        if key == "default":
            continue
        yaml_val = expected.get("providers", {}).get(key)
        py_val = py_providers.get(key)
        rust_val = rust_registry.get(key)
        if yaml_val != py_val or yaml_val != rust_val:
            print(f"Registry drift: {key} — yaml={yaml_val}, py={py_val}, rust={rust_val}", file=sys.stderr)
            sys.exit(1)

    print("Registry parity OK")
    sys.exit(0)

if __name__ == "__main__":
    main()
```

Run in CI: `python3 ci/validate_registry_parity.py`. No git metadata required — works in shallow clones and detached HEAD states.

**Runtime injection (optional):**
- At proxy startup, load `providers_sdk_types.yaml` and inject via PyO3 config
- Python registry imported by Rust via `pyo3::embedded_constants` or config file

If a provider is added to Python but omitted from Rust build, CI will catch the drift via generated-file comparison.

```python
# Provider SDK types — dispatch to correct bridge
PROVIDER_SDK_TYPES = {
    "openai": "async",        # AsyncOpenAI
    "anthropic": "async",    # AsyncAnthropic
    "mistral": "async",       # AsyncMistral
    "gemini": "async",        # N18 fix: google-generativeai (async)
    "ollama": "sync",         # requests-based sync (no official async SDK)
    "deepinfra": "async",     # AsyncDeepInfra
    "groq": "async",          # N18 fix: groq also has async SDK
    "bedrock": "sync",        # N18 fix: AWS boto3/botocore (sync via spawn_blocking)
    # Default: sync is safer — blocks don't starve the async executor
    "default": "sync",
}
```

```rust
// Rust proxy dispatch based on provider SDK type
// NOTE: Python PROVIDER_SDK_TYPES default="sync" — Rust fallback MUST match
async fn handle_completion(req: Request, provider: &str) -> Result<Response, HyperError> {
    let sdk_type = PROVIDER_SDK_TYPES.get(provider).unwrap_or(&"sync");

    match sdk_type {
        "sync" => {
            // spawn_blocking bridge for sync SDKs
            let py_result = tokio::task::spawn_blocking(|| {
                Python::with_gil(|py| call_python_sdk_sync(py, &req))
            }).await?;
            Ok(Response::new(Body::from(py_result)))
        }
        "async" => {
            // N5 fix: pyo3_asyncio::tokio::into_future (not into_async — deprecated/removed)
            let py_result = pyo3_asyncio::tokio::into_future(
                call_python_sdk_async(req)
            ).await?;
            Ok(Response::new(Body::from(py_result)))
        }
    }
}
```

**Dependency:** `pyo3-asyncio` is REQUIRED for `any-llm-mode` proxy builds. Compatible with:
- Python 3.10+ (required for `contextvars` event loop policy)
- Tokio 1.x (use `tokio::task::spawn_local` for local task dispatch)
- Fallback: If `pyo3-asyncio` initialization fails, proxy falls back to `spawn_blocking` for all calls (degraded performance but no crash)

**Operational visibility for pyo3-asyncio fallback:**
- **WARN logging**: When falling back to `spawn_blocking` (pyo3-asyncio init failed), log at WARN level: `"pyo3-asyncio init failed, falling back to spawn_blocking for provider={provider}"`
- **/health endpoint**: Exposes `pyo3_asyncio_available: bool` flag (true = async bridge active, false = fallback mode)
- **Metrics**: `quota_router_pyo3_async_bridge_fallback_total` counter (labels: `provider`) increments each time a fallback occurs
  - **Export format**: Prometheus `/metrics` endpoint (default); OpenTelemetry OTLP (optional)
  - **Label schema**: `provider="openai"`, `bridge="spawn_blocking"`
- **Router lock hold time**: `router_lock_hold_time_us` histogram for routing lock contention monitoring
  - **Type**: Histogram
  - **Labels**: `strategy="cost-based-routing" | "usage-based-routing"`
  - **Buckets**: [10, 25, 50, 75, 100, 250, 500]
  - **Collection**: `time.monotonic_ns()` diff around `with self._state_lock:` block
  - **Sampling**: Configurable sampling rate (default: 10%). At >10k RPS, reduce to 1% or disable.
  - **Note**: Metric collection adds ~1μs overhead. Do not run at 100% sampling in latency-critical deployments.

**Key invariant:** PyO3 provider calls from the proxy ALWAYS go through the correct bridge based on `PROVIDER_SDK_TYPES`. The proxy never guesses — dispatch is explicit.

**For `acompletion(stream=True)`:**
- In `litellm-mode`: Rust async stream → Python async iterator via PyO3 async support
- In `any-llm-mode`: Python async SDK stream → returned directly as `AsyncIterator[ChatCompletionChunk]`
- In `full` mode: Uses whichever mode is active via `QUOTA_ROUTER_MODE`

Note: `async_iter_to_sync_iter()` bridge is used for **sync** `completion(stream=True)` only, NOT for async `acompletion(stream=True)`.

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
        """OpenAI SSE: pass-through (already normalized by OpenAI Python SDK).

        The OpenAI Python SDK yields ChatCompletionChunk objects directly — no
        transformation needed. This method handles the case where raw SSE bytes
        are received (e.g., from a custom HTTP client in litellm-mode).

        SSE format: ``data: {"id":"...","choices":[{"delta":{"content":"..."}}]}\n\n``
        """
        line = chunk.decode("utf-8").strip()
        if not line.startswith("data: "):
            return None
        data = line[6:]  # Strip "data: "
        if data == "[DONE]":
            return None
        import json
        try:
            obj = json.loads(data)
        except json.JSONDecodeError:
            return None
        # Already OpenAI format — construct ChatCompletionChunk from dict
        return ChatCompletionChunk(
            id=obj.get("id", ""),
            choices=[ChoiceDelta(
                index=0,
                delta={"content": obj.get("choices", [{}])[0].get("delta", {}).get("content", "")},
                finish_reason=obj.get("choices", [{}])[0].get("finish_reason"),
            )]
        )

    @staticmethod
    def parse_anthropic_sse(chunk: bytes) -> Optional[ChatCompletionChunk]:
        """Anthropic event-stream: transform to OpenAI SSE format.

        Anthropic SSE uses event-type lines:
        ``event: message_delta\ndata: {"usage":{"output_tokens":123},"delta":{"text":"..."}}\n\n``

        NOTE: This is a stateless single-function parser. Since event type and data
        arrive in SEPARATE SSE lines, this function cannot produce output on the event-type
        line (no data) and cannot produce output on the data line (no remembered event type).

        N7 fix: This method now delegates to a stateful parser class that maintains
        event-type context across calls. The static method signature is preserved for
        API compatibility; an internal class handles multi-line SSE state.
        """
        return _AnthropicSSEParser.parse(chunk)


class _AnthropicSSEParser:
    """Stateful Anthropic SSE parser — maintains event-type context across calls.

    Anthropic SSE events span multiple lines:
        event: content_block_delta
        data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}

    The event type and data fields must be remembered together to produce
    a valid ChatCompletionChunk. This class tracks pending event-type state.
    """

    _pending_event_type: Optional[str] = None  # class-level state across calls

    @classmethod
    def parse(cls, chunk: bytes) -> Optional[ChatCompletionChunk]:
        import json
        line = chunk.decode("utf-8").strip()
        # Blank line — potential SSE event boundary (ignore)
        if not line:
            return None
        if line.startswith("event: "):
            # Extract event type and STORE for the next data line
            # There may be multiple event: lines before data: — keep updating
            cls._pending_event_type = line.split("\n")[0][7:]  # e.g., "message_delta"
            return None  # Event type lines have no content to parse yet
        if not line.startswith("data: "):
            return None
        data = line[6:]
        if data == "[DONE]":
            cls._pending_event_type = None  # Reset state after terminal event
            return None
        try:
            obj = json.loads(data)
        except json.JSONDecodeError:
            cls._pending_event_type = None
            return None
        # Use the STORED event type from the preceding event: line
        event_type = cls._pending_event_type
        cls._pending_event_type = None  # Consume after use (per SSE spec)
        if event_type is None:
            # data: line without preceding event: type — cannot parse
            return None
        # Handle content_block_delta events (the main streaming event type)
        if event_type == "content_block_delta":
            delta = obj.get("delta", {})
            text = delta.get("text", "")
            if not text:
                return None
            return ChatCompletionChunk(
                id=f"chatcmpl-{obj.get('message', {}).get('id', '')}",
                choices=[ChoiceDelta(
                    index=obj.get("index", 0),
                    delta={"content": text},
                    finish_reason=None,
                )]
            )
        # Handle message_delta — final chunk with usage
        if event_type == "message_delta":
            delta = obj.get("delta", {})
            text = delta.get("text", "")
            return ChatCompletionChunk(
                id=f"chatcmpl-{obj.get('message', {}).get('id', '')}",
                choices=[ChoiceDelta(
                    index=0,
                    delta={"content": text},
                    finish_reason="stop",
                )]
            )
        # Handle message_stop — terminal event
        if event_type == "message_stop":
            return None  # Already handled final chunk via message_delta
        return None

    @staticmethod
    def parse_mistral_sse(chunk: bytes) -> Optional[ChatCompletionChunk]:
        """Mistral: OpenAI SSE pass-through.

        Mistral API uses OpenAI-compatible SSE format. The Mistral Python SDK
        parses SSE internally and yields typed objects. This handler is for the
        case where raw SSE bytes are received from a custom HTTP client.

        SSE format: ``data: {"id":"...","choices":[{"delta":{"content":"..."}}]}\n\n``
        """
        # Mistral uses OpenAI-compatible SSE — same parsing as OpenAI
        return SSEParser.parse_openai_sse(chunk)

    @staticmethod
    def parse_ollama_sse(chunk: bytes) -> Optional[ChatCompletionChunk]:
        """Ollama: custom JSON-lines format, transform to OpenAI SSE.

        Ollama SSE format (JSON lines, not SSE-formatted):
        ``{"model":"llama3","done":false,"message":{"role":"assistant","content":"..."}}\n``

        Transforms to OpenAI format: ``{"choices":[{"delta":{"content":"..."}}]}``
        """
        import json
        line = chunk.decode("utf-8").strip()
        if not line:
            return None
        try:
            obj = json.loads(line)
        except json.JSONDecodeError:
            return None
        done = obj.get("done", False)
        message = obj.get("message", {})
        content = message.get("content", "")
        if not content and not done:
            return None
        model = obj.get("model", "")
        return ChatCompletionChunk(
            id=f"chatcmpl-{model}",
            choices=[ChoiceDelta(
                index=0,
                delta={"content": content},
                finish_reason=done and "stop" or None,
            )]
        )

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
            # N6 fix: Filter None — parse_openai_sse returns None for non-content SSE events
            parsed = SSEParser.parse_openai_sse(chunk)
            if parsed is not None:
                yield parsed
    elif provider == "anthropic":
        async for event in anthropic_sdk.messages.stream(model=model, messages=messages, **kwargs):
            # N6 fix: Filter None — parse_anthropic_sse returns None for non-content SSE events
            parsed = SSEParser.parse_anthropic_sse(event)
            if parsed is not None:
                yield parsed
    # ... other providers

def _stream_sync_bridge(
    provider: str,
    model: str,
    messages: List[Dict],
    **kwargs,
) -> Iterator[ChatCompletionChunk]:
    """
    Bridge async streaming to sync iterator using async_iter_to_sync_iter().
    Must NOT be async def — this is a sync generator that wraps an async iterator.
    """
    async_iter = _stream_provider_response(provider, model, messages, **kwargs)
    # async_iter_to_sync_iter() handles the conversion
    return async_iter_to_sync_iter(async_iter)
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
    # Anthropic thinking param — REQUIRED for drop-in replacement (CRITICAL)
    thinking: Optional[Dict] = None,
    # Multi-modal output — REQUIRED for drop-in replacement (HIGH)
    modalities: Optional[List[str]] = None,  # e.g., ["text", "audio"]
    audio: Optional[Dict] = None,  # Audio output params
    prediction: Optional[Dict] = None,  # Predicted outputs (content prediction optimization)
    **kwargs,  # All other params (temperature, max_tokens, etc.) passed to provider
) -> Union[CompletionResponse, AsyncIterator[ChatCompletionChunk]]:
    """
    When stream=True, returns an AsyncIterator[ChatCompletionChunk].
    The caller uses `async for chunk in result:` to consume chunks.

    Reference: LiteLLM's CustomStreamWrapper.__aiter__/__anext__
    (litellm/litellm_core_utils/streaming_handler.py `stream_chunk_handler` method)

    Example:
        result = await acompletion(model="gpt-4o", messages=[...], stream=True)
        # result is AsyncIterator[ChatCompletionChunk]
        async for chunk in result:
            print(chunk.delta.content, end="")

    Note: All completion params (temperature, max_tokens, top_p, etc.) are passed
    through via **kwargs to the provider SDK, matching the full acompletion() signature.
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
        """Yield chunks from the provider's async stream.

        Note: This is a coroutine (async def with return), NOT an async generator.
        __anext__ returns a single ChatCompletionChunk per call, advancing the
        stored stream iterator one step at a time.
        """
        if self._stream is None:
            self._stream = self._create_stream_iter()
        try:
            raw = await self._stream.__anext__()
            return self._transform_chunk(raw)
        except StopAsyncIteration:
            raise

    def _create_stream_iter(self) -> "ChatCompletionStreamIterator":
        """Create and return an async iterator over chunks.

        Returns a ChatCompletionStreamIterator instance.
        Phase 2 implementation will add provider-specific stream creation here.
        """
        return ChatCompletionStreamIterator(self.provider, self.model, self.messages, self.kwargs)

    def _transform_chunk(self, chunk) -> ChatCompletionChunk:
        """Provider-specific chunk normalization."""
        # Provider-specific SSE parsing happens here
        pass


class ChatCompletionStreamIterator:
    """Async iterator that wraps provider SDK stream calls.

    Stores the stream persistently so __anext__ can advance it across calls.
    Phase 2 implementation creates real provider streams.
    """

    def __init__(self, provider: str, model: str, messages: List[Dict], kwargs: dict):
        self.provider = provider
        self.model = model
        self.messages = messages
        self.kwargs = kwargs
        self._stream = None  # Lazily initialized stream

    def __aiter__(self) -> "ChatCompletionStreamIterator":
        return self

    async def __anext__(self) -> ChatCompletionChunk:
        """Advance the provider's stream one step.

        Lazily initializes stream on first call, then advances it.
        """
        if self._stream is None:
            self._stream = await self._create_stream()
        try:
            return await self._stream.__anext__()
        except StopAsyncIteration:
            raise StopAsyncIteration

    async def _create_stream(self) -> AsyncIterator:
        """Create and return the provider's async stream.

        Phase 2 (fully spec-ed): Creates provider-specific async streams using
        official Python SDKs. Each provider's SDK returns async iterators of
        typed objects (not raw SSE bytes).
        """
        if self.provider == "openai":
            from openai import AsyncOpenAI
            client = AsyncOpenAI()
            stream = await client.chat.completions.create(
                model=self.model,
                messages=self.messages,
                stream=True,
                **self.kwargs
            )
            return stream  # AsyncIterator[ChatCompletionChunk] — already normalized
        elif self.provider == "anthropic":
            from anthropic import AsyncAnthropic
            client = AsyncAnthropic()
            stream = await client.messages.stream(
                model=self.model,
                messages=self.messages,
                **self.kwargs
            )
            return stream  # AsyncIterator[MessageDeltaEvent] — needs transform
        elif self.provider == "mistral":
            from mistralai import AsyncMistral
            client = AsyncMistral()
            stream = await client.chat.stream(
                model=self.model,
                messages=self.messages,
                **self.kwargs
            )
            return stream  # AsyncIterator — Mistral SDK yields typed objects
        elif self.provider == "ollama":
            import ollama
            # ollama Python SDK uses sync client — use run_in_executor to avoid blocking
            # the asyncio event loop. Do NOT use a sync for-loop inside async def.
            async def ollama_stream():
                loop = asyncio.get_event_loop()
                client = ollama.AsyncClient()
                # Call the async SDK method — not ollama.async_stream (doesn't exist)
                stream = await client.chat(model=self.model, messages=self.messages, stream=True, **self.kwargs)
                async for chunk in stream:
                    yield chunk
            return ollama_stream()
        else:
            raise StopAsyncIteration
```

**Note on SSE parsing:** Phase 1 uses `async_iter_to_sync_iter()` bridge for **sync** streaming with sync providers. The bridge is available in Phase 1 — `stream=True` does NOT raise `NotImplementedError`. However, Phase 1 returns **raw provider-native chunks** via the bridge (no SSE normalization).

**⚠️ Phase 1 Streaming Behavior:**
- `stream=True`: Available via `async_iter_to_sync_iter()` bridge. Returns **raw provider-native chunks** (Anthropic event-stream, Mistral SSE, etc.) — NOT OpenAI SSE.
- `raw_stream=True`: Phase 1 ignored (same as stream=True); Phase 3 marker for forcing raw chunks.
- SSE normalization (F3): Phase 3 item — transforms provider-native chunks to OpenAI SSE format.

**Phase 3 SSE Normalization Pipeline (raw_stream hook):**
```python
# Phase 3 SSE normalization pipeline
async def _stream_with_normalization(provider: str, raw_stream: bool, provider_chunks):
    if raw_stream:
        # raw_stream=True: bypass normalization, yield raw provider chunks
        async for chunk in provider_chunks:
            yield chunk
    else:
        # stream=True: normalize provider chunks to OpenAI SSE format
        async for chunk in provider_chunks:
            normalized = normalize_to_openai_sse(provider, chunk)
            yield normalized

def normalize_to_openai_sse(provider: str, raw_chunk: Any) -> ChatCompletionChunk:
    """Transform provider-native streaming chunk to OpenAI-compatible ChatCompletionChunk.

    Phase 3 fully spec-ed — provider-specific transformation logic:

    | Provider   | Input Type                          | Transformation                             |
    |-----------|-------------------------------------|--------------------------------------------|
    | openai    | ChatCompletionChunk                 | Pass-through (already OpenAI format)        |
    | anthropic | MessageDeltaEvent, ContentBlockDeltaEvent | Map .delta.text → delta.content         |
    | mistral   | ChatCompletionDelta (typed or dict) | Map .delta.content → delta.content         |
    | ollama    | ChatResponse (typed) or dict        | Map .message.content → delta.content        |
    """
    if provider == "openai":
        # Already OpenAI format — return as-is if already ChatCompletionChunk
        if isinstance(raw_chunk, ChatCompletionChunk):
            return raw_chunk
        # If dict (from raw SSE), construct ChatCompletionChunk
        return _dict_to_chat_completion_chunk(raw_chunk)

    elif provider == "anthropic":
        # Anthropic: MessageDeltaEvent has .delta.text
        delta_text = getattr(raw_chunk, "delta", None)
        if delta_text is None:
            delta_text = raw_chunk.get("delta", {}) if isinstance(raw_chunk, dict) else {}
        content = getattr(delta_text, "text", "") or delta_text.get("text", "")
        chunk_id = getattr(raw_chunk, "message", None) or raw_chunk.get("message", {}) if isinstance(raw_chunk, dict) else {}
        msg_id = getattr(chunk_id, "id", "") or chunk_id.get("id", "")
        return ChatCompletionChunk(
            id=f"chatcmpl-{msg_id}",
            choices=[ChoiceDelta(
                index=0,
                delta={"content": content},
                finish_reason=getattr(raw_chunk, "type", None) == "message_stop" and "stop" or None,
            )]
        )

    elif provider == "mistral":
        # Mistral: ChatCompletionDelta has .delta.content
        if isinstance(raw_chunk, dict):
            delta = raw_chunk.get("delta", {})
            content = delta.get("content", "") if isinstance(delta, dict) else ""
        else:
            delta = getattr(raw_chunk, "delta", None)
            content = getattr(delta, "content", "") if delta else ""
        return ChatCompletionChunk(
            id=getattr(raw_chunk, "id", "") or raw_chunk.get("id", "chatcmpl-mistral"),
            choices=[ChoiceDelta(index=0, delta={"content": content}, finish_reason=None)]
        )

    elif provider == "ollama":
        # Ollama: ChatResponse has .message.content
        if isinstance(raw_chunk, dict):
            message = raw_chunk.get("message", {})
            content = message.get("content", "") if isinstance(message, dict) else ""
            done = raw_chunk.get("done", False)
            model = raw_chunk.get("model", "")
        else:
            message = getattr(raw_chunk, "message", None)
            content = getattr(message, "content", "") if message else ""
            done = getattr(raw_chunk, "done", False)
            model = getattr(raw_chunk, "model", "")
        return ChatCompletionChunk(
            id=f"chatcmpl-{model}",
            choices=[ChoiceDelta(index=0, delta={"content": content}, finish_reason=done and "stop" or None)]
        )

    else:
        raise ValueError(f"normalize_to_openai_sse: unknown provider {provider}")


def _dict_to_chat_completion_chunk(obj: Dict) -> ChatCompletionChunk:
    """Convert a dict (from JSON parsing) to ChatCompletionChunk."""
    choices = obj.get("choices", [])
    first = choices[0] if choices else {}
    delta = first.get("delta", {}) if isinstance(first, dict) else {}
    return ChatCompletionChunk(
        id=obj.get("id", ""),
        choices=[ChoiceDelta(
            index=first.get("index", 0) if isinstance(first, dict) else 0,
            delta=delta,
            finish_reason=first.get("finish_reason") if isinstance(first, dict) else None,
        )]
    )
```

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

from concurrent.futures import ThreadPoolExecutor, as_completed, wait, FIRST_COMPLETED
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
    cache_bypass: bool = False,  # C1 fix: forward to underlying completion calls
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

        **GIL consideration:** Python SDKs (httpx, openai, anthropic) release the GIL
        during I/O operations (network waits, SSL handshake, socket reads), so
        ThreadPoolExecutor provides effective parallelism for network-bound calls.
        For CPU-heavy post-processing, prefer abatch_completion() with asyncio.
    """
    if not messages:
        return []

    results: List[Optional[CompletionResponse]] = [None] * len(messages)
    errors: List[Optional[Exception]] = [None] * len(messages)

    def submit_one(idx: int, msgs: List[Dict]) -> None:
        try:
            # Call completion (sync) for each message set
            # C1 fix: Use functools.partial for explicit cache_bypass binding.
            # Batch workers MUST receive cache_bypass via explicit argument binding.
            # Implicit closure capture is prohibited for this parameter.
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
                cache_bypass=cache_bypass,
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
    cache_bypass: bool = False,  # C1 fix: forward to underlying acompletion calls
    **kwargs,
) -> List[CompletionResponse]:
    """
    Async version: gather responses concurrently using asyncio.
    Uses asyncio.Semaphore to limit concurrency to max_workers.
    """
    import asyncio

    async def submit_one(semaphore: asyncio.Semaphore, msgs: List[Dict]) -> CompletionResponse:
        async with semaphore:
            return await acompletion(
                model=model,
                messages=msgs,
                provider=provider,
                temperature=temperature,
                max_tokens=max_tokens,
                n=n,
                cache_bypass=cache_bypass,  # C1 fix: explicit forward
                **kwargs,
            )

    semaphore = asyncio.Semaphore(max_workers)
    results = await asyncio.gather(*[submit_one(semaphore, msgs) for msgs in messages], return_exceptions=True)
    # LiteLLM behavior: return all results (successful + exceptions as values), don't raise
    # Failed items appear as None in the returned list; caller can inspect for exceptions
    successful = [r if not isinstance(r, Exception) else None for r in results]
    return successful
```

#### batch_completion_models() — Race Multiple Models (LiteLLM Compatible)

**Severity: Important**

`batch_completion_models()` sends the **same request** to **multiple models concurrently** and returns the **first response** (race condition). Distinct from `batch_completion()` which sends **many messages** to **one model**.

**Reference:** LiteLLM's `batch_completion_models` (`litellm/batch_completion/main.py`).

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
        as soon as any model responds. Other requests run to completion
        (their results are discarded). The ThreadPoolExecutor waits for
        all submitted futures when exiting the `with` block.
        Distinct from batch_completion() which sends many messages to
        one model and returns ALL results.
        N14 fix: "Other requests are cancelled" was incorrect — they run to completion.
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
        # If first model fails, continue waiting for others until success or all fail
        remaining = set(futures.values())
        while remaining:
            done, remaining = wait(remaining, return_when=FIRST_COMPLETED)
            for future in done:
                try:
                    return future.result()
                except Exception:
                    # Model failed — continue waiting for others
                    continue

    raise AllModelsFailedError(
        f"All {len(models)} models failed: {[m for m in models]}"
    )
```

**Full implementation for `batch_completion_models_all_responses()`:**

```python
from typing import List, Dict, Union, Tuple, Optional
from concurrent.futures import ThreadPoolExecutor, wait, ALL_COMPLETED, Future

def batch_completion_models_all_responses(
    *args,
    messages: List[Dict],
    models: Union[str, List[str]],
    **kwargs,
) -> List[Optional[CompletionResponse]]:
    """
    Send a request to multiple models concurrently, return ALL responses.

    Args:
        *args: Variable-length positional args (passed to completion)
        messages: The message list to send to ALL models
        models: Single model name (str) or list of model names
        **kwargs: Passed to completion() for each model

    Returns:
        List[Optional[CompletionResponse]] — ALL responses in model order.
        Failed models have None at their index.
        N20 fix: Return type is List[Optional[CompletionResponse]] — None for failures.

    Note:
        Uses ThreadPoolExecutor.wait(ALL_COMPLETED) — waits for all
        models to respond (or fail). Returns all results.
    """
    if isinstance(models, str):
        models = [models]

    kwargs.pop("model", None)
    kwargs.pop("models", None)

    # H2 fix: Use list of (model, future) tuples instead of dict keyed by model name.
    # Dict keys collide when models contains duplicates (e.g. ["gpt-4o", "gpt-4o"]
    # for A/B comparison). Tuples preserve each submission distinctly.
    futures: List[Tuple[str, Future]] = []
    with ThreadPoolExecutor(max_workers=len(models)) as executor:
        for model in models:
            futures.append((model, executor.submit(
                completion, *args, model=model, messages=messages, **kwargs
            )))
        # Wait for all completions
        done, _ = wait([f for _, f in futures], return_when=ALL_COMPLETED)

    results = []
    for model, future in futures:
        try:
            results.append(future.result())
        except Exception:
            results.append(None)  # Append None for failed models

    return results
```

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

**⚠️ DEPRECATION NOTICE (v1.68 UPDATE): The deprecated Python Router class was fully ERASED in v1.68. The current architecture uses thin PyO3 delegation only — RouterHandle is a thin stub to RustRouterHandle with NO Python-side routing state.**

```
╔═══════════════════════════════════════════════════════════════════════════════════════════╗
║                                                                                           ║
║   ⚠️  DEPRECATION NOTICE (v1.68) — Python Router class REMOVED.                           ║
║                                                                                           ║
║   Phase 1 Python Router with routing state (_total_spend, _spend_history,               ║
║   _state_lock, decay math, metric counters) was ERASED in v1.68 (~700 lines removed).   ║
║                                                                                           ║
║   CURRENT ARCHITECTURE (v1.68+): Thin PyO3 delegation via RouterHandle.                 ║
║   - Python Router class: REMOVED                                                          ║
║   - Routing strategies: owned by Rust core (per RFC-0917)                                ║
║   - Spend tracking with decay: owned by Rust core (per RFC-0917)                        ║
║   - State management: owned by Rust core (per RFC-0917)                                  ║
║   - Python adds ONLY: marshaling overhead (<2ms)                                          ║
║                                                                                           ║
║   See RFC-0920 version history for removal details.                                       ║
║   See RFC-0917 v2.44+ for authoritative heavy-lifting definitions.                      ║
║                                                                                           ║
╚═══════════════════════════════════════════════════════════════════════════════════════════╝
```

**Architecture:**

```
Python Router (DEPRECATED — to be replaced with thin PyO3 stub)
  └── Calls RustRouterHandle (PyO3) — NO Python-side routing state
        └── Rust core owns: routing strategies, spend tracking, latency metrics,
            decay math, lock-free atomics, batch executor, fallback coordination

Rust RouterHandle (quota-router-core)
  └── Used by Python SDK for all routing decisions
  └── Exposed via PyO3 as thin handle (<2ms marshaling overhead)
  └── All heavy lifting = routing, state, caching, telemetry in Rust core
```

#### RouterHandle / RustRouterHandle — Concrete Definition

**Location:** `quota-router-core/src/python_sdk_entry.rs` (per RFC-0917 v2.38)

**Purpose:** PyO3-exposed entry point for `quota-router-pyo3`. All routing, state, caching, and telemetry are owned by Rust core. Python SDK adds only marshaling overhead (<2ms).

**Construction:** Created via `RouterHandle::new(router, storage)` — router is typically `Router::global()` or a shared `Arc<Router>`. Storage is a persisted `Arc<dyn KeyStorage>` (e.g., `StoolapKeyStorage`).

**Structure:**
```rust
// quota-router-core/src/python_sdk_entry.rs

pub struct RouterHandle {
    router: Arc<Router>,
    storage: Arc<dyn KeyStorage>,
}

impl RouterHandle {
    /// Create new RouterHandle
    pub fn new(router: Arc<Router>, storage: Arc<dyn KeyStorage>) -> Self

    /// Single completion call — routes, applies fallback, records spend
    /// Full 22-parameter signature per RFC-0920 Completion API and LiteLLM compatibility sections
    pub async fn completion(
        &self,
        model: &str,
        messages: Vec<SdkMessage>,
        stream: bool,
        timeout: Option<f64>,
        // LiteLLM compatibility params
        temperature: Option<f64>,
        max_tokens: Option<i32>,
        top_p: Option<f64>,
        n: Option<i32>,
        stop: Option<String>,
        presence_penalty: Option<f64>,
        frequency_penalty: Option<f64>,
        user: Option<String>,
        seed: Option<i32>,
        extra_headers: Option<String>,
        base_url: Option<String>,
        api_version: Option<String>,
        api_key: Option<String>,
        // Phase 4 params (Extended API Surface)
        service_tier: Option<String>,
        background: Option<bool>,
        prompt_cache_key: Option<String>,
        prompt_cache_retention: Option<String>,
        conversation: Option<String>,
    ) -> Result<SdkCompletionResponse, RouterError> { ... }

    /// Batch completion — same model, parallel requests (per RFC-0920 Batch Completion section)
    pub async fn batch_completion(
        &self,
        requests: Vec<SdkCompletionRequest>,
    ) -> Result<Vec<SdkCompletionResponse>, RouterError> { ... }

    /// Batch completion with model race — first response wins (per RFC-0920 Batch Completion section)
    pub async fn batch_completion_models(
        &self,
        models: Vec<String>,
        messages: Vec<SdkMessage>,
        // All 22 params
        temperature: Option<f64>,
        max_tokens: Option<i32>,
        top_p: Option<f64>,
        n: Option<i32>,
        stream: bool,
        stop: Option<String>,
        presence_penalty: Option<f64>,
        frequency_penalty: Option<f64>,
        user: Option<String>,
        seed: Option<i32>,
        timeout: Option<f64>,
        extra_headers: Option<String>,
        base_url: Option<String>,
        api_version: Option<String>,
        api_key: Option<String>,
        service_tier: Option<String>,
        background: Option<bool>,
        prompt_cache_key: Option<String>,
        prompt_cache_retention: Option<String>,
        conversation: Option<String>,
    ) -> Result<SdkCompletionResponse, RouterError> { ... }

    /// Batch completion with all responses — all models, all responses (per RFC-0920 Batch Completion section)
    pub async fn batch_completion_models_all(
        &self,
        models: Vec<String>,
        messages: Vec<SdkMessage>,
        // All 22 params
        temperature: Option<f64>,
        max_tokens: Option<i32>,
        top_p: Option<f64>,
        n: Option<i32>,
        stream: bool,
        stop: Option<String>,
        presence_penalty: Option<f64>,
        frequency_penalty: Option<f64>,
        user: Option<String>,
        seed: Option<i32>,
        timeout: Option<f64>,
        extra_headers: Option<String>,
        base_url: Option<String>,
        api_version: Option<String>,
        api_key: Option<String>,
        service_tier: Option<String>,
        background: Option<bool>,
        prompt_cache_key: Option<String>,
        prompt_cache_retention: Option<String>,
        conversation: Option<String>,
    ) -> Result<Vec<SdkCompletionResponse>, RouterError> { ... }

    /// Get metrics — forward to Rust Prometheus/OTLP
    pub fn get_metrics(&self) -> Result<PrometheusDict, RouterError> { ... }
}
```

**Python binding (quota-router-pyo3):**
```python
class Router:
    def __init__(self, model_list, routing_strategy="simple-shuffle", **kwargs):
        self._rust = RustRouterHandle(model_list=model_list, strategy=routing_strategy, **kwargs)

    def completion(self, model, messages, **kwargs):
        return self._rust.completion(model=model, messages=messages, **kwargs)

    def batch_completion_models(self, models, messages, **kwargs):
        return self._rust.batch_completion_models(models=models, messages=messages, **kwargs)

    def batch_completion_models_all(self, models, messages, **kwargs):
        return self._rust.batch_completion_models_all(models=models, messages=messages, **kwargs)
```

**Note:** `RouterHandle`/`RustRouterHandle` is referenced throughout this RFC but was never concretely defined here. RFC-0917 v2.38 provides the concrete definition in `python_sdk_entry` module with full 22-parameter signature and all three batch methods. The implementation must create `quota-router-core/src/python_sdk_entry.rs` with this `RouterHandle` struct.

#### Direct Rust Client Entry Point

**Location:** `quota-router-core/src/client.rs` (per RFC-0917 v2.38)

External Rust programs (CLI tools, other crates) can call quota-router-core directly without HTTP serialization or PyO3 overhead.

**Structure:**
```rust
// quota-router-core/src/client.rs

/// Direct Rust client entry point — no HTTP, no PyO3
pub struct Client {
    router: Arc<Router>,
    storage: Arc<dyn KeyStorage>,
}

impl Client {
    /// Create new Client
    pub fn new(router: Arc<Router>, storage: Arc<dyn KeyStorage>) -> Self

    /// Single completion call — same 22 params as RouterHandle.completion()
    pub async fn completion(
        &self,
        model: &str,
        messages: Vec<SdkMessage>,
        stream: bool,
        timeout: Option<f64>,
        // ... all 22 params per RFC-0920 Completion API and Extended API Surface sections
    ) -> Result<SdkCompletionResponse, RouterError> { ... }

    /// Batch completion — same model, parallel requests
    pub async fn batch_completion(
        &self,
        requests: Vec<SdkCompletionRequest>,
    ) -> Result<Vec<SdkCompletionResponse>, RouterError> { ... }

    /// Batch completion with model race — first response wins
    pub async fn batch_completion_models(
        &self,
        models: Vec<String>,
        messages: Vec<SdkMessage>,
        // All 22 params available
        temperature: Option<f64>,
        max_tokens: Option<i32>,
        top_p: Option<f64>,
        n: Option<i32>,
        stream: bool,
        stop: Option<String>,
        presence_penalty: Option<f64>,
        frequency_penalty: Option<f64>,
        user: Option<String>,
        seed: Option<i32>,
        timeout: Option<f64>,
        extra_headers: Option<String>,
        base_url: Option<String>,
        api_version: Option<String>,
        api_key: Option<String>,
        service_tier: Option<String>,
        background: Option<bool>,
        prompt_cache_key: Option<String>,
        prompt_cache_retention: Option<String>,
        conversation: Option<String>,
    ) -> Result<SdkCompletionResponse, RouterError> { ... }

    /// Batch completion with all responses — all models, all responses
    pub async fn batch_completion_models_all(
        &self,
        models: Vec<String>,
        messages: Vec<SdkMessage>,
        // All 22 params available
        temperature: Option<f64>,
        max_tokens: Option<i32>,
        top_p: Option<f64>,
        n: Option<i32>,
        stream: bool,
        stop: Option<String>,
        presence_penalty: Option<f64>,
        frequency_penalty: Option<f64>,
        user: Option<String>,
        seed: Option<i32>,
        timeout: Option<f64>,
        extra_headers: Option<String>,
        base_url: Option<String>,
        api_version: Option<String>,
        api_key: Option<String>,
        service_tier: Option<String>,
        background: Option<bool>,
        prompt_cache_key: Option<String>,
        prompt_cache_retention: Option<String>,
        conversation: Option<String>,
    ) -> Result<Vec<SdkCompletionResponse>, RouterError> { ... }

    /// Get metrics
    pub fn get_metrics(&self) -> Result<MetricsDict, RouterError> { ... }
}
```

**Usage:**
```rust
use quota_router_core::client::Client;

let client = Client::new(router, storage);
let response = client.completion(model, messages, stream, timeout, ...).await?;
```

**Three entry points to quota-router-core:**

| Entry Point | Path | Use Case |
|-------------|------|----------|
| `gateway` | HTTP proxy | External services, network access |
| `python_sdk_entry` | PyO3 | Python SDK (quota-router-pyo3) |
| `client` | Direct Rust | CLI tools, embedded Rust programs |

#### Migration Target: Rust ←→ Python State Mapping (Normative)

**Purpose:** Document the authoritative mapping between Python Router state fields and their RFC-0917/Rust-core equivalents. All Python-side state is **ephemeral** (in-memory, lost on restart); Rust-core state persists via stoolap.

| Python Router field (Phase 1) | Type | RFC-0917 Rust equivalent | Persisted? | Notes |
|-------------------------------|------|--------------------------|-------------|-------|
| `_round_robin_index` | `int` | `AtomicUsize` in `RouterState` — `fetch_add` for lock-free round-robin | No | Lock-free via atomic; no `threading.Lock` needed |
| `_round_robin_lock` | `threading.Lock` | **Not needed** — RFC-0917 uses `AtomicUsize.fetch_add()` (no lock) | — | Lock-free eliminates TOCTOU race |
| `_total_spend[idx]` | `int` (μunits) | `record_spend()` → `STORAGE.record_spend(&event)` → `octo_w_ledger` per RFC-0909 | **Yes** (stoolap) | Ephemeral in Python; persisted in Rust |
| `_spend_history[idx]` | `deque(maxlen=500)` | `record_spend_with_idempotency()` → `SpendEvent` ledger per RFC-0909 | **Yes** (stoolap) | Persists full history in Rust; Python had in-memory only |
| `_active_requests[idx]` | `int` | `RouterState.connection_pools` — tracks active requests per deployment | No | In-memory state for `least-busy` strategy |
| `_latencies[idx]` | `deque(maxlen=100)` | `LatencyTracker.samples: HashMap<String, Vec<u64>>` — microsecond precision | No | Fixed 100-sample window; `best_provider()` for latency-based routing |
| `_by_model` | `Dict[str, List[int]]` | `Router.provider_impls: HashMap<String, Vec<ProviderWithState>>` | No | Model → deployment indices mapping |

**Critical notes:**
- `_state_lock` (Python `threading.Lock`) has **no equivalent in RFC-0917** because Rust core uses lock-free atomics (`AtomicUsize`) or interior mutability (`RwLock<RouterState>`). Python locks are not needed in the Rust delegation layer.
- All ephemeral Python state (latencies, active requests) is **in-memory only** in Rust core too — process restart resets it identically.
- All persisted Python state (`_total_spend`, `_spend_history`) maps to **stoolap persistence** via `record_spend()` — survives process restart.

**Phase 2 delegation pattern:**

```python
# Phase 2: All state in Rust core — Python is thin marshal layer
class Router:
    def __init__(self, model_list, routing_strategy="simple-shuffle", **kwargs):
        self._rust = RustRouterHandle(model_list=model_list, strategy=routing_strategy, **kwargs)

    def _select_deployment(self, model: str) -> int:
        # Thin delegation — returns deployment index from Rust core
        return self._rust.select_deployment(model)

    def _record_request_end(self, idx: int, latency_ms: float, prompt_tokens: int, completion_tokens: int):
        # Thin delegation — Rust core updates latency tracker, records spend
        self._rust.record_request_end(idx, latency_ms, prompt_tokens, completion_tokens)

    def get_metrics(self) -> Dict:
        return self._rust.get_metrics()  # Rust Prometheus/OTLP metrics
```

**Target (Phase 2):**

```python
# Phase 2: Thin PyO3 wrapper — no Python-side routing state
class Router:
    """
    Thin PyO3 binding layer. All routing, state, caching, and telemetry
    are owned by Rust core (RustRouterHandle).

    Phase 1 (current) has Python-side routing state for iterative development.
    Phase 2 replaces this with RustRouterHandle delegation.
    """
    def __init__(self, model_list: List[Dict], routing_strategy: str = "simple-shuffle", **kwargs):
        # C1 fix: Delegate ALL routing, state, and telemetry to Rust core
        self._rust_router = RustRouterHandle(model_list=model_list, strategy=routing_strategy, **kwargs)

    def completion(self, model: str, messages: List[Dict], cache_bypass: bool = False, **kwargs):
        # Thin delegation — Python only handles API surface & type conversion
        return self._rust_router.completion(model=model, messages=messages, cache_bypass=cache_bypass, **kwargs)

    def batch_completion(self, models: List[str], messages: List[List[Dict]], cache_bypass: bool = False, **kwargs):
        # H2 fix: Delegate batch execution to Rust core parallel executor
        return self._rust_router.batch_completion(models=models, messages=messages, cache_bypass=cache_bypass, **kwargs)

    def get_metrics(self) -> Dict:
        """Forward metric query to Rust core telemetry module."""
        return self._rust_router.get_metrics()
```

**Phase 1–2 Reference:**
All routing state and strategy implementations are defined in RFC-0917 (Router struct, ProviderWithState, RoutingStrategy enum, all 8 strategy algorithms, and SpendTracker with decay math).

**Note on `cache_responses`:** Uses **stoolap** (RFC-0913) cache — NOT Redis. Stoolap is the sole persistence layer per RFC-0914. No `redis_url` parameter.

**Caching Implementation Stages:**

| Phase | Cache Type | Mechanism |
|-------|-----------|-----------|
| Phase 1-2 | **Exact-match KV cache** | SHA256(request_hash) → cached response. No embedding model required. |
| Phase 3 (RFC-0913) | **Semantic cache** | Embedding model vectorizes prompts; similarity threshold determines cache hit. Requires `semantic_cache_model` parameter. |

**Phase 1-2 Implementation (exact-match KV):**

**Request hash computation (canonical JSON):**
```python
import hashlib
import json

def compute_request_hash(provider: str, model: str, messages: List[Dict], cache_bypass: bool = False) -> Optional[str]:
    """
    Compute SHA256 hash for exact-match KV cache lookup.
    Uses canonical JSON serialization (sort_keys, consistent separators)
    to ensure deterministic hash across different Python dict orderings.
    Returns None if payload exceeds size heuristic (large prompt bypass to preserve latency).
    Raises InvalidRequestError on nested NaN/Inf or non-serializable types.

    **cache_bypass:** If True, skips all validation and serialization entirely.
    **L1 fix — Execution order:** `compute_request_hash()` MUST be invoked BEFORE routing
    selection and budget validation to ensure fast-fail on malformed payloads.
    Execution order: validate params → compute_request_hash → route → budget check.

    **L2 fix — Call-site enforcement:** The following execution order MUST be maintained
    in `completion()` and `acompletion()` implementations. Reordering violates fast-fail
    guarantees and may cause quota leaks on malformed payloads:
```python
def completion(model: str, messages: List[Dict], cache_bypass: bool = False, **kwargs):
    # H1 fix: Skip validation when cache_bypass=True (caller accepts risk)
    if not cache_bypass:
        _validate_no_nan_inf(kwargs)                          # 1. Validate params
    cache_hash = compute_request_hash(provider, model, messages, cache_bypass)  # 2. Wired
    if cache_hash:
        cached = cache_lookup(cache_hash)                     # 3. Cache check
        if cached:
            return cached
    deployment = router.select(...)                          # 4. Route AFTER hash computation
    budget.check(deployment, ...)                             # 5. Budget check
    # ... provider call ...
```
"""
    canonical = {
        "provider": provider,
        "model": model,
        "messages": messages,
    }
    # Canonical JSON: sorted keys, consistent separators, NO ASCII escaping
    # allow_nan=False ensures NaN/Inf raise ValueError (matches Rust serde_json)
    # ValueError (from nested NaN/Inf) and TypeError (from non-serializable objects)
    # are both converted to InvalidRequestError to prevent unhandled crashes
    try:
        serialized = json.dumps(canonical, sort_keys=True, separators=(",", ":"), ensure_ascii=False, allow_nan=False)
    except (ValueError, TypeError, OverflowError) as e:
        raise InvalidRequestError(f"Invalid or non-serializable data in request payload: {e}")
    return hashlib.sha256(serialized.encode("utf-8")).hexdigest()
```

**Note:** The canonical JSON serialization (`sort_keys=True, separators=(",", ":")`, `ensure_ascii=False`, `allow_nan=False`) ensures that:
- `{"a": 1, "b": 2}` and `{"b": 2, "a": 1}` produce the same hash (sorted keys)
- Non-ASCII characters (e.g., Unicode) are NOT escaped (matches Rust default)
- NaN/Infinity cause ValueError — SDK entry point validates and rejects with InvalidRequestError

**⚠️ Cross-language serialization consistency:** Both Python and Rust MUST use identical canonical JSON rules:

| Parameter | Python (`json.dumps`) | Rust (`serde_json`) |
|-----------|----------------------|---------------------|
| `sort_keys` | `True` | `true` |
| `separators` | `(",", ":")` | explicit `","` and `":"` |
| `ensure_ascii` | `False` | `false` (default) |
| `allow_nan` | `False` | `false` (required) |
| float handling | raise ValueError on NaN/Inf | raise Error on NaN/Inf |

CI MUST validate that SDK (Python) and Proxy (Rust) produce identical hashes using fuzzed payloads with varied Unicode, float precision, and nested structures.

**H2 fix: Fast O(1) payload heuristic for large prompt cache bypass.**
`json.dumps` serialization on large prompts (50k-100k+ tokens) can take 15-40ms, creating a critical-path bottleneck. The guard must itself be O(1) to preserve G1 <10ms. Exact-match KV cache is optimized for short/medium prompts:

```python
# C2 fix: Sample BOTH ends of conversation history — largest payloads are at the end.
# In LLM chat completions, conversation grows at the END. messages[-1] = latest prompt,
# messages[-2] = assistant context dump. Sampling [:3] misses trailing massive messages.
# O(1) guard: check message count + sample first & last
# C2 fix: Add empty message guard + isinstance(content, str) for multimodal safety.
# Multimodal payloads use content: [{"type": "text", ...}, {"type": "image_url", ...}] —
# calling str() on a list of dicts creates massive strings, triggering false bypasses.
def compute_request_hash(provider: str, model: str, messages: List[Dict], cache_bypass: bool = False) -> Optional[str]:
    # L2 fix: cache_bypass parameter — skip all validation/serialization if True
    if cache_bypass:
        return None  # Bypass cache entirely — skip hashing and lookup
    if not messages:
        return None  # Empty message list — nothing to hash
    if len(messages) > 50:
        return None  # O(1): too many messages
    # Sample both conversation boundaries (system/first + latest prompt).
    # Only check isinstance(content, str) — multimodal lists (list of dicts) skipped.
    # M1 fix: Note on len==1 case — when messages has one element, messages[0] and
    # messages[-1] reference the same item. The duplicate length check is intentional
    # and harmless (O(1) string len comparison). No explicit dedup needed.
    for m in (messages[0], messages[-1]):
        content = m.get("content", "")
        if isinstance(content, str) and len(content) > 10_000:
            return None  # Bypass cache for large content
    # ... hash computation ...
```

**Limitation:** Character count ≠ token count. Non-ASCII Unicode, base64 images, and tool-call JSON inflate chars disproportionately to tokens. The heuristic is a fast approximation — some 10k-token payloads may bypass, some 60k-char ASCII may not. For token-accurate bypass, use explicit `cache_bypass=True` parameter or Phase 3 semantic cache.

**⚠️ For long-context prompts:** Use semantic cache (Phase 3) or pass `cache_bypass=True`. Exact-match KV cache is not designed for >50k token workloads.

```rust
// PyO3 bridge — exact-match KV cache
#[pyfunction]
fn cache_lookup(
    py_request_hash: &str,  // SHA256 hash of (provider, model, messages)
) -> PyResult<Option<PyObject>> {
    // Exact match lookup — no similarity search
    let result = quota_router_core::cache::kv_lookup(py_request_hash)?;
    Ok(result.map(|r| r.into_pyobject(py)))
}

#[pyfunction]
fn cache_insert(
    py_request_hash: &str,
    py_response: &PyObject,
    py_ttl_seconds: u64,
) -> PyResult<()> {
    quota_router_core::cache::kv_insert(py_request_hash, py_response, py_ttl_seconds)?;
    Ok(())
}
```

**Phase 3 Enhancement (semantic cache):**
```python
# Additional parameter for Phase 3
semantic_cache_model: Optional[str] = None,  # e.g., "text-embedding-3-small"
similarity_threshold: float = 0.95,  # Cosine similarity threshold for cache hit
```

Phase 3 semantic cache uses a **separate** lookup path:
- Phase 1-2: `kv_lookup` / `kv_insert` — exact SHA256 hash match (no similarity)
- Phase 3: `semantic_lookup` — embedding vector similarity search

The Python API (`cache_lookup` / `cache_insert`) delegates to the appropriate backend based on whether `semantic_cache_model` is set.

Until the Phase 3 embedding integration is implemented, `cache_responses=True` uses exact-match KV caching. The `semantic_cache_model` parameter is ignored until Phase 3.

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

**Interaction with Router fallback loop:**

The Router's `num_retries` controls the **fallback loop** (trying different deployments). The same `num_retries` is also passed to Rust's `FallbackExecutor` which handles HTTP-level retries on the same deployment. Total HTTP call budget = fallback attempts × (1 + Rust retry count).

**Normative coordination rule:** When the Python Router is initialised with `fallbacks` or `context_window_fallbacks` or `content_policy_fallbacks`, the Rust core's `FallbackExecutor` MUST be configured with `max_retries = 1` (i.e., no internal retries beyond the first attempt). The Router's fallback loop provides the primary resilience; Rust-level retries are disabled to avoid redundant retry attempts.

**Implementation:** Pass `max_retries=1` to Rust core when initializing Router with any fallback configuration. This is NOT a recommendation — it is a REQUIRED specification.

**Example coordination:**
```python
# If Router has 2 fallback targets and FallbackExecutor retry_count=2:
# - Best case: first deployment succeeds in 1 call (1 + 0 Rust retries)
# - Worst case: exhausts all retries across 3 deployments = 6 HTTP calls
```

**PyO3 Parameter Bridge for num_retries:**

The Python `num_retries` parameter is passed through the PyO3 bridge to Rust's `FallbackExecutor`:

```rust
// In PyO3 completion bridge (Rust side)
#[pyfunction]
fn completion(
    py_model: &str,
    py_messages: Vec<PyDict>,
    py_num_retries: Option<u32>,
    // ... other params
) -> PyResult<PyObject> {
    let max_retries = py_num_retries.unwrap_or(3);

    // Pass to Rust core's completion path
    let config = FallbackExecutorConfig {
        max_retries,
        backoff_multiplier: 2.0,
        retry_delay_ms: 500,
        max_backoff_ms: 5000,
    };

    // Call Rust core with config
    // ...
}
```

The `num_retries` Python parameter maps directly to `FallbackExecutorConfig::max_retries` in Rust. When `set_api_key()` is used with `num_retries=None`, the Rust core uses its default (3). When `num_retries=N` is passed, it overrides the Rust default for that specific call.

**Fallback types (from RFC-0902):**

| Type                       | Trigger                     | Description                                   |
| -------------------------- | --------------------------- | --------------------------------------------- |
| `fallbacks`                | All errors                  | Route to next model on failure                |
| `content_policy_fallbacks` | ContentPolicyViolationError | Map to provider with different content policy |
| `context_window_fallbacks` | ContextWindowExceededError  | Map to model with larger context              |

Reference: RFC-0902 §Fallback Mechanisms

**Note on single-target fallbacks:** `context_window_fallbacks` and `content_policy_fallbacks` are single-target — they map a model to exactly one fallback. If the fallback itself suffers from the same error (e.g., context length exceeded), it will be retried repeatedly until `num_retries` is exhausted. For resilience, provide multiple candidates in the generic `fallbacks` list rather than relying on single-target fallbacks for error types that may affect the fallback itself.

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

**PyO3 Bridge Exception Mapping (Rust → Python):**

The Rust core raises structured errors that are translated to Python exceptions via the PyO3 bridge:

| Rust Error Type | Python Exception | Trigger Condition |
|-----------------|-----------------|-------------------|
| `BudgetError::InsufficientBalance` | `InsufficientFundsError` | OCTO-W balance < request cost |
| `BudgetError::KeyBudgetExceeded` | `InsufficientFundsError` | Per-key budget limit exceeded |
| `BudgetError::TeamBudgetExceeded` | `InsufficientFundsError` | Team budget limit exceeded |
| `KeyError::NotFound` | `MissingApiKeyError` | API key not found in storage |
| `KeyError::Revoked` | `AuthenticationError` | API key has been revoked |
| `KeyError::InvalidFormat` | `InvalidRequestError` | Malformed API key format |
| `StorageError::OctoWNotEnabled` | `InvalidRequestError` | OCTO-W not enabled for team |
| `StorageError::Database(_)` | `ProviderError` | Upstream storage/database failure |
| `RateLimitError` | `RateLimitError` | Provider rate limit hit |
| `ContextLengthExceededError` | `ContextLengthExceededError` | Prompt exceeds model context |
| `ContentFilterError` | `ContentFilterError` | Content policy violation |
| `UpstreamProviderError` | `ProviderError` | Generic provider error |

```rust
// PyO3 bridge exception translation
fn map_rust_error_to_python(e: QuotaRouterError) -> PyErr {
    match e {
        QuotaRouterError::Budget(BudgetError::InsufficientBalance { .. }) => {
            PyInsufficientFundsError::new_err(e.to_string())
        }
        QuotaRouterError::Budget(BudgetError::KeyBudgetExceeded { .. }) => {
            PyInsufficientFundsError::new_err(e.to_string())
        }
        QuotaRouterError::Budget(BudgetError::TeamBudgetExceeded { .. }) => {
            PyInsufficientFundsError::new_err(e.to_string())
        }
        QuotaRouterError::Key(KeyError::NotFound) => {
            PyMissingApiKeyError::new_err(e.to_string())
        }
        QuotaRouterError::Key(KeyError::Revoked) => {
            PyAuthenticationError::new_err(e.to_string())
        }
        QuotaRouterError::Key(KeyError::InvalidFormat) => {
            PyInvalidRequestError::new_err(e.to_string())
        }
        QuotaRouterError::Storage(StorageError::OctoWNotEnabled) => {
            PyInvalidRequestError::new_err(e.to_string())
        }
        QuotaRouterError::Storage(StorageError::Database(_)) => {
            PyProviderError::new_err(e.to_string())
        }
        // ... etc
    }
}
```

#### Platform Provider (any-api Key Format)

**Severity: Medium**

any-llm supports `any-...` API keys that encode the provider internally. quota-router supports this via the `platform` pseudo-provider (listed in RFC-0917 Phase 3's 42 providers as `"platform"`).

**Verified consistency with RFC-0917 Phase 3:** The `platform` pseudo-provider matches RFC-0917 Phase 3's provider list (`platform` among 42 providers). It is NOT a different platform integration — it is the same `any-...` key format mechanism.

**Specification:**

```python
# When set_api_key("platform", "any-ant-...") or api_key="any-ant-...":
# Parse the any-... key to extract the actual provider and key

def parse_platform_key(api_key: str) -> tuple[str, str]:
    """
    Parse any-api format key using longest-match provider lookup.

    Examples:
        "any-ant-sk-..." -> ("anthropic", "sk-...")
        "any-openai-sk-..." -> ("openai", "sk-...")
        "any-azureopenai-sk-..." -> ("azureopenai", "sk-...")
        "any-vertexai-sk-..." -> ("vertexai", "sk-...")

    Returns:
        (provider_name, underlying_api_key)

    Raises:
        ValueError: If not a valid any-... key or no provider matches

    Security Note: any- keys bypass quota-router key validation and go directly to
    the provider SDK. Use only in controlled environments. The actual key is validated
    by the provider, not by quota-router.
    """
    if not api_key.startswith("any-"):
        raise ValueError(f"Invalid any-api format: {api_key}")

    remainder = api_key[4:]  # Strip "any-" prefix

    # Longest-match provider lookup: sort by length descending to match
    # "azureopenai" before "azure", "vertexai" before "vertex"
    # This prevents greedy capture bugs with hyphenated provider names
    sorted_providers = sorted(KNOWN_PROVIDERS, key=len, reverse=True)
    for provider in sorted_providers:
        prefix = provider + "-"
        if remainder.startswith(prefix):
            actual_key = remainder[len(prefix):]
            return provider, actual_key

    raise ValueError(
        f"Unknown provider in any- key: '{api_key[:20]}...'. "
        f"Known providers: {', '.join(sorted(KNOWN_PROVIDERS)[:10])}..."
    )

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

**PyO3 Timeout Normalization:**

The Python `timeout` parameter is normalized to Rust `Duration` via the PyO3 bridge:

```rust
// quota-router-pyo3/src/timeout.rs
use std::time::Duration;

fn normalize_timeout(py_timeout: &Bound<PyAny>) -> PyResult<Duration> {
    // Case 1: float/f64 → seconds as f64
    if let Ok(secs) = py_timeout.extract::<f64>() {
        return Ok(Duration::from_secs_f64(secs));
    }

    // Case 2: int → exact seconds
    if let Ok(secs) = py_timeout.extract::<i64>() {
        return Ok(Duration::from_secs(secs as u64));
    }

    // Case 3: str → parse duration string ("30s", "1m", "2h")
    if let Ok(s) = py_timeout.extract::<String>() {
        return parse_duration_string(&s).ok_or_else(|| {
            PyValueError::new_err(format!("Invalid duration string: {}", s))
        });
    }

    // Case 4: httpx.Timeout object → extract .total, .read, or .connect
    if py_timeout.hasattr("connect")? {
        // N19 fix: Precedence is .total > .read > .connect > default 60s
        // .total is the overall request timeout; .read is body-read-only timeout.
        // .total should take priority — users who set .total expect overall budget.
        let total = py_timeout.getattr("total")?;
        if let Some(timeout) = total {
            if let Ok(secs) = timeout.extract::<f64>() {
                return Ok(Duration::from_secs_f64(secs));
            }
        }
        // Fall back to .read (body-read-only)
        let read_timeout = py_timeout.getattr("read")?;
        if let Some(timeout) = read_timeout {
            if let Ok(secs) = timeout.extract::<f64>() {
                return Ok(Duration::from_secs_f64(secs));
            }
        }
        // Fall back to .connect (connection timeout only)
        let connect = py_timeout.getattr("connect")?;
        if let Some(timeout) = connect {
            if let Ok(secs) = timeout.extract::<f64>() {
                return Ok(Duration::from_secs_f64(secs));
            }
        }
        // No timeout values set — use default of 60s
        return Ok(Duration::from_secs(60));
    }

    Err(PyValueError::new_err(format!(
        "timeout must be float, int, str, or httpx.Timeout, got: {}",
        py_timeout.get_type()
    )))
}

fn parse_duration_string(s: &str) -> Option<Duration> {
    let s = s.trim();
    if s.ends_with('s') {
        let n: f64 = s[..s.len()-1].parse().ok()?;
        Some(Duration::from_secs_f64(n))
    } else if s.ends_with('m') {
        let n: u64 = s[..s.len()-1].parse().ok()?;
        Some(Duration::from_secs(n * 60))
    } else if s.ends_with('h') {
        let n: u64 = s[..s.len()-1].parse().ok()?;
        Some(Duration::from_secs(n * 3600))
    } else {
        // Plain number = seconds
        let n: f64 = s.parse().ok()?;
        Some(Duration::from_secs_f64(n))
    }
}
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

### Batch API Signature

The batch API supports **both** LiteLLM style (`input_file_path`) and any-llm style (`input_file_id`):

```python
def batch_create(
    provider: str,
    input_file: Union[str, Path],     # Local file path (LiteLLM style)
    endpoint: str,
    *,
    completion_window: str = "24h",
    metadata: Optional[Dict[str, str]] = None,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    **kwargs
) -> BatchCreateResponse:
    """
    Create a batch job.

    Args:
        provider: Provider name (e.g., "openai")
        input_file: Path to JSONL file with requests, OR pre-existing file ID
                   (if string starts with "file-", treated as file_id; otherwise as path)
        endpoint: API endpoint (e.g., "/v1/chat/completions")
        completion_window: Time window (default: "24h")
        metadata: Optional metadata dict
        api_key: Override API key
        api_base: Override base URL
        client_args: Additional provider-specific arguments

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
    provider: str,
    batch_id: str,
    *,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    **kwargs
) -> BatchRetrieveResponse:
    """Get batch job status and results."""

def batch_list(
    provider: str,
    *,
    after: Optional[str] = None,
    limit: Optional[int] = None,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    **kwargs
) -> Sequence[BatchCreateResponse]:
    """List batch jobs for a provider."""

def batch_cancel(
    provider: str,
    batch_id: str,
    *,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    **kwargs
) -> BatchRetrieveResponse:
    """Cancel a batch job."""

def batch_results(
    provider: str,
    batch_id: str,
    *,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    **kwargs
) -> BatchResult:
    """Retrieve batch results (after completion)."""
```

### Extended API Surface — Additional Functions

This section specifies the complete API surface for drop-in replacement parity.
Missing functions from any-llm and LiteLLM that are REQUIRED for true drop-in compatibility:

#### Text Completion (LiteLLM parity)

```python
async def atext_completion(
    model: str,
    prompt: Union[str, List[str]],
    **kwargs
) -> TextCompletionResponse:
    """
    Asynchronous text completion (non-chat models).

    LiteLLM parity: matches litellm.atext_completion signature.
    """

def text_completion(
    model: str,
    prompt: Union[str, List[str]],
    **kwargs
) -> TextCompletionResponse:
    """
    Synchronous text completion (non-chat models).

    LiteLLM parity: matches litellm.text_completion signature.
    """
```

#### Moderation (LiteLLM parity)

```python
async def amoderation(
    input: Union[str, List[str]],
    model: Optional[str] = None,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
) -> ModerationResponse:
    """
    Asynchronous moderation check.

    LiteLLM parity: matches litellm.amoderation signature.
    """

def moderation(
    input: Union[str, List[str]],
    model: Optional[str] = None,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
) -> ModerationResponse:
    """
    Synchronous moderation check.

    LiteLLM parity: matches litellm.moderation signature.
    """
```

#### Transcription (LiteLLM parity)

```python
async def atranscription(
    model: str,
    file: Union[str, Path, BinaryIO],
    language: Optional[str] = None,
    prompt: Optional[str] = None,
    response_format: Optional[str] = None,
    temperature: Optional[float] = None,
    timestamp_granularities: Optional[List[str]] = None,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
) -> TranscriptionResponse:
    """
    Asynchronous audio transcription.

    LiteLLM parity: matches litellm.atranscription signature.
    """
```

#### Adapter Completion (LiteLLM parity)

```python
async def aadapter_completion(
    model: str,
    adapter_id: str,
    messages: List[Dict],
    **kwargs
) -> CompletionResponse:
    """
    Adapter-based completion (custom model adapters).

    LiteLLM parity: matches litellm.aadapter_completion signature.
    Phase 4 feature — requires adapter registry infrastructure.
    """
```

#### Messages API — Anthropic (any-llm parity)

```python
def messages(
    model: str,
    messages: List[Dict],
    max_tokens: int,
    *,
    provider: Optional[str] = None,
    system: Optional[Union[str, List[Dict]]] = None,
    temperature: Optional[float] = None,
    top_p: Optional[float] = None,
    top_k: Optional[int] = None,
    stream: Optional[bool] = None,
    stop_sequences: Optional[List[str]] = None,
    tools: Optional[List[Dict]] = None,
    tool_choice: Optional[Union[str, Dict]] = None,
    metadata: Optional[Dict[str, Any]] = None,
    thinking: Optional[Dict] = None,
    cache_control: Optional[Dict[str, Any]] = None,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    **kwargs
) -> AnthropicMessagesResponse:
    """
    Synchronous Anthropic Messages API.

    any-llm parity: matches any_llm.messages signature.
    Differs from completion() in that it uses Anthropic's native
    messages endpoint rather than /v1/chat/completions.
    """

async def amessages(
    model: str,
    messages: List[Dict],
    max_tokens: int,
    *,
    provider: Optional[str] = None,
    system: Optional[Union[str, List[Dict]]] = None,
    temperature: Optional[float] = None,
    top_p: Optional[float] = None,
    top_k: Optional[int] = None,
    stream: Optional[bool] = None,
    stop_sequences: Optional[List[str]] = None,
    tools: Optional[List[Dict]] = None,
    tool_choice: Optional[Union[str, Dict]] = None,
    metadata: Optional[Dict[str, Any]] = None,
    thinking: Optional[Dict] = None,
    cache_control: Optional[Dict[str, Any]] = None,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    **kwargs
) -> AnthropicMessagesResponse:
    """
    Asynchronous Anthropic Messages API.

    any-llm parity: matches any_llm.amessages signature.
    """
```

#### Responses API — OpenAI (any-llm parity)

```python
def responses(
    model: str,
    input: Union[str, List[Union[str, Dict]]] = None,      # litellm convention
    input_data: Union[str, List[Union[str, Dict]]] = None,  # any-llm convention
    *,
    provider: Optional[str] = None,
    instructions: Optional[str] = None,
    tools: Optional[List[Dict]] = None,
    tool_choice: Optional[Union[str, Dict]] = None,
    max_output_tokens: Optional[int] = None,
    temperature: Optional[float] = None,
    top_p: Optional[float] = None,
    stream: Optional[bool] = None,
    include: Optional[List[str]] = None,
    parallel_tool_calls: Optional[bool] = None,
    previous_response_id: Optional[str] = None,
    reasoning: Optional[Dict] = None,
    text: Optional[Dict] = None,
    presence_penalty: Optional[float] = None,
    frequency_penalty: Optional[float] = None,
    truncation: Optional[str] = None,
    store: Optional[bool] = None,
    service_tier: Optional[str] = None,
    user: Optional[str] = None,
    metadata: Optional[Dict[str, Any]] = None,
    background: Optional[bool] = None,
    safety_identifier: Optional[str] = None,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    **kwargs
) -> ResponsesAPIResponse:
    """
    Synchronous OpenAI Responses API.

    any-llm parity: matches any_llm.responses signature.
    Uses OpenAI's /v1/responses endpoint (not /v1/chat/completions).
    """

async def aresponses(
    model: str,
    input: Union[str, List[Union[str, Dict]]] = None,      # litellm convention
    input_data: Union[str, List[Union[str, Dict]]] = None,  # any-llm convention
    *,
    provider: Optional[str] = None,
    instructions: Optional[str] = None,
    tools: Optional[List[Dict]] = None,
    tool_choice: Optional[Union[str, Dict]] = None,
    max_output_tokens: Optional[int] = None,
    temperature: Optional[float] = None,
    top_p: Optional[float] = None,
    stream: Optional[bool] = None,
    include: Optional[List[str]] = None,
    parallel_tool_calls: Optional[bool] = None,
    previous_response_id: Optional[str] = None,
    reasoning: Optional[Dict] = None,
    text: Optional[Dict] = None,
    presence_penalty: Optional[float] = None,
    frequency_penalty: Optional[float] = None,
    truncation: Optional[str] = None,
    store: Optional[bool] = None,
    service_tier: Optional[str] = None,
    user: Optional[str] = None,
    metadata: Optional[Dict[str, Any]] = None,
    background: Optional[bool] = None,
    safety_identifier: Optional[str] = None,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    **kwargs
) -> ResponsesAPIResponse:
    """
    Asynchronous OpenAI Responses API.

    any-llm parity: matches any_llm.aresponses signature.
    """
```

#### Embedding (any-llm parity)

```python
def embedding(
    model: str,
    input: Union[str, List[str]] = None,  # litellm convention
    inputs: Union[str, List[str]] = None,  # any-llm convention
    *,
    dimensions: Optional[int] = None,
    encoding_format: Optional[str] = None,
    provider: Optional[str] = None,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    timeout: Union[float, httpx.Timeout] = 600,
    **kwargs
) -> EmbeddingResponse:
    """
    Synchronous embedding generation.

    Dual-convention: accepts both `input` (litellm) and `inputs` (any-llm).
    Implementation resolves: data = input if input is not None else inputs.
    """

async def aembedding(
    model: str,
    input: Union[str, List[str]] = None,  # litellm convention
    inputs: Union[str, List[str]] = None,  # any-llm convention
    *,
    dimensions: Optional[int] = None,
    encoding_format: Optional[str] = None,
    provider: Optional[str] = None,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    timeout: Union[float, httpx.Timeout] = 600,
    **kwargs
) -> EmbeddingResponse:
    """
    Asynchronous embedding generation.

    Dual-convention: accepts both `input` (litellm) and `inputs` (any-llm).
    """
```

#### Completion with Retries (LiteLLM parity)

**M2/CROSS-3 fix:** `tenacity` is a required dependency (add to `pyproject.toml`).
To prevent retry budget multiplication (tenacity × FallbackExecutor × Router fallbacks),
`completion_with_retries` sets `num_retries=0` on the inner `completion()` call,
disabling Rust FallbackExecutor retry. Only the Python tenacity layer retries.

```python
# Required dependency: tenacity
# pip install tenacity
# Add to pyproject.toml: dependencies = [..., "tenacity>=0.4.0"]

from tenacity import retry, stop_after_attempt, wait_exponential_jitter

async def acompletion_with_retries(
    model: str,
    messages: List[Dict],
    **kwargs
) -> CompletionResponse:
    """
    Async completion with automatic retries using tenacity.

    LiteLLM parity: matches litellm.acompletion_with_retries.
    Retries on rate limit, timeout, and server errors.
    Retry policy: exponential backoff with jitter.

    CROSS-3 fix: num_retries=0 passed to acompletion() to disable
    FallbackExecutor retry and prevent retry budget multiplication
    (tenacity retries × FallbackExecutor retries × Router fallbacks).
    Only tenacity retries — single retry layer.
    """
    @retry(stop=stop_after_attempt(kwargs.get("num_retries", 3)),
           wait=wait_exponential_jitter(initial=0.5, max=30))
    async def _retry_wrapper():
        # Disable FallbackExecutor retry — tenacity handles retries only
        return await acompletion(model, messages, num_retries=0, **kwargs)

    return await _retry_wrapper()

def completion_with_retries(
    model: str,
    messages: List[Dict],
    **kwargs
) -> CompletionResponse:
    """
    Sync completion with automatic retries using tenacity.

    LiteLLM parity: matches litellm.completion_with_retries.
    Retries on rate limit, timeout, and server errors.
    Retry policy: exponential backoff with jitter.

    CROSS-3 fix: num_retries=0 passed to completion() to disable
    FallbackExecutor retry and prevent retry budget multiplication.
    """
    @retry(stop=stop_after_attempt(kwargs.get("num_retries", 3)),
           wait=wait_exponential_jitter(initial=0.5, max=30))
    def _retry_wrapper():
        return completion(model, messages, num_retries=0, **kwargs)

    return _retry_wrapper()
```

#### Responses API Sub-Methods (LiteLLM parity)

```python
def get_response(
    provider: str,
    response_id: str,
    *,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    **kwargs
) -> ResponsesAPIResponse:
    """Get a specific response by ID from provider storage."""

def delete_response(
    provider: str,
    response_id: str,
    *,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    **kwargs
) -> ResponsesAPIResponse:
    """Delete a specific response from provider storage."""

async def aget_response(
    provider: str,
    response_id: str,
    *,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    **kwargs
) -> ResponsesAPIResponse:
    """Async: Get a specific response by ID from provider storage."""

async def adelete_response(
    provider: str,
    response_id: str,
    *,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    **kwargs
) -> ResponsesAPIResponse:
    """Async: Delete a specific response from provider storage."""

async def acancel_responses(
    response_id: str,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
) -> ResponseResource:
    """Cancel an in-progress response."""

async def alist_input_items(
    response_id: str,
    after: Optional[str] = None,
    limit: int = 20,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
) -> List[InputItem]:
    """List input items for a response."""

async def acompact_responses(
    response_id: str,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
) -> ResponseResource:
    """Compact a response for storage efficiency."""
```

### Response Types for Extended Functions

```python
@dataclass
class TextCompletionResponse:
    id: str
    object: str = "text_completion"
    created: int
    model: str
    choices: List[TextChoice]
    usage: Usage

@dataclass
class TextChoice:
    text: str
    index: int
    finish_reason: Optional[str]

@dataclass
class ModerationResponse:
    id: str
    model: str
    results: List[ModerationResult]

@dataclass
class ModerationResult:
    flagged: bool
    categories: Dict[str, bool]
    category_scores: Dict[str, float]

@dataclass
class TranscriptionResponse:
    text: str
    language: Optional[str]
    duration: Optional[float]
    segments: Optional[List[Segment]]

@dataclass
class Segment:
    id: int
    start: float
    end: float
    text: str

@dataclass
class AnthropicMessagesResponse:
    id: str
    type: str
    role: str
    content: List[ContentBlock]
    model: str
    stop_reason: Optional[str]
    stop_sequence: Optional[int]
    usage: AnthropicUsage

@dataclass
class ResponsesAPIResponse:
    id: str
    object: str
    model: str
    created: int
    instructions: Optional[str]
    output: List[ResponseOutput]
    usage: Usage

@dataclass
class ResponseOutput:
    index: int
    type: str
    content: List[Dict]
```

## HTTP Proxy Architecture in any-llm-mode

**THIS SECTION IS THE COMPLETE DESIGN FOR HTTP PROXY IN ANY-LLM-MODE. THIS IS NOT DEFERRED — IT IS SPECIFIED NOW.**

### How it works

The HTTP proxy is a **Rust binary** (hyper/axum) that links to `quota-router-core`. In `any-llm-mode`, `quota-router-core` is compiled with PyO3, which means:

```
HTTP proxy process (hyper/axum, Rust):
  └── links to quota-router-core (Rust + PyO3)
         └── in any-llm-mode: Rust core calls Python SDKs via PyO3
                └── calls Python SDKs (openai, anthropic, etc.)
```

**Important clarification:** The proxy process DOES embed Python because `quota-router-core` in any-llm-mode embeds CPython via PyO3. The proxy does not directly call Python APIs — it delegates all Python interactions to the core library. But the Python interpreter runs in the same process space as the proxy.

### What this means practically

1. **Python installation required:** The proxy binary in any-llm-mode requires a compatible Python installation at runtime. It will not start without it.

2. **Startup sequence:** `quota-router-core` initializes the Python interpreter early (via `pyo3::prepare_freethreaded_python()` or equivalent) when the library is loaded.

3. **GIL management:** PyO3 requires GIL management for Python calls. The core library handles this — the proxy's concurrent HTTP requests must be designed to work with this. Specifically:
   - Each Python SDK call acquires the GIL, executes, then releases
   - Concurrent HTTP requests may queue on Python calls; design should use async task spawning to avoid blocking the proxy event loop
   - The core library should use a connection pool or queue for Python calls to manage GIL contention

4. **The proxy has zero direct Python awareness** — it just calls Rust functions in `quota-router-core`. The core library manages all Python interaction including GIL.

### How Rust core has PyO3 in any-llm-mode

In `any-llm-mode` builds:
1. `quota-router-core` is compiled with `pyo3/extension-module` feature
2. The Rust core binary includes PyO3 Rust bindings and links against `libpython`
3. When the proxy calls into Rust core, the Rust code can invoke Python functions (call Python SDKs)
4. The proxy process includes both Rust and Python runtime components

### What the proxy does in any-llm-mode

```
1. HTTP request arrives at proxy (hyper/axum)
2. Proxy parses request, validates API key via KeyMiddleware (in Rust core)
3. Proxy calls quota-router-core completion function (Rust-to-Rust call)
4. Rust core (in any-llm-mode) acquires GIL, calls Python SDK via PyO3, releases GIL
5. Response flows back through Rust core → proxy → client
```

### Comparison to litellm-mode

| Aspect | litellm-mode | any-llm-mode |
|--------|-------------|-------------|
| HTTP proxy process | Rust (hyper/axum) | Rust (hyper/axum) — SAME process |
| What proxy calls | quota-router-core (Rust) | quota-router-core (Rust) — SAME call |
| Provider strategy | reqwest (Rust HTTP client) | PyO3 → Python SDK |
| Python involvement | None | Yes — via PyO3 in Rust core |
| Python installation required | No | Yes |
| GIL management | N/A | Yes — managed by Rust core |

### GIL handling for concurrent requests

The primary concern with GIL is that multiple concurrent HTTP requests that call Python SDKs could contend on the GIL. The design approach:

- **Rust core uses async task handling** — Python SDK calls are made in async tasks that yield while waiting for the GIL
- **No global Python lock held across async await points** — GIL is acquired only for the duration of the actual Python SDK call
- **Concurrent requests serialize on Python calls** — this is acceptable since Python SDK calls are I/O-bound (network) and release the GIL while waiting

This is a Phase-3 implementation detail; the key point is that GIL management is the responsibility of `quota-router-core`, not the proxy.

## Feature Gate Architecture

**🚨 ARCHITECTURAL CONSTRAINT: HTTP PROXY MUST BE IN BOTH MODES — THIS CAN NEVER CHANGE 🚨**

Per RFC-0917 §Rust Feature Gates, **the mode gate selects the provider integration strategy, NOT the interface. Both HTTP proxy and Python SDK are available in ALL modes:**

**`quota-router-pyo3` has NO feature flags.** The Python SDK crate is a thin PyO3 binding layer that wraps whatever `quota-router-core` was compiled with. Mode selection happens at `quota-router-core` compile time:

```toml
# quota-router-core/Cargo.toml (RFC-0917 canonical definition)
[features]
default = ["full"]           # Both strategies + both interfaces

# Provider integration strategy ONLY — hyper/axum and py-o3 are UNCONDITIONAL
# (always compiled regardless of which feature flag is set)
litellm-mode = []             # Provider strategy: reqwest (native Rust HTTP)
any-llm-mode = []             # Provider strategy: PyO3 (official Python SDKs)
full = []                     # Both strategies simultaneously

# NOTE: hyper, axum, pyo3, pyo3-ffi are unconditional dependencies.
# They compile in ALL build configurations — NOT behind feature flags.
```

```toml
# quota-router-pyo3/Cargo.toml
[dependencies]
quota-router-core = { path = "../quota-router-core", features = ["full"] }
# No [features] section — pyo3 wraps whatever core was compiled with
# Mode is determined by which features quota-router-core was built with
```

**Mode controls only: what library is used to call providers (provider integration strategy)**
- `litellm-mode` (core): reqwest → direct to provider REST APIs
- `any-llm-mode` (core): PyO3 → official Python SDKs (Anthropic, OpenAI, Mistral, etc.)
- `full` (core): Both simultaneously

**Interface availability (all modes identical — NOT controlled by feature flags):**
| Interface       | litellm-mode | any-llm-mode | full |
|-----------------|:------------:|:------------:|:----:|
| HTTP proxy      |      ✅      |      ✅      |  ✅  |
| Python SDK      |      ✅      |      ✅      |  ✅  |

**Example deployment scenarios:**
- `litellm-mode` build: Run as HTTP proxy (`ProxyServer` on port 8080) AND use Python SDK via `import quota_router`
- `any-llm-mode` build: Run as HTTP proxy AND use Python SDK
- `full` build: Both simultaneously

**⚠️ CRITICAL: Both interfaces exist in ALL modes.** The table below shows provider strategy per mode, NOT which interfaces exist.

### Deployment Mode Selection

Mode is selected at **build time** via Cargo feature flags. **Mode selects provider strategy (reqwest vs PyO3), NOT interface availability:**

| Installation                          | Mode           | Provider Strategy | HTTP Proxy? | Python SDK? |
| ------------------------------------- | -------------- | ----------------- |:-----------:|:-----------:|
| `pip install quota-router` (PyPI)     | `any-llm-mode` | PyO3 only         | ✅ | ✅ |
| `quota-router-gateway` (crates.io)    | `litellm-mode` | reqwest only      | ✅ | ✅ |
| `cargo build --features full`          | `full`         | Both              | ✅ | ✅ |

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
    // N17 fix: cfg_if! ensures exactly ONE m.add call fires in full builds.
    // In "full" builds, feature "full" is enabled and "any-llm-mode" is NOT
    // (full = [] in Cargo.toml). Without cfg_if!, both #[cfg(feature = "full")]
    // and #[cfg(feature = "any-llm-mode")] would evaluate true simultaneously.
    cfg_if! {
        if #[cfg(feature = "full")] {
            m.add("__deployment_mode__", "full")?;
        } else if #[cfg(feature = "any-llm-mode")] {
            m.add("__deployment_mode__", "any-llm-mode")?;
        } else if #[cfg(feature = "litellm-mode")] {
            m.add("__deployment_mode__", "litellm-mode")?;
        }
    }
    Ok(())
}

// Python wrapper (optional convenience)
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

## Test Specifications

### Test File Mapping

| Test File | Covers |
|-----------|--------|
| `tests/test_drop_in_litellm.py` | LiteLLM drop-in compatibility |
| `tests/test_drop_in_any_llm.py` | any-llm drop-in compatibility |
| `tests/e2e_python.py` | E2E via OpenAI endpoint |
| `tests/e2e_anthropic.py` | E2E via Anthropic endpoint |
| `tests/smoke_test.py` | Basic import and structure |

### embedding() / aembedding() Tests

**Parameter Validation:**
- `test_embedding_positional_args`: `embedding("text-embedding-3-small", ["hello"])` — positional model, input
- `test_embedding_keyword_model`: `embedding(model="text-embedding-3-small", input=["hello"])` — keyword model
- `test_embedding_litellm_convention`: `embedding(model="...", input=["hello"])` — litellm uses `input=`
- `test_embedding_anyllm_convention`: `embedding(model="...", inputs=["hello"])` — any-llm uses `inputs=`
- `test_embedding_both_params_error`: `embedding(model="...", input=["a"], inputs=["b"])` — must raise error (ambiguous)
- `test_embedding_neither_param_error`: `embedding(model="...")` — must raise error (no input)
- `test_embedding_empty_input`: `embedding(model="...", input=[])` — empty list
- `test_embedding_string_input`: `embedding(model="...", input="hello")` — single string
- `test_embedding_list_input`: `embedding(model="...", input=["hello", "world"])` — list of strings

**Return Type:**
- `test_embedding_returns_object`: response has `.data`, `.model`, `.usage` fields
- `test_embedding_data_is_list`: `response.data` is a list of embedding objects
- `test_embedding_data_has_embedding`: each item has `.embedding` (list of floats)
- `test_embedding_usage_has_tokens`: `response.usage.prompt_tokens > 0`

**Advanced:**
- `test_embedding_client_args`: `embedding(model, input=["hello"], client_args={"timeout": 30})`

**Error Handling:**
- `test_embedding_invalid_model`: raises `ModelNotFoundError`
- `test_embedding_no_api_key`: raises `AuthenticationError` or `MissingApiKeyError`
- `test_embedding_rate_limited`: raises `RateLimitError` with `retry_after`

**Async:**
- `test_aembedding_returns_coroutine`: `aembedding(...)` returns a coroutine
- `test_aembedding_same_result_as_sync`: `await aembedding(...)` matches `embedding(...)`

### messages() / amessages() Tests

**Parameter Validation:**
- `test_messages_required_params`: `messages(model, messages, max_tokens)` — all required
- `test_messages_max_tokens_required`: `messages(model="...", messages=[...])` without max_tokens raises TypeError
- `test_messages_system_optional`: `messages(model, messages, max_tokens, system="You are helpful")` — system is optional
- `test_messages_system_union_type`: system accepts both `str` and `list[dict]` (Anthropic content blocks)
- `test_messages_stream_param`: `messages(model, messages, max_tokens, stream=True)` — uses `stream` not `streaming`
- `test_messages_stop_sequences`: `messages(model, messages, max_tokens, stop_sequences=["END"])` — uses `stop_sequences` not `stop`
- `test_messages_thinking_optional`: `messages(model, messages, max_tokens, thinking={"type": "enabled", "budget_tokens": 1000})`
- `test_messages_cache_control_optional`: `messages(model, messages, max_tokens, cache_control={"type": "ephemeral"})`
- `test_messages_client_args`: `messages(model, messages, max_tokens, client_args={"timeout": 30})`

**Provider Routing:**
- `test_messages_routes_to_anthropic`: messages() routes to Anthropic Messages API
- `test_messages_with_provider_kwarg`: `messages(model="...", messages=[...], max_tokens=100, provider="anthropic")`

**Error Handling:**
- `test_messages_invalid_model`: raises `ModelNotFoundError`
- `test_messages_no_api_key`: raises `AuthenticationError` or `MissingApiKeyError`

### responses() / aresponses() Tests

**Parameter Validation:**
- `test_responses_litellm_convention`: `responses(model="...", input="Hello")` — litellm uses `input=`
- `test_responses_anyllm_convention`: `responses(model="...", input_data="Hello")` — any-llm uses `input_data=`
- `test_responses_both_params_error`: `responses(model="...", input="a", input_data="b")` — must raise error
- `test_responses_neither_param_error`: `responses(model="...")` — must raise error
- `test_responses_max_output_tokens`: `responses(model, input, max_output_tokens=100)` — not `max_tokens`
- `test_responses_instructions_optional`: `responses(model, input, instructions="Be helpful")`
- `test_responses_client_args`: `responses(model, input, client_args={"timeout": 30})`

**Error Handling:**
- `test_responses_invalid_model`: raises `ModelNotFoundError`
- `test_responses_no_api_key`: raises `AuthenticationError` or `MissingApiKeyError`

### batch Functions Tests

**batch_create():**
- `test_batch_create_required_params`: `batch_create(provider, input_file, endpoint)` — all required
- `test_batch_create_no_model`: `batch_create` does NOT accept `model` param
- `test_batch_create_endpoint_required`: `batch_create(provider, input_file)` without endpoint raises TypeError
- `test_batch_create_client_args`: `batch_create(provider, input_file, endpoint, client_args={"timeout": 30})`
- `test_batch_create_return_type`: returns `BatchCreateResponse` with `.batch_id`, `.status` fields

**batch_retrieve():**
- `test_batch_retrieve_param_order`: `batch_retrieve(provider, batch_id)` — provider first
- `test_batch_retrieve_provider_required`: `batch_retrieve(batch_id="...")` without provider raises TypeError
- `test_batch_retrieve_return_type`: returns `BatchRetrieveResponse` with `.status`, `.output_file_id` fields

**batch_cancel():**
- `test_batch_cancel_param_order`: `batch_cancel(provider, batch_id)` — provider first
- `test_batch_cancel_return_type`: returns `BatchRetrieveResponse`

**batch_list():**
- `test_batch_list_required_params`: `batch_list(provider)` — provider required
- `test_batch_list_limit_optional`: `batch_list(provider, limit=10)` — limit is optional
- `test_batch_list_return_type`: returns `Sequence[BatchCreateResponse]`

**batch_results():**
- `test_batch_results_param_order`: `batch_results(provider, batch_id)` — provider first
- `test_batch_results_return_type`: returns `BatchResult` with `.results` list
- `test_batch_results_not_complete_error`: raises `BatchNotCompleteError` if batch not done

### list_models() Tests

**Parameter Validation:**
- `test_list_models_provider_required`: `list_models(provider="openai")` — provider is required
- `test_list_models_no_provider_error`: `list_models()` without provider raises TypeError
- `test_list_models_optional_params`: `list_models(provider, api_key="...", api_base="...")`
- `test_list_models_client_args`: `list_models(provider, client_args={"timeout": 30})`

**Return Type:**
- `test_list_models_return_type`: returns `Sequence[Model]` with `.id`, `.name`, `.provider`, `.created` fields

**Error Handling:**
- `test_list_models_no_api_key`: raises `AuthenticationError` or `MissingApiKeyError`
- `test_list_models_bad_provider`: raises `UnsupportedProviderError`

### Dual-Convention Tests (Drop-in Compatibility)

**LiteLLM Convention:**
- `test_litellm_embedding_input_kwarg`: `import quota_router as litellm; litellm.embedding(model="...", input=[...])`
- `test_litellm_responses_input_kwarg`: `import quota_router as litellm; litellm.responses(model="...", input="...")`

**any-llm Convention:**
- `test_anyllm_embedding_inputs_kwarg`: `import quota_router as any_llm; any_llm.embedding(model="...", inputs=[...])`
- `test_anyllm_responses_input_data_kwarg`: `import quota_router as any_llm; any_llm.responses(model="...", input_data="...")`

### Exception Compatibility Tests

**LiteLLM Aliases:**
- `test_budget_exceeded_alias`: `BudgetExceededError` catches `InsufficientFundsError`
- `test_service_unavailable_alias`: `ServiceUnavailableError` catches `UpstreamProviderError`
- `test_api_connection_alias`: `APIConnectionError` catches `GatewayTimeoutError`
- `test_timeout_alias`: `Timeout` catches `GatewayTimeoutError`
- `test_api_error_alias`: `APIError` catches `QuotaRouterError`
- `test_not_found_alias`: `NotFoundError` catches `ModelNotFoundError`
- `test_context_window_alias`: `ContextWindowExceededError` catches `ContextLengthExceededError`
- `test_content_policy_alias`: `ContentPolicyViolationError` catches `ContentFilterError`

**any-llm Exceptions:**
- `test_anyllm_error_base`: `AnyLLMError` catches `QuotaRouterError`
- `test_authentication_error`: `AuthenticationError` is catchable
- `test_rate_limit_error`: `RateLimitError` is catchable
- `test_invalid_request_error`: `InvalidRequestError` is catchable
- `test_provider_error`: `ProviderError` is catchable
- `test_content_filter_error`: `ContentFilterError` is catchable
- `test_model_not_found_error`: `ModelNotFoundError` is catchable
- `test_context_length_exceeded_error`: `ContextLengthExceededError` is catchable
- `test_missing_api_key_error`: `MissingApiKeyError` is catchable
- `test_unsupported_provider_error`: `UnsupportedProviderError` is catchable
- `test_unsupported_parameter_error`: `UnsupportedParameterError` is catchable
- `test_batch_not_complete_error`: `BatchNotCompleteError` is catchable

## Security Considerations

### API Key Trust Boundary

The SDK has **two incompatible API key handling modes** with different security properties:

| Aspect                 | `set_api_key()` (recommended)  | `api_key=...` per-call                  |
| ---------------------- | ------------------------------ | --------------------------------------- |
| Key storage            | Rust memory (enforceable)      | Goes directly to provider SDK           |
| Budget enforcement     | Enforceable (HMAC-SHA256 key) | **NOT enforceable** (SDK bypasses Rust) |
| Virtual key (RFC-0903) | Enforceable                    | **NOT enforceable**                     |
| Traceability           | Key identity → Rust → Provider | Key identity → Provider directly        |

**Budget Identity Derivation (per RFC-0917 §Budget Identity in SDK Mode):**

When `set_api_key(provider, api_key)` is called:
1. The SDK stores the provider API key in Rust memory via StoolapKeyStorage
2. `key_id = HMAC-SHA256(server_secret, provider_key)[:16]` — 16-byte budget identity
3. A budget entry is created/updated in the `api_keys` table with `key_id`, `budget_limit`, `rpm_limit`, `tpm_limit`
4. Subsequent `record_spend()` calls use this `key_id` for budget tracking

**server_secret Provisioning:**

The `server_secret` used for HMAC derivation must be provisioned securely:

```python
import os

def _get_server_secret() -> bytes:
    """
    Get server secret for HMAC-SHA256 budget identity derivation.

    Provisioning priority:
    1. QUOTA_ROUTER_HMAC_SECRET env var (min 32 bytes recommended)
    2. Machine-derived fallback for dev/test only:
       - Cross-platform via uuid.getnode() + app salt
       - Hashed to 32 bytes via SHA256

    ⚠️ WARNING: Machine-derived fallback is VULNERABLE TO KEY-COLLISION ATTACKS
    in multi-tenant environments. Production deployments MUST set QUOTA_ROUTER_HMAC_SECRET.

    **⚠️ Containerized/cloud environment warning:** `uuid.getnode()` may return
    virtualized/MAC addresses in container environments (Docker, Kubernetes pods).
    In multi-tenant or replicated deployments, this can cause budget identity collisions.
    The fallback is ONLY for local dev/test on a single machine.

    **Required env var for production:** `QUOTA_ROUTER_HMAC_SECRET`

    **Dev/test fallback gate:** The fallback is used only if
    `QUOTA_ROUTER_ALLOW_INSECURE_HMAC_FALLBACK=1` is set. Without this flag,
    production builds fail fast if `QUOTA_ROUTER_HMAC_SECRET` is not set.

    **Env var case sensitivity:** `QUOTA_ROUTER_HMAC_SECRET` is case-sensitive.
    On Windows (case-insensitive env vars), prefer lowercase alias
    `quota_router_hmac_secret` for portability. The SDK checks exact case first,
    then falls back to lowercase variant for dev convenience.
    """
    # Case-sensitive lookup first, then lowercase fallback for dev portability
    secret = os.environ.get("QUOTA_ROUTER_HMAC_SECRET")
    if not secret:
        secret = os.environ.get("quota_router_hmac_secret")  # lowercase fallback for dev
    if secret:
        secret_bytes = secret.encode("utf-8")
        if len(secret_bytes) < 16:
            import warnings
            warnings.warn(
                "QUOTA_ROUTER_HMAC_SECRET is shorter than 16 bytes. "
                "This reduces HMAC security. Use at least 32 bytes.",
                UserWarning,
            )
        return secret_bytes

    # N15 fix: Dev/test fallback gate — NOT for production.
    # The fallback (uuid.getnode() machine identity) is vulnerable to key-collision
    # attacks in multi-tenant environments. Require explicit opt-in flag.
    if os.environ.get("QUOTA_ROUTER_ALLOW_INSECURE_HMAC_FALLBACK") != "1":
        raise RuntimeError(
            "QUOTA_ROUTER_HMAC_SECRET not set and HMAC fallback not enabled. "
            "Production deployments MUST set QUOTA_ROUTER_HMAC_SECRET env var. "
            "For local dev/test only, set QUOTA_ROUTER_ALLOW_INSECURE_HMAC_FALLBACK=1."
        )
    # Use uuid.getnode() for cross-platform machine identity
    import hashlib
    import uuid
    try:
        node = uuid.getnode()
        machine_id = str(node).encode("utf-8")
    except Exception:
        machine_id = b"dev-machine-no-machine-id"
    salt = b"quota-router-sdk-v1"
    return hashlib.sha256(machine_id + salt).digest()
```

**Note:** If `QUOTA_ROUTER_HMAC_SECRET` is unset, budget identity falls back to `SHA256(provider_key)` (non-HMAC). Budget tracking still works but is vulnerable to key-collision attacks in multi-tenant environments. Production SDK deployments MUST provision the secret out-of-band.

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
| 42 providers            | Partial     | ✅ All 42      |

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

## Async Implementation Specification

### Current State

All `a*` functions (`acompletion`, `aembedding`, `amessages`, `aresponses`, `abatch_*`, `alist_models`) are **sync wrappers** — they call their sync counterparts and block the Python event loop.

```rust
// CURRENT: acompletion blocks the event loop
pub async fn acompletion(...) -> PyResult<Py<PyAny>> {
    completion(...)  // sync call — blocks!
}
```

### Target State

All `a*` functions must be **true async** — they `.await` the Rust core's async provider dispatch without blocking the Python event loop.

### How It Works

**pyo3 0.21 `experimental-async`** (already enabled in `Cargo.toml`):
- `pub async fn` in `#[pyfunction]` creates a native Python coroutine
- pyo3 manages the tokio runtime internally
- The coroutine can `.await` Rust futures without blocking

**Rust core already has async traits:**
```rust
// quota-router-core/src/native_http/mod.rs
#[async_trait]
pub trait HttpProvider: Send + Sync {
    async fn completion(&self, request: &HttpCompletionRequest, api_key: Option<&str>) -> Result<HttpCompletionResponse, ProviderError>;
    async fn streaming_completion(&self, request: &HttpCompletionRequest, api_key: Option<&str>) -> Result<StreamingResponse, ProviderError>;
    async fn embedding(&self, request: &HttpEmbeddingRequest, api_key: Option<&str>) -> Result<HttpEmbeddingResponse, ProviderError>;
}
```

### Required Changes

**1. Create async dispatch function in `completion.rs`:**

```rust
async fn dispatch_completion_async(
    provider: &dyn HttpProvider,
    request: &HttpCompletionRequest,
    api_key: Option<&str>,
) -> Result<HttpCompletionResponse, ProviderError> {
    provider.completion(request, api_key).await
}
```

**2. Update `acompletion()` to use async dispatch:**

```rust
#[pyfunction]
#[pyo3(name = "acompletion")]
pub async fn acompletion(...) -> PyResult<Py<PyAny>> {
    let parsed = ParsedModel::parse(&model)?;
    let provider = HttpProviderFactory::create(&parsed.provider, &parsed.model);
    let request = build_request(...);

    // True async — does NOT block the event loop
    let result = provider.completion(&request, api_key.as_deref()).await
        .map_err(|e| ...)?;

    // Convert to Python dict
    Python::with_gil(|py| { ... })
}
```

**3. Apply same pattern to all `a*` functions:**
- `aembedding()` → `provider.embedding(&request, api_key).await`
- `amessages()` → async provider dispatch (once Messages API is implemented)
- `aresponses()` → async provider dispatch (once Responses API is implemented)
- `abatch_*` → async provider dispatch (once Batch API is implemented)

**4. Remove sync wrapper pattern:**

```rust
// BEFORE (sync wrapper — blocks event loop)
pub async fn acompletion(...) -> PyResult<Py<PyAny>> {
    completion(...)  // BAD: blocks
}

// AFTER (true async — non-blocking)
pub async fn acompletion(...) -> PyResult<Py<PyAny>> {
    provider.completion(&request, api_key).await  // GOOD: non-blocking
}
```

### Provider Factory Async Support

The `HttpProviderFactory::create()` returns a `Box<dyn HttpProvider>`. Since `HttpProvider` is `#[async_trait]`, calling `.completion().await` on it is already supported. No changes needed to the trait or factory.

### py_bridge (any-llm-mode) Async

For `py_bridge` providers (any-llm-mode), async dispatch requires calling the Python provider's async method from Rust. This uses `pyo3_asyncio`:

```rust
// py_bridge async dispatch
async fn dispatch_py_bridge_async(
    provider: &dyn PyBridgeProvider,
    model: &str,
    messages: &[Message],
) -> Result<ChatCompletion, PyBridgeError> {
    // PyBridgeProvider::completion() is sync (calls Python SDK)
    // For true async, need to call Python's async method via pyo3_asyncio
    // This is a Phase 2 enhancement for py_bridge
}
```

**Note:** `py_bridge` providers call Python SDKs (which may be async). True async for py_bridge requires `pyo3_asyncio` to bridge Rust async ↔ Python async. This is separate from `native_http` async (which is pure Rust).

### Scope

| Function | native_http async | py_bridge async |
|----------|------------------|-----------------|
| `acompletion()` | Phase 1 | Phase 2 (pyo3_asyncio) |
| `aembedding()` | Phase 1 | Phase 2 |
| `amessages()` | Phase 1 | Phase 2 |
| `aresponses()` | Phase 1 | Phase 2 |
| `abatch_*` | Phase 1 | Phase 2 |
| `alist_models()` | Phase 1 | Phase 2 |

Phase 1: `native_http` providers use Rust async (tokio + reqwest async).
Phase 2: `py_bridge` providers use `pyo3_asyncio` to call Python async SDKs.

## Future Work

- F4: Response caching (RFC-0906) — spec is in RFC-0906

## Rationale

The dual-style approach maximizes adoption by meeting users where they are:

- LiteLLM users keep their `provider=` param pattern
- any-llm users keep their `provider:model` pattern
- Both can coexist in the same codebase

This is the only approach that achieves true drop-in replacement for both ecosystems.

## Version History

| Version | Date       | Changes |
| ------- | ---------- | ------- |
| 1.72    | 2026-05-21 | **SPEC** Added sync get_response()/delete_response() with `provider` first param, matching test specs. Added async variants aget_response()/adelete_response(). |
| 1.71    | 2026-05-21 | **FIX** C3: Removed stale `model` param from batch_create() docstring. **FIX** H3: Updated test specs — removed NotImplementedError stubs for batch/messages/responses, added return type and error handling tests. **FIX** H6: Added thinking/cache_control/client_args tests for messages(). **FIX** M3/M4: Added client_args tests for embedding() and list_models(). |
| 1.70    | 2026-05-10 | **FIX** O1: Provider count corrected from 41→42 throughout (header table, Phase 2 checklist, KNOWN_PROVIDERS description, platform provider reference, acceptance criteria table). Mission 0920-b correctly lists 42 providers with deepinfra; RFC was inconsistent. |
| 1.69    | 2026-05-10 | **FIX** N1 (Low): Updated stale deprecation notice to reflect v1.68 structural change. The notice previously referenced "Python Router class" — a class that was ERASED in v1.68. Updated to confirm current RouterHandle is a thin PyO3 delegation stub with NO routing state, and that all heavy lifting resides in RFC-0917. |
| 1.68    | 2026-05-09 | **ERASE** Deprecated Python Router class (~2907-3387 lines): Removed all Phase 1 Python routing state, 8 routing strategy implementations, `_record_spend` with decay math, `_select_by_weighted_spend`, and all `completion`/`acompletion` methods with fallback logic. RFC-0920 is now purely a thin-binding layer: API surface, type marshaling, exception translation, and Phase 2 state mapping reference. All heavy lifting (ProviderWithState, RoutingStrategy enum, all 8 strategy algorithms, SpendTracker with decay math) is now exclusively in RFC-0917. |
| 1.67    | 2026-05-09 | **FIX** RFC-0920 thin-binding constraint made explicit: Summary now states sole role is API surface + type marshaling; all heavy lifting is in RFC-0917. Dependencies marks RFC-0917 as "definitive source." Crate Architecture section now has red box: "RFC-0920 MUST NEVER DEFINE ANY HEAVY LIFTING." Phase 4 unspecced items removed per deferred-vs-unspecified rule. |
| 1.66    | 2026-05-09 | **CROSS** F1: RouterHandle.batch_completion_models_all() in RFC-0917 v2.43 now passes all 22 params (was dropping 20 params). **REMOVE** Phase 4 unspecced items (modalities, audio, prediction, 8 routing strategies, additional providers) per deferred-vs-unspecified rule — no spec means remove. F1/LangChain and F2/LlamaIndex removed (no spec). F3/SSE normalization deferred to RFC-0906 Phase 3. F4/Response caching already references RFC-0906. Cross-reference to F4 (SdkMessage.name clarification) and F6 (duplicate TODO removal). |
| 1.65    | 2026-05-09 | **CROSS** E1/E2/E3: Client.batch_completion_models() and batch_completion_models_all() now have same 22-param signatures as RouterHandle (per RFC-0917 v2.42). |
| 1.64    | 2026-05-09 | **CROSS** D4/D7/D8: RFC-0917 v2.41 SdkMessage.role now specifies valid values (user|assistant|system|function|tool). RFC-0920 Python SDK should validate role against these values at entry point. |
| 1.63    | 2026-05-09 | **FIX** D1/D2: RouterHandle.batch_completion_models() and batch_completion_models_all() now show all 22 params (was only temperature/max_tokens — inconsistent with Client section which had full params). **FIX** D9: batch_completion_models_all() documented as returning only successful responses (failures silently dropped). |
| 1.62    | 2026-05-09 | **FIX** C1: Client.batch_completion_models() now uses tokio::sync::mpsc (per RFC-0917 v2.39). **FIX** C2: Client.get_metrics() returns Result (matches RouterHandle). **FIX** C6: batch_completion_models() and batch_completion_models_all() now show all 22 params (was only temperature/max_tokens). |
| 1.61    | 2026-05-09 | **FIX** B1: RouterHandle.completion() now has full 22 params (was incomplete in v1.60). **FIX** B2: Added batch_completion_models() and batch_completion_models_all() to RouterHandle section. **FIX** B5: Added RouterHandle::new() constructor. **FIX** B4: Client section now shows full method list matching RouterHandle. Cross-RFC: RFC-0917 v2.38 provides concrete implementation. |
| 1.60    | 2026-05-09 | **FIX** A1: Added `client` module entry point for direct Rust-to-core calls (no HTTP, no PyO3). **FIX** A3: RouterHandle.completion() now enumerates all 22 params per Completion API section. **FIX** A4: Added batch_completion_models() and batch_completion_models_all() to RouterHandle section. **FIX** A8: Version history now notes RFC-0917 v2.37 cross-reference (RouterHandle defined there). Cross-RFC dependency: RFC-0917 v2.37 provides concrete `RouterHandle` struct which RFC-0920 references throughout. |
| 1.58    | 2026-05-06 | **FIX** N1: _record_spend pricing and write restored (was missing — only comment remained). **FIX** N3: UnsupportedProviderError `field()` removed (non-dataclass). **FIX** N4: BatchNotCompleteError, AllModelsFailedError, BatchPartialFailureError now have `__init__`. **FIX** N5: into_async → into_future in RFC-0920 proxy code (2 locations). **FIX** N6: _stream_provider_response filters None from SSE parsers. **FIX** N7: parse_anthropic_sse now stateful via _AnthropicSSEParser class (multi-line SSE). **FIX** N8: Router.completion() indentation fixed (comment and result dedented to try block). **FIX** N13/N21: _spend_history initialized in __init__; guard reordered before access. **FIX** N14: batch_completion_models docstring corrected (losers run to completion, not cancelled). **FIX** N15: _get_server_secret fallback gate now enforced. **FIX** N16: QUOTA_ROUTER_MODE clarified as label-only (not provider strategy switch). **FIX** N17: __deployment_mode__ via cfg_if! (mutually exclusive). **FIX** N18: bedrock added to PROVIDER_SDK_TYPES registry (Rust + Python + YAML). **FIX** N19: normalize_timeout precedence corrected (.total > .read > .connect). **FIX** N20: batch_completion_models_all_responses return type List[Optional[CompletionResponse]]. **FIX** N22: Router.completion() imports time inside try block. |
| 1.57    | 2026-04-30 | **FIX** H5: QuotaRouterError base class aligned with RFC-0917 — added `status` (int=0) and `details` (dict={}) fields, canonical 5-param `__init__`. **FIX** H6: All exception subclasses now have proper `__init__` overrides that accept and assign extra fields (retry_after, param, upstream_code, max_tokens, received_tokens, etc.). **FIX** D3: _record_spend TOCTOU race — pricing moved inside single lock acquisition; current_spend read inside lock (not stale captured value). **FIX** D4: Ollama streaming — corrected `ollama.async_stream` → `ollama.AsyncClient().chat()` (proper async SDK pattern); removed sync for-loop inside async def. **FIX** M2: resolve_provider empty string check now single early-return guard. |
| 1.55    | 2026-04-30 | **DOCS** RFC-0920 no changes needed — confirms HTTP proxy constraint box and Mode Gate ≠ Interface invariant already correctly state both interfaces available in all modes. All 8 Round 38 findings formally rebutted as stale-cached review of pre-v1.54 version. |
| 1.54    | 2026-04-30 | **FIX** Feature Gate Architecture: Replaced incorrect Cargo.toml block — all three modes had identical `pyo3/extension-module` with contradictory comments. quota-router-pyo3 now correctly documented as having NO feature flags; it wraps whatever quota-router-core was compiled with. Mode selection happens at quota-router-core compile time. **FIX** set_api_key() budget enforcement: Corrected "In-memory, no enforcement" to correctly reflect that budget enforcement (RFC-0904) is active in ALL modes via HMAC-SHA256 key_id + StoolapKeyStorage. **FIX** Provider count: Changed "42" to "41" in header and checklist. Fixed any-llm gap analysis text (was "39+1=40", now correct). **FIX** Python Router NON-NORMATIVE marker: Added explicit note that Phase 1 Python Router violates Rust-owns-all-heavy-lifting constraint and is non-normative placeholder. |
| 1.53    | 2026-04-30 | **FIX** 0920-C1 (cross-impact): HTTP proxy constraint box corrected — HTTP proxy IS available in any-llm-mode via PyO3 bridge. Box now matches mode table (both ✅ YES). **FIX** 0920-C2: Provider count "42" → "41" (list has 41, not 42). **FIX** 0920-C4 (cross-impact): B5 parsing rules now provider-list matching per RFC-0917 update. **FIX** 0920-C3 (cross-impact): deepinfra added to RFC-0917 Phase 3 list, completing cross-RFC sync. |
| 1.52    | 2026-04-30 | **FIX** 0920-C1: Unified QuotaRouterError base class with optional status/provider fields. **FIX** 0920-H1: InsufficientFundsError balance field u64→u32 μunits int. **FIX** 0920-H2: batch_completion_models_all_responses dict→list-of-tuples (key collision on duplicate model names). **FIX** 0920-H3: CI validator operator precedence — parentheses around compound OR, `.is_dir()` for .git detection. **FIX** 0920-M1: resolve_provider explicit empty string check before None check. **FIX** 0920-M2: completion_with_retries tenacity num_retries=0 to prevent budget multiplication. **FIX** 0920-M3: _validate_no_nan_inf isinstance check for messages parameter + closed code fence. **FIX** 0920-L1: code fence lang specifier fixed (python→python). **FIX** CROSS-1: YAML registry sync — groq added to RFC-0920, canonical source designation to RFC-0917. **FIX** (RFC-0917 cross-impact): HTTP proxy availability corrected to build-dependent (Assertion A in RFC-0917 propagated to constraint box). |
| 1.51    | 2026-04-30 | **ADD** Extended API Surface section: add all missing functions from any-llm (embedding, aembedding, messages, amessages, responses, aresponses) and LiteLLM (completion_with_retries, acompletion_with_retries, text_completion, atext_completion, moderation, amoderation, atranscription, adapter_completion, Responses API sub-methods) required for true drop-in replacement parity. Add response types for all extended functions. |
| 1.50    | 2026-04-30 | **FULL SPEC** Phase 2/3 SSE parsing: replaced all TODO stubs with full implementation for parse_openai_sse, parse_anthropic_sse, parse_mistral_sse, parse_ollama_sse (SSEParser class) and normalize_to_openai_sse with provider-specific transformation logic. Fully spec-ed ChatCompletionStreamIterator._create_stream with OpenAI/Anthropic/Mistral/Ollama SDK streams. |
| 1.49    | 2026-04-30 | **ADD** Phase 2 Rust ←→ Python State Mapping table: maps Python Router fields to RFC-0917 equivalents. |
| 1.48    | 2026-04-29 | **CRITICAL** Add missing LiteLLM params: `thinking`, `modalities`, `audio`, `prediction` to both sync and async completion signatures. |
| 1.45    | 2026-04-29 | Fix external adversarial review round 27: L1 (cache_bypass docstring updated to reference Rust forwarding in PyO3 builds), L2 (CI validator header comment added clarifying build-time stub validation scope). C1, C2, H1, H2, M1, M2 formally rebutted as architecture change requests, not bugs — RFC-0920 explicitly specifies Python Router as Python-level component with no Rust delegation. |
| 1.44    | 2026-04-29 | Fix external adversarial review round 26: C1 (batch worker cache_bypass explicit binding via functools.partial comment), C2 (CI regex scoped to PROVIDER_SDK_TYPES constant block), H1 (.git/ and ci/ dir markers replace subproject toml files), H2 (counter-based modulo sampling replaces random.random()), M1 (OPERATIONAL WARNING added to completion() function docstring), M2 (Path.absolute() replaces resolve() for container-safe path resolution), L1 (Accepted — codegen enforces pure-literal constraint), L2 (lock_metric_sampling_rate init param + QUOTA_ROUTER_LOCK_METRIC_SAMPLE_RATE env). |
| 1.43    | 2026-04-29 | Fix external adversarial review round 23: C1 (cache_bypass wired in Router.acompletion and batch methods), C2 (CI regex includes ,? for rustfmt trailing commas), H1 (marker-file search replaces parent.parent fragile traversal), H2 (router_lock_hold_time_us collection adds sampling rate config), M1 (import math confirmed at module level), M2 (cache_bypass docstring adds fallback amplification warning), L1 (codegen contract explicitly forbids inline comments), L2 (add standard v1.43 changelog entry). |
| 1.42    | 2026-04-29 | Fix external adversarial review round 22: C1 (explicit cache_bypass delegation in Router.completion), C2 (whitespace-agnostic CI regex allows rustfmt), H1 (pure-dict-literal codegen contract + error message), H2 (cache_bypass docstring clarifies kwargs-only validation), M1 (pathlib script-relative path resolution), M2 (math.exp2 decay optimization pushes threshold to >12k RPS), L1 (idiomatic regex character class), L2 (router_lock_hold_time_us histogram definition). |
| 1.41    | 2026-04-29 | Fix external adversarial review round 21: C1 (docstring aligned with implementation), C2 (try/except around ast.literal_eval + tree.body iteration), H1 (decay math documented with trade-off note), H2 (strict Rust const array format mandated), M1 (comment added clarifying len==1 overlap), M2 (docstring clarifies caller responsibility), L1 (tree.body replaces ast.walk), L2 (now = time.monotonic() captured once). |
| 1.40    | 2026-04-29 | Fix external adversarial review round 20: C1 (ast.literal_eval replaces exec()), C2 (setdefault replaced with lock-protected if-not-in), H1 (cache_bypass skips _validate_no_nan_inf when True), H2 (YAML-driven codegen parity + file existence guards), M1 (cache_bypass docstring includes cost warning), M2 (already addressed in C2), L1 (deque handles single-message dedup), L2 (file existence checks added). |
| 1.39    | 2026-04-29 | Fix external adversarial review round 21: C1 (use setdefault for atomic deque init — no check-then-act race), C2 (add empty-message guard + isinstance(content, str) check for multimodal safety), H1 (get_pricing moved to module level as _get_pricing — eliminates hot-path import), H2 (replaced git-dependent bash script with environment-agnostic Python schema validator), M1 (cache_bypass wired into compute_request_hash — skips validation/serialization when True), M2 (capture model_name inside _state_lock before external pricing lookup), L1 (deque import already at module level), L2 (compute_request_hash signature added cache_bypass parameter). |
| 1.38    | 2026-04-29 | Fix external adversarial review round 20: C1 (use deque(maxlen=500) instead of list slicing under lock), C2 (sample messages[0] and messages[-1] instead of [:3]), H1 (pricing lookup moved outside _state_lock — must be O(1) in-memory access), H2 (replaced impossible "identical at line/entry level" CI rule with YAML source-hash and schema parity validation), M1 (corrected normalize_to_openai_sse docstring: Mistral/Ollama yield SDK-parsed dicts, not raw SSE strings), M2 (added cache_bypass: bool = False to both unified signatures), L1 (added OverflowError to json.dumps exception handler), L2 (added explicit call-site snippet enforcing execution order). |
| 1.37    | 2026-04-29 | Fix external adversarial review round 19: C1 (reverted split-lock to single lock acquisition in _record_spend, eliminates TOCTOU race), C2 (catch (ValueError, TypeError) in compute_request_hash), H1 (O(k=3) heuristic replaces O(N) sum for cache bypass guard), H2 (documented char-vs-token mismatch limitation and configurable bypass options), M1 (added providers_sdk_types.yaml single source of truth for Python/Rust registry sync), M2 (bounded _spend_history with maxlen=500 on truncate), L1 (mandated compute_request_hash invocation before routing/budget checks), L2 (added normalize_to_openai_sse explicit signature and error contract). |
| 1.36    | 2026-04-29 | Fix external adversarial review round 18: C1 (json.dumps wrapped in try/except → InvalidRequestError on nested NaN/Inf), C2 (Rust dispatch .unwrap_or(&"sync") aligned with Python default="sync"), H1 (_record_spend: compute elapsed/decay outside lock, lock hold target <50μs), H2 (compute_request_hash returns None for >50k char payloads, bypassing cache), M1 (Prometheus /metrics and OpenTelemetry OTLP specified for fallback metrics), M2 (documented time.monotonic() ephemeral semantics and restart behavior), L1 (added _validate_no_nan_inf call site in completion/acompletion entry), L2 (added Phase 3 raw_stream bypass hook in SSE normalization pipeline). |
| 1.35    | 2026-04-29 | Fix external adversarial review round 17: C2 (SDK entry validates NaN/Inf → InvalidRequestError; compute_request_hash removed recursive sanitize, uses allow_nan=False), H1 (PROVIDER_SDK_TYPES default changed from "async" to "sync" for safety), M1 (removed recursive sanitize from compute_request_hash; top-level only validation preserves G1 <10ms), M2 (copy-on-read snapshot pattern for _total_spend reduces lock contention), L1 (raw_stream lifecycle clarified: Phase 1 ignored, Phase 3 marker), L2 (WARN logging, /health flag, metrics for pyo3-asyncio fallback). C1/H2 already fixed in v1.34 (time.monotonic + elapsed clamping). |
| 1.34    | 2026-04-28 | Fix external adversarial review round 16: C1 (time-based exponential decay: decay_factor = 0.5 ** (elapsed_seconds / half_life), prevents routing inversion), C2 (pre-sanitize floats: NaN→"NaN", Inf→"Infinity", removed allow_nan=False crash vector), H1 (added PROVIDER_SDK_TYPES registry + dispatch logic for dual-path proxy), H2 (locked _total_spend reads in cost-based-routing and usage-based-routing), M1 (simple-shuffle is uniform random, explicit weight/RPM/TPM handling noted), M2 (added pyo3-asyncio dependency + Tokio/Python compatibility + spawn_blocking fallback), L1 (raw_stream deprecated with comment, no runtime warning), L2 (terminology already correct: ModelSelector, not transient Router). |
| 1.30    | 2026-04-28 | Fix external adversarial review round 12: E1 (defined QUOTA_ROUTER_HMAC_SECRET provisioning for SDK budget derivation), E2 (changed Rust Balance struct to u64 μunits per RFC-0904 G3), E3 (fixed resolve_provider to use RFC-0917 C8 provider-list matching — not reject-if-both), E4 (removed QUOTA_ROUTER_MODE runtime switching claim for PyPI wheels — compile-time only), E5 (fixed async_iter_to_sync_iter: use run_in_executor to avoid blocking event loop), E6 (clarified cache_responses is exact-match KV until Phase 3 semantic cache), E7 (extended exception mapping for TeamBudgetExceeded and StorageError), E8 (added .connect fallback to httpx.Timeout normalization), E9 (replaced greedy regex with longest-match provider-list for any- keys), E10 (gated stream=True in Phase 1 behind NotImplementedError, opt-in via raw_stream=True). |
| 1.28    | 2026-04-28 | Fix external adversarial review round 10: C2 (batch_completion_models_all_responses() now has full implementation), H1 (_record_request_start/end now lock _active_requests with _state_lock), H2 (_create_stream_iter return type changed to ChatCompletionStreamIterator), H3 (weighted strategy fallback clarified as safety net), M1 (streaming spec clarified that **kwargs passes all params to provider), M2 (_EMBEDDED_MODE moved before _get_compiled_modes for clarity), M3 (_select_by_weighted_spend now locked with _state_lock), M5 (abatch_completion no longer raises on partial failure — matches LiteLLM behavior, returns successful results with None for failed). C1 was false positive — response_format already in sync completion since v1.16. |
| 1.26    | 2026-04-28 | Fix external adversarial review round 8: H1 (batch_completion_models now waits for remaining models after first failure with loop), H2 (round-robin now thread-safe via threading.Lock), H3 (_record_spend added to record token cost for usage-based routing), H4 (usage-based-routing-v2 implementation with _spend_history and recency-weighted scoring), H7 (resolve_provider normalizes provider_param to lowercase), H8 (_stream_sync_bridge fixed — not async def, returns async_iter_to_sync_iter result), H9 (get_deployment_mode now has single implementation with _get_compiled_modes and _EMBEDDED_MODE defined), M1 (BatchPartialFailureError defined in exception hierarchy), M5 (model_list validation: if empty list passed, raises error), M10 (_EMBEDDED_MODE now defined at module level for get_deployment_mode). |
| 1.25    | 2026-04-28 | Fix external adversarial review round 7: Observation 1 (cleaned get_budget_status docstring), Observation 2 (safety_identifier removed from any-llm-not-specced table, documented as Phase 3), Observation 3 (added note on single-target fallback behavior and resilience recommendation). |
| 1.24    | 2026-04-28 | Fix external adversarial review round 6: C6-1 (corrected HTTP proxy embedding — Python IS embedded via PyO3 in Rust core, proxy delegates to core), CH-6 (Router now explicitly sets num_retries=1 in call_kwargs when fallbacks configured — mandatory, not recommended), CM-9 (added GIL management design for concurrent HTTP requests), L11 (ignored — rebuttal: emphatic language is appropriate for critical constraints). |
| 1.23    | 2026-04-28 | Fix external adversarial review round 5: C5-2 (fallback_idx now local per-request variable, not persisted), C5-3 (last_error stored before continue, raises meaningful error if all fallbacks exhausted), CM-7 (Rust FallbackExecutor coordination now REQUIRED max_retries=1 when fallbacks configured), CM-8 (fallback list iterates once without wrapping), L9 (clarified "DIRECTLY" — proxy calls Rust core which may internally use PyO3), L10 (is_known_provider cross-reference added). |
| 1.22    | 2026-04-28 | 🚨 MASSIVE RED FLAG 🚨 HTTP proxy is FOREVER in BOTH litellm-mode and any-llm-mode. #1 architectural constraint. Flooded throughout RFC to prevent future incorrect claims. |
| 1.21    | 2026-04-28 | CORRECTION: HTTP proxy is ALWAYS available in both litellm-mode and any-llm-mode. The proxy ALWAYS calls quota-router-core directly — never through PyO3 bindings. C4-1 in v1.20 was incorrect and is reverted. Added explicit note that HTTP proxy architecture is performance-first (direct Rust core calls). |
| 1.20    | 2026-04-28 | Fix external adversarial review round 4 (2026-04-28): CH-4 (Router fallback now re-selects deployment with correct params), CM-4 (fallback iteration advances through list using _fallback_idx), CM-5 (acompletion(stream=True) in any-llm-mode returns AsyncIterator, not sync via bridge), CM-6 (added Router/Rust FallbackExecutor coordination note), L6 (KNOWN_PROVIDERS defined as runtime registry), L8 (fallbacks List[Dict] normalized to Dict for lookup). NOTE: C4-1 (HTTP proxy descoped to full) was INCORRECT and is reverted in v1.21. |
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
| 1.2     | 2026-04-27 | Gap analysis vs any-llm/litellm: add missing completion params (timeout, thinking, system, etc.), streaming async bridge spec, batch_completion() spec, router strategies, exception mapping, platform provider. Phase 4 added for full LiteLLM compat. Provider count 41→42 (added deepinfra). Clarify redis_url=N/A (stoolap replaces Redis per RFC-0912/0914); cache_responses uses stoolap semantic cache per RFC-0913. |
| 1.2     | 2026-04-27 | Gap analysis vs any-llm/litellm: add missing completion params (timeout, thinking, system, etc.), streaming async bridge spec, batch_completion() spec, router strategies, exception mapping, platform provider. Phase 4 added for full LiteLLM compat. Provider count 41→42 (added deepinfra). Clarify redis_url=N/A (stoolap replaces Redis per RFC-0912/0914); cache_responses uses stoolap semantic cache per RFC-0913. |
| 1.2     | 2026-04-27 | Gap analysis vs any-llm/litellm: add missing completion params, streaming async bridge spec, batch_completion() spec, router strategies, exception mapping, platform provider. Phase 4 added for full LiteLLM compat. Provider count 41→42 (added deepinfra). Clarify redis_url=N/A (stoolap replaces Redis per RFC-0912/0914); cache_responses uses stoolap semantic cache per RFC-0913. |
| 1.1     | 2026-04-27 | Fix all adversarial review issues: C2 (security model docs), C3 (raise error not silent fallback), C4 (ambiguity detection), C5 (case-insensitive provider lookup); I1 (G1=<10ms), I2 (stream=None), I3 (list_models spec), I4 (typed CompletionResponse), I5 (session_label docs), I6 (client_args schema), I7 (error codes), I8 (GIL isolation); L1 (Phase 1 clarify), L2 (deployment mode), L3 (batch API), L4 (RFC-0904 required) |
| 1.0     | 2026-04-27 | Initial draft |

## Related RFCs

- RFC-0908: Python SDK PyO3 Bindings (Superseded)
- RFC-0917: Dual-Mode Query Router (Defines feature gates)

## Related Use Cases

- `docs/use-cases/enhanced-quota-router-gateway.md`
