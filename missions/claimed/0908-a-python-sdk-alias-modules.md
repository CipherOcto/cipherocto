# Mission: 0908-a — Python SDK Alias Modules

## Status

Open

## RFC

RFC-0908 (Economics): Python SDK and PyO3 Bindings

## Context

LiteLLM users do `import litellm` and any-llm users do `import any_llm`. quota-router must support both aliases so users can switch with zero code changes.

## Acceptance Criteria

### Alias Module Structure

- [ ] Create `python/quota_router/__init__.py` with full API surface
- [ ] Create `python/quota_router/litellm.py` that re-exports everything
- [ ] Create `python/quota_router/any_llm.py` that re-exports everything
- [ ] Support `import quota_router as litellm` pattern
- [ ] Support `from quota_router import completion, acompletion, Router`

### API Surface

- [ ] Export `completion()`, `acompletion()` — chat completions
- [ ] Export `text_completion()`, `atext_completion()` — legacy completions
- [ ] Export `embedding()`, `aembedding()` — embeddings
- [ ] Export `messages()`, `amessages()` — Anthropic Messages API
- [ ] Export `responses()`, `aresponses()` — OpenAI Responses API
- [ ] Export `Router` class
- [ ] Export `set_api_key()`, `get_budget_status()`, `get_metrics()`
- [ ] Export `get_supported_providers()`, `is_provider_supported()`

### Exception Classes

- [ ] Export `AuthenticationError` (401)
- [ ] Export `RateLimitError` (429)
- [ ] Export `BudgetExceededError` (403)
- [ ] Export `InvalidRequestError` (400)
- [ ] Export `ContextWindowExceededError` (400)
- [ ] Export `ContentPolicyViolationError` (400)
- [ ] Export `TimeoutError` (408)
- [ ] Export `ProviderError` (500+)
- [ ] Export `ServiceUnavailableError` (503)
- [ ] Export `APIConnectionError` (502)
- [ ] Export `APIError` (500)
- [ ] Export `NotFoundError` (404)

### Global Settings

- [ ] Support `litellm.drop_params = True`
- [ ] Support `litellm.set_verbose = True`
- [ ] Support `litellm.api_key = "sk-..."`
- [ ] Support `litellm.api_base = "https://..."`
- [ ] Support `litellm.num_retries = 3`
- [ ] Support `litellm.request_timeout = 30`
- [ ] Support `litellm.cache = True`

## Files to Create

- `python/quota_router/__init__.py` — main API surface
- `python/quota_router/litellm.py` — litellm alias
- `python/quota_router/any_llm.py` — any-llm alias
- `python/quota_router/exceptions.py` — exception classes
- `python/quota_router/router.py` — Router class wrapper

## Verification

```bash
python -c "import quota_router as litellm; print(litellm.completion)"
python -c "from quota_router import Router, completion"
python -c "from quota_router.exceptions import AuthenticationError"
```
