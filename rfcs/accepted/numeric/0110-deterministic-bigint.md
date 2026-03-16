# RFC-0110 (Numeric/Math): Deterministic BIGINT

## Status

**Version:** 2.12 (2026-03-15)
**Status:** Accepted

> **Note:** This RFC is extracted from RFC-0106 (Deterministic Numeric Tower) as part of the Track B dismantling effort.

> **Adversarial Review v2.6 Changes (Complete Correctness):**
>
> - FIXED: quotient[j] = q_estimate assignment — quotient array was never written (D1)
> - ADDED: a_norm[j:] slice semantics definition (D2)
> - FIXED: divmod sign assigned before canonicalize (CC2)
> - FIXED: SHR canonicalize call and sequential step numbering (SR1)
> - ADDED: I128_ROUNDTRIP operation ID 0x000D (P3)
> - ADDED: Two-input probe verification procedure (P2)
> - FIXED: ZK section probe entry reference (P4)
> - FIXED: Deserialization canonical check with explicit limb checks (SE2)
> - FIXED: SHR test vector 2^4096→2^4095 (T1)
> - FIXED: DIV 2^2640/2^64 removed (was valid operation, not TRAP) (T3)
> - FIXED: DIV 2^4100 TRAP note (T2)
> - FIXED: SHL canonicalize before bit-length check (SH1)
> - ADDED: j=0 correctness comment (D3)
> - FIXED: Probe removal history note split (P1)
> - FIXED: Checklist ZK item updated (IC1)
> - ADDED: ADD gas/TRAP clarification (A1)
> - FIXED: num_limbs byte layout clarified (SE1)
> - FIXED: is_zero definition includes sign check (CC1)

> **Adversarial Review v2.7 Changes (Adversarial Review Fixes):**
>
> - FIXED: borrow overflow in a_norm[j:] subtraction — use two-step overflowing_sub (D1)
> - FIXED: bigint_to_i128_bytes step 3 explicit u128 reconstruction (I1)
> - FIXED: probe entries 42-46 Operation column → I128_ROUNDTRIP (P1)
> - FIXED: probe entries 51-53 concrete ADD/SUB operation IDs (P2)
> - FIXED: probe entries 54-55 concrete SHA-256 hash values (P3)
> - FIXED: probe verification procedure mentions Merkle root (P4)
> - ADDED: temp -= b_norm explanation comment (D2)
> - FIXED: Boundary Cases table ADD note clarifies 2^4095 bit width (B1)
> - FIXED: Boundary Cases SHL rows add Expected column (B2)
> - FIXED: Canonical Form Enforcement SHR row column alignment (T1)
> - FIXED: 4096-bit Boundary DIV row column layout (T2)
> - FIXED: bigint_to_i128_bytes backslash escapes removed (I2)
> - FIXED: duplicate gas proof paragraph removed (G1)
> - ADDED: divmod step 1 early return comment (D3)
> - ADDED: maximum wire size statement (SE1)
> - ADDED: SHL bit_shift == 0 guard comment (SH1)
> - ADDED: SHL canonicalize-before-TRAP ordering comment (SH2)
> - FIXED: DQA→BIGINT note moved to informative (A1)

> **Adversarial Review v2.8 Changes (Complete Adversarial Review Fixes):**
>
> - FIXED: probe entry 12 concrete value (0 × 1 = 0) (P1)
> - FIXED: probe entry 54-55 SHA-256 hash values for BigInt(1) (B1)
> - ADDED: probe Merkle root TBD placeholder (B2)
> - ADDED: MAX_BIGINT sentinel in probe format (P2)
> - ADDED: probe format SHL/BITLEN clarification note (P3)
> - ADDED: CMP result encoding in probe format (P4)
> - FIXED: Boundary Cases table ADD row column count (T1)
> - FIXED: Canonical Form Enforcement SHR row (T2)
> - FIXED: SUB step 2 returns canonicalize(a) (A2)
> - ADDED: result_sign assignment in SUB step 3 (M2)
> - ADDED: ADD result_limbs initialization (A1)
> - FIXED: ADD step 5 leading_zeros to proper bit-length calculation (M1)
> - ADDED: a_norm copy semantics when norm_shift == 0 (D1)
> - ADDED: quotient over-allocation note (D2)
> - FIXED: b_norm \* q_estimate type conversion (D3)

> **Adversarial Review v2.9 Changes (Final Review Fixes):**
>
> - ADDED: SHA-256 derivation footnote for entries 54-55 (H1)
> - FIXED: SUB step 3 variable definitions (A2)
> - FIXED: SUB step 4 uses larger_limbs/smaller_limbs (A3)
> - ADDED: bigint_divmod step 2 sign stripping for magnitudes (A4)
> - FIXED: ADD step sequential renumbering (A1)
> - FIXED: probe entry 24 Input B shows shift amount 4095 (P1)
> - ADDED: TRAP sentinel 0xDEAD_DEAD_DEAD_DEAD for probe (P3)
> - ADDED: SHR shift-amount note for probe entries (P2)
> - FIXED: Boundary Cases ADD row Input A (T1)
> - ADDED: Canonical Form Enforcement SHR note (T2)
> - FIXED: bigint_to_i128_bytes code fence (CF1, M1, M3)
> - ADDED: bytes array initialization in bigint_to_i128_bytes (M3)
> - FIXED: DIV q_estimate uses 0xFFFF_FFFF_FFFF_FFFFu128 (M2)

