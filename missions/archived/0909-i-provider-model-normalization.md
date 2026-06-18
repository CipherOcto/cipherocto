# Mission: RFC-0909 Provider/Model Normalization — CONSISTENCY GOAL Implementation

## Status

Archived

## RFC

RFC-0909 v69 (Accepted): Deterministic Quota Accounting

## Dependencies

None (can proceed independently)

## Summary

Implement provider/model normalization at the gateway input boundary to fulfill the CONSISTENCY GOAL from RFC-0909 v69. The RFC specifies routers SHOULD normalize provider/model to lowercase ASCII at gateway input.

## Acceptance Criteria

- [x] `unicode-normalization` crate added to `Cargo.toml`
- [x] `normalize_provider_model(provider, model)` function added to `crates/quota-router-core/src/keys/mod.rs` — applies NFC normalization then lowercase ASCII
- [x] Gateway input layer (middleware) normalizes `provider` and `model` to lowercase ASCII before storage and before calling `compute_event_id`
- [x] Normalization applied at `process_response` entry point in `middleware.rs` (before `compute_event_id` is called)
- [x] Test vector TV1 still passes (provider="openai", model="gpt-4" — already lowercase)
- [x] Add test case: provider="OpenAI", model="GPT-4" → normalized to "openai", "gpt-4" → same event_id as lowercase version
- [x] `cargo clippy --all-targets --all-features -- -D warnings` passes with zero warnings
- [x] `cargo test --lib` passes

## Claimant

@claude-code

## Pull Request

https://github.com/CipherOcto/cipherocto/commit/5faf31f

## Notes

## Implementation Notes

**File:** `crates/quota-router-core/src/keys/mod.rs` (normalization function)

**File:** `crates/quota-router-core/src/middleware.rs` (call site)

**Normalization function to add in `keys/mod.rs`:**
```rust
pub fn normalize_provider_model(provider: &str, model: &str) -> (String, String) {
    use unicode_normalization::UnicodeNormalization;
    let p = provider.nfc().collect::<String>().to_lowercase();
    let m = model.nfc().collect::<String>().to_lowercase();
    (p, m)
}
```

**Call site in `middleware.rs` `process_response` — apply normalization BEFORE compute_event_id AND before SpendEvent construction:**
```rust
// Validate request_id first — must be 1..=1024 bytes
crate::keys::validate_request_id(request_id)?;

// Normalize provider/model per RFC-0909 CONSISTENCY GOAL — must apply to BOTH:
// (1) compute_event_id inputs, and (2) SpendEvent.provider/SpendEvent.model storage
let (provider, model) = crate::keys::normalize_provider_model(provider, model);

// (1) Compute deterministic event_id with normalized inputs
let event_id = crate::keys::compute_event_id(
    request_id,
    &key_id,
    &provider,
    &model,
    input_tokens,
    output_tokens,
    &pricing_hash,
    token_source,
);

// (2) Build SpendEvent with the SAME normalized local variables (not the original parameters)
// provider and model are now owned String locals from normalize_provider_model
let event = SpendEvent {
    event_id: event_id.clone(),
    request_id: request_id.to_string(),
    key_id,
    team_id,
    provider: provider,   // ← normalized String, directly (no .to_string() needed)
    model: model,         // ← normalized String, directly (no .to_string() needed)
    // ...
};
```

**CRITICAL:** The normalized `provider` and `model` (owned `String` locals from `normalize_provider_model`) MUST be used for BOTH `compute_event_id` AND `SpendEvent` construction. Do NOT pass the original `&str` parameters to `SpendEvent.provider`/`SpendEvent.model`.

**Cargo dependency to add:**
```toml
unicode-normalization = "0.1.25"
```

**Test to add in `keys/mod.rs` `compute_event_id_tests`:**
```rust
#[test]
fn test_normalize_provider_model() {
    let (p, m) = normalize_provider_model("OpenAI", "GPT-4");
    assert_eq!(p, "openai");
    assert_eq!(m, "gpt-4");

    // Already lowercase: unchanged
    let (p, m) = normalize_provider_model("openai", "gpt-4");
    assert_eq!(p, "openai");
    assert_eq!(m, "gpt-4");
}
```
