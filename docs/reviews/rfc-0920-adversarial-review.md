# Adversarial Review: RFC-0920 Unified Python SDK

**RFC:** RFC-0920 (Economics): Unified Python SDK — Dual-Mode LiteLLM/any-llm Compatibility
**Review Date:** 2026-04-27
**Reviewer:** @mmacedoeu (self-review)
**RFC Version:** 1.0

---

## Executive Summary

RFC-0920 addresses a valid gap (unified Python SDK bridging LiteLLM and any-llm styles), but has issues with security model conflations and several edge cases that need resolution.

**Verdict: Return to Draft — Non-Blocking Issues**

**Note:** C1 (below) was initially flagged as critical based on RFC-0917 v2.24's incorrect claim that modes have exclusive interfaces. RFC-0917 has been corrected (v2.25) to clarify both modes have both interfaces. With that correction, RFC-0920's core premise is VALID.

---

## Critical Issues (Must Fix)

### C1 ~~RFC-0920 Conflates Modes That RFC-0917 Declares Mutually Exclusive~~ — **INVALIDATED**

**Status:** This issue is now **INVALID** after RFC-0917 v2.25 fixed the contradiction.

RFC-0917 v2.24 incorrectly claimed litellm-mode had no Python SDK and any-llm-mode had no HTTP proxy. This has been corrected in v2.25. Both modes now correctly state they have BOTH interfaces.

**RFC-0920's claim** ("The SDK accepts both calling conventions regardless of mode") is now **CORRECT** — both interfaces are available in both modes.

**No fix required for RFC-0920 on this issue.**

---

### C2: Two Incompatible Security Models Conflated

**Location:** §Security Considerations, lines 460-465

**Problem:** The RFC presents `set_api_key()` (any-llm style) and per-call `api_key` override (LiteLLM style) as equivalent. They are not:

| Aspect | any-llm style (`set_api_key`) | LiteLLM style (`api_key=...`) |
|--------|-------------------------------|-------------------------------|
| Key storage | Rust memory (enforceable) | Goes directly to provider SDK |
| Budget enforcement | Possible (Rust holds key) | Impossible (SDK bypasses Rust) |
| Virtual key (RFC-0903) | Enforceable | Not enforceable |
| Traceability | Key identity → Rust → Provider | Key identity → Provider directly |

**The RFC claims** (line 460): "Keys stored in memory only, never persisted"

**The RFC does NOT address:** When `api_key="sk-..."` is passed per-call in LiteLLM style, it goes directly to the provider SDK. The Rust core never sees it. Budget enforcement is bypassed.

**This breaks RFC-0903's virtual key enforcement** which requires all requests to pass through the proxy's `validate_key()` middleware. If users pass keys directly to provider SDKs, virtual keys are useless.

**Fix Required:** Explicitly document this trade-off. If LiteLLM mode uses per-call `api_key`, virtual key enforcement does NOT apply. Clarify the trust boundary.

---

### C3: Provider Resolution Silent Fallback is Dangerous

**Location:** §Provider Resolution Algorithm, lines 209-251

**Problem:** When no provider can be determined, the algorithm **silently defaults to OpenAI**:

```python
# 3. Default provider
if deployment_mode == "litellm-mode":
    return "openai", model  # LiteLLM default
elif deployment_mode == "any-llm-mode":
    return "openai", model   # any-llm default
else:  # "full"
    return "openai", model  # Default to OpenAI
```

**Failure modes:**
1. User types `completion(model="gpt-4", messages=[...])` expecting Google provider
2. Algorithm silently uses OpenAI
3. User gets OpenAI's GPT-4, not Google's expected model
4. No error, no warning

**What should happen:** Raise `MissingProviderError` with a message like:
> "Cannot determine provider for model 'gpt-4'. Use provider='...' or prefix model with 'provider:' (e.g., 'google:gpt-4')."

**Fix Required:** Remove silent fallback. Require explicit provider specification.

---

### C4: Model Names Containing Provider Names

**Location:** §Provider Resolution Algorithm, lines 233-240

**Problem:** Some providers have model names that are also valid provider names. Example:

- `openai:openai` — is this provider=openai, model=openai? Or a malformed request?
- `anthropic:anthropic` — same issue

**Current algorithm:**
```python
if ":" in model:
    provider, model_name = model.split(":", 1)
    if is_known_provider(provider):
        return provider, model_name
```

