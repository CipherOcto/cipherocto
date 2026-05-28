//! Mission key hierarchy (RFC-0853 §6)

use crate::ocrypt::error::CryptoError;
use blake3;
use hkdf::Hkdf;
use sha2::Sha256;

/// Domain separation strings for mission key derivation
const MISSION_ROOT_DOMAIN: &str = "ocrypt:mission:root:v1";
const TRANSPORT_KEYS_DOMAIN: &str = "ocrypt:mission:transport:v1";
const RELAY_KEYS_DOMAIN: &str = "ocrypt:mission:relay:v1";
const EXECUTION_KEYS_DOMAIN: &str = "ocrypt:mission:execution:v1";

/// Mission key hierarchy — compartmentalized key structure per mission.
///
/// Compromise of one mission MUST NOT compromise other missions,
/// overlay identity, or unrelated sessions.
#[derive(Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct MissionKeyHierarchy {
    /// Root key for this mission
    pub mission_root_key: [u8; 32],
    /// Root for transport encryption keys
    pub transport_keys_root: [u8; 32],
    /// Root for relay encryption keys
    pub relay_keys_root: [u8; 32],
    /// Root for execution keys
    pub execution_keys_root: [u8; 32],
}

impl MissionKeyHierarchy {
    /// Derive mission key hierarchy from a root seed.
    ///
    /// In practice, mission_root_seed is derived from:
    ///   HKDF-BLAKE3(coordinator_identity_key, "ocrypt:mission:seed:v1", mission_id)
    /// or from a DKG ceremony for decentralized mission creation.
    pub fn derive(
        mission_root_seed: &[u8; 32],
        mission_id: &[u8; 32],
    ) -> Result<Self, CryptoError> {
        let mission_root_key =
            Self::hkdf_expand(mission_root_seed, MISSION_ROOT_DOMAIN, mission_id)?;

        let transport_keys_root =
            Self::hkdf_expand(&mission_root_key, TRANSPORT_KEYS_DOMAIN, mission_id)?;

        let relay_keys_root = Self::hkdf_expand(&mission_root_key, RELAY_KEYS_DOMAIN, mission_id)?;

        let execution_keys_root =
            Self::hkdf_expand(&mission_root_key, EXECUTION_KEYS_DOMAIN, mission_id)?;

        Ok(Self {
            mission_root_key,
            transport_keys_root,
            relay_keys_root,
            execution_keys_root,
        })
    }

    /// Derive mission root seed from coordinator identity.
    pub fn derive_seed(
        coordinator_identity_key: &[u8; 32],
        mission_id: &[u8; 32],
    ) -> Result<[u8; 32], CryptoError> {
        Self::hkdf_expand(
            coordinator_identity_key,
            "ocrypt:mission:seed:v1",
            mission_id,
        )
    }

    fn hkdf_expand(
        ikm: &[u8; 32],
        domain: &'static str,
        info: &[u8; 32],
    ) -> Result<[u8; 32], CryptoError> {
        let hk = Hkdf::<Sha256>::new(Some(domain.as_bytes()), ikm);
        let mut output = [0u8; 32];
        hk.expand(info, &mut output)
            .map_err(|_| CryptoError::KeyDerivationFailure { stage: domain })?;
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_deterministic() {
        let seed = [0xF1u8; 32];
        let mission_id = [0x01u8; 32];
        let h1 = MissionKeyHierarchy::derive(&seed, &mission_id).unwrap();
        let h2 = MissionKeyHierarchy::derive(&seed, &mission_id).unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_derive_different_seeds() {
        let seed1 = [0xF1u8; 32];
        let seed2 = [0xF2u8; 32];
        let mission_id = [0x01u8; 32];
        let h1 = MissionKeyHierarchy::derive(&seed1, &mission_id).unwrap();
        let h2 = MissionKeyHierarchy::derive(&seed2, &mission_id).unwrap();
        assert_ne!(h1.mission_root_key, h2.mission_root_key);
    }

    #[test]
    fn test_derive_different_missions() {
        let seed = [0xF1u8; 32];
        let m1 = [0x01u8; 32];
        let m2 = [0x02u8; 32];
        let h1 = MissionKeyHierarchy::derive(&seed, &m1).unwrap();
        let h2 = MissionKeyHierarchy::derive(&seed, &m2).unwrap();
        assert_ne!(h1.mission_root_key, h2.mission_root_key);
    }

    #[test]
    fn test_key_hierarchy_roots_differ() {
        let seed = [0xF1u8; 32];
        let mission_id = [0x01u8; 32];
        let h = MissionKeyHierarchy::derive(&seed, &mission_id).unwrap();
        assert_ne!(h.mission_root_key, h.transport_keys_root);
        assert_ne!(h.transport_keys_root, h.relay_keys_root);
        assert_ne!(h.relay_keys_root, h.execution_keys_root);
    }

    #[test]
    fn test_derive_seed_deterministic() {
        let key = [0x42u8; 32];
        let mission_id = [0x01u8; 32];
        let s1 = MissionKeyHierarchy::derive_seed(&key, &mission_id).unwrap();
        let s2 = MissionKeyHierarchy::derive_seed(&key, &mission_id).unwrap();
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_derive_seed_different_keys() {
        let key1 = [0x42u8; 32];
        let key2 = [0x43u8; 32];
        let mission_id = [0x01u8; 32];
        let s1 = MissionKeyHierarchy::derive_seed(&key1, &mission_id).unwrap();
        let s2 = MissionKeyHierarchy::derive_seed(&key2, &mission_id).unwrap();
        assert_ne!(s1, s2);
    }

    #[test]
    fn test_mission_key_size() {
        let seed = [0xF1u8; 32];
        let mission_id = [0x01u8; 32];
        let h = MissionKeyHierarchy::derive(&seed, &mission_id).unwrap();
        assert_eq!(h.mission_root_key.len(), 32);
        assert_eq!(h.transport_keys_root.len(), 32);
        assert_eq!(h.relay_keys_root.len(), 32);
        assert_eq!(h.execution_keys_root.len(), 32);
    }
}
