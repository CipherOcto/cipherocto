# RFC-0104 (Numeric/Math): Deterministic Floating-Point Abstraction (DFP)

## Status

Draft

> **Note:** This RFC was originally numbered RFC-0104 under the legacy numbering system. It remains at 0104 as it belongs to the Numeric/Math category.

## Summary

This RFC introduces Deterministic Floating-Point (DFP) — a numeric abstraction that provides floating-point developer ergonomics while guaranteeing bit-identical execution across all nodes participating in CipherOcto consensus. DFP enables floating-point arithmetic in blockchain state transitions and deterministic query execution without sacrificing reproducibility.

The design introduces a two-tier numeric model: non-deterministic FLOAT/DOUBLE for analytics, and deterministic DFP for consensus-critical computations. Type mixing requires explicit casting to prevent ambiguous semantics.

> ⚠️ **EXPERIMENTAL WARNING**: DFP consensus usage is **experimental and carries high technical risk**. Most production blockchains avoid floating-point in consensus paths entirely. DFP should be considered **alpha-stage technology** until:
>
> - Hardware verification is proven robust over years of production
> - Comprehensive test vectors are validated across architectures
> - Transcendental functions (Mission 1b) are implemented
> - Real-world benchmarks demonstrate acceptable performance
>
> **Recommendation**: Start with software-only path. Use hardware fast-path only after extensive validation.

## Motivation

### Problem Statement

IEEE-754 floating-point arithmetic is non-deterministic across hardware architectures. Sources of nondeterminism include:

- CPU extended precision registers (x86 80-bit vs ARM 64-bit)
- Fused multiply-add (FMA) instruction differences
- Compiler optimization variations
- Platform-specific math library implementations
- Rounding mode inconsistencies

For example, `0.1 + 0.2` can produce slightly different bit patterns on different systems, causing state divergence in replicated state machines.

### Current State

Most blockchain systems avoid floating-point entirely:

- Bitcoin: Integer-only arithmetic
- Ethereum: No native float types
- Solana: Integer primitives
- Cosmos SDK: Fixed-point decimals

This creates developer friction for AI, statistical, and scientific workloads that naturally require floating-point semantics.

### Desired State

CipherOcto should support:

- Deterministic float arithmetic for consensus
- Standard SQL float types for analytics
- Explicit type boundaries with no silent conversions
- Hardware acceleration for compliant nodes
- Software fallback for non-compliant nodes

### Use Case Link

- [Hybrid AI-Blockchain Runtime](../docs/use-cases/hybrid-ai-blockchain-runtime.md)
- [Decentralized Mission Execution](../docs/use-cases/decentralized-mission-execution.md)

## Specification

### Two-Tier Numeric Model

```
Tier 1 — Non-Deterministic (Analytics)
├── FLOAT   — 32-bit IEEE-754
├── DOUBLE  — 64-bit IEEE-754
└── Use: Local queries, ML inference, vector search

Tier 2 — Deterministic (Consensus)
├── DFP     — Deterministic Floating-Point (this RFC)
├── DECIMAL — Deterministic Fixed-Point
└── Use: Blockchain state, smart contracts, replicated queries
```

### SQL Literal Parsing

In **deterministic execution mode**, numeric literals are implicitly typed as **DFP**:

```sql
-- In Deterministic View:
SELECT 0.1 + 0.2;  -- 0.1 and 0.2 parsed as DFP, result is deterministic
SELECT 1.5 * 2.0;  -- DFP multiplication
SELECT 1 / 0;      -- Returns MAX (saturating arithmetic)
```

| Context            | Literal Type | Behavior                   |
| ------------------ | ------------ | -------------------------- |
| Deterministic View | DFP          | Bit-identical across nodes |
| Analytics Query    | FLOAT/DOUBLE | Non-deterministic allowed  |
| Mixed              | ERROR        | Must use explicit CAST     |

### Data Structures

```rust
/// DFP class tag to avoid encoding collisions with numeric values
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DfpClass {
    /// Normal numeric value
    Normal,
    /// Positive infinity
    Infinity,
    /// Not a Number
    NaN,
    /// Zero (sign preserved)
    Zero,
}

/// Deterministic Floating-Point representation
/// Uses tagged representation to avoid encoding collisions
pub struct Dfp {
    /// Class tag (Normal, Infinity, NaN, Zero)
    class: DfpClass,
    /// Sign bit (0 = positive, 1 = negative)
    sign: bool,
    /// Mantissa (unsigned, only valid for Normal class)
    /// Stored as absolute value; sign is separate field
    mantissa: u128,
    /// Binary exponent (only valid for Normal class)
    exponent: i32,
}

impl Dfp {
    /// Create a normal DFP value
    pub fn new(mantissa: u128, exponent: i32, sign: bool) -> Self {
        Self {
            class: DfpClass::Normal,
            sign,
            mantissa,
            exponent,
        }
    }

    /// Create from signed mantissa
    pub fn from_signed(mantissa: i128, exponent: i32) -> Self {
        Self {
            class: DfpClass::Normal,
            sign: mantissa < 0,
            mantissa: mantissa.unsigned_abs(),
            exponent,
        }
    }

    /// Create infinity
    pub fn infinity(sign: bool) -> Self {
        Self {
            class: DfpClass::Infinity,
            sign,
            mantissa: 0,
            exponent: 0,
        }
    }

    /// Create canonical NaN
    pub fn nan() -> Self {
        Self {
            class: DfpClass::NaN,
            sign: false,
            mantissa: 0,
            exponent: 0,
        }
    }

    /// Create zero with sign preservation
    pub fn zero(sign: bool) -> Self {
        Self {
            class: DfpClass::Zero,
            sign,
            mantissa: 0,
            exponent: 0,
        }
    }

    /// Create from f64 (with canonical rounding)
    pub fn from_f64(value: f64) -> Self;

    /// Convert to f64 (lossy)
    pub fn to_f64(&self) -> f64;

    /// Arithmetic operations
    pub fn add(self, other: Self) -> Self;
    pub fn sub(self, other: Self) -> Self;
    pub fn mul(self, other: Self) -> Self;
    pub fn div(self, other: Self) -> Self;
}

/// DFP encoding for storage/consensus
/// Uses explicit 24-byte layout (no padding with repr(C))
#[derive(Clone, Copy, Debug)]
#[repr(C, align(8))]
pub struct DfpEncoding {
    /// Mantissa in big-endian (16 bytes, unsigned)
    mantissa: u128,
    /// Exponent in big-endian (4 bytes)
    exponent: i32,
    /// Class tag (0=Normal, 1=Infinity, 2=NaN, 3=Zero) - 1 byte
    /// Sign bit - 1 byte
    /// Reserved - 2 bytes
    class_sign: u32,  // [class:8][sign:8][reserved:16]
}

// SAFETY: DfpEncoding is 24 bytes exactly (16 + 4 + 4)
// Field order ensures no padding: u128(16) + i32(4) + u32(4) = 24
// ALWAYS use to_bytes() for cross-platform serialization

/// Optimized accessor methods
impl DfpEncoding {
    /// Create from DFP value
    pub fn from_dfp(dfp: &Dfp) -> Self {
        let class_sign = ((match dfp.class {
            DfpClass::Normal => 0,
            DfpClass::Infinity => 1,
            DfpClass::NaN => 2,
            DfpClass::Zero => 3,
        } as u32) << 24) | ((dfp.sign as u32) << 16);

        Self {
            mantissa: dfp.mantissa.to_be(),
            exponent: dfp.exponent.to_be(),
            class_sign,
        }
    }

    /// Convert to DFP value
    pub fn to_dfp(&self) -> Dfp {
        let class = (self.class_sign >> 24) & 0xFF;
        let sign = (self.class_sign >> 16) & 0x01;

        Dfp {
            class: match class {
                0 => DfpClass::Normal,
                1 => DfpClass::Infinity,
                2 => DfpClass::NaN,
                3 => DfpClass::Zero,
                _ => DfpClass::NaN,
            },
            sign: sign != 0,
            mantissa: u128::from_be(self.mantissa),
            exponent: i32::from_be(self.exponent),
        }
    }

    /// Canonical serialization for Merkle tree (24 bytes)
    pub fn to_bytes(&self) -> [u8; 24] {
        let mut bytes = [0u8; 24];
        bytes[..16].copy_from_slice(&self.mantissa.to_be_bytes());
        bytes[16..20].copy_from_slice(&self.exponent.to_be_bytes());
        bytes[20..24].copy_from_slice(&self.class_sign.to_be_bytes());
        bytes
    }
}

/// Node capability flags for DFP execution
pub struct NodeCapabilities {
    /// DFP spec version for replay pinning (Mission 5)
    /// Nodes must advertise which version of DFP spec they use for consensus
    pub dfp_spec_version: u32,
}
```

> ⚠️ **ARCHITECTURE CHANGE**: Hardware fast-path has been **removed**. DFP now uses **pure integer arithmetic** only. The CPU accelerates 128-bit integer operations, not floating-point. This ensures true determinism across all hardware.

### APIs/Interfaces

