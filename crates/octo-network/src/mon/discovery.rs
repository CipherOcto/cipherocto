//! Mission Discovery (RFC-0855 §8)
//!
//! Mission discovery with 5 scopes, advertisement generation,
//! scope-based isolation, and GDP integration.

use serde::{Deserialize, Serialize};

use super::mission_id::MissionId;

/// Mission discovery scopes — distinct from GDP's DiscoveryScope.
/// Discriminants start at 0x0100 to avoid collision with GDP (0x0001-0x0006).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u16)]
pub enum MissionDiscoveryScope {
    Public = 0x0100,
    InviteOnly = 0x0101,
    Stealth = 0x0102,
    Federated = 0x0103,
    Ephemeral = 0x0104,
}

impl MissionDiscoveryScope {
    /// Parse from u16 value.
    pub fn from_u16(val: u16) -> Option<Self> {
        match val {
            0x0100 => Some(Self::Public),
            0x0101 => Some(Self::InviteOnly),
            0x0102 => Some(Self::Stealth),
            0x0103 => Some(Self::Federated),
            0x0104 => Some(Self::Ephemeral),
            _ => None,
        }
    }

    /// Whether this scope requires encrypted advertisements.
    pub fn requires_encryption(&self) -> bool {
        matches!(self, Self::Stealth | Self::InviteOnly)
    }

    /// Default TTL for advertisements in this scope.
    pub fn default_ttl(&self) -> u16 {
        match self {
            Self::Public => 20,
            Self::InviteOnly => 10,
            Self::Stealth => 5,
            Self::Federated => 10,
            Self::Ephemeral => EPHEMERAL_ADVERTISEMENT_TTL,
        }
    }
}

/// Map MissionDiscoveryScope to GDP DiscoveryScope (RFC-0855 §8.2).
pub fn scope_to_gdp_scope(scope: MissionDiscoveryScope) -> u16 {
    match scope {
        MissionDiscoveryScope::Public => 0x0004,     // Global
        MissionDiscoveryScope::InviteOnly => 0x0005, // Private
        MissionDiscoveryScope::Stealth => 0x0005, // Private (stealth handled at advertisement encryption level)
        MissionDiscoveryScope::Federated => 0x0002, // Regional
        MissionDiscoveryScope::Ephemeral => 0x0003, // Mission
    }
}

/// Ephemeral mission TTL (hops) for advertisements.
pub const EPHEMERAL_ADVERTISEMENT_TTL: u16 = 5;

/// Mission advertisement (RFC-0855 §8.2).
///
/// Advertises a mission's existence and properties to the overlay.
/// Stealth missions encrypt the advertisement so only holders of
/// the discovery capability key can decrypt it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct MissionAdvertisement {
    /// Mission identifier
    pub mission_id: MissionId,
    /// Serialized mission descriptor (type, governance, etc.)
    pub descriptor_hash: [u8; 32],
    /// Mission discovery scope
    pub scope: MissionDiscoveryScope,
    /// Current participant count
    pub participant_count: u32,
    /// Minimum participants for formation
    pub min_participants: u32,
    /// Gateway providing this advertisement
    pub gateway_id: [u8; 32],
    /// Logical timestamp
    pub logical_timestamp: u64,
    /// Ed25519 signature
    #[serde(with = "serde_bytes")]
    pub signature: [u8; 64],
}

impl MissionAdvertisement {
    /// Create a new unsigned advertisement.
    pub fn new(
        mission_id: MissionId,
        descriptor_hash: [u8; 32],
        scope: MissionDiscoveryScope,
        participant_count: u32,
        min_participants: u32,
        gateway_id: [u8; 32],
        logical_timestamp: u64,
    ) -> Self {
        Self {
            mission_id,
            descriptor_hash,
            scope,
            participant_count,
            min_participants,
            gateway_id,
            logical_timestamp,
            signature: [0u8; 64],
        }
    }

    /// Compute signing bytes for this advertisement.
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.mission_id.to_canonical_bytes());
        bytes.extend_from_slice(&self.descriptor_hash);
        bytes.extend_from_slice(&(self.scope as u16).to_be_bytes());
        bytes.extend_from_slice(&self.participant_count.to_be_bytes());
        bytes.extend_from_slice(&self.min_participants.to_be_bytes());
        bytes.extend_from_slice(&self.gateway_id);
        bytes.extend_from_slice(&self.logical_timestamp.to_be_bytes());
        bytes
    }

    /// Derive BLAKE3-256 hash of this advertisement.
    pub fn advertisement_hash(&self) -> [u8; 32] {
        *blake3::hash(&self.to_signing_bytes()).as_bytes()
    }

    /// Whether this advertisement should be encrypted (stealth/invite-only).
    pub fn is_encrypted(&self) -> bool {
        self.scope.requires_encryption()
    }

    /// Whether this advertisement's TTL has been exceeded based on hop count.
    pub fn is_ttl_exceeded(&self, current_hops: u16) -> bool {
        current_hops >= self.scope.default_ttl()
    }
}

