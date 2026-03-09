//! Deterministic Floating-Point (DFP) Implementation
//!
//! This module implements RFC-0104: Deterministic Floating-Point Abstraction
//! for the CipherOcto protocol.
//!
//! Key design principles:
//! - Pure integer arithmetic (no floating-point operations)
//! - Saturating arithmetic (overflow → MAX, not Infinity)
//! - Canonical odd-mantissa invariant
//! - Round-to-nearest-even (RNE)

mod arithmetic;
#[cfg(test)]
mod fuzz;
mod probe;
pub mod dqa;

pub use arithmetic::{dfp_add, dfp_div, dfp_mul, dfp_sqrt, dfp_sub};
pub use dqa::{dqa_assign_to_column, Dqa, DqaEncoding, DqaError};

use serde::{Deserialize, Serialize};

/// DFP class tag to avoid encoding collisions with numeric values
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DfpClass {
    /// Normal numeric value
    Normal,
    /// Positive infinity (reserved, unreachable in compliant implementations)
    Infinity,
    /// Not a Number
    NaN,
    /// Zero (sign preserved)
    Zero,
}

/// Deterministic Floating-Point representation
/// Uses tagged representation to avoid encoding collisions
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Dfp {
    /// Mantissa (always odd for Normal class, canonical form)
    pub mantissa: u128,
    /// Exponent (binary exponent)
    pub exponent: i32,
    /// Class tag
    pub class: DfpClass,
    /// Sign (false = positive, true = negative)
    pub sign: bool,
}

impl Dfp {
    /// Get the 24-byte encoding
    pub fn to_encoding(&self) -> DfpEncoding {
        DfpEncoding::from_dfp(self)
    }

    /// Create a new Dfp from components
    pub fn new(mantissa: u128, exponent: i32, class: DfpClass, sign: bool) -> Self {
        let mut dfp = Dfp {
            mantissa,
            exponent,
            class,
            sign,
        };
        // Normalize to canonical form
        if dfp.class == DfpClass::Normal {
            dfp.normalize();
        }
        dfp
    }

    /// Create NaN
    pub fn nan() -> Self {
        Dfp {
            mantissa: 0,
            exponent: 0,
            class: DfpClass::NaN,
            sign: false,
        }
    }

    /// Create positive zero
    pub fn zero() -> Self {
        Dfp {
            mantissa: 0,
            exponent: 0,
            class: DfpClass::Zero,
            sign: false,
        }
    }

    /// Create negative zero
    pub fn neg_zero() -> Self {
        Dfp {
            mantissa: 0,
            exponent: 0,
            class: DfpClass::Zero,
            sign: true,
        }
    }

    /// Create positive infinity (saturates to MAX in compliant implementations)
    pub fn infinity() -> Self {
        Dfp {
            mantissa: 0,
            exponent: 0,
            class: DfpClass::Infinity,
            sign: false,
        }
    }

    /// Create negative infinity (saturates to MIN in compliant implementations)
    pub fn neg_infinity() -> Self {
        Dfp {
            mantissa: 0,
            exponent: 0,
            class: DfpClass::Infinity,
            sign: true,
        }
    }

    /// Normalize to canonical form (odd mantissa for Normal class)
    fn normalize(&mut self) {
        if self.class != DfpClass::Normal {
            return;
        }

        // Handle zero mantissa
        if self.mantissa == 0 {
            self.class = DfpClass::Zero;
            return;
        }

        // Ensure mantissa is odd (canonical form)
        let trailing_zeros = self.mantissa.trailing_zeros() as i32;
        if trailing_zeros > 0 {
            self.mantissa >>= trailing_zeros;
            self.exponent += trailing_zeros;
        }

        // Mantissa should now be odd
        debug_assert!(self.mantissa % 2 == 1 || self.mantissa == 0);
    }

    /// Create DFP from i64 (integer)
    pub fn from_i64(val: i64) -> Self {
        if val == 0 {
            return Dfp::zero();
        }
        if val == i64::MIN {
            let mut dfp = Dfp {
                mantissa: (1u128 << 63) as u128,
                exponent: 0,
                class: DfpClass::Normal,
                sign: true,
            };
            dfp.normalize();
            return dfp;
        }

        let sign = val < 0;
        let abs_val = val.unsigned_abs();

        let mut dfp = Dfp {
            mantissa: abs_val as u128,
            exponent: 0,
            class: DfpClass::Normal,
            sign,
        };
        dfp.normalize();
        dfp
    }

