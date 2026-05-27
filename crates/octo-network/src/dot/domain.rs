//! Broadcast domain identification (RFC-0850 §3.1)

use crate::dot::error::DotError;

/// Supported platform types for DOT transport
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum PlatformType {
    Telegram = 0x0001,
    Discord = 0x0002,
    Matrix = 0x0003,
    Nostr = 0x0004,
    Signal = 0x0005,
    IRC = 0x0006,
    Slack = 0x0007,
    WhatsApp = 0x0008,
    Webhook = 0x0009,
    NativeP2P = 0x000A,
    Bluetooth = 0x000B,
    LoRa = 0x000C,
    WebRTC = 0x000D,
}

/// Identifies a broadcast domain (group/channel/room) across platforms
///
/// Determinism: domain_hash = BLAKE3-256(normalized_platform_id)
/// Platform IDs MUST be lowercase, trimmed before hashing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct BroadcastDomainId {
    /// Platform type identifier
    pub platform_type: u16,
    /// BLAKE3-256 of platform-specific group/channel/room identifier
    pub domain_hash: [u8; 32],
}

impl BroadcastDomainId {
    /// Create a new domain ID from platform type and identifier.
    ///
    /// The platform_id is normalized (lowercase, trimmed) before hashing.
    pub fn new(platform_type: PlatformType, platform_id: &str) -> Self {
        let normalized = platform_id.trim().to_lowercase();
        let hash = blake3::hash(normalized.as_bytes());
        Self {
            platform_type: platform_type as u16,
            domain_hash: *hash.as_bytes(),
        }
    }

    /// Serialize to canonical bytes (RFC-0126 DCS).
    /// Order: platform_type (2 bytes, big-endian) || domain_hash (32 bytes)
    pub fn to_canonical_bytes(&self) -> [u8; 34] {
        let mut buf = [0u8; 34];
        buf[0..2].copy_from_slice(&self.platform_type.to_be_bytes());
        buf[2..34].copy_from_slice(&self.domain_hash);
        buf
    }

    /// Deserialize from canonical bytes.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, DotError> {
        if bytes.len() < 34 {
            return Err(DotError::Serialization(
                "BroadcastDomainId requires 34 bytes".into(),
            ));
        }
        let platform_type = u16::from_be_bytes([bytes[0], bytes[1]]);
        let mut domain_hash = [0u8; 32];
        domain_hash.copy_from_slice(&bytes[2..34]);
        Ok(Self {
            platform_type,
            domain_hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_id_deterministic() {
        let id1 = BroadcastDomainId::new(PlatformType::Telegram, "-1001234567890");
        let id2 = BroadcastDomainId::new(PlatformType::Telegram, "-1001234567890");
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_domain_id_case_insensitive() {
        let id1 = BroadcastDomainId::new(PlatformType::Telegram, "-1001234567890");
        let id2 = BroadcastDomainId::new(PlatformType::Telegram, "  -1001234567890  ");
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_domain_id_serialization_roundtrip() {
        let id = BroadcastDomainId::new(PlatformType::Discord, "channel:9876543210");
        let bytes = id.to_canonical_bytes();
        let recovered = BroadcastDomainId::from_canonical_bytes(&bytes).unwrap();
        assert_eq!(id, recovered);
    }

    #[test]
    fn test_domain_id_different_platforms() {
        let id1 = BroadcastDomainId::new(PlatformType::Telegram, "group:123");
        let id2 = BroadcastDomainId::new(PlatformType::Discord, "group:123");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_domain_id_from_bytes_too_short() {
        let result = BroadcastDomainId::from_canonical_bytes(&[0u8; 10]);
        assert!(result.is_err());
    }
}
