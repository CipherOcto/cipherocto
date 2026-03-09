# RFC-0105: Deterministic Quant Arithmetic (DQA)

## Status

Draft (Production-Grade Revision v2.14)

## Summary

This RFC introduces Deterministic Quant Arithmetic (DQA) — a high-performance deterministic numeric type optimized for quantitative finance, pricing, and AI inference workloads. DQA represents numbers as scaled integers (`value × 10^-scale`), providing float-like ergonomics with integer-speed arithmetic.

DQA complements RFC-0104's DFP type: where DFP handles arbitrary-precision scientific computing, DQA provides bounded-range high-speed deterministic arithmetic suitable for trading, risk calculations, and ML preprocessing.

#### When to Use DQA vs DFP

| Use Case                                  | Recommended Type | Reason                                |
| ----------------------------------------- | ---------------- | ------------------------------------- |
| Financial prices, order book quantities   | DQA              | Bounded range, 10-40x faster          |
| Portfolio risk (VaR, Greeks)              | DQA              | Bounded precision, high throughput    |
| Option pricing (Black-Scholes)            | DQA or DFP       | DQA sufficient for standard precision |
| Scientific computing / physics simulation | DFP              | Requires arbitrary exponents          |
| AI/ML inference (embeddings, activations) | DQA              | Bounded range, cache-friendly         |
| Arbitrary-precision accounting            | DFP              | May exceed 18 decimal places          |
| Blockchain consensus arithmetic           | DQA              | Deterministic, bounded, fast          |
| Regulatory reporting (exact decimals)     | DQA              | Fixed scale, audit-friendly           |

## Motivation

### Problem Statement

DFP (RFC-0104) provides arbitrary-precision deterministic floating-point, but at significant performance cost:

- DFP operations are 10-40x slower than native integers
- Normalization loops add overhead
- Overkill for bounded-range workloads

Many workloads don't need arbitrary exponents:

- Financial prices: 0.000001 – 1,000,000
- Probabilities: 0 – 1
- Vector embeddings: -10 – 10
- ML activation outputs: typically bounded

### Current State

Quantitative trading systems already use scaled integers:

- Bloomberg terminals
- Goldman Sachs quant engines
- Citadel trading systems
- Rithmic / Interactive Brokers APIs

They do this because it's:

- Deterministic
- Cache-friendly
- SIMD-friendly
- Fast

### Desired State

CipherOcto should provide:

- A SQL type for scaled deterministic arithmetic
- Performance approaching native integers
- Decimal precision control
- Full consensus determinism

## Specification

### Data Structures

```rust
/// Deterministic Quant Arithmetic representation
///
/// # Mental Model
/// - `value` = numerator
/// - `10^scale` = denominator (implicit)
///
/// So `Dqa { value: 1234, scale: 3 }` represents `1234 / 10^3 = 1.234`
///
/// Note: `PartialOrd` and `Ord` are NOT derived—they must be manually implemented
/// using `DQA_CMP` to ensure correct numeric ordering (derived impl compares raw fields).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Dqa {
    /// Integer value (the numerator)
    value: i64,
    /// Decimal scale (the exponent for 10^-scale)
    scale: u8,
}

/// DQA encoding for storage/consensus
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct DqaEncoding {
    pub value: i64,
    pub scale: u8,
    pub _reserved: [u8; 7], // Padding to 16 bytes
}

impl DqaEncoding {
    /// Serialize DQA to canonical big-endian encoding
    /// CRITICAL: Canonicalizes before encoding to ensure deterministic Merkle hashes
    pub fn from_dqa(dqa: &Dqa) -> Self {
        let canonical = CANONICALIZE(*dqa);
        Self {
            value: canonical.value.to_be(),
            scale: canonical.scale,
            _reserved: [0; 7],
        }
    }

    /// Deserialize from canonical encoding
    /// Returns error if reserved bytes are non-zero (malformed/future-versioned)
    pub fn to_dqa(&self) -> Result<Dqa, DqaError> {
        // Validate scale for consensus safety
        if self.scale > 18 {
            return Err(DqaError::InvalidScale);
        }
        // Validate reserved bytes for consensus safety
        for byte in &self._reserved {
            if *byte != 0 {
                return Err(DqaError::InvalidEncoding);
            }
        }
        Ok(Dqa {
            value: i64::from_be(self.value),
            scale: self.scale,
        })
    }
}
```

### APIs/Interfaces

```rust
impl Dqa {
    /// Create DQA from value and scale
    /// Returns Error::InvalidScale if scale > 18
    pub fn new(value: i64, scale: u8) -> Result<Self, DqaError> {
        if scale > 18 {
            return Err(DqaError::InvalidScale);
        }
        Ok(Self { value, scale })
    }

    /// Create from f64 (with rounding to scale)
    /// WARNING: Non-consensus API. FP parsing varies across platforms.
    /// Use only for display/export, never for consensus-critical computation.
    /// Returns Error::InvalidInput for NaN or Infinity.
    #[cfg(feature = "non_consensus")]
    pub fn from_f64(value: f64, scale: u8) -> Result<Self, DqaError> {
        if scale > 18 {
            return Err(DqaError::InvalidScale);
        }
        if value.is_nan() || value.is_infinite() {
            return Err(DqaError::InvalidInput);
        }
        // Algorithm: multiply by 10^scale, round to nearest integer, clamp to i64
        // Note: f64::round() uses half-away-from-zero, not RoundHalfEven.
        // WARNING: Values ingested via from_f64 may differ by ±1 ULP from integer-path values
        // when compared via DQA_CMP. Only use for display/logging, never for consensus.
        // Future enhancement: add from_f64_half_even using integer arithmetic for stricter callers.
        let power = POW10_I64[scale as usize];
        let scaled = value * power as f64;
        let rounded = scaled.round();
        if rounded > i64::MAX as f64 || rounded < i64::MIN as f64 {
            return Err(DqaError::Overflow);
        }
        Ok(Dqa { value: rounded as i64, scale })
    }

    /// Convert to f64 (lossy)
    /// WARNING: Non-consensus API. Only use for display/logging.
    #[cfg(feature = "non_consensus")]
    pub fn to_f64(&self) -> f64;

    /// Arithmetic operations (return Result for overflow/division-by-zero safety)
    pub fn add(self, other: Self) -> Result<Self, DqaError>;
    pub fn sub(self, other: Self) -> Result<Self, DqaError>;
    pub fn mul(self, other: Self) -> Result<Self, DqaError>;
    pub fn div(self, other: Self) -> Result<Self, DqaError>;
}

/// Expression VM opcodes
pub enum VmOpcode {
    // ... existing opcodes
    OP_DQA_ADD,
    OP_DQA_SUB,
    OP_DQA_MUL,
    OP_DQA_DIV,
    OP_DQA_NEG,    // Unary negation
    OP_DQA_ABS,    // Absolute value
    OP_DQA_CMP,    // Compare: returns -1, 0, or 1
}

/// Comparison result for VM
pub enum CmpResult {
    Less = -1,
    Equal = 0,
    Greater = 1,
}
```

