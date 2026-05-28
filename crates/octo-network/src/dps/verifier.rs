//! VerifierRegistry — RFC-0854 §8
//!
//! BTreeMap-backed registry for deterministic iteration.
//! Each entry holds a proof suite and verification key.

use std::collections::BTreeMap;

use crate::dps::suite::{ProofSuite, ProofSystemId};
use crate::dps::DpsError;

/// A registered verifier entry with verification key.
#[derive(Debug, Clone)]
pub struct VerifierEntry {
    /// Proof suite configuration
    pub proof_suite: ProofSuite,
    /// Serialized verification key (opaque)
    pub verification_key: Vec<u8>,
    /// Epoch when this entry was registered
    pub registered_at: u64,
    /// Optional expiration epoch
    pub expires_at: Option<u64>,
}

/// Verifier registry — maps proof system ID to verifier entry.
///
/// Uses BTreeMap for deterministic iteration.
#[derive(Debug, Clone)]
pub struct VerifierRegistry {
    entries: BTreeMap<u16, VerifierEntry>,
}

impl VerifierRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// Register a verifier entry.
    pub fn register(&mut self, entry: VerifierEntry) {
        self.entries
            .insert(entry.proof_suite.system_id.as_u16(), entry);
    }

    /// Get a verifier entry by proof system ID.
    pub fn get(&self, system_id: ProofSystemId) -> Option<&VerifierEntry> {
        self.entries.get(&system_id.as_u16())
    }

    /// Check if a proof system is registered.
    pub fn contains(&self, system_id: ProofSystemId) -> bool {
        self.entries.contains_key(&system_id.as_u16())
    }

    /// Remove expired entries. Returns count removed.
    pub fn evict_expired(&mut self, current_epoch: u64) -> usize {
        let before = self.entries.len();
        self.entries
            .retain(|_, entry| entry.expires_at.map_or(true, |exp| exp > current_epoch));
        before - self.entries.len()
    }

    /// Number of registered verifiers.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if registry is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate entries in deterministic order (by system_id).
    pub fn iter(&self) -> impl Iterator<Item = (&u16, &VerifierEntry)> {
        self.entries.iter()
    }
}

impl Default for VerifierRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dps::suite::{ProofCircuitModel, ProofExecutionClass};

    fn make_entry(id: ProofSystemId) -> VerifierEntry {
        VerifierEntry {
            proof_suite: ProofSuite::new(id, ProofCircuitModel::AIR, ProofExecutionClass::ClassA),
            verification_key: vec![0xAA; 64],
            registered_at: 100,
            expires_at: None,
        }
    }

    fn make_expiring_entry(id: ProofSystemId, expires: u64) -> VerifierEntry {
        VerifierEntry {
            proof_suite: ProofSuite::new(id, ProofCircuitModel::R1CS, ProofExecutionClass::ClassB),
            verification_key: vec![0xBB; 32],
            registered_at: 100,
            expires_at: Some(expires),
        }
    }

    #[test]
    fn test_registry_new() {
        let reg = VerifierRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn test_registry_register_and_get() {
        let mut reg = VerifierRegistry::new();
        reg.register(make_entry(ProofSystemId::STWO));
        assert_eq!(reg.len(), 1);
        assert!(reg.contains(ProofSystemId::STWO));
        let entry = reg.get(ProofSystemId::STWO).unwrap();
        assert_eq!(entry.proof_suite.system_id, ProofSystemId::STWO);
    }

    #[test]
    fn test_registry_get_missing() {
        let reg = VerifierRegistry::new();
        assert!(reg.get(ProofSystemId::Cairo).is_none());
    }

    #[test]
    fn test_registry_overwrite() {
        let mut reg = VerifierRegistry::new();
        reg.register(make_entry(ProofSystemId::STWO));
        reg.register(make_entry(ProofSystemId::STWO)); // overwrite
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn test_registry_evict_expired() {
        let mut reg = VerifierRegistry::new();
        reg.register(make_entry(ProofSystemId::STWO)); // no expiry
        reg.register(make_expiring_entry(ProofSystemId::PLONK, 200));
        reg.register(make_expiring_entry(ProofSystemId::Halo2, 300));
        let evicted = reg.evict_expired(250);
        assert_eq!(evicted, 1); // PLONK evicted, Halo2 and STWO remain
        assert!(reg.contains(ProofSystemId::STWO));
        assert!(!reg.contains(ProofSystemId::PLONK));
        assert!(reg.contains(ProofSystemId::Halo2));
    }

    #[test]
    fn test_registry_deterministic_iteration() {
        let mut reg = VerifierRegistry::new();
        reg.register(make_entry(ProofSystemId::Cairo));
        reg.register(make_entry(ProofSystemId::STWO));
        reg.register(make_entry(ProofSystemId::PLONK));
        let ids: Vec<u16> = reg.iter().map(|(k, _)| *k).collect();
        // BTreeMap sorts by key — 0x0001, 0x0007, 0x0008
        assert_eq!(ids, vec![0x0001, 0x0007, 0x0008]);
    }

    #[test]
    fn test_registry_default() {
        let reg = VerifierRegistry::default();
        assert!(reg.is_empty());
    }
}