> **Adversarial Review v2.10 Changes (Critical Bug Fixes):**
>
> - FIXED: CRITICAL i128::MAX value in probe entry 42 (was 2^63-1, now 2^127-1)
> - FIXED: CRITICAL i128::MIN value in probe entry 43 (was -2^63, now -2^127)
> - FIXED: CRITICAL MOD divisor in probe entry 21 (was -1, now 3) — Rust semantics: -7 % -1 = 0, but expected result is -1
> - FIXED: CRITICAL SHR shift amount in probe entries 28-30 (was 2^128, now 2^4095) — shift source, not shift amount
> - FIXED: CRITICAL SHA-256 hash values in probe entries 54-55 — recomputed after encoding fix
> - FIXED: Corrected Merkle root after script bug fixes
>
> **Adversarial Review v2.11 Changes (Script Bug Fixes):**
>
> - FIXED: CRITICAL mk_entry negative encoding bug — negative integers were converted to strings then encode() returned zero (10 entries affected)
> - FIXED: HIGH entries 54-55 double-hash bug — hash reference value > MAX_U56 was hashed again; use raw HASHREF tuple
> - FIXED: MEDIUM entry 51 Input B = 1 not TRAP sentinel — ADD(MAX_BIGINT, 1) → TRAP overflow
> - FIXED: MEDIUM RFC table entry 21 (divisor -1 → 3) — MOD(-7, 3) = -1
> - FIXED: MEDIUM RFC table entry 30 (2^128 → 2^4095) — SHR shift source
> - FIXED: LOW RFC table entry 13 (MAX_LIMBS → MAX_BIGINT) — 64-limb × 64-limb → TRAP
> - FIXED: LOW bigint_to_i128_bytes if/else structure — use single val variable
> - FIXED: Corrected Merkle root after all script bugs resolved
>
> **Adversarial Review v2.13 Changes (Final Review Fixes):**
>
> - FIXED: LOW entry 1 label (2^64 + 1 → 2^64) — matches Python/Rust reference
> - FIXED: MEDIUM Rule 4 DIV iteration count — now correctly states m+1 where m = dividend.len() - divisor.len()
> - FIXED: Removed unnecessary j=0 special case — standard D1 formula works with implicit r[-1] = 0
>
> **Adversarial Review v2.12 Changes (All Review Findings):**
>
> - FIXED: MEDIUM sign encoding for small values — byte 7 = 0x80 for negative values ≤ 2^56
> - FIXED: LOW entry 40 table label (-MAX → -1)
> - ADDED: LOW CANONICALIZE entry 34 limitation note
> - FIXED: LOW entries 4 and 13 TRAP notation
> - FIXED: LOW bigint_to_i128_bytes code fence verification
> - ADDED: New Merkle root after sign encoding fix
>
> **Adversarial Review v2.5 Changes (Comprehensive Fixes):**
>
> - FIXED: bigint_divmod defined with quotient/remainder return (M1)
> - FIXED: SHL upper-carry uses |= not = (SH1)
> - FIXED: SHR result array initialized (SR1)
> - ADDED: bigint_deserialize algorithm (W3)
> - FIXED: Determinism Rule 4 iteration count (D1)
> - ADDED: Operation IDs for BITLEN/SERIALIZE/DESERIALIZE (P3)
> - FIXED: Probe entry 51 (ZK LUT) handling (P2)
> - ADDED: Probe field semantics (P1)
> - FIXED: MUL overflow check (T1)
> - FIXED: ADD boundary test vectors (T2)
> - FIXED: DIV by zero test vector (T3)
> - FIXED: Wire format limb byte order (W2)
> - FIXED: Duplicate closing fence removed (W1)
> - ADDED: MOD gas justification (G1)
> - ADDED: Per-block budget derivation (G2)
> - FIXED: Implementation checklist items (IC1)
> - ADDED: DIV restore precondition note (D3)
> - FIXED: b_norm.limbs.last() expression (D2)

> **Adversarial Review v2.4 Changes (Comprehensive Fixes):**
>
> - FIXED: SUB borrow detection with overflowing_sub (S1)
> - FIXED: DIV j=0 case properly sets q_estimate (D1)
> - FIXED: DIV clamp expression avoiding UB (D2)
> - FIXED: DIV double restore step per Knuth Algorithm D (D3)
> - FIXED: Determinism Rule 4 clarified for limb iteration (D4)
> - FIXED: ADD sum type annotation as u128 (A1)
> - FIXED: MUL high word computation and bounded carry loop (M1)
> - FIXED: MUL sign ordering before canonicalize (M2)
> - FIXED: SHR sign behavior preserved (SR1)
> - FIXED: SHL result array initialized to zero (SH1)
> - FIXED: bigint_to_i128_bytes algorithm for two's complement BE (I1)
> - FIXED: i128 MAX limb vector in boundary table (I2)
> - ADDED: MOD uses divmod for single-pass efficiency (G2)
> - FIXED: Gas proof paragraph uses 64-limb not 40-limb (G1)
> - FIXED: All 40-limb references replaced with 64-limb (ST1)
> - ADDED: Probe format encoding note (P1)
> - FIXED: Probe checklist count 20→57 (P2)
> - FIXED: Probe entry 29 uses legal input 2^4095 (SR2)
> - ADDED: Input Canonicalization Requirement section (C1, C2)
> - ADDED: Block Header Offset TBD placeholder (V1)
> - ADDED: Version Increment Policy section (V2)
> - FIXED: Fuzz harness run count 10000→100000 (ST3)
> - FIXED: ZK section header marked Informative (Z2)

> **Adversarial Review v2.2 Changes (Final Production-Grade):**
>
> - Added deterministic canonicalization algorithm (normative step-by-step)
> - Explicitly mandated 128-bit intermediate arithmetic with emulation rules
> - Specified canonical schoolbook multiplication algorithm
> - Bound division to bitlen(a) iteration count
> - Added serialization version byte
> - Proved gas upper bounds
> - Removed constant-time requirement (clarified optional)
> - Fully specified shift operations with carry behavior
> - Added determinism guarantee section
> - Expanded verification probe to 56 entries\*\*
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
>
> - Complete i128 round-trip proof with formal requirements + 8 additional vectors (entries 11-18)
> - Formalized DIV algorithm with verbatim limb-by-limb pseudocode + constant-time primitives
> - Finalized ZK LUT hash with actual SHA-256 placeholder + probe Entry 16 verification
> - Gas-model proof paragraph + per-block BIGINT budget (50,000)
> - Extended probe to 20 entries + differential fuzzing mandate
> - Constant-time enforcement guidance with intrinsics reference + 4 timing vectors

> **Adversarial Review v1.3 Changes:**
>
> - Added i128 round-trip invariant proof and 4 new test vectors (entries 11-14)
> - Added fixed-iteration DIV with constant-time guarantees (64 × limb count)
> - Extended verification probe to 16 entries with canonical-form checks
> - Formalized numeric_spec_version block-header integration rules
> - Added ZK circuit commitments (Poseidon2 gate counts)
> - Expanded test vectors to 40+ cases covering canonical-form enforcement
> - Added constant-time comparison mandate to Determinism Rules

> **Adversarial Review v1.2 Changes:**
>
> - Added i128 canonical serialization for byte-identical round-trip with RFC-0105
> - Added post-operation canonicalization mandate for all algorithms
> - Updated verification probe to 24-byte canonical format (matching RFC-0104)
> - Added numeric_spec_version for replay pinning

> **Adversarial Review v1.1 Changes:**
>
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

> **Note (Informative):** DQA (RFC-0105) may use BIGINT internally for
> intermediate values exceeding i128 precision. The interface between
> DQA and BIGINT is specified in RFC-0105.

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

The relationship "BIGINT provides i128 via 2×i64 limbs" means BIGINT _can_ represent i128 values, not that it _is_ i128.

## Motivation

### Problem Statement

| Integer Type | Range             | Limitation                                     |
| ------------ | ----------------- | ---------------------------------------------- |
| i8           | -128 to 127       | Too small                                      |
| i16          | -32,768 to 32,767 | Too small                                      |
| i32          | ±2.1B             | Too small                                      |
| i64          | ±9.2×10^18        | Cryptography needs 256-4096 bits               |
| i128         | ±2^127            | Insufficient for some cryptographic operations |

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

