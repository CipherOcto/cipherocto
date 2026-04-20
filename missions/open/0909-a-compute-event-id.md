# Mission: RFC-0909 compute_event_id + Deterministic Event ID

## Status

Open

## RFC

RFC-0909 v59 (Economics): Deterministic Quota Accounting

## Summary

Implement `compute_event_id()` — the deterministic SHA256 hex function that produces identical output across all router implementations. Includes test vectors verifying UUID format (RFC 4122 hyphenated lowercase), little-endian token byte ordering, and cross-router determinism.

## Acceptance Criteria

- [ ] `compute_event_id(request_id, key_id, provider, model, input_tokens, output_tokens, pricing_hash, token_source) -> String`
- [ ] Returns 64-char lowercase hex SHA256
- [ ] UUID format: `key_id.to_string()` uses RFC 4122 hyphenated lowercase (36 chars with hyphens)
- [ ] Token ordering: `input_tokens.to_le_bytes()`, `output_tokens.to_le_bytes()` (little-endian)
- [ ] Test vector TV1 passes: `"req-001"`, `"550e8400-e29b-41d4-a716-446655440000"`, `"openai"`, `"gpt-4"`, `100`, `50`, pricing_hash, ProviderUsage → `"8d22792346a0417bb928da0c16f2af5330640678f365d16bc392d400c2aa4ab2"`
- [ ] Test vector TV2 passes: same as TV1 but CanonicalTokenizer → `"0f26450e1734034b9bc6f999b61586c671dd8249002524dd740a94c51ded3f36"`
- [ ] Test vector TV3 passes: different key_id (`"660e8400-e29b-41d4-a716-446655440001"`) → `"a3e31fbaa4b3bf6fe9d5c1eeb59055cfe4a3389358fc0e38c8820e2c2e6912ed"`
- [ ] Test vector TV4 passes: different pricing_hash (`"8b48fe37e84565f99285690a835a881fe2d580ec63775aa5f9465ba38a5a2f60"`) → `"06a6eb1c68f8a75287d0ac45b1ede9f00cd770f106c505685c299cf3b593726c"`
- [ ] UUID format mandate documented: MUST use `uuid::Uuid::to_string()` (hyphenated lowercase), NOT `to_simple().to_string()` (32-char no hyphen)

## Implementation Notes

- Location: `crates/quota-router-core/src/keys/models.rs` or new `compute.rs` module
- `compute_event_id` is a standalone function (not a method on any struct)
- `pricing_hash` is passed as `&[u8; 32]` (32 raw bytes)
- `token_source` is `TokenSource` enum variant
- Security note: function concatenates fields WITHOUT length prefixes or delimiters — multi-tenant deployments MUST implement one of two mitigations (see RFC-0909 §Security Note — No Field Delimiters)

## Reference

- RFC-0909 §compute_event_id
- RFC-0909 §UUID Format Mandate
- RFC-0909 §Test Vectors for Cross-Router Determinism (TV1-TV4)

## Complexity

Medium — requires understanding of deterministic hashing and exact test vector matching

---
**Mission Type:** Implementation
**Priority:** Critical
**Phase:** RFC-0909 Phase 1 Core
