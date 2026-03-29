# Mission: RFC-0201 Phase 2f — DFP Dispatcher Integration

## Status

Open

## RFC

- RFC-0201 (Storage): Binary BLOB Type for Deterministic Hash Storage — Phase 2f
- RFC-0104 (Numeric): Deterministic Floating Point — Accepted

## Dependencies

- `octo-determin` crate in stoolap (provides `Dfp`, `DfpEncoding`)
- Independent of BigInt work (BigInt is covered by RFC-0202)

## Context

**DFP in stoolap is already Extension-based:**
- `Value::dfp(Dfp)` creates `Value::Extension(CompactArc<[u8]>)` with `DataType::DeterministicFloat` tag byte
- The 24-byte DFP encoding is precomputed via `DfpEncoding::from_dfp(&dfp).to_bytes()`
- DFP already serializes via the Extension path (tag 6 in current code)

**What's missing:**
- Wire tag 13 is reserved for DFP in RFC-0201, but currently DFP uses the generic Extension path
- Phase 2f-A adds explicit `serialize_value` and `deserialize_value` arms for wire tag 13
- This makes DFP first-class in the wire protocol (not mixed with generic Extension)

**Important:** DFP does NOT need a dedicated `Value::Dfp(Dfp)` variant — the Extension storage is correct. Phase 2f-A is purely about the wire protocol dispatch (tag 13).

## Acceptance Criteria

- [ ] `serialize_value` has arm for `Value::Dfp` via Extension (wire tag 13)
- [ ] `deserialize_value` handles wire tag 13, reconstructing DFP from 24-byte encoding
- [ ] DFP round-trip: `Value::dfp(dfp)` → serialize → deserialize → same DFP value
- [ ] `cargo test` passes including DFP serialization tests
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes

## Technical Details

### Current State

In stoolap's `persistence.rs`, DFP is serialized via the generic Extension arm (tag 6):
```rust
6 => {
    // Json/Extension — stored as tag + payload
    let tag = rest[0];
    ...
}
```

### Target State: Wire Tag 13 for DFP

Add explicit arm in `serialize_value`:
```rust
// DFP (Deterministic Floating Point) — RFC-0104 24-byte canonical format
// Stored as Extension in Value, but uses explicit wire tag 13
Value::Extension(bytes) if bytes.first() == Some(&DataType::DeterministicFloat as u8) => {
    buf.push(13);  // wire tag 13 for DFP
    // The encoding bytes are already stored after the tag byte in the Extension
    buf.extend_from_slice(&bytes[1..]);
}
```

Or alternatively, check if this should be a separate `Value::Dfp(Dfp)` variant. The decision:
- If DFP should be first-class: Add `Value::Dfp(Dfp)` variant and match directly
- If Extension storage is preferred: Use the tag-byte check approach above

**Note:** Per RFC-0201 spec, DFP uses wire tag 13 explicitly (not the generic Extension tag 6).

### Deserialization for Tag 13

```rust
13 => {
    // DFP — 24 bytes per RFC-0104
    if rest.len() < 24 {
        return Err(Error::internal("missing DFP data"));
    }
    let encoding_bytes: [u8; 24] = rest[..24].try_into().unwrap();
    let dfp = DfpEncoding::from_bytes(encoding_bytes).to_dfp();
    Ok(Value::dfp(dfp))  // Stores as Extension
}
```

## Key Files to Modify

| File | Change |
|------|--------|
| `src/storage/mvcc/persistence.rs` | Add serialize/deserialize arms for wire tag 13 (DFP) |

## Design Reference

- RFC-0104 DFP format: `rfcs/accepted/numeric/0104-deterministic-floating-point.md`
- RFC-0201 dispatcher: `rfcs/accepted/storage/0201-binary-blob-type-support.md`

---

**Mission Type:** Implementation
**Priority:** High
**Phase:** Phase 2f-A
