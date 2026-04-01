//! Deterministic Quant Arithmetic (DQA) Implementation
//!
//! This module implements RFC-0105: Deterministic Quant Arithmetic
//! for the CipherOcto protocol.
//!
//! Key design principles:
//! - Pure integer arithmetic (no floating-point operations)
//! - Bounded range: i64 value with 0-18 decimal scale
//! - Canonical representation (trailing zeros stripped)
//! - RoundHalfEven (banker's rounding)

/// Maximum allowed scale (0-18)
pub const MAX_SCALE: u8 = 18;

/// Maximum decimal digits in abs(i64): i64::MAX has 19 digits
pub const MAX_I64_DIGITS: u32 = 19;

/// Maximum decimal digits in i128
pub const MAX_I128_DIGITS: u32 = 39;

/// Deterministic POW10 table for scale alignment and division
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
    100000000000000000000000000000,        // 10^29
    1000000000000000000000000000000,       // 10^30
    10000000000000000000000000000000,      // 10^31
    100000000000000000000000000000000,     // 10^32
    1000000000000000000000000000000000,    // 10^33
    10000000000000000000000000000000000,   // 10^34
    100000000000000000000000000000000000,  // 10^35
    1000000000000000000000000000000000000, // 10^36
];

/// For i64-safe operations (scales 0-18 only)
const POW10_I64: [i64; 19] = [
    1,
    10,
    100,
    1000,
    10000,
    100000,
    1000000,
    10000000,
    100000000,
    1000000000,
    10000000000,
    100000000000,
    1000000000000,
    10000000000000,
    100000000000000,
    1000000000000000,
    10000000000000000,
    100000000000000000,
    1000000000000000000,
];

/// DQA error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DqaError {
    /// Integer overflow during arithmetic
    Overflow,
    /// Division by zero
    DivisionByZero,
    /// Invalid scale (must be 0-18)
    InvalidScale,
    /// Invalid input (e.g., NaN, Infinity in f64 conversion)
    InvalidInput,
    /// Invalid encoding (reserved bytes non-zero)
    InvalidEncoding,
}

/// Deterministic Quant representation
/// Represents value × 10^(-scale)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Dqa {
    /// Integer value (the numerator)
    pub value: i64,
    /// Decimal scale (the exponent for 10^-scale)
    pub scale: u8,
}

/// Canonical zero value (value=0, scale=0)
pub const CANONICAL_ZERO: Dqa = Dqa { value: 0, scale: 0 };

