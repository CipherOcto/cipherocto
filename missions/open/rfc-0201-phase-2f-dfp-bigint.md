# Mission: RFC-0201 Phase 2f — DFP and BigInt Dispatcher Integration

## Status

Open

## RFC

RFC-0201 (Storage): Binary BLOB Type for Deterministic Hash Storage

## Dependencies

- RFC-0104 (Numeric): Deterministic Floating Point — Accepted
- RFC-0110 (Numeric): Deterministic BigInt — Accepted
- `octo-determin` crate in stoolap (provides `Dfp`, `DfpEncoding`, `BigInt` types)
- Mission: RFC-0201 Phase 2a/2b/2c/2e (BYTEA Core) — must be completed first

## Acceptance Criteria

- [ ] `Value::Dfp` round-trip: serialize → deserialize preserves DFP value
- [ ] `Value::BigInt` round-trip: serialize → deserialize preserves BigInt value
- [ ] `serialize_value` has arms for `Value::Dfp` (wire tag 13) and `Value::BigInt` (wire tag 14)
- [ ] `deserialize_value` handles wire tags 13 and 14
- [ ] `Value::from_typed` and `cast_to_type` have DFP and BigInt coercion paths
- [ ] `NUMERIC_SPEC_VERSION` bumped to 2 after BigInt implementation (per RFC-0110 governance)
- [ ] `cargo test` passes
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes

## Description

Implement `serialize_dfp`/`deserialize_dfp` and `serialize_bigint`/`deserialize_bigint` in the RFC-0201 dispatcher, replacing the `Err(DcsError)` stubs. Both RFC-0104 (DFP, 24-byte canonical format) and RFC-0110 (BigInt, little-endian limb array) are Accepted and specify complete wire formats.

## Technical Details

### DFP (RFC-0104) — Wire Tag 13

Per RFC-0104, DFP uses a 24-byte canonical format: sign(1 byte) + exponent(2 bytes) + mantissa(21 bytes).

The `octo-determin` crate provides:
```rust
DfpEncoding::from_dfp(&dfp).to_bytes()  // → [u8; 24]
DfpEncoding::from_bytes(bytes).to_dfp()  // → Dfp
```

`Value::Dfp` already exists in stoolap (used for `Value::dfp()`, `Value::as_dfp()` etc.). The missing piece is the **dispatcher integration** in serialization.

Wire format (tag 13):
```
[u8: 13] [u8: sign] [u8: exp_hi] [u8: exp_lo] [u8 x 21: mantissa]
```

### BigInt (RFC-0110) — Wire Tag 14

Per RFC-0110, BigInt uses little-endian limb array format:
- 4-byte little-endian limb count N
- N × 8-byte little-endian limbs, least-significant first

```rust
fn serialize_bigint(bigint: &BigInt) -> Vec<u8> {
    let limbs = bigint.to_limbs(); // Vec<u64> little-endian
    let mut buf = Vec::with_capacity(4 + limbs.len() * 8);
    buf.extend_from_slice(&(limbs.len() as u32).to_le_bytes());
    for limb in limbs {
        buf.extend_from_slice(&limb.to_le_bytes());
    }
    buf
}
```

### Serialization (`src/storage/mvcc/persistence.rs`)

Add arms to `serialize_value`:
```rust
Value::Dfp(dfp) => {
    buf.push(13); // wire tag 13 for DFP
    buf.extend_from_slice(&DfpEncoding::from_dfp(dfp).to_bytes());
}
Value::BigInt(bigint) => {
    buf.push(14); // wire tag 14 for BigInt
    buf.extend_from_slice(&serialize_bigint(bigint));
}
```

Add arms to `deserialize_value`:
```rust
13 => {
    // DFP — 24 bytes
    if rest.len() < 24 {
        return Err(Error::internal("missing DFP data"));
    }
    let encoding_bytes: [u8; 24] = rest[..24].try_into().unwrap();
    let dfp = DfpEncoding::from_bytes(encoding_bytes).to_dfp();
    Ok(Value::Dfp(dfp))
}
14 => {
    // BigInt
    if rest.len() < 4 {
        return Err(Error::internal("missing BigInt limb count"));
    }
    let limb_count = u32::from_le_bytes(rest[..4].try_into().unwrap()) as usize;
    let expected_len = 4 + limb_count * 8;
    if rest.len() < expected_len {
        return Err(Error::internal("missing BigInt limbs"));
    }
    let mut limbs = Vec::with_capacity(limb_count);
    for i in 0..limb_count {
        let offset = 4 + i * 8;
        limbs.push(u64::from_le_bytes(rest[offset..offset+8].try_into().unwrap()));
    }
    Ok(Value::BigInt(BigInt::from_limbs(&limbs)))
}
```

### NUMERIC_SPEC_VERSION

After BigInt implementation, bump the `NUMERIC_SPEC_VERSION` constant to 2. This is required by RFC-0110 governance before BigInt can be considered production-ready.

## Key Files to Modify

| File | Change |
|------|--------|
| `src/storage/mvcc/persistence.rs` | Add DFP (tag 13) and BigInt (tag 14) serialize/deserialize arms |
| `src/core/value.rs` | `from_typed` and `cast_to_type` for DFP/BigInt |
| (config) | `NUMERIC_SPEC_VERSION = 2` after BigInt |

## Design Reference

Full design rationale: `docs/plans/2026-03-28-rfc-0201-blob-implementation-missions.md`

---

**Mission Type:** Implementation
**Priority:** High
**Phase:** Phase 2f
