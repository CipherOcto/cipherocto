# RFC-0136 (Numeric/Math): Deterministic DFP-BigInt Conversion

## Status

**Version:** 1.0 (Draft)
**Status:** Draft
**Depends On:** RFC-0104 (DFP), RFC-0110 (BIGINT), RFC-0132 (BigIntWithScale type)
**Category:** Numeric/Math

## Summary

This RFC specifies the bidirectional conversion between DFP (RFC-0104, 113-bit mantissa floating-point) and BigInt (RFC-0110, arbitrary-precision integer up to 4096 bits). This conversion fills the remaining gap in the CipherOcto Numeric Tower's conversion matrix — all other type pairs (DFP↔DQA, DQA↔BigInt, BigInt↔Decimal) are covered by existing RFCs.

The DFP→BigInt direction is **lossless** for all Normal DFP values: every finite non-zero DFP value has an exact decimal representation as `BigIntWithScale`. The BigInt→DFP direction provides both a **checked exact** conversion (fails if precision would be lost) and a **rounded** conversion (always succeeds, applies Round-Half-Even).

**Round-trip guarantee:** For all DFP Normal values, `DFP → BigIntWithScale → DFP_exact` is the identity function.

## Motivation

### Problem Statement

The CipherOcto Numeric Tower defines conversions between its four numeric domains:

```
DFP (RFC-0104) ── DQA (RFC-0105) ── BigInt (RFC-0110)
                  │                  │
                  └── Decimal ───────┘
```

The current conversion matrix covers:

| From\To     | DFP     | DQA      | BigInt   | Decimal  |
| ----------- | ------- | -------- | -------- | -------- |
| **DFP**     | —       | RFC-0124 | **GAP**  | —        |
| **DQA**     | —       | —        | RFC-0132 | RFC-0135 |
| **BigInt**  | **GAP** | RFC-0131 | —        | RFC-0133 |
| **Decimal** | —       | RFC-0135 | RFC-0134 | —        |

The DFP↔BigInt gap forces callers to route through DFP→DQA→BigInt (two conversions, intermediate precision loss to 18 decimal digits). This is unacceptable for:

1. **Cryptographic hash computations** on floating-point-derived values (need exact integer representation)
2. **Financial calculations** requiring full DFP precision to be preserved in BigInt domain
3. **Cross-type arithmetic** where DFP values must participate in BigInt operations without precision loss
4. **Round-trip integrity** for Merkle tree inclusion proofs

### Why Not Route Through DQA?

| Path                  | Precision   | Round-trip | Overhead      |
| --------------------- | ----------- | ---------- | ------------- |
| DFP → DQA → BigInt    | 18 decimals | Lossy      | 2 conversions |
| DFP → BigInt (direct) | Exact       | Lossless   | 1 conversion  |

DQA's i64 mantissa with 0-18 scale cannot represent DFP's full 113-bit mantissa (~34 decimal digits). Any DFP value requiring more than 18 significant decimal digits would lose precision through the DQA path.

### Key Insight: Every Finite DFP Has an Exact Decimal Representation

A DFP Normal value is `m × 2^e` where `m` is a 113-bit unsigned integer and `e ∈ [-1074, 1023]`.

- When `e ≥ 0`: the value is an integer `m × 2^e` — trivially representable as BigInt with scale 0.
- When `e = -k` (negative): the value is `m / 2^k = m × 5^k / 10^k` — an exact decimal with at most `k` decimal places, representable as `BigIntWithScale { value: m × 5^k, scale: k }`.

The maximum `k` is 1074 (when `e = -1074`), so the maximum scale is 1074 and the maximum BigInt value is `(2^113 - 1) × 5^1074`. This fits well within BigInt's 4096-bit limit (the product requires at most 113 + ⌈1074 × log₂5⌉ ≈ 113 + 2498 = 2611 bits).

## Specification

### Data Structures