```rust
impl Dfp {
    /// Arithmetic operations using pure integer math
    pub fn add(self, other: Self) -> Self;
    pub fn sub(self, other: Self) -> Self;
    pub fn mul(self, other: Self) -> Self;
    pub fn div(self, other: Self) -> Self;
}
```

#### Addition Algorithm (Deterministic Specification)

For deterministic execution, all implementations must use this exact algorithm:

```
DFP_ADD(a, b):
    1. If a.class != Normal or b.class != Normal:
       → Handle special values per class rules
       // Note: Infinity inputs unreachable (overflow saturates to MAX); treat as NaN if encountered

    // Signed-zero arithmetic per IEEE-754-2019 §6.3
    // Zero operands preserve sign through the operation
    1b. Signed-zero rules (apply before main addition):
        - If a.class == Zero AND b.class == Zero:
            - a.sign == b.sign: result.sign = a.sign (same signs preserve)
            - a.sign != b.sign: result.sign = false (positive wins under RNE per IEEE-754 §6.3)
            - Return Zero with computed sign
        - If exactly one operand is Zero:
            - Result takes the sign of the non-zero operand
            - Return Zero with that sign

    2. Align exponents:
       → diff = a.exponent - b.exponent
       → Shift mantissa with smaller exponent by |diff|
    3. Add/Subtract mantissas (respecting sign):
       → result_mantissa = a.mantissa +/- b.mantissa
    4. Apply round-to-nearest-even to precision cap (113 bits)
    5. Normalize: ensure mantissa is odd (mantissa % 2 == 1)
    6. Return result
```

#### Multiplication Algorithm

```
DFP_MUL(a, b):
    1. Handle special values (NaN, Infinity, Zero)
       // Note: Infinity inputs unreachable (overflow saturates to MAX); treat as NaN if encountered

    // Signed-zero arithmetic per IEEE-754-2019 §6.3
    // Zero × anything = Zero with sign = a.sign XOR b.sign
    1b. If a.class == Zero OR b.class == Zero:
        result_sign = a.sign XOR b.sign  // XOR determines sign: -0 * + = -0, etc.
        Return Zero with result_sign

    2. result_sign = a.sign XOR b.sign
    3. result_exponent = a.exponent + b.exponent

    // 113-bit × 113-bit = up to 226-bit intermediate
    // Use 256-bit multiplication (two u128s)
    product = (a.mantissa as u256) * (b.mantissa as u256)

    // CRITICAL: Align intermediate for round_to_113
    // Find MSB position in 256-bit product (0-255)
    product_msb = 255 - product.leading_zeros()

    // Shift product right so MSB is at bit 112:
    // - Bit 112 becomes the result LSB (of the 113 we keep)
    // - Bit 113 becomes the round bit
    // - Bits shifted off the right end become the sticky bit
    shift_amount = product_msb.saturating_sub(112)
    aligned = product >> shift_amount

    // Apply RNE rounding from 128 to 113 bits
    // Pass aligned value to round_to_113
    (result_mantissa, exp_adj) = round_to_113(aligned as i128)
    result_exponent += exp_adj + shift_amount as i32

    4. Normalize (ensure odd mantissa)
    5. Return
```

#### Division Algorithm (Deterministic Long Division)

```
DFP_DIV(a, b):
    1. Handle special values
       - If b == 0: return saturating MAX with sign
       // Note: Infinity inputs unreachable (overflow saturates to MAX); treat as NaN if encountered
    2. result_sign = a.sign XOR b.sign
    3. result_exponent = a.exponent - b.exponent

    // DETERMINISTIC LONG DIVISION using standard shift-and-subtract
    // Represent dividend as 256-bit: (dividend_hi, dividend_lo)
    // where initial: hi = a.mantissa, lo = 0
    dividend_hi = a.mantissa
    dividend_lo = 0u128

    // Quotient accumulator
    quotient = 0u128

    // Fixed 256 iterations for determinism
    for i in 0..256:
        // Shift dividend left by 1 (with carry between hi/lo)
        (dividend_hi, dividend_lo, carry) = shift_left_with_carry(
            dividend_hi, dividend_lo
        )

        // Shift quotient left by 1
        quotient = quotient << 1

        // Compare: dividend >= b.mantissa ?
        // (Compare 256-bit dividend against 128-bit b.mantissa positioned at top)
        if (dividend_hi > b.mantissa) OR
           (dividend_hi == b.mantissa AND dividend_lo > 0):
            // Subtract b.mantissa from dividend (dividend_hi is top 128 bits)
            dividend_hi = dividend_hi - b.mantissa
            // Set quotient bit
            quotient = quotient | 1
        // Else: quotient bit remains 0, dividend unchanged

    // quotient now has 256-bit precision
    // CRITICAL: Align quotient for round_to_113
    // Find MSB position in 256-bit quotient (0-255)
    quotient_msb = 255 - quotient.leading_zeros()

    // Shift quotient right so MSB is at bit 112:
    // - Bit 112 becomes the result LSB (of the 113 we keep)
    // - Bit 113 becomes the round bit
    // - Bits shifted off the right end become the sticky bit
    shift_amount = quotient_msb.saturating_sub(112)
    aligned = quotient >> shift_amount

    // Apply RNE rounding from aligned value to 113 bits
    (result_mantissa, exp_adj) = round_to_113(aligned as i128)
    result_exponent += exp_adj + shift_amount as i32

    4. Normalize (ensure odd mantissa)
    5. Return
```

#### Square Root Algorithm (Fixed 32 Iterations)

```
DFP_SQRT(a):
   1. Handle special values
       - NaN: return NaN
       - Negative: return NaN (invalid)
       - Zero: return Zero
       - Infinity: unreachable in compliant implementations (would saturate to MAX before this op); if encountered, return NaN

    // Note: DfpClass::Infinity inputs are unreachable in compliant implementations
    // because overflow saturates to MAX (class=Normal). If an implementation
    // produces Infinity somehow, treat as NaN per IEEE-754 conventions.

    2. Decompose: sqrt(mantissa * 2^exponent) = sqrt(mantissa) * 2^(exponent/2)

    // CRITICAL: Adjust mantissa for odd exponent BEFORE all computations
    // Use (exp != 0 for & 1) parity - works correctly for negative exponents
    // Use >> 1 for floor division (arithmetic right shift in two's complement)
    adjusted_mantissa = a.mantissa
    if (a.exponent & 1) != 0:  // Odd exponent (works for negative too)
        adjusted_mantissa = a.mantissa << 1
        exponent_quotient = a.exponent >> 1  // Floor division via right shift
    else:
        exponent_quotient = a.exponent >> 1

    // CRITICAL: Scale up to get 113-bit precision in result
    // To compute sqrt(x) to 113 bits, we compute sqrt(x * 2^226)
    // This shifts the result up by 113 bits, giving us 113 bits of precision
    // Use u512 arithmetic (or multi-word) for: scaled = adjusted_mantissa << 226
    scaled_input = adjusted_mantissa << 226  // Requires 354+ bits

    // Use bit-by-bit integer square root on the scaled input
    // This algorithm is deterministic and has no floating-point rounding issues
    result = 0u256
    // Start from bit 225 (113 bits of precision in the scaled space) down to 0
    for bit in (0..226).rev():
        // Try setting this bit in the result
        candidate = result | (1u256 << bit)
        // Check if candidate² ≤ scaled_input
        if candidate * candidate <= scaled_input:
            result = candidate

    // result now contains sqrt(adjusted_mantissa * 2^226) in 226-bit precision

    // Shift right by 113 to get the mantissa in normal range
    // This extracts the upper 113 bits, effectively dividing by 2^113
    // The scaling and unscaling cancel: sqrt(x * 2^226) / 2^113 = sqrt(x) * 2^113 / 2^113 = sqrt(x)
    result_mantissa = result >> 113

    // The exponent is simply the halved exponent from decomposition
    // (scaling/unscaling cancel exactly)
    result_exponent = exponent_quotient

    3. Normalize (ensure odd mantissa)
    4. Return

// Worked example: sqrt(2.0)
// Input: mantissa=1, exponent=1 (represents 1 * 2^1 = 2.0)
// Step 2: exponent % 2 = 1 (odd)
//   adjusted_mantissa = 1 << 1 = 2
//   exponent_quotient = 1 / 2 = 0
// Step 4: scaled_input = 2 << 226 = 2 * 2^226
// Bit-by-bit sqrt: result ≈ 2^113 * sqrt(2) ≈ 2^113 * 1.414...
//   result has 226 bits, approximately 0x1.6A09E667F3BCD... in hex
//   result_mantissa = result >> 113 ≈ sqrt(2) in 113-bit mantissa
//   result_mantissa ≈ 0x1.6A09E667F3BCD (113-bit odd mantissa)
//   result_exponent = 0
// Final: sqrt(2.0) ≈ mantissa * 2^0 = 1.41421356...
// Result: mantissa ≈ 0x1.6A09E667F3BCD (canonical 113-bit), exponent = 0 ✓
```

### Expression VM opcodes

pub enum VmOpcode {
// ... existing opcodes
OP_DFP_ADD,
OP_DFP_SUB,
OP_DFP_MUL,
OP_DFP_DIV,
}

