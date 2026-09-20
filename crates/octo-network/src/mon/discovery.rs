//! Mission Discovery (RFC-0855 §8)
//!
//! Mission discovery with 5 scopes, advertisement generation,
//! scope-based isolation, and GDP integration.

use std::collections::BTreeMap;

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

/// Cache for mission advertisements (RFC-0011-h §Substrate-Additions
/// row G23 + RFC-0011-m Phase 5). Backing store keyed by the
/// BLAKE3-256 advertisement hash per RFC-0855 §8.2 deterministic-key
/// contract. BTreeMap chosen over HashMap for deterministic iteration
/// order per RFC-0011-h §Output Envelope order determinism.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MissionAdvertisementCache {
    entries: BTreeMap<[u8; 32], MissionAdvertisement>,
}

impl MissionAdvertisementCache {
    /// Substrate-faithful lookup helper for `octo network
    /// discovery advertisement show --advertisement-id <ID>`
    /// (RFC-0011-m Phase 5 G23).
    #[must_use]
    pub fn get(&self, advertisement_id: &[u8; 32]) -> Option<&MissionAdvertisement> {
        self.entries.get(advertisement_id)
    }

    /// Substrate-faithful iterator for `octo network
    /// discovery advertisement show` (RFC-0011-m Phase 5 G23).
    /// Returns gateway-id-keyed iteration per RFC-0855 §8.2.
    /// Owned-key iteration decouples the iterator lifetime from
    /// the cache lifetime per [[cipherocto-design-principles]]
    /// §No premature coupling.
    pub fn iter(&self) -> impl Iterator<Item = ([u8; 32], &MissionAdvertisement)> {
        self.entries.iter().map(|(k, v)| (*k, v))
    }

    /// Insert or replace an advertisement entry (substrate-faithful
    /// registry surface; CLI dispatch does NOT call this — write
    /// paths remain `AdapterUnwired` per Phase 6 follow-on
    /// `0011-h-s-a-discovery-advertisement-persistence`).
    pub fn insert(&mut self, advertisement: MissionAdvertisement) {
        let key = advertisement.advertisement_hash();
        self.entries.insert(key, advertisement);
    }

    /// Number of cached advertisements (operator-side
    /// observability helper).
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty (operator-side
    /// observability helper).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Cache for mission invitations (RFC-0011-h §Substrate-Additions
/// row G24 + RFC-0011-m Phase 5). Backing store keyed by the
/// BLAKE3-256 hash of the invitation signing bytes per
/// RFC-0855 §8.2 deterministic-key contract. BTreeMap chosen over
/// HashMap for deterministic iteration order per RFC-0011-h
/// §Output Envelope order determinism.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MissionInvitationCache {
    entries: BTreeMap<[u8; 32], MissionInvitation>,
}

impl MissionInvitationCache {
    /// Substrate-faithful lookup helper for `octo network
    /// discovery invitation show --invitation-id <ID>`
    /// (RFC-0011-m Phase 5 G24).
    #[must_use]
    pub fn get(&self, invitation_id: &[u8; 32]) -> Option<&MissionInvitation> {
        self.entries.get(invitation_id)
    }

    /// Substrate-faithful iterator for `octo network
    /// discovery invitation show` (RFC-0011-m Phase 5 G24).
    /// Returns gateway-id-keyed iteration per RFC-0855 §8.2.
    /// Owned-key iteration decouples the iterator lifetime from
    /// the cache lifetime per [[cipherocto-design-principles]]
    /// §No premature coupling.
    pub fn iter(&self) -> impl Iterator<Item = ([u8; 32], &MissionInvitation)> {
        self.entries.iter().map(|(k, v)| (*k, v))
    }

    /// Insert or replace an invitation entry (substrate-faithful
    /// registry surface; CLI dispatch does NOT call this — write
    /// paths remain `AdapterUnwired` per Phase 6 follow-on
    /// `0011-h-s-a-discovery-invitation-persistence`). The
    /// invitation key is the BLAKE3-256 hash of `to_signing_bytes()`
    /// per RFC-0855 §8.2 deterministic-key contract.
    pub fn insert(&mut self, invitation: MissionInvitation) {
        let key = *blake3::hash(&invitation.to_signing_bytes()).as_bytes();
        self.entries.insert(key, invitation);
    }