/// Invitation for invite-only missions (RFC-0855 §8.2).
///
/// Contains the Coordinator's signature authorizing a specific gateway
/// to join the mission.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MissionInvitation {
    /// Mission being invited to
    pub mission_id: MissionId,
    /// Gateway being invited
    pub invitee_gateway_id: [u8; 32],
    /// Coordinator's gateway ID
    pub coordinator_gateway_id: [u8; 32],
    /// Logical timestamp of invitation
    pub logical_timestamp: u64,
    /// Coordinator's Ed25519 signature
    #[serde(with = "serde_bytes")]
    pub signature: [u8; 64],
}

impl MissionInvitation {
    /// Create a new unsigned invitation.
    pub fn new(
        mission_id: MissionId,
        invitee_gateway_id: [u8; 32],
        coordinator_gateway_id: [u8; 32],
        logical_timestamp: u64,
    ) -> Self {
        Self {
            mission_id,
            invitee_gateway_id,
            coordinator_gateway_id,
            logical_timestamp,
            signature: [0u8; 64],
        }
    }

    /// Compute signing bytes.
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.mission_id.to_canonical_bytes());
        bytes.extend_from_slice(&self.invitee_gateway_id);
        bytes.extend_from_slice(&self.coordinator_gateway_id);
        bytes.extend_from_slice(&self.logical_timestamp.to_be_bytes());
        bytes
    }
}

