//! Deterministic DECIMAL Implementation
//!
//! RFC-0111: Deterministic DECIMAL
//! i128 mantissa with 0-36 decimal scale.

use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{Signed, ToPrimitive, Zero};
use serde::{Deserialize, Serialize};

/// DECIMAL specification version
pub const DECIMAL_SPEC_VERSION: u32 = 1;

/// Maximum scale for DECIMAL (0-36)
pub const MAX_DECIMAL_SCALE: u8 = 36;

/// Maximum operation cost for any DECIMAL operation (gas limit)
pub const MAX_DECIMAL_OP_COST: u64 = 5000;

/// Maximum absolute mantissa: 10^36 - 1
pub const MAX_DECIMAL_MANTISSA: i128 = 10_i128.pow(36) - 1;

/// Minimum value: -(10^36 - 1)
pub const MIN_DECIMAL_MANTISSA: i128 = -(10_i128.pow(36) - 1);

/// POW10[i] = 10^i as i128
/// Range: 10^0 to 10^36
/// MUST be byte-identical across all implementations (part of config hash)
#[allow(dead_code)]
pub const POW10: [i128; 37] = [
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
    10000000000000000000,
    100000000000000000000,
    1000000000000000000000,
    10000000000000000000000,
    100000000000000000000000,
    1000000000000000000000000,
    10000000000000000000000000,
    100000000000000000000000000,
    1000000000000000000000000000,
    10000000000000000000000000000,
    100000000000000000000000000000,
    1000000000000000000000000000000,
    10000000000000000000000000000000,
    100000000000000000000000000000000,
    1000000000000000000000000000000000,
    10000000000000000000000000000000000,
    100000000000000000000000000000000000,
    1000000000000000000000000000000000000,
];

/// DECIMAL error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecimalError {
    /// Mantissa outside |≤ 10^36-1| or intermediate exceeded range
    Overflow,
    /// Division by zero
    DivisionByZero,
    /// Scale > 36 on construction or conversion
    InvalidScale,
    /// Deserialized input not in canonical form
    NonCanonical,
    /// DECIMAL→DQA scale > 18, or DECIMAL→BIGINT scale != 0
    ConversionLoss,
}

/// Rounding mode for DECIMAL operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoundingMode {
    /// Round half to even (banker's rounding) — required for financial
    #[default]
    RoundHalfEven,
    /// Round toward zero (floor for positive, ceil for negative)
    RoundDown,
    /// Round away from zero
    RoundUp,
}

/// Decimal: i128 mantissa with 0-36 decimal scale
/// Canonical form: trailing zeros removed, zero = {0, 0}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Decimal {
    mantissa: i128,
    scale: u8,
}

impl Decimal {
    /// Create a new Decimal, validating and canonicalizing.
    /// Returns Err if scale > 36 or |mantissa| > MAX_DECIMAL_MANTISSA.
    pub fn new(mantissa: i128, scale: u8) -> Result<Self, DecimalError> {
        if scale > MAX_DECIMAL_SCALE {
            return Err(DecimalError::InvalidScale);
        }
        let mut d = Decimal { mantissa, scale };
        d.canonicalize();
        if d.mantissa.abs() > MAX_DECIMAL_MANTISSA {
            return Err(DecimalError::Overflow);
        }
        Ok(d)
    }

    /// Internal constructor: assumes already validated/canonical.
    /// Arithmetic operations use this after completing overflow checks.
    #[allow(dead_code)]
    fn from_parts_unchecked(mantissa: i128, scale: u8) -> Self {
        Decimal { mantissa, scale }
    }

    pub fn mantissa(&self) -> i128 {
        self.mantissa
    }

    pub fn scale(&self) -> u8 {
        self.scale
    }

    /// Returns true if Decimal is zero (canonical form)
    pub fn is_zero(&self) -> bool {
        self.mantissa == 0
    }