/// Executor mode
pub enum ExecutorMode {
/// Standard SQL execution
Standard,
/// Deterministic execution (DFP required)
Deterministic,
}

````

### Canonicalization Rules

After every operation, DFP values are normalized to canonical form:

```rust
/// Canonical normalization algorithm
/// Uses O(1) trailing_zeros for constant-time normalization
/// Only applies to Normal class values
fn normalize(dfp: &mut Dfp) {
    // Only normalize normal values
    if dfp.class != DfpClass::Normal {
        return; // Infinity, NaN, Zero already in canonical form
    }

    // Handle zero - class Zero with sign preserved
    if dfp.mantissa == 0 {
        dfp.class = DfpClass::Zero;
        return;
    }

    // O(1) normalization using trailing zeros count
    let trailing = dfp.mantissa.trailing_zeros() as i32;
    dfp.mantissa >>= trailing;
    dfp.exponent = dfp.exponent.saturating_add(trailing);

    // Handle overflow - SATURATING ARITHMETIC (not Infinity)
    // This prevents NaN poisoning in subsequent calculations
    if dfp.exponent > DFP_MAX_EXPONENT {
        dfp.exponent = DFP_MAX_EXPONENT;
        dfp.mantissa = DFP_MAX_MANTISSA;  // Clamp to maximum value
        // Class remains Normal, NOT Infinity
    }

    // Handle underflow - saturate to Zero
    if dfp.exponent < DFP_MIN_EXPONENT {
        dfp.class = DfpClass::Zero;
        dfp.mantissa = 0;
        dfp.exponent = 0;
    }
}

/// Division by zero: saturate to MAX with sign preserved
fn div_by_zero(sign: bool) -> Dfp {
    Dfp {
        class: DfpClass::Normal,  // NOT Infinity!
        sign,
        mantissa: DFP_MAX_MANTISSA,
        exponent: DFP_MAX_EXPONENT,
    }
}
````

### Rounding Rules

All operations use **Round-to-Nearest-Even (RNE)** with a **Sticky Bit** for 113-bit precision:

**Internal Representation (128-bit for accuracy):**

- **Target:** 113-bit mantissa
- **Guard bits:** 15 bits (128 - 113)
- **Round bit:** Bit 113 (first guard bit)
- **Sticky bit:** OR of all bits beyond bit 113

```rust
/// Round 128-bit intermediate to 113-bit with sticky bit (RNE)
/// Input: 128-bit signed integer representing mantissa with guard bits
/// Output: (113-bit odd mantissa, exponent_adjustment)
/// NOTE: The exponent adjustment MUST be added to the result exponent
fn round_to_113(mantissa: i128) -> (u128, i32) {
    // CRITICAL: Handle zero input - trailing_zeros() on 0 returns 128
    // which would produce incorrect exponent adjustment
    if mantissa == 0 {
        return (0, 0);
    }

    // We work with absolute value for rounding logic
    let abs_mant = mantissa.unsigned_abs();

    // Bit layout (128 bits total):
    // [ bits 0-112: kept mantissa (113 bits) ]
    // [ bit 113: round bit ]
    // [ bits 114-127: sticky bits (14 bits) ]

    const ROUND_BIT_POS: u32 = 113;
    const STICKY_BITS: u32 = 14;  // bits 114-127

    // Extract round bit (bit 113)
    let round_bit = (abs_mant >> ROUND_BIT_POS) & 1;

    // Extract sticky bits (bits 114-127) - OR them together
    // Sticky = any bit set ABOVE the round bit (positions 114-127)
    let sticky_bit = (abs_mant >> (ROUND_BIT_POS + 1)) != 0;

    // Extract kept bits (lower 113 bits)
    let kept_bits = abs_mant & ((1u128 << ROUND_BIT_POS) - 1);

    // RNE: Round up if (round AND sticky) OR (round AND LSB=1 AND sticky=0)
    let lsb = kept_bits & 1;

    let rounded = if round_bit && (sticky_bit || lsb == 1) {
        kept_bits + 1  // Round up
    } else {
        kept_bits  // Truncate
    };

    // STEP 2: Normalize (ensure mantissa is odd)
    // After rounding, we may have even mantissa - shift and adjust exponent
    let trailing = rounded.trailing_zeros();
    let normalized = rounded >> trailing;

    // CRITICAL: Return both mantissa AND exponent adjustment
    // Shifting right by trailing zeros DECREASES the value, so we ADD to exponent
    (normalized, trailing as i32)
}

/// Normalize after rounding to ensure canonical odd mantissa
fn normalize_mantissa(mantissa: &mut u128, exponent: &mut i32) {
    if *mantissa == 0 {
        return;  // Zero - no normalization needed
    }

    // Ensure mantissa is odd (canonical form)
    let trailing = mantissa.trailing_zeros();
    *mantissa >>= trailing;
    *exponent = exponent.saturating_add(trailing as i32);
}
```

**RNE Table:**

| Scenario  | Round Bit | Sticky Bit | LSB (113th) | Result         |
| --------- | --------- | ---------- | ----------- | -------------- |
| 1.5       | 1         | 0          | 1           | Round UP → 2   |
| 2.5       | 1         | 0          | 0           | Round DOWN → 2 |
| 2.500...1 | 1         | 1          | 0           | Round UP → 3   |
| 3.5       | 1         | 0          | 1           | Round UP → 4   |

**Multi-step expressions:** RNE is applied after **every individual operation**. There are no fused paths. For example, `(a + b) * c` is computed as: `(a + b)` → round → then multiply → round. This ensures deterministic results regardless of evaluation order.

### Special Values

DFP uses **saturating arithmetic** — Infinity class is NOT produced in computed results. Instead, overflow saturates to MAX/MIN:

| Special Value | Class  | Sign | Mantissa     | Exponent | Behavior                         |
| ------------- | ------ | ---- | ------------ | -------- | -------------------------------- |
| NaN           | NaN    | -    | -            | -        | Canonical NaN, propagates        |
| +Overflow     | Normal | 0    | MAX_MANTISSA | MAX_EXP  | Saturates to +MAX (not Infinity) |
| -Overflow     | Normal | 1    | MAX_MANTISSA | MAX_EXP  | Saturates to -MAX (not Infinity) |
| +0.0          | Zero   | 0    | -            | -        | Distinct from -0.0               |
| -0.0          | Zero   | 1    | -            | -        | Distinct from +0.0               |
| Normal        | Normal | 0/1  | u128         | i32      | Standard value                   |

> **Design Decision:** DFP does NOT produce Infinity in computed results. Overflow saturates to MAX value (class=Normal). This prevents NaN poisoning chains where `Infinity - Infinity = NaN`.

**Conversion from f64:**

- NaN → canonical NaN (class=NaN)
- +Infinity → saturates to DFP_MAX (class=Normal)
- -Infinity → saturates to DFP_MIN (class=Normal)
- +0.0 → Zero (class=Zero, sign=0)
- -0.0 → Zero (class=Zero, sign=1)
- Normal → converted directly with RNE rounding
- Subnormal → see algorithm below

**Subnormal conversion algorithm:**

IEEE-754 double subnormals do NOT have an implicit leading 1-bit. Their value is `0.fraction × 2^(-1022)`, where `fraction` is the raw 52-bit significand field.

```
from_f64_subnormal(f64_bits):
    // f64 layout: [sign:1][exponent:11][fraction:52]
    sign = (f64_bits >> 63) & 1
    exponent = (f64_bits >> 52) & 0x7ff  // Always 0 for subnormal
    fraction = f64_bits & 0xfffffffffffff  // 52 bits

    // Detect subnormal: exponent field == 0 but fraction != 0
    if exponent == 0 && fraction != 0:
        // Find position of highest set bit in fraction
        leading_zeros = fraction.leading_zeros()  // 0-51
        significant_bits = 52 - leading_zeros  // 1-52

        // The subnormal value is: fraction × 2^(-1022 - (52 - significant_bits))
        // Equivalent to: (fraction << (52 - significant_bits)) × 2^(-1022)
        // Normalize: shift fraction left so the highest bit is at position 51 (the MSB)
        // After this shift: normalized_fraction.bit_length() == significant_bits
        normalized_fraction = fraction << (52 - significant_bits)  // Shift to normalize; highest bit at position 51

        // Effective exponent after normalization
        // Original: -1022 - (52 - significant_bits)
        // After shifting: -1022 - 52 + significant_bits = -(1022 + 52 - significant_bits)
        effective_exponent = -1074 + significant_bits  // -1074 to -1023

        // Now we have a normal-like representation: normalized_fraction × 2^effective_exponent
        // Convert to DFP: normalize to 113-bit mantissa with RNE
        (dfp_mantissa, exp_adj) = round_to_113(normalized_fraction as i128)
        dfp_exponent = effective_exponent + exp_adj

        // Apply sign
        return Dfp { class: Normal, sign, mantissa: dfp_mantissa, exponent: dfp_exponent }