is_zero(x) = x.limbs == [0] && x.sign == false
// Precondition: x is canonical. Canonical zero has sign=false by invariant.
// The sign check is redundant for canonical inputs but prevents silent
// correctness errors if called on non-canonical values.
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

### Helper Functions

#### magnitude_cmp — Compare Absolute Values

```
magnitude_cmp(a_limbs: &[u64], b_limbs: &[u64]) -> i32

Compares the absolute values |a| and |b| as unsigned integers:
  - Returns -1 if |a| < |b|
  - Returns  0 if |a| == |b|
  - Returns +1 if |a| > |b|

Algorithm:
  1. Compare limb counts (more limbs = larger magnitude):
     if a_limbs.len() != b_limbs.len():
       return 1 if a_limbs.len() > b_limbs.len() else -1

  2. Compare limbs from most-significant to least-significant:
     for i in (0..a_limbs.len()).rev():
       if a_limbs[i] != b_limbs[i]:
         return 1 if a_limbs[i] > b_limbs[i] else -1

  3. All limbs equal: return 0
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

  3. let mut result_limbs: Vec<u64> = Vec::with_capacity(max(a.limbs.len, b.limbs.len) + 1);

  4. Limb-wise addition with carry:
     let carry: u64 = 0
     for i in 0..max(a.limbs.len, b.limbs.len):
       let sum: u128 = (carry as u128)
         + (if i < a.limbs.len { a.limbs[i] as u128 } else { 0u128 })
         + (if i < b.limbs.len { b.limbs[i] as u128 } else { 0u128 });

       result_limbs.push(sum as u64);
       carry = (sum >> 64) as u64;

  5. If carry > 0:
       result_limbs.push(carry)

  6. // Compute bit length of result:
     let top_limb = result_limbs.last().copied().unwrap_or(0);
     let result_bits = if top_limb == 0 {
       0  // only possible if result_limbs is empty, which cannot happen
     } else {
       (result_limbs.len() - 1) * 64 + (64 - top_limb.leading_zeros() as usize)
     };
     if result_bits > MAX_BIGINT_BITS: TRAP
     // Gas is charged based on max(a.limbs.len, b.limbs.len) regardless of TRAP.
     // Implementations MAY add an early-exit check before allocation:
     // if a.bits() == MAX_BIGINT_BITS && !b.is_zero() { TRAP }
     // This is an optimization, not a normative requirement.

  7. return BigInt { limbs: result_limbs, sign: result_sign }
```

### SUB — Subtraction

```
bigint_sub(a: BigInt, b: BigInt) -> BigInt

Preconditions:
  - a.bits() <= MAX_BIGINT_BITS
  - b.bits() <= MAX_BIGINT_BITS

Algorithm:
  1. If a == b: return ZERO

  2. If b is zero: return canonicalize(a)
     // a is already canonical per the input requirement, so canonicalize(a) = a.
     // This satisfies Determinism Rule 7 without changing the result.

  3. Compare magnitudes (ignoring signs):
     if magnitude_cmp(a.limbs, b.limbs) >= 0:  // |a| >= |b|
       result_sign = a.sign
       larger_limbs = a.limbs.clone()
       smaller_limbs = b.limbs.clone()
     else:
       result_sign = b.sign
       larger_limbs = b.limbs.clone()
       smaller_limbs = a.limbs.clone()
     // magnitude_cmp compares two limb arrays as unsigned integers,
     // MSB first (highest index first in little-endian representation).

  4. Limb-wise subtraction with borrow (larger_limbs - smaller_limbs):
     borrow = 0
     for i in 0..larger_limbs.len:
       a_limb = larger_limbs[i]
       b_limb = if i < smaller_limbs.len { smaller_limbs[i] } else { 0 }

       // Use overflowing subtraction to detect borrow correctly
       let (diff1, borrow1) = a_limb.overflowing_sub(b_limb);
       let (diff2, borrow2) = diff1.overflowing_sub(borrow);
       result_limbs.push(diff2);
       borrow = (borrow1 as u64) | (borrow2 as u64);

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

  1. If either is zero: return ZERO

  2. Result limbs = vec![0; a.limbs.len + b.limbs.len]

  3. Schoolbook multiplication:
     for i in 0..a.limbs.len:
       for j in 0..b.limbs.len:
         // Multiply two u64, result is u128
         let product: u128 = (a.limbs[i] as u128) * (b.limbs[j] as u128);

         // Add to result at position i+j
         let acc: u128 = (result.limbs[i+j] as u128) + (product & 0xFFFF_FFFF_FFFF_FFFFu128);
         result.limbs[i+j] = acc as u64;
         let mut carry: u128 = (acc >> 64) + (product >> 64);

         // Propagate carry with bounds checking
         let mut k = i + j + 1;
         while carry > 0 {
           debug_assert!(k < result.limbs.len());
           let s: u128 = (result.limbs[k] as u128) + carry;
           result.limbs[k] = s as u64;
           carry = s >> 64;
           k += 1;
         }

  4. Remove leading zero limbs
     result_bits = bigint_bit_length(result)
     if result_bits > MAX_BIGINT_BITS: TRAP

  5. result.sign = a.sign XOR b.sign

  6. result = canonicalize(result)

  7. return result
```

### bigint_divmod — Division with Remainder

