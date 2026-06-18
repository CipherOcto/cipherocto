# Adversarial Review Round 1: Mission 0909-i — RFC-0909 v69 Normalization

**Reviewer:** Code Review Agent
**Date:** 2026-04-25
**Mission:** `missions/open/0909-i-provider-model-normalization.md` (v1, initial)
**RFC:** RFC-0909 v69 (Accepted): Deterministic Quota Accounting
**Code:** `crates/quota-router-core/src/keys/mod.rs`, `crates/quota-router-core/src/middleware.rs`

---

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 2 |
| HIGH | 1 |
| MEDIUM | 1 |
| LOW | 0 |

Mission specification is correct — implementation does not exist. Two CRITICAL gaps: missing `unicode-normalization` dependency and normalization function not applied in `process_response`. One HIGH: normalization function not yet implemented. One MEDIUM: mixed-case test case missing.

---

## CRITICAL Issues

### CRITICAL-1: `unicode-normalization` Crate Not in Dependencies

**Finding:** Mission Implementation Notes specify using `unicode-normalization` crate for NFC normalization, but the crate is not listed in `crates/quota-router-core/Cargo.toml`.

**Evidence:**
```bash
$ grep "unicode-normalization" crates/quota-router-core/Cargo.toml
# (no output — crate is not listed)
```

**Impact:** Without this dependency, the normalization function cannot be implemented as specified.

**Fix required:** Add to `Cargo.toml`:
```toml
unicode-normalization = "1.11"
```

---

### CRITICAL-2: `process_response` Does Not Apply Normalization Before `compute_event_id`

**Finding:** `middleware.rs` `process_response` (lines 134-189) passes `provider` and `model` directly to `compute_event_id` without calling any normalization function:

```rust
let event_id = crate::keys::compute_event_id(
    request_id,
    &key_id,
    provider,   // ← raw, un-normalized
    model,      // ← raw, un-normalized
    input_tokens,
    output_tokens,
    &pricing_hash,
    token_source,
);
```

The RFC (lines 278-283) explicitly states: "The router MUST apply normalization at the gateway input boundary before storage and before calling this function."

**Impact:** If a gateway passes mixed-case provider/model (e.g., "OpenAI"/"GPT-4"), the event_id will differ from a router that receives lowercase. This breaks cross-router idempotency.

**Fix required:** Apply `normalize_provider_model` to `provider` and `model` before calling `compute_event_id`:
```rust
let (provider, model) = crate::keys::normalize_provider_model(provider, model);
```

---

## HIGH Issues

### HIGH-1: `normalize_provider_model` Function Does Not Exist

**Finding:** The mission Implementation Notes show a `normalize_provider_model` function in `crates/quota-router-core/src/keys/mod.rs`, but grep shows no such function exists in the codebase:

```bash
$ grep -n "normalize_provider_model" crates/quota-router-core/src/keys/mod.rs
# (no output)
```

**Fix required:** Implement the function in `keys/mod.rs`:
```rust
pub fn normalize_provider_model(provider: &str, model: &str) -> (String, String) {
    use unicode_normalization::UnicodeNormalization;
    let p = provider.nfc().collect::<String>().to_lowercase();
    let m = model.nfc().collect::<String>().to_lowercase();
    (p, m)
}
```

---

## MEDIUM Issues

### MED-1: Mixed-Case Test Case Absent

**Finding:** Mission Acceptance Criteria require:
> Add test case with mixed-case input: provider="OpenAI", model="GPT-4" → normalized to "openai", "gpt-4" → same event_id as lowercase version

The `compute_event_id_tests` module (lines 912-1068) has no such test. Existing tests only use pre-lowercased inputs.

**Fix required:** Add test case:
```rust
#[test]
fn test_compute_event_id_mixed_case_normalization() {
    // Mixed-case inputs should produce same event_id as lowercase
    let request_id = "req-001";
    let key_id = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
    let input_tokens = 100u32;
    let output_tokens = 50u32;
    let pricing_hash =
        hex_to_32_bytes("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff");
    let token_source = TokenSource::ProviderUsage;

    let lowercase_event_id = compute_event_id(
        request_id, &key_id, "openai", "gpt-4",
        input_tokens, output_tokens, &pricing_hash, token_source,
    );

    let mixed_case_event_id = compute_event_id(
        request_id, &key_id, "OpenAI", "GPT-4",
        input_tokens, output_tokens, &pricing_hash, token_source,
    );

    assert_eq!(
        mixed_case_event_id, lowercase_event_id,
        "Mixed-case inputs must normalize to same event_id as lowercase"
    );
}
```

Note: This test will FAIL until CRITICAL-2 is fixed (normalization is applied in `process_response`). The test verifies `compute_event_id` itself is case-sensitive, while the fix belongs at the `process_response` call site.

**Alternative interpretation:** If the intent is that `compute_event_id` itself should normalize internally, then the test would pass without the `process_response` fix. However, RFC-0909 line 278-283 says the normalization must happen "before calling this function" — at the gateway boundary, not inside `compute_event_id`. So the test belongs at the `process_response` level or as a standalone normalization unit test, not as a test of `compute_event_id` alone.

**Recommendation:** Add a `normalize_provider_model` unit test instead:
```rust
#[test]
fn test_normalize_provider_model() {
    let (p, m) = normalize_provider_model("OpenAI", "GPT-4");
    assert_eq!(p, "openai");
    assert_eq!(m, "gpt-4");

    // Already lowercase unchanged
    let (p, m) = normalize_provider_model("openai", "gpt-4");
    assert_eq!(p, "openai");
    assert_eq!(m, "gpt-4");
}
```

And keep the mixed-case test at the `process_response` level once that fix is in place.

---

## LOW Issues

### LOW-1: None

All issues found are CRITICAL/HIGH/MEDIUM.

---

## Pre-Existing Issues (Not Introduced by Mission)

### KNOWN-1: `unicode-normalization` Dependency Missing

Already missing before this mission. Will be fixed as part of this mission.

---

## What Was Fixed in This Round ✅

Nothing — initial review.

---

## Required Fixes Summary

| Issue | Priority | Fix | Status |
|-------|----------|-----|--------|
| CRITICAL-1: unicode-normalization not in Cargo.toml | MUST FIX | Add crate dependency | ❌ Not fixed |
| CRITICAL-2: process_response doesn't normalize | MUST FIX | Call normalize_provider_model before compute_event_id | ❌ Not fixed |
| HIGH-1: normalize_provider_model function missing | MUST FIX | Implement in keys/mod.rs | ❌ Not fixed |
| MED-1: Mixed-case test case absent | SHOULD FIX | Add normalize_provider_model unit test + process_response integration test | ❌ Not fixed |

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| v1.0 | 2026-04-25 | Initial Round 1 adversarial review — 2 CRITICAL, 1 HIGH, 1 MED, 0 LOW |
