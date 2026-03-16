# RFC-0126 (Numeric/Math): Deterministic Serialization

## Status

**Version:** 1.0 (2026-03-16)
**Status:** Draft

> **Note:** This RFC defines canonical serialization formats for all protocol numeric data structures to ensure bit-identical encoding across implementations.

## Authors

- Primary Author: CipherOcto Team
- Contributing Reviewers: TBD

## Maintainers

- Lead Maintainer: TBD
- Technical Contact: TBD
- Repository: `rfcs/draft/numeric/0126-deterministic-serialization.md`

## Dependencies

### Required RFCs

| RFC | Relationship | Reason |
|-----|--------------|--------|
| RFC-0104 (DFP) | Required | Defines DfpEncoding format |
| RFC-0105 (DQA) | Required | Defines DqaEncoding format |
| RFC-0110 (BIGINT) | Required | Defines BigIntEncoding format |

### Optional RFCs

| RFC | Relationship | Reason |
|-----|--------------|--------|
| RFC-0111 (DECIMAL) | Optional | Future decimal encoding extension |

## Design Goals

1. **Determinism**: All numeric types must serialize to identical bytes across implementations
2. **No Ambiguity**: Each numeric type uses distinct encoding to prevent Merkle hash collisions
3. **Efficiency**: Fixed-size encodings where possible for fast parsing
4. **Extensibility**: Version byte allows future format changes without breaking compatibility
5. **Validation**: All deserialized data validated for canonical form

## Motivation

### Why Serialization Matters

Currently serialization is implicitly assumed. Without a standard:

- **Hash mismatches** between implementations (different byte orderings)
- **Proof verification failures** (inconsistent encoding)
- **Cross-language compatibility bugs** (endianness, padding, struct layout)

### The Merkle Hash Ambiguity Problem

If multiple numeric types use the same encoding format, a Merkle tree cannot distinguish between them:

```
Example: DQA(1.0) vs BIGINT(1)
If both encode to identical bytes, their Merkle hashes are identical.
This breaks consensus state verification.
```

**Solution**: Each numeric type uses a distinct encoding format.

## Summary

This RFC defines canonical serialization formats for all protocol data structures to ensure bit-identical encoding across implementations. It specifies:

- **DFP Encoding**: 24-byte fixed-size format
- **DQA Encoding**: 16-byte fixed-size format
- **BIGINT Encoding**: Variable-size format (8-520 bytes)
- **I128 Encoding**: 16-byte fixed-size format

All encodings use big-endian byte order for network compatibility, with explicit version bytes for future extensibility.

## Relationship to Other RFCs

| RFC | Relationship |
|-----|--------------|
| RFC-0104 (DFP) | Defines DfpEncoding |
| RFC-0105 (DQA) | Defines DqaEncoding |
| RFC-0110 (BIGINT) | Defines BigIntEncoding |
| RFC-0111 (DECIMAL) | Future: extends with DecimalEncoding |

### Numeric Tower Encoding Architecture

```
┌─────────────────────────────────────────────┐
│           Numeric Encoding Types            │
├─────────────────────────────────────────────┤
│ I128Encoding   → i128 (16 bytes, BE)       │
│ BigIntEncoding → Arbitrary Integer          │
│                 (variable, 8-520 bytes)     │
│ DqaEncoding    → Decimal (16 bytes, BE)    │
│ DfpEncoding    → Floating-Point (24 bytes) │
└─────────────────────────────────────────────┘
```

## Specification

### Design Principles

1. **Big-Endian Everywhere**: All multi-byte integers use big-endian byte order (network byte order)
2. **Version First**: First byte of every encoding is a version identifier
3. **Canonical Only**: Only canonical forms may be serialized (invalid inputs TRAP)
4. **No Padding Overlap**: Each encoding has distinct structure to prevent ambiguity
5. **Explicit Size**: Variable encodings include explicit length fields

### Encoding Overview

| Encoding | Type | Size | Byte Order | Version |
|----------|------|------|------------|---------|
| I128Encoding | Integer | 16 bytes | Big-Endian | First byte |
| BigIntEncoding | Arbitrary Integer | 8-520 bytes | LE limbs, BE header | Byte 0 |
| DqaEncoding | Decimal | 16 bytes | Big-Endian | Implicit (v1) |
| DfpEncoding | Floating-Point | 24 bytes | Big-Endian | Implicit (v1) |