```rust
/// Error variants for DFP↔BigInt conversion
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DfpBigIntError {
    /// DFP value is NaN — no meaningful integer representation.
    /// NaN has no mapping to any BigInt value.
    NotANumber,

    /// DFP value is Infinity (positive or negative).
    /// Infinity cannot be represented as a finite integer.
    Infinite,

    /// DFP value is negative zero (-0.0).
    /// Per design decision, negative zero is treated as a sign error
    /// to prevent silent sign-loss in Merkle tree computations.
    /// Callers who want to treat -0.0 as +0 should check before conversion.
    NegativeZero,

    /// The BigIntWithScale scale exceeds the maximum representable scale.
    /// Maximum scale is 1074 (= |DFP_MIN_EXPONENT|).
    ScaleOutOfRange {
        requested: u32,
        max_scale: u32,
    },

    /// The BigInt value is too large to represent as a DFP Normal value.
    /// This occurs when the BigInt requires more than 113 bits of mantissa
    /// precision after accounting for the scale factor.
    Overflow {
        bit_length: usize,
        max_bits: usize,  // 113
    },
}

/// Result of exact BigInt→DFP conversion
pub type DfpFromBigIntExactResult = Result<Dfp, DfpBigIntError>;

/// Result of rounded BigInt→DFP conversion
pub struct RoundingInfo {
    /// Whether the conversion was exact (no rounding needed)
    pub exact: bool,
    /// Units in the Last Place error: 0 if exact, 1 if rounded
    pub ulp_error: u8,
}

/// Result of rounded BigInt→DFP conversion
pub struct DfpFromBigIntRoundedResult {
    /// The resulting DFP value
    pub value: Dfp,
    /// Rounding metadata
    pub rounding: RoundingInfo,
}
```

### Function Signatures

```rust
/// Convert DFP to BigIntWithScale (lossless).
///
/// Every finite non-zero DFP Normal value has an exact decimal representation.
/// Returns an error for NaN, Infinity, and negative zero.
///
/// # Arguments
/// * `dfp` - The DFP value to convert
///
/// # Returns
/// * `Ok(BigIntWithScale)` for Normal and positive Zero values
/// * `Err(NotANumber)` for NaN
/// * `Err(Infinite)` for Infinity
/// * `Err(NegativeZero)` for -0.0
pub fn dfp_to_bigint(dfp: &Dfp) -> Result<BigIntWithScale, DfpBigIntError>

/// Convert BigIntWithScale to DFP (checked exact).
///
/// Returns an error if the conversion would lose precision — i.e., if
/// `value / 5^scale` is not an integer (the decimal value cannot be
/// exactly represented as `mantissa × 2^exponent`).
///
/// # Arguments
/// * `bws` - The BigIntWithScale value to convert
///
/// # Returns
/// * `Ok(Dfp)` if the value can be represented exactly
/// * `Err(ExactnessLost)` if precision would be lost (caller should use `bigint_to_dfp_rounded`)
/// * `Err(Overflow)` if the BigInt magnitude exceeds what DFP can represent
pub fn bigint_to_dfp_exact(bws: &BigIntWithScale) -> DfpFromBigIntExactResult

/// Convert BigIntWithScale to DFP (rounded).
///
/// Always succeeds for finite values. Applies Round-Half-Even (RNE) rounding
/// to fit the value into DFP's 113-bit mantissa. Returns the DFP value
/// plus rounding metadata.
///
/// # Arguments
/// * `bws` - The BigIntWithScale value to convert
///
/// # Returns
/// * `DfpFromBigIntRoundedResult` with the DFP value and rounding info
/// * `Err(Overflow)` only if the BigInt magnitude exceeds DFP_MAX_MANTISSA × 2^1023
pub fn bigint_to_dfp_rounded(bws: &BigIntWithScale) -> Result<DfpFromBigIntRoundedResult, DfpBigIntError>
```

### Algorithm: DFP → BigIntWithScale

