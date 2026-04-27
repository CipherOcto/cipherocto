# RFC-0920 (Economics): Unified Python SDK — Dual-Mode LiteLLM/any-llm Compatibility

## Status

Draft

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu

## Summary

Define a unified Python SDK via PyO3 that supports **both** LiteLLM-style API (separate `provider` param) and any-llm-style API (`provider:model` format), enabling drop-in replacement for both LiteLLM and any-llm users. This RFC supersedes RFC-0908 and extends RFC-0917 Phase 3 to provide a single SDK that works in both deployment modes.

## Dependencies

**Requires:**

- RFC-0917: Dual-Mode Query Router (Accepted)

**Optional:**

- RFC-0902: Multi-Provider Routing and Load Balancing
- RFC-0903: Virtual API Key System
- RFC-0904: Real-Time Cost Tracking
- RFC-0909: Deterministic Quota Accounting
- RFC-0910: Pricing Table Registry

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

| Goal | Target | Metric |
|------|--------|--------|
| G1 | <10ms function call overhead | Latency (PyO3 call into Rust) |
| G2 | 100% LiteLLM API compatibility | Test coverage against LiteLLM test suite |
| G3 | 100% any-llm API compatibility | Test coverage against any-llm test suite |
| G4 | pip installable | PyPI package |
| G5 | Type hints | mypy pass |
| G6 | Both API styles work | `completion(provider="openai", ...)` AND `completion(model="openai:gpt-4o", ...)` |

## Specification

### Dual-Mode Architecture

The SDK operates in two modes determined at **deployment time** (via feature flags), not runtime:

```
┌─────────────────────────────────────────────────────────────────┐
│                    quota_router Python SDK                       │
│                                                                 │
│  ┌─────────────────────┐    ┌─────────────────────┐            │
│  │   LiteLLM Mode      │    │    any-llm Mode     │            │
│  │   (feature flag)    │    │   (feature flag)    │            │
│  │                     │    │                     │            │
│  │ completion(        │    │ completion(         │            │
│  │   provider="openai",│    │   model="openai:...",│           │
│  │   model="gpt-4o",  │    │   messages=[...]   │            │
│  │   messages=[...]   │    │ )                   │            │
│  │ )                   │    │                     │            │
│  └─────────────────────┘    └─────────────────────┘            │
│                                                                 │
│  Both modes use the same underlying PyO3 → Rust provider calls   │
└─────────────────────────────────────────────────────────────────┘
```

**Key insight**: The SDK **accepts both calling conventions** regardless of mode. Mode determines:
1. Which provider integration strategy is compiled (reqwest HTTP vs PyO3 SDK)
2. Default behavior when `provider` param is absent

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

    # Completion parameters
    temperature: Optional[float] = None,
    top_p: Optional[float] = None,
    max_tokens: Optional[int] = None,
    max_completion_tokens: Optional[int] = None,
    n: Optional[int] = None,
    stream: Optional[bool] = False,
    stream_options: Optional[Dict] = None,
    stop: Optional[Union[str, List[str]]] = None,
    presence_penalty: Optional[float] = None,
    frequency_penalty: Optional[float] = None,
    logit_bias: Optional[Dict[int, float]] = None,
    user: Optional[str] = None,
    seed: Optional[int] = None,

    # Reasoning (Anthropic, OpenAI o1)
    reasoning_effort: Optional[str] = None,

    # Tools / Function calling
    tools: Optional[List[Dict]] = None,
    tool_choice: Optional[Union[str, Dict]] = None,
    parallel_tool_calls: Optional[bool] = None,

    # Response format (structured output)
    response_format: Optional[Union[str, Dict]] = None,

    # LiteLLM extras
    logprobs: Optional[bool] = None,
    top_logprobs: Optional[int] = None,
    session_label: Optional[str] = None,
    client_args: Optional[Dict] = None,

    # Remaining kwargs passed to provider
    **kwargs,
) -> Dict[str, Any]:
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
def resolve_provider(
    provider_param: Optional[str],
    model: str,
    deployment_mode: str,  # "litellm-mode" | "any-llm-mode" | "full"
) -> tuple[str, str]:
    """
    Returns (provider, model_name).

    Resolution priority:
    1. provider param if provided and non-empty
    2. Parse model string for "provider:model" or "provider/model" format
    3. Use default provider for deployment mode

    Raises:
        MissingProviderError: If no provider can be determined
    """
    # 1. Explicit provider param wins
    if provider_param:
        return provider_param, model

    # 2. Parse model string
    if ":" in model:
        provider, model_name = model.split(":", 1)
        if is_known_provider(provider):
            return provider, model_name
    if "/" in model:
        provider, model_name = model.split("/", 1)
        if is_known_provider(provider):
            return provider, model_name

    # 3. Default provider
    if deployment_mode == "litellm-mode":
        return "openai", model  # LiteLLM default
    elif deployment_mode == "any-llm-mode":
        return "openai", model   # any-llm default
    else:  # "full"
        return "openai", model  # Default to OpenAI

    raise MissingProviderError(f"Cannot determine provider for model: {model}")
