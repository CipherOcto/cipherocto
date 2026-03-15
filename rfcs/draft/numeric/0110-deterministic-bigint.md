# RFC-0110 (Numeric/Math): Deterministic BIGINT

## Status

**Version:** 1.0 (2026-03-14)
**Status:** Draft

> **Note:** This RFC is extracted from RFC-0106 (Deterministic Numeric Tower) as part of the Track B dismantling effort.

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
| RFC-0105 (DQA) | Independent — no dependency |
| RFC-0111 (DECIMAL) | BIGINT provides i128 via 2×i64 limbs |

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
2. Zero represented as empty limbs with sign = false
3. Minimum number of limbs for the value
```

### Zero Handling

```
ZERO = BigInt { limbs: vec![], sign: false }

is_zero(x) = x.limbs.is_empty()
```

### Constants

```rust
/// Maximum bit width for BIGINT operations
const MAX_BIGINT_BITS: usize = 4096;

/// Maximum number of 64-bit limbs
/// 4096 bits / 64 bits = 64 limbs
const MAX_BIGINT_LIMBS: usize = 64;

/// Maximum gas cost per BIGINT operation
const MAX_BIGINT_OP_COST: u64 = 10000;
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

Algorithm: Binary long division (restoring)

  1. If |a| < |b|: return ZERO

  2. If b has single limb: use binary long division
     If b has multiple limbs: use Knuth algorithm

  3. Result sign = a.sign XOR b.sign

  4. Use D2 (double precision) normalization for efficiency:
     Shift b left until MSB is 1
     Shift a left by same amount
     Perform division
     Shift remainder right by shift amount

  5. Return quotient
```

### MOD — Modulo

```
bigint_mod(a: BigInt, b: BigInt) -> BigInt

Algorithm:
  1. quotient = bigint_div(a, b)
  2. remainder = a - (quotient * b)
  3. return remainder  // Always positive (or zero)
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

> **Note**: All gas costs MUST fit within MAX_BIGINT_OP_COST (10,000). Larger operations TRAP.

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

## Verification Probe

BIGINT verification probe uses Merkle-hash structure for cross-node verification:

```rust
struct BigIntProbe {
    /// Entry 0: 0 + 0 = 0
    entry_0: [u8; 32],
    /// Entry 1: 1 + 1 = 2
    entry_1: [u8; 32],
    /// Entry 2: MAX + 0 = MAX
    entry_2: [u8; 32],
    /// Entry 3: 2^64 + 1 = 2^64 + 1
    entry_3: [u8; 32],
    /// Entry 4: 2 * 3 = 6
    entry_4: [u8; 32],
    /// Entry 5: 2^32 * 2^32 = 2^64
    entry_5: [u8; 32],
    /// Entry 6: 10 / 3 = 3 (integer)
    entry_6: [u8; 32],
}

/// SHA-256 of all entries concatenated
fn bigint_probe_root(probe: &BigIntProbe) -> [u8; 32] {
    sha256(concat!(
        probe.entry_0,
        probe.entry_1,
        probe.entry_2,
        probe.entry_3,
        probe.entry_4,
        probe.entry_5,
        probe.entry_6
    ))
}
```

## Determinism Rules

1. **Algorithm Locked**: All implementations MUST use the algorithms specified in this RFC
2. **No Karatsuba**: Multiplication uses schoolbook O(n²) algorithm
3. **No SIMD**: Vectorized operations are forbidden
4. **Fixed Iteration**: Division uses fixed iteration count (not variable-time)
5. **No Hardware**: CPU carry flags, SIMD, or FPU are forbidden

## Implementation Checklist

- [ ] BigInt struct with limbs: Vec<u64> and sign: bool
- [ ] Canonical form enforcement (no leading zeros)
- [ ] ADD algorithm
- [ ] SUB algorithm
- [ ] MUL algorithm (schoolbook)
- [ ] DIV algorithm (binary long division)
- [ ] MOD algorithm
- [ ] CMP comparison
- [ ] SHL left shift
- [ ] SHR right shift
- [ ] From/To i64 conversion
- [ ] From/To i128 conversion
- [ ] From/To string conversion
- [ ] Gas calculation per operation
- [ ] MAX_BIGINT_BITS enforcement (TRAP on overflow)
- [ ] Test vectors verified
- [ ] Verification probe implemented

## References

- RFC-0104: Deterministic Floating-Point
- RFC-0105: Deterministic Quant Arithmetic
- RFC-0106: Deterministic Numeric Tower (archived, superseded)
