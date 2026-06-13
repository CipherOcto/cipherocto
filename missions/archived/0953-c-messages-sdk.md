# Mission: Messages Python SDK Functions

## Status

Completed

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

## Pull Request

None

## Notes

- Anthropic Messages API (native format)
- max_tokens is now REQUIRED per RFC-0920 (was optional at original implementation time)
- Thin PyO3 binding to Rust core per RFC-0908 architectural constraint
- Content blocks: text, image, tool_use, tool_result

## Signature Changes (2026-05-21)

RFC-0920 and RFC-0953 signatures were updated. Follow-up mission needed:

| Function | Change |
|----------|--------|
| `messages()` | `max_tokens` now REQUIRED (not optional) — matches any-llm convention |
| `messages()` | `stop` renamed to `stop_sequences` |
| `messages()` | `stream` param — already correct, no rename needed |
| `messages()` | `system` type changed to `Optional[Union[str, List[Dict]]]` (supports content blocks) |
| `messages()` | Added `cache_control`, `client_args` params |
| `amessages()` | Same changes as `messages()` |

Status: **NEEDS FOLLOW-UP** — signatures no longer match RFC-0920.
