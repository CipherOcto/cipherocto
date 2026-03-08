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

pub use arithmetic::{dfp_add, dfp_div, dfp_mul, dfp_sqrt, dfp_sub};

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
}
