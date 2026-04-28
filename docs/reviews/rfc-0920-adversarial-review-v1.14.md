# Adversarial Review: RFC-0920 Unified Python SDK v1.14

**RFC:** RFC-0920 (Economics): Unified Python SDK — Dual-Mode LiteLLM/any-llm Compatibility
**Review Date:** 2026-04-28
**Reviewer:** @mmacedoeu (self-review)
**RFC Version:** 1.14
**Status:** Issues found, fix required

---

## Executive Summary

RFC-0920 v1.14 resolves all v1.13 issues (base_url in async). Cross-referencing against actual LiteLLM (`litellm/main.py`) and any-llm (`any_llm/`) source code reveals 2 issues.

**Verdict: Return to Draft — Minor Issues**

---

## Critical Issues (Must Fix)

None. All critical issues from previous reviews are resolved.

---

## Important Issues (Should Fix)

### I1: Sync Completion Note Says "thinking is alias for reasoning_effort" — But They're Separate in LiteLLM

**Location:** §Sync Completion Signature, line 974

**Problem:** The RFC's sync `completion()` signature has this comment:
```python
# Note: `thinking` (LiteLLM name) is accepted as alias for `reasoning_effort`
```

But in **LiteLLM's sync `completion()`** (main.py lines 1081-1106), `thinking` and `reasoning_effort` are **separate parameters**, not aliases:
```python
reasoning_effort: Optional[Literal["none", "minimal", "low", ...]] = None,
...
thinking: Optional[AnthropicThinkingParam] = None,
```

The RFC's async `acompletion()` correctly distinguishes them (lines 252-254 and 278-279) with:
- `reasoning_effort`: string enum
- `thinking`: structured Dict

But the sync signature's note incorrectly claims `thinking` is an alias for `reasoning_effort`. In LiteLLM sync, they are separate distinct parameters.

**Fix Required:** Remove or correct the misleading note at line 974. Replace with:
```python
# Note: `thinking` (structured Dict) and `reasoning_effort` (string enum) are separate parameters, not aliases
```

---

### I2: Streaming Spec Missing `timeout` — Common Streaming Use Case

**Location:** §accompletion() Streaming — AsyncIterator Return Type, lines 1110-1118

**Problem:** The streaming spec signature is:
```python
async def acompletion(
    model: str,
    messages: List[Dict],
    *,
    stream: Optional[bool] = None,
    stream_options: Optional[Dict] = None,
    response_format: Optional[Union[str, Dict, Type[Any]]] = None,
    **kwargs,
) -> Union[CompletionResponse, AsyncIterator[ChatCompletionChunk]]:
```

While `timeout` is available via `**kwargs`, it's a common parameter for streaming calls (to avoid hanging). For completeness, the streaming spec should explicitly include `timeout: Optional[Union[float, int]] = None`.

**Fix Required:** Add `timeout: Optional[Union[float, int]] = None` to the streaming spec signature, matching the full `acompletion()` signature.

---

## Low Priority Issues

### L1: Version History Entry for v1.14 Is Very Long

**Location:** §Version History, line 2335

**Problem:** v1.14 entry is a very long single line.

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
| Version history entries v1.0-v1.14 | All present and correctly formatted |

---

## Summary Table

| ID  | Severity  | Issue                                         | Status |
| --- | --------- | --------------------------------------------- | ------ |
| I1  | Important | Sync note says "thinking alias for reasoning_effort" — wrong | **FIX** — remove/correct misleading note |
| I2  | Low       | Streaming spec missing timeout explicit param | **FIX** — add timeout to streaming spec |
| L1  | Low       | Version history long                          | ACCEPTED |

---

## Recommendations

**Return to Draft.** Issues I1 and I2 are straightforward fixes.

Key findings:
1. **Sync completion note at line 974 is wrong** — thinking and reasoning_effort are separate parameters in LiteLLM, not aliases
2. **Streaming spec should explicitly include timeout** — common for streaming to avoid hanging

After fixes, re-submit for review.