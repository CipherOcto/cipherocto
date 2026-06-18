---
title: "RFC-0953: Extended Python SDK Functions"
status: Accepted
version: 0.2.2
created: 2026-05-18
updated: 2026-05-21
authors:
  - quota-router team
related:
  - RFC-0908 (Python SDK and PyO3 Bindings)
  - RFC-0920 (Unified Python SDK Dual-Mode Compatibility)
  - RFC-0917 (Dual-Mode Query Router)
  - RFC-0951 (Extended API Endpoints)
---

# RFC-0953: Extended Python SDK Functions

## Status

Accepted

## Summary

Implement batch_create(), responses(), and messages() Python SDK functions as specified in RFC-0920, providing thin PyO3 bindings to the Rust core.

> **ARCHITECTURAL CONSTRAINT:** Rust-owns-all-heavy-lifting. Python SDK is a THIN PYO3 BINDING LAYER ONLY. All batch orchestration, response state management, and message routing logic lives in Rust core (RFC-0917). Python SDK provides only the interface layer.

> **CRITICAL INVARIANT -- Mode Gate != Interface:** Both HTTP proxy and Python SDK exist in ALL modes. The mode gate controls HOW providers are called (reqwest vs PyO3), NOT WHETHER an interface exists.

## Dependencies

**Requires:**

- RFC-0908 (Economics): Python SDK and PyO3 Bindings
- RFC-0920 (Economics): Unified Python SDK Dual-Mode Compatibility (defines authoritative API surface)
- RFC-0917 (Economics): Dual-Mode Query Router (definitive source for all heavy lifting)

**Optional:**

- RFC-0951 (Economics): Extended API Endpoints
- RFC-0941 (Economics): Streaming Parity

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | Full litellm SDK surface | 63 exports |
| G2 | Drop-in replacement | Same function signatures |
| G3 | Type hints | Full typing support |
| G4 | Documentation | Docstrings + examples |

## Motivation

litellm's Python SDK provides 20+ functions. quota-router currently exports 60 symbols. The missing 3 functions block users who depend on:
- Batch processing (cost-efficient bulk requests)
- OpenAI Responses API (new stateful API)
- Anthropic Messages API (native format)

## Specification

### Function Signatures

All function signatures MUST match RFC-0920 (authoritative source). This RFC implements the batch/responses/messages surface defined there.

#### Batch Functions (per RFC-0920)

```python
# Sync variants
def batch_create(
    provider: str,
    input_file: Union[str, Path],
    endpoint: str,
    *,
    completion_window: str = "24h",
    metadata: Optional[Dict[str, str]] = None,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    **kwargs
) -> BatchCreateResponse:
    """Create a batch. Thin PyO3 binding to Rust core."""
    pass

def batch_retrieve(
    provider: str,
    batch_id: str,
    *,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    **kwargs
) -> BatchRetrieveResponse:
    """Get batch status. Thin PyO3 binding to Rust core."""
    pass

def batch_cancel(
    provider: str,
    batch_id: str,
    *,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    **kwargs
) -> BatchRetrieveResponse:
    """Cancel batch. Thin PyO3 binding to Rust core."""
    pass

def batch_list(
    provider: str,
    *,
    limit: Optional[int] = None,
    after: Optional[str] = None,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    **kwargs
) -> Sequence[BatchCreateResponse]:
    """List batches. Thin PyO3 binding to Rust core."""
    pass

def batch_results(
    provider: str,
    batch_id: str,
    *,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    **kwargs
) -> BatchResult:
    """Retrieve batch results (after completion). Thin PyO3 binding to Rust core."""
    pass

# Async variants (same signatures as sync, return same types)
async def abatch_create(...) -> BatchCreateResponse: ...
async def abatch_retrieve(...) -> BatchRetrieveResponse: ...
async def abatch_cancel(...) -> BatchRetrieveResponse: ...
async def abatch_list(...) -> Sequence[BatchCreateResponse]: ...
async def abatch_results(...) -> BatchResult: ...
```

#### Responses Functions (per RFC-0920)

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
    """Create response. Thin PyO3 binding to Rust core."""
    pass

def get_response(
    provider: str,
    response_id: str,
    *,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    **kwargs
) -> ResponsesAPIResponse:
    """Get response. Thin PyO3 binding to Rust core."""
    pass

def delete_response(
    provider: str,
    response_id: str,
    *,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    **kwargs
) -> ResponsesAPIResponse:
    """Delete response. Thin PyO3 binding to Rust core."""
    pass

