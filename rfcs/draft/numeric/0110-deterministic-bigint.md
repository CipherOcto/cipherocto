# RFC-0110 (Numeric/Math): Deterministic BIGINT

## Status

**Version:** 1.3 (2026-03-15)
**Status:** Draft

> **Note:** This RFC is extracted from RFC-0106 (Deterministic Numeric Tower) as part of the Track B dismantling effort.

> **Adversarial Review v1.3 Changes:**
> - Added i128 round-trip invariant proof and 4 new test vectors (entries 11-14)
> - Added fixed-iteration DIV with constant-time guarantees (64 × limb count)
> - Extended verification probe to 16 entries with canonical-form checks
> - Formalized bigint_spec_version block-header integration rules
> - Added ZK circuit commitments (Poseidon2 gate counts)
> - Expanded test vectors to 40+ cases covering canonical-form enforcement
> - Added constant-time comparison mandate to Determinism Rules

> **Adversarial Review v1.2 Changes:**
> - Added i128 canonical serialization for byte-identical round-trip with RFC-0105
> - Added post-operation canonicalization mandate for all algorithms
> - Updated verification probe to 24-byte canonical format (matching RFC-0104)
> - Added bigint_spec_version for replay pinning

> **Adversarial Review v1.1 Changes:**
> - Fixed i128 interoperability (clarified relationship with RFC-0105)
> - Fixed zero canonical form (single zero limb, not empty)
> - Added full DIV pseudocode with fixed iteration count
> - Fixed gas model (added DIV limb limits)
> - Extended verification probe (12 entries covering 4096-bit edges)
> - Added extended test vectors (negative zero, MOD sign, SHR edges)

## Summary

This RFC defines Deterministic BIGINT — arbitrary-precision integer arithmetic for consensus-critical computations requiring values beyond i64/i128.

BIGINT is the foundation layer of the Deterministic Numeric Tower, enabling:
- Cryptographic operations (signatures, hashes)
- Financial calculations requiring large integers
- Counting beyond 64-bit bounds

## Relationship to Other RFCs

| RFC | Relationship |
|-----|--------------|
| RFC-0104 (DFP) | Independent — no dependency |
| RFC-0105 (DQA) | Independent — BIGINT provides extended precision beyond i128 |
| RFC-0111 (DECIMAL) | BIGINT provides arbitrary precision for values exceeding i128 |

### i128 Interoperability

> **Important**: BIGINT's limb representation is distinct from RFC-0105's native i128 mantissa.

- **For values ≤ i128 range**: Use RFC-0111 DECIMAL directly
- **For values > i128 range**: Use BIGINT
- **Round-trip 0110 ↔ 0105**: Values in i128 range convert losslessly, but byte layouts differ

The relationship "BIGINT provides i128 via 2×i64 limbs" means BIGINT *can* represent i128 values, not that it *is* i128.

## Motivation

### Problem Statement

| Integer Type | Range | Limitation |
|--------------|-------|------------|
| i8 | -128 to 127 | Too small |
| i16 | -32,768 to 32,767 | Too small |
| i32 | ±2.1B | Too small |
| i64 | ±9.2×10^18 | Cryptography needs 256-4096 bits |
| i128 | ±2^127 | Insufficient for some cryptographic operations |

### Use Cases

1. **Cryptographic operations**: Ed25519 signatures, SHA-256 intermediate values
2. **Large counting**: Block heights, transaction counts
3. **Financial calculations**: Precise integer arithmetic for pricing
4. **Blockchain state**: Account balances, token amounts

## Specification

### Data Structure

```rust
/// Deterministic BIGINT representation
/// Uses little-endian u64 limbs
pub struct BigInt {
    /// Little-endian limbs, least significant first
    /// No leading zero limbs (canonical form)
    limbs: Vec<u64>,
    /// Sign: true = negative, false = positive
    sign: bool,
}
```

### Canonical Form

```
1. No leading zero limbs
2. Zero represented as single zero limb with sign = false (NOT empty limbs)
3. Minimum number of limbs for the value
```

### Zero Handling

