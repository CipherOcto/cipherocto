# Mission: Batch Python SDK Functions

## Status

Completed


## RFC

RFC-0953 (Economics): Extended Python SDK Functions

## Dependencies

- RFC-0953: Extended Python SDK Functions (requires RFC-0908, RFC-0920, RFC-0917)

## Acceptance Criteria

- [x] batch_create() function exported in Python SDK
- [x] batch_retrieve() function exported
- [x] batch_cancel() function exported
- [x] batch_list() function exported
- [x] batch_results() function exported
- [x] abatch_create() async variant exported
- [x] abatch_retrieve() async variant exported
- [x] abatch_cancel() async variant exported
- [x] abatch_list() async variant exported
- [x] abatch_results() async variant exported
- [ ] Function signatures match RFC-0920 exactly
- [x] PyO3 bindings match Python signatures
- [x] Streaming support for batch progress
- [x] Batch file uploads validated
- [x] API keys not logged in error messages
- [x] Error handling raises RFC-0920 compatible exceptions
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

- Function names MUST match RFC-0920: batch_create, batch_retrieve, batch_cancel, batch_list
- NOT create_batch, get_batch, cancel_batch, list_batches
- provider parameter is REQUIRED (no default value)
- Thin PyO3 binding to Rust core per RFC-0908 architectural constraint

## Signature Changes (2026-05-21)

RFC-0920 and RFC-0953 signatures were updated. Follow-up mission needed:

| Function | Change |
|----------|--------|
| `batch_create()` | Removed `model` param, made `endpoint` required, added `client_args` |
| `batch_retrieve()` | Order changed to `(provider, batch_id)`, `provider` required |
| `batch_cancel()` | Added `client_args` |
| `batch_list()` | `limit` now optional, added `client_args` |
| `batch_results()` | Order changed to `(provider, batch_id)`, `provider` required, added `client_args` |

Status: **NEEDS FOLLOW-UP** — signatures no longer match RFC-0920.