```
DFP_TO_BIGINT(dfp: Dfp) -> Result<BigIntWithScale, DfpBigIntError>

INPUT:  dfp (Dfp value)
OUTPUT: BigIntWithScale { value: BigInt, scale: u32 } or error

STEPS:

1. CLASSIFY
   Match dfp.class:
     NaN      → return Err(NotANumber)
     Infinity → return Err(Infinite)
     Zero     → if dfp.sign == true:
                   return Err(NegativeZero)
                else:
                   return Ok(BigIntWithScale {
                     value: BigInt::ZERO,
                     scale: 0,
                   })
     Normal   → continue to step 2

2. DECOMPOSE (Normal case)
   Let m = dfp.mantissa   (u128, 113-bit odd integer per RFC-0104)
   Let e = dfp.exponent   (i32, range [-1074, 1023])
   Let sign = dfp.sign    (bool)

3. BRANCH ON EXPONENT SIGN

   Case A: e >= 0 (value is an integer)
     // m × 2^e is already an integer
     // BigInt::from(m) shifted left by e bits
     value = BigInt::from_u128(m) << e   // left shift by e bits
     return Ok(BigIntWithScale { value, scale: 0 })

   Case B: e < 0, let k = -e (value has fractional part)
     // m × 2^(-k) = m × 5^k / 10^k
     // Compute value = m × 5^k (exact, no overflow within 4096 bits)
     // Scale = k (number of decimal places)

     k = (-e) as u32

     // Check scale fits in u32 (max k = 1074, well within u32 range)
     // No check needed: DFP_MIN_EXPONENT = -1074, so k ≤ 1074

     // Compute 5^k using BigInt exponentiation
     five_pow_k = BigInt::from_u64(5).pow(k)

     // Multiply mantissa by 5^k
     // |m| ≤ 2^113 - 1, |5^k| ≤ 5^1074 ≈ 2^2498
     // Product ≤ 2^113 × 2^2498 = 2^2611 < 2^4096 ✓
     value = BigInt::from_u128(m) × five_pow_k

     // Apply sign
     if sign:
       value = -value   // BigInt negation

     return Ok(BigIntWithScale { value, scale: k })
```

**Proof of 4096-bit sufficiency:**

The maximum BigInt produced is when `m = 2^113 - 1` and `k = 1074`:

```
bit_length(m × 5^1074) ≤ 113 + ⌈1074 × log₂(5)⌉
                       = 113 + ⌈1074 × 2.3219...⌉
                       = 113 + 2498
                       = 2611 bits
                       < 4096 bits ✓
```

### Algorithm: BigIntWithScale → DFP (Exact)

```
BIGINT_TO_DFP_EXACT(bws: BigIntWithScale) -> Result<Dfp, DfpBigIntError>

INPUT:  bws (BigIntWithScale { value: BigInt, scale: u32 })
OUTPUT: Dfp (Normal) or error

STEPS:

1. VALIDATE_SCALE
   If bws.scale > 1074:
     return Err(ScaleOutOfRange { requested: bws.scale, max_scale: 1074 })

2. HANDLE_ZERO
   If bws.value == BigInt::ZERO:
     return Ok(Dfp::zero(false))

3. EXTRACT_MAGNITUDE_AND_SIGN
   sign = bws.value.is_negative()
   magnitude = bws.value.abs()   // BigInt, positive

4. DECOMPOSE_SCALE
   Let s = bws.scale  (u32)

   // The decimal value is magnitude / 10^s
   // = magnitude / (2^s × 5^s)
   // For exact DFP representation, we need magnitude / 5^s to be an integer
   // (so the value = integer × 2^(-s) has no 5-factors in the mantissa)

5. EXACTNESS_CHECK
   // Divide magnitude by 5^s; if remainder is non-zero, exact conversion is impossible
   five_pow_s = BigInt::from_u64(5).pow(s)
   (quotient, remainder) = magnitude.div_rem(five_pow_s)

   If remainder != BigInt::ZERO:
     return Err(DfpBigIntError::ExactnessLost)
     // Caller should use bigint_to_dfp_rounded instead

6. CONSTRUCT_DFP
   // Now we know: decimal_value = quotient × 2^(-s)
   // = quotient × 2^(-s), where quotient is an integer with no 5-factors

   // Normalize quotient to odd mantissa (RFC-0104 canonical form)
   // DFP requires odd mantissa with adjusted exponent
   q = quotient
   exp = -(s as i32)

   // Strip trailing zero bits from q (equivalent to dividing by 2 until odd)
   While q is even AND q != 0:
     q = q >> 1
     exp = exp + 1

   // q is now odd (or zero, handled in step 2)
   // Check mantissa fits in 113 bits
   If q.bit_length() > 113:
     return Err(Overflow { bit_length: q.bit_length(), max_bits: 113 })

   // Check exponent in range
   If exp < -1074 OR exp > 1023:
     return Err(Overflow { bit_length: q.bit_length(), max_bits: 113 })

   mantissa = q.to_u128()   // Safe: bit_length ≤ 113
   return Ok(Dfp::new(mantissa, exp, sign))
```

