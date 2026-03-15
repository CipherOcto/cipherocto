# RFC-0110 (Numeric/Math): Deterministic BIGINT

## Status

**Version:** 2.3 (2026-03-15)
**Status:** Accepted

> **Note:** This RFC is extracted from RFC-0106 (Deterministic Numeric Tower) as part of the Track B dismantling effort.

> **Adversarial Review v2.3 Changes (Critical Bug Fixes):**
> - FIXED: DIV algorithm unified (removed duplicate algorithm contradiction)
> - FIXED: DIV index underflow bug (j=0 case accessing limbs[-1])
> - FIXED: Retracted false i128/DqaEncoding byte-identity claim
> - FIXED: bigint_to_i128_bytes algorithm consistency
> - REMOVED: Undefined POW/AND/OR/XOR/NOT probe entries (57-63)
> - FIXED: SUB borrow arithmetic with correct borrow propagation
> - REPLACED: Poseidon2 LUT hash with explicit placeholder marker

> **Adversarial Review v2.2 Changes (Final Production-Grade):**
> - Added deterministic canonicalization algorithm (normative step-by-step)
> - Explicitly mandated 128-bit intermediate arithmetic with emulation rules
> - Specified canonical schoolbook multiplication algorithm
> - Bound division to bitlen(a) iteration count
> - Added serialization version byte
> - Proved gas upper bounds
> - Removed constant-time requirement (clarified optional)
> - Fully specified shift operations with carry behavior
> - Added determinism guarantee section
> - Expanded verification probe to 64 entries**
> - Defined explicit canonicalization algorithm with negative-zero elimination
> - Mandated 128-bit intermediate arithmetic for limb overflow
> - Picked single division algorithm (bit-level restoring division)
> - Removed MAX_BIGINT_DIV_LIMBS conflict
> - Defined shift operations explicitly
> - Removed constant-time requirement (consensus determinism ≠ constant-time)
> - Added canonical serialization with limb count enforcement
> - Proved worst-case gas paths
> - Clarified cryptography use-case (not for crypto primitives)
> - Added numeric tower diagram with conversion rules
> - Added bit_length() function definition
> - Expanded verification probe to 32 entries

> **Adversarial Review v1.4 Changes (Full Consensus Readiness):**
> - Complete i128 round-trip proof with formal requirements + 8 additional vectors (entries 11-18)
> - Formalized DIV algorithm with verbatim limb-by-limb pseudocode + constant-time primitives
> - Finalized ZK LUT hash with actual SHA-256 placeholder + probe Entry 16 verification
> - Gas-model proof paragraph + per-block BIGINT budget (50,000)
> - Extended probe to 20 entries + differential fuzzing mandate
> - Constant-time enforcement guidance with intrinsics reference + 4 timing vectors

> **Adversarial Review v1.3 Changes:**
> - Added i128 round-trip invariant proof and 4 new test vectors (entries 11-14)
> - Added fixed-iteration DIV with constant-time guarantees (64 × limb count)
> - Extended verification probe to 16 entries with canonical-form checks
> - Formalized numeric_spec_version block-header integration rules
> - Added ZK circuit commitments (Poseidon2 gate counts)
> - Expanded test vectors to 40+ cases covering canonical-form enforcement
> - Added constant-time comparison mandate to Determinism Rules

> **Adversarial Review v1.2 Changes:**
> - Added i128 canonical serialization for byte-identical round-trip with RFC-0105
> - Added post-operation canonicalization mandate for all algorithms
> - Updated verification probe to 24-byte canonical format (matching RFC-0104)
> - Added numeric_spec_version for replay pinning

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

### Numeric Tower Architecture

```
INTEGER DOMAIN
i64 → i128 → BigInt (RFC-0110)

DECIMAL DOMAIN
DQA (RFC-0105)

FLOAT DOMAIN
DFP (RFC-0104)
```

**BIGINT interoperates with:**
- **i64** — direct conversion
- **i128** — direct conversion via I128Encoding

