# Mission: Responses Python SDK Functions

## Status

Completed


## RFC

RFC-0953 (Economics): Extended Python SDK Functions

## Dependencies

- RFC-0953: Extended Python SDK Functions (requires RFC-0908, RFC-0920, RFC-0917)

## Acceptance Criteria

- [x] responses() function exported in Python SDK
- [ ] get_response() function exported — spec below
- [ ] delete_response() function exported — spec below
- [x] aresponses() async variant exported
- [x] aget_response() async variant exported
- [x] adelete_response() async variant exported
- [x] Function signatures match RFC-0920 exactly

## get_response() / delete_response() Specification

> These functions are stateful sub-methods of the Responses API. They operate on provider-side storage (e.g., OpenAI stores responses server-side). The SDK makes HTTP calls to the provider's API — no local storage required.

### get_response()

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
    """
    Retrieve a specific response by ID from provider storage.

    Args:
        provider: Provider name (e.g., "openai")
        response_id: The response ID to retrieve
        api_key: Override API key
        api_base: Override base URL
        client_args: Additional provider-specific arguments

    Returns:
        ResponsesAPIResponse with full response data

    Raises:
        ModelNotFoundError: If response_id is invalid
        AuthenticationError: If API key is missing/invalid
    """
```

### delete_response()

```python
def delete_response(
    provider: str,
    response_id: str,
    *,
    api_key: Optional[str] = None,
    api_base: Optional[str] = None,
    client_args: Optional[Dict[str, Any]] = None,
    **kwargs
) -> ResponsesAPIResponse:
    """
    Delete a specific response from provider storage.

    Args:
        provider: Provider name (e.g., "openai")
        response_id: The response ID to delete
        api_key: Override API key
        api_base: Override base URL
        client_args: Additional provider-specific arguments

    Returns:
        ResponsesAPIResponse confirming deletion

    Raises:
        ModelNotFoundError: If response_id is invalid
        AuthenticationError: If API key is missing/invalid
    """
```

### Implementation Notes

- Thin PyO3 binding — delegates to Rust core HTTP client
- In litellm-mode: reqwest HTTP call to provider's responses API endpoint
- In any-llm-mode: py_bridge delegation to provider SDK
- Provider endpoint: `GET /v1/responses/{response_id}` (get) and `DELETE /v1/responses/{response_id}` (delete)
- Both sync and async variants required: `get_response()`, `delete_response()`, `aget_response()`, `adelete_response()`

### Tests (per RFC-0920 / RFC-0953)

- `test_get_response_param_order`: `get_response(provider, response_id)` — provider first
- `test_get_response_return_type`: returns `ResponsesAPIResponse`
- `test_get_response_not_found`: raises `ModelNotFoundError` for invalid ID
- `test_delete_response_param_order`: `delete_response(provider, response_id)` — provider first
- `test_delete_response_return_type`: returns `ResponsesAPIResponse`
- `test_delete_response_not_found`: raises `ModelNotFoundError` for invalid ID
- [x] PyO3 bindings match Python signatures
- [x] Streaming support (async generator with delta field)
- [x] Response content sanitized for logging
- [x] Error handling raises RFC-0920 compatible exceptions
- [x] API keys not logged in error messages
- [x] Type hints work with mypy
- [x] Works in litellm-mode (reqwest)
- [x] Works in any-llm-mode (py_bridge)
- [x] Unit tests pass
- [x] Integration tests pass

## Claimant

@claude


## Pull Request

None

## Notes

- OpenAI Responses API (stateful conversations)
- Input can be string or list of InputItem
- Supports function calling via tools parameter
- Thin PyO3 binding to Rust core per RFC-0908 architectural constraint

## Signature Changes (2026-05-21)

RFC-0920 and RFC-0953 signatures were updated. Follow-up mission needed:

| Function | Change |
|----------|--------|
| `responses()` | Dual-convention: accepts both `input=` (litellm) and `input_data=` (any-llm) |
| `responses()` | `max_tokens` renamed to `max_output_tokens` |
| `responses()` | Removed `modalities`, `audio` params |
| `responses()` | Added `include`, `parallel_tool_calls`, `previous_response_id`, `reasoning`, `text`, `presence_penalty`, `frequency_penalty`, `truncation`, `service_tier`, `safety_identifier`, `background`, `client_args` |
| `aresponses()` | Same changes as `responses()` |

Status: **NEEDS FOLLOW-UP** — signatures no longer match RFC-0920.