### Algorithm: BigIntWithScale → DFP (Rounded)

```
BIGINT_TO_DFP_ROUNDED(bws: BigIntWithScale) -> Result<DfpFromBigIntRoundedResult, DfpBigIntError>

INPUT:  bws (BigIntWithScale { value: BigInt, scale: u32 })
OUTPUT: DfpFromBigIntRoundedResult or error

STEPS:

1. VALIDATE_SCALE
   If bws.scale > 1074:
     return Err(ScaleOutOfRange { requested: bws.scale, max_scale: 1074 })

2. HANDLE_ZERO
   If bws.value == BigInt::ZERO:
     return Ok(DfpFromBigIntRoundedResult {
       value: Dfp::zero(false),
       rounding: RoundingInfo { exact: true, ulp_error: 0 },
     })

3. EXTRACT_MAGNITUDE_AND_SIGN
   sign = bws.value.is_negative()
   magnitude = bws.value.abs()

4. COMPUTE_BINARY_REPRESENTATION
   // Target: represent magnitude / 10^s = magnitude / (2^s × 5^s) as mantissa × 2^exp

   s = bws.scale
   five_pow_s = BigInt::from_u64(5).pow(s)

   // Divide magnitude by 5^s to get the binary mantissa component
   // quotient = floor(magnitude / 5^s), remainder = magnitude % 5^s
   (quotient, remainder) = magnitude.div_rem(five_pow_s)

   // exp = -s (we need to multiply quotient by 2^(-s))
   exp = -(s as i32)

   // If remainder is non-zero, we need to track it for rounding
   is_exact = (remainder == BigInt::ZERO)

5. NORMALIZE_TO_113_BITS (with rounding)
   // We need exactly 113 bits of mantissa with RNE rounding.
   // Use guard bit (bit 113), round bit (bit 114), and sticky bit (bits 115+)

   bl = quotient.bit_length()

   If bl <= 113:
     // Quotient fits exactly — no rounding needed
     // But we need odd mantissa, so normalize
     q = quotient
     While q is even AND q != 0:
       q = q >> 1
       exp = exp + 1
     mantissa = q.to_u128()
     exact = is_exact
   Else:
     // Quotient exceeds 113 bits — round with RNE
     shift = bl - 113

     // Check if rounding information exists in the discarded bits
     // We need: guard_bit, round_bit, sticky_bit
     // For the shifted-away bits of quotient:
     guard_bit = quotient.bit(shift - 1)     // First discarded bit
     round_bit = if shift >= 2 { quotient.bit(shift - 2) } else { false }
     sticky = if shift >= 3 { quotient.any_bits_below(shift - 2) } else { false }
     // Also include remainder in sticky if non-zero
     sticky = sticky OR !is_exact

     // Shift quotient down to 113 bits
     q = quotient >> shift
     exp = exp + (shift as i32)

     // Apply RNE
     // Round up if: guard=1 AND (round=1 OR sticky=1 OR q is odd)
     round_up = guard_bit AND (round_bit OR sticky OR (q & 1 == 1))

     If round_up:
       q = q + 1
       // If increment caused overflow (q now has 114 bits), shift right
       If q.bit_length() == 114:
         q = q >> 1
         exp = exp + 1

     // Strip trailing zeros to get odd mantissa (canonical form)
     While q is even AND q != 0:
       q = q >> 1
       exp = exp + 1

     mantissa = q.to_u128()
     exact = false  // We rounded, so not exact

6. CHECK_RANGE
   If exp < -1074:
     // Underflow: value rounds to zero
     return Ok(DfpFromBigIntRoundedResult {
       value: Dfp::zero(sign),
       rounding: RoundingInfo { exact: false, ulp_error: 1 },
     })

   If exp > 1023:
     // Overflow: value exceeds DFP representable range
     return Err(Overflow { bit_length: bl, max_bits: 113 })

7. RETURN
   return Ok(DfpFromBigIntRoundedResult {
     value: Dfp::new(mantissa, exp, sign),
     rounding: RoundingInfo {
       exact,
       ulp_error: if exact { 0 } else { 1 },
     },
   })
```