    /// Number of cached invitations (operator-side
    /// observability helper).
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty (operator-side
    /// observability helper).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
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

    // --- MissionAdvertisementCache tests (G23 substrate) ---

    fn make_advertisement(scope: MissionDiscoveryScope) -> MissionAdvertisement {
        MissionAdvertisement::new(
            MissionId::new(1, &[0xAA; 32], 100, &[0xBB; 32], 1),
            [0xBB; 32],
            scope,
            5,
            3,
            [0xCC; 32],
            1000,
        )
    }

    #[test]
    fn t_advertisement_cache_get_returns_none_substrate_faithful() {
        let cache = MissionAdvertisementCache::default();
        let key = [0x11u8; 32];
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn t_advertisement_cache_get_returns_some_after_insert() {
        let mut cache = MissionAdvertisementCache::default();
        let adv = make_advertisement(MissionDiscoveryScope::Public);
        let key = adv.advertisement_hash();
        cache.insert(adv);
        assert!(cache.get(&key).is_some());
    }

    #[test]
    fn t_advertisement_cache_insert_idempotent_for_same_advertisement() {
        let mut cache = MissionAdvertisementCache::default();
        let adv = make_advertisement(MissionDiscoveryScope::Public);
        let key = adv.advertisement_hash();
        cache.insert(adv.clone());
        cache.insert(adv);
        assert_eq!(cache.len(), 1);
        assert!(cache.get(&key).is_some());
    }

    #[test]
    fn t_advertisement_cache_iter_empty_substrate_faithful() {
        let cache = MissionAdvertisementCache::default();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
        let collected: Vec<_> = cache.iter().collect();
        assert_eq!(collected.len(), 0);
    }

    #[test]
    fn t_advertisement_cache_iter_returns_inserted_entries_in_btreemap_order() {
        let mut cache = MissionAdvertisementCache::default();
        // Two advertisements with distinct gateway_ids produce
        // distinct hashes — BTreeMap iter is sorted ascending
        let adv_a = MissionAdvertisement::new(
            MissionId::new(1, &[0xAA; 32], 100, &[0xBB; 32], 1),
            [0xBB; 32],
            MissionDiscoveryScope::Public,
            5,
            3,
            [0xCC; 32],
            1000,
        );
        let adv_b = MissionAdvertisement::new(
            MissionId::new(1, &[0xAA; 32], 100, &[0xBB; 32], 1),
            [0xBB; 32],
            MissionDiscoveryScope::Public,
            5,
            3,
            [0xDD; 32],
            1000,
        );
        let key_a = adv_a.advertisement_hash();
        let key_b = adv_b.advertisement_hash();
        cache.insert(adv_a);
        cache.insert(adv_b);
        let collected: Vec<_> = cache.iter().map(|(k, _)| k).collect();
        assert_eq!(collected.len(), 2);
        // BTreeMap iter is ascending order; verify by sorting
        let mut expected = vec![key_a, key_b];
        expected.sort();
        assert_eq!(collected, expected);
    }

    #[test]
    fn t_advertisement_cache_len_and_is_empty_observability_helpers() {
        let cache = MissionAdvertisementCache::default();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
        let mut cache = cache;
        cache.insert(make_advertisement(MissionDiscoveryScope::Public));
        assert_eq!(cache.len(), 1);
        assert!(!cache.is_empty());
    }

    // --- MissionInvitationCache tests (G24 substrate) ---

    fn make_invitation() -> MissionInvitation {
        MissionInvitation::new(
            MissionId::new(1, &[0xAA; 32], 100, &[0xBB; 32], 1),
            [0xBB; 32],
            [0xCC; 32],
            1000,
        )
    }

    fn invitation_key(inv: &MissionInvitation) -> [u8; 32] {
        *blake3::hash(&inv.to_signing_bytes()).as_bytes()
    }

    #[test]
    fn t_invitation_cache_get_returns_none_substrate_faithful() {
        let cache = MissionInvitationCache::default();
        let key = [0x11u8; 32];
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn t_invitation_cache_get_returns_some_after_insert() {
        let mut cache = MissionInvitationCache::default();
        let inv = make_invitation();
        let key = invitation_key(&inv);
        cache.insert(inv);
        assert!(cache.get(&key).is_some());
    }

    #[test]
    fn t_invitation_cache_insert_idempotent_for_same_invitation() {
        let mut cache = MissionInvitationCache::default();
        let inv = make_invitation();
        let key = invitation_key(&inv);
        cache.insert(inv.clone());
        cache.insert(inv);
        assert_eq!(cache.len(), 1);
        assert!(cache.get(&key).is_some());
    }

    #[test]
    fn t_invitation_cache_iter_empty_substrate_faithful() {
        let cache = MissionInvitationCache::default();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
        let collected: Vec<_> = cache.iter().collect();
        assert_eq!(collected.len(), 0);
    }

    #[test]
    fn t_invitation_cache_iter_returns_inserted_entries_in_btreemap_order() {
        let mut cache = MissionInvitationCache::default();
        let inv_a = MissionInvitation::new(
            MissionId::new(1, &[0xAA; 32], 100, &[0xBB; 32], 1),
            [0xBB; 32],
            [0xCC; 32],
            1000,
        );
        let inv_b = MissionInvitation::new(
            MissionId::new(1, &[0xAA; 32], 100, &[0xBB; 32], 1),
            [0xDD; 32],
            [0xEE; 32],
            2000,
        );
        let key_a = invitation_key(&inv_a);
        let key_b = invitation_key(&inv_b);
        cache.insert(inv_a);
        cache.insert(inv_b);
        let collected: Vec<_> = cache.iter().map(|(k, _)| k).collect();
        assert_eq!(collected.len(), 2);
        let mut expected = vec![key_a, key_b];
        expected.sort();
        assert_eq!(collected, expected);
    }

    #[test]
    fn t_invitation_cache_insert_key_is_blake3_of_signing_bytes() {
        let mut cache = MissionInvitationCache::default();
        let inv = make_invitation();
        let expected_key = *blake3::hash(&inv.to_signing_bytes()).as_bytes();
        cache.insert(inv);
        // The key is derived from signing bytes, not the field
        // tuple — verify by checking the cache has exactly one
        // entry with the expected key
        assert_eq!(cache.len(), 1);
        let collected: Vec<_> = cache.iter().map(|(k, _)| k).collect();
        assert_eq!(collected[0], expected_key);
    }

    #[test]
    fn t_invitation_cache_len_and_is_empty_observability_helpers() {
        let cache = MissionInvitationCache::default();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
        let mut cache = cache;
        cache.insert(make_invitation());
        assert_eq!(cache.len(), 1);
        assert!(!cache.is_empty());
    }
}
