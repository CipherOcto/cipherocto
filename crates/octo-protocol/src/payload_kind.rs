//! Payload kind discriminator (RFC-0871 §Data Structures).
//!
//! 128-bit UUID per RFC-0965 caveat discriminator pattern (16 bytes instead of 1).
//! Old code fails-closed on unknown discriminators (RFC-0965 §3.2 pattern).
//! No central enum: each new payload kind = new RFC + new UUID allocation.

use borsh::{BorshDeserialize, BorshSerialize};

/// 128-bit payload discriminator (UUID-shaped).
///
/// RFC-0871 §Data Structures. Wire form is a flat 16-byte big-endian UUID.
/// RFC-allocated namespace + user-extension range; see [`rfc_namespace`] /
/// [`user_extension_range`] / [`capability_extension_range`].
///
/// No `Display` / `FromStr` is provided — `PayloadKindId` is opaque on the
/// wire and meaningful only in the context of an RFC-allocated range. Cross-
/// mission identifiers are exchanged via their human-readable RFC-XXXX number,
/// not via the wire bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct PayloadKindId(pub [u8; 16]);

impl PayloadKindId {
    /// Wrap a 16-byte buffer. No validation — caller must ensure the bytes
    /// were sourced from an RFC-allocated range or user-extension registration.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Borrow the inner 16-byte buffer.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// True if the discriminator sits in the RFC-allocated namespace
    /// (`0x0009_0000` … `0x0009_FFFF` for identity-substrate RFCs, the historical
    /// placeholder used by RFC-0871 §Test Vectors TV1).
    #[must_use]
    pub fn is_rfc_allocated(&self) -> bool {
        rfc_namespace().contains(&self.0)
    }

    /// True if the discriminator sits in the capability-extension namespace
    /// (RFC-0965 reserved range `0x0010_0000` … `0x0010_FFFF`).
    #[must_use]
    pub fn is_capability_extension(&self) -> bool {
        capability_extension_range().contains(&self.0)
    }

    /// True if the discriminator sits in the user-extension namespace
    /// (`0xFFFF_FF00` … `0xFFFF_FFFF`).
    #[must_use]
    pub fn is_user_extension(&self) -> bool {
        user_extension_range().contains(&self.0)
    }
}

/// RFC-allocated namespace (`0x0009_0000_0000_0000_0000_0000_0000_0000`
/// … `0x0009_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF`, first 16 bits = `0x0009`).
///
/// `0x0009` is the historical placeholder used by RFC-0871 §TV1 — concrete
/// sub-ranges are allocated per-RFC.
pub const fn rfc_namespace() -> RangeU128 {
    RangeU128 {
        start: 0x0009_0000_0000_0000_0000_0000_0000_0000,
        end: 0x0009_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF,
    }
}

/// Capability-extension namespace (RFC-0965 reserved range, first 16 bits = `0x0010`).
pub const fn capability_extension_range() -> RangeU128 {
    RangeU128 {
        start: 0x0010_0000_0000_0000_0000_0000_0000_0000,
        end: 0x0010_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF,
    }
}

/// User-extension namespace (last 256 values of UUID space).
pub const fn user_extension_range() -> RangeU128 {
    RangeU128 {
        start: 0xFFFF_FF00_0000_0000_0000_0000_0000_0000,
        end: 0xFFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF,
    }
}

/// Inclusive-exclusive range over the 128-bit UUID space.
#[derive(Clone, Copy, Debug)]
pub struct RangeU128 {
    /// Inclusive start.
    pub start: u128,
    /// Inclusive end.
    pub end: u128,
}

impl RangeU128 {
    /// True if `value` falls within the range.
    #[must_use]
    pub fn contains(&self, bytes: &[u8; 16]) -> bool {
        let v = u128::from_be_bytes(*bytes);
        v >= self.start && v <= self.end
    }
}

/// Identity-resolve payload kind (RFC-0871 §Test Vectors TV1).
///
/// UUID: `0x0009:0001:0000:0000:0000:0000:0000:0001`
pub const IDENTITY_RESOLVE: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
]);

/// Wallet sign Ed25519 (RFC-0871 §Wallet Node Lifecycle, Phase 2 mission 0871a).
///
/// UUID: `0x0009:0002:0000:0000:0000:0000:0000:0001`
pub const WALLET_SIGN_ED25519: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
]);

/// Wallet mint capability (RFC-0871 §Wallet Node Lifecycle, Phase 2 mission 0871a).
///
/// UUID: `0x0009:0002:0000:0000:0000:0000:0000:0002`
pub const WALLET_MINT_CAPABILITY: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02,
]);

/// Wallet attenuate capability (RFC-0871 §Wallet Node Lifecycle, Phase 2 mission 0871a).
///
/// UUID: `0x0009:0002:0000:0000:0000:0000:0000:0003`
pub const WALLET_ATTENUATE_CAPABILITY: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03,
]);

/// Wallet resolve DID (RFC-0871 §Test Vectors TV7).
///
/// UUID: `0x0009:0002:0000:0000:0000:0000:0000:0004`
pub const WALLET_RESOLVE_DID: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04,
]);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_resolve_uuid_matches_tv1() {
        // RFC-0871 §TV1: payload_kind = UUID 0x0009:0001:0000:0000:0000:0000:0000:0001
        let expected: [u8; 16] = [
            0x00, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x01,
        ];
        assert_eq!(IDENTITY_RESOLVE.0, expected);
    }

    #[test]
    fn rfc_namespace_classification() {
        assert!(IDENTITY_RESOLVE.is_rfc_allocated());
        assert!(!IDENTITY_RESOLVE.is_capability_extension());
        assert!(!IDENTITY_RESOLVE.is_user_extension());
    }

    #[test]
    fn user_extension_namespace_high_bytes() {
        let ext = PayloadKindId([0xFF; 16]);
        assert!(ext.is_user_extension());
        assert!(!ext.is_rfc_allocated());
    }

    #[test]
    fn borsh_round_trip() {
        let bytes = borsh::to_vec(&IDENTITY_RESOLVE).unwrap();
        let back: PayloadKindId = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, IDENTITY_RESOLVE);
    }
}