```

### Range and Precision

DFP provides higher precision than IEEE-754 double:

| Characteristic | DFP (Effective)    | IEEE-754 Double |
| -------------- | ------------------ | --------------- |
| Mantissa bits  | 113 (internal 128) | 53 (implicit)   |
| Exponent bits  | 11                 | 11              |
| Decimal digits | ~34                | ~15-17          |
| Exponent range | ±1023              | ±1023           |
| MAX value      | ~1.7×10³⁰⁸         | ~1.8×10³⁰⁸      |
| MIN positive   | ~2.2×10⁻³⁰⁸        | ~2.2×10⁻³⁰⁸     |

> **Note:** Internal storage uses 128-bit u128, but effective precision is capped at **113 bits** to ensure stable f64 round-trips and maintain the canonical odd-mantissa invariant.

**Canonical mantissa invariant:** For all Normal values: `mantissa % 2 == 1` (mantissa is always odd). This ensures unique canonical encoding.

**Constants:**

```rust
pub const DFP_MAX_EXPONENT: i32 = 1023;
pub const DFP_MIN_EXPONENT: i32 = -1074;

/// Maximum finite DFP value (113-bit odd mantissa at max exponent)
/// Value: (2^113 - 1) × 2^1023 ≈ 1.7 × 10^308
pub const DFP_MAX_MANTISSA: u128 = (1u128 << 113) - 1;  // All 113 bits set (odd)

pub const DFP_MAX: Dfp = Dfp {
    class: DfpClass::Normal,
    sign: false,
    mantissa: DFP_MAX_MANTISSA,
    exponent: DFP_MAX_EXPONENT,
};

/// Minimum positive DFP value
pub const DFP_MIN: Dfp = Dfp {
    class: DfpClass::Normal,
    sign: false,
    mantissa: 1,
    exponent: DFP_MIN_EXPONENT,
};

/// Canonical NaN (all NaN values collapse to this)
pub const DFP_CANONICAL_NAN: Dfp = Dfp {
    class: DfpClass::NaN,
    sign: false,
    mantissa: 0,
    exponent: 0,
};

/// Infinity class is reserved for completeness but NEVER produced by arithmetic.
/// All overflow saturates to MAX value (class=Normal).
/// Only used in from_f64() conversion before saturation.
/// This constant exists only for completeness - DO NOT use in computations.
#[allow(dead_code)]
pub const DFP_POS_INFINITY: Dfp = Dfp {
    class: DfpClass::Infinity,
    sign: false,
    mantissa: 0,
    exponent: 0,
};

/// @hidden - see DFP_POS_INFINITY
#[allow(dead_code)]
pub const DFP_NEG_INFINITY: Dfp = Dfp {
    class: DfpClass::Infinity,
    sign: true,
    mantissa: 0,
    exponent: 0,
};
```

### SQL Integration

```sql
-- Deterministic table
CREATE TABLE trades (
    id INTEGER PRIMARY KEY,
    price DFP NOT NULL,
    quantity DFP,
    executed_at TIMESTAMP
);

-- Deterministic view (enforces DFP)
CREATE DETERMINISTIC VIEW v_portfolio AS
SELECT
    price * quantity AS total
FROM trades;

-- Explicit casting (required for mixed arithmetic)
SELECT
    CAST(price AS DFP) * CAST(quantity AS DFP)
FROM trades;

-- Error: cannot mix DFP and FLOAT
SELECT price * quantity FROM trades;
-- Error: cannot mix DFP and FLOAT
```

### CAST Safety in Deterministic Contexts

> ⚠️ **CRITICAL**: Casting FLOAT/DOUBLE to DFP in deterministic contexts is **FORBIDDEN** because FLOAT values may differ across platforms.

```sql
-- FORBIDDEN: FLOAT values may be non-deterministic across nodes
SELECT CAST(price AS DFP) FROM trades;  -- Error in deterministic context

-- Solution: Use DFP from the start
CREATE TABLE trades (price DFP NOT NULL);
```

**Rationale:** If Node A stores `0.30000000000000004` in a FLOAT column and Node B stores `0.3` for the same logical value, casting to DFP produces different results, breaking consensus determinism.

### Deterministic Context Rules

Inside deterministic execution contexts (blockchain state transitions, deterministic views):

```
FLOAT  → FORBIDDEN
DOUBLE → FORBIDDEN
DFP    → ALLOWED
DECIMAL → ALLOWED
INTEGER → ALLOWED
INT    → ALLOWED (implicit to DFP)
```

**Type Promotion Rules for Mixed-Type Expressions:**

| Left Type | Right Type | Promotion Behavior                                          |
| --------- | ---------- | ----------------------------------------------------------- |
| DFP       | DFP        | Direct DFP operation                                        |
| DFP       | INT        | Integer implicitly promoted to DFP via `Dfp::from_i64(int)` |
| INT       | DFP        | Integer implicitly promoted to DFP via `Dfp::from_i64(int)` |
| DFP       | FLOAT      | **FORBIDDEN** — requires explicit `CAST`                    |
| FLOAT     | DFP        | **FORBIDDEN** — requires explicit `CAST`                    |

**Rationale for INT → DFP implicit promotion:** Integer values have exact representations in DFP (any integer up to 2^113 can be represented exactly as a DFP mantissa with exponent 0). This differs from FLOAT, where implicit conversion could introduce platform-dependent rounding. Therefore, `dfp_col * 2` is allowed implicitly, but `dfp_col * 1.5` requires `CAST(1.5 AS DFP)`.

### Execution Paths

```
DFP Operation
    │
    └─[Software Path]─→ Deterministic 128-bit integer arithmetic → DFP
