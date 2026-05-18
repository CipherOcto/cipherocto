---
title: "RFC-0953: Extended Python SDK Functions"
status: Accepted
version: 0.1.0
created: 2026-05-18
updated: 2026-05-18
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
    model: str = "gpt-4o",
    endpoint: str = "/v1/chat/completions",
    completion_window: str = "24h",
    metadata: Optional[Dict[str, str]] = None,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    **kwargs
) -> Dict[str, Any]:
    """Create a batch. Thin PyO3 binding to Rust core."""
    pass

def batch_retrieve(
    batch_id: str,
    provider: str,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    **kwargs
) -> Dict[str, Any]:
    """Get batch status. Thin PyO3 binding to Rust core."""
    pass

def batch_cancel(
    batch_id: str,
    provider: str,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    **kwargs
) -> Dict[str, Any]:
    """Cancel batch. Thin PyO3 binding to Rust core."""
    pass

def batch_list(
    provider: str,
    limit: int = 20,
    after: Optional[str] = None,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    **kwargs
) -> List[Dict[str, Any]]:
    """List batches. Thin PyO3 binding to Rust core."""
    pass

# Async variants
async def abatch_create(...) -> Dict[str, Any]: ...
async def abatch_retrieve(...) -> Dict[str, Any]: ...
async def abatch_cancel(...) -> Dict[str, Any]: ...
async def abatch_list(...) -> List[Dict[str, Any]]: ...
```

#### Responses Functions (per RFC-0920)

```python
def responses(
    model: str,
    input: Union[str, List[Dict[str, Any]]],
    instructions: Optional[str] = None,
    max_output_tokens: Optional[int] = None,
    temperature: Optional[float] = None,
    tools: Optional[List[Dict[str, Any]]] = None,
    tool_choice: Optional[Union[str, Dict]] = None,
    stream: bool = False,
    provider: Optional[str] = None,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    **kwargs
) -> Dict[str, Any]:
    """Create response. Thin PyO3 binding to Rust core."""
    pass

def get_response(
    response_id: str,
    provider: str,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    **kwargs
) -> Dict[str, Any]:
    """Get response. Thin PyO3 binding to Rust core."""
    pass

def delete_response(
    response_id: str,
    provider: str,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    **kwargs
) -> Dict[str, Any]:
    """Delete response. Thin PyO3 binding to Rust core."""
    pass

# Async variants
async def aresponses(...) -> Dict[str, Any]: ...
async def aget_response(...) -> Dict[str, Any]: ...
async def adelete_response(...) -> Dict[str, Any]: ...
```

#### Messages Functions (per RFC-0920)

```python
def messages(
    model: str,
    messages: List[Dict[str, Any]],
    max_tokens: Optional[int] = None,
    system: Optional[str] = None,
    temperature: Optional[float] = None,
    top_p: Optional[float] = None,
    top_k: Optional[int] = None,
    stream: bool = False,
    tools: Optional[List[Dict[str, Any]]] = None,
    tool_choice: Optional[Union[str, Dict]] = None,
    thinking: Optional[Dict] = None,
    provider: Optional[str] = None,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    **kwargs
) -> Dict[str, Any]:
    """Send message via Anthropic Messages API. Thin PyO3 binding to Rust core."""
    pass

# Async variant
async def amessages(...) -> Dict[str, Any]: ...
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

```rust
#[pyfunction]
fn batch_create(
    py: Python,
    model: &str,
    requests: Vec<PyObject>,
    endpoint: Option<&str>,
    completion_window: Option<&str>,
    metadata: Option<HashMap<String, String>>,
) -> PyResult<PyObject> {
    // Convert Python objects to Rust types
    // Call HTTP proxy or py_bridge
    // Convert response back to Python
}

#[pyfunction]
fn responses(
    py: Python,
    model: &str,
    input: PyObject,
    instructions: Option<&str>,
    max_output_tokens: Option<u32>,
    temperature: Option<f32>,
    tools: Option<Vec<PyObject>>,
    stream: Option<bool>,
) -> PyResult<PyObject> {
    // Implementation
}

#[pyfunction]
fn messages(
    py: Python,
    model: &str,
    messages: Vec<PyObject>,
    max_tokens: u32,
    system: Option<&str>,
    temperature: Option<f32>,
    stream: Option<bool>,
    tools: Option<Vec<PyObject>>,
) -> PyResult<PyObject> {
    // Implementation
}
```

## Acceptance Criteria

- [ ] batch_create() creates batch and returns batch ID
- [ ] batch_retrieve() returns batch status and results
- [ ] batch_cancel() cancels running batch
- [ ] batch_list() returns list of batches
- [ ] responses() creates response via OpenAI Responses API
- [ ] get_response() returns response by ID
- [ ] delete_response() deletes response
- [ ] messages() sends message via Anthropic Messages API
- [ ] All functions support streaming
- [ ] All functions raise RFC-0920 compatible exceptions
- [ ] Type hints work with mypy
- [ ] All existing tests pass

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
| 0.1.0 | 2026-05-18 | Initial draft |