    /// Canonicalize in-place: remove trailing zeros, force zero to {0, 0}
    fn canonicalize(&mut self) {
        if self.mantissa == 0 {
            self.scale = 0;
            return;
        }
        while self.scale > 0 && self.mantissa % 10 == 0 {
            self.mantissa /= 10;
            self.scale -= 1;
        }
    }

    /// Validate range (does NOT canonicalize)
    #[allow(dead_code)]
    fn validate(&self) -> Result<(), DecimalError> {
        if self.scale > MAX_DECIMAL_SCALE {
            return Err(DecimalError::InvalidScale);
        }
        if self.mantissa.abs() > MAX_DECIMAL_MANTISSA {
            return Err(DecimalError::Overflow);
        }
        Ok(())
    }

    /// Return canonicalized copy
    pub fn canonicalized(mut self) -> Self {
        self.canonicalize();
        self
    }
}

/// Serialize Decimal to 24-byte canonical wire format
pub fn decimal_to_bytes(d: &Decimal) -> [u8; 24] {
    let mut bytes = [0u8; 24];
    bytes[0..16].copy_from_slice(&d.mantissa.to_be_bytes());
    // bytes[16..23] remain zero padding
    bytes[23] = d.scale;
    bytes
}

/// Deserialize from 24-byte canonical wire format
pub fn decimal_from_bytes(bytes: [u8; 24]) -> Result<Decimal, DecimalError> {
    // Verify zero padding
    if bytes[16..23] != [0u8; 7] {
        return Err(DecimalError::NonCanonical);
    }
    let mantissa = i128::from_be_bytes(bytes[0..16].try_into().unwrap());
    let scale = bytes[23];

    // Check scale bounds first
    if scale > MAX_DECIMAL_SCALE {
        return Err(DecimalError::InvalidScale);
    }

    // Check non-canonical forms BEFORE accepting
    // Zero with non-zero scale is non-canonical
    if mantissa == 0 && scale != 0 {
        return Err(DecimalError::NonCanonical);
    }

    // Check trailing zeros - if mantissa has factors of 10 that could be
    // stripped to reduce scale, the input is non-canonical
    let abs_mantissa = mantissa.abs();
    if abs_mantissa != 0 {
        let mut trailing_zeros = 0;
        let mut temp = abs_mantissa;
        while temp % 10 == 0 {
            trailing_zeros += 1;
            temp /= 10;
        }
        // If there are trailing zeros that could be stripped (trailing_zeros >= scale),
        // the representation is non-canonical
        if trailing_zeros >= scale as usize {
            return Err(DecimalError::NonCanonical);
        }
    }

    // Now safe to construct
    Decimal::new(mantissa, scale)
}

impl From<&Decimal> for [u8; 24] {
    fn from(d: &Decimal) -> [u8; 24] {
        decimal_to_bytes(d)
    }
}

impl TryFrom<[u8; 24]> for Decimal {
    type Error = DecimalError;
    fn try_from(bytes: [u8; 24]) -> Result<Self, DecimalError> {
        decimal_from_bytes(bytes)
    }
}

// ─── Arithmetic Operations ────────────────────────────────────────────────────

/// ADD — Addition with safe BigInt scale alignment
///
/// Algorithm (RFC-0111 §ADD):
/// 1. Align scales using BigInt for scale multiplication
/// 2. Add in BigInt, check range, then cast to i128
/// 3. Canonicalize result
pub fn decimal_add(a: &Decimal, b: &Decimal) -> Result<Decimal, DecimalError> {
    let target_scale = a.scale.max(b.scale);
    let diff_a = target_scale - a.scale;
    let diff_b = target_scale - b.scale;

    // Scale alignment in BigInt
    let a_val = if diff_a > 0 {
        let a_big = BigInt::from(a.mantissa);
        let pow10_big = BigInt::from(POW10[diff_a as usize]);
        a_big
            .checked_mul(&pow10_big)
            .ok_or(DecimalError::Overflow)?
    } else {
        BigInt::from(a.mantissa)
    };

    let b_val = if diff_b > 0 {
        let b_big = BigInt::from(b.mantissa);
        let pow10_big = BigInt::from(POW10[diff_b as usize]);
        b_big
            .checked_mul(&pow10_big)
            .ok_or(DecimalError::Overflow)?
    } else {
        BigInt::from(b.mantissa)
    };

    let sum_big = a_val.checked_add(&b_val).ok_or(DecimalError::Overflow)?;

    // Check range before casting to i128
    let max_big = BigInt::from(MAX_DECIMAL_MANTISSA);
    let neg_max_big = -max_big.clone();
    if sum_big > max_big || sum_big < neg_max_big {
        return Err(DecimalError::Overflow);
    }

    let sum = sum_big.to_i128().ok_or(DecimalError::Overflow)?;
    Decimal::new(sum, target_scale)
}

