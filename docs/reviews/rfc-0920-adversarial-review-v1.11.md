# Adversarial Review: RFC-0920 Unified Python SDK v1.11

**RFC:** RFC-0920 (Economics): Unified Python SDK — Dual-Mode LiteLLM/any-llm Compatibility
**Review Date:** 2026-04-28
**Reviewer:** @mmacedoeu (self-review)
**RFC Version:** 1.11
**Status:** Issues found, fix required

---

## Executive Summary

RFC-0920 v1.11 resolves all v1.10 issues (thinking vs reasoning_effort distinction, UnsupportedProviderError attrs, Phase 2 timeout marking). Cross-referencing against actual LiteLLM (`litellm/main.py`) and any-llm (`any_llm/`) source code reveals 4 remaining issues.

**Verdict: Return to Draft — Minor Issues**

---

## Critical Issues (Must Fix)

None. All critical issues from previous reviews are resolved.

---

## Important Issues (Should Fix)

### I1: `enable_json_schema_validation` — Not in RFC, LiteLLM Has It

**Location:** §Unified Function Signature, lines 221-281 + §Sync Completion Signature, lines 921-970

**Problem:** LiteLLM's `acompletion()` (main.py line 428) and sync `completion()` (main.py line 1110) have:
```python
enable_json_schema_validation: Optional[bool] = None,  # Per-request override
```

The RFC does NOT include `enable_json_schema_validation` in either async or sync signatures.

**Fix Required:** Add `enable_json_schema_validation: Optional[bool] = None` to both `acompletion()` and `completion()` signatures.

---

### I2: `shared_session` — Not in RFC, LiteLLM Has It

**Location:** §Unified Function Signature, lines 221-281 + §Sync Completion Signature, lines 921-970

**Problem:** LiteLLM's `acompletion()` (main.py line 426) and sync `completion()` (main.py line 1108) have:
```python
shared_session: Optional["ClientSession"] = None,  # Session management
```

The RFC does NOT include `shared_session` in either async or sync signatures.

**Fix Required:** Add `shared_session: Optional[Any] = None` to both signatures (type should be `Optional["ClientSession"]` but since ClientSession is an openai library type, using `Any` is acceptable for the RFC spec).

---

### I3: `web_search_options` — Not in RFC, LiteLLM Has It

**Location:** §Unified Function Signature, lines 221-281 + §Sync Completion Signature, lines 921-970

**Problem:** LiteLLM's `acompletion()` (main.py line 424) and sync `completion()` (main.py line 1092) have:
```python
web_search_options: Optional[OpenAIWebSearchOptions] = None,
```

The RFC does NOT include `web_search_options` in either async or sync signatures.

