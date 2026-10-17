//! `HolderKind` enum (RFC-0957-A1 §Data Structures).
//!
//! 4-variant discriminator for the holder registry. Each variant maps to a
//! distinct credential type. Stored as `INTEGER` in the schema; the byte
//! values are wire-stable (BLAKE3 commitments over them are deterministic).
//!
//! **Debug redaction:** manual `Debug` impl prints only the variant name,
//! no payload (keeps logs free of credential material).

/// Per RFC-0957-A1 §Data Structures (4-variant discriminator).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum HolderKind {
    /// RFC-0957 v1 macaroon (legacy capability token).
    V1 = 0x00,
    /// RFC-0958 ZK-bearing capability (proof-bundle subclass).
    ZKBearing = 0x01,
    /// RFC-0959-A1 delivery artifact (legacy bearer via dual-mode).
    Bearer = 0x02,
    /// RFC-0970 hop-wrapped capability (forwarding).
    HopCapability = 0x03,
}

impl HolderKind {
    /// Parse from the wire byte. Returns `None` for unknown discriminants.
    #[must_use]
    pub const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0x00 => Some(Self::V1),
            0x01 => Some(Self::ZKBearing),
            0x02 => Some(Self::Bearer),
            0x03 => Some(Self::HopCapability),
            _ => None,
        }
    }

    /// Wire byte constant (inverse of `from_byte`).
    #[must_use]
    pub const fn as_byte(self) -> u8 {
        self as u8
    }
}

impl std::fmt::Debug for HolderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Display variant name only (no payload).
        let name = match self {
            Self::V1 => "V1",
            Self::ZKBearing => "ZKBearing",
            Self::Bearer => "Bearer",
            Self::HopCapability => "HopCapability",
        };
        f.write_str(name)
    }
}

impl std::fmt::Display for HolderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_round_trip_all_variants() {
        for k in [
            HolderKind::V1,
            HolderKind::ZKBearing,
            HolderKind::Bearer,
            HolderKind::HopCapability,
        ] {
            assert_eq!(HolderKind::from_byte(k.as_byte()), Some(k));
        }
    }

    #[test]
    fn byte_values_match_rfc() {
        assert_eq!(HolderKind::V1.as_byte(), 0x00);
        assert_eq!(HolderKind::ZKBearing.as_byte(), 0x01);
        assert_eq!(HolderKind::Bearer.as_byte(), 0x02);
        assert_eq!(HolderKind::HopCapability.as_byte(), 0x03);
    }

    #[test]
    fn unknown_byte_is_none() {
        assert_eq!(HolderKind::from_byte(0xFF), None);
        assert_eq!(HolderKind::from_byte(0x04), None);
    }

    #[test]
    fn debug_is_variant_name_only() {
        assert_eq!(format!("{:?}", HolderKind::V1), "V1");
        assert_eq!(format!("{:?}", HolderKind::ZKBearing), "ZKBearing");
        assert_eq!(format!("{:?}", HolderKind::Bearer), "Bearer");
        assert_eq!(format!("{:?}", HolderKind::HopCapability), "HopCapability");
    }

    #[test]
    fn serde_json_round_trip() {
        for k in [
            HolderKind::V1,
            HolderKind::ZKBearing,
            HolderKind::Bearer,
            HolderKind::HopCapability,
        ] {
            let s = serde_json::to_string(&k).unwrap();
            let back: HolderKind = serde_json::from_str(&s).unwrap();
            assert_eq!(k, back);
        }
    }
}
