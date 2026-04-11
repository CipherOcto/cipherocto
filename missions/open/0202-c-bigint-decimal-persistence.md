# Mission: RFC-0202-A Phase 2 — Persistence and BTree Indexing

## Status

Open

## RFC

RFC-0202-A (Storage): Stoolap BIGINT and DECIMAL Core Types

## Summary

Add persistence layer support for BIGINT and DECIMAL: wire tags 13/14 in serialize/deserialize, NUMERIC_SPEC_VERSION header wiring, BTree index type selection, and lexicographic key encoding. **Production deployment is blocked until lexicographic encoding is verified.**

## Acceptance Criteria

- [ ] Wire tag 13 arm added to `serialize_value` for BIGINT: `[13][BigIntEncoding bytes]`
- [ ] Wire tag 14 arm added to `serialize_value` for DECIMAL: `[14][decimal_to_bytes]`
- [ ] Wire tag 13 handler added to `deserialize_value` reconstructing BigInt from BigIntEncoding:
  - Read `num_limbs` from byte offset 4 of the BigIntEncoding header
  - Compute total size = 8 + num_limbs * 8 bytes
  - Bounds-check: return `Error::internal("truncated bigint data")` if `rest.len() < total`
  - Slice `&rest[..total]` before passing to `BigInt::deserialize()` — do NOT pass entire `rest` slice
  - Caller must advance buffer by `total` bytes after deserialization
- [ ] Wire tag 14 handler added to `deserialize_value` reconstructing Decimal from 24-byte encoding
- [ ] Debug assertion added: place inside the generic Extension arm (tag 11 branch) — if tag byte is 13 or 14 and the code reaches the generic arm, the assertion fires. **Note:** With correct arm ordering (see Mission Notes), wire tags 13/14 are handled by dedicated arms before the generic branch, so this assertion is a defensive check against future arm-reordering bugs.
- [ ] `auto_select_index_type()` updated: `DataType::Bigint | DataType::Decimal → IndexType::BTree`
- [ ] `NUMERIC_SPEC_VERSION` wired to WAL/snapshot header read/write per RFC §4a:
  - Read version from bytes 0–3 of WAL segment header (u32 little-endian) on recovery
  - Write version to same offset on WAL segment creation (default = 2 for new databases)
  - Header upgrade to version 2 triggered when DDL uses BIGINT/DECIMAL keywords in a version-1 database
  - Header upgrade and DDL commit occur in the same WAL transaction (atomic)
  - If WAL segment is corrupt (checksum failure), skip entire segment — no partial replay
- [ ] **Lexicographic key encoding implemented and verified for BIGINT** (§6.11 format):
  - Format: `[limb_count_with_sign: u8][limb0: BE][limb1: BE]...[limbN: BE][zero_pad: 8 × (64 − N)]` — 521 bytes max
  - Sign encoding: positive = `num_limbs | 0x80`; negative = `0x80 − num_limbs`
  - Ordering: negative < zero < positive; limb-by-limb big-endian within same sign
  - Verification test vectors: `-2^64 < -1 < 0 < 1 < 2^64` in encoded key space
  - Verify encoded key length is 521 bytes (1 byte sign-prefix + 64 × 8 bytes padded limbs)
- [ ] **Lexicographic key encoding implemented and verified for DECIMAL** (§6.11 format):
  - Format: `[mantissa_byte0_xor_0x80][mantissa_bytes_1_15][scale: BE u8]` — 17 bytes total
  - Sign-flip: XOR byte 0 of mantissa with `0x80`; zero mantissa encodes as `0x80...00`
  - Zero mantissa sorts between negatives and positives (not below all negatives)
  - Verification test vectors: `-12.3 < 0 < 12.3` in encoded key space
  - Verify scale byte appended as BE u8 at byte 16
- [ ] REINDEX documentation added for BIGINT/DECIMAL BTree indexes:
  - Existing BTree indexes on BIGINT/DECIMAL columns must be rebuilt after deploying lexicographic encoding
  - For version-1 databases: existing columns stored as Integer/Float do not need reindexing
  - Recommended migration path: `REINDEX INDEX idx_name` or `CREATE INDEX ... USING btree (col) WITH (encoding = 'lexicographic')` for online migration

## Dependencies

- Mission: 0202-a-bigint-decimal-typesystem (open)
- Mission: 0202-b-bigint-decimal-schema-value (open)
- Mission: 0110-wal-numeric-spec-version (open)

## Location

`/home/mmacedoeu/_w/databases/stoolap/src/storage/mvcc/persistence.rs`
`/home/mmacedoeu/_w/databases/stoolap/src/storage/mvcc/table.rs`
`/home/mmacedoeu/_w/databases/stoolap/src/core/types.rs`

## Complexity

High — lexicographic encoding requires careful implementation; blocking for production

## Notes

**serialize_value arm ordering:** Wire tag 13 (BIGINT) and 14 (DECIMAL) arms MUST appear **before** the generic Extension arm (tag 11) in `serialize_value`. If the generic arm is placed first, BIGINT/DECIMAL values fall through to it, losing the dedicated wire tag optimization (5 bytes per value). AC-3 (debug assertion) is the defense against this ordering bug.

**deserialize arm ordering:** Wire tag 13 (BIGINT) and 14 (DECIMAL) handlers MUST appear **before** the generic Extension handler (tag 11) in `deserialize_value`. If tag 11 appears before 13/14, a BIGINT value (wire tag 13) would be misread as a generic Extension and parsed incorrectly. This is the same ordering principle as `serialize_value`.

## Reference

- RFC-0202-A §5 (Persistence Wire Format)
- RFC-0202-A §6.10 (BTree index type selection)
- RFC-0202-A §6.11 (Lexicographic key encoding)
- RFC-0202-A §Storage Overhead (521 bytes max for BIGINT serialized)
