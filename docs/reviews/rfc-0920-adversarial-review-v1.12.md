# Adversarial Review: RFC-0920 Unified Python SDK v1.12

**RFC:** RFC-0920 (Economics): Unified Python SDK — Dual-Mode LiteLLM/any-llm Compatibility
**Review Date:** 2026-04-28
**Reviewer:** @mmacedoeu (self-review)
**RFC Version:** 1.12
**Status:** Issues found, fix required

---

## Executive Summary

RFC-0920 v1.12 resolves all v1.11 issues (enable_json_schema_validation, shared_session, web_search_options, response_format, reasoning_effort default). Cross-referencing against actual LiteLLM (`litellm/main.py`) and any-llm (`any_llm/`) source code reveals 2 remaining issues.

**Verdict: Return to Draft — Minor Issues**

---

## Critical Issues (Must Fix)

None. All critical issues from previous reviews are resolved.

---

## Important Issues (Should Fix)

### I1: `extra_headers` — In Sync Completion But NOT in Async `acompletion()`

**Location:** §Unified Function Signature (async), lines 221-305 + §Sync Completion Signature, lines 921-980

**Problem:** LiteLLM's `acompletion()` (main.py line 421) has:
```python
extra_headers: Optional[dict] = None,
```

The RFC's sync `completion()` (line 959) has `extra_headers: Optional[Dict] = None` — correct.

But the RFC's async `acompletion()` signature does NOT have `extra_headers`. This is an inconsistency — both async and sync should have the same params for LiteLLM compatibility.

**Fix Required:** Add `extra_headers: Optional[Dict] = None` to `acompletion()` signature.

---

### I2: `response_format` — RFC Says `Union[str, Dict]`, LiteLLM Also Has `Type[BaseModel]`

**Location:** §Unified Function Signature (async), line 266 + §Sync Completion Signature, line 985

**Problem:** The RFC's `response_format` is spec'd as:
```python
response_format: Optional[Union[str, Dict]] = None,
```

**But LiteLLM's actual signature** (main.py line 402 for async, line 1085 for sync) is:
```python
response_format: Optional[Union[dict, Type[BaseModel]]] = None,
```

LiteLLM supports `Type[BaseModel]` (Pydantic model type) as the response_format value. The RFC's `Union[str, Dict]` doesn't capture this.

**Fix Required:** Change to `response_format: Optional[Union[str, Dict, Type[Any]]] = None` to support Pydantic model types. Or at minimum `Union[str, Dict, type]` to capture the class type.

---

## Low Priority Issues

### L1: Version History Entry for v1.12 Is Very Long

**Location:** §Version History, line 2335

**Problem:** v1.12 entry is a very long single line.

**Fix Required:** None — accepted as-is per previous decisions.

---

### L2: `session_label` — RFC Has It, But Any-llm Doesn't

**Location:** §Unified Function Signature, line 271

**Problem:** The RFC's `acompletion()` has `session_label: Optional[str] = None` (line 271). LiteLLM uses this for session management. However, any-llm doesn't have this parameter.

This is fine — it's a LiteLLM compatibility parameter. But worth noting that any-llm users won't benefit from it.

**Fix Required:** None — informational only.

---

## Items Correctly Specced (No Change Needed)

| Item | Reason it's correct |
| ---- | ------------------- |
| C1 (v1.8): Mode-aware default provider | Correct — litellm-mode/full default to "openai", any-llm-mode raises |
| I1 (v1.8): functions/function_call pass-through | Correct — marked as "PASSED THROUGH" |
| I2 (v1.8): modalities, audio, prediction in Phase 4 | Correct — moved from Phase 4, correct placement |
| I3/I4/I5/I6 (v1.8): Sync completion explicit params | Correct — timeout, api_version, extra_headers, model_list all present in sync |
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
| Version history entries v1.0-v1.12 | All present and correctly formatted |

---

## Summary Table

| ID  | Severity  | Issue                                         | Status |
| --- | --------- | --------------------------------------------- | ------ |
| I1  | Important | extra_headers missing from async acompletion() | **FIX** — add to async signature |
| I2  | Important | response_format missing Type[BaseModel]       | **FIX** — add type hint for BaseModel |
| L1  | Low       | Version history long                          | ACCEPTED |
| L2  | Low       | session_label is LiteLLM-only (informational) | NO FIX — informational |

---

## Recommendations

**Return to Draft.** Issues I1 and I2 are straightforward fixes.

Key findings:
1. **`extra_headers` is missing from async `acompletion()`** — it's in sync but not async
2. **`response_format` doesn't support `Type[BaseModel]`** — LiteLLM uses Pydantic models

After fixes, re-submit for review.