/// SUB — Subtraction with safe BigInt scale alignment
pub fn decimal_sub(a: &Decimal, b: &Decimal) -> Result<Decimal, DecimalError> {
    let target_scale = a.scale.max(b.scale);
    let diff_a = target_scale - a.scale;
    let diff_b = target_scale - b.scale;

    let a_val = if diff_a > 0 {
        BigInt::from(a.mantissa)
            .checked_mul(&BigInt::from(POW10[diff_a as usize]))
            .ok_or(DecimalError::Overflow)?
    } else {
        BigInt::from(a.mantissa)
    };

    let b_val = if diff_b > 0 {
        BigInt::from(b.mantissa)
            .checked_mul(&BigInt::from(POW10[diff_b as usize]))
            .ok_or(DecimalError::Overflow)?
    } else {
        BigInt::from(b.mantissa)
    };

    let diff_big = a_val.checked_sub(&b_val).ok_or(DecimalError::Overflow)?;

    let max_big = BigInt::from(MAX_DECIMAL_MANTISSA);
    let neg_max_big = -max_big.clone();
    if diff_big > max_big || diff_big < neg_max_big {
        return Err(DecimalError::Overflow);
    }

    let diff = diff_big.to_i128().ok_or(DecimalError::Overflow)?;
    Decimal::new(diff, target_scale)
}

/// MUL — Multiplication with BigInt intermediate and RoundHalfEven normalization
///
/// Algorithm (RFC-0111 §MUL):
/// 1. Calculate raw scale
/// 2. If raw_scale > MAX, round the intermediate before scaling down
/// 3. Canonicalize result
pub fn decimal_mul(a: &Decimal, b: &Decimal) -> Result<Decimal, DecimalError> {
    let raw_scale = a.scale.wrapping_add(b.scale);

    if raw_scale > MAX_DECIMAL_SCALE {
        // Scale normalization: round before scaling down
        let scale_reduction = raw_scale - MAX_DECIMAL_SCALE;
        let intermediate = BigInt::from(a.mantissa)
            .checked_mul(&BigInt::from(b.mantissa))
            .ok_or(DecimalError::Overflow)?;

        let divisor = BigInt::from(POW10[scale_reduction as usize]);
        let (product_big, remainder) = intermediate.div_rem(&divisor);

        // RoundHalfEven on magnitude
        let abs_remainder = remainder.abs();
        let half = &divisor / 2;

        let product_big = if abs_remainder > half {
            // Round up (away from zero)
            if product_big >= BigInt::from(0) {
                product_big + BigInt::from(1)
            } else {
                product_big - BigInt::from(1)
            }
        } else if abs_remainder == half && !product_big.is_zero() {
            // Tie: round to even (only round up if odd)
            if &product_big % 2 != BigInt::from(0) {
                if product_big >= BigInt::from(0) {
                    product_big + BigInt::from(1)
                } else {
                    product_big - BigInt::from(1)
                }
            } else {
                product_big
            }
        } else {
            product_big
        };

        // Check overflow after rounding
        let max_big = BigInt::from(MAX_DECIMAL_MANTISSA);
        let neg_max_big = -max_big.clone();
        if product_big > max_big || product_big < neg_max_big {
            return Err(DecimalError::Overflow);
        }

        let product = product_big.to_i128().ok_or(DecimalError::Overflow)?;
        Decimal::new(product, MAX_DECIMAL_SCALE)
    } else {
        // Normal case: no scale overflow
        let intermediate = BigInt::from(a.mantissa)
            .checked_mul(&BigInt::from(b.mantissa))
            .ok_or(DecimalError::Overflow)?;

        if intermediate.abs() > BigInt::from(MAX_DECIMAL_MANTISSA) {
            return Err(DecimalError::Overflow);
        }

        let product = intermediate.to_i128().ok_or(DecimalError::Overflow)?;
        Decimal::new(product, raw_scale)
    }
}

