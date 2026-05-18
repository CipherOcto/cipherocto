# Mission: Responses Python SDK Functions

## Status

Open

## RFC

RFC-0953 (Economics): Extended Python SDK Functions

## Dependencies

- RFC-0953: Extended Python SDK Functions (requires RFC-0908, RFC-0920, RFC-0917)

## Acceptance Criteria

- [ ] responses() function exported in Python SDK
- [ ] get_response() function exported
- [ ] delete_response() function exported
- [ ] aresponses() async variant exported
- [ ] aget_response() async variant exported
- [ ] adelete_response() async variant exported
- [ ] Function signatures match RFC-0920 exactly
- [ ] PyO3 bindings match Python signatures
- [ ] Streaming support (async generator with delta field)
- [ ] Response content sanitized for logging
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

- OpenAI Responses API (stateful conversations)
- Input can be string or list of InputItem
- Supports function calling via tools parameter
- Thin PyO3 binding to Rust core per RFC-0908 architectural constraint