**Edge case:**
- Input: `model="openai:gpt-4o"`
- `provider="openai"`, `model_name="gpt-4o"` → Correct
- Input: `model="openai:openai"` (OpenAI has a model literally named "openai")
- `provider="openai"`, `model_name="openai"` → Technically correct, but ambiguous

**Fix Required:** Add ambiguity detection. If `model_name == provider`, emit a warning or error explaining the ambiguity.

---

### C5: Case Sensitivity of Provider Names

**Location:** §Provider Resolution Algorithm, line 229

**Problem:** The current implementation would treat `"OpenAI"` and `"openai"` as different providers unless normalized.

**The RFC does not specify:**
- Is provider lookup case-sensitive?
- Does `"Provider"="OpenAI"` resolve to the OpenAI provider?
- Does the model string `"OPENAI:gpt-4o"` work?

**LiteLLM compatibility:** LiteLLM's `provider` param is case-insensitive. The RFC should specify this explicitly.

**Fix Required:** Add normalization step: `provider_param.lower()` before lookup.

---

## Important Issues (Should Fix)

### I1: G1 Target Conflict with RFC-0908

**Location:** §Design Goals, line 55 vs RFC-0908 G1

**Problem:** RFC-0908 G1 says `<10ms` function call overhead. RFC-0920 G1 says `<5ms`.

**No justification provided** for the tighter target. PyO3 call overhead alone is typically 1-5ms depending on payload size.

**Fix Required:** Either justify the tighter target or match RFC-0908's `<10ms`.

---

### I2: Streaming Default Mismatch with LiteLLM

**Location:** §Unified Function Signature, line 159

**Problem:** RFC-0920 specifies `stream: Optional[bool] = False`.

**LiteLLM behavior:** `stream` defaults to `None` (which behaves as False for sync, but enables streaming for async).

**Consequence:** `acompletion(model="...", messages=[...])` with no `stream` param:
- LiteLLM: No streaming (defaults to sync behavior)
- RFC-0920: No streaming (explicit `False`)

This happens to match, but `stream=None` vs `stream=False` semantic difference could cause issues.

**Fix Required:** Specify `stream: Optional[bool] = None` to match LiteLLM behavior exactly.

---

### I3: Missing `list_models()` Signature

**Location:** §Package Structure, lines 410-430

**Problem:** The RFC shows `completion()` signature in detail but omits `list_models()`, which LiteLLM users depend on heavily.

**Missing specification:**
- `list_models(provider: Optional[str] = None, api_key: Optional[str] = None, api_base: Optional[str] = None, client_args: Optional[Dict] = None) -> List[Model]`
- What does `list_models()` return? Typed objects? Dicts?
- How is provider determined when not specified?

**Fix Required:** Add complete `list_models()` signature and behavior description.

---

### I4: Return Type is Untyped (`Dict[str, Any]`)

**Location:** §Unified Function Signature, line 187

**Problem:** `acompletion(...) -> Dict[str, Any]`

**Consequences:**
- No IDE autocompletion
- No type checking at development time
- Users must read documentation to know response shape

**LiteLLM compatibility:** LiteLLM returns `ModelResponse` objects with typed fields.

**Fix Required:** Define a `CompletionResponse` dataclass/Pydantic model and return that instead.

---

### I5: `session_label` Is Dropped

**Location:** §Unified Function Signature, line 182

**Problem:** `session_label: Optional[str] = None` is in the signature but never mentioned in the docstring or resolution algorithm.

**What happens:** It's silently passed to `**kwargs` and likely dropped by provider SDKs that don't understand it.

**LiteLLM compatibility:** `session_label` is used for metrics grouping. Dropping it breaks observability.

**Fix Required:** Either:
1. Document that `session_label` is quota-router specific and used for metrics
2. Or explicitly state it's ignored

---

### I6: `client_args` Is Undefined

**Location:** §Unified Function Signature, line 183

**Problem:** `client_args: Optional[Dict] = None` is in the signature but never explained.

**Questions:**
- Is this passed to provider SDKs as-is?
- Is it filtered?
- Does it override `api_key` and `api_base` if set?
- What's the schema?

**Fix Required:** Define `client_args` schema and behavior explicitly.

---

### I7: Exception `code` Field is Unused

**Location:** §Exception Hierarchy, line 276

**Problem:** `QuotaRouterError` has `code: str` but:
- No standard error codes are defined
- No usage of `code` in any exception
- LiteLLM exceptions don't have a `code` field — they use exception type hierarchy