/// DIV — Division with precision growth control and RoundHalfEven rounding
///
/// Algorithm (RFC-0111 §DIV):
/// 1. Division by zero check
/// 2. Compute result scale: min(36, max(a.scale, b.scale) + 6)
/// 3. Work with absolute values, track sign separately
/// 4. Scale dividend, divide, round, apply sign
pub fn decimal_div(a: &Decimal, b: &Decimal, _target_scale: u8) -> Result<Decimal, DecimalError> {
    if b.mantissa == 0 {
        return Err(DecimalError::DivisionByZero);
    }

    // Compute result scale using unified precision growth rule
    let raw_scale = a.scale.max(b.scale).wrapping_add(6);
    let target_scale = raw_scale.min(MAX_DECIMAL_SCALE);

    // Result sign BEFORE division
    let result_sign = (a.mantissa < 0) != (b.mantissa < 0);

    // Work with absolute values
    let abs_a = a.mantissa.abs();
    let abs_b = b.mantissa.abs();

    let scale_diff = (target_scale as i32) - (a.scale as i32) + (b.scale as i32);

    let scaled_dividend: i128 = if scale_diff > 0 {
        // Increase dividend by multiplying to get more precision
        let scaled = BigInt::from(POW10[scale_diff as usize])
            .checked_mul(&BigInt::from(abs_a))
            .ok_or(DecimalError::Overflow)?;
        let max_i128 = BigInt::from(i128::MAX);
        if scaled > max_i128 {
            return Err(DecimalError::Overflow);
        }
        scaled.to_i128().ok_or(DecimalError::Overflow)?
    } else if scale_diff < 0 {
        // Decrease dividend by dividing to reduce scale (RoundHalfEven rounding)
        let scale_reduction = (-scale_diff) as usize;
        let divisor = POW10[scale_reduction];
        let quotient = abs_a / divisor;
        let remainder = abs_a % divisor;
        let half = divisor / 2;

        // RoundHalfEven: round up if remainder > half, or if tie and quotient is odd
        if remainder > half || (remainder == half && quotient % 2 != 0) {
            quotient + 1
        } else {
            quotient
        }
    } else {
        abs_a
    };

    // Divide
    let magnitude = scaled_dividend.abs();
    let quotient = magnitude / abs_b;
    let remainder = magnitude % abs_b;

    // Round to target using RoundHalfEven on magnitude
    let half = abs_b / 2;
    let result = if remainder < half {
        quotient
    } else if remainder > half {
        quotient + 1
    } else if quotient % 2 == 0 {
        quotient // already even
    } else {
        quotient + 1 // round up to even
    };

    // Apply sign
    let result = if result_sign { -result } else { result };

    Decimal::new(result, target_scale)
}

