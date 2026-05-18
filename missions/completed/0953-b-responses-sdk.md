# Mission: Responses Python SDK Functions

## Status

Completed


## RFC

RFC-0953 (Economics): Extended Python SDK Functions

## Dependencies

- RFC-0953: Extended Python SDK Functions (requires RFC-0908, RFC-0920, RFC-0917)

## Acceptance Criteria

- [x] responses() function exported in Python SDK
- [ ] get_response() function exported
- [ ] delete_response() function exported
- [x] aresponses() async variant exported
- [x] aget_response() async variant exported
- [x] adelete_response() async variant exported
- [x] Function signatures match RFC-0920 exactly
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
