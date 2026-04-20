# Mission: RFC-0909 BLOB Storage Boundary Helpers

## Status

Open

## RFC

RFC-0909 v59 (Economics): Deterministic Quota Accounting

## Summary

Implement BLOB storage boundary helper functions for converting between application-layer types (String, uuid::Uuid) and database-layer raw bytes (BLOB). Required for RFC-0903-B1/B1 compliance in SpendEvent storage.

## Acceptance Criteria

- [ ] `hex_to_blob_32(hex_str: &str) -> [u8; 32]` — hex string (64 chars) → raw 32 bytes for event_id BLOB(32) storage
- [ ] `blob_32_to_hex(blob: &[u8; 32]) -> String` — raw 32 bytes → hex string for event_id API responses
- [ ] `uuid_to_blob_16(uuid: &uuid::Uuid) -> [u8; 16]` — Uuid → raw 16 bytes for key_id BLOB(16) storage
- [ ] `blob_16_to_uuid(blob: &[u8; 16]) -> uuid::Uuid` — raw 16 bytes → Uuid from key_id BLOB(16) retrieval
- [ ] All functions are `#[inline]` for zero-cost abstraction
- [ ] `hex_to_blob_32` uses `hex::decode` and panics on invalid hex (implementation bug, not user input)
- [ ] `blob_16_to_uuid` uses `uuid::Uuid::from_bytes(*blob)` and assumes 16-byte input

## Implementation Notes

- Location: `crates/quota-router-core/src/keys/models.rs` (near SpendEvent) or `crates/quota-router-core/src/storage.rs`
- `hex_to_blob_32` / `blob_32_to_hex`: for event_id (BLOB(32) per RFC-0903-B1)
- `uuid_to_blob_16` / `blob_16_to_uuid`: for key_id (BLOB(16) per RFC-0903-B1) and team_id (BLOB(16) per RFC-0903-C1)
- Note: `blob_32_to_hex` returns `hex::encode(blob)` — this is the reverse of `hex::decode`
- These functions are the storage boundary — they should be called ONLY at the INSERT/SELECT boundary, not inside business logic

## Reference

- RFC-0909 §hex_to_blob_32
- RFC-0909 §blob_32_to_hex
- RFC-0909 §uuid_to_blob_16
- RFC-0909 §blob_16_to_uuid
- RFC-0903-B1 §Storage Encoding

## Complexity

Low — pure conversion functions with direct byte manipulation

---
**Mission Type:** Implementation
**Priority:** Critical
**Phase:** RFC-0909 Phase 1 Core
