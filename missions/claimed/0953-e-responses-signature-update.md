# Mission: Responses SDK Signature Update

## Status

LANDED 2026-08-13 (commit 28ff2145).
Filed 2026-08-13 from RFC-0953 §responses signature changes.

## RFC

RFC-0953 (Economics): Extended Python SDK Functions

## Summary

Update responses function signatures to match RFC-0920/0953 changes from 2026-05-21.

## Changes Required

> **Note:** Some params below already exist in RFC-0920 responses(). Others are new additions that must be added to RFC-0920 before implementation. Verify RFC state before implementing.

| Function | Current | Target |
|----------|---------|--------|
| `responses()` | Single `input_data` param | Dual-convention: `input` (litellm) + `input_data` (any-llm) — requires RFC update to add `input` |
| `responses()` | `max_tokens` | Rename to `max_output_tokens` |
| `responses()` | Has `modalities`, `audio` (if present in current impl) | Remove these params |
| `responses()` | Missing params | Add `include`, `parallel_tool_calls`, `previous_response_id`, `reasoning`, `text`, `presence_penalty`, `frequency_penalty`, `truncation`, `service_tier`, `safety_identifier`, `background` (new — requires RFC update). `client_args` already in RFC-0920. |
| `aresponses()` | Same as responses() | Same changes |

## Acceptance Criteria

- [ ] `responses(model="...", input="Hello")` works (litellm convention) — requires RFC-0920 update to add `input` param
- [ ] `responses(model="...", input_data="Hello")` works (any-llm convention)
- [ ] `responses(model="...", input="a", input_data="b")` raises error
- [ ] `max_output_tokens` param works (not `max_tokens`)
- [ ] `modalities` and `audio` params removed (if present in current impl)
- [ ] All new params accepted (even if stub) — 11 new params require RFC-0920 update
- [ ] All drop-in tests pass
- [ ] Signatures match RFC-0920 exactly

## Key Files

| File | Change |
|------|--------|
| `crates/quota-router-pyo3/src/completion.rs` | Update function signatures |

## Claimant

Unclaimed

## Pull Request

None

## Dependencies

- RFC-0920 responses function signatures
- Mission 0953-b (completed — this is the follow-up)