```

> ⚠️ **ARCHITECTURE**: Hardware fast-path has been **removed**. DFP uses **pure integer arithmetic** (i128 operations) only. The CPU accelerates 128-bit integer operations, not floating-point. This ensures true determinism across x86, ARM, RISC-V, and virtualized environments.

### Execution Verification

DFP uses software-only deterministic arithmetic. Verification ensures the implementation matches the specification:

> ⚠️ **NOTE**: Verification suite is abbreviated. Full verification requires 265+ test vectors across edge cases, subnormal handling, and cross-platform validation. See `determ/verify/test_vectors.rs` for complete suite.

```rust
/// Verification test vectors
/// NOTE: This is a developer smoke test only. For consensus-grade verification,
/// use VERIFICATION_PROBE (24-byte byte comparison) instead.
/// These tests compare DFP.to_f64() against f64 approximations, which cannot
/// detect errors smaller than ~1 ULP in f64 (~2^50 ULPs in DFP space).
const VERIFICATION_TESTS: &[(&str, f64)] = &[
    ("0.1 + 0.2", 0.3),
    ("sqrt(2)", 1.4142135623730951),
    ("1e300 * 1e-300", 1.0),
];
```

> **Note**: `sin`, `cos`, `log`, `exp` are excluded from initial verification because transcendental functions are deferred to Mission 1b.
>
> **Authoritative Verification**: Use `VERIFICATION_PROBE` (defined above) for consensus-grade verification. It compares full 24-byte DfpEncoding serialization against known-correct reference values.

#### Continuous Verification

To ensure ongoing deterministic behavior:

| Mechanism                | Description                                   |
| ------------------------ | --------------------------------------------- |
| Periodic re-verification | Re-run probe tests every N blocks             |
| Cross-node spot-checks   | Randomly compare DFP results across nodes     |
| Divergence alerts        | Flag and investigate unexpected differences   |
| Slashing conditions      | Penalize nodes producing inconsistent results |

#### Compiler Flags for Reproducibility

To ensure deterministic software-path execution, nodes must compile with specific flags:

| Platform | Required Flags                                             |
| -------- | ---------------------------------------------------------- |
| x86      | `-C target-feature=+sse2` (disable x87 extended precision) |
| ARM      | Standard AAPCS (deterministic by default)                  |
| All      | Use `release` profile (overflow checks off by default)     |

> **Note:** Rust's `release` profile disables integer overflow checks. Do NOT use `debug` profile for DFP operations. For `overflow-checks = true` in any profile, wrap semantics are defined (`wrapping_add`, etc.).

> ⚠️ **Virtualized environments**: Hardware fast-path is NOT permitted. All nodes must use the software path.

### Storage Encoding

DFP values serialize deterministically using **one canonical path**:

```rust
impl Serialize for Dfp {
    fn serialize(&self) -> Vec<u8> {
        // CRITICAL: Use DfpEncoding::to_bytes() for Merkle compatibility
        // This ensures identical byte layout across all implementations
        let encoding = DfpEncoding::from_dfp(self);
        encoding.to_bytes().to_vec()  // 24 bytes, big-endian
    }
}
```

> **Critical:** All implementations MUST use `DfpEncoding::to_bytes()` for serialization. This produces 24-byte canonical layout: `[mantissa: 16][exponent: 4][class_sign: 4]`.

### Gas Limits Scope

| Limit                      | Scope           | Notes                                |
| -------------------------- | --------------- | ------------------------------------ |
| Max DFP ops per block      | Per-transaction | 10,000 per tx                        |
| Max DFP_DIV/SQRT per block | Per-transaction | 1,000 per tx                         |
| Interaction with block gas | N/A             | DFP ops are charged as compute units |

> **Unit definition:** One "op" = one DFP opcode execution (DFP_ADD, DFP_MUL, etc.). A complex expression like `(a + b) * c` counts as 3 ops.

### Constraints

- **Determinism**: All nodes must produce bit-identical DFP results
- **Explicit types**: No implicit FLOAT → DFP conversion
- **Type mixing**: Forbidden without explicit CAST
- **Canonical form**: Every value has exactly one representation
- **Range**: Exponent bounded to prevent overflow/underflow
- **Sign handling**: -0.0 preserved for scientific accuracy; normalized to +0.0 only when mathematically equivalent
- **Gas cost**: DFP operations must be charged higher than integer operations (see Gas/Fee Modeling)

### Gas/Fee Modeling

DFP software arithmetic is significantly slower than native integer operations. Gas costs reflect true computational cost to prevent resource exhaustion attacks:

| Operation    | Relative Gas Cost | Notes                                    |
| ------------ | ----------------- | ---------------------------------------- |
| INT_ADD      | 1x (baseline)     | Native                                   |
| DFP_ADD      | 6-10x             | 128-bit + normalization                  |
| DFP_MUL      | 10-15x            | 128-bit multiplication                   |
| **DFP_DIV**  | **50-100x**       | Iterative algorithm                      |
| **DFP_SQRT** | **50-100x**       | Bit-by-bit integer sqrt (226-bit scaled) |
| DFP_FROM_I64 | 2x                | Conversion                               |
| DFP_TO_I64   | 2x                | Conversion                               |
| DFP_FROM_F64 | 4-6x              | Canonicalization                         |
| DFP_TO_F64   | 3-4x              | Roundtrip                                |

**Rationale:** Software DFP uses 128-bit arithmetic with normalization loops. Division and square root require iterative algorithms (16-32 iterations minimum). The 50-100x multiplier prevents DoS attacks via computationally dense DFP operations.

**Resource Exhaustion Protection:**

- Max DFP ops per transaction: 10,000
- Max DFP_DIV/DFP_SQRT per transaction: 1,000
- Exceeding limits → transaction rejected

### Deterministic Ordering

DFP defines total ordering for sorting and comparison operations:

| Order | Class     | Sign | Mantissa   | Exponent   |
| ----- | --------- | ---- | ---------- | ---------- | ------ |
| 1     | -Infinity | 1    | -          | -          |
| 2     | Zero      | 1    | -          | -          | (-0.0) |
| 3     | Normal    | 1    | descending | descending |
| ...   | ...       | ...  | ...        | ...        |
| N-2   | Normal    | 0    | ascending  | ascending  |
| N-1   | Zero      | 0    | -          | -          | (+0.0) |
| N     | +Infinity | 0    | -          | -          |
| N+1   | NaN       | -    | -          | -          |

**Total ordering:** `-Infinity < -0.0 < negative values < +0.0 < positive values < +Infinity < NaN`

> **Note:** For **equality comparison** (`WHERE col = 0`): `-0.0 == +0.0` returns TRUE.
> For **ordering comparison** (`ORDER BY`, `<`, `>`): `-0.0 < +0.0` returns TRUE.
> This matches IEEE-754 behavior.

> **DFP Note:** Since DFP uses saturating arithmetic, Infinity class never appears in computed results. Overflow saturates to Normal(MAX_MANTISSA, MAX_EXP). The ordering table includes Infinity for completeness but it will not be produced by arithmetic operations.

**Sorting algorithm:**

```
compare(a, b):
    // CRITICAL: Combined (sign, class) ordering for correct total order
    // Order: -Infinity < -Normal < -Zero < +Zero < +Normal < +Infinity < NaN

    // 1. NaN always compares greater (sorts to end)
    if a.class == NaN && b.class == NaN:
        return false  // NaN == NaN
    if a.class == NaN:
        return false  // NaN > anything
    if b.class == NaN:
        return true   // anything < NaN

    // 2. Compute combined order key: (sign, class) where:
    //    sign: 0 = positive, 1 = negative
    //    class: Infinity=0, Normal=1, Zero=2
    //    Lower value = more negative = comes first
    fn order_key(x: Dfp) -> (u8, u8) {
        let class_ord = match x.class {
            DfpClass::Infinity => 0,
            DfpClass::Normal => 1,
            DfpClass::Zero => 2,
            DfpClass::NaN => 3,  // Never reached (handled above)
        };
        let sign_ord = if x.sign { 1 } else { 0 };  // negative = 1 comes first
        (sign_ord, class_ord)
    }

    let a_key = order_key(a);
    let b_key = order_key(b);

    if a_key != b_key:
        return a_key < b_key

    // 3. Same (sign, class) - compare magnitude for Normal
    if a.class == Normal:
        // CRITICAL: Must be sign-aware for correct ordering
        if a.sign {  // Negative: larger magnitude = smaller value
            if a.exponent != b.exponent:
                return a.exponent > b.exponent  // Higher exponent = larger magnitude = smaller value
            return a.mantissa > b.mantissa     // Larger mantissa = smaller value
        } else {      // Positive: larger magnitude = larger value
            if a.exponent != b.exponent:
                return a.exponent < b.exponent  // Higher exponent = larger magnitude
            return a.mantissa < b.mantissa
        }

    // 4. Zero: already handled by sign in order_key
    // 5. Infinity: already handled by sign in order_key
    return false  // Equal