# Async variants (same signatures as sync, return same types)
async def aresponses(...) -> ResponsesAPIResponse: ...
async def aget_response(...) -> ResponsesAPIResponse: ...
async def adelete_response(...) -> ResponsesAPIResponse: ...
```

#### Messages Functions (per RFC-0920)

```python
def messages(
    model: str,
    messages: List[Dict[str, Any]],
    max_tokens: int,
    *,
    provider: Optional[str] = None,
    system: Optional[Union[str, List[Dict[str, Any]]]] = None,
    temperature: Optional[float] = None,
    top_p: Optional[float] = None,
    top_k: Optional[int] = None,
    stream: Optional[bool] = None,
    stop_sequences: Optional[List[str]] = None,
    tools: Optional[List[Dict[str, Any]]] = None,
    tool_choice: Optional[Union[str, Dict]] = None,
    metadata: Optional[Dict[str, Any]] = None,
    thinking: Optional[Dict] = None,
    cache_control: Optional[Dict[str, Any]] = None,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    **kwargs
) -> AnthropicMessagesResponse:
    """Send message via Anthropic Messages API. Thin PyO3 binding to Rust core."""
    pass

# Async variant (same signature as sync, returns same type)
async def amessages(...) -> AnthropicMessagesResponse: ...
```

### Streaming Support

All functions MUST support streaming via generators:

```python
# Batch streaming (progress updates)
for event in qr.abatch_create(..., stream=True):
    print(event["type"], event["data"])

# Responses streaming
for chunk in qr.responses(..., stream=True):
    print(chunk["delta"], end="")

# Messages streaming
for chunk in qr.messages(..., stream=True):
    print(chunk["delta"], end="")
```

### Error Handling

All functions MUST raise litellm-compatible exceptions:

```python
try:
    qr.responses(model="gpt-4o", input="Hello")
except AuthenticationError:
    print("Invalid API key")
except RateLimitError:
    print("Rate limit exceeded")
except InvalidRequestError:
    print("Invalid request")
except ModelNotFoundError:
    print("Model not found")
```

### Type Definitions

```python
from typing import Dict, Any, List, Optional, Union, Generator

# Batch types
BatchRequest = Dict[str, Any]
BatchObject = Dict[str, Any]
BatchStatus = str  # "validating", "failed", "in_progress", "finalizing", "completed", "expired", "cancelled"

# Response types
InputItem = Union[str, Dict[str, Any]]
ResponseObject = Dict[str, Any]
ResponseOutput = Dict[str, Any]

# Message types
Message = Dict[str, Any]
ContentBlock = Dict[str, Any]
MessagesResponse = Dict[str, Any]
```

### PyO3 Bindings

> **Note:** PyO3 bindings MUST match the Python signatures above. The bindings below show the minimum required parameters. Additional parameters from the Python signatures are passed via `**kwargs` or explicit parameter forwarding.

```rust
#[pyfunction]
fn batch_create(
    py: Python,
    provider: &str,
    input_file: &str,
    endpoint: &str,
    completion_window: Option<&str>,
    metadata: Option<HashMap<String, String>>,
    api_key: Option<&str>,
    api_base: Option<&str>,
    client_args: Option<HashMap<String, PyObject>>,
) -> PyResult<PyObject> {
    // Convert Python objects to Rust types
    // Call HTTP proxy or py_bridge
    // Convert response back to Python
}

#[pyfunction]
fn responses(
    py: Python,
    model: &str,
    input: Option<PyObject>,
    input_data: Option<PyObject>,
    provider: Option<&str>,
    instructions: Option<&str>,
    tools: Option<Vec<PyObject>>,
    tool_choice: Option<PyObject>,
    max_output_tokens: Option<u32>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    stream: Option<bool>,
    include: Option<Vec<String>>,
    parallel_tool_calls: Option<bool>,
    previous_response_id: Option<String>,
    reasoning: Option<HashMap<String, PyObject>>,
    text: Option<HashMap<String, PyObject>>,
    presence_penalty: Option<f32>,
    frequency_penalty: Option<f32>,
    truncation: Option<HashMap<String, PyObject>>,
    service_tier: Option<String>,
    safety_identifier: Option<String>,
    background: Option<bool>,
    client_args: Option<HashMap<String, PyObject>>,
) -> PyResult<PyObject> {
    // Implementation
}

#[pyfunction]
fn messages(
    py: Python,
    model: &str,
    messages: Vec<PyObject>,
    max_tokens: u32,
    provider: Option<&str>,
    system: Option<PyObject>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    top_k: Option<u32>,
    stream: Option<bool>,
    stop_sequences: Option<Vec<String>>,
    tools: Option<Vec<PyObject>>,
    tool_choice: Option<PyObject>,
    metadata: Option<HashMap<String, PyObject>>,
    thinking: Option<HashMap<String, PyObject>>,
    cache_control: Option<HashMap<String, PyObject>>,
    api_key: Option<&str>,
    api_base: Option<&str>,
    client_args: Option<HashMap<String, PyObject>>,
) -> PyResult<PyObject> {
    // Implementation
}

