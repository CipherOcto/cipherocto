//! Mission Key Hierarchy (RFC-0855 §7.1)

use serde::{Deserialize, Serialize};

/// Mission key hierarchy — 4 root keys derived from genesis secret.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct MissionKeyHierarchy {
    pub mission_root_key: [u8; 32],
    pub transport_keys_root: [u8; 32],
    pub relay_keys_root: [u8; 32],
    pub execution_keys_root: [u8; 32],
}

/// Derive mission genesis secret via HKDF-BLAKE3.
///
/// `genesis_secret = HKDF-BLAKE3(salt=mission_hash, ikm=creator_private_key, info="mission-genesis-secret")`
pub fn derive_genesis_secret(creator_private_key: &[u8], mission_hash: &[u8; 32]) -> [u8; 32] {
    let mut okm = [0u8; 32];
    crate::ocrypt::hkdf_blake3(
        mission_hash,
        creator_private_key,
        b"mission-genesis-secret",
        &mut okm,
    );
    okm
}

/// Derive full key hierarchy from genesis secret.
pub fn derive_key_hierarchy(
    genesis_secret: &[u8; 32],
    mission_hash: &[u8; 32],
) -> MissionKeyHierarchy {
    let mut mission_root_key = [0u8; 32];
    crate::ocrypt::hkdf_blake3(
        mission_hash,
        genesis_secret,
        b"mission_root_key",
        &mut mission_root_key,
    );

    let mut transport_keys_root = [0u8; 32];
    crate::ocrypt::hkdf_blake3(
        b"transport",
        &mission_root_key,
        b"transport_keys_root",
        &mut transport_keys_root,
    );

    let mut relay_keys_root = [0u8; 32];
    crate::ocrypt::hkdf_blake3(
        b"relay",
        &mission_root_key,
        b"relay_keys_root",
        &mut relay_keys_root,
    );

    let mut execution_keys_root = [0u8; 32];
    crate::ocrypt::hkdf_blake3(
        b"execution",
        &mission_root_key,
        b"execution_keys_root",
        &mut execution_keys_root,
    );

    MissionKeyHierarchy {
        mission_root_key,
        transport_keys_root,
        relay_keys_root,
        execution_keys_root,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_genesis_secret_deterministic() {
        let key = [42u8; 64];
        let hash = [1u8; 32];
        let s1 = derive_genesis_secret(&key, &hash);
        let s2 = derive_genesis_secret(&key, &hash);
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_genesis_secret_different_keys() {
        let key1 = [1u8; 64];
        let key2 = [2u8; 64];
        let hash = [3u8; 32];
        let s1 = derive_genesis_secret(&key1, &hash);
        let s2 = derive_genesis_secret(&key2, &hash);
        assert_ne!(s1, s2);
    }

    #[test]
    fn test_key_hierarchy_deterministic() {
        let genesis = [42u8; 32];
        let hash = [1u8; 32];
        let kh1 = derive_key_hierarchy(&genesis, &hash);
        let kh2 = derive_key_hierarchy(&genesis, &hash);
        assert_eq!(kh1.mission_root_key, kh2.mission_root_key);
        assert_eq!(kh1.transport_keys_root, kh2.transport_keys_root);
        assert_eq!(kh1.relay_keys_root, kh2.relay_keys_root);
        assert_eq!(kh1.execution_keys_root, kh2.execution_keys_root);
    }

    #[test]
    fn test_key_hierarchy_roots_differ() {
        let genesis = [42u8; 32];
        let hash = [1u8; 32];
        let kh = derive_key_hierarchy(&genesis, &hash);
        assert_ne!(kh.mission_root_key, kh.transport_keys_root);
        assert_ne!(kh.transport_keys_root, kh.relay_keys_root);
        assert_ne!(kh.relay_keys_root, kh.execution_keys_root);
    }

    #[test]
    fn test_key_hierarchy_different_genesis() {
        let hash = [1u8; 32];
        let kh1 = derive_key_hierarchy(&[1u8; 32], &hash);
        let kh2 = derive_key_hierarchy(&[2u8; 32], &hash);
        assert_ne!(kh1.mission_root_key, kh2.mission_root_key);
    }
}
