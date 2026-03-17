# RFC-0111 (Numeric/Math): Deterministic DECIMAL

## Status

**Version:** 1.19 (2026-03-17)
**Status:** Draft

> **Note:** This RFC is extracted from RFC-0106 (Deterministic Numeric Tower) as part of the Track B dismantling effort.

> **Adversarial Review v1.19 Changes (4 Issues Fixed):**
> - ISSUE-1 (CRITICAL): Entry 50 now tests negative overflow (-MAX + -1), returns TRAP
> - ISSUE-2 (HIGH): Entry 54 result corrected to {1, 0}
> - ISSUE-3 (HIGH): Entry 56 result corrected to {0, 0}
> - ISSUE-4 (MEDIUM): Python comment updated (56 → 57 entries)
> - Version updated to 1.19, new Merkle root

> **Adversarial Review v1.18 Changes (3 Issues Fixed):**
> - CRITICAL-1: Unified Merkle root values (lines 1457 and 1469)
> - HIGH-2: FROM_DQA now applies canonicalization (probe entry 48)
> - MEDIUM-2: Added 625-byte length assertion to config hash script
> - Version updated to 1.18

> **Adversarial Review v1.17 Changes (2 Python Bugs Fixed):**
> - ISSUE-3/ISSUE-5: Python SQRT now includes BUG-6 off-by-one correction
> - ISSUE-9: Python DIV now returns canonicalized result
> - ISSUE-6: Added config hash verification script
> - ISSUE-7: Fixed probe entry 25 description (explicit form)
> - ISSUE-8: Updated Known Issues table
> - Version updated to 1.17

> **Adversarial Review v1.16 Changes (7 Bugs Fixed):**
> - BUG-1: MUL RoundHalfEven sign-aware rounding for negative products
> - BUG-2: DIV unsafe cast i256→i128 with explicit range check
> - BUG-3: BIGINT→DECIMAL uses RFC-0110 I128_ROUNDTRIP
> - BUG-4: Probe entry 24b committed (57 entries, new Merkle root)
> - BUG-5: Config hash serialization uses deterministic big-endian u128
> - BUG-6: SQRT Newton-Raphson off-by-one correction
> - BUG-7: ROUND encoding documented in probe section
> - Version updated to 1.16

> **Adversarial Review v1.15 Changes (10 Remaining Issues Fixed):**
> - REMAINING-1: SQRT TRAP removed (scale 25-35 now valid via split multiplication)
> - REMAINING-2: Added bit_length(i256) normative definition
> - REMAINING-3: Probe Merkle root now includes outputs (80-byte format)
> - REMAINING-4: DIV variable naming clarified (abs_a → magnitude)
> - REMAINING-5: BIGINT→DECIMAL uses RFC-0110 defined conversion
> - REMAINING-6: MUL RoundHalfEven works on magnitude for negative products
> - REMAINING-7: Canonicalization heading corrected (lazy boundary model)
> - REMAINING-8: Probe scheduling normative (node startup + 100k blocks)
> - REMAINING-9: Config hash has canonical value
> - REMAINING-10: ADD/SUB use i256 throughout (no unsafe cast)
> - Version updated to 1.15

> **Adversarial Review v1.14 Changes (Remaining Issues):**
> - HIGH-1: Added Known Issue for +6 rule breaking algebraic identities
> - HIGH-3: Updated D1 known issue (40 iterations, convergence guarantee)
> - HIGH-4: Clarified probe Merkle root commits to (operation, input, output)
> - MED-2: Added Known Issue for SQRT probe gap (exact-result canonicalization)
> - MED-3: Added Known Issue for DQA↔DECIMAL round-trip drift
> - MED-4: Verified D2 exists (gas model benchmarks)
> - MED-6: Added Known Issue for probe entry 47 canonicalization gap
> - Version updated to 1.14

> **Adversarial Review v1.13 Changes (Critical Fixes):**
> - CRIT-1: SQRT scale_factor bounds check (if > 36 or < 0: TRAP)
> - CRIT-2: DIV sign handling consistency (work with abs, apply sign after)
> - CRIT-3: MUL negative overflow check (check both +MAX and -MAX)
> - CRIT-4: CMP always use i256 (removed i128 fast path)
> - CRIT-5: TO_DQA remainder uses abs() for correct rounding
> - HIGH-2: BIGINT → DECIMAL algorithm defined
> - HIGH-5: DIV rounding added to arithmetic config hash
> - MED-1: Canonicalization contradiction resolved (lazy model)
> - MED-5: TO_DQA quantum_scale bounds check (0-18)
> - Version updated to 1.13

> **Adversarial Review v1.12 Changes (Critical Fixes):**
> - C1: Unified division scale rule (max + 6) instead of (max + 18)
> - C2: Fixed SQRT algorithm with correct scale factor formula
> - C3: Fixed DIV rounding for negative numbers (round on magnitude, apply sign after)
> - C4: Fixed DECIMAL→DQA conversion (scale alignment before rounding)
> - C5: Probe now includes result in hash (80 bytes → 32 bytes)
> - C6: Fuzz against reference impl in determin/ (not external libs)
> - H2: Fixed ADD overflow check (use checked_add)
> - H3: CMP handles non-canonical inputs without explicit canonicalization
> - H5: Probe verified at node startup (not periodically)
> - Version updated to 1.12

> **Adversarial Review v1.11 Changes (System Architecture):**
> - Precision Growth Control: scale_result ≤ min(36, max(scale_a, scale_b) + 6)
> - Numeric Domain Isolation: No implicit Decimal ↔ DQA conversions during arithmetic
> - Arithmetic Configuration Commitment: DECIMAL_ARITHMETIC_CONFIG_HASH required
> - Version updated to 1.11

> **Adversarial Review v1.10 Changes (Algorithmic Correctness):**
> - C1: ADD/SUB now uses checked_mul with i256 for scale alignment
> - C2: MUL scale normalization with RoundHalfEven before clamping
> - C3: MUL overflow check after rounding (not before)
> - C4: DIV uses i256 for scale_diff multiplication
> - C5: SQRT replaced with integer sqrt algorithm (no DECIMAL_DIV)
> - C6: Canonicalization rule clarified (outputs MUST be canonical)
> - Version updated to 1.10

> **Adversarial Review v1.9 Changes (Production Hardening):**
> - FIX 1: Added Decimal range invariant (|mantissa| ≤ 10^36-1)
> - FIX 2: Canonicalization rule clarified (outputs MUST be canonical)
> - FIX 3: Safe scale alignment with overflow bounds checking
> - FIX 4: Multiplication requires 256-bit intermediate
> - FIX 5: Division precision rule added (min(36, max+18))
> - FIX 9: DECIMAL↔DQA conversion with explicit quantum
> - FIX 10: Gas model confirmed deterministic
> - Version updated to 1.9

> **Adversarial Review v1.8 Changes (Acceptance Path):**
> - SQRT convergence bound added (40 iterations, quadratic proof)
> - DIV rounding semantics clarified (matches RFC-0105)
> - Probe descriptions synchronized with Python/Rust
> - VM lazy canonicalization checklist completed
> - ZK constant-time note added
> - Prose inconsistencies fixed (10^36 → 10^36-1)
> - Version updated to 1.8

> **Adversarial Review v1.7 Changes (Post-Merge Fixes):**
> - C2: Probe description fixed (24→32-byte SHA256 hashes)
> - C4: Merkle root verification instructions added
> - H4: Implementation checklist updated (24→32-byte)
> - POW10 table verified with Python script (all 37 entries correct)
> - Version updated to 1.7

> **Adversarial Review v1.6 Changes (Post-Merge Fixes):**
> - C1: POW10 table entries 31-36 fixed (31-36 zeros each)
> - C2: Probe entry struct updated (24 → 32 bytes for SHA256)
> - C4: DIV scale_diff < 0 now uses RoundHalfEven (not truncation)
> - H1: DIV tie-breaking comment clarified
> - H2: CMP scale diff note corrected (18 → 36)
> - H4: Gas proof expanded with breakdown
> - H5: String conversion edge cases added (zero handling)

> **Adversarial Review v1.5 Changes (Post-Merge Fixes):**
> - H2: String conversion locale specification added (whitespace, sign handling)
> - H1: Gas model worst-case proof expanded to full table (matching RFC-0110 style)

> **Adversarial Review v1.4 Changes (Critical Issues Fixed):**
> - C1: POW10 table corrected (entries 29-30 fixed)
> - C2: DIV tie-breaking clarified (magnitude-first approach)
> - C3: Probe format explicitly uses Merkle leaf encoding (SHA256)
> - C4: Probe Merkle root remains [TBD] (requires reference impl)
> - H5: CMP algorithm added (copied from RFC-0105)

> **Adversarial Review v1.3 Changes (High-Severity Issues Fixed):**
> - H1: POW10 table corrected (entries 25-27 fixed)
> - H2: DIV algorithm added scale_diff < 0 handling
> - H3: SQRT deterministic initial guess specified
> - H4: Probe entry format clarified with compact encoding
> - H5: Probe Merkle root added [TBD]
> - H6: DIV tie-breaking uses result_sign
> - H7: Serialization byte order justification added
> - H8: MAX_DECIMAL_OP_COST constant added
> - H9: SQRT circular dependency clarified
> - H10: Probe entry 50 corrected overflow case
> - H11: String conversion locale specification added
> - H12: Lazy canonicalization rule added

> **Adversarial Review v1.2 Changes (Critical Issues Fixed):**
> - C1/C16: SQRT fixed iteration (40, no early exit)
> - C2: MUL overflow check order (scale first, round if exceeded)
> - C3: DIV sign handling (result sign before division)
> - C4: Input Canonicalization Requirement added
> - C5: Verification probe expanded to 57 entries
> - C6: i128 intermediate range check added
> - C7: ROUND Rust modulo semantics explicitly defined
> - C8: Gas model formula-based with worst-case proof
> - C9: DECIMAL→BIGINT canonicalize before conversion
> - C10: Canonical Byte Format (24 bytes)
> - C11: DQA→DECIMAL canonicalize after conversion
> - C12: String conversion full algorithm with 256-byte limit
> - C13: Determinism Guarantee section added
> - C14: Error codes use DqaError enum
> - C15: NUMERIC_SPEC_VERSION integration
> - C17: Differential Fuzzing Requirement (100,000+ runs)