**DQA uses BIGINT internally** when intermediate precision exceeds i128.

> **Note:** BigInt encoding is separate from DQA encoding. No numeric encoding is reused across types to prevent Merkle hash ambiguity.

### Intended Use

```
BigInt is designed for:
- deterministic arithmetic
- financial calculations
- protocol-level numeric operations (counters, balances, indices)

BigInt is NOT intended for:
- Implementing cryptographic primitives inside smart contracts
- Ed25519, RSA, ECC, or similar crypto operations
- High-performance computing workloads

Note: Cryptographic operations must use specialized primitives, not BigInt.
BigInt's O(n²) multiplication and intentional determinism make it unsuitable
for crypto. Ed25519 arithmetic uses finite fields, not arbitrary integers.
```

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
const MAX_LIMBS: usize = 64;

/// Maximum gas cost per BIGINT operation (worst case)
const MAX_BIGINT_OP_COST: u64 = 15000;
```

> **Note:** MAX_BIGINT_DIV_LIMBS has been removed. All operations support up to MAX_LIMBS (64).

## Arithmetic Semantics

**128-bit Intermediate Arithmetic Requirement:**

All limb arithmetic MUST use 128-bit intermediate precision to prevent overflow:

```
sum = (a_limb as u128) + (b_limb as u128) + (carry as u128)
result_limb = sum as u64
carry = (sum >> 64) as u64
```

Implementations in languages lacking native u128 MUST emulate it using two u64 values.

**Wrap vs Saturate:** All operations wrap on overflow (mod 2^64 for limbs).

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

       // Use wrapping subtraction to detect borrow
       diff = a_limb.wrapping_sub(b_limb)
       // If a_limb < b_limb + borrow, we underflowed and need to borrow
       let needs_borrow = a_limb < b_limb.wrapping_add(borrow);
       diff = diff.wrapping_sub(borrow)
       result_limbs.push(diff)
       // Borrow propagates: if we needed to borrow in this step,
       // we carry 1 to the next limb
       borrow = if needs_borrow { 1 } else { 0 }

     // After loop, borrow MUST be 0 (|a| >= |b| by design)

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
  - b.limbs.len <= MAX_LIMBS

Algorithm: Restoring division with D1 normalization

  1. If |a| < |b|: return ZERO

  2. Normalize: Shift b left until MSB is 1
     norm_shift = count_leading_zeros(b.limbs.last)
     b_norm = b << norm_shift
     a_norm = a << norm_shift

  3. Initialize quotient limbs: vec![0; a_norm.limbs.len]

  4. Main loop (for j from a_norm.limbs.len - 1 down to 0):
     a. Form estimate (D1):
        // Handle j=0 case: use a_norm.limbs[0] with implicit zero for j-1
        if j == 0:
          // For least significant limb, use single-limb division
          dividend = a_norm.limbs[0] as u128
        else if a_norm.limbs[j] == b_norm.limbs.last:
          q_estimate = u64::MAX
        else:
          // Standard D1: ((r[j] << 64) | r[j-1]) / d[m-1]
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

### bit_length() — Bit Length

```
fn bigint_bit_length(x: BigInt) -> usize

// Returns the number of bits required to represent x
// Zero returns 1 (for canonical zero representation)

if x == 0:
    return 1