```
bigint_divmod(a: BigInt, b: BigInt) -> (BigInt, BigInt)

Preconditions:
  - a.bits() <= MAX_BIGINT_BITS
  - b.bits() <= MAX_BIGINT_BITS
  - b != ZERO
  - b.limbs.len <= MAX_LIMBS

Algorithm: Restoring division with D1 normalization

  1. If |a| < |b|: return (ZERO, a)
     // When |a| < |b|: quotient = 0, remainder = a (preserving a's sign).
     // Correct: a = 0×b + a, and remainder sign matches dividend per convention.

  2. Normalize: Shift b left until MSB is 1
     norm_shift = count_leading_zeros(b.limbs[b.limbs.len - 1])
     b_norm = b << norm_shift
     a_norm = a << norm_shift

     // When norm_shift == 0, b_norm = b and a_norm = a.
     // a_norm MUST be a fresh copy — in-place modifications in step 4
     // MUST NOT affect the caller's a.
     // Implementation: always copy limbs, even if norm_shift == 0.

     // All inner-loop operations in step 4 use magnitudes only.
     // Strip signs from normalized values to prevent sign contamination
     // when b is negative.
     b_norm.sign = false;
     a_norm.sign = false;
     // Signs are re-applied to quotient and remainder in steps 5–6.

  3. Initialize quotient limbs: vec![0; a_norm.limbs.len]
     // The quotient array is over-allocated relative to the true quotient length
     // (true quotient has at most a_norm.limbs.len - b_norm.limbs.len + 1 limbs).
     // Trailing zero limbs are removed by canonicalize in step 6.

  4. Main loop (for j from a_norm.limbs.len - 1 down to 0):
     a. Form estimate (D1):
        // At j=0, a_norm.limbs[0] is the single leading limb; the standard
        // D1 formula ((r[j] << 64) | r[j-1]) works with r[-1] = 0.
        if a_norm.limbs[j] == b_norm.limbs[b_norm.limbs.len - 1]:
          q_estimate = 0xFFFF_FFFF_FFFF_FFFFu128
        else:
          // Standard D1: ((r[j] << 64) | r[j-1]) / d[m-1]
          q_estimate = ((a_norm.limbs[j] as u128) << 64 |
                        a_norm.limbs[j-1] as u128) /
                        b_norm.limbs[b_norm.limbs.len - 1] as u128

     b. Clamp estimate:
        if q_estimate > 0xFFFF_FFFF_FFFF_FFFFu128 {
          q_estimate = 0xFFFF_FFFF_FFFF_FFFFu128
        }

     c. Multiply and subtract (restoring):
        // Definition — Partial Remainder Slice:
        //   a_norm[j:] denotes the BigInt formed by limbs a_norm.limbs[j..a_norm.limbs.len],
        //   treated as a non-negative integer (sign ignored).
        //
        //   Comparison `temp > a_norm[j:]` uses bigint_cmp on magnitudes only.
        //
        //   Subtraction `a_norm[j:] -= temp` modifies a_norm.limbs in-place:
        //     let mut borrow: u64 = 0;
        //     for k in 0..temp.limbs.len:
        //       let (d1, b1) = a_norm.limbs[j+k].overflowing_sub(temp.limbs[k]);
        //       let (d2, b2) = d1.overflowing_sub(borrow);
        //       a_norm.limbs[j+k] = d2;
        //       borrow = (b1 as u64) | (b2 as u64);
        //     // Post-condition: borrow == 0 (guaranteed after at most 2 corrections)
        //
        //   After in-place subtraction, borrow MUST be 0 (guaranteed by the correctness
        //   of q_estimate after at most two corrections).

        // Precondition: q_estimate >= 0. The D1 normalization (MSB of b_norm = 1)
        // guarantees the initial estimate exceeds the true quotient digit by at most 2.
        // Therefore at most two corrections are needed and q_estimate will not underflow.
        // Convert q_estimate to a single-limb BigInt for multiplication.
        // Safe: q_estimate was clamped to u64::MAX in step 4b.
        let q_est_bigint = BigInt { limbs: vec![q_estimate as u64], sign: false };
        temp = bigint_mul(b_norm, q_est_bigint)
        // temp -= b_norm uses bigint_sub(temp, b_norm).
        // Since q_estimate >= 1 before each correction (D1 normalization guarantees
        // the estimate is at most 2 above the true digit), temp remains non-negative.
        // First correction
        if temp > a_norm[j:]:
          q_estimate -= 1
          temp -= b_norm
        // Second correction (required by Knuth D)
        if temp > a_norm[j:]:
          q_estimate -= 1
          temp -= b_norm
        a_norm[j:] -= temp
        quotient[j] = q_estimate as u64

  5. remainder = a_norm >> norm_shift
     remainder.sign = a.sign           // assign sign BEFORE canonicalize
     remainder = canonicalize(remainder) // canonicalize corrects negative zero

  6. quotient.sign = a.sign XOR b.sign  // assign sign BEFORE canonicalize
     quotient = canonicalize(quotient)   // canonicalize corrects negative zero

  7. Return (quotient, remainder)
```

---

**bigint_div** — Division (quotient only)

```
bigint_div(a: BigInt, b: BigInt) -> BigInt
  return bigint_divmod(a, b).0
```

---

**bigint_mod** — Modulo (remainder only)

```
bigint_mod(a: BigInt, b: BigInt) -> BigInt
  return bigint_divmod(a, b).1
```

> **Note**: MOD follows RFC-0105 convention: result has same sign as dividend.

### DIV — Division

> **Note:** DIV is implemented as `bigint_divmod(a, b).0`. See `bigint_divmod` algorithm above.

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

  3. result = vec![0u64; a.limbs.len + limb_shift + 1]
     result.sign = a.sign

  4. For each limb in a:
       // Guard required: when bit_shift == 0, the expression (64 - bit_shift) = 64,
       // and right-shifting a u64 by 64 is undefined behavior in C and zero in Rust.
       // When bit_shift == 0, no bits cross limb boundaries, so the upper carry is not needed.
       result.limbs[i + limb_shift] |= a.limbs[i] << bit_shift
       if bit_shift > 0:
         result.limbs[i + limb_shift + 1] |= a.limbs[i] >> (64 - bit_shift)

  5. result = canonicalize(result)
     // canonicalize is called before the TRAP check to remove trailing zero limbs
     // that were pre-allocated in step 3 but not written (when bit_shift == 0 and
     // the highest source limb has no high bits to carry). This does not reduce the
     // value — it is safe to call before the overflow check.
  6. if bigint_bit_length(result) > MAX_BIGINT_BITS: TRAP
  7. return result
```

### SHR — Right Shift

```
bigint_shr(a: BigInt, shift: usize) -> BigInt

Algorithm:
  1. if shift == 0: return a

  2. limb_shift = shift / 64
     bit_shift = shift % 64

  3. If limb_shift >= a.limbs.len: return ZERO

  4. result = vec![0u64; a.limbs.len - limb_shift]
     result.sign = a.sign  // SHR is arithmetic: sign preserved from input

  5. For i in 0..(a.limbs.len - limb_shift):
       result.limbs[i] = a.limbs[i + limb_shift] >> bit_shift
       if bit_shift > 0 and i + limb_shift + 1 < a.limbs.len:
         result.limbs[i] |= a.limbs[i + limb_shift + 1] << (64 - bit_shift)

  6. Remove leading zero limbs from result

  7. result = canonicalize(result)  // corrects sign if result is zero

  8. return result
```

## Serialization & Canonical Encoding

### Numeric Encoding Types

**Three canonical numeric encodings exist in the CipherOcto numeric tower:**

| Encoding       | Type              | Format                                 |
| -------------- | ----------------- | -------------------------------------- |
| I128Encoding   | Integer           | 16 bytes, two's complement, big-endian |
| BigIntEncoding | Arbitrary Integer | Variable, see below                    |
| DqaEncoding    | Decimal           | Reference RFC-0105                     |

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
│ Byte 4: Number of limbs (u8, range 1–64)                   │
│ Bytes 5-7: Reserved, MUST be 0x00                          │
│   (Bytes 4-7 together are a 4-byte field; future versions  │
│    MAY extend num_limbs to u32 via a new version byte.)    │
│ Byte 8+: Limb array in ascending limb order (limb[0] first), │
│          each limb serialized as 8 bytes, least-significant  │
│          byte first (little-endian within each limb).        │
└─────────────────────────────────────────────────────────────┘
```

**Version byte rule:** Nodes MUST reject unknown versions. Current version: 0x01.

Total size: 8 + (num_limbs × 8) bytes