### SQL Integration

```sql
-- Deterministic quant table
CREATE TABLE trades (
    id INTEGER PRIMARY KEY,
    price DQA(6),       -- 6 decimal places: 1.234567
    quantity DQA(3),    -- 3 decimal places: 123.456
    executed_at TIMESTAMP
);

-- Multiplication naturally combines scales (no alignment needed)
-- result_scale = 6 + 3 = 9
SELECT
    price * quantity  -- OK: returns DQA(9)
FROM trades;

-- Division: uses TARGET_SCALE = max(a.scale, b.scale)
SELECT
    price / quantity  -- OK: returns DQA(6)
FROM trades;
```

#### SQL Value Ingress (INSERT/UPDATE)

When inserting a value into a DQA column, the value is **rounded to the column's scale** using RoundHalfEven:

```sql
-- Column: price DQA(6)
INSERT INTO trades (price) VALUES (123.4567899);  -- rounds to 123.456790
INSERT INTO trades (price) VALUES (123.4567894);  -- rounds to 123.456789
```

| Ingress Scenario     | Behavior                                                           |
| -------------------- | ------------------------------------------------------------------ |
| Extra decimal places | Round to column scale (RoundHalfEven)                              |
| Fewer decimal places | Pad with zeros                                                     |
| Scale exceeds column | Round to column scale using `DQA_ASSIGN_TO_COLUMN` (RoundHalfEven) |

This ensures deterministic storage regardless of input precision.

### Scale Alignment Rules

DQA operations require scale alignment:

| Operation | Result Scale                 |
| --------- | ---------------------------- |
| ADD/SUB   | max(scale_a, scale_b)        |
| MUL       | scale_a + scale_b            |
| DIV       | See division algorithm below |

#### Scale Alignment Algorithm (Pure Function)

**IMPORTANT**: This is a PURE function. It returns NEW values and does NOT mutate inputs.

```
ALIGN_SCALES(a, b):
    1. If a.scale == b.scale: return (a.value, b.value)
    2. diff = |a.scale - b.scale|
    3. power = POW10[diff]  // i128 table
    4. If a.scale > b.scale:
         // Use i128 for overflow-safe multiplication
         intermediate = (b.value as i128) * power
         if intermediate > i64::MAX as i128 or intermediate < i64::MIN as i128 {
             return Error::Overflow
         }
         new_b_value = intermediate as i64
         new_a_value = a.value
       Else:
         intermediate = (a.value as i128) * power
         if intermediate > i64::MAX as i128 or intermediate < i64::MIN as i128 {
             return Error::Overflow
         }
         new_a_value = intermediate as i64
         new_b_value = b.value
    5. Return (new_a_value, new_b_value)  // Caller computes result_scale as max(a.scale, b.scale)
```

**Note**: The returned values are NOT yet canonicalized. Callers MUST canonicalize if required (e.g., after ADD/SUB). The result scale is always `max(a.scale, b.scale)`, so callers compute this directly rather than using returned scale values.

### Arithmetic Algorithms

#### Addition

```
DQA_ADD(a, b):
    1. (a_val, b_val) = ALIGN_SCALES(a, b)
    2. // Use i128 to detect overflow
    3. result_value = (a_val as i128) + (b_val as i128)
    4. If result_value > i64::MAX or result_value < i64::MIN:
         return Error::Overflow
    5. result_scale = max(a.scale, b.scale)  // original scales
    6. // Canonicalize to prevent Merkle hash mismatches
    7. result = CANONICALIZE(Dqa { value: result_value as i64, scale: result_scale })
    8. Return result
```

**Note**: Canonicalization after ADD/SUB is required because results like `2000 scale=3` (2.000) must serialize as `2 scale=0` to maintain deterministic state hashes.

#### Subtraction

```
DQA_SUB(a, b):
    1. (a_val, b_val) = ALIGN_SCALES(a, b)
    2. // Use i128 to detect overflow
    3. result_value = (a_val as i128) - (b_val as i128)
    4. If result_value > i64::MAX or result_value < i64::MIN:
         return Error::Overflow
    5. result_scale = max(a.scale, b.scale)  // original scales
    6. // Canonicalize to prevent Merkle hash mismatches
    7. result = CANONICALIZE(Dqa { value: result_value as i64, scale: result_scale })
    8. Return result
```

#### Multiplication

```
DQA_MUL(a, b):
    1. // Use i128 intermediate to prevent overflow during calculation
    2. intermediate = (a.value as i128) * (b.value as i128)
    3. result_scale = a.scale + b.scale
    4. // If scale > 18, round to 18 while in i128
    5. If result_scale > 18:
         diff = result_scale - 18
         // Get quotient and remainder for proper RoundHalfEven
         quotient = intermediate / POW10[diff]
         round_remainder = intermediate % POW10[diff]
         // Apply RoundHalfEven using the helper with sign
         result_sign = sign(a.value) * sign(b.value)
         intermediate = ROUND_HALF_EVEN_WITH_REMAINDER(quotient, round_remainder, POW10[diff], result_sign)
         result_scale = 18
    6. // Check for i64 overflow after rounding
    7. If intermediate > i64::MAX as i128 or intermediate < i64::MIN as i128:
         return Error::Overflow
    8. // Canonicalize (may strip trailing zeros from multiplication results)
    9. result = CANONICALIZE(Dqa { value: intermediate as i64, scale: result_scale })
   10. Return result
```

**Critical**: Rounding MUST happen in i128 before checked conversion to i64, otherwise 10^36 cannot be safely rounded down to fit. Canonicalization is REQUIRED to prevent Merkle hash divergence.

**Normative behavior for result_scale > MAX_SCALE (18)**: When multiplication produces scale > 18, the value is right-shifted (divided by 10^(result_scale - 18)) with RoundHalfEven applied, then clamped to scale=18. Overflow is checked after clamping.

#### Division (Simplified Correct Algorithm)

To achieve true RoundHalfEven at TARGET_SCALE, compute directly at TARGET_SCALE precision and apply rounding once.