### Error Taxonomy

| Error             | Trigger                        | DFP→BigInt | BigInt→DFP Exact | BigInt→DFP Rounded |
| ----------------- | ------------------------------ | :--------: | :--------------: | :----------------: |
| `NotANumber`      | DFP NaN class                  |     ✓      |                  |                    |
| `Infinite`        | DFP Infinity class             |     ✓      |                  |                    |
| `NegativeZero`    | DFP Zero class, sign=true      |     ✓      |                  |                    |
| `ScaleOutOfRange` | scale > 1074                   |            |        ✓         |         ✓          |
| `Overflow`        | mantissa > 113 bits or exp OOB |            |        ✓         |         ✓          |
| `ExactnessLost`   | value/5^scale not integer      |            |        ✓         |                    |

### Round-Trip Guarantee

**Theorem:** For all DFP Normal values `v`, the composition `bigint_to_dfp_exact(dfp_to_bigint(v)?)` returns `Ok(v')` where `v' == v` (bit-identical).

**Proof sketch:**

1. `dfp_to_bigint(v)` produces `BigIntWithScale { value, scale }` where:
   - If `v.exponent ≥ 0`: `value = v.mantissa << v.exponent`, `scale = 0`
   - If `v.exponent < 0`: `value = v.mantissa × 5^k`, `scale = k` where `k = -v.exponent`

2. `bigint_to_dfp_exact` recovers:
   - Divides `value` by `5^scale` → quotient = `v.mantissa` (exact, since `value = v.mantissa × 5^k`)
   - Sets `exp = -scale = v.exponent`
   - Normalizes to odd mantissa — but `v.mantissa` is already odd (RFC-0104 invariant), so no shift occurs
   - Recovers `v.sign` from BigInt sign

3. Therefore `v' = Dfp { mantissa: v.mantissa, exponent: v.exponent, sign: v.sign, class: Normal } = v` ∎

## Test Vectors

### V1: DFP→BigInt — Positive Zero

```
Input:    Dfp { class: Zero, sign: false, mantissa: 0, exponent: 0 }
Output:   Ok(BigIntWithScale { value: BigInt(0), scale: 0 })
```

### V2: DFP→BigInt — Negative Zero

```
Input:    Dfp { class: Zero, sign: true, mantissa: 0, exponent: 0 }
Output:   Err(NegativeZero)
```

### V3: DFP→BigInt — NaN

```
Input:    Dfp { class: NaN, sign: false, mantissa: 0, exponent: 0 }
Output:   Err(NotANumber)
```

### V4: DFP→BigInt — Infinity

```
Input:    Dfp { class: Infinity, sign: false, mantissa: 0, exponent: 0 }
Output:   Err(Infinite)
```

### V5: DFP→BigInt — Integer (exponent ≥ 0)

```
Input:    Dfp { class: Normal, sign: false, mantissa: 3, exponent: 1 }
          // 3 × 2^1 = 6.0
Output:   Ok(BigIntWithScale { value: BigInt(6), scale: 0 })
```

