# Mission: RFC-0909 Provider/Model Normalization — CONSISTENCY GOAL Implementation

## Status

Open

## RFC

RFC-0909 v69 (Accepted): Deterministic Quota Accounting

## Dependencies

None (can proceed independently)

## Summary

Implement provider/model normalization at the gateway input boundary to fulfill the CONSISTENCY GOAL from RFC-0909 v69. The RFC specifies routers SHOULD normalize provider/model to lowercase ASCII at gateway input.

## Acceptance Criteria

- [ ] Gateway input layer normalizes `provider` and `model` to lowercase ASCII before storage and before calling `compute_event_id`
- [ ] Unicode NFC normalization applied for any non-ASCII characters (via `unicode-normalization` crate)
- [ ] Normalization applied at `process_response` entry point (before `compute_event_id` is called)
- [ ] Test vector TV1 still passes (provider="openai", model="gpt-4" — already lowercase)
- [ ] Add test case with mixed-case input: provider="OpenAI", model="GPT-4" → normalized to "openai", "gpt-4" → same event_id as lowercase version
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes with zero warnings
- [ ] `cargo test --lib` passes

## Implementation Notes

**File:** `crates/quota-router-core/src/keys/mod.rs` or gateway input layer

**Normalization function to add:**
```rust
fn normalize_provider_model(provider: &str, model: &str) -> (String, String) {
    use unicode_normalization::UnicodeNormalization;
    let p = provider.nfc().collect::<String>().to_lowercase();
    let m = model.nfc().collect::<String>().to_lowercase();
    (p, m)
}
```

**Call site:** In `process_response`, apply normalization before passing to `compute_event_id`.