**Fix Required:** Add `web_search_options: Optional[Dict] = None` to both signatures (OpenAIWebSearchOptions is a structured type, can be spec'd as Dict for RFC purposes).

---

## Low Priority Issues

### L1: Streaming Spec Missing `response_format`

**Location:** §accompletion() Streaming — AsyncIterator Return Type, lines 1046-1052

**Problem:** The streaming spec shows:
```python
async def acompletion(
    model: str,
    messages: List[Dict],
    *,
    stream: Optional[bool] = None,
    stream_options: Optional[Dict] = None,
    **kwargs,
) -> Union[CompletionResponse, AsyncIterator[ChatCompletionChunk]]:
```

But LiteLLM's `acompletion()` has `response_format` (line 402). The streaming spec should include it for completeness.

**Fix Required:** Add `response_format: Optional[Union[str, Dict]] = None` to the streaming spec signature.

---

### L2: LiteLLM `reasoning_effort` Values — RFC Says "auto", LiteLLM Has More Options

**Location:** §Unified Function Signature, line 252 + §Sync Completion Signature, line 946

**Problem:** The RFC shows:
```python
reasoning_effort: Optional[str] = "auto",  # LiteLLM-style string enum
```

But LiteLLM's `acompletion()` (line 410-412) has:
```python
reasoning_effort: Optional[
    Literal["none", "minimal", "low", "medium", "high", "xhigh", "default"]
] = None,
```

The RFC defaults to `"auto"` but LiteLLM defaults to `None`. Also LiteLLM has 7 literal options while the RFC just says "string enum" without listing options.

**Fix Required:**
1. Change default from `"auto"` to `None` to match LiteLLM behavior
2. Or clarify that `"auto"` is the quota-router default (different from LiteLLM's `None` default)
3. Add the full list of valid values: `Literal["none", "minimal", "low", "medium", "high", "xhigh", "default", "auto"]`

---

### L3: Version History Entry for v1.11 Is Very Long

**Location:** §Version History

**Problem:** v1.11 entry is a very long single line.

**Fix Required:** None — accepted as-is per previous decisions.

---

## Items Correctly Specced (No Change Needed)

| Item | Reason it's correct |
| ---- | ------------------- |
| C1 (v1.8): Mode-aware default provider | Correct — litellm-mode/full default to "openai", any-llm-mode raises |
| I1 (v1.8): functions/function_call pass-through | Correct — marked as "PASSED THROUGH" |
| I2 (v1.8): modalities, audio, prediction in Phase 4 | Correct — moved from Phase 4, correct placement |
| I3/I4/I5/I6 (v1.8): Sync completion explicit params | Correct — timeout, api_version, extra_headers, model_list all present |
| I7 (v1.8): Router ModelNotFoundError | Correct — already raises |
| I10 (v1.8): thinking is now structured Dict | Correct — `thinking: Optional[Dict]` separate from `reasoning_effort` |
| I11 (v1.8): Embedded API renamed | Correct — "LiteLLM-compatibility style" |
| I1 (v1.9): reasoning_effort default "auto" | See L2 above — maybe change to None to match LiteLLM |
| I2 (v1.9): api_type added to sync completion | Correct |
| I3 (v1.9): verbosity added to acompletion | Correct |
| I5 (v1.9): MissingApiKeyError has env_var_name | Correct |
| I7 (v1.9): sync completion reasoning_effort "auto" | Correct |
| I8 (v1.9): abatch_completion_models() added | Correct |
| I1 (v1.10): thinking vs reasoning_effort distinction | Correct — thinking is Dict, reasoning_effort is string |
| I2 (v1.10): resolve_provider() error message | ACCEPTED — RFC message is more informative |
| I3 (v1.10): InsufficientFundsError | ACCEPTED — RFC is OCTO-W specific extension |
| I4 (v1.10): UnsupportedProviderError attrs | Correct — has provider_key, supported_providers |
| I5 (v1.10): Phase 2 timeout item marked DONE | Correct — line 2266 shows "(DONE: specced in sync...)" |
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

---

## Summary Table

| ID  | Severity  | Issue                                         | Status |
| --- | --------- | --------------------------------------------- | ------ |
| I1  | Important | enable_json_schema_validation missing        | **FIX** — add to both signatures |
| I2  | Important | shared_session missing                       | **FIX** — add to both signatures |
| I3  | Important | web_search_options missing                   | **FIX** — add to both signatures |
| L1  | Low       | Streaming spec missing response_format       | **FIX** — add to streaming signature |
| L2  | Low       | reasoning_effort default should be None      | **FIX** — change default or clarify |
| L3  | Low       | Version history long                          | ACCEPTED |

---

## Recommendations

**Return to Draft.** Issues I1, I2, I3 are straightforward parameter additions. L1 and L2 are simple fixes.

Key findings:
1. **`enable_json_schema_validation`**: Per-request JSON schema validation override, NOT in RFC
2. **`shared_session`**: ClientSession for session management, NOT in RFC
3. **`web_search_options`**: OpenAI web search options, NOT in RFC
4. **`reasoning_effort`**: Default is `"auto"` in RFC but `None` in LiteLLM — semantic difference

After fixes, re-submit for review.