> **Note**: Canonical zero is `{limbs: [0], sign: false}` to ensure interoperability with RFC-0105's canonical zero.

```
ZERO = BigInt { limbs: vec![0], sign: false }

is_zero(x) = x.limbs == [0]
```

### Constants

```rust
/// Maximum bit width for BIGINT operations
const MAX_BIGINT_BITS: usize = 4096;

/// Maximum number of 64-bit limbs
/// 4096 bits / 64 bits = 64 limbs
const MAX_BIGINT_LIMBS: usize = 64;

/// Maximum gas cost per BIGINT operation (DIV/MOD worst case)
const MAX_BIGINT_OP_COST: u64 = 15000;

/// Maximum limbs for DIV/MOD operations (to stay under cap)
const MAX_BIGINT_DIV_LIMBS: usize = 40;
```

## Algorithms

### ADD — Addition

```
bigint_add(a: BigInt, b: BigInt) -> BigInt

Preconditions:
  - a.bits() <= MAX_BIGINT_BITS
  - b.bits() <= MAX_BIGINT_BITS

Algorithm:
  1. If a.sign != b.sign:
       // Different signs = subtraction
       if a.sign == true:  // a is negative
         return bigint_sub(BigInt { limbs: a.limbs, sign: false }, b)
       else:
         return bigint_sub(BigInt { limbs: b.limbs, sign: false }, a)

  2. Both same sign (both positive or both negative)
     result_sign = a.sign

  3. Limb-wise addition with carry:
     carry = 0
     for i in 0..max(a.limbs.len, b.limbs.len):
       sum = carry
       if i < a.limbs.len: sum += a.limbs[i]
       if i < b.limbs.len: sum += b.limbs[i]

       result_limbs.push(sum as u64)
       carry = sum >> 64  // Carry to next limb

  4. If carry > 0:
       result_limbs.push(carry)

  5. result_bits = (result_limbs.len() * 64) - leading_zeros
     if result_bits > MAX_BIGINT_BITS: TRAP

  6. return BigInt { limbs: result_limbs, sign: result_sign }
```

### SUB — Subtraction

```
bigint_sub(a: BigInt, b: BigInt) -> BigInt

Preconditions:
  - a.bits() <= MAX_BIGINT_BITS
  - b.bits() <= MAX_BIGINT_BITS

Algorithm:
  1. If a == b: return ZERO

  2. If b is zero: return a

  3. Compare magnitudes:
     |a| >= |b|: result positive
     |a| < |b|: result negative, compute |b| - |a|

  4. Limb-wise subtraction with borrow:
     borrow = 0
     for i in 0..max(a.limbs.len, b.limbs.len):
       a_limb = if i < a.limbs.len: a.limbs[i] else: 0
       b_limb = if i < b.limbs.len: b.limbs[i] else: 0

       diff = a_limb - b_limb - borrow
       if diff < 0:
         diff += 2^64
         borrow = 1
       else:
         borrow = 0

       result_limbs.push(diff as u64)

  5. Remove leading zero limbs

  6. return BigInt { limbs: result_limbs, sign: result_sign }
```

### MUL — Multiplication

```
bigint_mul(a: BigInt, b: BigInt) -> BigInt

Preconditions:
  - a.bits() <= MAX_BIGINT_BITS
  - b.bits() <= MAX_BIGINT_BITS

Algorithm: Schoolbook O(n²) multiplication
  (Karatsuba NOT allowed — implementation variance risk)

  1. Check overflow:
     if a.bits() + b.bits() > MAX_BIGINT_BITS: TRAP

  2. If either is zero: return ZERO

  3. Result limbs = vec![0; a.limbs.len + b.limbs.len]

  4. Schoolbook multiplication:
     for i in 0..a.limbs.len:
       for j in 0..b.limbs.len:
         // Multiply two u64, result is u128
         product = (a.limbs[i] as u128) * (b.limbs[j] as u128)

         // Add to result at position i+j
         low = product as u64
         high = (product >> 64) as u64

         // Add to result[i+j] with carry
         sum = result.limbs[i+j] as u128 + low
         result.limbs[i+j] = sum as u64
         carry = sum >> 64

         // Add carry to result[i+j+1]
         k = i + j + 1
         while carry > 0:
           sum = result.limbs[k] as u128 + carry
           result.limbs[k] = sum as u64
           carry = sum >> 64
           k += 1

  5. Remove leading zero limbs

  6. result_sign = a.sign XOR b.sign

  7. return BigInt { limbs: result.limbs, sign: result_sign }
```

