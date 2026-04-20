# Mission: RFC-0909 BLOB Storage Boundary Helpers

## Status

Open (v4)

## RFC

RFC-0909 v59 (Economics): Deterministic Quota Accounting

## Summary

Implement BLOB storage boundary helper functions for converting between application-layer types (String, uuid::Uuid) and database-layer raw bytes (BLOB). Required for RFC-0903-B1/B1 compliance in SpendEvent storage.

## Acceptance Criteria

- [ ] `hex_to_blob_32(hex_str: &str) -> [u8; 32]` — hex string (64 chars) → raw 32 bytes for event_id BLOB(32) storage
- [ ] `blob_32_to_hex(blob: &[u8; 32]) -> String` — raw 32 bytes → hex string for event_id API responses. **Critical constraint:** This function does NOT apply to request_id, which is stored as raw binary BLOB(32), not hex. Never use `blob_32_to_hex` on request_id data.
- [ ] `uuid_to_blob_16(uuid: &uuid::Uuid) -> [u8; 16]` — Uuid → raw 16 bytes for key_id BLOB(16) storage
- [ ] `blob_16_to_uuid(blob: &[u8; 16]) -> uuid::Uuid` — raw 16 bytes → Uuid from key_id BLOB(16) retrieval. **Important:** `uuid::Uuid::from_bytes` has undefined behavior for invalid 16-byte sequences (invalid version/variant bits). Per RFC-0903-B1: "UUIDs with invalid version or variant bits MUST be rejected." Implement validation before construction, or document that downstream validation will catch invalid UUIDs.
- [ ] All functions are `#[inline]` for zero-cost abstraction
- [ ] `hex_to_blob_32` uses `hex::decode` and panics on invalid hex (implementation bug, not user input)

## Implementation Notes

- Location: `crates/quota-router-core/src/keys/models.rs` (near SpendEvent) or `crates/quota-router-core/src/storage.rs`
- `hex_to_blob_32` / `blob_32_to_hex`: for event_id (BLOB(32) per RFC-0903-B1)
- `uuid_to_blob_16` / `blob_16_to_uuid`: for key_id (BLOB(16) per RFC-0903-B1) and team_id (BLOB(16) per RFC-0903-C1)
- Note: `blob_32_to_hex` returns `hex::encode(blob)` — this is the reverse of `hex::decode`
- These functions are the storage boundary — they should be called ONLY at the INSERT/SELECT boundary, not inside business logic
- **Error handling asymmetry (M1):** `hex_to_blob_32` panics on invalid hex input (intentional: programming errors should abort). `blob_16_to_uuid` silently accepts any 16 bytes — invalid UUIDs will fail downstream validation, not at the boundary. Document this distinction in code comments.
- **Wrong-data path (M2):** These helpers are low-level conversion functions with no type checking. Callers MUST ensure the correct helper is used for each field. Using `blob_32_to_hex` on a request_id BLOB produces garbage (raw SHA256 → 64 hex chars that don't match original gateway text).
- **request_id constraint (H1 + L2):** request_id is stored as raw SHA256 binary (BLOB(32)), NOT hex. This differs from event_id which is stored as hex-encoded SHA256. Per RFC-0903-B1: `encode_request_id()` = `SHA256(gateway_request_id_text)` → raw bytes stored directly. `encode_request_id()` is defined in RFC-0903-B1 §request_id, not RFC-0909.

## Dependencies

- `uuid = "1.x"` for uuid::Uuid
- `hex = "0.4"` for hex encode/decode

## Reference

- RFC-0909 §hex_to_blob_32
- RFC-0909 §blob_32_to_hex
- RFC-0909 §uuid_to_blob_16
- RFC-0909 §blob_16_to_uuid
- RFC-0903-B1 §Storage Encoding
- RFC-0903-B1 §request_id (encode_request_id function, not defined in RFC-0909)

## Complexity

Low — pure conversion functions with direct byte manipulation

---
**Mission Type:** Implementation
**Priority:** Critical
**Phase:** RFC-0909 Phase 1 Core

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| v4 | 2026-04-20 | Round 3 adversarial review fixes: fix M1 (Dependencies section now correctly placed before Reference) |
| v3 | 2026-04-20 | Round 2 adversarial review fixes: fix M1 (remove redundant AC line 23 — function signature already guarantees 16 bytes); fix L1 (move Dependencies before Reference for consistency with other missions) |
| v2 | 2026-04-20 | Round 1 adversarial review fixes: fix C1/H2 (add RFC-0903-B1 §request_id to references + impl notes); fix C2 (document uuid::Uuid::from_bytes undefined behavior on invalid bytes); fix H1 (add blob_32_to_hex must-not-be-used-for-request_id constraint); fix M1 (document panic vs silent failure asymmetry); fix M2 (add wrong-data-path note); fix L1 (add uuid crate dependency); fix L2 (clarify request_id is raw SHA256 binary, not hex) |
