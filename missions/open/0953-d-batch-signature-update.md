# Mission: Batch SDK Signature Update

## Status

Completed

## RFC

RFC-0953 (Economics): Extended Python SDK Functions

## Summary

Update batch function signatures to match RFC-0920/0953 changes from 2026-05-21.

## Changes Required

| Function | Current | Target |
|----------|---------|--------|
| `batch_create()` | `endpoint` optional, no `client_args` | Make `endpoint` required, add `client_args` |
| `batch_retrieve()` | `(batch_id, provider)` | `(provider, batch_id)`, `provider` required, add `client_args` |
| `batch_cancel()` | No `client_args` | Add `client_args` |
| `batch_list()` | `limit` required | `limit` optional, add `client_args` |
| `batch_results()` | `(batch_id, provider)` | `(provider, batch_id)`, `provider` required, add `client_args` |

## Acceptance Criteria

- [ ] `batch_create(provider, input_file, endpoint)` — no `model` param
- [ ] `batch_retrieve(provider, batch_id)` — provider first
- [ ] `batch_cancel(provider, batch_id)` — has `client_args`
- [ ] `batch_list(provider, limit=None)` — limit optional
- [ ] `batch_results(provider, batch_id)` — provider first
- [ ] All async variants (abatch_create, abatch_retrieve, abatch_cancel, abatch_list, abatch_results) match sync signatures
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

- RFC-0920 batch function signatures
- Mission 0953-a (completed — this is the follow-up)
