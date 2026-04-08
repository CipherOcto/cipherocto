# Mission: BigInt Conversions & Serialization

## Status
Archived

## RFC
RFC-0110 (Numeric): Deterministic BIGINT

## Summary
Implement BigInt conversions (i64, i128, string) and canonical wire serialization format. This mission enables interoperability with Rust primitives and persistent storage.

## Phase 1: Primitive Conversions

### Acceptance Criteria
- [x] From<i64> trait implementation
- [x] From<i128> trait implementation
- [x] TryFrom<u64> trait implementation
- [x] TryFrom<u128> trait implementation
- [x] To<i64> trait (TRAP on overflow)
- [x] To<i128> trait (TRAP on overflow)
- [x] To<u64> trait (TRAP on overflow)
- [x] To<u128> trait (TRAP on overflow)

### i64 Conversion
```rust
impl From<i64> for BigInt {
    fn from(n: i64) -> BigInt {
        if n == 0 { return ZERO; }
        let sign = n < 0;
        let mag = n.unsigned_abs() as u64;
        canonicalize(BigInt {
            limbs: vec![mag],
            sign,
        })
    }
}

impl TryFrom<BigInt> for i64 {
    type Error = (); // TRAP on overflow

    fn try_from(b: BigInt) -> Result<i64, Self::Error> {
        if b.limbs.len() > 1 { return Err(()); } // Overflow
        let mag = b.limbs[0];
        if b.sign {
            if mag > (i64::MIN.unsigned_abs() as u64) { return Err(()); }
            Ok(-(mag as i64))
        } else {
            if mag > i64::MAX as u64 { return Err(()); }
            Ok(mag as i64)
        }
    }
}
```

### i128 Conversion
```rust
impl From<i128> for BigInt {
    fn from(n: i128) -> BigInt {
        if n == 0 { return ZERO; }
        let sign = n < 0;
        let mag = n.unsigned_abs() as u128;
        let lo = mag as u64;
        let hi = (mag >> 64) as u64;
        let limbs = if hi == 0 {
            vec![lo]
        } else {
            vec![lo, hi]
        };
        canonicalize(BigInt { limbs, sign })
    }
}

impl TryFrom<BigInt> for i128 {
    type Error = (); // TRAP on overflow

    fn try_from(b: &BigInt) -> Result<i128, Self::Error> {
        if b.limbs.len() > 2 { return Err(()); }
        let lo = b.limbs[0];
        let hi = b.limbs.get(1).copied().unwrap_or(0);
        let mag = ((hi as u128) << 64) | (lo as u128);
        if b.sign {
            if mag > (i128::MIN.unsigned_abs() as u128) { return Err(()); }
            Ok(-(mag as i128))
        } else {
            if mag > i128::MAX as u128 { return Err(()); }
            Ok(mag as i128)
        }
    }
}
```

## Phase 2: String Conversions

### Acceptance Criteria
- [x] FromStr trait implementation (parsing)
- [x] Display trait implementation (formatting)
- [x] Support decimal string representation
- [x] Support hex string prefix (0x)
- [x] Error handling for invalid input

### String Format
```
Decimal: "12345678901234567890"
Hex:     "0x1a2b3c4d5e6f"
Negative: "-9876543210"
```

## Phase 3: Serialization (Wire Format)

### Acceptance Criteria
- [x] BigIntEncoding: canonical 16-byte wire format
- [x] Serialization: struct → bytes
- [x] Deserialization: bytes → struct with canonical form verification
- [x] Version byte: 0x01 for v1

### Wire Format (RFC-0110 §Canonical Byte Format)
```
┌─────────────────────────────────────────────────────────────┐
│ Byte 0: Version (0x01)                                     │
│ Byte 1: Sign (0 = positive, 0xFF = negative)               │
│ Bytes 2-3: Reserved (MUST be 0x0000)                       │
│ Byte 4: Number of limbs (u8, range 1-64)                   │
│ Bytes 5-7: Reserved (MUST be 0x00)                         │
│ Byte 8+: Limb array (little-endian u64 × num_limbs)        │
└─────────────────────────────────────────────────────────────┘
```

