//! VerifierRegistry — RFC-0854 §8
//!
//! BTreeMap-backed registry for deterministic iteration.
//! Each entry holds a proof suite and verification key.

use std::collections::BTreeMap;

use crate::dps::suite::{ProofSuite, ProofSuiteId};

/// A registered verifier entry with verification key.
#[derive(Debug, Clone)]
pub struct VerifierEntry {
    /// Proof suite composite key
    pub suite_id: ProofSuiteId,
    /// Proof suite configuration
    pub proof_suite: ProofSuite,
    /// Serialized verification key (opaque)
    pub verification_key: Vec<u8>,
    /// Epoch when this entry was registered
    pub registered_at: u64,
    /// Optional expiration epoch
    pub expires_at: Option<u64>,
}

/// Verifier registry — maps ProofSuiteId hash to verifier entry.
///
/// Uses BTreeMap for deterministic iteration.
#[derive(Debug, Clone)]
pub struct VerifierRegistry {
    entries: BTreeMap<[u8; 32], VerifierEntry>,
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
        self.entries.insert(entry.suite_id.to_hash(), entry);
    }

    /// Get a verifier entry by proof suite ID.
    pub fn get(&self, suite_id: &ProofSuiteId) -> Option<&VerifierEntry> {
        self.entries.get(&suite_id.to_hash())
    }

    /// Check if a proof suite is registered.
    pub fn contains(&self, suite_id: &ProofSuiteId) -> bool {
        self.entries.contains_key(&suite_id.to_hash())
    }

    /// Remove expired entries. Returns count removed.
    pub fn evict_expired(&mut self, current_epoch: u64) -> usize {
        let before = self.entries.len();
        self.entries
            .retain(|_, entry| entry.expires_at.is_none_or(|exp| exp > current_epoch));
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

    /// Iterate entries in deterministic order (by suite_id hash).
    pub fn iter(&self) -> impl Iterator<Item = (&[u8; 32], &VerifierEntry)> {
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
    use crate::dps::suite::{ProofCircuitModel, ProofExecutionClass, ProofSystemId};

    fn make_suite_id(system: ProofSystemId) -> ProofSuiteId {
        ProofSuiteId::new(system.as_u16(), 0x0001, 0x0001, 0x0001)
    }

    fn make_entry(suite_id: ProofSuiteId) -> VerifierEntry {
        let system = ProofSystemId::from_u16(suite_id.proof_system).unwrap();
        VerifierEntry {
            suite_id,
            proof_suite: ProofSuite::new(
                system,
                ProofCircuitModel::AIR,
                ProofExecutionClass::ClassA,
            ),
            verification_key: vec![0xAA; 64],
            registered_at: 100,
            expires_at: None,
        }
    }

    fn make_expiring_entry(suite_id: ProofSuiteId, expires: u64) -> VerifierEntry {
        let system = ProofSystemId::from_u16(suite_id.proof_system).unwrap();
        VerifierEntry {
            suite_id,
            proof_suite: ProofSuite::new(
                system,
                ProofCircuitModel::R1CS,
                ProofExecutionClass::ClassB,
            ),
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
        let sid = make_suite_id(ProofSystemId::STWO);
        let mut reg = VerifierRegistry::new();
        reg.register(make_entry(sid));
        assert_eq!(reg.len(), 1);
        assert!(reg.contains(&sid));
        let entry = reg.get(&sid).unwrap();
        assert_eq!(entry.suite_id, sid);
    }

    #[test]
    fn test_registry_get_missing() {
        let sid = make_suite_id(ProofSystemId::Cairo);
        let reg = VerifierRegistry::new();
        assert!(reg.get(&sid).is_none());
    }

    #[test]
    fn test_registry_overwrite() {
        let sid = make_suite_id(ProofSystemId::STWO);
        let mut reg = VerifierRegistry::new();
        reg.register(make_entry(sid));
        reg.register(make_entry(sid)); // overwrite
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn test_registry_evict_expired() {
        let sid_stwo = make_suite_id(ProofSystemId::STWO);
        let sid_plonk = make_suite_id(ProofSystemId::PLONK);
        let sid_halo2 = make_suite_id(ProofSystemId::Halo2);
        let mut reg = VerifierRegistry::new();
        reg.register(make_entry(sid_stwo)); // no expiry
        reg.register(make_expiring_entry(sid_plonk, 200));
        reg.register(make_expiring_entry(sid_halo2, 300));
        let evicted = reg.evict_expired(250);
        assert_eq!(evicted, 1); // PLONK evicted, Halo2 and STWO remain
        assert!(reg.contains(&sid_stwo));
        assert!(!reg.contains(&sid_plonk));
        assert!(reg.contains(&sid_halo2));
    }

    #[test]
    fn test_registry_deterministic_iteration() {
        let sid_cairo = make_suite_id(ProofSystemId::Cairo);
        let sid_stwo = make_suite_id(ProofSystemId::STWO);
        let sid_plonk = make_suite_id(ProofSystemId::PLONK);
        let mut reg = VerifierRegistry::new();
        reg.register(make_entry(sid_cairo));
        reg.register(make_entry(sid_stwo));
        reg.register(make_entry(sid_plonk));
        // BTreeMap sorts by key (the [u8; 32] hash) — deterministic order
        let hashes: Vec<[u8; 32]> = reg.iter().map(|(k, _)| *k).collect();
        assert_eq!(hashes.len(), 3);
        // Verify sorted
        assert!(hashes.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn test_registry_default() {
        let reg = VerifierRegistry::default();
        assert!(reg.is_empty());
    }
}
