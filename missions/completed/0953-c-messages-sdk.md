# Mission: Messages Python SDK Functions

## Status

Completed

Open

## RFC

RFC-0953 (Economics): Extended Python SDK Functions

## Dependencies

- RFC-0953: Extended Python SDK Functions (requires RFC-0908, RFC-0920, RFC-0917)

## Acceptance Criteria

- [x] messages() function exported in Python SDK
- [x] amessages() async variant exported
- [x] Function signatures match RFC-0920 exactly
- [x] PyO3 bindings match Python signatures
- [x] Streaming support (async generator with delta field)
- [x] System prompt support (top-level system parameter)
- [x] Tool use support (tools and tool_choice parameters)
- [x] Content blocks handled: text, image, tool_use, tool_result
- [x] Message content sanitized for logging
- [x] Error handling raises RFC-0920 compatible exceptions
- [x] API keys not logged in error messages
- [x] Type hints work with mypy
- [x] Works in litellm-mode (reqwest)
- [x] Works in any-llm-mode (py_bridge)
- [x] Unit tests pass
- [x] Integration tests pass

## Claimant

@claude

Unclaimed

## Pull Request

None

## Notes

- Anthropic Messages API (native format)
- max_tokens is optional per RFC-0920 (not required)
- Thin PyO3 binding to Rust core per RFC-0908 architectural constraint
- Content blocks: text, image, tool_use, tool_result
