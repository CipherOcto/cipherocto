# Adversarial Review: RFC-0920 Unified Python SDK v1.15

**RFC:** RFC-0920 (Economics): Unified Python SDK — Dual-Mode LiteLLM/any-llm Compatibility
**Review Date:** 2026-04-28
**Reviewer:** @mmacedoeu (self-review)
**RFC Version:** 1.15
**Status:** Issues found, fix required

---

## Executive Summary

RFC-0920 v1.15 resolves all v1.14 issues (thinking/reasoning_effort note correction, timeout in streaming spec). Cross-referencing against actual LiteLLM (`litellm/main.py`) and any-llm (`any_llm/`) source code reveals 1 issue.

**Verdict: Return to Draft — Minor Issue**

---

## Critical Issues (Must Fix)

None. All critical issues from previous reviews are resolved.

---

## Important Issues (Should Fix)

### I1: `response_format` — In Async `acompletion()` But NOT in Sync `completion()`

**Location:** §Unified Function Signature (async), line 267 + §Sync Completion Signature, lines 930-982

**Problem:** The RFC's async `acompletion()` (line 267) has:
```python
response_format: Optional[Union[str, Dict, Type[Any]]] = None,
```

But the RFC's sync `completion()` (lines 930-982) does NOT have `response_format`.

**LiteLLM sync `completion()`** (main.py line 1085) has:
```python
response_format: Optional[Union[dict, Type[BaseModel]]] = None,
```

This is a significant gap — both async and sync should have the same parameters for LiteLLM compatibility. The streaming spec (lines 1118) also has `response_format`, so this inconsistency is visible in multiple places.

**Fix Required:** Add `response_format: Optional[Union[str, Dict, Type[Any]]] = None` to sync `completion()` signature, matching async and streaming specs.

---

## Low Priority Issues

### L1: Version History Entry for v1.15 Is Very Long

**Location:** §Version History, line 2335

**Problem:** v1.15 entry is a very long single line.

**Fix Required:** None — accepted as-is per previous decisions.

---

## Items Correctly Specced (No Change Needed)

| Item | Reason it's correct |
| ---- | ------------------- |
| C1 (v1.8): Mode-aware default provider | Correct — litellm-mode/full default to "openai", any-llm-mode raises |
| I1 (v1.8): functions/function_call pass-through | Correct — marked as "PASSED THROUGH" |
| I2 (v1.8): modalities, audio, prediction in Phase 4 | Correct — Phase 4, not Phase 3 |
| I3/I4/I5/I6 (v1.8): Sync completion explicit params | Correct — timeout, api_version, extra_headers, model_list, base_url, api_type all present |
| I7 (v1.8): Router ModelNotFoundError | Correct — already raises |
| I10 (v1.8): thinking is structured Dict | Correct — `thinking: Optional[Dict]` separate from `reasoning_effort` |
| I11 (v1.8): Embedded API renamed | Correct — "LiteLLM-compatibility style" |
| I1 (v1.9): reasoning_effort default None | Correct — matches LiteLLM |
| I2 (v1.9): api_type added to sync completion | Correct |
| I3 (v1.9): verbosity added to acompletion | Correct |
| I5 (v1.9): MissingApiKeyError has env_var_name | Correct |
| I7 (v1.9): sync completion reasoning_effort None | Correct |
| I8 (v1.9): abatch_completion_models() added | Correct |
| I1 (v1.10): thinking vs reasoning_effort distinction | Correct — thinking is Dict, reasoning_effort is string |
| I2 (v1.10): resolve_provider() error message | ACCEPTED — RFC message is more informative |
| I3 (v1.10): InsufficientFundsError | ACCEPTED — RFC is OCTO-W specific extension |
| I4 (v1.10): UnsupportedProviderError attrs | Correct — has provider_key, supported_providers |
| I5 (v1.10): Phase 2 timeout item marked DONE | Correct |
| I1 (v1.11): enable_json_schema_validation added | Correct — now in both signatures |
| I2 (v1.11): shared_session added | Correct — now in both signatures |
| I3 (v1.11): web_search_options added | Correct — now in both signatures |
| L1 (v1.11): streaming spec response_format | Correct |
| L2 (v1.11): reasoning_effort default None | Correct — now matches LiteLLM |
| I1 (v1.12): extra_headers added to acompletion() | Correct — now in both signatures |
| I2 (v1.12): response_format Type[Any] | Correct — now supports Pydantic BaseModel types |
| I1 (v1.13): base_url added to acompletion() | Correct — now in both signatures |
| I1 (v1.14): thinking/reasoning_effort note corrected | Correct — now says separate params, not aliases |
| I2 (v1.14): timeout added to streaming spec | Correct |
| CRITICAL INVARIANT block | Clear, mathematically stated |
| Mode gate table (both interfaces in all modes) | Correct per RFC-0917 |
| Provider resolution algorithm (case-insensitive) | Correct |
| Exception hierarchy (complete) | Correct including `AllModelsFailedError` |
| `async_iter_to_sync_iter()` bridge spec | Correct pattern |
| streaming SSE table | Correct per provider formats |
| async batch via `asyncio.gather` | Correct |
| Router Python-level class | Correct — not wrapping Rust Router |
| All 8 routing strategies listed | Correct |
| `set_api_key()` storage modes table | Correct distinction any-llm vs full |
| `get_budget_status()` Balance reference | Correct per RFC-0904 |
| Platform provider (any-api key format) | Valid alternative to any-llm's `PlatformProvider` |
| Version history entries v1.0-v1.15 | All present and correctly formatted |

---

## Summary Table

| ID  | Severity  | Issue                                         | Status |
| --- | --------- | --------------------------------------------- | ------ |
| I1  | Important | response_format missing from sync completion() | **FIX** — add to sync signature |
| L1  | Low       | Version history long                          | ACCEPTED |

---

## Recommendations

**Return to Draft.** Issue I1 is a straightforward addition — add `response_format` to sync `completion()` signature.

Key findings:
1. **`response_format` is missing from sync `completion()`** — it's in async and streaming specs, but not sync. This is an inconsistency that should be fixed.

After fixes, re-submit for review.