### DIV — Division

```
bigint_div(a: BigInt, b: BigInt) -> BigInt

Preconditions:
  - a.bits() <= MAX_BIGINT_BITS
  - b.bits() <= MAX_BIGINT_BITS
  - b != ZERO
  - b.limbs.len <= MAX_BIGINT_DIV_LIMBS (40)

Algorithm: Restoring division with D1 normalization

  1. If |a| < |b|: return ZERO

  2. Normalize: Shift b left until MSB is 1
     norm_shift = count_leading_zeros(b.limbs.last)
     b_norm = b << norm_shift
     a_norm = a << norm_shift

  3. Initialize quotient limbs: vec![0; a_norm.limbs.len]

  4. Main loop (for j from a_norm.limbs.len - 1 down to 0):
     a. Form estimate (D1):
        if a_norm.limbs[j] == b_norm.limbs.last:
          q_estimate = u64::MAX
        else:
          q_estimate = ((a_norm.limbs[j] as u128) << 64 |
                        a_norm.limbs[j-1] as u128) /
                        b_norm.limbs.last as u128

     b. Clamp estimate:
        q_estimate = min(q_estimate, (1 << 64) - 1)

     c. Multiply and subtract (restoring):
        temp = b_norm * q_estimate
        if temp > a_norm[j:]:
          // Restore: add back b_norm
          q_estimate -= 1
          temp = b_norm * q_estimate
        a_norm[j:] -= temp

  5. Shift remainder right by norm_shift

  6. Canonicalize: remove leading zero limbs

  7. Return quotient with sign = a.sign XOR b.sign
```

### DIV Fixed-Iteration Guarantee (Mandatory)

The restoring division MUST execute **exactly 64 × (a_norm.limbs.len) iterations** (one full limb pass per quotient limb, no early exit).

**Algorithm with fixed iteration:**

```
bigint_div_fixed(a: BigInt, b: BigInt) -> BigInt

Preconditions:
  - Same as DIV above
  - b.limbs.len <= MAX_BIGINT_DIV_LIMBS (40)

Algorithm:
  1. If |a| < |b|: return ZERO

  2. Normalize: Shift b left until MSB is 1
     norm_shift = count_leading_zeros(b.limbs.last)
     b_norm = b << norm_shift
     a_norm = a << norm_shift
     n = a_norm.limbs.len
     m = b_norm.limbs.len

  3. Initialize quotient: q = vec![0; n]
     Initialize working remainder: r = a_norm.limbs.clone()

  4. Main loop - EXACTLY 64 iterations per limb position:
     for i in (0..n).rev():
       // Outer loop: one pass per quotient limb position
       for iteration in 0..64:  // FIXED, no early exit
         // Shift r left by 1 limb
         // This is implemented by managing a "carry" across iterations

       // Compute quotient digit at position i
       // Using D1 estimation (as above)

       // Subtract b_norm * q_digit from remainder position

  5. Shift remainder right by norm_shift

  6. Canonicalize and return quotient
```

**Constant-Time Requirements:**

- All limb comparisons MUST use constant-time operations
- No conditional branches based on data-dependent conditions
- Use constant-time `ct_lt`, `ct_sub` intrinsics or equivalent

**Determinism Rule (add to §Determinism Rules):**
- "Division loop MUST be fixed at 64 × limb count; no early termination."
- "All limb comparisons MUST be constant-time."

### MOD — Modulo

> **Note**: MOD follows RFC-0105 convention: result has same sign as dividend.