### I128Encoding

For i128 interoperability with external systems.

```
┌─────────────────────────────────────────────────────────────┐
│ I128Encoding (16 bytes)                                   │
├─────────────────────────────────────────────────────────────┤
│ Bytes 0-15: i128 in two's complement, big-endian          │
└─────────────────────────────────────────────────────────────┘
```

**Rust definition:**
```rust
struct I128Encoding {
    value: i128,  // 16 bytes, big-endian
}
```

**Canonical form:** Standard i128 two's complement.

### BigIntEncoding

For arbitrary-precision integer serialization (RFC-0110).

```
┌─────────────────────────────────────────────────────────────┐
│ Byte 0: Version (0x01)                                    │
│ Byte 1: Sign (0x00 = positive, 0xFF = negative)           │
│ Bytes 2-3: Reserved (MUST be 0x0000)                       │
│ Byte 4: Number of limbs (u8, range 1-64)                    │
│ Bytes 5-7: Reserved (MUST be 0x000000)                     │
│ Bytes 8+: Limb array (little-endian within each limb)      │
└─────────────────────────────────────────────────────────────┘
```

**Total size:** 8 + (num_limbs × 8) bytes

**Maximum size:** 520 bytes (64 limbs × 8 bytes + 8 header bytes)

**Rust definition:**
```rust
pub struct BigIntEncoding {
    pub version: u8,           // 0x01
    pub sign: u8,              // 0x00 or 0xFF
    pub num_limbs: u8,        // 1-64
    pub limbs: Vec<u64>,       // little-endian
}
```

### DqaEncoding

For bounded decimal arithmetic (RFC-0105).

```
┌─────────────────────────────────────────────────────────────┐
│ Bytes 0-7:  value (i64, big-endian)                       │
│ Byte 8:     scale (u8)                                    │
│ Bytes 9-15: Reserved (MUST be zeros)                      │
└─────────────────────────────────────────────────────────────┘
```

**Rust definition:**
```rust
#[repr(C)]
pub struct DqaEncoding {
    pub value: i64,      // big-endian
    pub scale: u8,      // 0-18
    pub _reserved: [u8; 7],
}
```

**Canonical form:**
- Value must be canonical per RFC-0105 (trailing zeros removed)
- Scale must be 0-18
- Reserved bytes must be zero

### DfpEncoding

For deterministic floating-point (RFC-0104).

```
┌─────────────────────────────────────────────────────────────┐
│ Bytes 0-15:  mantissa (u128, big-endian)                  │
│ Bytes 16-19: exponent (i32, big-endian)                   │
│ Bytes 20-23: class_sign (u32, big-endian)                 │
│   - Bits 24-31: class (0=Normal, 1=Infinity, 2=NaN, 3=Zero)│
│   - Bits 16-23: sign (0=positive, 1=negative)             │
│   - Bits 0-15:  reserved                                  │
└─────────────────────────────────────────────────────────────┘
```

**Rust definition:**
```rust
#[repr(C, align(8))]
pub struct DfpEncoding {
    mantissa: u128,     // 16 bytes, big-endian
    exponent: i32,      // 4 bytes, big-endian
    class_sign: u32,    // 4 bytes, big-endian
}
```

## Serialization Algorithms

### BigInt Serialization

```
bigint_serialize(b: BigInt) -> BigIntEncoding

Precondition: b is in canonical form

1. If not canonical: TRAP (non-canonical input)
2. Return BigIntEncoding {
       version: 0x01,
       sign: if b.sign then 0xFF else 0x00,
       num_limbs: b.limbs.len() as u8,
       limbs: b.limbs.clone(),
   }
```

### BigInt Deserialization