### V6: DFP→BigInt — Large Integer (exponent = 1023)

```
Input:    Dfp { class: Normal, sign: false, mantissa: (2^113 - 1), exponent: 1023 }
          // (2^113 - 1) × 2^1023 — maximum DFP value
Output:   Ok(BigIntWithScale {
            value: BigInt((2^113 - 1) << 1023),  // 1136-bit integer
            scale: 0
          })
```

### V7: DFP→BigInt — Fractional (exponent < 0)

```
Input:    Dfp { class: Normal, sign: false, mantissa: 1, exponent: -1 }
          // 1 × 2^(-1) = 0.5 = 5/10
Output:   Ok(BigIntWithScale { value: BigInt(5), scale: 1 })
```

### V8: DFP→BigInt — Fractional (exponent = -3)

```
Input:    Dfp { class: Normal, sign: false, mantissa: 7, exponent: -3 }
          // 7 × 2^(-3) = 7/8 = 875/1000
Output:   Ok(BigIntWithScale { value: BigInt(875), scale: 3 })
```

### V9: DFP→BigInt — Negative Fractional

```
Input:    Dfp { class: Normal, sign: true, mantissa: 3, exponent: -2 }
          // -3 × 2^(-2) = -0.75 = -75/100
Output:   Ok(BigIntWithScale { value: BigInt(-75), scale: 2 })
```

### V10: DFP→BigInt — Minimum Exponent

```
Input:    Dfp { class: Normal, sign: false, mantissa: 1, exponent: -1074 }
          // 1 × 2^(-1074) = 5^1074 / 10^1074
Output:   Ok(BigIntWithScale {
            value: BigInt(5^1074),  // ~2498 bits
            scale: 1074
          })
```

### V11: BigInt→DFP Exact — Simple Integer

```
Input:    BigIntWithScale { value: BigInt(6), scale: 0 }
Output:   Ok(Dfp { class: Normal, sign: false, mantissa: 3, exponent: 1 })
          // 6 = 3 × 2^1 (odd mantissa)
```

### V12: BigInt→DFP Exact — Fractional

```
Input:    BigIntWithScale { value: BigInt(5), scale: 1 }
          // 5 / 10^1 = 0.5 = 1 × 2^(-1)
Output:   Ok(Dfp { class: Normal, sign: false, mantissa: 1, exponent: -1 })
```

### V13: BigInt→DFP Exact — Inexact (fails)

```
Input:    BigIntWithScale { value: BigInt(1), scale: 1 }
          // 1 / 10^1 = 0.1 — cannot represent exactly as mantissa × 2^exp
          // Because 1/5 = 0.2 is not an integer, so 1 / 5^1 has remainder
Output:   Err(ExactnessLost)
```

### V14: BigInt→DFP Rounded — Inexact Value

```
Input:    BigIntWithScale { value: BigInt(1), scale: 1 }
          // 0.1 — rounds to nearest DFP
Output:   Ok(DfpFromBigIntRoundedResult {
            value: Dfp { class: Normal, sign: false,
                         mantissa: 0xCCCC_CCCC_CCCC_CCCC_CCCC_CCCC_CCCC_CC,
                         exponent: -120 },
                         // Closest DFP approximation of 0.1
            rounding: RoundingInfo { exact: false, ulp_error: 1 },
          })
```

### V15: BigInt→DFP Exact — Negative Value

```
Input:    BigIntWithScale { value: BigInt(-75), scale: 2 }
          // -75 / 100 = -0.75 = -3 × 2^(-2)
Output:   Ok(Dfp { class: Normal, sign: true, mantissa: 3, exponent: -2 })
```

### V16: BigInt→DFP — Zero

```
Input:    BigIntWithScale { value: BigInt(0), scale: 0 }
Output:   Ok(Dfp { class: Zero, sign: false, mantissa: 0, exponent: 0 })
```

### V17: Round-Trip — V5 Round-Trip