**Fix Required:** Either remove `code` from base exception, or define a standard error code enum.

---

### I8: PyO3 GIL Boundary Claim is Misleading

**Location:** §Security Considerations, line 464

**Problem:** "Provider isolation: Each provider's SDK runs in separate PyO3 GIL boundary"

**Reality:** PyO3 shares the GIL. Providers are not isolated — they serialize on the GIL. "Separate GIL boundary" implies stronger isolation than exists.

**What actually happens:**
- Python → Rust (acquires GIL) → Python SDK call (holds GIL) → Rust → Python
- All provider calls serialize on the GIL

**Fix Required:** Remove "separate GIL boundary" claim. State: "Provider SDK calls are serialized through PyO3's GIL management."

---

## Low Priority Issues

### L1: Phase 1 Says "Real SDK Calls" But Current Code is Mock

**Location:** §Implementation Phases, line 486

**Problem:** Current `quota-router-pyo3` completion functions are mock stubs that echo messages. The RFC says Phase 1 includes "OpenAI provider integration (real SDK calls)".

**This is implementation, not spec. But the RFC should be clearer that Phase 1 replaces mock with real integration.**

---

### L2: No Specification for Deployment Mode Selection

**Problem:** Users need to choose litellm-mode vs any-llm-mode vs full at deployment time. The RFC doesn't explain:
- How is mode selected? (Cargo feature flags? Environment variable?)
- What happens if user installs `pip install quota-router` — which mode do they get?
- Can mode be changed at runtime?

**Fix Required:** Add deployment model selection section.

---

### L3: Batch API Gap

**Problem:** any-llm's batch uses `input_file_path` (local file) + upload. quota-router-pyo3's batch uses `input_file_id` (pre-existing ID). These are incompatible.

**The RFC says** (Phase 3): "Batch API" but doesn't specify the signature.

**Fix Required:** Define batch API signature that works for both styles, or explicitly pick one.

---

### L4: `InsufficientFundsError` Depends on Optional RFC-0904

**Location:** §Dependencies, line 30

**Problem:** RFC-0904 (Real-Time Cost Tracking) is listed as **optional**. But `InsufficientFundsError` (line 321-324) is in the exception hierarchy and references OCTO-W balance.

**If RFC-0904 is not implemented, what does `InsufficientFundsError` mean?**

**Fix Required:** Either:
1. Move RFC-0904 to **Requires**
2. Or remove `InsufficientFundsError` from base spec

---

## Summary Table

| ID | Severity | Issue | Status |
|----|----------|-------|--------|
| C1 | ~~Critical~~ | RFC-0917 mode contradiction | **FIXED** — RFC-0917 v2.25 corrected |
| C2 | Critical | Security model conflict | **FIXED** — Key trust boundary documented in v1.1 |
| C3 | Critical | Silent OpenAI fallback | **FIXED** — Raises MissingProviderError in v1.1 |
| C4 | Important | Model names containing providers | **FIXED** — Ambiguity detection added in v1.1 |
| C5 | Important | Case sensitivity unspecified | **FIXED** — Normalized to lowercase in v1.1 |
| I1 | Important | G1 target conflict | **FIXED** — Matches RFC-0908 (<10ms) in v1.1 |
| I2 | Important | Stream default mismatch | **FIXED** — `None` not `False` in v1.1 |
| I3 | Important | Missing list_models() spec | **FIXED** — Complete signature in v1.1 |
| I4 | Important | Untyped return Dict | **FIXED** — Typed CompletionResponse in v1.1 |
| I5 | Low | session_label dropped | **FIXED** — Handling documented in v1.1 |
| I6 | Low | client_args undefined | **FIXED** — Schema defined in v1.1 |
| I7 | Low | Exception code unused | **FIXED** — Error codes defined in v1.1 |
| I8 | Low | GIL boundary claim wrong | **FIXED** — Isolation language corrected in v1.1 |
| L1 | Low | Mock vs real impl | **FIXED** — Phase 1 clarified in v1.1 |
| L2 | Low | Mode selection unspecified | **FIXED** — Deployment section added in v1.1 |
| L3 | Low | Batch API gap | **FIXED** — Signatures defined in v1.1 |
| L4 | Low | InsufficientFundsError dependency | **FIXED** — RFC-0904 moved to Requires in v1.1 |

---

## Recommendation

**All issues resolved in RFC-0920 v1.1.** Return to review queue.