```
bigint_deserialize(data: &[u8]) -> BigInt

1. If data.len < 8: TRAP (too short)
2. version = data[0]
   If version != 0x01: TRAP (unknown version)
3. sign_byte = data[1]
   If sign_byte == 0x00: sign = false
   else if sign_byte == 0xFF: sign = true
   else: TRAP (invalid sign)
4. If data[2] != 0x00 or data[3] != 0x00: TRAP (reserved)
5. num_limbs = data[4] as usize
   If num_limbs == 0 or num_limbs > 64: TRAP
6. If data[5] != 0x00 or data[6] != 0x00 or data[7] != 0x00: TRAP
7. expected_len = 8 + num_limbs * 8
   If data.len != expected_len: TRAP (length mismatch)
8. For i in 0..num_limbs:
     limbs[i] = u64::from_le_bytes(data[8 + i*8 .. 16 + i*8])
9. Construct b = BigInt { limbs, sign }
10. Validate canonical form:
    a. If num_limbs > 1 AND limbs[num_limbs-1] == 0: TRAP
    b. If num_limbs == 1 AND limbs[0] == 0 AND sign == true: TRAP
11. Return b
```

### DQA Serialization

```
dqa_serialize(d: Dqa) -> DqaEncoding

1. canonical = dqa_canonicalize(d)
2. Return DqaEncoding {
       value: canonical.value.to_be(),
       scale: canonical.scale,
       _reserved: [0; 7],
   }
```

### DQA Deserialization

```
dqa_deserialize(data: DqaEncoding) -> Dqa

1. If data.scale > 18: TRAP (invalid scale)
2. For each byte in data._reserved:
     If byte != 0: TRAP (reserved must be zero)
3. Return Dqa {
       value: i64::from_be(data.value),
       scale: data.scale,
   }
```

### DFP Serialization

```
dfp_serialize(d: Dfp) -> DfpEncoding

1. Return DfpEncoding::from_dfp(d)  // handles class/sign encoding
```

### DFP Deserialization

```
dfp_deserialize(data: [u8; 24]) -> Dfp

1. Return DfpEncoding::from_bytes(data).to_dfp()
```

## Serialization Invariants

### Cross-Type Ambiguity Prevention

| Type A | Type B | Encoding Difference |
|--------|--------|---------------------|
| DQA(1.0) | BIGINT(1) | DQA: 16 bytes, scale field; BIGINT: header + limbs |
| DFP(1.0) | BIGINT(1) | DFP: 24 bytes, class_sign; BIGINT: header + limbs |
| DQA(1) | I128(1) | DQA: 16 bytes, scale field; I128: 16 bytes raw |

**Each type's encoding is structurally distinct**, preventing Merkle hash collisions.

### Canonical Form Requirements

| Type | Canonical Form | Non-Canonical Behavior |
|------|---------------|----------------------|
| BIGINT | No leading zero limbs, no negative zero | TRAP on deserialize |
| DQA | Trailing zeros removed, scale minimized | TRAP on serialize |
| DFP | Per RFC-0104 canonical rules | Implementation-defined |
| I128 | Standard two's complement | Standard |

### Version Handling

- **Known versions**: 0x01 (current)
- **Unknown versions**: TRAP on deserialization
- **Version negotiation**: Not supported (always use latest)

## Error Handling

### Error Codes

| Error | Code | Condition |
|-------|------|-----------|
| SER_VERSION_UNKNOWN | 0xS001 | Unknown encoding version |
| SER_INVALID_SIGN | 0xS002 | Invalid sign byte |
| SER_INVALID_LENGTH | 0xS003 | Length mismatch for limbs |
| SER_NONCANONICAL | 0xS004 | Input not in canonical form |
| SER_RESERVED_NONZERO | 0xS005 | Reserved bytes not zero |
| SER_SCALE_INVALID | 0xS006 | Scale out of valid range |

### Error Semantics

All serialization errors are fatal (TRAP):
- Contract execution reverts
- Gas consumed up to failure point
- Error code logged for debugging

## Test Vectors

### BigInt Serialization

| Input | Expected Bytes (hex) |
|-------|---------------------|
| BigInt(0) | 01 00 00 00 01 00 00 00 00 00 00 00 00 00 00 00 |
| BigInt(1) | 01 00 00 00 01 00 00 00 01 00 00 00 00 00 00 00 |
| BigInt(-1) | 01 FF 00 00 01 00 00 00 01 00 00 00 00 00 00 00 |
| BigInt(2^64) | 01 00 00 00 02 00 00 00 00 00 00 00 00 00 00 00 01 00 00 00 00 00 00 00 |

### DQA Serialization