```
Step 1:   Dfp { mantissa: 3, exponent: 1, sign: false }
          → dfp_to_bigint → BigIntWithScale { value: BigInt(6), scale: 0 }
Step 2:   BigIntWithScale { value: BigInt(6), scale: 0 }
          → bigint_to_dfp_exact → Ok(Dfp { mantissa: 3, exponent: 1, sign: false })
Verify:   Output == Input ✓
```

### V18: Round-Trip — V7 Round-Trip

```
Step 1:   Dfp { mantissa: 1, exponent: -1, sign: false }
          → dfp_to_bigint → BigIntWithScale { value: BigInt(5), scale: 1 }
Step 2:   BigIntWithScale { value: BigInt(5), scale: 1 }
          → bigint_to_dfp_exact → Ok(Dfp { mantissa: 1, exponent: -1, sign: false })
Verify:   Output == Input ✓
```

### V19: Round-Trip — V9 Round-Trip (Negative)

```
Step 1:   Dfp { mantissa: 3, exponent: -2, sign: true }
          → dfp_to_bigint → BigIntWithScale { value: BigInt(-75), scale: 2 }
Step 2:   BigIntWithScale { value: BigInt(-75), scale: 2 }
          → bigint_to_dfp_exact → Ok(Dfp { mantissa: 3, exponent: -2, sign: true })
Verify:   Output == Input ✓
```

### V20: Round-Trip — V10 Round-Trip (Minimum Exponent)

```
Step 1:   Dfp { mantissa: 1, exponent: -1074, sign: false }
          → dfp_to_bigint → BigIntWithScale { value: BigInt(5^1074), scale: 1074 }
Step 2:   BigIntWithScale { value: BigInt(5^1074), scale: 1074 }
          → bigint_to_dfp_exact → Ok(Dfp { mantissa: 1, exponent: -1074, sign: false })
Verify:   Output == Input ✓
```

### V21: BigInt→DFP Rounded — Overflow

```
Input:    BigIntWithScale {
            value: BigInt(2^200),  // 200-bit integer
            scale: 0
          }
          // 2^200 needs 200 bits of mantissa after normalization
          // (actually, normalized: mantissa=1, exponent=200, but exp > 1023)
Output:   Err(Overflow { bit_length: 201, max_bits: 113 })
```

### V22: BigInt→DFP — Scale Out of Range

```
Input:    BigIntWithScale { value: BigInt(1), scale: 2000 }
Output:   Err(ScaleOutOfRange { requested: 2000, max_scale: 1074 })
```

## Gas Model

### DFP → BigIntWithScale

| Case              | Operations                       | Gas Estimate        |
| ----------------- | -------------------------------- | ------------------- |
| Zero/NaN/Inf      | Class dispatch                   | 1                   |
| exp ≥ 0           | BigInt::from + shift_left        | 2 + e/64            |
| exp < 0 (small k) | BigInt::from + 5^k + multiply    | 2 + k × 3           |
| exp < 0 (k=1074)  | BigInt::from + 5^1074 + multiply | 2 + 1074 × 3 ≈ 3224 |

The worst case (k=1074) involves computing 5^1074 as a ~2500-bit BigInt and multiplying by the 113-bit mantissa. Both operations are bounded by BigInt's O(n²) schoolbook multiplication where n ≤ 4096/64 = 64 limbs.

**Maximum gas:** 5000 (covers worst-case 5^1074 computation)

### BigIntWithScale → DFP (Exact)

| Case           | Operations                | Gas Estimate  |
| -------------- | ------------------------- | ------------- |
| Zero           | Check                     | 1             |
| Normal (exact) | 5^s + div_rem + normalize | 2 + s × 3 + n |

**Maximum gas:** 5000 (mirrors DFP→BigInt worst case)

### BigIntWithScale → DFP (Rounded)

| Case             | Operations                      | Gas Estimate  |
| ---------------- | ------------------------------- | ------------- |
| Zero             | Check                           | 1             |
| Fits in 113 bits | 5^s + div_rem + normalize       | 2 + s × 3     |
| Needs rounding   | 5^s + div_rem + normalize + RNE | 2 + s × 3 + 5 |