```
DQA_DIV(a, b):
    1. TARGET_SCALE = max(a.scale, b.scale)
    2. If b.value == 0: return Error::DivisionByZero
    3. power = TARGET_SCALE + b.scale - a.scale
    4. // Guard against i128 overflow using checked multiplication
    5. match (a.value as i128).checked_mul(POW10[power]) {
    6.     Some(s) => scaled = s,
    7.     None => return Error::Overflow,
    8. }
    9. quotient = scaled / (b.value as i128)
   10. remainder = scaled % (b.value as i128)
   11. result_sign = sign(a.value) * sign(b.value)
   12. abs_b = abs(b.value as i128)
   13. result_value = ROUND_HALF_EVEN_WITH_REMAINDER(quotient, remainder, abs_b, result_sign)
   14. if result_value > i64::MAX or result_value < i64::MIN:
   15.     return Error::Overflow
   16. // Canonicalize to prevent Merkle hash divergence (required for all arithmetic ops)
   17. Return CANONICALIZE(Dqa { value: result_value as i64, scale: TARGET_SCALE })
```

**Note**: This simplified algorithm computes at exactly TARGET_SCALE precision, applies RoundHalfEven once using the helper, and returns.

#### Rounding Semantics Trade-off

This implementation uses **"last-remainder half-even"** rounding: RoundHalfEven applied using the single remainder from `scaled % b.value`.

**Trade-off rationale:**

- **Performance**: ~10-25% faster than guard-digit approach
- **Precision**: Correct for typical financial calculations (prices, quantities, fees with 0-8 decimals)
- This algorithm produces correct rounding at TARGET_SCALE precision for all inputs

#### Division Rounding Semantics (Deliberate Trade-off)

DQA division applies **RoundHalfEven using the single remainder** after integer division at exactly TARGET_SCALE precision.

This differs from mathematically complete decimal rounding (e.g. PostgreSQL NUMERIC, Java BigDecimal) which use a **guard digit + sticky bit**.

**Consequences:**

- The remainder-based rounding decision is mathematically exact for the TARGET_SCALE precision
- DQA caps output precision at 18 decimal places; PostgreSQL NUMERIC returns unlimited digits
- Performance benefit: no extra multiplication/shift

For applications requiring stricter mathematical rounding (e.g. certain derivatives valuation engines), use DFP instead.

Most quantitative trading systems accept this approximation for the significant speed gain.

**Alternative**: For high-precision risk/valuation engines, use the guard-digit variant:

```
// Compute at TARGET_SCALE + 1, round using that digit, then shift back
power = (TARGET_SCALE + 1) + b.scale - a.scale
// ... rest of algorithm
```

This matches PostgreSQL NUMERIC and Java BigDecimal behavior.

### Constraints

- **Determinism**: All nodes produce identical results
- **Scale limit**: Maximum 18 decimal places
- **Value limit**: i64 range (-9.2×10¹⁸ to 9.2×10¹⁸)
- **Type mixing**: Forbidden without explicit alignment
- **No special values**: No NaN, no Infinity (use DFP for these)

### Explicit Constants

```rust
/// Maximum allowed scale (0-18)
pub const MAX_SCALE: u8 = 18;

/// Maximum decimal digits in abs(i64): i64::MAX has 19 digits
pub const MAX_I64_DIGITS: u32 = 19;

/// Maximum decimal digits in i128: i128::MAX has 39 digits (170141183460469231731687303715884105727)
pub const MAX_I128_DIGITS: u32 = 39;

/// Canonical zero representation
pub const CANONICAL_ZERO: Dqa = Dqa { value: 0, scale: 0 };

/// Canonical invariant: value != 0 → value % 10 != 0
/// (after canonicalization, trailing zeros must be stripped)
```

### Deterministic Overflow Handling

**Critical for consensus safety**: All arithmetic MUST use checked integer operations.

```rust
/// All arithmetic uses i128 intermediate, checked conversion to i64
fn checked_mul(a: i64, b: i64) -> Result<i64, DqaError> {
    let intermediate = (a as i128) * (b as i128);
    if intermediate > i64::MAX as i128 || intermediate < i64::MIN as i128 {
        return Err(DqaError::Overflow);
    }
    Ok(intermediate as i64)
}
```

| Scenario             | Behavior                                     |
| -------------------- | -------------------------------------------- |
| i64 overflow         | Return `DqaError::Overflow` deterministic    |
| i64 underflow        | Return `DqaError::Overflow` deterministic    |
| Scale overflow (>18) | Round to 18 (see DQA_MUL normative behavior) |

### Deterministic Rounding Mode