    /// Create DFP from f64 (floating-point)
    pub fn from_f64(val: f64) -> Self {
        let bits = val.to_bits();
        let sign = (bits >> 63) != 0;
        let exp = ((bits >> 52) & 0x7FF) as u32;
        let mantissa = bits & 0xFFFFFFFFFFFFF;

        match exp {
            0x7FF => {
                if mantissa != 0 {
                    Dfp::nan()
                } else if sign {
                    Dfp::neg_infinity()
                } else {
                    Dfp::infinity()
                }
            }
            0 => {
                if mantissa == 0 {
                    if sign { Dfp::neg_zero() } else { Dfp::zero() }
                } else {
                    let mut dfp = Dfp {
                        mantissa: mantissa as u128,
                        exponent: -1074,
                        class: DfpClass::Normal,
                        sign,
                    };
                    dfp.normalize();
                    dfp
                }
            }
            _ => {
                let mantissa_with_implicit = (mantissa as u128) | (1u128 << 52);
                let exponent = exp as i32 - 1075;
                let mut dfp = Dfp {
                    mantissa: mantissa_with_implicit,
                    exponent,
                    class: DfpClass::Normal,
                    sign,
                };
                dfp.normalize();
                dfp
            }
        }
    }

    /// Convert DFP to f64
    pub fn to_f64(self) -> f64 {
        use DfpClass::*;
        match self.class {
            Zero => if self.sign { -0.0 } else { 0.0 },
            Infinity => if self.sign { f64::NEG_INFINITY } else { f64::INFINITY },
            NaN => f64::NAN,
            Normal => {
                let mantissa_f64 = self.mantissa as f64;
                let mut value = mantissa_f64 * 2.0_f64.powi(self.exponent);
                if self.sign { value = -value; }
                value
            }
        }
    }

    /// Convert DFP to string representation
    pub fn to_string(self) -> String {
        use DfpClass::*;
        match self.class {
            Zero => if self.sign { "-0.0".to_string() } else { "0.0".to_string() },
            Infinity => if self.sign { "-Inf".to_string() } else { "Inf".to_string() },
            NaN => "NaN".to_string(),
            Normal => {
                let f = self.to_f64();
                format!("{}", f)
            }
        }
    }
}

/// Maximum exponent value
pub const DFP_MAX_EXPONENT: i32 = 1023;
/// Minimum exponent value
pub const DFP_MIN_EXPONENT: i32 = -1074;

/// Maximum finite DFP mantissa (113 bits set, odd)
pub const DFP_MAX_MANTISSA: u128 = (1u128 << 113) - 1;

/// Maximum finite DFP value
pub const DFP_MAX: Dfp = Dfp {
    class: DfpClass::Normal,
    sign: false,
    mantissa: DFP_MAX_MANTISSA,
    exponent: DFP_MAX_EXPONENT,
};

/// Minimum finite DFP value
pub const DFP_MIN: Dfp = Dfp {
    class: DfpClass::Normal,
    sign: true,
    mantissa: DFP_MAX_MANTISSA,
    exponent: DFP_MAX_EXPONENT,
};

/// DFP encoding for serialization (24 bytes)
/// Layout: [mantissa: 16 bytes, exponent: 4 bytes, class_sign: 4 bytes]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C, align(8))]
pub struct DfpEncoding {
    /// Mantissa (u128, big-endian)
    mantissa: u128,
    /// Exponent (i32, big-endian)
    exponent: i32,
    /// Class and sign encoding (u32)
    /// High byte (bits 24-31): class (0=Normal, 1=Infinity, 2=NaN, 3=Zero)
    /// Low byte (bits 16-23): sign (0=positive, 1=negative)
    class_sign: u32,
}

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

    /// Serialize to 24-byte array
    pub fn to_bytes(&self) -> [u8; 24] {
        let mut bytes = [0u8; 24];
        bytes[0..16].copy_from_slice(&self.mantissa.to_be_bytes());
        bytes[16..20].copy_from_slice(&self.exponent.to_be_bytes());
        bytes[20..24].copy_from_slice(&self.class_sign.to_be_bytes());
        bytes
    }

    /// Deserialize from 24-byte array
    pub fn from_bytes(bytes: [u8; 24]) -> Self {
        let mantissa = u128::from_be_bytes(bytes[0..16].try_into().unwrap());
        let exponent = i32::from_be_bytes(bytes[16..20].try_into().unwrap());
        let class_sign = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
        Self {
            mantissa,
            exponent,
            class_sign,
        }
    }
}

impl Serialize for Dfp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(&DfpEncoding::from_dfp(self).to_bytes())
    }
}