## Authors

- Primary Author: TBD
- Contributing Reviewers: TBD

## Maintainers

- Lead Maintainer: TBD
- Technical Contact: TBD
- Repository: `rfcs/draft/numeric/0111-deterministic-decimal.md`

## Dependencies

### Required RFCs

| RFC | Relationship | Reason |
|-----|--------------|--------|
| RFC-0105 (DQA) | Required | DECIMAL extends DQA from i64→i128, scale 0-18→0-36 |
| RFC-0110 (BIGINT) | Required | i128 uses 2×i64 limbs internally |

### Optional RFCs

| RFC | Relationship | Reason |
|-----|--------------|--------|
| RFC-0104 (DFP) | Optional | Interoperability with floating-point |

## Design Goals

1. **Precision**: Support up to 36 decimal places for high-precision financial calculations
2. **Determinism**: Ensure bit-exact reproducible results across all implementations
3. **Compatibility**: Provide seamless conversion to/from DQA (RFC-0105)
4. **Performance**: Maintain 1.2-1.5x slower than DQA (acceptable for high-precision use cases)
5. **Safety**: Prevent overflow/underflow through explicit scale limits (0-36)

## Motivation

### Why DECIMAL?

While DQA (RFC-0105) provides sufficient precision for most financial calculations (up to 18 decimal places), certain use cases demand higher precision:

1. **High-precision risk calculations**: VaR, exotic derivatives, and complex financial models
2. **Regulatory requirements**: Some jurisdictions require more than 18 decimal places for specific instruments
3. **Scientific computing**: Certain scientific calculations benefit from extended precision
4. **Interoperability**: Compatibility with external systems that use higher precision decimals

