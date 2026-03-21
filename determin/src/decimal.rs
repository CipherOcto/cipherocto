//! Deterministic DECIMAL Implementation
//!
//! RFC-0111: Deterministic DECIMAL
//! i128 mantissa with 0-36 decimal scale.

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
const POW10: [i128; 37] = [
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

#[cfg(test)]
impl Decimal {
    /// For testing only — bypasses validation to create non-canonical values
    fn new_non_canonical(mantissa: i128, scale: u8) -> Self {
        Decimal { mantissa, scale }
    }

    /// For testing only — raw bytes without canonicalization
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
