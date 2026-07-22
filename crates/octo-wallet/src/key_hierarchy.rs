//! Mission key hierarchy per (asker, model) — RFC-0853 §6 (Phase E).
//!
//! Derives a unique mission key per `(asker_did, model)` tuple so that
//! compromising one mission's key material doesn't leak across missions.
//!
//! Hierarchy (top-down):
//! - Root: identity seed (32 bytes, Ed25519)
//! - Mission key: HKDF-BLAKE3(salt=identity_seed, info="cipherocto/mission/v1/{asker_did}:{model}", ikm=identity_seed)
//! - Per-axis subkeys: HKDF-BLAKE3(salt=mission_key, info="cipherocto/mission/v1/{asker_did}:{model}/{axis_id}")
//!
//! Cross-mission isolation property: a key derived for `(A, M1)` is statistically
//! independent of a key derived for `(A, M2)` or `(B, M1)` — separate context
//! strings in HKDF-Extract ensure this.

use blake3::derive_key;
use serde::{Deserialize, Serialize};

use crate::error::WalletError;

/// Mission key (per `(asker_did, model)` tuple). 32 bytes.
///
/// Derived from identity seed via HKDF-BLAKE3 with a tuple-specific context
/// string. Zeroized on drop (the holder is responsible for using it within
/// a scope that ensures drop happens promptly).
#[derive(Clone, ZeroizeOnDrop)]
pub struct MissionKey([u8; 32]);

impl std::fmt::Debug for MissionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MissionKey")
            .field("bytes", &"[REDACTED 32 bytes]")
            .finish()
    }
}

impl AsRef<[u8]> for MissionKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl MissionKey {
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Per-axis subkey (within a mission). 32 bytes.
#[derive(Clone, ZeroizeOnDrop)]
pub struct AxisSubkey([u8; 32]);

impl std::fmt::Debug for AxisSubkey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AxisSubkey")
            .field("bytes", &"[REDACTED 32 bytes]")
            .finish()
    }
}

impl AxisSubkey {
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Mission identifier (composite of asker_did + model).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MissionId {
    pub asker_did: String,
    pub model: String,
}

impl MissionId {
    /// HKDF info string for the mission root key.
    #[must_use]
    pub fn info_string(&self) -> String {
        format!("cipherocto/mission/v1/{}:{}", self.asker_did, self.model)
    }

    /// HKDF info string for an axis subkey within this mission.
    #[must_use]
    pub fn axis_info_string(&self, axis_id: &str) -> String {
        format!("{}/{}", self.info_string(), axis_id)
    }
}

/// Key hierarchy: derive mission keys + per-axis subkeys from identity seed.
///
/// Construct via `KeyHierarchy::new(identity_seed_bytes)`. Stateless — every
/// call to `derive` is deterministic for the same input.
#[derive(Clone, Debug)]
pub struct KeyHierarchy {
    identity_seed: [u8; 32],
}

impl KeyHierarchy {
    /// Construct a key hierarchy from an identity seed.
    #[must_use]
    pub fn new(identity_seed: [u8; 32]) -> Self {
        Self { identity_seed }
    }

    /// Derive the root mission key for a (asker_did, model) tuple.
    /// # Errors
    /// Returns `WalletError::MissionKey` if derivation fails (shouldn't happen
    /// with valid inputs).
    pub fn derive_mission_key(&self, mission: &MissionId) -> Result<MissionKey, WalletError> {
        let info = mission.info_string();
        let derived = derive_key(&info, &self.identity_seed);
        let mut out = [0u8; 32];
        out.copy_from_slice(&derived);
        Ok(MissionKey(out))
    }

    /// Derive a per-axis subkey within a mission.
    /// # Errors
    /// Returns `WalletError::MissionKey` on derivation failure.
    pub fn derive_axis_subkey(
        &self,
        mission: &MissionId,
        axis_id: &str,
    ) -> Result<AxisSubkey, WalletError> {
        let info = mission.axis_info_string(axis_id);
        let derived = derive_key(&info, &self.identity_seed);
        let mut out = [0u8; 32];
        out.copy_from_slice(&derived);
        Ok(AxisSubkey(out))
    }
}

use zeroize::ZeroizeOnDrop;

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_seed() -> [u8; 32] {
        [
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
            24, 25, 26, 27, 28, 29, 30, 31,
        ]
    }