```

### Error Handling

| Scenario                       | Behavior                    |
| ------------------------------ | --------------------------- |
| FLOAT in deterministic context | Compile error               |
| DFP \* FLOAT                   | Compile error (use CAST)    |
| DFP overflow                   | Clamp to MAX/MIN            |
| DFP underflow                  | Clamp to 0.0                |
| Hardware verification fail     | Silent fallback to software |

## Rationale

### Why This Approach?

The two-tier model balances:

- **Performance**: Analytics use fast native floats
- **Safety**: Consensus requires explicit determinism
- **Compatibility**: Standard SQL float types preserved

### Risk Assessment

This RFC represents an ambitious attempt to bring floating-point into consensus-critical execution. Industry practice overwhelmingly avoids this due to significant challenges:

| Risk                          | Severity | Mitigation                                                        |
| ----------------------------- | -------- | ----------------------------------------------------------------- |
| Hardware determinism fragile  | High     | Software-only path default; hardware opt-in only after validation |
| Software fallback 3-6× slower | Medium   | Acceptable for limited consensus operations                       |
| Transcendental functions      | High     | Deferred to Mission 1b; full AI workloads limited until then      |
| Verification probe coverage   | Medium   | Expand test vectors; continuous cross-node checks                 |
| Exponent overflow/underflow   | Medium   | Align with IEEE double range (±1023)                              |
| NaN/denormal handling         | Medium   | Canonical forms; clear documentation                              |

**Industry Comparison:**

| Chain            | Consensus FP? | Approach              |
| ---------------- | ------------- | --------------------- |
| Ethereum EVM     | No            | 256-bit integers only |
| Solana SVM       | Emulated      | Software FP (slow)    |
| Cosmos SDK       | No            | Fixed-point decimals  |
| CipherOcto (DFP) | Yes (opt-in)  | Custom binary FP      |

This RFC makes CipherOcto a potential outlier. The experimental warning reflects genuine technical risk.

### Arithmetic Properties

The deterministic floating-point (DFP) format provides **bit-identical deterministic arithmetic**, but it does **not change fundamental floating-point algebraic limitations**.

As with IEEE-754 arithmetic, certain mathematical identities do **not hold exactly** due to rounding and exponent alignment.

#### Associativity

Addition is **not associative**.

```
(a + b) + c ≠ a + (b + c)
```

Example:

```
a = 1e20
b = -1e20
c = 3
```

Evaluation order:

```
(a + b) + c
= (1e20 - 1e20) + 3
= 3
```

```
a + (b + c)
= 1e20 + (-1e20 + 3)
= 1e20 + (-1e20)
= 0
```

The difference occurs because:

- `b + c` loses precision when aligning exponents
- rounding occurs during intermediate operations

> **Note: Addition associativity is intentionally excluded from required arithmetic invariants.**

#### Guaranteed Properties

The following properties **are guaranteed** under DFP:

**Determinism**: All compliant implementations must produce **bit-identical results**.

```
f(a, b) = identical bits on all platforms
```

**Canonical Representation**: All results are normalized such that:

```
mantissa is odd
value = mantissa × 2^exponent
```

**Deterministic Rounding**: Rounding behavior is fully specified and independent of:

- CPU architecture
- compiler optimizations
- hardware floating-point units

**Algebraic Identities That Hold** (when no overflow/underflow):

| Property                     | Formula                     |
| ---------------------------- | --------------------------- |
| Multiplicative associativity | `(a × b) × c = a × (b × c)` |
| Multiplicative identity      | `a × 1 = a`                 |
| Additive identity            | `a + 0 = a`                 |
| Inverse property             | `a − a = 0`                 |

#### Properties Explicitly Not Guaranteed

| Property                  | Reason                             |
| ------------------------- | ---------------------------------- |
| Addition associativity    | rounding during exponent alignment |
| Subtraction associativity | same reason                        |
| Distributivity            | rounding after multiplication      |

Example:

```
a × (b + c) ≠ a×b + a×c
```

#### Testing Guidance

Because of the above properties, the test suite **must not assert associativity** for addition or subtraction.

Recommended patterns:

- **Stable identities**: `a + 0 = a`, `a × 1 = a`, `a − a = 0`
- **Round-trip checks**: `(a × b) ÷ b ≈ a`
- **Determinism checks**: `f(a,b)` executed twice produces identical bits

#### Determinism Hazards and Mitigations

Deterministic Floating Point (DFP) is designed to produce **bit-identical results across architectures and compilers**. However, several common compiler and platform behaviors can break determinism if not controlled.

##### 1. Floating-Point Hardware Execution

**Hazard:** Native floating-point hardware may introduce:

- extended precision registers
- fused multiply-add (FMA)
- platform-dependent rounding
- hidden intermediate precision

These behaviors differ across x86, ARM, RISC-V, and WASM.

**Mitigation:** DFP arithmetic **must not rely on hardware floating-point instructions** for core arithmetic operations. All operations must use integer arithmetic with explicit normalization and deterministic rounding.

##### 2. Fused Multiply-Add (FMA)

**Hazard:** Compilers may replace `a * b + c` with `fma(a,b,c)`, performing multiplication and addition in a single rounding step, which changes results.

**Mitigation:** Disable FMA contraction:

| Compiler  | Flag                     |
| --------- | ------------------------ |
| GCC/Clang | `-ffp-contract=off`      |
| Rust      | `-C target-feature=-fma` |
| MSVC      | `/fp:strict`             |

##### 3. Compiler Reordering

**Hazard:** Compilers may reorder operations like `(a + b) + c` → `a + (b + c)`, changing results due to non-associativity.

**Mitigation:** Disable auto-vectorization and fast-math flags. Prohibited: `-ffast-math`, `-Ofast`.

##### 4. Undefined Integer Behavior

**Hazard:** Signed integer overflow, shifting by ≥ word size, and negative shifts have undefined behavior.

**Mitigation:** Use checked arithmetic and explicit overflow handling:

```rust
let (m, overflow) = a.mantissa.overflowing_mul(b.mantissa);
if overflow { return Err(DfpError::Overflow); }
```

##### 5. Platform Word Size

**Hazard:** 32-bit, 64-bit, and 128-bit native integers can cause implicit widening/narrowing inconsistencies.

**Mitigation:** Explicitly define all numeric widths:

```
mantissa: i128
exponent: i32
```

##### 6. Endianness

**Hazard:** Little vs big endian can misinterpret serialized values.

**Mitigation:** Use fixed byte order (little-endian) in serialization:

```
struct { int128 mantissa, int32 exponent }
```

##### 7. Non-Deterministic Math Libraries

**Hazard:** Standard math libraries (libm, libc) use platform-specific implementations for sqrt(), pow(), exp().

**Mitigation:** All mathematical operations must be implemented within the DFP library itself.

##### 8. Parallel Execution

**Hazard:** Parallel reductions and SIMD can change evaluation order.

**Mitigation:** DFP arithmetic must execute in deterministic sequential order with fixed reduction trees.

##### 9. Compiler Optimization Levels

**Hazard:** Certain optimization levels enable floating-point transformations.

**Mitigation:** Use:

```
-O2
-ffp-contract=off
-fno-fast-math
```

##### Determinism Compliance Checklist

| Requirement                      | Status   |
| -------------------------------- | -------- |
| No hardware FP for arithmetic    | Required |
| Explicit integer widths          | Required |
| Canonical normalization enforced | Required |
| Deterministic rounding           | Required |
| FMA disabled                     | Required |
| Fast-math disabled               | Required |
| Stable serialization format      | Required |

##### Verification Strategy

- **Cross-platform testing**: Execute test vectors on x86_64, ARM64, RISC-V, WASM - all must match bit-for-bit
- **Deterministic test vectors**: Minimum 500 reference vectors covering add, sub, mul, div, sqrt, normalization
- **Reproducibility testing**: `f(a,b)` executed multiple times must produce identical bits

##### Security Considerations

Non-deterministic arithmetic may lead to:

- consensus failure in distributed systems
- divergent blockchain state
- inconsistent simulation outcomes

Deterministic arithmetic is a **consensus-critical component** and must be treated as such.

#### Cross-Language Arithmetic Verifier

The cross-language verifier ensures the Rust DFP implementation produces results consistent with a mathematically correct reference model using arbitrary precision integers.

##### Architecture

```
Test Vector Generator
        ↓
Reference Model (Python big integer math)
        ↓
Rust Implementation
        ↓
Result Comparator
```

All implementations operate on the same serialized DFP representation:

```
struct {
    int128 mantissa
    int32 exponent
}
```

##### Python Reference Implementation

The Python model uses unlimited precision integers to ensure exact mathematical values before canonicalization:

```python
from dataclasses import dataclass

@dataclass
class Dfp:
    mantissa: int
    exponent: int

def normalize(m, e):
    if m == 0:
        return Dfp(0, 0)
    while m % 2 == 0:
        m //= 2
        e += 1
    return Dfp(m, e)

def dfp_add(a: Dfp, b: Dfp) -> Dfp:
    if a.mantissa == 0: return b
    if b.mantissa == 0: return a
    if a.exponent > b.exponent:
        shift = a.exponent - b.exponent
        m = a.mantissa + (b.mantissa >> shift)
        e = a.exponent
    else:
        shift = b.exponent - a.exponent
        m = (a.mantissa >> shift) + b.mantissa
        e = b.exponent
    return normalize(m, e)

def dfp_mul(a: Dfp, b: Dfp) -> Dfp:
    return normalize(a.mantissa * b.mantissa, a.exponent + b.exponent)

DIV_PRECISION = 256

def dfp_div(a: Dfp, b: Dfp) -> Dfp:
    num = a.mantissa << DIV_PRECISION
    m = num // b.mantissa
    e = a.exponent - b.exponent - DIV_PRECISION
    return normalize(m, e)

def dfp_sqrt(x: Dfp) -> Dfp:
    import math
    m = x.mantissa << 256
    root = math.isqrt(m)
    e = (x.exponent - 256) // 2
    return normalize(root, e)
