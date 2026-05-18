# Mission: Messages Python SDK Functions

## Status

Open

## RFC

RFC-0953 (Economics): Extended Python SDK Functions

## Dependencies

- RFC-0953: Extended Python SDK Functions (requires RFC-0908, RFC-0920, RFC-0917)

## Acceptance Criteria

- [ ] messages() function exported in Python SDK
- [ ] amessages() async variant exported
- [ ] Function signatures match RFC-0920 exactly
- [ ] PyO3 bindings match Python signatures
- [ ] Streaming support (async generator with delta field)
- [ ] System prompt support (top-level system parameter)
- [ ] Tool use support (tools and tool_choice parameters)
- [ ] Content blocks handled: text, image, tool_use, tool_result
- [ ] Message content sanitized for logging
- [ ] Error handling raises RFC-0920 compatible exceptions
- [ ] API keys not logged in error messages
- [ ] Type hints work with mypy
- [ ] Works in litellm-mode (reqwest)
- [ ] Works in any-llm-mode (py_bridge)
- [ ] Unit tests pass
- [ ] Integration tests pass

## Claimant

Unclaimed

## Pull Request

None

## Notes

- Anthropic Messages API (native format)
- max_tokens is optional per RFC-0920 (not required)
- Thin PyO3 binding to Rust core per RFC-0908 architectural constraint
- Content blocks: text, image, tool_use, tool_result