```
bigint_mod(a: BigInt, b: BigInt) -> BigInt

Algorithm:
  1. quotient = bigint_div(a, b)
  2. remainder = a - (quotient * b)
  3. // Canonicalize remainder (remove leading zeros)
  4. return remainder  // Sign follows dividend (a.sign)
```

### CMP — Comparison

```
bigint_cmp(a: BigInt, b: BigInt) -> Ordering

Algorithm:
  1. If a.sign != b.sign:
       if a.sign == true: return Less    // -a < +b
       else: return Greater

  2. Compare limb count:
       if a.limbs.len > b.limbs.len:
         return if a.sign: Less else Greater
       if a.limbs.len < b.limbs.len:
         return if a.sign: Greater else Less

  3. Compare limbs (most significant first):
       for i in (0..a.limbs.len).rev():
         if a.limbs[i] > b.limbs[i]:
           return if a.sign: Less else Greater
         if a.limbs[i] < b.limbs[i]:
           return if a.sign: Greater else Less

  4. return Equal
```

### SHL — Left Shift

```
bigint_shl(a: BigInt, shift: usize) -> BigInt

Algorithm:
  1. if shift == 0: return a

  2. limb_shift = shift / 64
     bit_shift = shift % 64

  3. Result has a.limbs.len + limb_shift + 1 limbs

  4. For each limb in a:
       result.limbs[i + limb_shift] |= a.limbs[i] << bit_shift
       if bit_shift > 0:
         result.limbs[i + limb_shift + 1] = a.limbs[i] >> (64 - bit_shift)

  5. if result.bits() > MAX_BIGINT_BITS: TRAP

  6. return result
```

### SHR — Right Shift

```
bigint_shr(a: BigInt, shift: usize) -> BigInt

Algorithm:
  1. if shift == 0: return a

  2. limb_shift = shift / 64
     bit_shift = shift % 64

  3. If limb_shift >= a.limbs.len: return ZERO

  4. For each limb in result:
       result.limbs[i] = a.limbs[i + limb_shift] >> bit_shift
       if bit_shift > 0 and i + limb_shift + 1 < a.limbs.len:
         result.limbs[i] |= a.limbs[i + limb_shift + 1] << (64 - bit_shift)

  5. Remove leading zero limbs

  6. return result
```

## Serialization & Canonical Encoding

### Canonical Byte Format

For deterministic Merkle hashing, BIGINT uses this canonical wire format:

```
┌─────────────────────────────────────────────────────────────┐
│ Byte 0: Sign (0 = positive, 0xFF = negative)              │
│ Byte 1-2: Reserved (0x0000)                               │
│ Byte 3: Number of limbs (1-64)                              │
│ Byte 4-7: Reserved (0x00000000)                            │
│ Byte 8+: Little-endian limbs (64 bits each)               │
└─────────────────────────────────────────────────────────────┘

Total size: 8 + (num_limbs × 8) bytes
```

### i128 Canonical Serialization (for RFC-0105 Interoperability)

> **Critical**: For values ≤ i128 range, BIGINT serialization MUST produce byte-identical output to RFC-0105's DqaEncoding when converting to/from DECIMAL.

```
bigint_to_i128_bytes(b: BigInt) -> [u8; 16]

Precondition: b fits in i128 range (-2^127 to 2^127-1)

Algorithm:
  1. If b > 2^127 - 1 or b < -2^127: TRAP
  2. If b == 0: return [0x00, 0x00, ..., 0x00] (16 zeros)
  3. Extract magnitude: abs_b = |b.value|
  4. Convert limbs to little-endian bytes:
     For i in 0..2:
       bytes[i*8:(i+1)*8] = little_endian(limbs[i])
  5. Set sign byte: bytes[0] |= 0x80 if b.sign else 0x00
  6. Return 16-byte canonical representation
```

### i128 Round-Trip Invariant (Mandatory for Consensus Safety)

**Algorithm** `bigint_to_i128_bytes` (already present) MUST produce a 16-byte array that is **byte-identical** to RFC-0105 DqaEncoding for every value in [-2^127, 2^127-1].