```

##### Rust CLI Interface

```rust
fn main() {
    let args: Vec<String> = env::args().collect();
    let op = &args[1];
    let a = Dfp { mantissa: args[2].parse().unwrap(), exponent: args[3].parse().unwrap() };
    let b = Dfp { mantissa: args[4].parse().unwrap(), exponent: args[5].parse().unwrap() };
    let result = match op.as_str() {
        "add" => dfp_add(a, b),
        "mul" => dfp_mul(a, b),
        "div" => dfp_div(a, b),
        "sqrt" => dfp_sqrt(a),
        _ => panic!("unknown op"),
    };
    println!("{} {}", result.mantissa, result.exponent);
}
```

##### Verification Scale

| Level                 | Tests             |
| --------------------- | ----------------- |
| Basic CI              | 10,000 vectors    |
| Pre-release           | 100,000 vectors   |
| Production validation | 1,000,000 vectors |

##### CI Integration

```bash
cargo build --release
python verifier.py
```

Recommended CI matrix: x86_64-linux, arm64-linux, macOS, wasm

### Alternatives Considered

| Alternative            | Pros          | Cons                                    | Rejection Reason                         |
| ---------------------- | ------------- | --------------------------------------- | ---------------------------------------- |
| Auto-convert FLOAT→DFP | Seamless      | Hidden semantics, consensus risk        | Unacceptable for safety-critical systems |
| Deprecate FLOAT        | Clean         | Breaks SQL compatibility, vector search | Unrealistic — fighting the ecosystem     |
| Fixed-point only       | Deterministic | Poor scientific workloads               | Loses the AI-native value proposition    |
| BigFloat               | Precise       | Extremely slow, VM impractical          | Performance unacceptable                 |
| IEEE-754 only          | Fast          | Non-deterministic                       | Unsafe for consensus                     |

### Trade-offs

| Priority     | Trade-off                       |
| ------------ | ------------------------------- |
| Prioritize   | Determinism, explicit types     |
| Deprioritize | Implicit conversion convenience |
| Accept       | 3-6x slower software fallback   |
| Accept       | Explicit casting required       |

## Implementation

### Mission 1: DFP Core Type

- Location: `determ/dfp.rs`
- Acceptance criteria:
  - [ ] DFP struct with mantissa/exponent
  - [ ] Canonical normalization
  - [ ] Arithmetic: add, sub, mul, div
  - [ ] Round-to-nearest-even
  - [ ] Special values: NaN, ±Infinity, ±0.0 handling
  - [ ] Range bounds and overflow/underflow clamping
  - [ ] From/To f64 conversion
  - [ ] Serialization
  - [ ] sqrt (square root) - bit-by-bit integer sqrt with 226-bit scaled input
  - [ ] **Test vectors: 500+ verified cases** including edge cases
  - [ ] **Differential fuzzing** against Berkeley SoftFloat reference
- Estimated complexity: Medium

> **Prerequisite before consensus integration:** At least 300 passing test vectors + differential fuzzing report.

### ⚠️ Three Golden Rules for Implementation

> **CRITICAL:** These rules must be followed exactly to ensure deterministic execution:

1. **Intermediate u256 for Division:** In `DFP_DIV`, when shifting `a.mantissa << 128`, you MUST use a 256-bit intermediate (or two u128s). Using u128 will shift bits to zero.

2. **No f64 for SQRT Seed:** The initial approximation for SQRT must use bit-by-bit integer sqrt. Using `f64::sqrt(x)` as a seed is FORBIDDEN — it introduces non-determinism.

3. **No Iteration Short-Circuiting:** Execute ALL iterations as specified (256 for division, 226 for SQRT). Compilers must NOT elide "useless" iterations via "fast-math" flags.

### Mission 1b: Additional Transcendental Functions (Future Phase)

Deterministic transcendental functions require fixed-iteration algorithms with bounded precision:

| Phase | Functions        | Algorithm                      | Status |
| ----- | ---------------- | ------------------------------ | ------ |
| 1b.1  | cbrt             | Newton-Raphson (16 iterations) | Future |
| 1b.2  | sin, cos, tan    | CORDIC or Chebyshev (bounded)  | Future |
| 1b.3  | log, log2, log10 | Series expansion               | Future |
| 1b.4  | exp, pow         | Series expansion               | Future |

**Determinism requirements for transcendental functions:**

- Fixed iteration count (no early termination)
- Deterministic initial approximations
- Bounded precision guarantees
- Round-to-nearest-even at each step

**Note:** sqrt is included in Mission 1 to support basic AI workloads (e.g., distance calculations, normalization). Full transcendental support is deferred to future phases.

### Mission 2: DataType Integration

- Location: `src/parser/ast.rs`, `src/parser/statements.rs`
- Acceptance criteria:
  - [ ] Add `DataType::DeterministicFloat` variant
  - [ ] SQL parser accepts `DFP` type
  - [ ] CAST(... AS DFP) parsing
  - [ ] Type error for FLOAT in deterministic context
- Estimated complexity: Low

### Mission 3: Expression VM Opcodes

- Location: `src/vm/`
- Acceptance criteria:
  - [ ] OP_DFP_ADD, OP_DFP_SUB, OP_DFP_MUL, OP_DFP_DIV
  - [ ] Compile error on DFP \* FLOAT without CAST
  - [ ] DeterministicExecutor mode
- Estimated complexity: Medium

### Mission 4: Hardware Verification

- Location: `determ/probe.rs`
- Acceptance criteria:
  - [ ] DeterministicFloatProbe test suite
  - [ ] Node capability advertisement
  - [ ] Automatic fallback on verification failure
  - [ ] Comprehensive test vectors (edge cases, cross-platform)
- Estimated complexity: Low

#### Test Vector Suite

Comprehensive verification requires extensive test vectors:

| Category           | Test Cases                       | Count |
| ------------------ | -------------------------------- | ----- |
| Basic arithmetic   | add, sub, mul, div edge cases    | ~50   |
| Special values     | NaN, ±Infinity, ±0.0             | ~20   |
| Overflow/underflow | MAX, MIN boundaries              | ~30   |
| Precision loss     | Decimal precision comparisons    | ~25   |
| Transcendental     | sqrt, sin, cos, log, exp         | ~40   |
| Cross-platform     | Same inputs across architectures | ~100  |

### Consensus Verification Probe

Every 100,000 blocks, nodes should execute a **Deterministic Sanity Check** to detect compiler bugs, CPU microcode errors, or VM implementation flaws:

```rust
/// Probe operation type
enum ProbeOp {
    Add,
    Sub,
    Mul,
    Div,
    Sqrt,
}

/// Probe test entry with full specification
struct ProbeEntry {
    op: ProbeOp,
    a: Dfp,           // First operand (fully specified Dfp)
    b: Option<Dfp>,   // Second operand (None for unary ops like sqrt)
    expected_bytes: [u8; 24],  // Canonical 24-byte to_bytes() output
}

/// Hard-coded DFP verification probe entries
/// Each entry specifies: operation, operands, and expected 24-byte output
/// Expected bytes derived from reference implementation (Berkeley SoftFloat)
///
/// DfpEncoding layout (24 bytes total):
///   Bytes 0-15:  mantissa (u128, big-endian)
///   Bytes 16-19: exponent (i32, big-endian)
///   Bytes 20-23: class_sign (u32, big-endian) - high byte contains class (0=Normal,1=Infinity,2=NaN,3=Zero)
const VERIFICATION_PROBE: &[ProbeEntry] = &[
    // Test: 1.5 + 2.0 = 3.5
    // a = 1.5 = mantissa=3, exponent=-1; b = 2.0 = mantissa=4, exponent=-1
    // Result: 3.5 = mantissa=7, exponent=-1 (canonical: 7 is odd, exponent=-1)
    ProbeEntry {
        op: ProbeOp::Add,
        a: Dfp::new(3, -1, DfpClass::Normal, false),       // 1.5 = 3 * 2^-1
        b: Some(Dfp::new(4, -1, DfpClass::Normal, false)),  // 2.0 = 4 * 2^-1
        expected_bytes: [0x00, 0x00, 0x00, 0x00,             // mantissa bytes 0-11
                         0x00, 0x00, 0x00, 0x00,
                         0x00, 0x00, 0x00, 0x00,
                         0x00, 0x00, 0x00, 0x07,             // mantissa bytes 12-15: 7 in lowest byte
                         0xff, 0xff, 0xff, 0xff,             // exponent: -1 (0xffffffff in i32)
                         0x00, 0x00, 0x00, 0x01],            // class_sign: Normal(1), sign: positive(0)
    },
    // Test: 3.0 * 2.0 = 6.0
    // a = 3.0 = mantissa=3, exponent=0; b = 2.0 = mantissa=2, exponent=0
    // Result: 6.0 = mantissa=3, exponent=1 (3*2^1 = 6, 3 is odd)
    ProbeEntry {
        op: ProbeOp::Mul,
        a: Dfp::new(3, 0, DfpClass::Normal, false),
        b: Some(Dfp::new(2, 0, DfpClass::Normal, false)),
        expected_bytes: [0x00, 0x00, 0x00, 0x00,             // mantissa bytes 0-11
                         0x00, 0x00, 0x00, 0x00,
                         0x00, 0x00, 0x00, 0x00,
                         0x00, 0x00, 0x00, 0x03,             // mantissa: 3
                         0x01, 0x00, 0x00, 0x00,             // exponent: 1
                         0x00, 0x00, 0x00, 0x01],            // class_sign: Normal(1), sign: positive(0)
    },
    // Test: sqrt(4.0) = 2.0
    // Input: 4.0 = mantissa=1, exponent=2 (1*2^2 = 4, 1 is odd, canonical)
    // Output: 2.0 = mantissa=1, exponent=1 (1*2^1 = 2)
    ProbeEntry {
        op: ProbeOp::Sqrt,
        a: Dfp::new(1, 2, DfpClass::Normal, false),
        b: None,
        expected_bytes: [0x00, 0x00, 0x00, 0x00,             // mantissa bytes 0-11
                         0x00, 0x00, 0x00, 0x00,
                         0x00, 0x00, 0x00, 0x00,
                         0x00, 0x00, 0x00, 0x01,             // mantissa: 1
                         0x01, 0x00, 0x00, 0x00,             // exponent: 1
                         0x00, 0x00, 0x00, 0x01],            // class_sign: Normal(1), sign: positive(0)
    },
    // Test: NaN + 1.0 = NaN (NaN propagation)
    // NaN has class=NaN (2), exponent=0, mantissa=0
    ProbeEntry {
        op: ProbeOp::Add,
        a: Dfp::nan(),
        b: Some(Dfp::new(2, 0, DfpClass::Normal, false)),
        expected_bytes: [0x00, 0x00, 0x00, 0x00,             // mantissa: 0
                         0x00, 0x00, 0x00, 0x00,
                         0x00, 0x00, 0x00, 0x00,
                         0x00, 0x00, 0x00, 0x00,
                         0x00, 0x00, 0x00, 0x00,             // exponent: 0
                         0x00, 0x00, 0x00, 0x02],            // class_sign: NaN(2), sign: positive(0)
    },
    // Test: Overflow saturation - MAX * 2 = MAX (saturating, not infinity)
    // DFP_MAX_MANTISSA = (1u128 << 113) - 1 = 0x1FFFFFFFFFFFFFFFFFFFFFFFFFFF
    // DFP_MAX_EXPONENT = 1023
    ProbeEntry {
        op: ProbeOp::Mul,
        a: Dfp::new((1u128 << 113) - 1, 1023, DfpClass::Normal, false),  // MAX
        b: Some(Dfp::new(2, 0, DfpClass::Normal, false)),
        expected_bytes: [0x1f, 0xff, 0xff, 0xff,             // mantissa: 0x1FFF...FFF (113 bits set)
                         0xff, 0xff, 0xff, 0xff,
                         0xff, 0xff, 0xff, 0xff,
                         0xff, 0xff, 0xff, 0xff,
                         0xff, 0x03, 0x00, 0x00,             // exponent: 1023 (0x3ff)
                         0x00, 0x00, 0x00, 0x01],            // class_sign: Normal(1), sign: positive(0)
    },
    // Test: Division by zero saturation - 1.0 / 0 = MAX (saturating)
    ProbeEntry {
        op: ProbeOp::Div,
        a: Dfp::new(2, 0, DfpClass::Normal, false),
        b: Some(Dfp::zero()),
        expected_bytes: [0x1f, 0xff, 0xff, 0xff,             // mantissa: MAX
                         0xff, 0xff, 0xff, 0xff,
                         0xff, 0xff, 0xff, 0xff,
                         0xff, 0xff, 0xff, 0xff,
                         0xff, 0x03, 0x00, 0x00,             // exponent: 1023
                         0x00, 0x00, 0x00, 0x01],            // class_sign: Normal(1), sign: positive(0)
    },
    // Test: Zero result - 1.0 - 1.0 = 0
    // Result: class=Zero, mantissa=0, exponent=0, sign=positive
    ProbeEntry {
        op: ProbeOp::Sub,
        a: Dfp::new(2, 0, DfpClass::Normal, false),
        b: Some(Dfp::new(2, 0, DfpClass::Normal, false)),
        expected_bytes: [0x00, 0x00, 0x00, 0x00,             // mantissa: 0
                         0x00, 0x00, 0x00, 0x00,
                         0x00, 0x00, 0x00, 0x00,
                         0x00, 0x00, 0x00, 0x00,
                         0x00, 0x00, 0x00, 0x00,             // exponent: 0
                         0x00, 0x00, 0x00, 0x00],            // class_sign: Zero(0), sign: positive(0)
    },
];