Maximum size: 8 + (64 × 8) = 520 bytes (when num_limbs = MAX_LIMBS = 64).
Implementations MUST be able to handle buffers of up to 520 bytes.

### Deserialization Algorithm

```
bigint_deserialize(bytes: &[u8]) -> BigInt

1. If bytes.len < 8: TRAP (too short for header)
2. version = bytes[0]
   If version != 0x01: TRAP (unknown version)
3. sign_byte = bytes[1]
   If sign_byte == 0x00: sign = false
   else if sign_byte == 0xFF: sign = true
   else: TRAP (invalid sign byte)
4. If bytes[2] != 0x00 or bytes[3] != 0x00: TRAP (reserved bytes must be zero)
5. num_limbs = bytes[4] as usize
   If num_limbs == 0 or num_limbs > MAX_LIMBS: TRAP
6. If bytes[5] != 0x00 or bytes[6] != 0x00 or bytes[7] != 0x00: TRAP (reserved)
7. expected_len = 8 + num_limbs * 8
   If bytes.len != expected_len: TRAP (length mismatch)
8. For i in 0..num_limbs:
     limbs[i] = u64::from_le_bytes(bytes[8 + i*8 .. 16 + i*8])
9. Construct b = BigInt { limbs, sign }
10. Validate canonical form:
    a. If num_limbs > 1 AND limbs[num_limbs - 1] == 0: TRAP
       // Most significant limb must be non-zero for multi-limb values
    b. If num_limbs == 1 AND limbs[0] == 0 AND sign == true: TRAP
       // Negative zero is not canonical
11. Return b
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
3. Reconstruct magnitude as u128:
   // b fits in i128 range, so b has at most 2 limbs.
   magnitude: u128 = b.limbs[0] as u128;
   if b.limbs.len >= 2 {
     magnitude |= (b.limbs[1] as u128) << 64;
   }
4. let mut bytes = [0u8; 16];
5. let val: u128 = if b.sign == false {
     magnitude
   } else {
     (!magnitude).wrapping_add(1)
   };
6. for i in 0..16 {
     bytes[i] = ((val >> (120 - i*8)) & 0xFF) as u8;
   }
7. Return bytes
```

### i128 Round-Trip Test Vectors

| Operation           | Input      | Expected Result                                                  |
| ------------------- | ---------- | ---------------------------------------------------------------- |
| i128::MIN           | -2^127     | limbs=[0, 0x8000_0000_0000_0000], sign=true                      |
| i128::MAX           | 2^127-1    | limbs=[0xFFFF_FFFF_FFFF_FFFF, 0x7FFF_FFFF_FFFF_FFFF], sign=false |
| i128 zero           | 0          | limbs=[0], sign=false                                            |
| Positive 1          | 1          | limbs=[1], sign=false                                            |
| Negative -1         | -1         | limbs=[1], sign=true                                             |
| i128::MAX + 1       | 2^127      | TRAP (out of range)                                              |
| -i128::MIN overflow | -2^127 - 1 | TRAP (out of range)                                              |

### Serialization Invariant

```

BIGINT → serialize → bytes → deserialize → BIGINT'
BIGINT == BIGINT' // MUST be true

```

### Input Canonicalization Requirement (Normative)

All inputs to BIGINT operations MUST be in canonical form.
An implementation MUST reject (TRAP) any non-canonical input:

- Trailing zero limbs (except canonical zero [0])
- sign=true with limbs=[0] (negative zero)

### Canonical Form Enforcement

After ANY operation, the result MUST be canonicalized using this **deterministic algorithm**:

```

fn bigint_canonicalize(x: BigInt) -> BigInt
// Step 1: Remove leading zero limbs
while x.limbs.len > 1 AND last(x.limbs) == 0:
remove last limb

// Step 2: Eliminate negative zero
if x.limbs == [0]:
x.sign = false // positive only

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

| Operation | Gas Formula                | Example (64 limbs) |
| --------- | -------------------------- | ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| ADD       | 10 + limbs                 | 74                 |
| SUB       | 10 + limbs                 | 74                 |
| MUL       | 50 + 2 × limbs_a × limbs_b | 8,242              |
| DIV       | 50 + 3 × limbs_a × limbs_b | 12,362             |
| MOD       | Same as DIV                | 12,362             | (MOD uses `bigint_divmod`, computing remainder as a direct by-product of the division pass with no additional multiplication or subtraction step. Gas cost is therefore identical to DIV.) |
| CMP       | 5 + limbs                  | 69                 |
| SHL       | 10 + limbs                 | 74                 |
| SHR       | 10 + limbs                 | 74                 |

**Unified Limits:**

```

MAX_LIMBS = 64
MAX_BIGINT_BITS = 4096