top = x.limbs[x.limbs.len - 1]  // most significant limb
return (x.limbs.len - 1) * 64 + (64 - leading_zeros(top))
```

**Note:** `leading_zeros(u64)` returns the count of zero bits before the first 1 bit.

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

### Numeric Encoding Types

**Three canonical numeric encodings exist in the CipherOcto numeric tower:**

| Encoding | Type | Format |
|----------|------|--------|
| I128Encoding | Integer | 16 bytes, two's complement, big-endian |
| BigIntEncoding | Arbitrary Integer | Variable, see below |
| DqaEncoding | Decimal | Reference RFC-0105 |

**No numeric encoding is reused across numeric types.** This prevents Merkle hash ambiguity.

### I128Encoding (for i128 interoperability)

```
struct I128Encoding {
    value: i128
}
```

Canonical representation: 16 bytes, two's complement, big-endian.

### BigIntEncoding (BIGINT native format)

As defined below in §Canonical Byte Format.

### DqaEncoding (RFC-0105 decimal)

Reference RFC-0105: `value: i64`, `scale: u8`, `reserved: [7]`.

### Canonical Byte Format

For deterministic Merkle hashing, BIGINT uses this canonical wire format:

```
┌─────────────────────────────────────────────────────────────┐
│ Byte 0: Version (0x01)                                     │
│ Byte 1: Sign (0 = positive, 0xFF = negative)              │
│ Byte 2-3: Reserved (0x0000)                               │
│ Byte 4: Number of limbs (1-64)                              │
│ Byte 5-7: Reserved (0x000000)                              │
│ Byte 8+: Little-endian limbs (64 bits each)                 │
└─────────────────────────────────────────────────────────────┘
```

**Version byte rule:** Nodes MUST reject unknown versions. Current version: 0x01.
└─────────────────────────────────────────────────────────────┘

Total size: 8 + (num_limbs × 8) bytes
```

### i128 Interoperability

> **Clarification**: BIGINT uses a separate encoding from RFC-0105's DqaEncoding. They are NOT byte-identical. This prevents Merkle hash ambiguity between numeric types.

**BIGINT to i128 conversion** (for values in i128 range):

```
bigint_to_i128_bytes(b: BigInt) -> [u8; 16]

Precondition: b fits in i128 range (-2^127 to 2^127-1)

Algorithm:
  1. If b > 2^127 - 1 or b < -2^127: TRAP
  2. If b == 0: return [0x00, 0x00, ..., 0x00] (16 zeros)
  3. If b.sign is true: negate to get positive magnitude
  4. Convert magnitude to bytes (little-endian, 16 bytes):
     For i in 0..16:
       bytes[i] = (magnitude >> (i * 8)) as u8
  5. If b.sign is true: bytes[15] |= 0x80  // Set sign bit in MSB
  6. Return 16-byte representation

### i128 Round-Trip Test Vectors

| Operation | Input | Expected Result |
|-----------|-------|-----------------|
| i128::MIN | -2^127 | limbs=[0, 0x8000_0000_0000_0000], sign=true |
| i128::MAX | 2^127-1 | limbs=[0xFFFF_FFFF_FFFF_FFFF, 0x7FFF_FFFF_FFFF_FFFF], sign=false |
| i128 zero | 0 | limbs=[0], sign=false |
| Positive 1 | 1 | limbs=[1], sign=false |
| Negative -1 | -1 | limbs=[1], sign=true |
| i128::MAX + 1 | 2^127 | TRAP (out of range) |
| -i128::MIN overflow | -2^127 - 1 | TRAP (out of range) |

### Serialization Invariant

```
BIGINT → serialize → bytes → deserialize → BIGINT'
BIGINT == BIGINT'  // MUST be true
```

### Canonical Form Enforcement

After ANY operation, the result MUST be canonicalized using this **deterministic algorithm**:

```
fn bigint_canonicalize(x: BigInt) -> BigInt
  // Step 1: Remove leading zero limbs
  while x.limbs.len > 1 AND last(x.limbs) == 0:
       remove last limb

  // Step 2: Eliminate negative zero
  if x.limbs == [0]:
       x.sign = false  // positive only

  return x