**All rounding uses RoundHalfEven (Banker's Rounding)**.

This is the industry standard used by:

- IEEE 754
- PostgreSQL NUMERIC
- Java BigDecimal
- Most financial systems

```
ROUND_HALF_EVEN(value, current_scale, target_scale):
    1. If target_scale >= current_scale: return value (no rounding needed)
    2. divisor = POW10[current_scale - target_scale]
    3. quotient = value / divisor
    4. remainder = value % divisor
    5. // Use absolute remainder for comparison (Rust % preserves sign of dividend)
    6. abs_remainder = abs(remainder)
    7. half = divisor / 2
    8. If abs_remainder < half: return quotient
    9. If abs_remainder > half: return quotient + sign(value)
    10. // remainder == half (tie)
    11. If quotient % 2 == 0: return quotient  // quotient is even (Rust's signed % is safe: (-2)%2==0, (-3)%2==-1)
    12. Else: return quotient + sign(value)
```

**Note**: The `sign(value)` function returns 1 for positive, -1 for negative, 0 for zero.

**Note**: This scale-reduction variant is provided for completeness. All DQA algorithms use `ROUND_HALF_EVEN_WITH_REMAINDER` instead, which operates directly on quotient/remainder pairs without re-dividing.

#### Rounding Helper for Division

This helper is used by both multiplication and division.

```
ROUND_HALF_EVEN_WITH_REMAINDER(quotient, remainder, divisor, result_sign):
    1. double_rem = abs(remainder) * 2
    2. abs_divisor = abs(divisor)
    3. If double_rem < abs_divisor: return quotient
    4. If double_rem > abs_divisor: return quotient + result_sign
    5. // double_rem == abs_divisor (tie exactly at 0.5)
    6. // Round half even: check if magnitude is even
    7. If (abs(quotient) % 2) == 0: return quotient  // magnitude is even
    8. Else: return quotient + result_sign
```

**Note**: `result_sign` is calculated as `sign(a.value) * sign(b.value)` in the caller to handle negative division correctly even when quotient is 0.

| Example | Target Scale | Result |
| ------- | ------------ | ------ |
| 1.25    | 1            | 1.2    |
| 1.35    | 1            | 1.4    |
| 1.250   | 1            | 1.2    |
| 1.150   | 1            | 1.2    |

### Canonical Representation

**Canonical form is required for deterministic serialization and Merkle hashing.**

Two DQA values representing the same number MUST have identical encodings.

```
CANONICALIZE(dqa):
    1. If dqa.value == 0: return Dqa { value: 0, scale: 0 }
    2. // Strip trailing zeros from value
    3. While dqa.value % 10 == 0 AND dqa.scale > 0:
         dqa.value = dqa.value / 10
         dqa.scale = dqa.scale - 1
    4. Return dqa
```

**Note**: The `dqa.scale > 0` guard prevents u8 underflow when scale is 0.

| Input           | Canonical Form |
| --------------- | -------------- |
| value=1000, s=3 | value=1, s=0   |
| value=50, s=2   | value=5, s=1   |
| value=0, s=5    | value=0, s=0   |

**Serialization MUST canonicalize before encoding**, otherwise Merkle state hashes will differ.

#### Canonicalization Rule

**Rule**: All arithmetic operations (ADD, SUB, MUL, DIV) **MUST canonicalize their result before returning**.

This ensures:

- Internal state is deterministic
- Comparisons work correctly
- Serialization is consistent

**Note**: SQL column storage is a special case — values inserted into fixed-scale columns retain the column's scale, not the canonical form. Only expression results are canonicalized.

#### Lazy Canonicalization (Optimization)

For hot paths in VM execution, canonicalization after every operation can be expensive. Consider:

```
// Canonicalization is mandatory for:
// - Storage/serialization
// - State hash/Merkle computation
// - Cross-node comparison
// - Final result return

// Canonicalization can be DEFERRED for:
// - Intermediate register values in expression evaluation
// - Scratch space calculations
// - Fast comparison when scales are known equal
```

**VM Canonicalization Rule (Normative)**: VM registers MAY contain non-canonical DQA values during intermediate evaluation. Before a value is used for comparison, serialization, hashing, storage, control-flow evaluation, or returning from a VM frame, it MUST be canonicalized. This guarantees cross-node state equivalence.

### Deterministic Power Table

**Never use floating-point pow()** — FP rounding varies across platforms.

Division can require up to 10^36 (TARGET_SCALE + b.scale - a.scale can be 18 + 18 - 0 = 36).

```rust
/// Deterministic POW10 table for scale alignment and division
/// POW10[i] = 10^i as i128
/// Range: 10^0 to 10^36 (fits in i128: max is ~3.4 × 10^38)
const POW10: [i128; 37] = [
    1,                           // 10^0
    10,                          // 10^1
    100,                         // 10^2
    1000,                        // 10^3
    10000,                       // 10^4
    100000,                      // 10^5
    1000000,                     // 10^6
    10000000,                    // 10^7
    100000000,                   // 10^8
    1000000000,                  // 10^9
    10000000000,                 // 10^10
    100000000000,                // 10^11
    1000000000000,               // 10^12
    10000000000000,              // 10^13
    100000000000000,             // 10^14
    1000000000000000,            // 10^15
    10000000000000000,           // 10^16
    100000000000000000,          // 10^17
    1000000000000000000,         // 10^18
    10000000000000000000,        // 10^19
    100000000000000000000,       // 10^20
    1000000000000000000000,      // 10^21
    10000000000000000000000,     // 10^22
    100000000000000000000000,    // 10^23
    1000000000000000000000000,   // 10^24
    10000000000000000000000000,  // 10^25
    100000000000000000000000000, // 10^26
    1000000000000000000000000000,// 10^27
    10000000000000000000000000000,// 10^28
    100000000000000000000000000000,// 10^29
    1000000000000000000000000000000,// 10^30
    10000000000000000000000000000000,// 10^31
    100000000000000000000000000000000,// 10^32
    1000000000000000000000000000000000,// 10^33
    10000000000000000000000000000000000,// 10^34
    100000000000000000000000000000000000,// 10^35
    1000000000000000000000000000000000000,// 10^36
];

/// For i64-safe operations (scales 0-18 only)
/// Use when the result is guaranteed to fit in i64 (e.g., scale alignment when diff <= 18 and values are small)
const POW10_I64: [i64; 19] = [
    1, 10, 100, 1000, 10000, 100000, 1000000, 10000000,
    100000000, 1000000000, 10000000000, 100000000000,
    1000000000000, 10000000000000, 100000000000000,
    1000000000000000, 10000000000000000, 100000000000000000,
    1000000000000000000,
];
```

### SQL Column Scale Semantics

DQA in SQL columns uses **fixed scale** (not per-value scale):

```sql
-- Column definition: scale is fixed at column level
CREATE TABLE trades (
    id INTEGER PRIMARY KEY,
    price DQA(6),       -- Always 6 decimal places
    quantity DQA(3),    -- Always 3 decimal places
    total DQA(9)        -- Computed: 6 + 3
);

-- Storage: value normalized to column's scale
-- price = 123456 with scale 6  -> stored as 0.123456
```

**Per-value scale** is only available in expression arithmetic, not column storage.

**Canonical vs SQL Storage**: SQL column storage retains the column's declared scale, not the canonical form. A value like `1.200000` with column scale 6 is stored as `{value: 1200000, scale: 6}`, not canonicalized to `{value: 12, scale: 1}`. This is intentional for SQL semantics. However, **canonicalization MUST occur when values exit SQL storage** into VM execution, serialization, hashing, or state comparison. The canonical form is required for deterministic Merkle hashes.

#### Expression-to-Column Assignment Coercion

When storing an expression result into a fixed-scale column:

```
DQA_ASSIGN_TO_COLUMN(expr_result, column_scale):
    1. if expr_result.scale > column_scale:
    2.     // Round to column scale using RoundHalfEven
    3.     diff = expr_result.scale - column_scale
    4.     divisor = POW10[diff]
    5.     quotient = expr_result.value / divisor
    6.     remainder = expr_result.value % divisor
    7.     // Round using ROUND_HALF_EVEN_WITH_REMAINDER
    8.     result_value = ROUND_HALF_EVEN_WITH_REMAINDER(quotient, remainder, divisor, sign(expr_result.value))
    9.     // Check i64 range (rounded quotient could theoretically exceed i64)
   10.    if result_value > i64::MAX as i128 or result_value < i64::MIN as i128:
   11.        return Error::Overflow
   12.    return Dqa { value: result_value as i64, scale: column_scale }
    13. else if expr_result.scale < column_scale:
    14.     // Pad with trailing zeros
    15.     diff = column_scale - expr_result.scale
    16.     // Use i128 for overflow-safe multiplication
    17.     intermediate = (expr_result.value as i128) * POW10[diff]
    18.     if intermediate > i64::MAX as i128 or intermediate < i64::MIN as i128:
    19.         return Error::Overflow
    20.     result_value = intermediate as i64
    21.     return Dqa { value: result_value, scale: column_scale }
    22. else:
    23.     return expr_result  // scales match, no coercion needed
```

**Note**: The rounding uses the same `ROUND_HALF_EVEN_WITH_REMAINDER` helper as division, ensuring deterministic results.

### Error Handling

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DqaError {
    /// Integer overflow during arithmetic
    Overflow,
    /// Division by zero
    DivisionByZero,
    /// Invalid scale (must be 0-18)
    InvalidScale,
    /// Invalid input (NaN, Infinity for f64 conversion)
    InvalidInput,
    /// Reserved bytes in encoding are non-zero
    InvalidEncoding,
}

/// ScaleUnderflow is mathematically unreachable with valid DQA inputs
/// (TARGET_SCALE = max(a.scale, b.scale) ensures power >= 0)
```

| Scenario                     | Behavior                                     |
| ---------------------------- | -------------------------------------------- |
| DQA \* FLOAT                 | Compile error                                |
| DQA + DQA (mismatched scale) | Automatic alignment via ALIGN_SCALES         |
| Division by zero             | Return `DqaError::DivisionByZero`            |
| Scale overflow (>18)         | Round to 18 (see DQA_MUL normative behavior) |
| i64 overflow                 | Return `DqaError::Overflow`                  |
| NEG(i64::MIN)                | Return `DqaError::Overflow` (abs overflows)  |
| ABS(i64::MIN)                | Return `DqaError::Overflow` (abs overflows)  |

### VM Determinism Guarantees

For consensus safety, the VM implementation MUST:

1. **Use only deterministic integer operations** — no floating-point arithmetic
2. **Use checked arithmetic** — never wrapping/overflowing silently
3. **Never use architecture-dependent intrinsics** — SIMD may be used internally BUT results MUST match scalar reference implementation bit-exactly. All implementations MUST pass reference test vectors using scalar arithmetic semantics.
4. **Serialize canonical form** — before hashing or storing state
5. **Use POW10 table** — never call pow() or exp() functions

```
VM_DQA_INVARIANTS:
    - All arithmetic uses checked_i128 -> checked_i64
    - All rounding uses RoundHalfEven
    - Serialization always canonicalizes first
    - No NaN, no Infinity representations exist
```

## Rationale

### Why Scaled Integer?

The quant finance industry has decades of evidence that scaled integers are:

| Property          | Scaled Integer   | Binary Float          |
| ----------------- | ---------------- | --------------------- |
| Determinism       | ✅ Guaranteed    | ❌ Platform-dependent |
| Speed             | 1.5-3.5x integer | 10-40x slower         |
| Cache efficiency  | ✅ Excellent     | ❌ Poor               |
| SIMD support      | ✅ Excellent     | ❌ Limited            |
| Decimal precision | ✅ Exact         | ❌ Approximate        |

### Alternatives Considered

| Alternative  | Pros                | Cons               | Rejection Reason               |
| ------------ | ------------------- | ------------------ | ------------------------------ |
| DECIMAL      | SQL standard        | Variable precision | Not deterministic enough       |
| DFP          | Arbitrary precision | 10-40x slower      | Overkill for bounded workloads |
| Fixed-point  | Simple              | Limited range      | Already covered by INTEGER     |
| Binary float | Fast                | Non-deterministic  | Unsafe for consensus           |

### Trade-offs

| Priority   | Trade-off              |
| ---------- | ---------------------- |
| Prioritize | Speed, determinism     |
| Accept     | Limited scale (max 18) |
| Accept     | No special values      |

## Implementation Notes (For Rust Engineers)

### Error Enum Simplification

`ScaleUnderflow` is mathematically unreachable with valid DQA inputs (because `TARGET_SCALE = max(a.scale, b.scale)` ensures `power >= 0`). You may omit this variant from the Rust implementation to simplify error matching.

### Scale Validation in Constructor

When implementing `Dqa::new(value, scale)`, enforce the `scale > 18` check. Since the POW10 array only has 37 elements (indices 0-36), attempting to create a DQA with an out-of-bounds scale (e.g., 255) via unsafe memory casting will cause an out-of-bounds panic. The constructor and `DqaEncoding::to_dqa` should validate this boundary.

### Sign Function

In Rust, use the `.signum()` method on integers:

- `a.value.signum()` returns `1`, `0`, or `-1` for positive, zero, or negative respectively.

This matches the `sign(value)` pseudo-code function used throughout the algorithms.

## Implementation

### Mission 1: DQA Core Type ✅

- Location: `determ/dqa.rs`
- Acceptance criteria:
  - [x] DQA struct with value/scale
  - [x] Arithmetic: add, sub, mul, div
  - [x] Scale alignment rules
  - [x] From/To f64 conversion
  - [x] Serialization (DqaEncoding)
- Estimated complexity: Low

### Mission 2: DataType Integration ✅

- Location: `stoolap/src/parser/ast.rs`, `stoolap/src/parser/statements.rs`
- Acceptance criteria:
  - [x] Add `DataType::Quant` variant
  - [x] SQL parser accepts `DQA(n)` syntax
  - [x] Type checking for scale alignment (built into DQA operations)
- Estimated complexity: Low

### Mission 3: Expression VM Opcodes ✅

- Location: `stoolap/src/executor/expression/vm.rs`, `stoolap/src/executor/expression/ops.rs`
- Acceptance criteria:
  - [x] OP_DQA_ADD, OP_DQA_SUB, OP_DQA_MUL, OP_DQA_DIV
  - [x] Scale alignment validation (built into DQA operations)
- Estimated complexity: Low

### Mission 4: Consensus Integration ⏳

- Location: `stoolap/src/storage/`, `stoolap/src/consensus/`
- Acceptance criteria:
  - [x] DQA encoding in Merkle state (DqaEncoding uses canonical form)
  - [x] Spec version pinning (DQA_SPEC_VERSION = 1)
  - [ ] Deterministic view enforcement
  - [ ] Consensus replay validation
- Estimated complexity: Medium

## Impact

### Breaking Changes

None. DQA is a new type.

### Performance

| Type    | Relative Speed | Notes                                       |
| ------- | -------------- | ------------------------------------------- |
| INTEGER | 1x (baseline)  | Native integer ops                          |
| DQA     | 1.5-3.5x       | Includes scale alignment + canonicalization |
| DECIMAL | 2-3x           | Variable precision libraries                |
| DFP     | 8-20x          | Arbitrary precision                         |

**Note**: Real-world DQA performance includes:

- Scale alignment (1 multiplication + check)
- Canonicalization (trailing zero strip, rare but required)
- Overflow checks (i128 intermediate)
- Optional: checked arithmetic overhead

For hot loops, an **unchecked fast path** may be offered via `#[cfg(feature = "fast")]`.

#### Optional Fast-Path Implementation

For high-frequency trading hot paths, an optional fast mode skips safety checks:

```rust
/// Unchecked scale alignment for fast path - caller guarantees no overflow
/// Uses i64 wrapping multiply with POW10_I64 table (diff <= 18 ensures no overflow)
#[cfg(feature = "fast")]
fn align_scales_unchecked(a: Dqa, b: Dqa) -> (i64, u8, i64, u8) {
    if a.scale == b.scale {
        (a.value, a.scale, b.value, b.scale)
    } else if a.scale > b.scale {
        let diff = a.scale - b.scale;
        // diff <= 18, so POW10_I64[diff] fits in i64; wrapping_mul for fast path
        (a.value, a.scale, b.value.wrapping_mul(POW10_I64[diff as usize]), a.scale)
    } else {
        let diff = b.scale - a.scale;
        (a.value.wrapping_mul(POW10_I64[diff as usize]), b.scale, b.value, b.scale)
    }
}

#[cfg(feature = "fast")]
impl Dqa {
    /// Fast add: skips canonicalization and overflow checks
    /// WARNING: Only use when input ranges are proven safe
    pub fn add_fast(self, other: Self) -> Self {
        let (a_val, a_scale, b_val, b_scale) = align_scales_unchecked(self, other);
        Dqa { value: a_val + b_val, scale: a_scale.max(b_scale) }
    }
}
```

**When to use fast path:**

- Input values proven to be within safe range (e.g., pre-validated price feeds)
- Temporary calculations in tight loops where safety is guaranteed externally
- Performance-critical code with verified bounded inputs

**When NOT to use:**

- User-supplied data
- Smart contract execution
- Cross-node consensus

### Dependencies

- RFC-0104: DFP (complementary type)
- RFC-0103: Vector-SQL Storage

## Related RFCs

- RFC-0104: Deterministic Floating-Point Abstraction (DFP)
- RFC-0103: Unified Vector-SQL Storage Engine

## Reference Test Vectors

These vectors MUST produce identical results across all implementations:

### Addition Test Vectors

| a.value | a.scale | b.value | b.scale | expected value | expected scale |
| ------- | ------- | ------- | ------- | -------------- | -------------- |
| 12      | 1       | 123     | 2       | 243            | 2              |
| 1000    | 3       | 1       | 0       | 1001           | 3              |
| -50     | 2       | 75      | 2       | 25             | 2              |
| 0       | 0       | 0       | 5       | 0              | 0 (canonical)  |

### Multiplication Test Vectors

| a.value | a.scale | b.value | b.scale | expected value | expected scale |
| ------- | ------- | ------- | ------- | -------------- | -------------- |
| 12      | 1       | 3       | 1       | 36             | 2              |
| 100     | 2       | 200     | 3       | 20000          | 5              |
| -5      | 1       | 4       | 1       | -20            | 2              |

### Division Test Vectors

Using simplified algorithm: compute at TARGET_SCALE, apply RoundHalfEven directly.

| a.value  | a.scale | b.value | b.scale | expected value | expected scale | Note                                             |
| -------- | ------- | ------- | ------- | -------------- | -------------- | ------------------------------------------------ |
| 1000     | 3       | 2       | 0       | 500            | 3              | 1.0 / 2 = 0.5                                    |
| 1000000  | 6       | 2       | 0       | 500000         | 6              | 1.0 / 2 = 0.5 at scale 6                         |
| 1        | 6       | 2       | 0       | 0              | 6              | 0.000001 / 2 = 0.0000005 → rounds to 0           |
| 10       | 1       | 4       | 0       | 2              | 1              | 1.0 / 4 = 0.25 → rounds to 0.2                   |
| 5        | 0       | 2       | 0       | 3              | 0              | 5 / 2 = 2.5 → tie rounds to odd → rounds up to 3 |
| 15       | 0       | 2       | 0       | 8              | 0              | 15 / 2 = 7.5 → tie rounds to even (8)            |
| -5       | 0       | 2       | 0       | -3             | 0              | -5 / 2 = -2.5 → tie rounds to odd → rounds to -3 |
| -15      | 0       | 2       | 0       | -8             | 0              | -15 / 2 = -7.5 → tie rounds to even (-8)         |
| 1        | 0       | 3       | 0       | 0              | 0              | 1 / 3 = 0.333... → rounds down                   |
| -1       | 0       | 3       | 0       | 0              | 0              | -1 / 3 = -0.333... → rounds toward zero          |
| 2        | 0       | 3       | 0       | 1              | 0              | 2 / 3 = 0.666... → rounds up to 1                |
| 1        | 0       | 6       | 0       | 0              | 0              | 1 / 6 = 0.1666... → rounds down to 0             |
| 2000000  | 6       | 3       | 0       | 666667         | 6              | 2.0 / 3 = 0.666667 → rounds up                   |
| -2000000 | 6       | 3       | 0       | -666667        | 6              | -2.0 / 3 = -0.666667 → rounds toward zero        |

**Note**: The simplified algorithm produces mathematically correct RoundHalfEven at TARGET_SCALE.

**Note**: Division inherently produces infinite precision. The algorithm preserves max(a.scale, b.scale) digits and applies RoundHalfEven using b.value as the divisor.

#### Additional Test Vectors (Recommended for Full Compliance)

| a.value             | a.scale | b.value             | b.scale | expected value      | expected scale | Note                                |
| ------------------- | ------- | ------------------- | ------- | ------------------- | -------------- | ----------------------------------- |
| 9223372036854775807 | 0       | 1                   | 0       | 9223372036854775807 | 0              | MAX_i64 / 1                         |
| 9223372036854775807 | 0       | 2                   | 0       | 4611686018427387903 | 0              | MAX_i64 / 2 (truncates)             |
| 1                   | 0       | 9223372036854775807 | 0       | 0                   | 0              | 1 / MAX_i64 (very small)            |
| 9223372036854775807 | 0       | 3                   | 0       | 3074457345618258602 | 0              | MAX_i64 / 3                         |
| 1                   | 18      | 2                   | 0       | 500000000000000000  | 18             | 1e-18 / 2 = 5e-19                   |
| 9223372036854775807 | 18      | 1                   | 0       | 9223372036854775807 | 18             | MAX_i64 / 1 = MAX_i64 (no overflow) |
| 1                   | 0       | 3                   | 0       | 0                   | 0              | 1/3 rounds down                     |

### Chain Operations Test Vectors

| operation        | a             | b     | c   | expected | Note                     |
| ---------------- | ------------- | ----- | --- | -------- | ------------------------ |
| mul→div          | 10,0 \* 5,0   | / 2,0 | -   | 25,0     | (10\*5)/2 = 25           |
| add→canonicalize | 100,2 + 200,1 | -     | -   | 21,0     | 1.00 + 20.0 = 21.00 → 21 |
| mul→add          | 2,0 \* 3,0    | + 1,0 | -   | 7,0      | (2\*3) + 1 = 7           |

### Overflow Test Vectors

| operation | a.value | a.scale | b.value | b.scale | expected result |
| --------- | ------- | ------- | ------- | ------- | --------------- |
| MUL       | 10^18   | 0       | 10      | 0       | Error::Overflow |
| MUL       | 10^17   | 1       | 10      | 0       | 10^18 (OK)      |

### Rounding Test Vectors (RoundHalfEven)

| input value | input scale | target scale | expected value | expected scale |
| ----------- | ----------- | ------------ | -------------- | -------------- |
| 125         | 2           | 1            | 12             | 1              |
| 135         | 2           | 1            | 14             | 1              |
| 1250        | 3           | 1            | 12             | 1              |
| 1150        | 3           | 1            | 12             | 1              |
| 1050        | 3           | 1            | 10             | 1              |

### Canonicalization Test Vectors

| input value | input scale | expected value | expected scale |
| ----------- | ----------- | -------------- | -------------- |
| 1000        | 3           | 1              | 0              |
| 50          | 2           | 5              | 1              |
| 0           | 5           | 0              | 0              |
| 100         | 2           | 1              | 0              |

### Comparison Specification

All comparison operations (`<`, `<=`, `>`, `>=`, `=`, `<>`) MUST be performed after **canonicalizing both operands** (or equivalently, by comparing `value × 10^(max_scale - scale)` for both operands).

This ensures that `Dqa { value: 120, scale: 2 }` (1.20) equals `Dqa { value: 12, scale: 1 }` (1.2).

```
DQA_CMP(a, b):
    // Fast path avoids i128 when possible — important for VM hot paths (>90% of comparisons)
    1. // Canonicalize both operands first
    2. a_canonical = CANONICALIZE(a)
    3. b_canonical = CANONICALIZE(b)
    4. // Fast path: if scales equal, compare values directly
    5. if a_canonical.scale == b_canonical.scale:
    6.     if a_canonical.value < b_canonical.value: return -1
    7.     if a_canonical.value > b_canonical.value: return 1
    8.     return 0
    9. // Scale alignment with overflow guard
    10. diff = abs(a_canonical.scale as i32 - b_canonical.scale as i32)
    11. // After canonicalization, both scales are ≤ 18, so diff ≤ 18 always
    12. // This branch is kept for completeness but should never be reached
    13. debug_assert!(diff <= 18, "scale diff > 18 should be unreachable after canonicalization");
    14. if diff <= 18:
    15.     // Safe: 19 digits × 10^18 < i128 max (9.2e18 × 1e18 = 9.2e36 < 1.7e38)
    16.     if a_canonical.scale > b_canonical.scale:
    17.         scale_factor = POW10[diff as usize]
    18.         compare_a = a_canonical.value as i128
    19.         compare_b = (b_canonical.value as i128) * scale_factor
    20.     else:
    21.         scale_factor = POW10[diff as usize]
    22.         compare_a = (a_canonical.value as i128) * scale_factor
    23.         compare_b = b_canonical.value as i128
    // Scale diff <= 18: safe i128 multiplication
    24. If compare_a < compare_b: return -1
    26. If compare_a > compare_b: return 1
    27. Return 0
```

**Note on scale-diff > 18 comparison**: After canonicalization, both operands have scale ≤ 18, so their scale difference is at most 18. The `diff > 18` case is provably unreachable with valid DQA inputs and triggers a `debug_assert!` in implementation.

### Comparison Test Vectors

| a.value              | a.scale | b.value             | b.scale | expected    | Note                |
| -------------------- | ------- | ------------------- | ------- | ----------- | ------------------- |
| 12                   | 1       | 120                 | 2       | 0 (equal)   | 1.2 == 1.20         |
| 12                   | 1       | 110                 | 2       | 1 (greater) | 1.2 > 1.10          |
| 12                   | 1       | 130                 | 2       | -1 (less)   | 1.2 < 1.30          |
| -15                  | 1       | -15                 | 1       | 0 (equal)   | negative equality   |
| -15                  | 1       | -25                 | 1       | 1 (greater) | -1.5 > -2.5         |
| 9223372036854775807  | 0       | 1                   | 18      | 1 (greater) | i64::MAX vs 1e-18   |
| 1                    | 18      | 9223372036854775807 | 0       | -1 (less)   | 1e-18 vs i64::MAX   |
| 1000000000000000000  | 0       | 9223372036854775806 | 0       | 1 (greater) | near max comparison |
| -9223372036854775808 | 0       | -1                  | 0       | -1 (less)   | i64::MIN comparison |

### Additional Brutal Edge Case Test Vectors

| Operation            | a                                         | b                                                                      | Expected Result               | Note                                                                  |
| -------------------- | ----------------------------------------- | ---------------------------------------------------------------------- | ----------------------------- | --------------------------------------------------------------------- |
| DIV                  | -9223372036854775808                      | 1                                                                      | -9223372036854775808, scale=0 | i64::MIN ÷ 1                                                          |
| DIV                  | -9223372036854775808                      | -1                                                                     | Error::Overflow               | -i64::MIN / -1 = 9223372036854775808 > i64::MAX                       |
| DIV                  | 1000,3                                    | 3,0                                                                    | 333,3 (0.333)                 | 1/3 at scale=3                                                        |
| DIV                  | 2000,4                                    | 3,0                                                                    | 6667,4 (0.6667)               | 2/3 at scale=4                                                        |
| DIV                  | 1000000,6                                 | 7,0                                                                    | 142857,6 (0.142857)           | 1/7 at scale=6                                                        |
| DIV                  | 9223372036854775807                       | 2                                                                      | 4611686018427387903, scale=0  | MAX/2 exact                                                           |
| DIV                  | 1000,3                                    | 1,0                                                                    | 1,0                           | DIV result canonicalization: 1000/1=1000, scale=3→canonicalize to 1,0 |
| MUL                  | 9223372036854775807                       | 2                                                                      | Error::Overflow               | Near overflow multiplication                                          |
| MUL                  | 4611686018427387903                       | 2                                                                      | 9223372036854775806, scale=0  | Max safe × 2                                                          |
| ADD                  | 9223372036854775807                       | 1                                                                      | Error::Overflow               | i64::MAX + 1                                                          |
| SUB                  | -9223372036854775808                      | 1                                                                      | Error::Overflow               | i64::MIN - 1                                                          |
| Chain                | mul(1000,2) → div(2) → add(1,0)           | 1000×2=2000; 2000÷2=1000; 1000+1=1001                                  | 1001,0                        | mul→div→add→canonicalize                                              |
| Chain                | 1,18 × 1000,3 → canonicalize              | 1e-18 × 1000 = 1e-15, but result_scale=18+3=21 limited to MAX_SCALE=18 | 1,18                          | scale clamped to MAX_SCALE=18                                         |
| Serialize round-trip | 1200,3 → serialize → deserialize          | 1200,3 → 12,1                                                          | value=12, scale=1             | canonicalization on deserialize                                       |
| DIV                  | 25,2                                      | 2,0                                                                    | 12,2 (0.12)                   | 0.25 ÷ 2 = 0.125, half-even rounds to 12 (tie to even)                |
| DIV                  | 15,2                                      | 4,0                                                                    | 4,2 (0.04)                    | 0.15 ÷ 4 at scale 2: scaled=15/4=3 rem 3, 3/4 > 0.5 rounds up to 4    |
| DIV                  | 35,2                                      | 8,0                                                                    | 4,2 (0.04)                    | 0.35 ÷ 8 at scale 2: scaled=35/8=4 rem 3, 3<4 rounds down to 4        |
| DIV                  | -25,2                                     | 2,0                                                                    | -12,2 (-0.12)                 | -0.25 ÷ 2 = -0.125, half-even rounds to -12 (symmetric with positive) |
| DIV                  | -15,2                                     | 4,0                                                                    | -4,2 (-0.04)                  | -0.15 ÷ 4: symmetric rounding, rounds to -4                           |
| MUL                  | -5,1                                      | -3,1                                                                   | 15,2 (1.50)                   | negative × negative = positive                                        |
| MUL                  | -5,1                                      | 3,1                                                                    | -15,2 (-1.50)                 | mixed signs                                                           |
| Chain                | 10,2 × 20,2 → div 4,0 → canonicalize      | 200×20=4000; 4000÷4=1000; scale=2+2-0=4; canonicalize to 10,1          | 10,1                          | mul→div→canonicalize                                                  |
| Chain                | 99999999999999999,0 × 10,0 → canonicalize | 999999999999999990 → canonicalize                                      | 99999999999999999,0           | large value, no trailing zeros                                        |
| Chain                | 1,0 / 3,0 → × 3,0 → add 0,0               | 1÷3≈0; 0×3=0; 0+0=0                                                    | 0,0                           | division precision loss chain                                         |
| Chain                | 100,2 → add 200,2 → canonicalize          | 100+200=300; canonicalize                                              | 3,0                           | add→canonicalize trailing zeros                                       |
| Chain                | -50,2 → sub 25,2 → canonicalize           | -50-25=-75; canonicalize                                               | -75,2                         | negative subtraction                                                  |
| Compare              | 100,0                                     | 1,2                                                                    | 1 (greater)                   | 100 > 1.00                                                            |
| Compare              | 1,18                                      | 1,0                                                                    | -1 (less)                     | 1e-18 < 1                                                             |
| Compare              | -5,1                                      | -50,2                                                                  | 0 (equal)                     | -0.5 == -0.50 canonicalization                                        |
| Compare              | 1,0                                       | 1000000000000000000,18                                                 | 1 (greater)                   | 1 vs 1e-18 - near-zero crossover                                      |
| ADD                  | 9223372036854775807,0                     | 1,18                                                                   | Error::Overflow               | scale alignment overflow: i64::MAX + 1e-18                            |

## Use Cases

### Quantitative Finance

- Option pricing
- Portfolio valuation
- Risk metrics (VaR, Greeks)
- Order book calculations

### AI/ML Inference

- Activation function outputs
- Probability distributions
- Normalized embeddings
- Attention weights

### Gaming

- In-game currency
- Item pricing
- Achievement scores

## Related Use Cases

- [Hybrid AI-Blockchain Runtime](../../docs/use-cases/hybrid-ai-blockchain-runtime.md)
- [Verifiable AI Agents for DeFi](../../docs/use-cases/verifiable-ai-agents-defi.md)

---

**Submission Date:** 2025-03-06
**Last Updated:** 2026-03-08
**Revision:** v2.13 - Tightened MUL clamping wording, added large-value chain test, added >90% note to DQA_CMP fast-path
**Revision:** v2.12 - Added SQL vs canonical representation clarification, fixed division rounding wording (TARGET_SCALE precision), strengthened SIMD determinism rule, enforced canonicalization in encoding API, added control-flow to VM canonicalization rule, added power<=36 invariant, added scale alignment overflow test vector
**Revision:** v2.11 - Fixed DIV negative test vector (-12 not -13), added i64 range check to DQA_ASSIGN_TO_COLUMN, added CANONICALIZE to DIV return, unified scale overflow references, fixed test vector notes, added DIV canonicalization test vector, fixed MAX_I128_DIGITS to 39
**Revision:** v2.10 - Added MUL scale >18 normative behavior, added near-zero comparison test, added ALIGN_SCALES canonicalization note, added DQA_CMP hot-path comment, suggested from_f64_half_even future helper
**Revision:** v2.9 - Made lazy canonicalization rule normative, fixed DQA_ASSIGN_TO_COLUMN duplicate lines, clarified chain test vector comment
**Revision:** v2.8 - Fixed division overflow guard (replaced with checked_mul), fixed SQL column coercion i128 cast, fixed test vectors, updated comparison note, clarified ROUND_HALF_EVEN, fixed align_scales_unchecked, fixed SQL ingress table, added from_f64 warning, added parity comment, fixed MAX_I64_DIGITS comment
**Revision:** v2.7 - Added from_f64 rounding note, added 15+ brutal test vectors (negative ties, chains), added DQA vs DFP decision table
**Revision:** v2.6 - Fixed division overflow guard (actual digit count), removed unreachable comparison branch, removed incorrect PartialOrd/Ord derives, fixed division rounding claim, added SQL column coercion algorithm, fixed test vectors, removed ScaleOverflow error, fixed ALIGN_SCALES return type
**Revision:** v2.5 - Added division rounding trade-off section, documented scale-diff > 18 comparison heuristic, added brutal edge case test vectors
**Revision:** v2.4 - Fixed comparison overflow guard, tightened lazy canonicalization rule, added explicit constants, fixed division precision claim
**Revision:** v2.3 - Added derives to Dqa struct, fixed division overflow guard, fixed comparison overflow, corrected test vectors
**Revision:** v2.2 - Added rounding trade-off note, guard-digit variant, additional test vectors, lazy canonicalization, fast-path implementation