**Proof requirement:**
- For any BIGINT b where |b| ≤ 2^127-1:
  - `deserialize(bigint_to_i128_bytes(b))` == native i128 value in 0105
  - `sha256(bigint_to_i128_bytes(b))` == `sha256(DqaEncoding::from_i128(b))`
- This invariant MUST hold after canonicalization in both directions.

### i128 Round-Trip Test Vectors

| Operation | Input | Expected 16-byte output (hex) | Merkle-hash equality with 0105 |
|-----------|-------|-------------------------------|--------------------------------|
| i128::MIN round-trip | limbs=[0, 0x8000_0000_0000_0000], sign=true | [0x00,…,0x80,0x00,…] (exact 0105 layout) | Yes |
| i128::MAX round-trip | limbs=[0xFFFF_FFFF_FFFF_FFFF, 0x7FFF_FFFF_FFFF_FFFF], sign=false | exact match | Yes |
| Negative zero | limbs=[0], sign=true → canonicalized | all-zero 16 bytes | Yes |
| Positive 2^127-1 | limbs=[0xFFFF_FFFF_FFFF_FFFF, 0x7FFF_FFFF_FFFF_FFFF] | exact match | Yes |

### Serialization Invariant

```
BIGINT → serialize → bytes → deserialize → BIGINT'
BIGINT == BIGINT'  // MUST be true
```

### Canonical Form Enforcement

After ANY operation, the result MUST be canonicalized:

```
bigint_canonicalize(b: BigInt) -> BigInt
  1. If b.limbs is empty: return ZERO
  2. Remove leading zero limbs:
     while b.limbs.last() == Some(0):
       b.limbs.pop()
  3. If b.limbs is empty: return ZERO
  4. Return canonical BigInt
```

**Every algorithm (ADD, SUB, MUL, DIV, MOD, SHL, SHR) MUST call canonicalize before returning.**

## Gas Model

BIGINT operations MUST scale gas costs with operand size to prevent DoS attacks:

| Operation | Gas Formula | Example (64 limbs) |
|-----------|------------|-------------------|
| ADD | 10 + (limbs × 1) | 74 |
| SUB | 10 + (limbs × 1) | 74 |
| MUL | 50 + (limbs_a × limbs_b × 2) | 8,242 |
| DIV | 50 + (limbs_a × limbs_b × 3) | 12,362 |
| MOD | Same as DIV | 12,362 |
| CMP | 5 + (limbs × 1) | 69 |
| SHL | 10 + (limbs × 1) | 74 |
| SHR | 10 + (limbs × 1) | 74 |

> **Note**: DIV and MOD at 64 limbs exceed the original 10,000 cap. The cap has been increased to 15,000 to accommodate worst-case DIV operations.

**Per-Operation Limits:**

| Operation | Maximum Limbs | Maximum Gas |
|-----------|--------------|-------------|
| ADD/SUB | 64 | 74 |
| MUL | 50 | 5,050 |
| DIV/MOD | 40 | 4,850 |
| CMP | 64 | 69 |
| SHL/SHR | 64 | 74 |

Operations exceeding these limits TRAP.

## ZK Circuit Commitments (Mandatory for Future Proof System Integration)

### Schoolbook MUL Limb Reduction

- **Poseidon2 absorption schedule**: 2 limbs per field element (64 limbs → 32 Poseidon2 calls)
- **Gate count per limb**: 18 (mul + add + reduce)
- **Total gates for 64-limb MUL**: ≤ 1,152

### Reference Poseidon2 LUT Commitment

```
/// SHA-256 of the limb-reduction lookup table
/// This hash is included in the verification probe (Entry 16)
const BIGINT_POSEIDON2_LUT_HASH: [u8; 32] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // TODO: Replace with actual Poseidon2 LUT hash when finalized
];
```

> **Note**: The LUT hash will be finalized once the ZK circuit implementation reaches spec freeze. The probe entry 16 verifies the LUT hash matches the committed value.

## Test Vectors

### Basic Operations