impl Dqa {
    /// Create DQA from value and scale
    /// Returns Error::InvalidScale if scale > 18
    pub fn new(value: i64, scale: u8) -> Result<Self, DqaError> {
        if scale > MAX_SCALE {
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
        if scale > MAX_SCALE {
            return Err(DqaError::InvalidScale);
        }
        if value.is_nan() || value.is_infinite() {
            return Err(DqaError::InvalidInput);
        }
        // Algorithm: multiply by 10^scale, round to nearest integer, clamp to i64
        // Note: f64::round() uses half-away-from-zero, not RoundHalfEven.
        let power = POW10_I64[scale as usize];
        let scaled = value * power as f64;
        let rounded = scaled.round();
        if rounded > i64::MAX as f64 || rounded < i64::MIN as f64 {
            return Err(DqaError::Overflow);
        }
        Ok(Dqa {
            value: rounded as i64,
            scale,
        })
    }

    /// Convert to f64 (lossy)
    /// WARNING: Non-consensus API. Only use for display/logging.
    #[cfg(feature = "non_consensus")]
    pub fn to_f64(&self) -> f64 {
        let power = POW10_I64[self.scale as usize];
        self.value as f64 / power as f64
    }

    /// Arithmetic: addition
    #[allow(clippy::should_implement_trait)]
    pub fn add(self, other: Self) -> Result<Self, DqaError> {
        dqa_add(self, other)
    }

    /// Arithmetic: subtraction
    pub fn subtract(self, other: Self) -> Result<Self, DqaError> {
        dqa_sub(self, other)
    }

    /// Arithmetic: multiplication
    pub fn multiply(self, other: Self) -> Result<Self, DqaError> {
        dqa_mul(self, other)
    }

    /// Arithmetic: division
    pub fn divide(self, other: Self) -> Result<Self, DqaError> {
        dqa_div(self, other)
    }

    /// Unary negation
    pub fn negate(self) -> Result<Self, DqaError> {
        // Check for overflow: -i64::MIN would overflow
        if self.value == i64::MIN {
            return Err(DqaError::Overflow);
        }
        Ok(Dqa {
            value: -self.value,
            scale: self.scale,
        })
    }

    /// Absolute value
    pub fn absolute(self) -> Result<Self, DqaError> {
        // Check for overflow: abs(i64::MIN) would overflow
        if self.value == i64::MIN {
            return Err(DqaError::Overflow);
        }
        Ok(Dqa {
            value: self.value.abs(),
            scale: self.scale,
        })
    }

    /// Compare two DQA values
    /// Returns -1 if self < other, 0 if equal, 1 if self > other
    pub fn compare(self, other: Self) -> i8 {
        dqa_cmp(self, other)
    }
}

/// Get sign of i64: 1 for positive, -1 for negative, 0 for zero
fn sign(value: i64) -> i64 {
    if value > 0 {
        1
    } else if value < 0 {
        -1
    } else {
        0
    }
}

/// Canonicalize DQA: strip trailing zeros, zero has scale 0
fn canonicalize(dqa: Dqa) -> Dqa {
    if dqa.value == 0 {
        return Dqa { value: 0, scale: 0 };
    }
    let mut value = dqa.value;
    let mut scale = dqa.scale;
    while value % 10 == 0 && scale > 0 {
        value /= 10;
        scale -= 1;
    }
    Dqa { value, scale }
}

/// Align scales for ADD/SUB operations
/// Returns (aligned_a_value, aligned_b_value, result_scale)
/// Note: This is a pure function - does NOT mutate inputs
fn align_scales(a: Dqa, b: Dqa) -> Result<(i64, i64, u8), DqaError> {
    let result_scale = a.scale.max(b.scale);
    if a.scale == b.scale {
        return Ok((a.value, b.value, result_scale));
    }
    let diff = (a.scale as i32 - b.scale as i32).unsigned_abs() as u8;
    // diff <= 18 after canonicalization, safe to use POW10_I64
    let power = POW10_I64[diff as usize];
    if a.scale > b.scale {
        // Multiply b to match a's scale
        let intermediate = (b.value as i128) * (power as i128);
        if intermediate > i64::MAX as i128 || intermediate < i64::MIN as i128 {
            return Err(DqaError::Overflow);
        }
        Ok((a.value, intermediate as i64, result_scale))
    } else {
        // Multiply a to match b's scale
        let intermediate = (a.value as i128) * (power as i128);
        if intermediate > i64::MAX as i128 || intermediate < i64::MIN as i128 {
            return Err(DqaError::Overflow);
        }
        Ok((intermediate as i64, b.value, result_scale))
    }
}

/// RoundHalfEven with remainder - used by division and multiplication
fn round_half_even_with_remainder(
    quotient: i128,
    remainder: i128,
    divisor: i128,
    result_sign: i64,
) -> i128 {
    let double_rem = remainder.abs() * 2;
    let abs_divisor = divisor.abs();
    if double_rem < abs_divisor {
        return quotient;
    }
    if double_rem > abs_divisor {
        return quotient + (result_sign as i128);
    }
    // double_rem == abs_divisor (tie exactly at 0.5)
    // Round half even: check if magnitude is even
    if (quotient.abs() % 2) == 0 {
        quotient
    } else {
        quotient + (result_sign as i128)
    }
}

/// DQA Addition
pub fn dqa_add(a: Dqa, b: Dqa) -> Result<Dqa, DqaError> {
    let (a_val, b_val, result_scale) = align_scales(a, b)?;
    let result_value = (a_val as i128) + (b_val as i128);
    if result_value > i64::MAX as i128 || result_value < i64::MIN as i128 {
        return Err(DqaError::Overflow);
    }
    let result = Dqa {
        value: result_value as i64,
        scale: result_scale,
    };
    // Canonicalize to prevent Merkle hash mismatches
    Ok(canonicalize(result))
}

/// DQA Subtraction
pub fn dqa_sub(a: Dqa, b: Dqa) -> Result<Dqa, DqaError> {
    let (a_val, b_val, result_scale) = align_scales(a, b)?;
    let result_value = (a_val as i128) - (b_val as i128);
    if result_value > i64::MAX as i128 || result_value < i64::MIN as i128 {
        return Err(DqaError::Overflow);
    }
    let result = Dqa {
        value: result_value as i64,
        scale: result_scale,
    };
    // Canonicalize to prevent Merkle hash mismatches
    Ok(canonicalize(result))
}

/// DQA Multiplication
pub fn dqa_mul(a: Dqa, b: Dqa) -> Result<Dqa, DqaError> {
    // Use i128 intermediate to prevent overflow during calculation
    let mut intermediate = (a.value as i128) * (b.value as i128);
    let mut result_scale = (a.scale as u16 + b.scale as u16) as u8;

    // If scale > 18, round to 18 while in i128
    if result_scale > MAX_SCALE {
        let diff = result_scale - MAX_SCALE;
        // Get quotient and remainder for proper RoundHalfEven
        let quotient = intermediate / POW10[diff as usize];
        let round_remainder = intermediate % POW10[diff as usize];
        // Apply RoundHalfEven using the helper with sign
        let result_sign = sign(a.value) * sign(b.value);
        intermediate = round_half_even_with_remainder(
            quotient,
            round_remainder,
            POW10[diff as usize],
            result_sign,
        );
        result_scale = MAX_SCALE;
    }

    // Check for i64 overflow after rounding
    if intermediate > i64::MAX as i128 || intermediate < i64::MIN as i128 {
        return Err(DqaError::Overflow);
    }

    // Canonicalize (may strip trailing zeros from multiplication results)
    Ok(canonicalize(Dqa {
        value: intermediate as i64,
        scale: result_scale,
    }))
}

/// DQA Division
pub fn dqa_div(a: Dqa, b: Dqa) -> Result<Dqa, DqaError> {
    if b.value == 0 {
        return Err(DqaError::DivisionByZero);
    }

    let target_scale = a.scale.max(b.scale);
    let power = target_scale + b.scale - a.scale;

    // Guard against i128 overflow using checked multiplication
    let scaled = match (a.value as i128).checked_mul(POW10[power as usize]) {
        Some(s) => s,
        None => return Err(DqaError::Overflow),
    };

    let quotient = scaled / (b.value as i128);
    let remainder = scaled % (b.value as i128);
    let result_sign = sign(a.value) * sign(b.value);
    let abs_b = (b.value as i128).abs();

    let result_value = round_half_even_with_remainder(quotient, remainder, abs_b, result_sign);

    if result_value > i64::MAX as i128 || result_value < i64::MIN as i128 {
        return Err(DqaError::Overflow);
    }

    // Canonicalize to prevent Merkle hash divergence
    Ok(canonicalize(Dqa {
        value: result_value as i64,
        scale: target_scale,
    }))
}

/// DQA Comparison
/// Returns -1 if a < b, 0 if equal, 1 if a > b
pub fn dqa_cmp(a: Dqa, b: Dqa) -> i8 {
    // Canonicalize both operands first
    let a_canonical = canonicalize(a);
    let b_canonical = canonicalize(b);

    // Fast path: if scales equal, compare values directly
    if a_canonical.scale == b_canonical.scale {
        if a_canonical.value < b_canonical.value {
            return -1;
        }
        if a_canonical.value > b_canonical.value {
            return 1;
        }
        return 0;
    }

    // Scale alignment with overflow guard
    let diff = (a_canonical.scale as i32 - b_canonical.scale as i32).unsigned_abs() as u8;
    // After canonicalization, both scales are ≤ 18, so diff ≤ 18 always

    // Safe: 19 digits × 10^18 < i128 max
    let (compare_a, compare_b) = if a_canonical.scale > b_canonical.scale {
        let scale_factor = POW10[diff as usize];
        (
            a_canonical.value as i128,
            (b_canonical.value as i128) * scale_factor,
        )
    } else {
        let scale_factor = POW10[diff as usize];
        (
            (a_canonical.value as i128) * scale_factor,
            b_canonical.value as i128,
        )
    };

    if compare_a < compare_b {
        -1
    } else if compare_a > compare_b {
        1
    } else {
        0
    }
}

/// DQA Negation: -a
pub fn dqa_negate(a: Dqa) -> Result<Dqa, DqaError> {
    a.negate()
}

/// DQA Absolute Value: |a|
pub fn dqa_abs(a: Dqa) -> Result<Dqa, DqaError> {
    a.absolute()
}

/// DQA Assignment to Column
///
/// Coerces a DQA expression result to a fixed-scale column.
/// Uses RoundHalfEven for deterministic rounding.
///
/// # Arguments
/// * `expr_result` - The DQA value from an expression
/// * `column_scale` - The target scale of the column
///
/// # Returns
/// * `Ok(Dqa)` - The value rounded/padded to column scale
/// * `Err(DqaError::Overflow)` - If result exceeds i64 range
pub fn dqa_assign_to_column(expr_result: Dqa, column_scale: u8) -> Result<Dqa, DqaError> {
    if expr_result.scale > column_scale {
        // Round to column scale using RoundHalfEven
        let diff = expr_result.scale - column_scale;
        let divisor = POW10[diff as usize];
        let value_i128 = expr_result.value as i128;
        let quotient = value_i128 / divisor;
        let remainder = value_i128 % divisor;
        let result_sign = expr_result.value.signum();
        let result_value =
            round_half_even_with_remainder(quotient, remainder, divisor, result_sign);
        // Check i64 range (rounded quotient could theoretically exceed i64)
        if result_value > i64::MAX as i128 || result_value < i64::MIN as i128 {
            return Err(DqaError::Overflow);
        }
        Ok(Dqa {
            value: result_value as i64,
            scale: column_scale,
        })
    } else if expr_result.scale < column_scale {
        // Pad with trailing zeros
        let diff = column_scale - expr_result.scale;
        // Use i128 for overflow-safe multiplication
        let intermediate = (expr_result.value as i128) * POW10[diff as usize];
        if intermediate > i64::MAX as i128 || intermediate < i64::MIN as i128 {
            return Err(DqaError::Overflow);
        }
        let result_value = intermediate as i64;
        Ok(Dqa {
            value: result_value,
            scale: column_scale,
        })
    } else {
        // Scales match, no coercion needed
        Ok(expr_result)
    }
}

/// DQA encoding for storage/consensus (16 bytes)
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
        let canonical = canonicalize(*dqa);
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
        if self.scale > MAX_SCALE {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create Dqa with scale
    fn dqa(value: i64, scale: u8) -> Dqa {
        Dqa { value, scale }
    }

    /// Test canonicalization
    #[test]
    fn test_canonicalize() {
        assert_eq!(canonicalize(dqa(1000, 3)), dqa(1, 0));
        assert_eq!(canonicalize(dqa(50, 2)), dqa(5, 1));
        assert_eq!(canonicalize(dqa(0, 5)), dqa(0, 0));
        assert_eq!(canonicalize(dqa(100, 2)), dqa(1, 0));
    }

    /// Test addition
    #[test]
    fn test_add() {
        // 1.2 + 12.3 = 13.5
        assert_eq!(dqa_add(dqa(12, 1), dqa(123, 2)).unwrap(), dqa(243, 2));
        // 1.000 + 1 = 2.000 → canonical 2,0
        assert_eq!(dqa_add(dqa(1000, 3), dqa(1, 0)).unwrap(), dqa(2, 0));
        // -0.50 + 0.75 = 0.25
        assert_eq!(dqa_add(dqa(-50, 2), dqa(75, 2)).unwrap(), dqa(25, 2));
        // 0 + 0.00000 = 0 (canonical)
        assert_eq!(dqa_add(dqa(0, 0), dqa(0, 5)).unwrap(), dqa(0, 0));
    }

    /// Test subtraction
    #[test]
    fn test_sub() {
        // 12.3 - 1.2 = 11.1 → 1.23 - 1.2 = 0.03
        assert_eq!(dqa_sub(dqa(123, 2), dqa(12, 1)).unwrap(), dqa(3, 2));
    }

    /// Test multiplication
    #[test]
    fn test_mul() {
        // 1.2 × 0.3 = 0.36
        assert_eq!(dqa_mul(dqa(12, 1), dqa(3, 1)).unwrap(), dqa(36, 2));
        // 1.00 × 2.000 = 2.00000 → canonical 2,0
        assert_eq!(dqa_mul(dqa(100, 2), dqa(2000, 3)).unwrap(), dqa(2, 0));
        // -0.5 × 0.4 = -0.20 → canonical -2,1
        assert_eq!(dqa_mul(dqa(-5, 1), dqa(4, 1)).unwrap(), dqa(-2, 1));
    }

    /// Test division
    #[test]
    fn test_div() {
        // 1.0 / 2 = 0.5 → canonical 5,1
        assert_eq!(dqa_div(dqa(1000, 3), dqa(2, 0)).unwrap(), dqa(5, 1));
        // 1.0 / 2 at scale 6 = 0.500000 → canonical 5,1
        assert_eq!(dqa_div(dqa(1000000, 6), dqa(2, 0)).unwrap(), dqa(5, 1));
        // 0.000001 / 2 = 0.0000005 → rounds to 0, canonical to 0,0
        assert_eq!(dqa_div(dqa(1, 6), dqa(2, 0)).unwrap(), dqa(0, 0));
        // 1.0 / 4 = 0.25 → 0.2
        assert_eq!(dqa_div(dqa(10, 1), dqa(4, 0)).unwrap(), dqa(2, 1));
        // 5 / 2 = 2.5 → tie rounds to even (2)
        assert_eq!(dqa_div(dqa(5, 0), dqa(2, 0)).unwrap(), dqa(2, 0));
        // 15 / 2 = 7.5 → tie rounds to even (8)
        assert_eq!(dqa_div(dqa(15, 0), dqa(2, 0)).unwrap(), dqa(8, 0));
        // -5 / 2 = -2.5 → tie rounds to even (-2)
        assert_eq!(dqa_div(dqa(-5, 0), dqa(2, 0)).unwrap(), dqa(-2, 0));
        // -15 / 2 = -7.5 → -8
        assert_eq!(dqa_div(dqa(-15, 0), dqa(2, 0)).unwrap(), dqa(-8, 0));
    }

    /// Test division by zero
    #[test]
    fn test_div_by_zero() {
        assert_eq!(
            dqa_div(dqa(1, 0), dqa(0, 0)).unwrap_err(),
            DqaError::DivisionByZero
        );
    }

    /// Test comparison
    #[test]
    fn test_cmp() {
        // 1.2 == 1.20
        assert_eq!(dqa_cmp(dqa(12, 1), dqa(120, 2)), 0);
        // 1.2 > 1.10
        assert_eq!(dqa_cmp(dqa(12, 1), dqa(110, 2)), 1);
        // 1.2 < 1.30
        assert_eq!(dqa_cmp(dqa(12, 1), dqa(130, 2)), -1);
        // negative equality
        assert_eq!(dqa_cmp(dqa(-15, 1), dqa(-15, 1)), 0);
        // -1.5 > -2.5
        assert_eq!(dqa_cmp(dqa(-15, 1), dqa(-25, 1)), 1);
    }

    /// Test encoding
    #[test]
    fn test_encoding() {
        let value = dqa(123456, 4);
        let encoding = DqaEncoding::from_dqa(&value);
        let decoded = encoding.to_dqa().unwrap();
        assert_eq!(decoded, dqa(123456, 4));
    }

    /// Test encoding canonicalization
    #[test]
    fn test_encoding_canonical() {
        // 1.20 should encode as 12,1 (canonical)
        let value = dqa(120, 2);
        let encoding = DqaEncoding::from_dqa(&value);
        let decoded = encoding.to_dqa().unwrap();
        assert_eq!(decoded, dqa(12, 1));
    }

    /// Test overflow detection
    #[test]
    fn test_mul_overflow() {
        // 10^18 * 10 overflows i64
        let result = dqa_mul(dqa(10i64.pow(18), 0), dqa(10, 0));
        assert_eq!(result.unwrap_err(), DqaError::Overflow);
    }

    /// Test add overflow
    #[test]
    fn test_add_overflow() {
        let max = dqa(i64::MAX, 0);
        let one = dqa(1, 0);
        assert_eq!(dqa_add(max, one).unwrap_err(), DqaError::Overflow);
    }

    /// Test assign to column - round down
    #[test]
    fn test_assign_round_down() {
        // 123.456789 -> scale 6 = 123.456789 (no change)
        assert_eq!(
            dqa_assign_to_column(dqa(123456789, 6), 6).unwrap(),
            dqa(123456789, 6)
        );
        // 123.456789 -> scale 4 = 123.4568 (round up)
        assert_eq!(
            dqa_assign_to_column(dqa(123456789, 6), 4).unwrap(),
            dqa(1234568, 4)
        );
        // 123.456789 -> scale 2 = 123.46 (round up)
        assert_eq!(
            dqa_assign_to_column(dqa(123456789, 6), 2).unwrap(),
            dqa(12346, 2)
        );
    }

    /// Test assign to column - round half even
    #[test]
    fn test_assign_round_half_even() {
        // 2.5 -> scale 0 = 2 (even, round down)
        assert_eq!(dqa_assign_to_column(dqa(25, 1), 0).unwrap(), dqa(2, 0));
        // 3.5 -> scale 0 = 4 (odd, round up)
        assert_eq!(dqa_assign_to_column(dqa(35, 1), 0).unwrap(), dqa(4, 0));
        // 1.25 -> scale 1 = 1.2 (round down)
        assert_eq!(dqa_assign_to_column(dqa(125, 2), 1).unwrap(), dqa(12, 1));
        // 1.35 -> scale 1 = 1.4 (round up - 3 is odd)
        assert_eq!(dqa_assign_to_column(dqa(135, 2), 1).unwrap(), dqa(14, 1));
    }

    /// Test assign to column - pad with zeros
    #[test]
    fn test_assign_pad_zeros() {
        // 123 -> scale 2 = 123.00
        assert_eq!(dqa_assign_to_column(dqa(123, 0), 2).unwrap(), dqa(12300, 2));
        // 1.5 (scale 1) -> scale 4 = 1.5000
        assert_eq!(dqa_assign_to_column(dqa(15, 1), 4).unwrap(), dqa(15000, 4));
    }

    /// Test assign to column - same scale
    #[test]
    fn test_assign_same_scale() {
        assert_eq!(dqa_assign_to_column(dqa(123, 2), 2).unwrap(), dqa(123, 2));
    }

    /// Test assign overflow
    #[test]
    fn test_assign_overflow() {
        // i64::MAX with scale 0 -> scale 18 would overflow
        let max = dqa(i64::MAX, 0);
        // This would require 10^18 multiplication, definitely overflows
        assert_eq!(
            dqa_assign_to_column(max, 18).unwrap_err(),
            DqaError::Overflow
        );
    }

    /// Test encoding is deterministic (canonical form)
    #[test]
    fn test_encoding_deterministic() {
        // 1.500 with scale 3 should canonicalize to 15 with scale 1
        let dqa1 = dqa(1500, 3);
        let encoding1 = DqaEncoding::from_dqa(&dqa1);
        // After canonicalization: value=15, scale=1
        assert_eq!(i64::from_be(encoding1.value), 15i64);
        assert_eq!(encoding1.scale, 1);

        // Same logical value should produce same encoding
        let dqa2 = dqa(15, 1);
        let encoding2 = DqaEncoding::from_dqa(&dqa2);
        assert_eq!(encoding1.value, encoding2.value);
        assert_eq!(encoding1.scale, encoding2.scale);
    }

    /// Test encoding round-trip
    #[test]
    fn test_encoding_roundtrip() {
        let original = dqa(123456789, 6);
        let encoding = DqaEncoding::from_dqa(&original);
        let recovered = encoding.to_dqa().unwrap();
        // Canonical form should match
        assert_eq!(canonicalize(recovered), canonicalize(original));
    }
}