/// Execute verification probe
fn run_verification_probe() -> ProbeResult {
    let mut expected = Vec::new();
    let mut actual = Vec::new();

    for entry in VERIFICATION_PROBE {
        // Compute expected result (from reference implementation)
        let result = match entry.op {
            ProbeOp::Add => dfp_add(entry.a, entry.b.unwrap()),
            ProbeOp::Sub => dfp_sub(entry.a, entry.b.unwrap()),
            ProbeOp::Mul => dfp_mul(entry.a, entry.b.unwrap()),
            ProbeOp::Div => dfp_div(entry.a, entry.b.unwrap()),
            ProbeOp::Sqrt => dfp_sqrt(entry.a),
        };

        // Compare 24-byte serialization
        let expected_bytes = entry.expected_bytes;
        let actual_bytes = result.to_bytes();

        expected.push(Digest::from_bytes(expected_bytes));
        actual.push(Digest::from_bytes(actual_bytes));
    }

    ProbeResult {
        expected_hashes: expected.clone(),
        actual_hashes: actual.clone(),
        passed: expected == actual,
    }
}
```

**Why Verification Probe is Critical:**

- Detects **compiler bugs** that produce incorrect i128 arithmetic
- Detects **CPU microcode errors** (e.g., flawed i128 division)
- Detects **VM implementation errors** in soft-float emulation
- Prevents signing fraudulent blocks due to arithmetic bugs

**Probe Execution:**

- Runs automatically every 100,000 blocks
- If probe fails: node halts, logs diagnostic, awaits manual intervention
- Probe results are published for network-wide visibility

### Mission 5: Consensus Integration

- Location: `src/storage/`, `src/consensus/`
- Acceptance criteria:
  - [ ] DFP encoding in Merkle state: DfpEncoding serialized to 24 bytes, included in state trie
  - [ ] Deterministic view enforcement: CREATE DETERMINISTIC VIEW syntax parsed, view flags stored, query planner enforces DFP-only types
  - [ ] Consensus replay validation: On replay, DFP ops re-executed and result hashes compared against stored state
  - [ ] **Fork handling**: When two nodes produce different DFP results, network detects divergence within 1 epoch, triggers soft fork (reject blocks with divergent results)
  - [ ] **Spec version pinning**: Block header includes `dfp_spec_version: u32`, historical blocks validated against their pinned spec version
  - [ ] **Divergence detection latency**: Probe runs every 100,000 blocks; interim divergence detected via Merkle root mismatch on each block (validators verify DFP results before signing)
  - [ ] **Replay pinning**: Node binary version tied to DFP spec version; during replay, use spec version from block header to select correct arithmetic behavior
- Estimated complexity: High

### Developer Tooling

To ensure smooth adoption, provide tooling support:

| Tool               | Description                                   | Priority |
| ------------------ | --------------------------------------------- | -------- |
| DFP-aware linter   | Warn when FLOAT used in deterministic context | High     |
| IDE type hints     | Show DFP vs FLOAT with context indicator      | High     |
| CAST auto-complete | Suggest `CAST(x AS DFP)` when needed          | Medium   |
| Migration analyzer | Scan code for FLOAT in consensus paths        | Medium   |
| REPL/Playground    | Interactive DFP computation testing           | Low      |

### Documentation Enhancements

Create comprehensive documentation:

| Document            | Content                                    | Priority |
| ------------------- | ------------------------------------------ | -------- |
| Migration Guide     | Step-by-step FLOAT→DFP conversion patterns | High     |
| Performance Guide   | When to use DFP vs FLOAT vs DECIMAL        | High     |
| Precision Reference | Decimal digit equivalence, loss scenarios  | Medium   |
| Cookbook            | Common patterns for AI/ML workloads        | Medium   |
| Troubleshooting     | Debugging type mismatch errors             | Low      |

### Breaking Changes

None. DFP is a new type that does not modify existing FLOAT/DOUBLE behavior.

### Migration Path

1. Existing tables continue using FLOAT/DOUBLE
2. New consensus-critical tables use DFP explicitly
3. Deterministic views require DFP columns
4. Gradual migration as needed

### Dependencies

- RFC-0103 (Numeric/Math): Vector-SQL Storage (uses f32 internally, but separate)
- Expression VM (existing Stoolap component)

### Performance

| Mode                | Relative Speed |
| ------------------- | -------------- |
| Software DFP        | ~3-6x slower   |
| Fixed-point integer | ~1.5x faster   |

> Note: DFP now uses pure integer arithmetic (i128), making performance more predictable across platforms.

## Related RFCs

- RFC-0103 (Numeric/Math): Unified Vector-SQL Storage Engine
- RFC-0100 (Economics): AI Quota Marketplace
- RFC-0102 (Numeric/Math): Wallet Cryptography

## Related Use Cases

- [Hybrid AI-Blockchain Runtime](../../docs/use-cases/hybrid-ai-blockchain-runtime.md)
- [Decentralized Mission Execution](../../docs/use-cases/decentralized-mission-execution.md)

## References

- IEEE-754-2019: IEEE Standard for Floating-Point Arithmetic
- [Berkeley SoftFloat](https://github.com/ucb-bar/berkeley-softfloat-3): Industry-standard software floating-point (used by QEMU, EOS, RISC-V)
- [libfixmath](https://github.com/aseprite/libfixmath): Fixed-point library reference
- Stoolap Expression VM documentation
- Deterministic execution in replicated state machines

## Implementation Roadmap

> ⚠️ **STRONGLY RECOMMENDED:** Before production deployment:
>
> 1. **Differential testing** against Berkeley SoftFloat (500+ test vectors)
> 2. **Multi-architecture fuzzing** (x86, ARM, RISC-V)
> 3. **External security audit** by numeric-specialist firm
> 4. **Implementation requirement**: At least partial implementation (add/mul/div/sqrt) before advancing RFC status

### Recommended Production Deployment Scope

> ⚠️ **CRITICAL RECOMMENDATION:**
>
> For initial production deployment, DFP should be **restricted to deterministic read-only contexts**:
>
> | Context              | DFP Allowed? | Notes                                        |
> | -------------------- | ------------ | -------------------------------------------- |
> | Read-only queries    | ✅ Yes       | Deterministic SQL queries                    |
> | Materialized views   | ✅ Yes       | Pre-computed aggregations                    |
> | Oracle data feeds    | ✅ Yes       | Off-chain computation, on-chain verification |
> | Smart contract state | ❌ No        | Wait for extensive testing                   |
> | State transitions    | ❌ No        | High-risk consensus path                     |
>
> This phased approach minimizes consensus risk while proving the technology.

---

**Version:** 1.16
**Submission Date:** 2025-03-06
**Last Updated:** 2026-03-08
**Changes:** v1.16 final fixes (10/10):

- R1: Add Infinity unreachable note to ADD, MUL, DIV, SQRT
- R2: Fix SQRT negative odd exponent - use >> 1 (floor), (exp & 1) != 0 (parity)
- v1.15: MOD-1, MOD-2, MOD-3 and L1-L5 fixes
- v1.14: SQRT 226-bit scaling, subnormal algorithm, signed-zero rules
- M2: Remove sqrt probe entry (was incorrect), now covered by algorithm fix
- M3: Replace dfp_soft with dfp_spec_version for replay pinning
- M4: Fix DFP_MAX_MANTISSA comment - correct formula to (2^113-1) × 2^1023
- M5: Add INT → DFP promotion rules with rationale
- v1.13: SQRT algorithm, verification probe structure, round_to_113 zero guard