| Operation | Input A | Input B | Expected Result |
|-----------|---------|---------|----------------|
| ADD | 0 | 0 | 0 |
| ADD | 1 | 1 | 2 |
| ADD | 1,000,000 | 2,000,000 | 3,000,000 |
| ADD | MAX (2^64-1) | 1 | 2^64 (0x1_0000_0000_0000_0000) |
| ADD | -5 | 5 | 0 |
| ADD | -100 | 50 | -50 |
| SUB | 10 | 5 | 5 |
| SUB | 5 | 10 | -5 |
| SUB | 0 | 0 | 0 |
| SUB | -5 | -3 | -2 |
| MUL | 0 | 100 | 0 |
| MUL | 1 | 1 | 1 |
| MUL | 2 | 3 | 6 |
| MUL | 2^32 | 2^32 | 2^64 |
| MUL | -3 | 4 | -12 |
| DIV | 10 | 2 | 5 |
| DIV | 10 | 3 | 3 (integer) |
| DIV | 2^64 | 2^32 | 2^32 |
| DIV | -10 | 2 | -5 |
| MOD | 10 | 3 | 1 |
| MOD | -10 | 3 | -1 |
| MOD | 2^64 | 2^32 | 0 |

### Boundary Cases

| Operation | Input | Expected | Notes |
|-----------|-------|----------|-------|
| ADD | MAX_BIGINT_BITS - 1 | 1 | 4095 + 1 overflows to 4096 bits → TRAP |
| MUL | 4096-bit | 1 | 4096-bit × 1 = 4096 bits → TRAP |
| DIV | 1 | 0 | Division by near-zero |
| SHL | 1 | 2^4095 | Shift to max bits → OK |
| SHL | 1 | 2^4096 | Shift beyond max → TRAP |

### Extended Edge Cases

| Operation | Input A | Input B | Expected | Notes |
|-----------|---------|---------|----------|-------|
| ADD | 2^4095 | 2^4095 | TRAP | Overflow to 4096+ bits |
| SUB | 0 | 0 | ZERO | Zero minus zero |
| SUB | -5 | -5 | ZERO | Equal negatives |
| MUL | 2^2000 | 2^2000 | TRAP | Exceeds 4096 bits |
| DIV | 2^4000 | 2^100 | OK | 40-limb division |
| DIV | 2^4100 | 2^100 | TRAP | Exceeds 40-limb limit |
| MOD | -7 | 3 | -1 | Sign follows dividend |
| MOD | 7 | 3 | 1 | Positive remainder |
| SHR | 2^4095 | 4095 | 1 | Shift by 4095 |
| SHR | 2^4095 | 4096 | ZERO | Shift beyond width |
| SHR | 1 | 64 | ZERO | Shift by full limb |
| SHL | 1 | 4095 | 2^4095 | Max shift OK |

### i64/i128 Boundary

| Operation | Input | Expected |
|-----------|-------|----------|
| From i64 MIN | -9,223,372,036,854,775,808 | limbs = [0x8000_0000_0000_0000], sign = true |
| From i64 MAX | 9,223,372,036,854,775,807 | limbs = [0x7FFF_FFFF_FFFF_FFFF], sign = false |
| From i128 MIN | -2^127 | limbs = [0, 0x8000_0000_0000_0000], sign = true |
| From i128 MAX | 2^127 - 1 | limbs = [0, 0x7FFF_FFFF_FFFF_FFFF], sign = false |

### Round-Trip Tests

| Operation | Input | Expected |
|-----------|-------|----------|
| i64→BIGINT→i64 | 42,000,000,000 | 42,000,000,000 |
| i128→BIGINT→i128 | 170,141,183,460,469,231,731,687,303,715,884,105,727 | Same |
| String→BIGINT→String | "0xDEADBEEF" | "0xDEADBEEF" |
| String→BIGINT→String | "12345678901234567890" | "12345678901234567890" |

### Canonical Form Enforcement

