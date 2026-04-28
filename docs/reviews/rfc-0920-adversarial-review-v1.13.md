# Adversarial Review: RFC-0920 Unified Python SDK v1.13

**RFC:** RFC-0920 (Economics): Unified Python SDK — Dual-Mode LiteLLM/any-llm Compatibility
**Review Date:** 2026-04-28
**Reviewer:** @mmacedoeu (self-review)
**RFC Version:** 1.13
**Status:** Issues found, fix required

---

## Executive Summary

RFC-0920 v1.13 resolves all v1.12 issues (extra_headers in async, Type[Any] for response_format). Cross-referencing against actual LiteLLM (`litellm/main.py`) and any-llm (`any_llm/`) source code reveals 1 minor issue.

**Verdict: Return to Draft — Trivial Issue**

---

## Critical Issues (Must Fix)

None. All critical issues from previous reviews are resolved.

---

## Important Issues (Should Fix)

### I1: `base_url` — In Sync Completion But NOT in Async `acompletion()`

**Location:** §Unified Function Signature (async), lines 221-305 + §Sync Completion Signature, lines 921-980

**Problem:** LiteLLM's `acompletion()` (main.py line 417) has:
```python
base_url: Optional[str] = None,
```

The RFC's sync `completion()` (line 961) has `base_url: Optional[str] = None` — correct.

But the RFC's async `acompletion()` signature does NOT have `base_url`. It only has `api_base` (line 233). While `api_base` and `base_url` are aliases for the same thing in LiteLLM, the RFC should explicitly include `base_url` for completeness.

**Fix Required:** Add `base_url: Optional[str] = None,  # Alias for api_base` to `acompletion()` signature, matching sync signature.

---

## Low Priority Issues

### L1: Version History Entry for v1.13 Is Very Long

**Location:** §Version History, line 2335

**Problem:** v1.13 entry is a very long single line.

**Fix Required:** None — accepted as-is per previous decisions.

---

### L2: `messages` Type Hint — RFC says `List[Dict[str, str]]`, LiteLLM says `List`

**Location:** §Unified Function Signature (async), line 226

**Problem:** The RFC's `acompletion()` signature has:
```python
messages: List[Dict[str, str]],  # LiteLLM message format
```

But LiteLLM's actual `acompletion()` (main.py line 382) has:
```python
messages: List = [],  # No type hint on List element
```

The RFC's `List[Dict[str, str]]` is more restrictive — it requires all dict values to be strings. LiteLLM accepts any dict values (including nested objects, lists, etc.).

**Fix Required:** None — the RFC's more restrictive type is intentional for type safety. LiteLLM's permissive typing is a Python gradual typing artifact. This is acceptable as the RFC's stricter typing will catch errors at type-check time.

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
| Version history entries v1.0-v1.13 | All present and correctly formatted |

---

## Summary Table

| ID  | Severity  | Issue                                         | Status |
| --- | --------- | --------------------------------------------- | ------ |
| I1  | Important | base_url missing from async acompletion()     | **FIX** — add base_url alias |
| L1  | Low       | Version history long                          | ACCEPTED |
| L2  | Low       | messages type hint more restrictive           | NO FIX — intentional |

---

## Recommendations

**Return to Draft.** Issue I1 is a simple addition — add `base_url` alias to async `acompletion()`.

Key findings:
1. **`base_url` is missing from async `acompletion()`** — it's an alias for `api_base` but should be explicit for LiteLLM compatibility
2. **`messages` type hint is more restrictive than LiteLLM's** — but this is intentional for type safety

After fixes, re-submit for review.