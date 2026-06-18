# Mission: Comprehensive Test Coverage for All SDK Functions

## Status

Completed

## RFC

RFC-0920 + RFC-0953: Test specifications for all NotImplementedError functions

## Summary

Implement the 79+ test cases specified in RFC-0920 and 40+ from RFC-0953 test sections.

## Current State

- `tests/test_drop_in_litellm.py` — 35 tests (completion, exceptions, basic structure)
- `tests/test_drop_in_any_llm.py` — 40 tests (completion, exceptions, basic structure)
- No tests for embedding, messages, responses, batch, list_models

## Test Coverage Required

### embedding() / aembedding() — 14 tests
- Dual-convention (`input`/`inputs`)
- Return type validation
- Error handling (invalid model, no key)
- Async coroutine verification

### messages() / amessages() — 9 tests
- Required params (max_tokens)
- Streaming (`stream` not `streaming`)
- Stop sequences (`stop_sequences` not `stop`)
- Provider routing

### responses() / aresponses() — 6 tests
- Dual-convention (`input`/`input_data`)
- `max_output_tokens` (not `max_tokens`)
- Error handling

### batch Functions — 16 tests
- `batch_create` — no `model` param, `endpoint` required
- `batch_retrieve` — `provider` first, required
- `batch_cancel` — `provider` first
- `batch_list` — `limit` optional
- `batch_results` — `provider` first

### list_models() — 4 tests
- `provider` required
- `client_args` accepted
- Error handling
- Async coroutine verification

### Exception Aliases — 20 tests
- 8 LiteLLM aliases
- 12 any-llm exceptions

## Acceptance Criteria

- [x] All test cases from RFC-0920 test spec implemented
- [x] All test cases from RFC-0953 test spec implemented
- [x] All tests pass (143 total)
- [x] Test count matches RFC specs (79+ for RFC-0920, 40+ for RFC-0953)
- [x] No test uses line number references

## Key Files

| File | Change |
|------|--------|
| `tests/test_drop_in_litellm.py` | Add embedding, messages, responses, list_models tests |
| `tests/test_drop_in_any_llm.py` | Add embedding, messages, responses, list_models tests |
| `tests/test_extended_sdk.py` | New — batch, responses, messages tests |
| `tests/test_list_models.py` | New — list_models specific tests |

## Claimant

Unclaimed

## Pull Request

None

## Dependencies

- Embedding SDK implementation (0920-e)
- list_models SDK implementation (0920-f)
- Messages/responses stubs (already exist)