```

**Canonical Invariants (mandatory):**
1. `limbs.len >= 1` — always at least one limb
2. `limbs[last] != 0` unless value == 0
3. Zero representation = `{limbs:[0], sign:false}`
4. Negative zero MUST NOT exist — eliminated by Step 2
5. No trailing zero limbs permitted

**Acceptance Test Vectors:**
- `[0] → [0]` (sign=false)
- `[0,0] → [0]` (trailing zeros removed)
- `[5,0,0] → [5]` (multiple zeros removed)
- `sign=true, limbs=[0] → sign=false` (negative zero eliminated)

**Every algorithm (ADD, SUB, MUL, DIV, MOD, SHL, SHR) MUST call canonicalize before returning.**

## Gas Model

BIGINT operations MUST scale gas costs with operand size to prevent DoS attacks:

| Operation | Gas Formula | Example (64 limbs) |
|-----------|------------|-------------------|
| ADD | 10 + limbs | 74 |
| SUB | 10 + limbs | 74 |
| MUL | 50 + 2 × limbs_a × limbs_b | 8,242 |
| DIV | 50 + 3 × limbs_a × limbs_b | 12,362 |
| MOD | Same as DIV | 12,362 |
| CMP | 5 + limbs | 69 |
| SHL | 10 + limbs | 74 |
| SHR | 10 + limbs | 74 |

**Unified Limits:**

```
MAX_LIMBS = 64
MAX_BIGINT_BITS = 4096
```

Operations must reject if `limbs > MAX_LIMBS`.

**Worst-Case Gas Bound Proof:**

| Operation | Max Formula | Max (64 limbs) |
|-----------|-------------|----------------|
| ADD/SUB | 10 + 64 | 74 |
| MUL | 50 + 2×64×64 | 8,242 |
| DIV/MOD | 50 + 3×64×64 | 12,362 |
| CMP | 5 + 64 | 69 |

**Proof:** All operations are ≤ 12,362 gas, well under MAX_BIGINT_OP_COST (15,000).
The worst case is 64×64 DIV: 50 + 3×4096 = 12,362 < 15,000. ✓

**Worst-case gas:** 64 × 64 multiplication = 8,242 gas (well under block limits).

**Gas Proof:** Every legal path (including worst-case 40-limb DIV + canonicalization) stays ≤ 15,000 gas. No path exceeds MAX_BIGINT_OP_COST (15,000). The single highest-cost path is a 40-limb restoring division followed by canonicalization (12,362 gas).

**Per-Block BIGINT Gas Budget:** 50,000 gas hard limit per block for all BIGINT operations combined.

## ZK Circuit Commitments (Mandatory for Future Proof System Integration)

### Schoolbook MUL Limb Reduction

- **Poseidon2 absorption schedule**: 2 limbs per field element (64 limbs → 32 Poseidon2 calls)
- **Gate count per limb**: 18 (mul + add + reduce)
- **Total gates for 64-limb MUL**: ≤ 1,152

### Poseidon2 Absorption Schedule

```
// For 64 limbs: process in chunks of 2
// absorption[i] = Poseidon2(limbs[2*i], limbs[2*i+1])
// Total: 32 absorptions for 64 limbs
fn poseidon2_absorb(limbs: &[u64; 64]) -> [FieldElement; 32] {
    let mut result = [FieldElement::zero(); 32];
    for i in 0..32 {
        result[i] = poseidon2(FieldElement(limbs[2*i]), FieldElement(limbs[2*i+1]));
    }
    result
}
```

### Reference Poseidon2 LUT Commitment

```
/// SHA-256 of the limb-reduction lookup table
/// This hash is included in the verification probe (Entry 16)
/// NOTE: This is a TBD placeholder. Actual value will be computed
/// from the committed LUT after specification finalization.
const BIGINT_POSEIDON2_LUT_HASH: [u8; 32] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];
```

> **Note**: This hash is a placeholder for the specification. Implementations MUST update this value when the LUT is finalized. Probe entry 51 verifies the LUT hash matches the committed value.

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

### Probe Entries (57 entries, 24-byte canonical format matching RFC-0104)

| Entry | Operation | Input A | Input B/Result | Purpose |
|-------|-----------|---------|----------------|---------|
| 0 | ADD | 0 | 2 | Basic |
| 1 | ADD | 2^64 + 1 | 1 | Multi-limb carry |
| 2 | ADD | MAX (2^64-1) | 1 | Carry overflow |
| 3 | ADD | 1 | -1 | Zero result |
| 4 | ADD | MAX | MAX | Max + max |
| 5 | SUB | -5 | -2 | Negative |
| 6 | SUB | 5 | 5 | Zero result |
| 7 | SUB | 0 | 0 | Zero minus zero |
| 8 | SUB | 1 | -1 | Underflow |
| 9 | SUB | MAX | 1 | Max - 1 |
| 10 | MUL | 2 | 3 | Basic mul |
| 11 | MUL | 2^32 | 2^32 | Limb boundary |
| 12 | MUL | 0 | anything | Zero multiplication |
| 13 | MUL | MAX_LIMBS | MAX_LIMBS | 64×64 worst case |
| 14 | MUL | -3 | 4 | Negative × positive |
| 15 | MUL | -2 | -3 | Negative × negative |
| 16 | DIV | 10 | 3 | Division |
| 17 | DIV | 100 | 10 | Exact division |
| 18 | DIV | MAX | 1 | Division by one |
| 19 | DIV | 1 | MAX | Division by max |
| 20 | DIV | 2^128 | 2^64 | Large division |
| 21 | MOD | -7 | -1 | MOD sign |
| 22 | MOD | 10 | 3 | Basic MOD |
| 23 | MOD | MAX | 3 | MOD edge |
| 24 | SHL | 1 | 2^4095 | Max shift |
| 25 | SHL | 1 | 64 | Limb shift |
| 26 | SHL | 1 | 1 | Shift by 1 |
| 27 | SHL | MAX | 1 | Shift max by 1 |
| 28 | SHR | 2^4095 | 1 | Bit shift boundary |
| 29 | SHR | 2^4096 | 0 | Shift to zero |
| 30 | SHR | 2^128 | 64 | Limb shift |
| 31 | SHR | 1 | 0 | Shift to zero |
| 32 | CANONICALIZE | [0,0,0] | [0] | Trailing zeros |
| 33 | CANONICALIZE | [5,0,0] | [5] | Multiple zeros |
| 34 | CANONICALIZE | [-0] | [+0] | Negative zero |
| 35 | CANONICALIZE | [1,0] | [1] | Single trailing |
| 36 | CANONICALIZE | [MAX,0,0] | [MAX] | Max trailing |
| 37 | CMP | -5 | -3 | Comparison |
| 38 | CMP | 0 | 1 | Zero vs one |
| 39 | CMP | MAX | MAX | Equal maxes |
| 40 | CMP | -MAX | MAX | Neg vs pos |
| 41 | CMP | 1 | 2 | One vs two |
| 42 | i128 MAX | 2^127-1 | round-trip | RFC-0105 |
| 43 | i128 MIN | -2^127 | round-trip | RFC-0105 |
| 44 | i128 zero | 0 | round-trip | Canonical |
| 45 | i128 | 1 | round-trip | Single |
| 46 | i128 | -1 | round-trip | Negative one |
| 47 | BITLEN | 0 | 1 | Zero bitlen |
| 48 | BITLEN | 1 | 1 | Single bit |
| 49 | BITLEN | MAX | 4096 | Max bitlen |
| 50 | BITLEN | 2^63 | 64 | Power of 2 |
| 51 | ZK LUT | Poseidon2 | hash | Gate verify |
| 52 | 4096-bit | MAX | +1 | Overflow trap |
| 53 | Carry chain | 2^64-1 + 1 | 2^64 | Full carry |
| 54 | Borrow chain | 0 - 1 | -1 | Underflow |
| 55 | Serialize | MAX | versioned | Format verify |
| 56 | Deserialize | bytes | MAX | Parse verify |

> **Note:** Entries 57-63 removed. POW, AND, OR, XOR, NOT are not specified in this RFC.

### Differential Fuzzing Requirement

All implementations MUST pass differential fuzzing against a reference library (e.g., num-bigint, GMP) with 100,000+ random inputs producing bit-identical outputs.

The fuzz harness command is: `cargo fuzz run bigint_fuzz -- -runs=10000`.

### Merkle Hash

```rust
struct BigIntProbe {
    entries: [[u8; 24]; 57],  // 57 entries × 24 bytes (matching RFC-0104)
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

## Determinism Guarantee

All operations defined in this RFC produce **identical results** across all compliant implementations regardless of:

- CPU architecture
- compiler
- programming language
- endianness (for wire format, see serialization)

This guarantee holds **provided** implementations follow:
1. The algorithms specified in this RFC
2. The canonicalization rules
3. The iteration bounds defined for each operation
4. The 128-bit intermediate arithmetic requirement

## Determinism Rules

1. **Algorithm Locked**: All implementations MUST use the algorithms specified in this RFC
2. **No Karatsuba**: Multiplication uses schoolbook O(n²) algorithm
3. **No SIMD**: Vectorized operations are forbidden
4. **Fixed Iteration**: Division executes exactly `bitlen(a)` iterations as specified in the algorithm
5. **Determinism Over Constant-Time**: Consensus determinism does NOT require constant-time execution. Implementations MAY use constant-time primitives but this is not required. The key requirement is algorithmic determinism (same inputs → same outputs).
6. **No Hardware**: CPU carry flags, SIMD, or FPU are forbidden
7. **Post-Operation Canonicalization**: Every algorithm MUST call canonicalize before returning

## Implementation Checklist

**Core Implementation:**
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

**Determinism & Safety:**
- [x] Gas calculation per operation
- [x] MAX_BIGINT_BITS enforcement (TRAP on overflow)
- [x] Post-operation canonicalization (all algorithms)
- [x] Per-block BIGINT gas budget (50,000)
- [x] i128 round-trip invariant verification (10 vectors)
- [x] Fixed-iteration DIV implementation (64 × limb count)
- [x] Constant-time comparison implementation (ct_lt/ct_sub or Barrett)

**Verification & Testing:**
- [x] Test vectors verified (40+ cases)
- [x] Verification probe implemented (20 entries, 24-byte format)
- [x] ZK circuit Poseidon2 commitment hash (Entry 16)
- [x] Differential fuzzing requirement (100,000+ random inputs)

**Acceptance Criteria:**
- All implementations MUST pass differential fuzzing against num-bigint
- Probe root MUST include all 20 entries with matching SHA-256
- Gas proof: worst-case 40-limb DIV + canonicalization ≤ 15,000
- Reference implementation: https://github.com/cipherocto/stoolap/blob/main/src/numeric/bigint.rs

## Spec Version & Replay Pinning

### numeric_spec_version

To ensure deterministic historical replay, all numeric implementations MUST declare a unified spec version that applies to DFP, DQA, and BigInt:

```rust
/// Numeric tower unified specification version (DFP, DQA, BigInt)
const NUMERIC_SPEC_VERSION: u32 = 1;
```

### Block Header Integration (normative)

**numeric_spec_version: u32** MUST be present in every block header at a defined offset.

```
┌─────────────────────────────────────────────────────────────┐
│ Block Header                                              │
├─────────────────────────────────────────────────────────────┤
│ ...                                                       │
│ numeric_spec_version: u32  // offset defined in header spec│
│ ...                                                       │
└─────────────────────────────────────────────────────────────┘
```

### Replay Rules (mandatory)

1. **Version Check**: If block.numeric_spec_version != current NUMERIC_SPEC_VERSION → reject block
2. **Historical Replay**: Load the exact algorithm version declared in the block header
3. **Algorithm Pinning**: All BIGINT operations inside the block MUST use the pinned version
4. **Canonical Form**: State transitions involving BIGINT MUST verify canonical form after each operation

> **Note**: This aligns with RFC-0104's DFP probe schedule (every 100,000 blocks).

## References

- RFC-0104: Deterministic Floating-Point
- RFC-0105: Deterministic Quant Arithmetic
- RFC-0106: Deterministic Numeric Tower (archived, superseded)