| Input | Expected Bytes (hex) |
|-------|---------------------|
| DQA(1.0, scale=0) | 00 00 00 00 00 00 00 01 00 00 00 00 00 00 00 00 |
| DQA(1.5, scale=1) | 00 00 00 00 00 00 00 0F 01 00 00 00 00 00 00 00 |
| DQA(-1.0, scale=0) | FF FF FF FF FF FF FF FF 00 00 00 00 00 00 00 00 |

### DFP Serialization

| Input | Expected Bytes (hex) |
|-------|---------------------|
| DFP(1.0) | 00 00 00 00 00 00 00 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 |
| DFP(-1.0) | 00 00 00 00 00 00 00 01 00 00 00 00 01 00 00 00 00 00 00 00 00 00 00 00 |

### I128 Serialization

| Input | Expected Bytes (hex) |
|-------|---------------------|
| i128::MAX | 00 FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF |
| i128::MIN | FF 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 |
| 1 | 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 01 |

## Performance Targets

| Operation | Target | Notes |
|-----------|--------|-------|
| BigInt serialize | O(n) | n = number of limbs |
| BigInt deserialize | O(n) | n = number of limbs |
| DQA serialize | O(1) | Fixed size |
| DQA deserialize | O(1) | Fixed size |
| DFP serialize | O(1) | Fixed size |
| DFP deserialize | O(1) | Fixed size |

## Security Considerations

### Threat Model

1. **Buffer Overflow**: Prevented by explicit length validation before memory allocation
2. **Canonical Form Violation**: TRAP on non-canonical input prevents state manipulation
3. **Version Rollback**: Unknown versions TRAP, preventing downgrade attacks
4. **Endianness Confusion**: Explicit big-endian everywhere eliminates ambiguity

### Attack Vectors

| Vector | Mitigation |
|--------|------------|
| Malformed length fields | Explicit bounds checking before allocation |
| Reserved byte tampering | TRAP if non-zero reserved bytes |
| Version downgrade | Unknown versions TRAP |
| Non-canonical forms | Canonical validation before/after serialization |

## Adversarial Review

### Review History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-03-16 | Initial draft from implementation |

### Known Issues

None yet.

## Alternatives Considered

### Option 1: Single Universal Numeric Encoding (Rejected)

**Approach**: Use one encoding for all numeric types

**Pros:**
- Simpler implementation
- Smaller code size

**Cons:**
- Merkle hash ambiguity between types
- Cannot distinguish DQA(1.0) from BIGINT(1)
- Loses type information

**Decision**: Each type has distinct encoding

### Option 2: Little-Endian Everywhere (Rejected)

**Approach**: Use little-endian for all encodings

**Pros:**
- Some architectures prefer LE

**Cons:**
- Network protocol convention is big-endian
- Inconsistent with existing standards

**Decision**: Big-endian everywhere (network byte order)

### Option 3: Self-Describing Length Prefix (Rejected)

**Approach**: Use length-prefixed envelopes for all types

**Pros:**
- Extensible

**Cons:**
- Additional overhead
- Not needed for fixed types

**Decision**: Header includes length for variable types only

## Version History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-03-16 | CipherOcto | Initial draft from implementation |

## Compatibility

### Backward Compatibility

- Version byte allows format evolution
- Old versions TRAP on deserialization
- Forward compatibility not guaranteed

### Interoperability

| Format | Systems | Notes |
|--------|---------|-------|
| I128Encoding | Rust, Go, Java | Standard big-endian i128 |
| BigIntEncoding | Custom | CipherOcto-specific |
| DqaEncoding | Custom | CipherOcto-specific |
| DfpEncoding | Custom | CipherOcto-specific |

## Future Work

1. **DecimalEncoding**: RFC-0111 DECIMAL type serialization
2. **Enum/Union Tags**: Type-safe wrapper for multi-type numeric values
3. **Compression**: Optional compressed encoding for large values
4. **ZK Commitments**: Integration with proof system for serialized values

## Related Use Cases

- UC-XXX: Cross-chain state verification (future)
- UC-XXX: Deterministic proof verification (future)

## References

- RFC-0104: Deterministic Floating-Point
- RFC-0105: Deterministic Quant Arithmetic
- RFC-0110: Deterministic BIGINT
- RFC-0111: Deterministic DECIMAL (planned)