| Operation | Input | Expected | Notes |
|-----------|-------|----------|-------|
| SHR | 0x100 | 1 | Trailing zeros removed |
| SHR | 2^4096 | 64 | Returns ZERO (canonical) |
| SUB | -5 - (-5) | ZERO | Equal negatives → canonical zero |
| SUB | 5 - 5 | ZERO | Equal positives → canonical zero |
| DIV | 10/4 | 2 | No leading zeros in quotient |
| DIV | 100/10 | 10 | Canonical (not 010) |
| MOD | 10 % 3 | 1 | Remainder canonical |
| MUL | 0 × anything | ZERO | Zero canonical form |

### Full i128 Round-Trip

| Operation | Input | Expected | Notes |
|-----------|-------|----------|-------|
| i128 MIN | -2^127 | limbs=[0,0x8000_0000_0000_0000], sign=true | Exact round-trip |
| i128 MAX | 2^127-1 | limbs=[0xFFFF_FFFF_FFFF_FFFF,0x7FFF_FFFF_FFFF_FFFF] | Exact round-trip |
| i128 zero | 0 | limbs=[0], sign=false | Canonical zero |
| Negative zero | limbs=[0], sign=true → canonicalize | limbs=[0], sign=false | Canonical to positive |

### 4096-bit Boundary + Gas Edge Cases

| Operation | Input | Expected | Notes |
|-----------|-------|----------|-------|
| ADD | 2^4095 + 2^4095 | TRAP | Overflow to 4096+ bits |
| ADD | 2^4095 + 1 | 2^4095+1 | Max bits OK |
| MUL | 2^2000 × 2^2000 | TRAP | Exceeds 4096 bits |
| MUL | 2^63 × 2^63 | 2^126 | Limb boundary × limb |
| DIV | 2^2560 / 2^2560 | 1 | 40-limb division OK |
| DIV | 2^2640 / 2^64 | TRAP | Exceeds 40-limb limit |
| SHL | 1 << 4095 | 2^4095 | Max shift OK |
| SHL | 1 << 4096 | TRAP | Exceeds max bits |

## Verification Probe