#[pyfunction]
fn get_response(
    py: Python,
    provider: &str,
    response_id: &str,
    api_key: Option<&str>,
    api_base: Option<&str>,
    client_args: Option<HashMap<String, PyObject>>,
) -> PyResult<PyObject> {
    // Implementation
}

#[pyfunction]
fn delete_response(
    py: Python,
    provider: &str,
    response_id: &str,
    api_key: Option<&str>,
    api_base: Option<&str>,
    client_args: Option<HashMap<String, PyObject>>,
) -> PyResult<PyObject> {
    // Implementation
}
```

## Acceptance Criteria

- [ ] batch_create() creates batch and returns batch ID
- [ ] batch_retrieve() returns batch status and results
- [ ] batch_cancel() cancels running batch
- [ ] batch_list() returns list of batches
- [ ] batch_results() retrieves batch results after completion
- [ ] responses() creates response via OpenAI Responses API
- [ ] get_response() returns response by ID
- [ ] delete_response() deletes response
- [ ] messages() sends message via Anthropic Messages API
- [ ] All functions support streaming (where applicable)
- [ ] All functions raise RFC-0920 compatible exceptions
- [ ] Type hints work with mypy
- [ ] All existing tests pass

## Test Specifications

### Test File

`tests/test_extended_sdk.py` — covers all functions defined in this RFC.

### messages() Tests

**Signature Compliance:**
- `test_messages_max_tokens_required`: `messages(model, messages)` without max_tokens raises TypeError
- `test_messages_max_tokens_positional`: `messages("claude-sonnet-4-20250514", [...], 100)` — max_tokens is 3rd positional
- `test_messages_system_optional`: `messages(model, messages, max_tokens, system="...")` — system is optional
- `test_messages_system_union_type`: system accepts `str` and `list[dict]` (Anthropic content blocks)
- `test_messages_stream_keyword`: `messages(model, messages, max_tokens, stream=True)` — uses `stream` not `streaming`
- `test_messages_stop_sequences_keyword`: `messages(model, messages, max_tokens, stop_sequences=["END"])` — uses `stop_sequences` not `stop`
- `test_messages_thinking_optional`: `messages(model, messages, max_tokens, thinking={"type": "enabled", "budget_tokens": 1000})`
- `test_messages_cache_control_optional`: `messages(model, messages, max_tokens, cache_control={"type": "ephemeral"})`
- `test_messages_provider_optional`: `messages(model, messages, max_tokens, provider="anthropic")`

**Behavior:**
- `test_messages_routes_to_anthropic`: messages() uses Anthropic Messages API, not /v1/chat/completions
- `test_messages_response_type`: response is `AnthropicMessagesResponse` with `.id`, `.content`, `.model` fields
- `test_messages_streaming_returns_iterator`: `messages(..., stream=True)` returns iterator of `MessageStreamEvent`

**Error Handling:**
- `test_messages_invalid_model`: raises `ModelNotFoundError`
- `test_messages_no_api_key`: raises `AuthenticationError` or `MissingApiKeyError`
- `test_messages_content_filter`: raises `ContentFilterError` for safety violations

**Async:**
- `test_amessages_returns_coroutine`: `amessages(...)` returns a coroutine
- `test_amessages_same_result_as_sync`: `await amessages(...)` matches `messages(...)`

### responses() Tests

**Signature Compliance:**
- `test_responses_litellm_convention`: `responses(model="...", input="Hello")` — litellm uses `input=`
- `test_responses_anyllm_convention`: `responses(model="...", input_data="Hello")` — any-llm uses `input_data=`
- `test_responses_both_params_error`: `responses(model="...", input="a", input_data="b")` raises error
- `test_responses_neither_param_error`: `responses(model="...")` raises error
- `test_responses_max_output_tokens`: uses `max_output_tokens` not `max_tokens`
- `test_responses_instructions_optional`: `responses(model, input, instructions="Be helpful")`
- `test_responses_tools_optional`: `responses(model, input, tools=[...])`
- `test_responses_stream_optional`: `responses(model, input, stream=True)`

**Behavior:**
- `test_responses_uses_openai_endpoint`: responses() uses /v1/responses, not /v1/chat/completions
- `test_responses_response_type`: response is `ResponsesAPIResponse` with `.id`, `.output`, `.model` fields
- `test_responses_streaming_returns_iterator`: `responses(..., stream=True)` returns iterator of `ResponseStreamEvent`

**Error Handling:**
- `test_responses_invalid_model`: raises `ModelNotFoundError`
- `test_responses_return_type`: returns `ResponsesAPIResponse` with `.id`, `.output`, `.status` fields

### batch Functions Tests

**batch_create():**
- `test_batch_create_required_params`: `batch_create(provider, input_file, endpoint)` — all required
- `test_batch_create_no_model`: batch_create does NOT accept `model` param
- `test_batch_create_endpoint_required`: `batch_create(provider, input_file)` without endpoint raises TypeError
- `test_batch_create_completion_window_optional`: `batch_create(provider, input_file, endpoint, completion_window="24h")`
- `test_batch_create_metadata_optional`: `batch_create(provider, input_file, endpoint, metadata={"key": "value"})`
- `test_batch_create_return_type`: returns `BatchCreateResponse` with `.batch_id`, `.status` fields

**batch_retrieve():**
- `test_batch_retrieve_param_order`: `batch_retrieve(provider, batch_id)` — provider first
- `test_batch_retrieve_provider_required`: `batch_retrieve(batch_id="...")` without provider raises TypeError
- `test_batch_retrieve_return_type`: returns `BatchRetrieveResponse` with `.status`, `.output_file_id` fields

**batch_cancel():**
- `test_batch_cancel_param_order`: `batch_cancel(provider, batch_id)` — provider first
- `test_batch_cancel_return_type`: returns `BatchRetrieveResponse`

**batch_list():**
- `test_batch_list_provider_required`: `batch_list(provider)` — provider required
- `test_batch_list_limit_optional`: `batch_list(provider, limit=10)` — limit optional
- `test_batch_list_after_optional`: `batch_list(provider, after="batch_123")` — pagination
- `test_batch_list_return_type`: returns `Sequence[BatchCreateResponse]`

**batch_results():**
- `test_batch_results_param_order`: `batch_results(provider, batch_id)` — provider first
- `test_batch_results_return_type`: returns `BatchResult` with `.results` list
- `test_batch_results_not_complete_error`: raises `BatchNotCompleteError` if batch not done

**Async Variants:**
- `test_abatch_create_returns_coroutine`: `abatch_create(...)` returns coroutine
- `test_abatch_retrieve_returns_coroutine`: `abatch_retrieve(...)` returns coroutine
- `test_abatch_cancel_returns_coroutine`: `abatch_cancel(...)` returns coroutine
- `test_abatch_list_returns_coroutine`: `abatch_list(...)` returns coroutine
- `test_abatch_results_returns_coroutine`: `abatch_results(...)` returns coroutine

### get_response() / delete_response() Tests

**get_response():**
- `test_get_response_param_order`: `get_response(provider, response_id)` — provider first
- `test_get_response_return_type`: returns `ResponsesAPIResponse`
- `test_get_response_not_found`: raises `ModelNotFoundError` for invalid ID

**delete_response():**
- `test_delete_response_param_order`: `delete_response(provider, response_id)` — provider first
- `test_delete_response_return_type`: returns `ResponsesAPIResponse`
- `test_delete_response_not_found`: raises `ModelNotFoundError` for invalid ID

### Streaming Tests

- `test_messages_streaming_yields_events`: streaming messages yields `MessageStreamEvent` objects
- `test_responses_streaming_yields_events`: streaming responses yields `ResponseStreamEvent` objects
- `test_streaming_error_propagation`: errors during streaming are propagated correctly
- `test_streaming_cancellation`: cancelling a stream stops iteration

### Cross-RFC Consistency Tests

- `test_0953_batch_create_matches_0920`: RFC-0953 batch_create signature matches RFC-0920
- `test_0953_responses_matches_0920`: RFC-0953 responses signature matches RFC-0920
- `test_0953_messages_matches_0920`: RFC-0953 messages signature matches RFC-0920
- `test_0953_batch_results_exists`: batch_results() is defined in RFC-0953 (not just RFC-0920)

## Key Files

| File | Change |
|------|--------|
| `crates/quota-router-core/src/py_bridge/batch.rs` | New - batch functions |
| `crates/quota-router-core/src/py_bridge/responses.rs` | New - responses functions |
| `crates/quota-router-core/src/py_bridge/messages.rs` | New - messages functions |
| `crates/quota-router-core/src/py_bridge/mod.rs` | Register new functions |
| `python/quota_router/__init__.py` | Export new functions |
| `python/quota_router/batch.py` | New - batch module |
| `python/quota_router/responses.py` | New - responses module |
| `python/quota_router/messages.py` | New - messages module |

## Security Considerations

- API keys MUST be passed securely (not logged)
- Batch file uploads MUST be validated
- Response/message content MUST be sanitized for logging

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 0.2.2 | 2026-05-21 | **SPEC** Added get_response()/delete_response() PyO3 bindings with provider first param. |
| 0.2.1 | 2026-05-21 | **FIX** H2: Expanded responses() PyO3 binding from 12 to 24 params to match Python signature. |
| 0.2.0 | 2026-05-21 | **FIX** C4/C5: Updated PyO3 bindings to match Python signatures (batch_create removed model, responses/messages expanded to full param sets). **FIX** H2: Added version history entry. |
| 0.1.0 | 2026-05-18 | Initial draft |
