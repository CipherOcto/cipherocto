# Mission: RFC-0909 compute_event_id + Deterministic Event ID

## Status

Open (v3)

## RFC

RFC-0909 v59 (Economics): Deterministic Quota Accounting

## Summary

Implement `compute_event_id()` — the deterministic SHA256 hex function that produces identical output across all router implementations. Includes test vectors verifying UUID format (RFC 4122 hyphenated lowercase), little-endian token byte ordering, and cross-router determinism.

## Acceptance Criteria

- [ ] `compute_event_id(request_id, key_id, provider, model, input_tokens, output_tokens, pricing_hash, token_source) -> String`
- [ ] Returns 64-char lowercase hex SHA256
- [ ] UUID format: `key_id.to_string()` uses RFC 4122 hyphenated lowercase (36 chars with hyphens)
- [ ] Token ordering: `input_tokens.to_le_bytes()`, `output_tokens.to_le_bytes()` (little-endian)
- [ ] `pricing_hash` parameter is `&[u8; 32]` (32 raw bytes, NOT hex string)
- [ ] Test vector TV1 passes:
  - Input: `request_id="req-001"`, `key_id="550e8400-e29b-41d4-a716-446655440000"`, `provider="openai"`, `model="gpt-4"`, `input_tokens=100`, `output_tokens=50`, `pricing_hash=00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff` (hex→32 raw bytes), `token_source=ProviderUsage`
  - Expected output: `"8d22792346a0417bb928da0c16f2af5330640678f365d16bc392d400c2aa4ab2"`
- [ ] Test vector TV2 passes:
  - Input: same as TV1 except `request_id="req-002"` and `token_source=CanonicalTokenizer`
  - Expected output: `"0f26450e1734034b9bc6f999b61586c671dd8249002524dd740a94c51ded3f36"`
- [ ] Test vector TV3 passes:
  - Input: same as TV1 except `key_id="660e8400-e29b-41d4-a716-446655440001"` (only key_id changes)
  - Expected output: `"a3e31fbaa4b3bf6fe9d5c1eeb59055cfe4a3389358fc0e38c8820e2c2e6912ed"`
- [ ] Test vector TV4 passes:
  - Input: same as TV1 except `pricing_hash="8b48fe37e84565f99285690a835a881fe2d580ec63775aa5f9465ba38a5a2f60"` (hex→32 raw bytes); `pricing_hash` derived from `SHA256(b"pricing-table-v2")` (UTF-8, no trailing newline)
  - Expected output: `"06a6eb1c68f8a75287d0ac45b1ede9f00cd770f106c505685c299cf3b593726c"`
- [ ] UUID format mandate documented: MUST use `uuid::Uuid::to_string()` (hyphenated lowercase), NOT `to_simple().to_string()` (32-char no hyphen)
- [ ] `validate_request_id(request_id: &str) -> Result<(), KeyError>` — returns `Ok(())` if 1 ≤ len ≤ 1024 bytes, `Err(KeyError::InvalidFormat)` otherwise (H1 + M2)
- [ ] `validate_request_id()` called in `process_response` before `compute_event_id` (M3)

## Implementation Notes

- Location: `crates/quota-router-core/src/keys/models.rs` or new `compute.rs` module
- `compute_event_id` is a standalone function (not a method on any struct)
- `pricing_hash` is passed as `&[u8; 32]` (32 raw bytes) — NOT a hex string
- `token_source` is `TokenSource` enum variant
- `TokenSource::to_hash_str()` must return `"provider"` (ProviderUsage) or `"tokenizer"` (CanonicalTokenizer)
- `validate_request_id(request_id: &str) -> Result<(), KeyError>` validates: rejects empty string, rejects >1024 bytes; returns `Err(KeyError::InvalidFormat)` on rejection. Called in `process_response` before `compute_event_id`.
- **Single-tenant scope** (this mission): function concatenates fields WITHOUT length prefixes or delimiters. This is safe for single-tenant deployments. Multi-tenant deployments require additional mitigations (see RFC-0909 §Security Note — No Field Delimiters).
- **event_id vs request_id encoding (L1):** event_id is hex-encoded (compute_event_id returns 64-char hex String for API compat). request_id is raw SHA256 binary stored as BLOB(32) — gateway text is hashed, not hex-encoded. These are different encodings: do not confuse them.

## Test Vector Setup Notes

- `pricing_hash` test values are hex notation. Tests must decode hex → 32 raw bytes before calling `compute_event_id`
- TV4 `pricing_hash` generation: `SHA256(b"pricing-table-v2")` → 32 raw bytes → hex encode → `"8b48fe37e84565f99285690a835a881fe2d580ec63775aa5f9465ba38a5a2f60"`

## Reference

- RFC-0909 §compute_event_id
- RFC-0909 §validate_request_id
- RFC-0909 §UUID Format Mandate
- RFC-0909 §Test Vectors for Cross-Router Determinism (TV1-TV4)
- RFC-0909 §Security Note — No Field Delimiters

## Dependencies

- TokenSource enum with `to_hash_str()` is defined in RFC-0909 §Usage Event Model (already in `models.rs` — no external dependency)
- `sha2 = "0.10"` (for SHA256 in compute_event_id)
- `uuid = "1.x"` (for uuid::Uuid)

## Complexity

Medium — requires understanding of deterministic hashing, exact test vector matching, and UUID byte ordering

---
**Mission Type:** Implementation
**Priority:** Critical
**Phase:** RFC-0909 Phase 1 Core

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| v3 | 2026-04-20 | Round 2 adversarial review fixes: fix C1 (add sha2 crate dependency); fix H1 (specify KeyError::InvalidFormat for validate_request_id); fix M1 (fix TokenSource dependency description — RFC-0909 defines it, not external); fix M2 (add validate_request_id as explicit AC item); fix M3 (note validate_request_id called in process_response); fix L1 (add event_id vs request_id encoding distinction) |
| v2 | 2026-04-20 | Round 1 adversarial review fixes: fix C1/C2 (TV2 uses request_id="req-002", restore correct expected output); fix C3 (TV3 description clarifies only key_id changed); fix C4 (TV4: pricing_hash is hex notation for 32 raw bytes, decode before calling); add validate_request_id to acceptance criteria; add test vector setup notes; clarify single-tenant scope; add TokenSource dependency note |
| v1 | 2026-04-20 | Initial |
