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
- [ ] Wire tag 13 handler added to `deserialize_value` reconstructing BigInt from BigIntEncoding
- [ ] Wire tag 14 handler added to `deserialize_value` reconstructing Decimal from 24-byte encoding
- [ ] Debug assertion added in generic Extension branch to catch wire tags 13/14 reaching it (prevents storage overhead regression)
- [ ] `auto_select_index_type()` updated: `DataType::Bigint | DataType::Decimal → IndexType::BTree`
- [ ] `NUMERIC_SPEC_VERSION` wired to WAL/snapshot header read/write (see mission 0110-wal-numeric-spec-version)
- [ ] **Lexicographic key encoding implemented for BIGINT** (§6.11 format: length-prefix with sign in byte 0, 64-limb fixed-width padding)
- [ ] **Lexicographic key encoding implemented for DECIMAL** (§6.11 format: sign-flip in mantissa byte 0, scale as BE u8)
- [ ] REINDEX documentation added for existing BTree indexes on BIGINT/DECIMAL columns

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

## Reference

- RFC-0202-A §5 (Persistence Wire Format)
- RFC-0202-A §6.10 (BTree index type selection)
- RFC-0202-A §6.11 (Lexicographic key encoding)
- RFC-0202-A §Storage Overhead (521 bytes max for BIGINT serialized)