DECIMAL addresses these requirements by extending DQA's approach to i128-based scaled integers, providing:
- Scale range: 0-36 (vs DQA's 0-18)
- Mantissa range: ±(10^36 - 1)
- Backward compatibility with DQA via explicit conversion

### When NOT to Use DECIMAL

- Default financial calculations: Use DQA (faster, sufficient precision)
- General computation: Use DFP (RFC-0104) for floating-point approximation
- Cryptographic operations: Use BIGINT (RFC-0110) for integer arithmetic

## Summary

This RFC defines Deterministic DECIMAL — extended-precision decimal arithmetic using i128-based scaled integers. DECIMAL provides higher precision than DQA (RFC-0105) for financial calculations requiring more than 18 decimal places.

## Relationship to Other RFCs

| RFC | Relationship |
|-----|--------------|
| RFC-0104 (DFP) | Independent |
| RFC-0105 (DQA) | DECIMAL extends DQA from i64→i128, scale 0-18→0-36 |
| RFC-0110 (BIGINT) | i128 uses 2×i64 limbs internally |

## When to Use DECIMAL vs DQA

| Aspect | DQA | DECIMAL |
|--------|-----|---------|
| Internal storage | i64 | i128 |
| Scale range | 0-18 | 0-36 |
| Performance | Faster (1x) | 1.2-1.5x slower |
| Use case | Default financial | High-precision risk |

**Recommendation:** Use DQA as default. Use DECIMAL only when:
- Scale > 18 required
- High-precision risk calculations (VaR, exotic derivatives)
- Regulatory requirements demand >18 decimal places

## Specification

### Data Structure

```rust
/// Deterministic DECIMAL representation
/// Uses i128 with decimal scale
pub struct Decimal {
    /// Signed 128-bit mantissa
    mantissa: i128,
    /// Decimal scale (0-36)
    scale: u8,
}
```

### Canonical Form

```
1. Trailing zeros removed from mantissa
2. Scale minimized without losing precision
3. Zero: mantissa = 0, scale = 0
```

### Value Representation

```
value = mantissa × 10^-scale
```

Examples:
- `Decimal { mantissa: 1234, scale: 2 }` = 12.34
- `Decimal { mantissa: 1000, scale: 3 }` = 1.000 → canonical: `{1, 0}`
- `Decimal { mantissa: 0, scale: 5 }` = 0 → canonical: `{0, 0}`

### Decimal Range Invariant (FIX 1)

A Decimal value MUST satisfy:

```
|mantissa| ≤ 10^36 − 1
scale ∈ [0, 36]
```

Implementations MUST reject any operation producing a mantissa outside this range.

**Violation raises:** `DECIMAL_OVERFLOW`

### Constants

```rust
/// Maximum scale for DECIMAL
const MAX_DECIMAL_SCALE: u8 = 36;

/// Maximum operation cost for any DECIMAL operation (gas limit)
const MAX_DECIMAL_OP_COST: u64 = 5000;

/// Maximum absolute mantissa: 10^36 - 1
const MAX_DECIMAL_MANTISSA: i128 = 10_i128.pow(36) - 1;

/// Minimum value: -(10^36 - 1)
const MIN_DECIMAL_MANTISSA: i128 = -(10_i128.pow(36) - 1);
```

### POW10 Table

Deterministic POW10 table for scale alignment and division (copied from `determin/src/dqa.rs:24-62`):

```rust
/// POW10[i] = 10^i as i128
/// Range: 10^0 to 10^36 (fits in i128: max is ~3.4 × 10^38)
const POW10: [i128; 37] = [
    1,                                     // 10^0
    10,                                    // 10^1
    100,                                   // 10^2
    1000,                                  // 10^3
    10000,                                 // 10^4
    100000,                                // 10^5
    1000000,                               // 10^6
    10000000,                              // 10^7
    100000000,                             // 10^8
    1000000000,                            // 10^9
    10000000000,                           // 10^10
    100000000000,                          // 10^11
    1000000000000,                         // 10^12
    10000000000000,                        // 10^13
    100000000000000,                       // 10^14
    1000000000000000,                      // 10^15
    10000000000000000,                     // 10^16
    100000000000000000,                    // 10^17
    1000000000000000000,                   // 10^18
    10000000000000000000,                  // 10^19
    100000000000000000000,                 // 10^20
    1000000000000000000000,                // 10^21
    10000000000000000000000,               // 10^22
    100000000000000000000000,              // 10^23
    1000000000000000000000000,             // 10^24
    10000000000000000000000000,            // 10^25
    100000000000000000000000000,           // 10^26
    1000000000000000000000000000,          // 10^27
    10000000000000000000000000000,         // 10^28
    100000000000000000000000000000,              // 10^29: 29 zeros
    1000000000000000000000000000000,             // 10^30: 30 zeros
    10000000000000000000000000000000,            // 10^31: 31 zeros
    100000000000000000000000000000000,           // 10^32: 32 zeros
    1000000000000000000000000000000000,          // 10^33: 33 zeros
    10000000000000000000000000000000000,         // 10^34: 34 zeros
    100000000000000000000000000000000000,        // 10^35: 35 zeros
    1000000000000000000000000000000000000,       // 10^36: 36 zeros
];
```

## Algorithms

### CANONICALIZE

```
decimal_canonicalize(d: Decimal) -> Decimal

1. If mantissa == 0: return {0, 0}  // Zero always has scale = 0

2. Remove trailing zeros:
   while mantissa % 10 == 0 and scale > 0:
     mantissa = mantissa / 10
     scale = scale - 1

3. Return normalized Decimal
```

**Canonical Invariants (mandatory):**
1. Zero representation = `{mantissa: 0, scale: 0}`
2. Trailing zeros removed (scale minimized without losing precision)
3. `|mantissa| ≤ 10^36-1` (fits in DECIMAL range)

### ADD — Addition (FIX 3 - Safe Scale Alignment)

```
decimal_add(a: Decimal, b: Decimal) -> Decimal

Preconditions:
  - a.scale <= MAX_DECIMAL_SCALE
  - b.scale <= MAX_DECIMAL_SCALE

Algorithm:
  1. Align scales:
     target_scale = max(a.scale, b.scale)
     diff_a = target_scale - a.scale
     diff_b = target_scale - b.scale

     // REMAINING-10: Keep intermediate in i256 through addition
     // Use i256 for scale alignment multiplication
     if diff_a > 0:
       match i256::from(POW10[diff_a]).checked_mul(i256::from(a.mantissa)):
         Some(val) => a_val_256 = val
         None => TRAP: DECIMAL_OVERFLOW
     else:
       a_val_256 = i256::from(a.mantissa)

     if diff_b > 0:
       match i256::from(POW10[diff_b]).checked_mul(i256::from(b.mantissa)):
         Some(val) => b_val_256 = val
         None => TRAP: DECIMAL_OVERFLOW
     else:
       b_val_256 = i256::from(b.mantissa)

     result_scale = target_scale

  2. Add in i256, then check range before casting to i128:
     // REMAINING-10: Keep in i256 through addition to avoid unsafe cast
     match a_val_256.checked_add(b_val_256):
       Some(sum_256) =>
         // Check range in i256 before casting
         if sum_256 > i256::from(MAX_DECIMAL_MANTISSA) or sum_256 < i256::from(-MAX_DECIMAL_MANTISSA):
           TRAP: DECIMAL_OVERFLOW
         else:
           sum = sum_256 as i128
       None => TRAP: DECIMAL_OVERFLOW

  3. Canonicalize result
```

### SUB — Subtraction

```
decimal_sub(a: Decimal, b: Decimal) -> Decimal

Algorithm: Same as ADD (REMAINING-10 fix), but subtract instead of add:
  1. Align scales using i256 (same as ADD step 1)
  2. Subtract in i256, check range, then cast to i128 (same as ADD step 2)
  3. Canonicalize result
```

### MUL — Multiplication (FIX 4 - 256-bit Intermediate)

```
decimal_mul(a: Decimal, b: Decimal) -> Decimal

Algorithm:
  1. Calculate raw scale first:
     raw_scale = a.scale + b.scale

  2. Scale normalization with rounding:
     // When raw_scale > MAX_DECIMAL_SCALE, we must round the product
     // before scaling down. This is the key fix for C2.
     if raw_scale > MAX_DECIMAL_SCALE:
       scale_reduction = raw_scale - MAX_DECIMAL_SCALE
       // First multiply at full precision, then round
       intermediate = i256::from(a.mantissa) * i256::from(b.mantissa)
       // Round using RoundHalfEven at the reduction point
       // REMAINING-6: Work on magnitude to handle negative products correctly
       divisor = i256::from(POW10[scale_reduction])
       (product_i256, remainder) = (intermediate / divisor, intermediate % divisor)
       // Apply RoundHalfEven rounding on MAGNITUDE (Rust % preserves sign of dividend)
       abs_remainder = if remainder < 0 { -remainder } else { remainder }
       half = divisor / 2
       if abs_remainder > half:
         // BUG-1 Fix: sign-aware rounding - round UP means larger magnitude
         if product_i256 >= 0:
           product_i256 = product_i256 + 1
         else:
           product_i256 = product_i256 - 1
       else if abs_remainder == half && product_i256 % 2 != 0:
         // Round to even: only round up if product is odd
         // BUG-1 Fix: sign-aware rounding
         if product_i256 >= 0:
           product_i256 = product_i256 + 1
         else:
           product_i256 = product_i256 - 1
       // Now scale is MAX_DECIMAL_SCALE
       result_scale = MAX_DECIMAL_SCALE
       // CRIT-3: Check overflow in both directions after rounding
       if product_i256 > i256::from(MAX_DECIMAL_MANTISSA) or product_i256 < i256::from(-MAX_DECIMAL_MANTISSA): TRAP
       product = product_i256 as i128
     else:
       // Normal case: no scale overflow
       result_scale = raw_scale
       // Multiply mantissas using 256-bit intermediate
       intermediate = i256::from(a.mantissa) * i256::from(b.mantissa)
       // Check overflow
       if |intermediate| > i256::from(MAX_DECIMAL_MANTISSA): TRAP
       product = intermediate as i128

  3. Canonicalize result
```

**FIX C2/C3 - MUL Scale Normalization:** When raw_scale > MAX_DECIMAL_SCALE, the multiplication MUST round the intermediate result BEFORE scaling down. This prevents the mathematical error where scale clamping loses precision. The algorithm: (1) compute full-precision product, (2) round using RoundHalfEven at the reduction point, (3) then clamp scale to MAX.

**FIX 4 Rationale:** Multiplication of two max DECIMAL values (10^36-1)² ≈ 10^72 exceeds i128 range (~10^38). Implementations MUST use 256-bit intermediate (i256 or RFC-0110 BIGINT) to prevent overflow. This matches RFC-0110 BIGINT multiplication approach.

### DIV — Division

```
decimal_div(a: Decimal, b: Decimal, target_scale: u8) -> Decimal

Algorithm:
  1. If b.mantissa == 0: TRAP (division by zero)

  2. Compute result scale using unified rule:
     // FIX C1: Division follows same precision growth rule as all operations
     raw_scale = max(a.scale, b.scale) + 6
     target_scale = min(MAX_DECIMAL_SCALE, raw_scale)

  3. Calculate result sign BEFORE division:
     // quotient can be zero, so use a.sign XOR b.sign, not sign(quotient)
     result_sign = (a.mantissa < 0) != (b.mantissa < 0)

  4. Scale to target precision:
     // CRIT-2: Work with absolute values, track sign separately
     scale_diff = target_scale + b.scale - a.scale
     // Work with magnitude only in this step
     abs_a = abs(a.mantissa)

     if scale_diff > 0:
       // Increase dividend by multiplying to get more precision
       // FIX C4: Use i256 to prevent overflow
       // BUG-2 Fix: Add explicit i128 range check before casting
       match i256::from(POW10[scale_diff as usize]).checked_mul(i256::from(abs_a)):
         Some(val) =>
           if val > i256::from(i128::MAX): TRAP: DECIMAL_OVERFLOW
           else: scaled_dividend = val as i128
         None => TRAP: DECIMAL_OVERFLOW
     else if scale_diff < 0:
       // Decrease dividend by dividing to reduce scale
       // MUST use RoundHalfEven rounding (not truncation)
       scale_reduction = -scale_diff
       divisor = POW10[scale_reduction as usize]
       // Work with absolute value for remainder calculation
       quotient = abs_a / divisor
       remainder = abs_a % divisor
       // Apply RoundHalfEven rounding
       half = divisor / 2
       if remainder > half:
         scaled_dividend = quotient + 1
       else if remainder == half:
         // Round to even: if quotient is odd, round up
         if quotient % 2 != 0 {
           scaled_dividend = quotient + 1
         } else {
           scaled_dividend = quotient
         }
       else:
         scaled_dividend = quotient
       // Sign will be applied in step 7, not here
     else:
       scaled_dividend = abs_a

  5. Divide (REMAINING-4 Fix: operate on absolute values, apply sign AFTER rounding):
     // INVARIANT: scaled_dividend is already in magnitude form (from step 4)
     magnitude = abs(scaled_dividend)
     abs_b = abs(b.mantissa)
     quotient = magnitude / abs_b
     remainder = magnitude % abs_b

  6. Round to target scale using RoundHalfEven:
     // FIX C3: RoundHalfEven is defined on MAGNITUDE, not sign
     // Apply rounding on absolute values, then apply sign in Step 7
     half = abs_b / 2
     if remainder < half:
       result = quotient  // round down
     else if remainder > half:
       result = quotient + 1  // round up
     else:
       // remainder == half (tie): round to even
       if quotient % 2 == 0:
         result = quotient  // already even, stay
       else:
         result = quotient + 1  // round up (magnitude)

  7. Apply sign AFTER rounding:
     if result_sign: result = -result

  8. Check overflow and canonicalize
```

**DIV Rounding Semantics (normative):** The algorithm computes the quotient directly at TARGET_SCALE precision and applies RoundHalfEven using a single remainder test. This is mathematically equivalent to full guard-digit rounding at exactly the requested scale and matches RFC-0105 DQA_DIV normative behaviour. It deliberately differs from PostgreSQL NUMERIC / Java BigDecimal guard-digit semantics only when the discarded digits would affect a tie at the (TARGET_SCALE+1) position; such cases are outside the 36-decimal guarantee of DECIMAL. The single-remainder method is chosen for performance while preserving determinism and consensus safety.

**Division Scale Rule (Unified with Precision Growth Control):** Division MUST obey the same precision growth rule as all operations:
```
result_scale = min(MAX_DECIMAL_SCALE, max(scale_a, scale_b) + 6)
```
This replaces the previous rule (max + 18) to ensure consistency across all arithmetic operations.

### SQRT — Square Root (CRIT-1 - Scale Factor Bounds Check)

```
decimal_sqrt(a: Decimal) -> Decimal

Algorithm: Deterministic integer square root based on precision growth control

  1. If a.mantissa < 0: TRAP (square root of negative)
  2. If a.mantissa == 0: return Decimal { mantissa: 0, scale: 0 }

  3. Compute result precision:
     // FIX C2: Follows precision growth rule: P = min(36, a.scale + 6)
     P = min(MAX_DECIMAL_SCALE, a.scale + 6)

  4. Scale mantissa to target precision:
     // To compute sqrt(m × 10^-s) with scale P precision:
     // Multiply by 10^(2P - s) to eliminate scale before sqrt
     scale_factor = 2 * P - a.scale
     // REMAINING-1: Only check for negative scale_factor (the actual constraint)
     // The POW10 table is NOT the gate - the constraint is whether
     // a.mantissa × 10^scale_factor overflows i256
     if scale_factor < 0: TRAP  // should not happen with P >= a.scale/2

     // n = m × 10^(2P-s), using split multiplication when scale_factor > 36
     // REMAINING-1: Use split multiplication to handle larger scale factors
     if scale_factor > 36:
       // Split: POW10[scale_factor] = POW10[36] × POW10[scale_factor - 36]
       lo = i256::from(POW10[scale_factor - 36])
       hi = i256::from(POW10[36])
       match i256::from(a.mantissa).checked_mul(lo):
         Some(partial) => match partial.checked_mul(hi):
           Some(n) => scaled_n = n
           None => TRAP: DECIMAL_OVERFLOW
         None => TRAP: DECIMAL_OVERFLOW
     else:
       match i256::from(a.mantissa).checked_mul(i256::from(POW10[scale_factor as usize])):
         Some(n) => scaled_n = n
         None => TRAP: DECIMAL_OVERFLOW

  5. Compute integer square root:
     // Use Newton-Raphson on i256, NOT on DECIMAL
     // Initial guess: 2^(bit_length(n)/2)
     x = 1_i256 << ((scaled_n.bit_length() + 1) / 2)

     // Iterate: x_new = (x + n/x) / 2
     // Use integer division, fixed 40 iterations for determinism
     repeat 40 times:
       x = (x + scaled_n / x) / 2

  6. Correct for off-by-one in integer sqrt:
     // BUG-6 Fix: Standard Newton-Raphson can overshoot by 1
     if x * x > scaled_n:
       x = x - 1

     // Ensure result fits in DECIMAL range
     if x < 0 or x > i256::from(MAX_DECIMAL_MANTISSA): TRAP

  7. Return result:
     return Decimal { mantissa: x as i128, scale: P }

  8. Canonicalize (redundant but explicit)
```

**REMAINING-2 - bit_length Definition (normative):**
The `bit_length(n: i256)` function used in step 5 is defined as:
```
bit_length(n: i256) -> u32:
  // n is guaranteed positive at this point
  // Equivalent to: 256 - n.unsigned_abs().leading_zeros()
  if n == 0: return 0
  return 256 - n.unsigned_abs().leading_zeros()
```

**FIX C2 - Correct SQRT Algorithm:** The mathematical correction ensures sqrt(m × 10^-s) produces correct magnitude:
- Target precision P = min(36, a.scale + 6) follows precision growth rule
- Scale factor 2P - s ensures integer sqrt produces correct decimal placement
- Uses i256 throughout to prevent overflow (scaled_n can be up to ~10^108)

This eliminates the circular dependency issue (no longer calls DIV) and ensures exact integer arithmetic throughout.

### ROUND — Rounding

```
decimal_round(d: Decimal, target_scale: u8, mode: RoundingMode) -> Decimal

Supported modes:
  - RoundHalfEven (default, required for financial)
  - RoundDown (floor toward zero)
  - RoundUp (away from zero)

Algorithm:
  1. If target_scale >= d.scale: return d (no rounding needed)

  2. diff = d.scale - target_scale

  3. divisor = 10^diff

  4. Apply rounding per mode:

     RoundHalfEven: (matches RFC-0105 exact algorithm)
       q = d.mantissa / divisor
       r = d.mantissa % divisor
       // Use absolute remainder for comparison (Rust % preserves sign of dividend)
       abs_r = abs(r)
       half = divisor / 2
       if abs_r < half: return q  // round down
       if abs_r > half: return q + sign(d.mantissa)  // round up
       // remainder == half (tie): round to even
       if q % 2 == 0: return q  // q is even, round to even
       else: return q + sign(d.mantissa)  // q is odd, round away from zero

     RoundDown:
       q = d.mantissa / divisor

     RoundUp:
       if r > 0: q += 1 (if positive) or q -= 1 (if negative)

  5. Return canonicalized Decimal
```

**Rust Modulo Semantics (Normative):**

The ROUND algorithm uses Rust's remainder semantics:
- `(-3) % 2 = -1` (odd dividend: result has same sign as dividend)
- `(-2) % 2 = 0` (even dividend: result is zero)
- `3 % 2 = 1` (positive dividend: result positive)

This is critical for RoundHalfEven correctness with negative values.

### Input Canonicalization Requirement (Normative)

Per RFC-0105 §Lazy Canonicalization, DECIMAL uses boundary canonicalization:

**At VM boundaries (external inputs):**
- Input is checked for canonical form
- Non-canonical input is REJECTED (TRAP)

**Internal operations:**
- All inputs to arithmetic operations are assumed canonical (per lazy model)
- This eliminates redundant canonicalization and supports lazy canonicalization

**Rejection criteria for external inputs:**
- Non-zero mantissa with trailing zeros not removed
- Zero representation with scale > 0 (canonical zero is `{0, 0}`)
- Mantissa outside range ±(10^36 - 1)
- Scale > 36

### Canonical Form Enforcement

After ANY operation, the result MUST be canonicalized using the CANONICALIZE algorithm defined above.

**Canonical Invariants (mandatory):**
1. Zero representation = `{mantissa: 0, scale: 0}`
2. Trailing zeros removed (scale minimized without losing precision)
3. `|mantissa| ≤ 10^36-1` (fits in DECIMAL range)

### Canonicalization Rule (REMAINING-7 Fix - FIX 2)

**Outputs MUST be canonical. External inputs MUST be canonical (TRAP otherwise). Internal operation inputs are guaranteed canonical by the post-operation canonicalization invariant.**

Per RFC-0105 §Lazy Canonicalization, DECIMAL implements lazy canonicalization at VM boundaries:

**On external input (deserialization, conversion from DQA/BIGINT):**
- Input is checked for canonical form
- Non-canonical input is REJECTED (TRAP)
- This ensures all internal operations receive canonical inputs

**On external output (serialization, conversion to DQA/BIGINT):**
- Output is always in canonical form (canonicalized before output)
- Results are guaranteed canonical

**Internal operations:**
- All arithmetic operations MUST canonicalize before returning
- This ensures intermediate results are always canonical

**Canonical form algorithm:**
```
while mantissa % 10 == 0 and scale > 0:
    mantissa /= 10
    scale -= 1
```

This approach matches RFC-0105's lazy canonicalization model and ensures deterministic behavior at VM boundaries.

### Canonical Byte Format

For deterministic Merkle hashing, DECIMAL uses this canonical wire format (24 bytes):

```
┌─────────────────────────────────────────────────────────────┐
│ Byte 0: Version (0x01)                                     │
│ Byte 1: Reserved (MUST be 0x00)                           │
│ Bytes 2-3: Reserved (MUST be 0x00)                        │
│ Byte 4: Scale (u8, range 0-36)                            │
│ Bytes 5-7: Reserved (MUST be 0x00)                        │
│ Bytes 8-23: Mantissa (i128 big-endian, two's complement)  │
└─────────────────────────────────────────────────────────────┘
```

**Version byte rule:** Nodes MUST reject unknown versions. Current version: 0x01.

**Reserved byte rule:** Bytes 1-3, 5-7 MUST be 0x00. TRAP if non-zero.

**Byte Order Justification (H7 Fix):** Big-endian is used for consistency with the decimal domain (DQA uses big-endian per RFC-0105). While RFC-0110's BIGINT uses little-endian for integer domain compatibility, DECIMAL follows RFC-0105's decimal convention. This ensures consistent decimal wire format across DQA and DECIMAL.

Total size: 24 bytes

### CMP — Comparison

Comparison returns -1 (less), 0 (equal), or 1 (greater).

**CRIT-4:** CMP uses i256 for all scale alignments. Per lazy canonicalization model, inputs are assumed canonical at operation boundaries.

```
fn cmp(a: Decimal, b: Decimal) -> i32

1. // Handle non-canonical inputs directly - no explicit canonicalization needed
   // The algorithm works correctly with trailing zeros
   a_work = a
   b_work = b

2. // Fast path: if both scales equal, compare values directly
   if a_work.scale == b_work.scale:
       if a_work.mantissa < b_work.mantissa: return -1
       if a_work.mantissa > b_work.mantissa: return 1
       return 0

3. // Scale alignment: normalize both to max_scale
   max_scale = max(a_work.scale, b_work.scale)
   scale_diff_a = max_scale - a_work.scale
   scale_diff_b = max_scale - b_work.scale

4. // CRIT-4: Always use i256 to prevent overflow (scale_diff can be up to 36)
   // After canonicalization, scale ≤ 36, so scale_diff can be up to 36
   // i128 multiplication overflows when diff > 18, so use i256 for all cases
   compare_a_i256 = i256::from(a_work.mantissa) * i256::from(POW10[scale_diff_a])
   compare_b_i256 = i256::from(b_work.mantissa) * i256::from(POW10[scale_diff_b])
   if compare_a_i256 < compare_b_i256: return -1
   if compare_a_i256 > compare_b_i256: return 1
   return 0

**Canonicalization Requirement (Normative):** Both operands MUST be canonicalized before comparison. This ensures `1.00` equals `1.0` correctly.

**CRIT-4 - CMP Complete Algorithm:** Always uses i256 for scale alignment to prevent overflow. After canonicalization, scale ≤ 36, so scale_diff can be up to 36 which exceeds i128 capacity. This eliminates the undefined "checked arithmetic or BigInt" behavior.

### Deserialization Algorithm

```
decimal_deserialize(bytes: &[u8]) -> Decimal

1. If bytes.len != 24: TRAP (invalid length)
2. version = bytes[0]
   If version != 0x01: TRAP (unknown version)
3. If bytes[1] != 0x00 or bytes[2] != 0x00 or bytes[3] != 0x00: TRAP (reserved)
4. scale = bytes[4] as u8
   If scale > 36: TRAP (invalid scale)
5. If bytes[5] != 0x00 or bytes[6] != 0x00 or bytes[7] != 0x00: TRAP (reserved)
6. mantissa = i128::from_be_bytes(bytes[8..24])
7. Return Decimal { mantissa, scale }
```

### Serialization Invariant

```
DECIMAL → serialize → bytes → deserialize → DECIMAL'
DECIMAL == DECIMAL' // MUST be true
```

## Conversions (FIX 9 - Explicit Quantization)

### DECIMAL → DQA (FIX C4 - Corrected Algorithm)

**Requires explicit quantum specification** (default: 10^-18):

```
decimal_to_dqa(d: Decimal, quantum_scale: u8 = 18) -> Dqa

// quantum_scale defines the quantization step: 10^-quantum_scale
// Default quantum_scale = 18 matches RFC-0105 DQA_MAX_SCALE

// MED-5: Bounds check quantum_scale
If quantum_scale > 18: TRAP (quantum_scale must be 0-18 for DQA)
If d.scale > 18: TRAP (precision loss)
If |d.mantissa| > i64::MAX: TRAP (overflow)

// FIX C4: Correct algorithm - align scales BEFORE rounding
// diff > 0: need to reduce scale (divide)
// diff < 0: need to increase scale (multiply)

diff = d.scale - quantum_scale

if diff > 0:
    // Reduce scale: divide with RoundHalfEven rounding
    divisor = POW10[diff as usize]
    // CRIT-5: Work with absolute values for correct rounding
    abs_mantissa = abs(d.mantissa)
    quotient = abs_mantissa / divisor
    remainder = abs_mantissa % divisor
    // RoundHalfEven on magnitude
    half = divisor / 2
    if remainder > half:
        rounded_mantissa = quotient + 1
    else if remainder == half:
        // Round to even: if quotient is odd, round up
        if quotient % 2 != 0:
            rounded_mantissa = quotient + 1
        else:
            rounded_mantissa = quotient
    else:
        rounded_mantissa = quotient
    // Apply original sign
    if d.mantissa < 0 {
        rounded_mantissa = -rounded_mantissa
    }

if diff < 0:
    // Increase scale: multiply (exact, no rounding)
    multiplier = POW10[(-diff) as usize]
    match i128::from(d.mantissa).checked_mul(i128::from(multiplier)):
        Some(v) => rounded_mantissa = v
        None => TRAP: DECIMAL_OVERFLOW

if diff == 0:
    rounded_mantissa = d.mantissa

Return Dqa { value: rounded_mantissa as i64, scale: quantum_scale }
```

### DQA → DECIMAL

```
dqa_to_decimal(d: Dqa) -> Decimal

1. Create Decimal: result = Decimal { mantissa: d.value as i128, scale: d.scale }

2. Canonicalize result (per RFC-0105 lazy canonicalization):
   result = canonicalize(result)

3. Return result
```

**FIX 9 Rationale:** DECIMAL ↔ DQA conversion requires explicit quantum specification to ensure deterministic quantization. The default quantum of 10^-18 matches RFC-0105 DQA_MAX_SCALE, ensuring round-trip consistency.

### DECIMAL → BIGINT

```
decimal_to_bigint(d: Decimal) -> BigInt

1. If d.scale > 0: TRAP (precision loss)

2. Canonicalize input:
   d = canonicalize(d)  // ensure no trailing zeros

3. Return BigInt::from(d.mantissa) per RFC-0110 From<i128> behavior
```

### BIGINT → DECIMAL

```
bigint_to_decimal(b: BigInt) -> Decimal

// BUG-3 Fix: Use RFC-0110 I128_ROUNDTRIP (op 0x000D) for proper conversion

1. Use RFC-0110 I128_ROUNDTRIP (op 0x000D) to convert b to i128:
   match bigint_i128_roundtrip(b):
     Ok(val) => mantissa = val
     Err(_)  => TRAP: DECIMAL_OVERFLOW

2. Check DECIMAL range (i128 > DECIMAL):
   if mantissa > MAX_DECIMAL_MANTISSA or mantissa < -MAX_DECIMAL_MANTISSA:
     TRAP: DECIMAL_OVERFLOW

3. Return Decimal { mantissa: mantissa, scale: 0 }

4. Canonicalize result:
   result = canonicalize(result)
   return result
```

### DECIMAL → String

```
decimal_to_string(d: Decimal) -> String

Precondition: Result MUST NOT exceed 256 bytes (TRAP if exceeded)

Algorithm:
  1. Handle zero special case:
     if d.mantissa == 0: return "0"  // Canonical zero always "0"

  2. If d.scale == 0: return d.mantissa.to_string()

  3. Handle sign:
     is_negative = d.mantissa < 0
     abs_mantissa = |d.mantissa|

  4. Calculate parts:
     divisor = POW10[d.scale as usize]
     integer_part = abs_mantissa / divisor
     fractional_part = abs_mantissa % divisor

  5. Format fractional part:
     fractional_str = fractional_part.to_string()
     // Pad with leading zeros to d.scale digits
     while fractional_str.len() < d.scale as usize {
       fractional_str = "0" + fractional_str;
     }

  6. Combine:
     if is_negative:
       return "-" + integer_part.to_string() + "." + fractional_str
     else:
       return integer_part.to_string() + "." + fractional_str
```

**Note:** Canonical form ensures no trailing zeros in fractional part, so `1.000` is stored as `{mantissa=1, scale=0}` (returns `"1"`), not `{mantissa=1000, scale=3}`. The zero special case handles canonical zero `{mantissa=0, scale=0}` which returns `"0"`.

**Locale Specification (Normative):**
- Decimal separator: period (`.`) only — never comma or other
- No thousands separators — digits are not grouped
- No exponent notation — never output scientific notation like "1.5e+10"
- Whitespace: trim leading/trailing, TRAP on internal whitespace
- Sign: optional '+' allowed for positive, '-' for negative
- Output uses ASCII characters only

## Determinism Guarantee

All operations defined in this RFC produce **identical results** across all compliant implementations regardless of:

- CPU architecture
- compiler
- programming language
- endianness (for wire format, see serialization)

This guarantee holds **provided** implementations follow:

1. The algorithms specified in this RFC
2. The canonicalization rules
3. The iteration bounds defined for each operation (40 for SQRT)
4. The 128-bit intermediate arithmetic requirement

## Determinism Rules

1. **Algorithm Locked**: All implementations MUST use the algorithms specified in this RFC
2. **No Karatsuba**: Multiplication uses schoolbook O(n²) algorithm
3. **No SIMD**: Vectorized operations are forbidden
4. **Fixed Iteration**: SQRT executes exactly 40 iterations (no early exit per RFC-0110 DIV rule)
5. **Determinism Over Constant-Time**: Consensus determinism does NOT require constant-time execution. Implementations MAY use constant-time primitives but this is not required. The key requirement is algorithmic determinism (same inputs → same outputs).

> **ZK Note:** For ZK circuit integration (post-v1) the DIV and SQRT algorithms will require constant-time implementations (Barrett reduction for division). The current fixed-iteration specification already satisfies the determinism requirement; constant-time is only a future ZK performance requirement.

6. **No Hardware**: CPU carry flags, SIMD, or FPU are forbidden
7. **Post-Operation Canonicalization**: Every algorithm MUST call canonicalize before returning
8. **i128 Intermediate**: All intermediate calculations use i128 (not arbitrary precision)
9. **Precision Growth Control**: scale_result ≤ min(36, max(scale_a, scale_b) + 6) — see §Precision Growth Control
10. **Numeric Domain Isolation**: No implicit Decimal ↔ DQA conversions during arithmetic — see §Numeric Domain Isolation
11. **Arithmetic Configuration Commitment**: All implementations MUST expose deterministic config hash — see §Arithmetic Configuration Commitment

## Precision Growth Control

Decimal operations can create precision amplification loops through repeated composition:

```
x = 1
repeat 100 times:
    x = x / 3
    x = x * 3
```

Even with deterministic rounding, this creates precision drift:
```
1 / 3 = 0.333333333333333333333333333333333333
*3    = 0.999999999999999999999999999999999999
```

After canonicalization: `0.999...` ≠ `1`

This enables **precision arbitrage** - systematic value leakage through rounding loops.

### Rule (Normative)

```
All arithmetic operations MUST produce results with:
    scale_result ≤ min(36, max(scale_a, scale_b) + 6)
```

### Rationale

- Prevents precision amplification beyond 6 decimal places per operation
- Caps at 36 (MAX_DECIMAL_SCALE) to maintain storage bounds
- Breaks precision arbitrage loops by limiting cumulative drift

### Example

```
Input A: 0.333333333333333333 (scale = 18)
Input B: 1.0 (scale = 0)

max(scale_a, scale_b) = 18
Allowed result scale ≤ min(36, 18 + 6) = 24
```

> **Known Issue (HIGH-1):** This rule may break expected algebraic identities in edge cases. For example, `(a × b) / b` does not always equal `a` due to the precision cap:
> - `1.0 × 3.0 = 3.0` (scale 0)
> - `3.0 / 3.0 = 1.0` (scale 0, exact division)
> - But with different scales: `1.0 × 3.000 = 3.000` (scale 3), then `3.000 / 3.0` → scale 3 → may not equal `1.000` after canonicalization
>
> Applications requiring exact algebraic identities should handle these edge cases explicitly.

## Numeric Domain Isolation

CipherOcto has three numeric domains that convert between each other:
- Decimal (RFC-0111): i128-based scaled integers, scale 0-36
- DQA (RFC-0105): i64-based scaled integers, scale 0-18
- BigInt (RFC-0110): arbitrary precision integers

Conversion is **not mathematically bijective**:

```
Decimal → DQA → Decimal
```

may produce different mantissa/scale after intermediate rounding.

### Example Failure

```
Decimal: 0.333333333333333333 (scale=18)
Convert to DQA (scale=18): 333333333333333333
Round-trip to Decimal: 0.333333333333333333
But if intermediate rounding occurs: 0.333333333333333334
Canonicalization produces DIFFERENT mantissa
```

This creates **cross-domain numeric representation drift** - different nodes may convert at different steps, causing economic divergence.

### Rule (Normative)

```
Each numeric operation MUST execute entirely within a single numeric domain.
- Decimal ops → Decimal only
- DQA ops → DQA only
- BigInt ops → BigInt only

Implicit conversions during arithmetic evaluation are FORBIDDEN.
```

### Mandatory Conversion Boundaries

Conversions are allowed only at:
- VM instruction boundaries
- Storage serialization
- Probe verification

### Rationale

Prevents consensus-layer hazards where different VM implementations may choose different internal domains, causing economic outcomes to diverge.

## Arithmetic Configuration Commitment

Probe verification proves arithmetic correctness but does **NOT** prove arithmetic configuration equivalence.

Two nodes can both pass probes but run different arithmetic environments:
- Different POW10 table generation
- Different overflow handling paths
- Different canonicalization thresholds

This creates a **conformance gap attack** - malicious implementation passes probes but exploits rare edge cases.

### Required Constant (Normative)

```
DECIMAL_ARITHMETIC_CONFIG_HASH: [u8; 32]
```

**REMAINING-9 Fix - Canonical Hash Value:**

Hash of the following configuration serialized in canonical format (SHA256):

**Serialization Format (BUG-5 Fix - Deterministic Big-Endian u128):**
```
Serialization format (deterministic, all values big-endian):
  [0..592]:   POW10[0..36] — 37 entries × 16 bytes each, big-endian u128
  [592..605]: "RoundHalfEven" — 13 bytes ASCII
  [605..618]: "RoundHalfEven" — 13 bytes ASCII (DIV)
  [618]:      MAX_DECIMAL_SCALE = 36 — 1 byte u8
  [619..623]: "TRAP" — 4 bytes ASCII
  [623]:      SQRT_ITERATIONS = 40 — 1 byte u8
  [624]:      PRECISION_CAP = 6 — 1 byte u8
  Total: 625 bytes
```

**Canonical Hash Value (BUG-5 Fix):**
```
DECIMAL_ARITHMETIC_CONFIG_HASH: b071fa37d62a50318fde35fa5064464db49c2faaf03a5e2a58c209251f400a14
```

### Verification Requirement

All nodes MUST verify arithmetic configuration hash matches the canonical value before participating in consensus.

### Rationale

Closes the conformance gap by ensuring all nodes run identical arithmetic configurations, not just correct ones.

## Gas Model (FIX 10 - Deterministic Gas)

Formula-based gas model (matching RFC-0110 style):

**Note:** This gas model is deterministic and consensus-safe. All operations have explicit formulas that account for scale differences, preventing DoS attacks via expensive operations.

| Operation | Formula | Description |
|-----------|---------|-------------|
| ADD | `10 + 2 × |scale_a - scale_b|` | Scale alignment + i128 add |
| SUB | `10 + 2 × |scale_a - scale_b|` | Scale alignment + i128 sub |
| MUL | `20 + 3 × scale_a × scale_b` | i128 mul + scale add |
| DIV | `50 + 3 × scale_a × scale_b` | Scale adjust + i128 div + round |
| SQRT | `100 + 5 × scale` | Newton-Raphson (40 iterations) |
| ROUND | `5 + diff` | Division by power of 10 |
| CANONICALIZE | `2 + trailing_zeros` | Trailing zero removal |
| TO_DQA | `3` | Scale check + cast |
| FROM_DQA | `2` | Zero-extend + canonicalize |
| TO_STRING | `10 + scale` | String allocation |

**Per-Block Budget:** 50,000 gas (matches RFC-0110 for BIGINT operations).

**Worst-Case Gas Bound Proof:**

| Operation | Max Formula | Max (scales=36) |
|-----------|-------------|-----------------|
| ADD/SUB   | 10 + 2×36   | 82              |
| MUL       | 20 + 3×36×36| 3,908           |
| DIV       | 50 + 3×36×36| 3,938           |
| SQRT      | 100 + 5×36  | 280             |
| ROUND     | 5 + 36      | 41              |
| CANONICALIZE | 2 + 36   | 38              |
| TO_STRING | 10 + 36    | 46              |

**Proof:** DIV has the highest gas cost at 3,938 gas (scale_a = scale_b = 36).
All other operations are ≤ 3,938 gas < MAX_DECIMAL_OP_COST (5,000). ✓

Worst-case breakdown:
- DIV: 50 + 3×36×36 = 50 + 3,888 = 3,938 gas
- MUL: 20 + 3×36×36 = 20 + 3,888 = 3,908 gas
- SQRT: 100 + 5×36 = 100 + 180 = 280 gas
- ADD/SUB: 10 + 2×36 = 10 + 72 = 82 gas

## Test Vectors

### Basic Operations

| Operation | a.mantissa | a.scale | b.mantissa | b.scale | Expected | Expected Scale |
|-----------|------------|---------|------------|---------|----------|----------------|
| ADD | 100 | 2 | 200 | 2 | 300 | 2 |
| ADD | 1000 | 3 | 1 | 0 | 1001 | 3 |
| SUB | 500 | 2 | 200 | 2 | 300 | 2 |
| MUL | 25 | 2 | 4 | 1 | 100 | 3 |
| DIV | 1000 | 3 | 2 | 0 | 500 | 3 |
| MUL | 12345678901234567890 | 18 | 2 | 0 | 24691357802469135780 | 18 |

### Scale Limits

| Operation | Input | Expected | Notes |
|-----------|-------|----------|-------|
| Scale 36 max | mantissa=1, scale=36 | OK | Max scale |
| Scale 37 overflow | mantissa=1, scale=37 | TRAP | Exceeds max |
| Mul overflow | scale=20 * scale=20 | TRAP | 20+20 > 36 |

### Rounding (RoundHalfEven)

| Input | Target Scale | Expected | Notes |
|-------|--------------|----------|-------|
| 1.234, 2 | 1 | 1.2 | 0.34 rounds down (4<5) |
| 1.235, 2 | 1 | 1.2 | 0.35 rounds to even (2) |
| 1.245, 2 | 1 | 1.2 | 0.45 rounds to even (2) |
| 1.255, 2 | 1 | 1.3 | 0.55 rounds to odd (3) |

### Rounding Negative Values (Critical for Consensus)

| Input | Target Scale | Expected | Notes |
|-------|--------------|----------|-------|
| -1.235, 2 | 1 | -1.2 | -0.35 rounds to even (-2→-1.2) |
| -1.245, 2 | 1 | -1.2 | -0.45 rounds to even (-2→-1.2) |
| -1.255, 2 | 1 | -1.3 | -0.55 rounds away from zero |
| -2.5, 1 | 0 | -2 | -0.5 rounds to even (-2) |
| -3.5, 1 | 0 | -4 | -0.5 rounds to even (-4) |

### Chain Operations

| Expression | Expected | Notes |
|------------|----------|-------|
| (1.5 × 2.0) + 0.5 | 3.5 | mul→add |
| (10.0 / 3.0) × 3.0 | 10.0 | div→mul, precision loss |
| sqrt(2.0) × sqrt(2.0) | 2.0 | sqrt→mul |

### Boundary Cases

| Operation | Input | Expected | Notes |
|-----------|-------|----------|-------|
| From i64 MAX | 9,223,372,036,854,775,807 | mantissa, scale=0 | OK |
| From i64 MIN | -9,223,372,036,854,775,808 | mantissa, scale=0 | OK |
| i128 boundary | ±(10^36-1) | mantissa, scale=36 | Max |
| Zero | 0 | {0, 0} | Canonical |

## Verification Probe

DECIMAL verification probe uses 32-byte SHA256 leaf hashes (per RFC-0111 §Canonical Probe Entry Format):

### Canonical Probe Entry Format (32 bytes - SHA256 leaf hash)

Each probe entry stores a SHA256 hash of the operation data INCLUDING OUTPUT:

```
┌─────────────────────────────────────────────────────────────┐
│ Bytes 0-31: SHA256(op_id || input_a || input_b || result)│
│   where:                                                    │
│     - op_id: 8 bytes (little-endian u64 operation ID)     │
│     - input_a: 24 bytes (DECIMAL canonical wire format)    │
│     - input_b: 24 bytes (DECIMAL canonical wire format)    │
│     - result: 24 bytes (DECIMAL canonical wire format)     │
│   Total raw data: 80 bytes → SHA256 output: 32 bytes      │
└─────────────────────────────────────────────────────────────┘
```

**FIX C5 - Probe Must Include Output:** The probe leaf MUST include the result to verify arithmetic correctness. Without the output, two implementations could produce different results yet have identical probe leaves, making the probe meaningless.

**Operation IDs:**
- 0x0001 = ADD
- 0x0002 = SUB
- 0x0003 = MUL
- 0x0004 = DIV
- 0x0005 = SQRT
- 0x0006 = ROUND
- 0x0007 = CANONICALIZE
- 0x0008 = CMP
- 0x0009 = SERIALIZE
- 0x000A = DESERIALIZE
- 0x000B = TO_DQA
- 0x000C = FROM_DQA

**Probe Entry Merkle Tree Encoding (REMAINING-3 Fix - HIGH-4 Clarification):**
- Each probe entry is a **Merkle tree leaf**: `SHA256(op_id || input_a || input_b || result)` = 32 bytes
- The probe stores 57 leaf hashes (32 bytes each)
- **The Merkle root commits to (operation, input_a, input_b, result) tuples** - INCLUDING the output
- The Merkle root of all 57 leaves is published with this RFC
- For TRAP entries (operations that should TRAP), encode a canonical TRAP sentinel: `{mantissa: 0x8000000000000000, scale: 0xFF}` (signals overflow/error condition)
- Verification: recompute each leaf hash and verify the Merkle root matches

**Verification Procedure:**

For two-input operations (ADD, SUB, MUL, DIV, CMP), the probe entry encodes (op_id, input_a, input_b, result). Verification is performed by:

1. Executing op(input_a, input_b) per the algorithms in this RFC.
2. Comparing the result to the value produced by the reference implementation for the same inputs.
3. Hashing (op_id || input_a || input_b || result) and verifying the Merkle leaf matches.

The probe root commits to the full tuple (operation + inputs + output). Conformance is verified in two ways:

1. The Merkle root of all 57 probe entries MUST match the expected root published with this RFC.
2. For each probe entry, the implementation MUST produce the same output as any other conformant implementation.

> **Note:** The probe commits to (operation, inputs, output) to ensure implementations not only use correct algorithms but also produce identical results. This prevents divergent implementations from passing probes.

### Probe Scheduling (REMAINING-8 Fix - Normative Rule)

> **Normative Rule:** Implementations MUST verify the DECIMAL probe Merkle root (1) at node startup before block production, and (2) at every block height multiple of 100,000. The probe verifies arithmetic correctness and prevents divergent implementations from affecting consensus.

### Probe Entries (57 entries, 32-byte SHA256 hashes)

> **BUG-7 Fix - ROUND encoding:** For ROUND probe entries, `input_b.mantissa` encodes the `target_scale` parameter. `input_b.scale` is 0. The result field encodes the rounded output.

| Entry | Operation      | Input A                            | Input B/Result        | Purpose                                 |
| ----- | -------------- | ---------------------------------- | --------------------- | --------------------------------------- |
| 0     | ADD            | 1.0 (mantissa=1, scale=0)         | 2.0                   | Basic                                   |
| 1     | ADD            | 1.5 (mantissa=15, scale=1)        | 2.0                   | 1.5 + 2.0 (scale alignment) |
| 2     | ADD            | 1.00 (mantissa=100, scale=2)      | 1.0                   | Trailing zeros                          |
| 3     | ADD            | 0.1 (mantissa=1, scale=1)          | 0.2 (mantissa=2, scale=1) | Decimal precision               |
| 4     | SUB            | 5.0                               | 2.0                   | Basic subtraction                       |
| 5     | SUB            | 1.5                               | 1.5                   | Zero result                             |
| 6     | SUB            | 0.1                               | 0.2                   | Negative result                         |
| 7     | SUB            | -1.5                              | -0.5                  | Negative subtraction                    |
| 8     | MUL            | 2.0 × 3.0                         | 6.0                   | Basic                                   |
| 9     | MUL            | 1.5 × 2.0                         | 3.0                   | Scale multiplication                   |
| 10    | MUL            | 0.1 × 0.2                         | 0.02                  | Decimal precision                       |
| 11    | MUL            | MAX (mantissa=10^36-1, scale=0)   | 1.0                   | Max boundary                            |
| 12    | MUL            | -2.0 × 3.0                       | -6.0                  | Negative multiplication                 |
| 13    | MUL            | -2.0 × -3.0                      | 6.0                   | Negative × negative                     |
| 14    | DIV            | 6.0 ÷ 2.0                        | 3.0                   | Basic division                          |
| 15    | DIV            | 1.000 ÷ 3.0                      | 0.333                 | 1.000 ÷ 3.0 |
| 16    | DIV            | 10.00 ÷ 3.0                      | 3.33 (RHE)            | 10.00 ÷ 3.0 |
| 17    | DIV            | 1.0 ÷ 2.0 (scale=1)              | 0.5                   | Exact division                          |
| 18    | DIV            | -6.0 ÷ 2.0                       | -3.0                  | Negative division                       |
| 19    | DIV            | 6.0 ÷ -2.0                       | -3.0                  | Negative divisor                        |
| 20    | SQRT           | 4.0                              | 2.0                   | Perfect square                          |
| 21    | SQRT           | 2.0                              | 1.414213... (scale=9) | Irrational                              |
| 22    | SQRT           | 0.04                             | 0.2                   | Decimal sqrt                            |
| 23    | SQRT           | 0.0001                           | 0.01                  | Small decimal                           |
| 24    | SQRT           | 0                                | 0                     | Zero                                    |
| 25    | SQRT           | 1.0 (mantissa=1, scale=25)       | {3162277660168379331, scale=31} | High-scale split multiplication |
| 26    | ROUND          | 1.234 → scale=1                  | 1.2                   | Round down                              |
| 27    | ROUND          | 1.235 → scale=1                  | 1.2                   | Round to even                           |
| 28    | ROUND          | 1.245 → scale=1                  | 1.2                   | Round to even (odd q)                   |
| 29    | ROUND          | 1.255 → scale=1                  | 1.3                   | Round up                                |
| 30    | ROUND          | -1.235 → scale=1                 | -1.2                  | Negative rounding                       |
| 31    | ROUND          | -1.245 → scale=1                 | -1.2                  | Negative round to even                  |
| 32    | ROUND          | -1.255 → scale=1                 | -1.3                  | Negative round up                       |
| 33    | CANONICALIZE   | 1000 (scale=3)                   | {1, 0}                | Trailing zeros                          |
| 34    | CANONICALIZE   | 0 (scale=5)                      | {0, 0}                | Zero                                    |
| 35    | CANONICALIZE   | 100 (scale=2)                    | {1, 0}                | Single trailing                         |
| 36    | CANONICALIZE   | 0.0 (mantissa=0, scale=2)        | {0, 0}                | Zero scale                              |
| 37    | CMP            | 1.0 vs 2.0                       | Less                  | Comparison                              |
| 38    | CMP            | 2.0 vs 1.0                       | Greater               | Comparison                              |
| 39    | CMP            | 1.5 vs 1.5                       | Equal                 | Equal                                   |
| 40    | CMP            | -1.0 vs 1.0                      | Less                  | Negative vs positive                    |
| 41    | CMP            | 1.0 vs 1.00                      | Equal                 | Same value, different scale             |
| 42    | CMP            | 0.1 vs 0.10                      | Equal                 | Trailing zeros                          |
| 43    | SERIALIZE      | 1.5                              | [01 00 00 00 01 00...] | Canonical bytes                   |
| 44    | DESERIALIZE    | [01 00 00 00 01 00...]          | 1.5                   | From bytes                              |
| 45    | TO_DQA         | 1.5 (scale=1)                    | Dqa(15, 1)            | Scale ≤ 18                              |
| 46    | TO_DQA         | 1.5 (scale=20)                   | TRAP                  | Scale > 18                              |
| 47    | FROM_DQA       | Dqa(15, 1)                       | 1.5                   | DQA → DECIMAL                          |
| 48    | FROM_DQA       | Dqa(0, 18)                       | 0.0                   | Max scale DQA                           |
| 49    | ADD            | MAX (10^36-1, scale=0)           | 1.0                   | Overflow trap (fuzzing)                |
| 50    | ADD            | -MAX + -1.0                      | TRAP                  | Underflow trap (-MAX + -1)             |
| 51    | MUL            | 10^18 (scale=0) × 10^19 (scale=0) | TRAP                | Mantissa overflow (10^37 > MAX) (fuzzing) |
| 52    | DIV            | 1.0 ÷ 0.0                       | TRAP                  | Division by zero                       |
| 53    | SQRT           | -1.0                             | TRAP                  | Negative sqrt                           |
| 54    | ADD            | 0.999999999999 + 0.000000000001  | {1, 0}                    | Canonicalizes to 1.0                  |
| 55    | MUL            | 0.000000000001 (scale=12) × 1000 (scale=0) | 0.000001 (scale=6) | Scale precision            |
| 56    | DIV            | 1.0 (scale=36) ÷ 3.0 (scale=0)  | {0, 0}              | Rounds to zero at max scale             |

### Differential Fuzzing Requirement (FIX C6 - Use Reference Implementation)

All implementations MUST pass differential fuzzing against the **reference implementation shipped with this RFC** in `determin/src/`, NOT external libraries.

**FIX C6 Rationale:** External libraries (rust_decimal, decimal.rs) may change rounding behavior, update algorithms, or include optimizations. Consensus spec must NEVER depend on external libraries - the reference implementation IS the spec.

The fuzz harness MUST verify:
- All operations produce identical results to reference implementation in `determin/`
- Canonical form is maintained after every operation
- Error cases (overflow, division by zero, etc.) are handled correctly

### Merkle Hash

```rust
struct DecimalProbe {
    entries: [[u8; 32]; 57],  // 57 entries × 32 bytes (SHA256 leaf hashes)
}

fn decimal_probe_root(probe: &DecimalProbe) -> [u8; 32] {
    // Build Merkle tree from 32-byte SHA256 leaf hashes
    let mut nodes: Vec<[u8; 32]> = probe.entries.to_vec();

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

**Probe Merkle Root (REMAINING-3 Fix - C3 Fix):**
> **Reference Merkle Root (80-byte format):** `496bc8038e3fd38462f4308bf03088b3f872d000256a45ddb53d4932efff0c1c`
>
> This root is computed from all 57 probe entries using SHA256 Merkle tree construction (see Python reference: `scripts/compute_decimal_probe_root.py`).

**Verification Instruction:**
All implementations MUST verify the Merkle root by:
1. Implementing all 57 probe entries per §Probe Entries table
2. For each entry, executing the operation to compute the result
3. Encoding each entry as 80-byte raw data (8-byte op_id + 24-byte input_a + 24-byte input_b + 24-byte result)
4. For TRAP entries, use sentinel encoding: {mantissa: 0x8000000000000000, scale: 0xFF}
5. Computing SHA256 hash of each 80-byte entry → 32-byte leaf hash
6. Building Merkle tree from 57 leaf hashes per §Merkle Hash algorithm
7. Verifying root matches: `496bc8038e3fd38462f4308bf03088b3f872d000256a45ddb53d4932efff0c1c`

**Cross-Verification:**
- Python: `python3 scripts/compute_decimal_probe_root.py` → outputs root above
- Rust: `cargo test decimal_tests::test_merkle_root` → verifies against reference

## Implementation Checklist

**Core Implementation:**
- [ ] Decimal struct with mantissa: i128, scale: u8
- [ ] Canonical form enforcement (no trailing zeros, zero={0,0})
- [ ] CANONICALIZE algorithm
- [ ] ADD with scale alignment and i128 range check
- [ ] SUB with scale alignment and i128 range check
- [ ] MUL with scale overflow rounding (per RFC-0105)
- [ ] DIV with target_scale and sign handling
- [ ] SQRT with Newton-Raphson (40 fixed iterations)
- [ ] ROUND with RoundHalfEven (Rust modulo semantics)
- [ ] CMP comparison algorithm

**Conversions:**
- [ ] From DQA conversion + canonicalize
- [ ] To DQA conversion (with scale ≤ 18 check)
- [ ] DECIMAL → BIGINT (canonicalize before, scale=0 check)
- [ ] BIGINT → DECIMAL (always valid)
- [ ] From/To string (256-byte limit)
- [ ] Serialize/Deserialize (24-byte canonical format)

**Determinism & Safety:**
- [ ] Gas calculation per operation (formula-based)
- [ ] MAX_DECIMAL_SCALE enforcement
- [ ] i128 intermediate range checks
- [ ] Post-operation canonicalization (all algorithms)
- [ ] Per-block DECIMAL gas budget (50,000)
- [x] Input canonicalization requirement (TRAP on non-canonical)
- [x] VM boundary lazy canonicalization (deserialization, DQA/BIGINT conversion) implemented and tested

The VM must invoke CANONICALIZE on every value returned by deserialize, dqa_to_decimal, and bigint_to_decimal before the value enters any arithmetic operation.
- [x] SQRT convergence bound (40 iterations, quadratic proof) documented and verified on all probe entries 20–24

**Verification & Testing:**
- [ ] Test vectors verified (40+ cases)
- [ ] Verification probe (57 entries, 32-byte SHA256 leaf hashes)
- [ ] Differential fuzzing (100,000+ random inputs vs rust_decimal)
- [x] Probe verification at node startup and every 100,000 blocks (REMAINING-8 fix)

## System Architecture

```mermaid
flowchart TB
    subgraph Input["Input Layer"]
        DQA[DQA i64]
        BIGINT[BIGINT arbitrary]
        STR[String]
    end

    subgraph Core["DECIMAL Core"]
        DEC[Decimal<br/>mantissa: i128<br/>scale: u8]
        CANON[Canonicalize]
        ADD[ADD]
        SUB[SUB]
        MUL[MUL]
        DIV[DIV]
        SQRT[SQRT]
        ROUND[ROUND]
    end

    subgraph Output["Output Layer"]
        DQA_OUT[DQA i64]
        BIGINT_OUT[BIGINT]
        STR_OUT[String]
    end

    DQA -->|dqa_to_decimal| DEC
    BIGINT -->|bigint_to_decimal| DEC
    STR -->|parse| DEC
    DEC --> CANON
    CANON --> ADD
    CANON --> SUB
    CANON --> MUL
    CANON --> DIV
    CANON --> SQRT
    CANON --> ROUND
    ADD --> DQA_OUT
    SUB --> BIGINT_OUT
    MUL --> STR_OUT
    DIV --> DEC
    SQRT --> DEC
    ROUND --> DEC
```

**Architecture Notes:**
- DECIMAL operates in the decimal domain, separate from INTEGER (BIGINT) and FLOAT (DFP) domains
- All operations flow through CANONICALIZE to ensure deterministic canonical form
- Conversions to DQA require explicit scale checks (scale ≤ 18)

## Error Handling

### Error Codes

DECIMAL uses the same `DqaError` enum as RFC-0105 for consistency:

| Error | Variant | Condition |
|-------|---------|-----------|
| DEC_OVERFLOW | `DqaError::Overflow` | Result exceeds ±(10^36 - 1) |
| DEC_SCALE_OVERFLOW | `DqaError::InvalidScale` | Scale exceeds 36 |
| DEC_DIVISION_BY_ZERO | `DqaError::DivisionByZero` | Division by zero |
| DEC_NEGATIVE_SQRT | `DqaError::InvalidInput` | Square root of negative |
| DEC_PRECISION_LOSS | `DqaError::InvalidInput` | Conversion to DQA loses precision (scale > 18) |
| DEC_INVALID_STRING | `DqaError::InvalidInput` | String parsing failure |
| DEC_INVALID_ENCODING | `DqaError::InvalidEncoding` | Reserved bytes non-zero in wire format |

### Error Semantics

All errors are fatal (TRAP) — no partial results or fallback behavior:
- Contract execution reverts on any DECIMAL error
- Gas is consumed up to the point of failure
- Error code is logged for debugging

## Security Considerations

### Threat Model

1. **Arithmetic Overflows**: Prevented by explicit bounds checking before every operation
2. **Division by Zero**: Explicit check before division, TRAP on zero divisor
3. **Negative Square Root**: Explicit check, TRAP on negative input
4. **Precision Loss**: Explicit scale checks for DQA conversion
5. **Canonical Form Violation**: All operations must return canonical form

### Attack Vectors

| Vector | Mitigation |
|--------|------------|
| Malicious scale values | Scale limited to 0-36, enforced at boundaries |
| Giant mantissa amplification | MAX_DECIMAL_MANTISSA bounds on all operations |
| Reentrancy | DECIMAL operations are atomic (single function call) |
| Front-running | Deterministic ordering eliminates race conditions |

### Consensus Security

- All nodes must produce identical results for identical inputs
- RoundHalfEven required for financial calculations (prevents manipulation)
- Canonical form ensures consistent Merkle tree hashes

## Adversarial Review

### Review History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-03-14 | Initial draft |
| 1.1 | 2026-03-15 | Fixed RoundHalfEven negative handling, added Newton-Raphson convergence |

### Known Issues

| Issue ID | Severity | Description | Status |
|----------|----------|-------------|--------|
| D1 | Medium | Newton-Raphson iteration limit (40) lacks formal convergence proof for extreme scales (10^36 magnitude) | Open |
| D2 | Low | Gas model not validated against real-world benchmarks | Open |
| HIGH-1 | High | Precision growth rule (+6 per operation) may break expected algebraic identities (e.g., (a×b)/b ≠ a in edge cases due to rounding) | Open |
| MED-2 | Medium | SQRT probe gap: no probe entry tests SQRT that produces perfect square with trailing zeros requiring canonicalization (e.g., √0.0400) | Open |
| MED-3 | Medium | DQA↔DECIMAL round-trip conversion may have unbounded drift due to scale differences | Open |
| MED-6 | Medium | Probe entry 48 tests max scale DQA but doesn't verify canonicalization behavior | Open |

## Alternatives Considered

### Option 1: Use DQA with Higher Scale (Rejected)

**Approach**: Extend DQA (RFC-0105) to support scale 0-36

**Pros:**
- No new type needed
- Simpler codebase

**Cons:**
- DQA uses i64, insufficient for scale 36 (would require 128-bit intermediate)
- Breaking change to DQA semantics

**Decision**: DECIMAL uses i128 to support full 36-digit precision

### Option 2: Use Arbitrary-Precision Decimal (Rejected)

**Approach**: Support arbitrary scale beyond 36

**Pros:**
- Unlimited precision

**Cons:**
- Gas costs become unpredictable
- No practical benefit (36 digits exceeds all known requirements)
- Implementation complexity

**Decision**: Fixed 36-digit limit provides sufficient precision with predictable gas

### Option 3: Use IEEE 754 Decimal128 (Rejected)

**Approach**: Adopt IEEE 754 decimal128 format

**Pros:**
- Industry standard
- Hardware support on some platforms

**Cons:**
- Not deterministic across implementations
- Different encoding than other numeric types
- Complex serialization

**Decision**: Custom i128 + scale format maintains consistency with DQA/BIGINT

## Version History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-03-14 | TBD | Initial draft extracted from RFC-0106 |
| 1.1 | 2026-03-15 | TBD | Fixed RoundHalfEven algorithm, added SQRT convergence |
| 1.2 | 2026-03-16 | TBD | Fixed critical issues C1-C17 from adversarial review |
| 1.3 | 2026-03-16 | TBD | Fixed high-severity issues H1-H12 from adversarial review |
| 1.4 | 2026-03-16 | TBD | Fixed critical issues C1-C4 and H5 from adversarial review |
| 1.5 | 2026-03-16 | TBD | Added locale specification, expanded gas proof, fixed POW10 31-36 |
| 1.6 | 2026-03-16 | TBD | Fixed POW10 31-36, probe format (32-byte), DIV rounding, CMP note, gas proof, string edge cases |
| 1.7 | 2026-03-16 | TBD | Fixed remaining 24→32-byte references, added Merkle root verification, POW10 verified |
| 1.8 | 2026-03-16 | TBD | SQRT convergence proof, DIV rounding semantics, probe sync, VM canonicalization, ZK note |
| 1.9 | 2026-03-16 | TBD | Production hardening: range invariant, safe alignment, 256-bit mul, division precision, DQA conversion quantum |
| 1.14 | 2026-03-17 | TBD | Fixed HIGH-1, HIGH-3, HIGH-4, MED-2, MED-3, MED-4, MED-6 from adversarial review |
| 1.16 | 2026-03-17 | TBD | Fixed BUG-1 (MUL sign-aware rounding), BUG-2 (DIV unsafe cast), BUG-3 (BIGINT→DECIMAL), BUG-4 (probe 24b), BUG-5 (config hash), BUG-6 (SQRT off-by-one), BUG-7 (ROUND encoding) |
| 1.17 | 2026-03-17 | TBD | Fixed Python SQRT (BUG-6 off-by-one), Python DIV (canonicalize), added config hash script, fixed probe 25 description, updated Known Issues |

## Compatibility

### Backward Compatibility

- DECIMAL v1.x is backward compatible within draft status
- Breaking changes may occur before Accepted status

### Forward Compatibility

- No forward compatibility guarantees for draft RFCs

### Interoperability

| From | To | Supported | Notes |
|------|-----|-----------|-------|
| DECIMAL | DQA | ✅ | Requires scale ≤ 18 |
| DQA | DECIMAL | ✅ | Always valid |
| DECIMAL | BIGINT | ✅ | Requires scale = 0 |
| BIGINT | DECIMAL | ✅ | Always valid |
| DECIMAL | String | ✅ | Full round-trip |
| DECIMAL | DFP | ❌ | Not recommended (precision loss) |

## Related Use Cases

- **UC-XXX**: High-Precision Financial Derivatives (future)
- **UC-XXX**: Regulatory Reporting with Extended Precision (future)

## Future Work

1. **ZK Circuit Commitments**: Add ZK proofs for DECIMAL operations (post-v1)
2. **SIMD Optimization**: Vectorized operations for batch processing
3. **Hardware Acceleration**: Leverage dedicated decimal arithmetic units where available
4. **Decimal128 Interoperability**: Optional conversion to IEEE 754 format

## Spec Version & Replay Pinning

### numeric_spec_version

DECIMAL uses the unified numeric spec version defined in RFC-0110:

```rust
/// Numeric tower unified specification version (DFP, DQA, DECIMAL, BigInt)
const NUMERIC_SPEC_VERSION: u32 = 1;
```

> **Note:** DECIMAL was added after RFC-0110. The unified NUMERIC_SPEC_VERSION applies to all numeric types including DECIMAL.

### Block Header Integration (normative)

As defined in RFC-0110 §Spec Version & Replay Pinning:

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

### Replay Rules (mandatory)

Per RFC-0110, all DECIMAL operations inside a block MUST use the pinned algorithm version from the block header.

> **Note:** This aligns with RFC-0104's DFP probe schedule (every 100,000 blocks).

## References

- RFC-0104: Deterministic Floating-Point
- RFC-0105: Deterministic Quant Arithmetic
- RFC-0110: Deterministic BIGINT
- RFC-0106: Deterministic Numeric Tower (archived)