**Maximum gas:** 5000 (same bound as exact path, RNE adds negligible cost)

## Cross-RFC Conformance

### Dependencies

| RFC  | What This RFC Uses                                                            |
| ---- | ----------------------------------------------------------------------------- |
| 0104 | Dfp struct, DfpClass, mantissa (u128), exponent (i32), odd-mantissa invariant |
| 0110 | BigInt struct, canonical form, limbs, shift/mul/div_rem operations            |
| 0132 | BigIntWithScale type definition                                               |

### Conversion Matrix Completeness

With this RFC, the Numeric Tower conversion matrix is complete:

| From\To     | DFP          | DQA      | BigInt       | Decimal            |
| ----------- | ------------ | -------- | ------------ | ------------------ |
| **DFP**     | —            | RFC-0124 | **RFC-0136** | RFC-0124→0132→0134 |
| **DQA**     | —            | —        | RFC-0132     | RFC-0135           |
| **BigInt**  | **RFC-0136** | RFC-0131 | —            | RFC-0133           |
| **Decimal** | —            | RFC-0135 | RFC-0134     | —                  |

### BigIntWithScale Contract

This RFC uses `BigIntWithScale` as defined in RFC-0132:

```rust
pub struct BigIntWithScale {
    pub value: BigInt,
    pub scale: u8,  // RFC-0132 limits to 0-18 for DQA; this RFC extends to 0-1074
}
```

**IMPORTANT:** RFC-0132 defines `BigIntWithScale.scale` as `u8` (range 0-255), sufficient for DQA's 0-18 scale range and this RFC's 0-1074 range. However, `u8` cannot hold values above 255. Since this RFC requires scales up to 1074, implementations MUST use `u32` for the scale field in `BigIntWithScale` when used with DFP conversions. This is a widening change from `u8` to `u32` — it is backward-compatible because all existing DQA scales (0-18) fit in both types.

**Recommendation:** RFC-0132 should be amended to widen `BigIntWithScale.scale` from `u8` to `u32` to accommodate DFP's larger scale range. Until then, this RFC defines a local `DfpBigIntWithScale` type alias or wrapper that uses `u32`.

## Determinism Rules

1. **All intermediate computations MUST use BigInt arithmetic** — no native floating-point operations are permitted in any conversion path.

2. **The 5^k computation MUST use BigInt exponentiation** — iterative squaring is recommended for gas efficiency but not mandated; the result must be bit-identical regardless of algorithm.

3. **RNE rounding in the rounded path MUST follow RFC-0104's RNE specification** — guard bit, round bit, sticky bit semantics with tie-breaking to even.

4. **The normalization step (stripping trailing zero bits) MUST be applied** in both directions to ensure the odd-mantissa invariant of RFC-0104.

5. **Sign handling is explicit:** negative zero from DFP is an error (not silently converted to positive zero); BigInt negative values preserve their sign through conversion.

## Implementation Checklist

- [ ] `dfp_to_bigint` with all five DfpClass cases
- [ ] `bigint_to_dfp_exact` with exactness check
- [ ] `bigint_to_dfp_rounded` with RNE
- [ ] `BigInt::pow` for 5^k computation (or reuse from RFC-0110)
- [ ] `BigInt::bit_length` (reuse from RFC-0110)
- [ ] `BigInt::div_rem` (reuse from RFC-0110)
- [ ] Compile-time assertion: `DfpBigIntError` is `PartialEq + Eq`
- [ ] All 22 test vectors passing
- [ ] Fuzz: random DFP Normal values round-trip through BigIntWithScale
- [ ] Fuzz: random BigIntWithScale values round-trip through DFP (when exact)
- [ ] Gas benchmarks for worst-case paths

## Version History

| Version | Date       | Changes       |
| ------- | ---------- | ------------- |
| 1.0     | 2026-04-02 | Initial draft |