/// Validate that a gateway is authorized to discover a mission.
///
/// - Public: anyone can discover
/// - Invite-only: requires valid invitation
/// - Stealth: requires discovery key
/// - Federated: requires membership in trusted domain
/// - Ephemeral: anyone can discover (short TTL)
pub fn is_discovery_authorized(
    scope: MissionDiscoveryScope,
    has_invitation: bool,
    has_discovery_key: bool,
) -> bool {
    match scope {
        MissionDiscoveryScope::Public => true,
        MissionDiscoveryScope::InviteOnly => has_invitation,
        MissionDiscoveryScope::Stealth => has_discovery_key,
        MissionDiscoveryScope::Federated => true, // domain membership checked at transport layer
        MissionDiscoveryScope::Ephemeral => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_repr_values() {
        assert_eq!(MissionDiscoveryScope::Public as u16, 0x0100);
        assert_eq!(MissionDiscoveryScope::Ephemeral as u16, 0x0104);
    }

    #[test]
    fn test_scope_no_collision_with_gdp() {
        assert!(MissionDiscoveryScope::Public as u16 > 0x00FF);
    }

    #[test]
    fn test_scope_to_gdp_mapping() {
        assert_eq!(scope_to_gdp_scope(MissionDiscoveryScope::Public), 0x0004);
        assert_eq!(
            scope_to_gdp_scope(MissionDiscoveryScope::InviteOnly),
            0x0005
        );
        assert_eq!(scope_to_gdp_scope(MissionDiscoveryScope::Stealth), 0x0005);
        assert_eq!(scope_to_gdp_scope(MissionDiscoveryScope::Federated), 0x0002);
        assert_eq!(scope_to_gdp_scope(MissionDiscoveryScope::Ephemeral), 0x0003);
    }

    #[test]
    fn test_scope_from_u16() {
        assert_eq!(
            MissionDiscoveryScope::from_u16(0x0100),
            Some(MissionDiscoveryScope::Public)
        );
        assert_eq!(
            MissionDiscoveryScope::from_u16(0x0104),
            Some(MissionDiscoveryScope::Ephemeral)
        );
        assert_eq!(MissionDiscoveryScope::from_u16(0x0099), None);
    }

    #[test]
    fn test_scope_requires_encryption() {
        assert!(!MissionDiscoveryScope::Public.requires_encryption());
        assert!(MissionDiscoveryScope::InviteOnly.requires_encryption());
        assert!(MissionDiscoveryScope::Stealth.requires_encryption());
        assert!(!MissionDiscoveryScope::Federated.requires_encryption());
        assert!(!MissionDiscoveryScope::Ephemeral.requires_encryption());
    }

    #[test]
    fn test_scope_default_ttl() {
        assert_eq!(MissionDiscoveryScope::Public.default_ttl(), 20);
        assert_eq!(MissionDiscoveryScope::Stealth.default_ttl(), 5);
        assert_eq!(MissionDiscoveryScope::Ephemeral.default_ttl(), 5);
    }

    #[test]
    fn test_advertisement_new() {
        let adv = MissionAdvertisement::new(
            MissionId::new(1, &[0xAA; 32], 100, &[0xBB; 32], 1),
            [0xBB; 32],
            MissionDiscoveryScope::Public,
            5,
            3,
            [0xCC; 32],
            1000,
        );
        assert_eq!(adv.participant_count, 5);
        assert_eq!(adv.min_participants, 3);
        assert_eq!(adv.scope, MissionDiscoveryScope::Public);
    }

    #[test]
    fn test_advertisement_hash_deterministic() {
        let adv = MissionAdvertisement::new(
            MissionId::new(1, &[0xAA; 32], 100, &[0xBB; 32], 1),
            [0xBB; 32],
            MissionDiscoveryScope::Public,
            5,
            3,
            [0xCC; 32],
            1000,
        );
        let h1 = adv.advertisement_hash();
        let h2 = adv.advertisement_hash();
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_advertisement_encrypted_for_stealth() {
        let adv = MissionAdvertisement::new(
            MissionId::new(1, &[0xAA; 32], 100, &[0xBB; 32], 1),
            [0xBB; 32],
            MissionDiscoveryScope::Stealth,
            5,
            3,
            [0xCC; 32],
            1000,
        );
        assert!(adv.is_encrypted());
    }

    #[test]
    fn test_advertisement_not_encrypted_for_public() {
        let adv = MissionAdvertisement::new(
            MissionId::new(1, &[0xAA; 32], 100, &[0xBB; 32], 1),
            [0xBB; 32],
            MissionDiscoveryScope::Public,
            5,
            3,
            [0xCC; 32],
            1000,
        );
        assert!(!adv.is_encrypted());
    }

    #[test]
    fn test_advertisement_expiry() {
        let adv = MissionAdvertisement::new(
            MissionId::new(1, &[0xAA; 32], 100, &[0xBB; 32], 1),
            [0xBB; 32],
            MissionDiscoveryScope::Stealth,
            5,
            3,
            [0xCC; 32],
            1000,
        );
        assert!(!adv.is_ttl_exceeded(3));
        assert!(adv.is_ttl_exceeded(5));
        assert!(adv.is_ttl_exceeded(10));
    }

    #[test]
    fn test_invitation_new() {
        let inv = MissionInvitation::new(
            MissionId::new(1, &[0xAA; 32], 100, &[0xBB; 32], 1),
            [0xBB; 32],
            [0xCC; 32],
            1000,
        );
        assert_eq!(inv.invitee_gateway_id, [0xBB; 32]);
        assert_eq!(inv.coordinator_gateway_id, [0xCC; 32]);
    }

    #[test]
    fn test_invitation_signing_bytes_deterministic() {
        let inv = MissionInvitation::new(
            MissionId::new(1, &[0xAA; 32], 100, &[0xBB; 32], 1),
            [0xBB; 32],
            [0xCC; 32],
            1000,
        );
        let b1 = inv.to_signing_bytes();
        let b2 = inv.to_signing_bytes();
        assert_eq!(b1, b2);
    }

    #[test]
    fn test_discovery_authorization_public() {
        assert!(is_discovery_authorized(
            MissionDiscoveryScope::Public,
            false,
            false
        ));
    }

    #[test]
    fn test_discovery_authorization_invite_only_requires_invitation() {
        assert!(!is_discovery_authorized(
            MissionDiscoveryScope::InviteOnly,
            false,
            false
        ));
        assert!(is_discovery_authorized(
            MissionDiscoveryScope::InviteOnly,
            true,
            false
        ));
    }

    #[test]
    fn test_discovery_authorization_stealth_requires_key() {
        assert!(!is_discovery_authorized(
            MissionDiscoveryScope::Stealth,
            false,
            false
        ));
        assert!(is_discovery_authorized(
            MissionDiscoveryScope::Stealth,
            false,
            true
        ));
    }

    #[test]
    fn test_discovery_authorization_ephemeral_always_allowed() {
        assert!(is_discovery_authorized(
            MissionDiscoveryScope::Ephemeral,
            false,
            false
        ));
    }
}
