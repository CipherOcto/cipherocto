# Mission: Messages SDK Signature Update

## Status

LANDED 2026-08-13 (commit 48bbd8c6).
Filed 2026-08-13 from RFC-0953 §messages signature changes.

## RFC

RFC-0953 (Economics): Extended Python SDK Functions

## Summary

Update messages function signatures to match RFC-0920/0953 changes from 2026-05-21.

## Changes Required

| Function | Current | Target |
|----------|---------|--------|
| `messages()` | `max_tokens: i32` | Already required per RFC-0920 — no change needed |
| `messages()` | `stop: Option<Vec<String>>` | `stop_sequences: Option<Vec<String>>` |
| `messages()` | `stream: Option<bool>` | No change needed — already uses `stream` |
| `messages()` | `system: Optional[Union[str, List[Dict]]]` | Already expanded per RFC-0920 — no change needed |
| `messages()` | Missing params | Add `cache_control`, `client_args` |
| `amessages()` | Same as messages() | Same changes |

## Acceptance Criteria

- [ ] `messages(model, messages, 100)` — max_tokens required (3rd positional)
- [ ] `messages(model, messages)` without max_tokens raises TypeError
- [ ] `stop_sequences` param works (not `stop`)
- [ ] `stream` param works (already correct — no rename needed)
- [ ] `system` accepts both `str` and `List[Dict]` (content blocks)
- [ ] `cache_control` param accepted
- [ ] `client_args` param accepted
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

- RFC-0920 messages function signatures
- Mission 0953-c (completed — this is the follow-up)