```

Operations must reject if `limbs > MAX_LIMBS`.

**Worst-Case Gas Bound Proof:**

| Operation | Max Formula  | Max (64 limbs) |
| --------- | ------------ | -------------- |
| ADD/SUB   | 10 + 64      | 74             |
| MUL       | 50 + 2×64×64 | 8,242          |
| DIV/MOD   | 50 + 3×64×64 | 12,362         |
| CMP       | 5 + 64       | 69             |

**Proof:** All operations are ≤ 12,362 gas < MAX_BIGINT_OP_COST (15,000). ✓
The worst case is a 64-limb DIV: 50 + 3×4096 = 12,362.

**Per-Block BIGINT Gas Budget:** 50,000 gas hard limit per block for all BIGINT operations combined.
[TBD: This limit will be calibrated against target block time and expected transaction
throughput in the Block Execution RFC. The current value permits approximately 4 worst-case
DIV operations or ~675 ADD operations per block.]

## ZK Circuit Commitments _(Informative — not required for v1)_

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

> **Note**: This hash is a placeholder for the specification. Implementations MUST update this value when the LUT is finalized. LUT hash verification is part of the informative ZK annex and is NOT included in the normative probe. When the ZK annex is formalized in a future RFC, it will define its own verification probe separate from the normative probe defined in this document.

## Test Vectors

### Basic Operations

| Operation | Input A      | Input B   | Expected Result                |
| --------- | ------------ | --------- | ------------------------------ |
| ADD       | 0            | 0         | 0                              |
| ADD       | 1            | 1         | 2                              |
| ADD       | 1,000,000    | 2,000,000 | 3,000,000                      |
| ADD       | MAX (2^64-1) | 1         | 2^64 (0x1_0000_0000_0000_0000) |
| ADD       | -5           | 5         | 0                              |
| ADD       | -100         | 50        | -50                            |
| SUB       | 10           | 5         | 5                              |
| SUB       | 5            | 10        | -5                             |
| SUB       | 0            | 0         | 0                              |
| SUB       | -5           | -3        | -2                             |
| MUL       | 0            | 100       | 0                              |
| MUL       | 1            | 1         | 1                              |
| MUL       | 2            | 3         | 6                              |
| MUL       | 2^32         | 2^32      | 2^64                           |
| MUL       | -3           | 4         | -12                            |
| DIV       | 10           | 2         | 5                              |
| DIV       | 10           | 3         | 3 (integer)                    |
| DIV       | 2^64         | 2^32      | 2^32                           |
| DIV       | -10          | 2         | -5                             |
| MOD       | 10           | 3         | 1                              |
| MOD       | -10          | 3         | -1                             |
| MOD       | 2^64         | 2^32      | 0                              |

### Boundary Cases

| Operation | Input A  | Input B | Expected | Notes                                                 |
| --------- | -------- | ------- | -------- | ----------------------------------------------------- |
| ADD       | 2^4095   | 0       | 2^4095   | OK — 4096-bit value + 0 = 4096 bits ≤ MAX_BIGINT_BITS |
| ADD       | 2^4095   | 2^4095  | TRAP     | 4096-bit + 4096-bit = 4097 bits                       |
| MUL       | 4096-bit | 1       | 4096-bit | OK — 4096-bit × 1 = 4096 bits                         |
| DIV       | 1        | 0       | TRAP     | Division by zero — precondition violation             |
| SHL       | 1        | 2^4095  | 2^4095   | Shift to max bits → OK                                |
| SHL       | 1        | 2^4096  | TRAP     | Shift beyond max bits                                 |

### Extended Edge Cases

| Operation | Input A | Input B | Expected | Notes                                                               |
| --------- | ------- | ------- | -------- | ------------------------------------------------------------------- |
| ADD       | 2^4095  | 2^4095  | TRAP     | Overflow to 4096+ bits                                              |
| SUB       | 0       | 0       | ZERO     | Zero minus zero                                                     |
| SUB       | -5      | -5      | ZERO     | Equal negatives                                                     |
| MUL       | 2^2000  | 2^2000  | TRAP     | Exceeds 4096 bits                                                   |
| DIV       | 2^4000  | 2^100   | OK       | 64-limb division                                                    |
| DIV       | 2^4100  | 2^100   | TRAP     | Input a exceeds MAX_BIGINT_BITS (4096) — TRAP at precondition check |
| MOD       | -7      | 3       | -1       | Sign follows dividend                                               |
| MOD       | 7       | 3       | 1        | Positive remainder                                                  |
| SHR       | 2^4095  | 4095    | 1        | Shift by 4095                                                       |
| SHR       | 2^4095  | 4096    | ZERO     | Shift beyond width                                                  |
| SHR       | 1       | 64      | ZERO     | Shift by full limb                                                  |
| SHL       | 1       | 4095    | 2^4095   | Max shift OK                                                        |

### i64/i128 Boundary

| Operation     | Input                      | Expected                                                             |
| ------------- | -------------------------- | -------------------------------------------------------------------- |
| From i64 MIN  | -9,223,372,036,854,775,808 | limbs = [0x8000_0000_0000_0000], sign = true                         |
| From i64 MAX  | 9,223,372,036,854,775,807  | limbs = [0x7FFF_FFFF_FFFF_FFFF], sign = false                        |
| From i128 MIN | -2^127                     | limbs = [0, 0x8000_0000_0000_0000], sign = true                      |
| From i128 MAX | 2^127 - 1                  | limbs = [0xFFFF_FFFF_FFFF_FFFF, 0x7FFF_FFFF_FFFF_FFFF], sign = false |

### Round-Trip Tests

| Operation            | Input                                               | Expected               |
| -------------------- | --------------------------------------------------- | ---------------------- |
| i64→BIGINT→i64       | 42,000,000,000                                      | 42,000,000,000         |
| i128→BIGINT→i128     | 170,141,183,460,469,231,731,687,303,715,884,105,727 | Same                   |
| String→BIGINT→String | "0xDEADBEEF"                                        | "0xDEADBEEF"           |
| String→BIGINT→String | "12345678901234567890"                              | "12345678901234567890" |

### Canonical Form Enforcement

> **Note:** In SHR rows, Input notation `X >> N` means: input value X, shift amount N.

| Operation | Input          | Expected | Notes                                       |
| --------- | -------------- | -------- | ------------------------------------------- |
| SHR       | 0x100 >> 8     | 1        | 0x100 = 256 = 2^8; shift right 8 gives 1    |
| SHR       | 2^4095 >> 4096 | ZERO     | Shift count ≥ bit length → ZERO (canonical) |
| SUB       | -5 - (-5)      | ZERO     | Equal negatives → canonical zero            |
| SUB       | 5 - 5          | ZERO     | Equal positives → canonical zero            |
| DIV       | 10/4           | 2        | No leading zeros in quotient                |
| DIV       | 100/10         | 10       | Canonical (not 010)                         |
| MOD       | 10 % 3         | 1        | Remainder canonical                         |
| MUL       | 0 × anything   | ZERO     | Zero canonical form                         |

### Full i128 Round-Trip

| Operation     | Input                               | Expected                                            | Notes                 |
| ------------- | ----------------------------------- | --------------------------------------------------- | --------------------- |
| i128 MIN      | -2^127                              | limbs=[0,0x8000_0000_0000_0000], sign=true          | Exact round-trip      |
| i128 MAX      | 2^127-1                             | limbs=[0xFFFF_FFFF_FFFF_FFFF,0x7FFF_FFFF_FFFF_FFFF] | Exact round-trip      |
| i128 zero     | 0                                   | limbs=[0], sign=false                               | Canonical zero        |
| Negative zero | limbs=[0], sign=true → canonicalize | limbs=[0], sign=false                               | Canonical to positive |

### 4096-bit Boundary + Gas Edge Cases

| Operation | Input               | Expected | Notes                                                    |
| --------- | ------------------- | -------- | -------------------------------------------------------- |
| ADD       | 2^4095 + 2^4095     | TRAP     | Overflow to 4096+ bits                                   |
| ADD       | 2^4095 + 1          | 2^4095+1 | Max bits OK                                              |
| MUL       | 2^2000 × 2^2000     | TRAP     | Exceeds 4096 bits                                        |
| MUL       | 2^63 × 2^63         | 2^126    | Limb boundary × limb                                     |
| DIV       | 2^2560 / 2^2560     | 1        | 64-limb division OK                                      |
| DIV       | 2^4096+1 (dividend) | TRAP     | Input a exceeds MAX_BIGINT_BITS — precondition violation |
| SHL       | 1 << 4095           | 2^4095   | Max shift OK                                             |
| SHL       | 1 << 4096           | TRAP     | Exceeds max bits                                         |

## Verification Probe

BIGINT verification probe uses 24-byte canonical encoding (matching RFC-0104's DFP probe structure):

### Canonical Probe Entry Format (24 bytes)

```