    #[test]
    fn mission_key_deterministic() {
        let h = KeyHierarchy::new(sample_seed());
        let m = MissionId {
            asker_did: "did:octo:a".to_owned(),
            model: "openai/gpt-4".to_owned(),
        };
        let k1 = h.derive_mission_key(&m).unwrap();
        let k2 = h.derive_mission_key(&m).unwrap();
        assert_eq!(k1.as_bytes(), k2.as_bytes());
    }

    #[test]
    fn mission_keys_independent_across_askers() {
        let h = KeyHierarchy::new(sample_seed());
        let m_a = MissionId {
            asker_did: "did:octo:a".to_owned(),
            model: "openai/gpt-4".to_owned(),
        };
        let m_b = MissionId {
            asker_did: "did:octo:b".to_owned(),
            model: "openai/gpt-4".to_owned(),
        };
        let k_a = h.derive_mission_key(&m_a).unwrap();
        let k_b = h.derive_mission_key(&m_b).unwrap();
        assert_ne!(k_a.as_bytes(), k_b.as_bytes());
    }

    #[test]
    fn mission_keys_independent_across_models() {
        let h = KeyHierarchy::new(sample_seed());
        let m_gpt = MissionId {
            asker_did: "did:octo:a".to_owned(),
            model: "openai/gpt-4".to_owned(),
        };
        let m_claude = MissionId {
            asker_did: "did:octo:a".to_owned(),
            model: "anthropic/claude".to_owned(),
        };
        let k_gpt = h.derive_mission_key(&m_gpt).unwrap();
        let k_claude = h.derive_mission_key(&m_claude).unwrap();
        assert_ne!(k_gpt.as_bytes(), k_claude.as_bytes());
    }

    #[test]
    fn axis_subkeys_independent_within_mission() {
        let h = KeyHierarchy::new(sample_seed());
        let m = MissionId {
            asker_did: "did:octo:a".to_owned(),
            model: "openai/gpt-4".to_owned(),
        };
        let k_input = h.derive_axis_subkey(&m, "input_tokens_per_1k").unwrap();
        let k_output = h.derive_axis_subkey(&m, "output_tokens_per_1k").unwrap();
        assert_ne!(k_input.as_bytes(), k_output.as_bytes());
    }

    #[test]
    fn axis_subkey_different_from_mission_key() {
        let h = KeyHierarchy::new(sample_seed());
        let m = MissionId {
            asker_did: "did:octo:a".to_owned(),
            model: "openai/gpt-4".to_owned(),
        };
        let mk = h.derive_mission_key(&m).unwrap();
        let sk = h.derive_axis_subkey(&m, "input_tokens_per_1k").unwrap();
        assert_ne!(mk.as_bytes(), sk.as_bytes());
    }

    #[test]
    fn info_string_format() {
        let m = MissionId {
            asker_did: "did:octo:a".to_owned(),
            model: "openai/gpt-4".to_owned(),
        };
        assert_eq!(
            m.info_string(),
            "cipherocto/mission/v1/did:octo:a:openai/gpt-4"
        );
        assert_eq!(
            m.axis_info_string("input_tokens_per_1k"),
            "cipherocto/mission/v1/did:octo:a:openai/gpt-4/input_tokens_per_1k"
        );
    }

    #[test]
    fn different_identities_yield_different_keys() {
        let seed_a = sample_seed();
        let mut seed_b = sample_seed();
        seed_b[0] = 0xff;
        let h_a = KeyHierarchy::new(seed_a);
        let h_b = KeyHierarchy::new(seed_b);
        let m = MissionId {
            asker_did: "did:octo:a".to_owned(),
            model: "openai/gpt-4".to_owned(),
        };
        let k_a = h_a.derive_mission_key(&m).unwrap();
        let k_b = h_b.derive_mission_key(&m).unwrap();
        assert_ne!(k_a.as_bytes(), k_b.as_bytes());
    }
}