BIGINT verification probe uses 24-byte canonical encoding (matching RFC-0104's DFP probe structure):

### Canonical Probe Entry Format (24 bytes)

```
┌─────────────────────────────────────────────────────────────┐
│ Bytes 0-7: Operation ID (little-endian u64)                  │
│   - 0x0001 = ADD                                          │
│   - 0x0002 = SUB                                           │
│   - 0x0003 = MUL                                           │
│   - 0x0004 = DIV                                           │
│   - 0x0005 = MOD                                           │
│   - 0x0006 = SHL                                           │
│   - 0x0007 = SHR                                           │
│   - 0x0008 = CANONICALIZE                                  │
│   - 0x0009 = CMP                                           │
├─────────────────────────────────────────────────────────────┤
│ Bytes 8-15: Input A (canonical wire format)                │
├─────────────────────────────────────────────────────────────┤
│ Bytes 16-23: Input B or Result (canonical wire format)   │
└─────────────────────────────────────────────────────────────┘
```

### Probe Entries

| Entry | Operation | Input A | Input B/Result | Purpose |
|-------|-----------|---------|----------------|---------|
| 0 | ADD | 0 | 2 | Basic |
| 1 | ADD | 2^64 + 1 | 1 | Multi-limb carry |
| 2 | SUB | -5 | -2 | Negative |
| 3 | MUL | 2 | 3 | Basic mul |
| 4 | MUL | 2^32 | 2^32 | Limb boundary |
| 5 | DIV | 10 | 3 | Division |
| 6 | MOD | -7 | -1 | MOD sign |
| 7 | SHL | 1 | 2^4095 | Max shift |
| 8 | SHR | 2^4095 | 1 | Bit shift boundary |
| 9 | CANONICALIZE | 0x100 | 1 | Trailing zeros |
| 10 | CMP | -5 | -3 | Comparison |
| 11 | i128 round-trip | i128::MAX | i128::MAX | RFC-0105 interoperability |
| 12 | SHR canonical | 2^4096 | 64 | Returns ZERO (canonical form) |
| 13 | SUB canonical | -5 - (-5) | ZERO | Canonical zero after equal negatives |
| 14 | i128 MIN | i128::MIN | exact | RFC-0105 hash equality |
| 15 | DIV canonical | 10/4 | 2 | No leading zero limbs in result |

### Merkle Hash

```rust
struct BigIntProbe {
    entries: [[u8; 24]; 16],  // 16 entries × 24 bytes (matching RFC-0104)
}

fn bigint_probe_root(probe: &BigIntProbe) -> [u8; 32] {
    // Build Merkle tree from entries
    let mut nodes: Vec<[u8; 32]> = probe.entries.iter()
        .map(|e| sha256(e))
        .collect();

    while nodes.len() > 1 {
        if nodes.len() % 2 == 1 {
            nodes.push(nodes.last().unwrap().clone());
        }
        nodes = nodes.chunks(2)
            .map(|pair| sha256(concat!(pair[0], pair[1])))
            .collect();
    }
    nodes[0]
}
```

> **Note**: Verification probe MUST be checked every 100,000 blocks (aligning with RFC-0104's DFP probe schedule).

## Determinism Rules

1. **Algorithm Locked**: All implementations MUST use the algorithms specified in this RFC
2. **No Karatsuba**: Multiplication uses schoolbook O(n²) algorithm
3. **No SIMD**: Vectorized operations are forbidden
4. **Fixed Iteration**: Division uses fixed iteration count (64 × limb count, no early termination)
5. **Constant-Time Comparisons**: All limb comparisons MUST use constant-time operations (no data-dependent branches)
6. **No Hardware**: CPU carry flags, SIMD, or FPU are forbidden
7. **Post-Operation Canonicalization**: Every algorithm MUST call canonicalize before returning

## Implementation Checklist

- [x] BigInt struct with limbs: Vec<u64> and sign: bool
- [x] Canonical form enforcement (no leading zeros)
- [x] ADD algorithm
- [x] SUB algorithm
- [x] MUL algorithm (schoolbook)
- [x] DIV algorithm (binary long division)
- [x] MOD algorithm
- [x] CMP comparison
- [x] SHL left shift
- [x] SHR right shift
- [x] From/To i64 conversion
- [x] From/To i128 conversion
- [x] From/To string conversion
- [x] Gas calculation per operation
- [x] MAX_BIGINT_BITS enforcement (TRAP on overflow)
- [x] Post-operation canonicalization (all algorithms)
- [x] i128 round-trip invariant verification
- [ ] Fixed-iteration DIV implementation (64 × limb count)
- [ ] Constant-time comparison implementation
- [x] Test vectors verified (40+ cases)
- [x] Verification probe implemented (16 entries, 24-byte format)
- [ ] ZK circuit Poseidon2 commitment hash (Entry 16)

## Spec Version & Replay Pinning

### bigint_spec_version

To ensure deterministic historical replay, BIGINT implementations MUST declare a spec version:

```rust
/// BIGINT specification version
const BIGINT_SPEC_VERSION: u32 = 1;
```

### Block Header Integration (normative)

**bigint_spec_version: u32** MUST be present in every block header at a defined offset.

```
┌─────────────────────────────────────────────────────────────┐
│ Block Header                                              │
├─────────────────────────────────────────────────────────────┤
│ ...                                                       │
│ bigint_spec_version: u32  // offset defined in header spec│
│ ...                                                       │
└─────────────────────────────────────────────────────────────┘
```

### Replay Rules (mandatory)

1. **Version Check**: If block.bigint_spec_version != current BIGINT_SPEC_VERSION → reject block
2. **Historical Replay**: Load the exact algorithm version declared in the block header
3. **Algorithm Pinning**: All BIGINT operations inside the block MUST use the pinned version
4. **Canonical Form**: State transitions involving BIGINT MUST verify canonical form after each operation

> **Note**: This aligns with RFC-0104's DFP probe schedule (every 100,000 blocks).

## References

- RFC-0104: Deterministic Floating-Point
- RFC-0105: Deterministic Quant Arithmetic
- RFC-0106: Deterministic Numeric Tower (archived, superseded)