/// SQRT — Square root with Newton-Raphson (40 iterations)
///
/// Algorithm (RFC-0111 §SQRT):
/// 1. Reject negative input
/// 2. Scale mantissa to target precision P = min(36, a.scale + 6)
/// 3. Compute integer sqrt using Newton-Raphson in BigInt
/// 4. Handle off-by-one correction and overflow check
pub fn decimal_sqrt(a: &Decimal) -> Result<Decimal, DecimalError> {
    if a.mantissa < 0 {
        return Err(DecimalError::InvalidScale); // sqrt of negative
    }
    if a.mantissa == 0 {
        return Decimal::new(0, 0);
    }

    // Compute result precision: P = min(36, a.scale + 6)
    let p = (a.scale as u16 + 6).min(MAX_DECIMAL_SCALE as u16) as u8;

    // Scale factor = 2*P - a.scale
    let scale_factor = (2 * p as i32) - (a.scale as i32);

    // Scale mantissa: n = a.mantissa * 10^(2P-s)
    // Use split multiplication when scale_factor > 36
    let scaled_n = if scale_factor > 36 {
        let lo = BigInt::from(POW10[(scale_factor - 36) as usize]);
        let hi = BigInt::from(POW10[36]);
        let partial = BigInt::from(a.mantissa)
            .checked_mul(&lo)
            .ok_or(DecimalError::Overflow)?;
        partial.checked_mul(&hi).ok_or(DecimalError::Overflow)?
    } else if scale_factor >= 0 {
        BigInt::from(a.mantissa)
            .checked_mul(&BigInt::from(POW10[scale_factor as usize]))
            .ok_or(DecimalError::Overflow)?
    } else {
        return Err(DecimalError::Overflow); // scale_factor < 0 should not happen
    };

    // Newton-Raphson integer square root
    // Initial guess: 2^(ceil(bit_length(n)/2))
    let bit_len = scaled_n.bits();
    let mut x = BigInt::from(1) << bit_len.div_ceil(2);

    // Fixed 40 iterations (no early exit per RFC-0111)
    for _ in 0..40 {
        if x.is_zero() {
            break;
        }
        let n_over_x = &scaled_n / &x;
        x = (&x + n_over_x) >> 1; // divide by 2
    }

    // Off-by-one correction
    if &x * &x > scaled_n {
        x -= BigInt::from(1);
    }

    // Range check
    let max_big = BigInt::from(MAX_DECIMAL_MANTISSA);
    if x > max_big {
        return Err(DecimalError::Overflow);
    }

    let mantissa = x.to_i128().ok_or(DecimalError::Overflow)?;
    Decimal::new(mantissa, p)
}

/// ROUND — Rounding with configurable mode
///
/// Algorithm (RFC-0111 §ROUND):
/// 1. If target_scale >= d.scale, return d (no rounding needed)
/// 2. Compute divisor = 10^diff
/// 3. Apply rounding per mode
pub fn decimal_round(
    d: &Decimal,
    target_scale: u8,
    mode: RoundingMode,
) -> Result<Decimal, DecimalError> {
    if target_scale >= d.scale {
        return Ok(*d);
    }

    let diff = (d.scale - target_scale) as usize;
    let divisor = POW10[diff];

    let q = d.mantissa / divisor;
    let r = d.mantissa % divisor;

    let result = match mode {
        RoundingMode::RoundHalfEven => {
            let abs_r = r.abs();
            let half = divisor / 2;
            if abs_r < half {
                q
            } else if abs_r > half {
                q + d.mantissa.signum()
            } else if q % 2 == 0 {
                q // already even
            } else {
                q + d.mantissa.signum() // round away from zero
            }
        }
        RoundingMode::RoundDown => q,
        RoundingMode::RoundUp => {
            if r > 0 && d.mantissa > 0 {
                q + 1
            } else if r < 0 && d.mantissa < 0 {
                q - 1
            } else {
                q
            }
        }
    };

    Decimal::new(result, target_scale)
}

/// CMP — Comparison using BigInt scale alignment
///
/// Returns: -1 (a < b), 0 (a == b), 1 (a > b)
///
/// Algorithm (RFC-0111 §CMP):
/// 1. Fast path: if scales equal, compare directly
/// 2. Scale alignment using BigInt (scale_diff up to 36)
pub fn decimal_cmp(a: &Decimal, b: &Decimal) -> i32 {
    // Fast path: same scale
    if a.scale == b.scale {
        if a.mantissa < b.mantissa {
            return -1;
        } else if a.mantissa > b.mantissa {
            return 1;
        }
        return 0;
    }

    // Scale alignment using BigInt
    let max_scale = a.scale.max(b.scale);
    let diff_a = (max_scale - a.scale) as usize;
    let diff_b = (max_scale - b.scale) as usize;

    let compare_a = BigInt::from(a.mantissa) * BigInt::from(POW10[diff_a]);
    let compare_b = BigInt::from(b.mantissa) * BigInt::from(POW10[diff_b]);

    if compare_a < compare_b {
        -1
    } else if compare_a > compare_b {
        1
    } else {
        0
    }
}

