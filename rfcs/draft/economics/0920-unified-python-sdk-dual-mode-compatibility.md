# RFC-0920 (Economics): Unified Python SDK — Dual-Mode LiteLLM/any-llm Compatibility

## Status

Draft (v1.2 — 2026-04-27)

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

**Optional:**

- RFC-0902: Multi-Provider Routing and Load Balancing
- RFC-0903: Virtual API Key System
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

| Goal | Target                         | Metric                                                                            |
| ---- | ------------------------------ | --------------------------------------------------------------------------------- |
| G1   | <10ms function call overhead   | Latency (PyO3 call into Rust)                                                     |
| G2   | 100% LiteLLM API compatibility | Test coverage against LiteLLM test suite                                          |
| G3   | 100% any-llm API compatibility | Test coverage against any-llm test suite                                          |
| G4   | pip installable                | PyPI package                                                                      |
| G5   | Type hints                     | mypy pass                                                                         |
| G6   | Both API styles work           | `completion(provider="openai", ...)` AND `completion(model="openai:gpt-4o", ...)` |

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
    stream: Optional[bool] = None,  # None = sync response; True = streaming; matches LiteLLM behavior
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
    if "/" in model:
        provider, model_name = model.split("/", 1)
        provider_lower = provider.lower()
        if is_known_provider(provider_lower):
            # Ambiguity check
            if model_name.lower() == provider_lower:
                import warnings
                warnings.warn(
                    f"Ambiguous model string '{model}' — provider and model name are identical. "
                    f"Assuming provider='{provider_lower}', model='{model_name}'.",
                    UserWarning,
                    stacklevel=2,
                )
            return provider_lower, model_name

    # 3. No provider determined — raise error (NOT silent fallback)
    raise MissingProviderError(
        f"Cannot determine provider for model '{model}'. "
        f"Use provider='<name>' parameter or prefix model with '<provider>:' (e.g., 'openai:gpt-4o'). "
        f"Known providers: {', '.join(sorted(KNOWN_PROVIDERS))}"
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

### list_models() Signature

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

This section documents known gaps between RFC-0920 and the reference implementations (any-llm, litellm) that may need to be addressed in future phases.

#### Completion Parameters Gap

**Parameters present in litellm but missing from RFC-0920:**

| Parameter                       | Type                            | Description                                | Priority |
| ------------------------------- | ------------------------------- | ------------------------------------------ | -------- |
| `timeout`                       | `float \| str \| httpx.Timeout` | Request timeout with httpx.Timeout support | Medium   |
| `extra_headers`                 | `dict`                          | Additional headers to pass to provider     | Low      |
| `base_url`                      | `str`                           | Alias for `api_base` (LiteLLM convention)  | Low      |
| `api_version`                   | `str`                           | API version for Azure-style providers      | Low      |
| `model_list`                    | `list`                          | Alternative model configuration            | Medium   |
| `web_search_options`            | `dict`                          | Web search for supported providers         | Low      |
| `modalities`                    | `list`                          | Audio output modalities                    | Low      |
| `audio`                         | `dict`                          | Audio parameters                           | Low      |
| `prediction`                    | `dict`                          | Prediction content for o1 models           | Low      |
| `thinking`                      | `dict`                          | Anthropic extended thinking budget         | Medium   |
| `shared_session`                | `ClientSession`                 | Shared httpx session                       | Low      |
| `enable_json_schema_validation` | `bool`                          | Validate response vs schema                | Low      |

**Parameters present in any-llm but missing from RFC-0920:**

| Parameter                | Type          | Description                        | Priority |
| ------------------------ | ------------- | ---------------------------------- | -------- |
| `system`                 | `str \| list` | System message(s) for messages API | Medium   |
| `top_k`                  | `int`         | Top-k sampling for Anthropic       | Low      |
| `truncation`             | `str`         | Cohere truncation strategy         | Low      |
| `service_tier`           | `str`         | Azure OpenAI service tier          | Low      |
| `background`             | `bool`        | Run request in background          | Low      |
| `safety_identifier`      | `str`         | Content safety category            | Low      |
| `prompt_cache_key`       | `str`         | Prompt caching key                 | Low      |
| `prompt_cache_retention` | `str`         | Prompt cache TTL                   | Low      |
| `conversation`           | `str`         | Conversation ID for continuity     | Low      |

#### Streaming Gap

**Gap severity: High**

| Aspect               | litellm               | any-llm                              | RFC-0920    |
| -------------------- | --------------------- | ------------------------------------ | ----------- |
| Sync stream return   | `CustomStreamWrapper` | `Iterator[ChatCompletionChunk]`      | Mock chunks |
| Async stream return  | `AsyncIterator`       | `AsyncIterator[ChatCompletionChunk]` | Mock chunks |
| Sync-to-async bridge | N/A                   | `async_iter_to_sync_iter()`          | Missing     |

**any-llm async bridge implementation** (`any-llm/src/any_llm/utils/aio.py`):

```python
def async_iter_to_sync_iter(async_iter, timeout=60):
    """Bridge async iterator to sync iterator using background thread."""
    queue = queue.Queue(maxsize=1)
    exception = [None]

    def consume_async():
        try:
            import asyncio
            loop = asyncio.new_event_loop()
            asyncio.set_event_loop(loop)
            async def run():
                async for item in async_iter:
                    queue.put(item, timeout=timeout)
                queue.put(StopIteration, timeout=timeout)
            loop.run_until_complete(run())
        except Exception as e:
            exception[0] = e
            queue.put(StopIteration, timeout=timeout)

    thread = threading.Thread(target=consume_async, daemon=True)
    thread.start()

    while True:
        item = queue.get(timeout=timeout * 2)
        if isinstance(item, type(StopIteration)):
            if exception[0]:
                raise exception[0]
            break
        yield item
```

RFC-0920 should adopt any-llm's async bridge pattern for sync streaming compatibility.

#### Batch API Gap

**Gap severity: High**

| Feature                          | litellm | any-llm | RFC-0920   |
| -------------------------------- | ------- | ------- | ---------- |
| `batch_completion()` (in-memory) | ✅      | ❌      | ❌ Missing |
| `batch_completion_models()`      | ✅      | ❌      | ❌ Missing |
| `input_file_path` (local file)   | ❌      | ✅      | ✅ Spec'd  |

**litellm `batch_completion()` signature** (`litellm/litellm/batch_completion/main.py`):

```python
def batch_completion(
    model: str,
    messages: List,
    functions: Optional[List] = None,
    function_call: Optional[str] = None,
    temperature: Optional[float] = None,
    top_p: Optional[float] = None,
    n: Optional[int] = None,
    stream: Optional[bool] = None,
    stop=None,
    max_tokens: Optional[int] = None,
    presence_penalty: Optional[float] = None,
    frequency_penalty: Optional[float] = None,
    logit_bias: Optional[dict] = None,
    user: Optional[str] = None,
    deployment_id=None,
    request_timeout: Optional[int] = None,
    timeout: Optional[int] = 600,
    max_workers: Optional[int] = 100,  # Parallelism
    **kwargs,
) -> List[response]
```

This is distinct from the file-based Batch API. RFC-0920 should add:

```python
def batch_completion(
    model: str,
    messages: List[Dict],
    *,
    temperature: Optional[float] = None,
    max_tokens: Optional[int] = None,
    n: Optional[int] = None,
    timeout: Optional[int] = 600,
    max_workers: int = 100,
    **kwargs,
) -> List[CompletionResponse]:
    """
    Submit multiple completion requests in parallel.
    Returns list of responses in same order as inputs.
    """
```

#### Router Gap

**Gap severity: High (implementation incomplete)**

| Feature                         | litellm         | any-llm | RFC-0920   |
| ------------------------------- | --------------- | ------- | ---------- |
| Load balancing strategies       | ✅ 6 strategies | ❌      | ✅ Spec'd  |
| `cache_responses`               | ✅              | ❌      | ❌ Missing |
| `redis_url`                     | ✅              | ❌      | ❌ Missing |
| `num_retries` per call          | ✅              | ❌      | ❌ Missing |
| `logger_fn`                     | ✅              | ❌      | ❌ Missing |
| `enable_json_schema_validation` | ✅              | ❌      | ❌ Missing |

**litellm routing strategies** (`litellm/router.py`):

```python
routing_strategy: Literal[
    "simple-shuffle",      # Random selection
    "least-busy",          # Fewest active requests
    "usage-based-routing", # Lowest usage
    "latency-based-routing", # Lowest latency
    "cost-based-routing",  # Lowest cost
    "usage-based-routing-v2",
] = "simple-shuffle"
```

RFC-0920 Router implementation should include these strategies.

#### Exception Mapping Gap (any-llm Style)

**Gap severity: Medium**

any-llm provides unified exception mapping via regex patterns (`any-llm/src/any_llm/utils/exception_handler.py`). When `ANY_LLM_UNIFIED_EXCEPTIONS=1`:

```python
EXCEPTION_PATTERNS = [
    (r"invalid_api_key", "AuthenticationError"),
    (r"incorrect_api_key", "AuthenticationError"),
    (r"rate_limit", "RateLimitError"),
    (r"context_length", "ContextLengthExceededError"),
    (r"model_not_found", "ModelNotFoundError"),
    (r"content_filter", "ContentFilterError"),
    # ... more patterns
]
```

RFC-0920 should add an optional unified exception mapping mode for any-llm compatibility.

#### Platform Provider (any-api Style)

**Gap severity: Medium**

any-llm supports a `PlatformProvider` that wraps any provider with an any-api format key (`any-...`). This allows generic API key handling for providers not explicitly supported.

RFC-0920 does not currently spec this. If needed, add:

```python
class PlatformProvider:
    """Wrapper for any-api format keys."""
    def __init__(self, api_key: str, **kwargs):
        # Parse any-... format, extract underlying provider
        platform_key = PlatformKey(api_key=api_key)
        self.provider = PROVIDER_MAP[platform_key.provider](**kwargs)
```

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

Per RFC-0917 §Rust Feature Gates:

```toml
# Cargo.toml for quota-router-pyo3
[features]
litellm-mode = ["pyo3/extension-module"]
any-llm-mode = ["pyo3/extension-module"]
full = ["pyo3/extension-module"]  # Both modes

# Per RFC-0917 §Rust Feature Gates:
# - litellm-mode:  Uses reqwest (native Rust HTTP) to call providers
# - any-llm-mode: Uses PyO3 (official Python SDKs) to call providers
# - full:         Uses both reqwest AND PyO3 simultaneously
# NOTE: Both HTTP proxy AND Python SDK interfaces are available in ALL modes.
```

### Deployment Mode Selection

Mode is selected at **build time** via Cargo feature flags:

| Installation                                   | Mode           | Provider Strategy     |
| ---------------------------------------------- | -------------- | --------------------- |
| `pip install quota-router` (from PyPI, wheels) | `full`         | Both (reqwest + PyO3) |
| `cargo build --features litellm-mode`          | `litellm-mode` | reqwest only          |
| `cargo build --features any-llm-mode`          | `any-llm-mode` | PyO3 only             |
| `cargo build --features full` (default)        | `full`         | Both                  |

**Runtime detection:** The SDK exposes `quota_router.get_deployment_mode()`:

```python
import quota_router
mode = quota_router.get_deployment_mode()
# Returns: "litellm-mode" | "any-llm-mode" | "full"
```

**API style is independent of mode:** Both `provider=...` and `provider:model` calling conventions work in all modes.

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
- [ ] **Replace mock with real PyO3 SDK calls** — current `quota-router-pyo3` completion functions are mock stubs that echo messages
- [ ] Basic test suite
- [ ] Async iterator bridge for sync streaming (`async_iter_to_sync_iter()`)

**Note:** Phase 1 MUST replace the current mock implementations with real provider SDK calls via PyO3.

### Phase 2: Full Provider Coverage

- [ ] Anthropic provider integration (with `thinking` and `cache_control` support)
- [ ] Mistral provider integration
- [ ] All 42 providers (mock until real SDK available)
- [ ] Embedding API
- [ ] Model listing
- [ ] `timeout` parameter with httpx.Timeout support
- [ ] `extra_headers`, `base_url`, `api_version` parameters

### Phase 3: Enterprise Features

- [ ] Router class with load balancing strategies
- [ ] `batch_completion()` and `batch_completion_models()` (in-memory parallel batch)
- [ ] Batch API (file-based)
- [ ] Responses API
- [ ] Messages API (with `system`, `top_k`, `truncation` support)
- [ ] Budget/metrics APIs
- [ ] `cache_responses` support
- [ ] `redis_url` for distributed caching
- [ ] `num_retries` per-call retry logic
- [ ] `logger_fn` custom logger
- [ ] Exception regex mapping mode (`ANY_LLM_UNIFIED_EXCEPTIONS=1`)
- [ ] Platform provider (any-api key format)

### Phase 4: Full LiteLLM Compatibility (Future)

- [ ] Remaining litellm-only parameters: `modalities`, `audio`, `prediction`, `web_search_options`, `shared_session`
- [ ] All litellm routing strategies (6 total)
- [ ] `enable_json_schema_validation`
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
- F3: Streaming SSE normalization
- F4: Response caching (RFC-0906)

## Rationale

The dual-style approach maximizes adoption by meeting users where they are:

- LiteLLM users keep their `provider=` param pattern
- any-llm users keep their `provider:model` pattern
- Both can coexist in the same codebase

This is the only approach that achieves true drop-in replacement for both ecosystems.

## Version History

| Version | Date       | Changes                                                                                                                                                                                                                                                                                                                                                                                                                               |
| ------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1.2     | 2026-04-27 | Gap analysis vs any-llm/litellm: add missing completion params (timeout, thinking, system, etc.), streaming async bridge spec, batch_completion() spec, router strategies, exception mapping, platform provider. Phase 4 added for full LiteLLM compat. Provider count 41→42 (added deepinfra).                                                                                                                                       |
| 1.1     | 2026-04-27 | Fix all adversarial review issues: C2 (security model docs), C3 (raise error not silent fallback), C4 (ambiguity detection), C5 (case-insensitive provider lookup); I1 (G1=<10ms), I2 (stream=None), I3 (list_models spec), I4 (typed CompletionResponse), I5 (session_label docs), I6 (client_args schema), I7 (error codes), I8 (GIL isolation); L1 (Phase 1 clarify), L2 (deployment mode), L3 (batch API), L4 (RFC-0904 required) |
| 1.0     | 2026-04-27 | Initial draft                                                                                                                                                                                                                                                                                                                                                                                                                         |

## Related RFCs

- RFC-0908: Python SDK PyO3 Bindings (Superseded)
- RFC-0917: Dual-Mode Query Router (Defines feature gates)

## Related Use Cases

- `docs/use-cases/enhanced-quota-router-gateway.md`