┌─────────────────────────────────────────────────────────────┐
│ Bytes 0-7: Operation ID (little-endian u64) │
│ - 0x0001 = ADD │
│ - 0x0002 = SUB │
│ - 0x0003 = MUL │
│ - 0x0004 = DIV │
│ - 0x0005 = MOD │
│ - 0x0006 = SHL │
│ - 0x0007 = SHR │
│ - 0x0008 = CANONICALIZE │
│ - 0x0009 = CMP │
│ - 0x000A = BITLEN │
│ - 0x000B = SERIALIZE │
│ - 0x000C = DESERIALIZE │
│ - 0x000D = I128_ROUNDTRIP │
├─────────────────────────────────────────────────────────────┤
│ Bytes 8-15: Input A (canonical wire format) │
├─────────────────────────────────────────────────────────────┤
│ Bytes 16-23: Input B or Result (canonical wire format) │
└─────────────────────────────────────────────────────────────┘

```

> **Note:** Probe fields use a compact 8-byte encoding:
>
> - Values ≤ 2^56: little-endian in bytes 0-6
>   byte 7 = 0x00 for positive values
>   byte 7 = 0x80 for negative values
> - Hash reference: lower 8 bytes of SHA-256(canonical format)
> - Special: 0xFFFF_FFFF_FFFF_FFFF = MAX
> - Special: 0x0000_0000_0000_0000 = ZERO
>
> Full canonical verification via serialization entries 54–55.
>
> Note: Disambiguation between a negative small-integer encoding (byte 7 = 0x80)
> and a hash reference for a large value relies on the probe table's positional context.
> No actual byte-7 = 0x80 collisions exist among the 56 probe entries.
>
> Note: The compact encoding sign flag (byte 7 = 0x00 or 0x80) differs from the
> canonical wire format sign byte (byte 1 = 0x00 or 0xFF). These are distinct formats.

**Field Semantics by Operation Type:**

Two-input operations (ADD, SUB, MUL, DIV, MOD, CMP):
Bytes 8-15: Input A
Bytes 16-23: Input B

One-input with result (CANONICALIZE, BITLEN, SHR, SHL, SERIALIZE, DESERIALIZE):
Bytes 8-15: Input
Bytes 16-23: Expected Result

Round-trip operations (i128 entries 42-46):
Bytes 8-15: Input value
Bytes 16-23: Expected canonical BigInt encoding

**Verification Procedure:**

For two-input operations (ADD, SUB, MUL, DIV, MOD, CMP), the probe entry
encodes (op_id, input_a, input_b). Verification is performed by:

1. Executing op(input_a, input_b) per the algorithms in this RFC.
2. Comparing the result to the value produced by the reference implementation
   for the same inputs.

The probe root commits to the input set. Conformance is verified in two ways:

1. The Merkle root of all 56 probe entries MUST match the expected root published
   with this RFC. This verifies that the implementation encodes inputs identically.
2. For each probe entry, the implementation MUST produce the same output as any
   other conformant implementation. Output conformance is enforced via differential
   fuzzing (see §Differential Fuzzing Requirement).

The expected probe Merkle root for v2.13 is:
`c447fa82db0763435c1a18268843300c2ed811e21fcb400b18c75e579ddac7c0`

All compliant implementations MUST produce this root when computing the Merkle
hash over all 56 probe entries using the encoding rules defined in this section.

### Probe Entries (56 entries, 24-byte canonical format matching RFC-0104)

> **Note:** ZK LUT verification (removed from probe) is part of the informative ZK annex.
>
> **Note:** In SHL entries (e.g., entry 24), Input B encodes the shift amount as a plain integer
> (4095 for entry 24, not the result 2^4095). In BITLEN entries (e.g., entry 49),
> Input B/Result encodes the expected bit-length result (4096 for MAX).
>
> For SHR entries (28–31), Input B encodes the shift amount as a plain integer using the
> ≤ 2^56 compact encoding rule. The shift amount is not a BigInt operand.
>
> **Probe Format Field Semantics:**
>
> - Special: 0xFFFF_FFFF_FFFF_FFFF = MAX_BIGINT (the 4096-bit maximum: 2^4096 - 1,
>   i.e., BigInt with 64 limbs all equal to u64::MAX, sign=false)
> - Special: 0xDEAD_DEAD_DEAD_DEAD = TRAP (expected result for entries that
>   should cause a precondition violation; verification confirms the implementation
>   raises an error rather than returning a value)

> Comparison operations (CMP):
> Result encoding: CMP produces an Ordering, encoded as follows for verification:
> Less = 0xFFFF_FFFF_FFFF_FFFF
> Equal = 0x0000_0000_0000_0000
> Greater = 0x0000_0000_0000_0001

| Entry | Operation      | Input A                            | Input B/Result        | Purpose                                 |
| ----- | -------------- | ---------------------------------- | --------------------- | --------------------------------------- |
| 0     | ADD            | 0                                  | 2                     | Basic                                   |
| 1     | ADD            | 2^64                               | 1                     | Multi-limb carry                        |
| 2     | ADD            | MAX (2^64-1)                       | 1                     | Carry overflow                          |
| 3     | ADD            | 1                                  | -1                    | Zero result                             |
| 4     | ADD            | MAX                                | MAX                   | Max + max → TRAP (overflow; verified via fuzzing) |
| 5     | SUB            | -5                                 | -2                    | Negative                                |
| 6     | SUB            | 5                                  | 5                     | Zero result                             |
| 7     | SUB            | 0                                  | 0                     | Zero minus zero                         |
| 8     | SUB            | 1                                  | -1                    | Underflow                               |
| 9     | SUB            | MAX                                | 1                     | Max - 1                                 |
| 10    | MUL            | 2                                  | 3                     | Basic mul                               |
| 11    | MUL            | 2^32                               | 2^32                  | Limb boundary                           |
| 12    | MUL            | 0                                  | 1                     | Zero multiplication (0 × 1 = 0)         |
| 13    | MUL            | MAX_BIGINT                         | MAX_BIGINT            | 64-limb × 64-limb → TRAP (overflow; verified via fuzzing) |
| 14    | MUL            | -3                                 | 4                     | Negative × positive                     |
| 15    | MUL            | -2                                 | -3                    | Negative × negative                     |
| 16    | DIV            | 10                                 | 3                     | Division                                |
| 17    | DIV            | 100                                | 10                    | Exact division                          |
| 18    | DIV            | MAX                                | 1                     | Division by one                         |
| 19    | DIV            | 1                                  | MAX                   | Division by max                         |
| 20    | DIV            | 2^128                              | 2^64                  | Large division                          |
| 21    | MOD            | -7                                 | 3                     | MOD sign (MOD(-7, 3) = -1; sign follows dividend) |
| 22    | MOD            | 10                                 | 3                     | Basic MOD                               |
| 23    | MOD            | MAX                                | 3                     | MOD edge                                |
| 24    | SHL            | 1                                  | 4095                  | SHL(1, 4095) = 2^4095 — max legal shift |
| 25    | SHL            | 1                                  | 64                    | Limb shift                              |
| 26    | SHL            | 1                                  | 1                     | Shift by 1                              |
| 27    | SHL            | MAX                                | 1                     | Shift max by 1                          |
| 28    | SHR            | 2^4095                             | 1                     | Bit shift boundary                      |
| 29    | SHR            | 2^4095                             | 4096                  | Shift full width → ZERO                 |
| 30    | SHR            | 2^4095                             | 64                    | Limb shift (SHR(2^4095, 64) shifts out 1 limb) |
| 31    | SHR            | 1                                  | 0                     | Shift to zero                           |
| 32    | CANONICALIZE   | [0,0,0]                            | [0]                   | Trailing zeros                          |
| 33    | CANONICALIZE   | [5,0,0]                            | [5]                   | Multiple zeros                          |
| 34    | CANONICALIZE   | [-0]                               | [+0]                  | Negative zero                           |
| 35    | CANONICALIZE   | [1,0]                              | [1]                   | Single trailing                         |
| 36    | CANONICALIZE   | [MAX,0,0]                          | [MAX]                 | Max trailing                            |

> **Note:** CANONICALIZE entries 32–36 use the compact encoding which only represents
> canonical BigInt values. Entry 34 therefore encodes CANONICALIZE(+0) = +0,
> not CANONICALIZE(-0) = +0 (negative zero). Non-canonical input handling is
> verified via the Input Canonicalization Requirement and differential fuzzing.

| 37    | CMP            | -5                                 | -3                    | Comparison                              |
| 38    | CMP            | 0                                  | 1                     | Zero vs one                             |
| 39    | CMP            | MAX                                | MAX                   | Equal maxes                             |
| 40    | CMP            | -1                                 | 1                     | Neg vs pos                              |
| 41    | CMP            | 1                                  | 2                     | One vs two                              |
| 42    | I128_ROUNDTRIP | 2^127-1                            | round-trip            | i128::MAX round-trip                    |
| 43    | I128_ROUNDTRIP | -2^127                             | round-trip            | i128::MIN round-trip                    |
| 44    | I128_ROUNDTRIP | 0                                  | round-trip            | i128 zero                               |
| 45    | I128_ROUNDTRIP | 1                                  | round-trip            | i128 positive 1                         |
| 46    | I128_ROUNDTRIP | -1                                 | round-trip            | i128 negative 1                         |
| 47    | BITLEN         | 0                                  | 1                     | Zero bitlen                             |
| 48    | BITLEN         | 1                                  | 1                     | Single bit                              |
| 49    | BITLEN         | MAX                                | 4096                  | Max bitlen                              |
| 50    | BITLEN         | 2^63                               | 64                    | Power of 2                              |
| 51    | ADD            | 0xFFFF_FFFF_FFFF_FFFF (MAX_BIGINT) | 1                      | Overflow trap (ADD result > MAX_BIGINT_BITS → TRAP; verified via fuzzing) |
| 52    | ADD            | 2^64-1                             | 1                     | 2^64 (full carry across limb boundary)  |
| 53    | SUB            | 0                                  | 1                     | -1 (borrow from zero)                   |
| 54    | SERIALIZE      | 1                                  | c4cbcdbb1fa3e794      | Serialize BigInt(1) → canonical bytes   |
| 55    | DESERIALIZE    | c4cbcdbb1fa3e794                   | 1                     | Deserialize canonical bytes → BigInt(1) |

> **Note:** Entries 54–55 compact encoding: SHA-256 of the canonical wire encoding of BigInt(1).
> BigInt(1) canonical bytes: `[0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]`
> SHA-256 first 8 bytes: `c4 cb cd bb 1f a3 e7 94`

> **Note:** The probe has been reduced from earlier versions in two stages:
>
> - v2.3: Entries 57–63 (POW, AND, OR, XOR, NOT) removed — these operations are not specified.
> - v2.5: Entry 51 (ZK LUT gate verify) removed — moved to informative ZK annex.
>   The normative probe is now 56 entries.

### Differential Fuzzing Requirement

All implementations MUST pass differential fuzzing against a reference library (e.g., num-bigint, GMP) with 100,000+ random inputs producing bit-identical outputs.

The fuzz harness command is: `cargo fuzz run bigint_fuzz -- -runs=100000`.

### Merkle Hash

```rust
struct BigIntProbe {
    entries: [[u8; 24]; 56],  // 56 entries × 24 bytes (matching RFC-0104)
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
4. **Fixed Iteration**: Division executes exactly `m + 1` outer iterations where `m = dividend.len() - divisor.len()`, i.e., `dividend.len() - divisor.len() + 1` total iterations. This matches the Knuth D algorithm: the loop iterates from `j = m` down to `j = 0` inclusive. No early exit is permitted.
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
- [x] i128 round-trip invariant verification (7 vectors)
- [x] DIV with D1 normalization and double restore (Knuth Algorithm D, bigint_divmod)
- [x] Constant-time comparison (optional — see Determinism Rule 5)

**Verification & Testing:**

- [x] Test vectors verified (40+ cases)
- [x] Verification probe implemented (56 entries, 24-byte format)
- [ ] ZK circuit commitments (informative only — not required for v1 compliance; see ZK annex)
- [x] Differential fuzzing requirement (100,000+ random inputs)

**Acceptance Criteria:**

- All implementations MUST pass differential fuzzing against num-bigint
- Probe root MUST include all 56 entries with matching SHA-256
- Gas proof: worst-case 64-limb DIV + canonicalization ≤ 15,000
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
│ numeric_spec_version: u32  // offset [TBD]                │
│ ...                                                       │
└─────────────────────────────────────────────────────────────┘
```

> **Note:** [TBD] will be defined in RFC-XXXX (Block Header Layout). This offset
> MUST NOT change once any block has been committed to a live network.

### Replay Rules (mandatory)

1. **Version Check**: If block.numeric_spec_version != current NUMERIC_SPEC_VERSION → reject block
2. **Historical Replay**: Load the exact algorithm version declared in the block header
3. **Algorithm Pinning**: All BIGINT operations inside the block MUST use the pinned version
4. **Canonical Form**: State transitions involving BIGINT MUST verify canonical form after each operation

> **Note**: This aligns with RFC-0104's DFP probe schedule (every 100,000 blocks).

### Version Increment Policy (Normative)

NUMERIC_SPEC_VERSION MUST be incremented when:

1. Any normative algorithm change in RFC-0104/0105/0110
2. Any change to canonical encoding formats
3. Any change to verification probe entries

Increment requires:

- New RFC or amendment approved by governance
- Minimum 2 epochs notice before activation at block H_upgrade
- Nodes MUST accept both version N and N+1 in window [H_upgrade - grace, H_upgrade]
- After H_upgrade, reject version N blocks

Version 0 is reserved and MUST NOT be used.

## References

- RFC-0104: Deterministic Floating-Point
- RFC-0105: Deterministic Quant Arithmetic
- RFC-0106: Deterministic Numeric Tower (archived, superseded)
- RFC-XXXX: Block Header Layout (TBD)