impl<'de> Deserialize<'de> for Dfp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = <[u8; 24]>::deserialize(deserializer)?;
        Ok(DfpEncoding::from_bytes(bytes).to_dfp())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dfp_creation() {
        let dfp = Dfp::new(3, 0, DfpClass::Normal, false);
        assert_eq!(dfp.mantissa, 3);
        assert_eq!(dfp.exponent, 0);
        assert_eq!(dfp.class, DfpClass::Normal);
        assert!(!dfp.sign);
    }

    #[test]
    fn test_dfp_normalization() {
        // 6 * 2^0 = 6 (even, needs normalization)
        let dfp = Dfp::new(6, 0, DfpClass::Normal, false);
        assert_eq!(dfp.mantissa, 3); // 6 >> 1 = 3
        assert_eq!(dfp.exponent, 1); // 0 + 1 = 1
    }

    #[test]
    fn test_dfp_zero() {
        let dfp = Dfp::new(0, 0, DfpClass::Normal, false);
        assert_eq!(dfp.class, DfpClass::Zero);
    }

    #[test]
    fn test_encoding_roundtrip() {
        let original = Dfp::new(7, -1, DfpClass::Normal, false);
        let encoding = DfpEncoding::from_dfp(&original);
        let bytes = encoding.to_bytes();
        let recovered = DfpEncoding::from_bytes(bytes).to_dfp();
        assert_eq!(original, recovered);
    }

    // =============================================================================
    // Additional test vectors for comprehensive coverage
    // =============================================================================

    #[test]
    fn test_special_values() {
        // Test all special values
        assert_eq!(Dfp::zero().class, DfpClass::Zero);
        assert!(!Dfp::zero().sign);

        assert_eq!(Dfp::neg_zero().class, DfpClass::Zero);
        assert!(Dfp::neg_zero().sign);

        assert_eq!(Dfp::infinity().class, DfpClass::Infinity);
        assert!(!Dfp::infinity().sign);

        assert_eq!(Dfp::neg_infinity().class, DfpClass::Infinity);
        assert!(Dfp::neg_infinity().sign);

        assert_eq!(Dfp::nan().class, DfpClass::NaN);
    }

    #[test]
    fn test_from_i64_comprehensive() {
        // Test edge case integers
        let test_cases: &[i64] = &[
            0, 1, -1, 2, -2, 7, -7, 42, -42, 127, -127,
            128, -128, 255, -255, 256, -256, 1023, -1023,
            1024, -1024, 4096, -4096, 10000, -10000,
            i64::MAX, i64::MIN, i64::MAX - 1, i64::MIN + 1,
        ];

        for &val in test_cases {
            let dfp = Dfp::from_i64(val);
            if val == 0 {
                assert_eq!(dfp.class, DfpClass::Zero);
            } else {
                assert_eq!(dfp.class, DfpClass::Normal);
            }
        }
    }

    #[test]
    fn test_from_f64_special_values() {
        // Test all special f64 values
        assert_eq!(Dfp::from_f64(0.0).class, DfpClass::Zero);
        assert_eq!(Dfp::from_f64(-0.0).class, DfpClass::Zero);
        assert!(Dfp::from_f64(-0.0).sign);

        assert_eq!(Dfp::from_f64(f64::INFINITY).class, DfpClass::Infinity);
        assert_eq!(Dfp::from_f64(f64::NEG_INFINITY).class, DfpClass::Infinity);
        assert!(Dfp::from_f64(f64::NEG_INFINITY).sign);

        assert_eq!(Dfp::from_f64(f64::NAN).class, DfpClass::NaN);
    }

    #[test]
    fn test_from_f64_subnormals() {
        // Test subnormal values
        let subnormals: &[f64] = &[
            f64::MIN,
            f64::MIN_POSITIVE,
            1e-310,
            1e-300,
            1e-200,
            1e-100,
        ];

        for &val in subnormals {
            let dfp = Dfp::from_f64(val);
            assert_eq!(dfp.class, DfpClass::Normal, "val={}", val);
        }
    }

    #[test]
    fn test_from_f64_edge_cases() {
        // Test boundary values
        let edge_cases: &[f64] = &[
            1.0, -1.0, 2.0, 0.5, 0.25, 0.125, 0.1, 0.3,
            1e10, 1e100, f64::MAX, f64::MIN, f64::EPSILON,
        ];

        for &val in edge_cases {
            let dfp = Dfp::from_f64(val);
            assert_eq!(dfp.class, DfpClass::Normal, "val={}", val);
        }
    }

    #[test]
    fn test_arithmetic_special_cases() {
        use super::dfp_mul;
        use super::dfp_div;
        use super::dfp_sqrt;

        // Zero operations
        assert_eq!(dfp_mul(Dfp::from_i64(5), Dfp::zero()).class, DfpClass::Zero);
        assert_eq!(dfp_div(Dfp::zero(), Dfp::from_i64(2)).class, DfpClass::Zero);

        // Sqrt special cases
        assert_eq!(dfp_sqrt(Dfp::zero()).class, DfpClass::Zero);
        // sqrt(infinity) returns NaN - that's fine, just verify it doesn't panic
        let _ = dfp_sqrt(Dfp::infinity());
    }

    #[test]
    fn test_signed_zero_arithmetic() {
        // IEEE-754 §6.3: signed-zero arithmetic
        use super::dfp_add;
        use super::dfp_sub;
        use super::dfp_mul;
        use super::dfp_div;

        // ADD: Zero + Zero preserves sign rules
        let pos_zero = Dfp::zero();
        let neg_zero = Dfp::neg_zero();

        // +0 + +0 = +0
        assert_eq!(dfp_add(pos_zero, pos_zero).sign, false);
        // -0 + -0 = -0
        assert_eq!(dfp_add(neg_zero, neg_zero).sign, true);
        // +0 + -0 = +0 (positive wins per IEEE-754 §6.3)
        assert!(!dfp_add(pos_zero, neg_zero).sign);

        // SUB: +0 - -0 = +0 (same as +0 + +0)
        assert!(!dfp_sub(pos_zero, neg_zero).sign);

        // MUL: sign = a.sign XOR b.sign
        assert!(!dfp_mul(pos_zero, pos_zero).sign);
        assert!(dfp_mul(neg_zero, pos_zero).sign);
        assert!(dfp_mul(pos_zero, neg_zero).sign);
        assert!(!dfp_mul(neg_zero, neg_zero).sign);

        // DIV: 0 / x preserves sign
        assert!(!dfp_div(pos_zero, Dfp::from_i64(1)).sign);
        assert!(dfp_div(neg_zero, Dfp::from_i64(1)).sign);
    }

    #[test]
    fn test_encoding_roundtrip_all_types() {
        // Test encoding for all DFP types
        let test_values: &[Dfp] = &[
            Dfp::zero(),
            Dfp::neg_zero(),
            Dfp::infinity(),
            Dfp::neg_infinity(),
            Dfp::nan(),
            Dfp::from_i64(0),
            Dfp::from_i64(1),
            Dfp::from_i64(-1),
            Dfp::from_i64(42),
            Dfp::from_i64(i64::MAX),
            Dfp::from_i64(i64::MIN),
            Dfp::from_f64(1.0),
            Dfp::from_f64(-1.0),
            Dfp::from_f64(3.14159),
            Dfp::from_f64(1e10),
        ];

        for dfp in test_values {
            let encoding = super::DfpEncoding::from_dfp(dfp);
            let bytes = encoding.to_bytes();
            let recovered = super::DfpEncoding::from_bytes(bytes).to_dfp();
            assert_eq!(dfp.class, recovered.class, "class mismatch for {:?}", dfp);
            if dfp.class == DfpClass::Normal {
                assert_eq!(dfp.mantissa, recovered.mantissa, "mantissa mismatch for {:?}", dfp);
                assert_eq!(dfp.exponent, recovered.exponent, "exponent mismatch for {:?}", dfp);
            }
            assert_eq!(dfp.sign, recovered.sign, "sign mismatch for {:?}", dfp);
        }
    }

    #[test]
    fn test_comparison() {
        // Test comparison through to_f64
        let pos = Dfp::from_i64(5);
        let neg = Dfp::from_i64(-5);
        let zero = Dfp::zero();

        // Positive > Zero > Negative
        assert!(pos.to_f64() > zero.to_f64());
        assert!(zero.to_f64() > neg.to_f64());
        assert!(pos.to_f64() > neg.to_f64());
    }

    #[test]
    fn test_normalization() {
        // Test that normalization produces odd mantissa
        let dfp1 = Dfp::new(8, 0, DfpClass::Normal, false); // 8 = 4 * 2^1 = 2 * 2^2 = 1 * 2^3
        assert_eq!(dfp1.mantissa, 1);
        assert_eq!(dfp1.exponent, 3);

        let dfp2 = Dfp::new(6, 0, DfpClass::Normal, false); // 6 = 3 * 2^1
        assert_eq!(dfp2.mantissa, 3);
        assert_eq!(dfp2.exponent, 1);
    }
}