```

### Supported Providers (41)

Both modes support identical 41 providers:

```
openai, anthropic, mistral, ollama, gemini,
azure, azureopenai, azureanthropic, bedrock, cerebras,
cohere, dashscope, databricks, deepseek, fireworks,
gateway, groq, huggingface, inception, llama,
llamacpp, llamafile, lmstudio, minimax, moonshot,
mzai, nebius, openrouter, perplexity, platform,
portkey, sagemaker, sambanova, together, vertexai,
vertexaianthropic, vllm, voyage, watsonx, xai, zai
```

### Exception Hierarchy

Matches LiteLLM exceptions + quota-router specifics:

```python
# quota_router/exceptions.py
class QuotaRouterError(Exception):
    """Base exception for all quota-router errors."""
    provider: Optional[str]
    code: str

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

class UnsupportedProviderError(QuotaRouterError):
    """Provider not supported."""
    pass

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
```

### Embedded API (any-llm Style)

For any-llm compatibility, the SDK must be configured before use:

```python
from quota_router import set_api_key, get_budget_status

# Set API key for a provider (any-llm style)
set_api_key("anthropic", "sk-ant-...")
set_api_key("openai", "sk-...")

# Check budget status
budget = get_budget_status()
print(f"OCTO-W Balance: {budget['balance']}")

# Get metrics
metrics = get_metrics()
print(f"Total spend: {metrics['total_spend']}")
```

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

## Feature Gate Architecture

Per RFC-0917 §Rust Feature Gates:

```toml
# Cargo.toml for quota-router-pyo3
[features]
litellm-mode = ["pyo3/extension-module"]
any-llm-mode = ["pyo3/extension-module"]
full = ["pyo3/extension-module"]  # Both modes

# Per RFC-0917 §Rust Feature Gates:
# - litellm-mode:  HTTP proxy only (hyper/axum compiled, py-o3 NOT for HTTP)
#                  Python SDK with provider param, OpenAI-compatible interface
# - any-llm-mode: Python SDK only (py-o3 compiled)
#                  provider:model format, set_api_key() style
# - full:         BOTH (both compiled)
```

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

# Backwards compatibility alias
litellm = sys.modules[__name__]  # Enables: import quota_router as litellm
```

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| Function call overhead | <5ms | PyO3 → Rust call latency |
| SDK import time | <100ms | Cold import |
| Memory per provider | <10MB | Cached Python client |

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

1. **API key handling**: Keys stored in memory only, never persisted
2. **Environment variable fallback**: Standard env var pattern (`OPENAI_API_KEY`, etc.)
3. **Provider isolation**: Each provider's SDK runs in separate PyO3 GIL boundary
4. **Input validation**: All parameters validated before passing to provider SDK

## Comparison with RFC-0908

| Aspect | RFC-0908 | RFC-0920 |
|--------|----------|----------|
| LiteLLM compatibility | ✅ Yes | ✅ Yes |
| any-llm compatibility | ❌ No | ✅ Yes |
| `provider` param | ✅ Separate | ✅ Both styles |
| `provider:model` format | ❌ No | ✅ Yes |
| `set_api_key()` style | ❌ No | ✅ Yes |
| Router class | ✅ Yes | ✅ Yes |
| 41 providers | Partial | ✅ All 41 |

## Implementation Phases

### Phase 1: Core SDK (Foundation)

- [ ] PyO3 Rust core with unified `acompletion()` signature
- [ ] Provider resolution algorithm (both styles)
- [ ] Exception hierarchy
- [ ] OpenAI provider integration (real SDK calls)
- [ ] Basic test suite

### Phase 2: Full Provider Coverage

- [ ] Anthropic provider integration
- [ ] Mistral provider integration
- [ ] All 41 providers (mock until real SDK available)
- [ ] Embedding API
- [ ] Model listing

### Phase 3: Enterprise Features

- [ ] Router class
- [ ] Batch API
- [ ] Responses API
- [ ] Messages API
- [ ] Budget/metrics APIs

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-pyo3/src/lib.rs` | Unified SDK exports |
| `crates/quota-router-pyo3/src/completion.rs` | Dual-mode completion |
| `crates/quota-router-pyo3/src/providers/` | Provider implementations |
| `crates/quota-router-pyo3/src/exceptions.rs` | Exception hierarchy |
| `crates/quota-router-pyo3/src/sdk.rs` | set_api_key, budget APIs |

## Future Work

- F1: LangChain integration
- F2: LlamaIndex integration
- F3: Streaming SSE normalization
- F4: Response caching (RFC-0906)

## Rationale

The dual-style approach maximizes adoption by meeting users where they are:
- LiteLLM users keep their `provider=` param pattern
- any-llm users keep their `provider:model` pattern
- Both can coexist in the same codebase

This is the only approach that achieves true drop-in replacement for both ecosystems.

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-04-27 | Initial draft |

## Related RFCs

- RFC-0908: Python SDK PyO3 Bindings (Superseded)
- RFC-0917: Dual-Mode Query Router (Defines feature gates)

## Related Use Cases

- `docs/use-cases/enhanced-quota-router-gateway.md`
