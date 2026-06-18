# Mission: WAL Header Numeric Spec Version Infrastructure

## Status

Open

## RFC

RFC-0110 (Numeric): Deterministic BIGINT
RFC-0111 (Numeric): Deterministic DECIMAL
RFC-0104 (Numeric): Deterministic Floating-Point
RFC-0105 (Numeric): Deterministic Quant Arithmetic

## Summary

Add WAL header read/write infrastructure for NUMERIC_SPEC_VERSION. This enables spec version pinning and replay for all numeric types.

## Acceptance Criteria

- [ ] WALManager API supports reading/writing header fields including numeric_spec_version
- [ ] Recovery path passes spec version to DDL replay callback
- [ ] `from_str_versioned()` is called with correct spec version during WAL replay
- [ ] Each numeric type (DFP, DQA, DECIMAL, BigInt) uses pinned spec version during replay

## Dependencies

- Mission: 0110-bigint-consensus-integration (in progress)
- Mission: 0105-dqa-consensus-integration (open)
- Mission: 0104-dfp-consensus-integration (claimed, blocked)

## Location

`/home/mmacedoeu/_w/databases/stoolap/src/storage/mvcc/`

## Complexity

High

## Reference

- docs/reviews/round-10-rfc-0202-adversarial.md (M3 finding)
- RFC-0110 §Spec Version & Replay Pinning
- RFC-0111 §Spec Version & Replay Pinning
- RFC-0104 §Spec Version (dfp_spec_version)
- RFC-0105 §Spec Version (DQA_SPEC_VERSION)