### Serialization Algorithm
```
bigint_serialize(b: BigInt) -> Vec<u8>

1. Precondition: b is canonical
2. version = 0x01
3. sign = 0xFF if b.sign else 0x00
4. num_limbs = b.limbs.len() as u8
5. Encode header: [version, sign, 0x00, 0x00, num_limbs, 0x00, 0x00, 0x00]
6. Append little-endian limbs

Total: 8 bytes header + 8*num_limbs bytes
```

### Deserialization Algorithm
```
bigint_deserialize(data: &[u8]) -> Result<BigInt, Error>

1. If data.len() < 8: return Err(InvalidEncoding)
2. version = data[0]; if version != 0x01: return Err(UnsupportedVersion)
3. sign = data[1]; if sign != 0x00 && sign != 0xFF: return Err(InvalidSign)
4. If bytes 2-3 != 0x00: return Err(NonCanonical)
5. num_limbs = data[4]; if num_limbs == 0 || num_limbs > 64: return Err(InvalidLimbs)
6. If bytes 5-7 != 0x00: return Err(NonCanonical)
7. If data.len() != 8 + 8*num_limbs: return Err(InvalidLength)

8. limbs = parse little-endian u64 from data[8..]
9. b = BigInt { limbs, sign: sign == 0xFF }

10. Verify canonical form:
    a. If limbs.len() > 1 and limbs[last] == 0: return Err(NonCanonical)
    b. If limbs == [0] and sign == 0xFF: return Err(NonCanonicalNegativeZero)

11. return Ok(b)
```

## Phase 4: i128 Round-Trip Conversion

### Acceptance Criteria
- [x] bigint_to_i128_bytes: BigInt → 16-byte two's complement BE
- [x] i128_roundtrip tests for all i128 values

### bigint_to_i128_bytes Algorithm (RFC-0110)
```
bigint_to_i128_bytes(b: BigInt) -> [u8; 16]

Precondition: b fits in i128 range (-2^127 to 2^127-1)

1. If b > 2^127 - 1 or b < -2^127: TRAP
2. If b == 0: return [0x00, 0x00, ..., 0x00] (16 zeros)
3. Reconstruct magnitude as u128:
   magnitude: u128 = b.limbs[0] as u128;
   if b.limbs.len >= 2 {
     magnitude |= (b.limbs[1] as u128) << 64;
   }
4. let mut bytes = [0u8; 16];
5. let val: u128 = if b.sign == false {
     magnitude
   } else {
     (!magnitude).wrapping_add(1)  // two's complement
   };
6. for i in 0..16 {
     bytes[i] = ((val >> (120 - i*8)) & 0xFF) as u8;
   }
7. Return bytes
```

### i128 Round-Trip Test Vectors (RFC-0110 §i128 Round-Trip Test Vectors)
| Entry | Input | Expected |
|-------|-------|----------|
| 42 | 2^127-1 (i128::MAX) | round-trip |
| 43 | -2^127 (i128::MIN) | round-trip |
| 44 | 0 | round-trip |
| 45 | 1 | round-trip |
| 46 | -1 | round-trip |

## Implementation Location
- **File**: `determin/src/bigint.rs` (extends core algorithms)
- **Module**: `determin/src/serialize.rs` (optional separate module)

## Prerequisites
- Mission 0110-bigint-core-algorithms (Phase 1-4 complete)

## Testing Requirements
- Unit tests for all conversion functions
- Round-trip tests: i64 → BigInt → i64
- Round-trip tests: i128 → BigInt → i128
- Serialization round-trip: serialize → deserialize → identical
- Edge cases: i64::MIN, i64::MAX, i128::MIN, i128::MAX, zero, negative zero

## Reference
- RFC-0110: Deterministic BIGINT (§Wire Format)
- RFC-0110: Deterministic BIGINT (§BIGINT to i128 conversion)
- RFC-0110: Deterministic BIGINT (§i128 Round-Trip Test Vectors)
- RFC-0110: Deterministic BIGINT (§Deserialization Algorithm)

## Complexity
Medium — Straightforward conversions with careful overflow handling