#[cfg(test)]
impl Decimal {
    /// For testing only — bypasses validation to create non-canonical values
    fn new_non_canonical(mantissa: i128, scale: u8) -> Self {
        Decimal { mantissa, scale }
    }

    /// For testing only — raw bytes without canonicalization
    #[allow(clippy::wrong_self_convention)]
    fn to_bytes_raw(&self) -> [u8; 24] {
        let mut bytes = [0u8; 24];
        bytes[0..16].copy_from_slice(&self.mantissa.to_be_bytes());
        bytes[23] = self.scale;
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_zero() {
        let d = Decimal::new(0, 5).unwrap();
        assert_eq!(d.mantissa(), 0);
        assert_eq!(d.scale(), 0);
    }

    #[test]
    fn negative_mantissa_canonicalizes() {
        // -1000 with scale=3 → -1 with scale=0
        let d = Decimal::new(-1000, 3).unwrap();
        assert_eq!(d.mantissa(), -1);
        assert_eq!(d.scale(), 0);
    }

    #[test]
    fn trailing_zeros_stripped() {
        let d = Decimal::new(1000, 3).unwrap();
        assert_eq!(d.mantissa(), 1);
        assert_eq!(d.scale(), 0);
    }

    #[test]
    fn max_mantissa_accepted() {
        // 10^36 - 1 is exactly at the boundary
        let d = Decimal::new(MAX_DECIMAL_MANTISSA, 36).unwrap();
        assert_eq!(d.mantissa(), MAX_DECIMAL_MANTISSA);
    }

    #[test]
    fn min_mantissa_accepted() {
        let d = Decimal::new(MIN_DECIMAL_MANTISSA, 0).unwrap();
        assert_eq!(d.mantissa(), MIN_DECIMAL_MANTISSA);
    }

    #[test]
    fn invalid_scale_rejected() {
        assert!(matches!(
            Decimal::new(100, 37),
            Err(DecimalError::InvalidScale)
        ));
    }

    #[test]
    fn positive_overflow_rejected() {
        // 10^36 exceeds MAX
        assert!(matches!(
            Decimal::new(10_i128.pow(36), 0),
            Err(DecimalError::Overflow)
        ));
    }

    #[test]
    fn negative_overflow_rejected() {
        // RFC v1.19 ISSUE-1: negative overflow is distinct case
        assert!(matches!(
            Decimal::new(-(10_i128.pow(36)), 0),
            Err(DecimalError::Overflow)
        ));
    }

    #[test]
    fn roundtrip_serialize() {
        let original = Decimal::new(123456789012345678901234567_i128, 18).unwrap();
        let bytes = decimal_to_bytes(&original);
        let restored = decimal_from_bytes(bytes).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn non_canonical_padding_rejected() {
        let mut bytes = [0u8; 24];
        bytes[0..16].copy_from_slice(&1_i128.to_be_bytes());
        bytes[16] = 0xFF; // non-zero padding
        bytes[23] = 0;
        assert!(matches!(
            decimal_from_bytes(bytes),
            Err(DecimalError::NonCanonical)
        ));
    }

    #[test]
    fn non_canonical_input_from_bytes_rejected() {
        // Non-canonical {1000, 3} should be rejected on deserialization
        let non_canonical = Decimal::new_non_canonical(1000, 3);
        let bytes = non_canonical.to_bytes_raw();
        assert!(matches!(
            decimal_from_bytes(bytes),
            Err(DecimalError::NonCanonical)
        ));
    }
}
