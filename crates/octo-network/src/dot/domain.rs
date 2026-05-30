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
    Bluesky = 0x000E,
    Twitter = 0x000F,
    Reddit = 0x0010,
    WeChat = 0x0011,
    DingTalk = 0x0012,
    Lark = 0x0013,
    QQ = 0x0014,
}

impl PlatformType {
    /// Convert from u16 value to enum variant.
    pub fn from_u16(val: u16) -> Option<Self> {
        match val {
            0x0001 => Some(Self::Telegram),
            0x0002 => Some(Self::Discord),
            0x0003 => Some(Self::Matrix),
            0x0004 => Some(Self::Nostr),
            0x0005 => Some(Self::Signal),
            0x0006 => Some(Self::IRC),
            0x0007 => Some(Self::Slack),
            0x0008 => Some(Self::WhatsApp),
            0x0009 => Some(Self::Webhook),
            0x000A => Some(Self::NativeP2P),
            0x000B => Some(Self::Bluetooth),
            0x000C => Some(Self::LoRa),
            0x000D => Some(Self::WebRTC),
            0x000E => Some(Self::Bluesky),
            0x000F => Some(Self::Twitter),
            0x0010 => Some(Self::Reddit),
            0x0011 => Some(Self::WeChat),
            0x0012 => Some(Self::DingTalk),
            0x0013 => Some(Self::Lark),
            0x0014 => Some(Self::QQ),
            _ => None,
        }
    }
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
    /// The hash input includes the platform type prefix to prevent cross-platform
    /// collisions (RFC-0850 S3.1): `BLAKE3-256("telegram:{group_id}")`.
    pub fn new(platform_type: PlatformType, platform_id: &str) -> Self {
        let normalized = platform_id.trim().to_lowercase();
        let prefix = match platform_type {
            PlatformType::Telegram => "telegram",
            PlatformType::Discord => "discord",
            PlatformType::Matrix => "matrix",
            PlatformType::Nostr => "nostr",
            PlatformType::Signal => "signal",
            PlatformType::IRC => "irc",
            PlatformType::Slack => "slack",
            PlatformType::WhatsApp => "whatsapp",
            PlatformType::Webhook => "webhook",
            PlatformType::NativeP2P => "nativep2p",
            PlatformType::Bluetooth => "bluetooth",
            PlatformType::LoRa => "lora",
            PlatformType::WebRTC => "webrtc",
            PlatformType::Bluesky => "bluesky",
            PlatformType::Twitter => "twitter",
            PlatformType::Reddit => "reddit",
            PlatformType::WeChat => "wechat",
            PlatformType::DingTalk => "dingtalk",
            PlatformType::Lark => "lark",
            PlatformType::QQ => "qq",
        };
        let hash_input = format!("{}:{}", prefix, normalized);
        let hash = blake3::hash(hash_input.as_bytes());
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
        // Validate platform_type corresponds to a known PlatformType variant
        if PlatformType::from_u16(platform_type).is_none() {
            return Err(DotError::Serialization(format!(
                "invalid platform_type: {:#06x}",
                platform_type
            )));
        }
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
