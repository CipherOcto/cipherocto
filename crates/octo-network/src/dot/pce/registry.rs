//! Verifier Registry — deterministic proof verifier lookup (RFC-0859 §5.3)

use crate::dot::pce::proof_type::ProofSystemId;
use std::collections::BTreeMap;

/// Entry in the verifier registry.
#[derive(Debug, Clone)]
pub struct VerifierEntry {
    /// The proof system this verifier supports
    pub proof_system: ProofSystemId,
    /// The verification key bytes
    pub verification_key: Vec<u8>,
    /// Epoch when the verifier was registered
    pub registered_at: u64,
    /// Optional expiration epoch (None = never expires)
    pub expires_at: Option<u64>,
}

/// Deterministic verifier registry using BTreeMap for canonical ordering.
///
/// Maps proof commitment → VerifierEntry. Uses BTreeMap (not HashMap)
/// for deterministic iteration order at consensus boundary.
#[derive(Debug, Clone)]
pub struct VerifierRegistry {
    /// proof_commitment → VerifierEntry
    entries: BTreeMap<[u8; 32], VerifierEntry>,
}

impl VerifierRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// Register a verifier. Returns true if this is a new entry.
    pub fn register(&mut self, proof_commitment: [u8; 32], entry: VerifierEntry) -> bool {
        let is_new = !self.entries.contains_key(&proof_commitment);
        self.entries.insert(proof_commitment, entry);
        is_new
    }

    /// Look up a verifier by proof commitment.
    pub fn lookup(&self, proof_commitment: &[u8; 32]) -> Option<&VerifierEntry> {
        self.entries.get(proof_commitment)
    }

    /// Remove a verifier. Returns true if it existed.
    pub fn remove(&mut self, proof_commitment: &[u8; 32]) -> bool {
        self.entries.remove(proof_commitment).is_some()
    }

    /// Evict expired entries. Returns the number removed.
    pub fn evict_expired(&mut self, current_epoch: u64) -> usize {
        let before = self.entries.len();
        self.entries.retain(|_, entry| match entry.expires_at {
            Some(expiry) => current_epoch < expiry,
            None => true, // never expires
        });
        before - self.entries.len()
    }

    /// Number of registered verifiers.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(system: ProofSystemId, epoch: u64) -> VerifierEntry {
        VerifierEntry {
            proof_system: system,
            verification_key: vec![0xAA; 32],
            registered_at: epoch,
            expires_at: None,
        }
    }

    fn make_expiring_entry(system: ProofSystemId, epoch: u64, expires: u64) -> VerifierEntry {
        VerifierEntry {
            proof_system: system,
            verification_key: vec![0xBB; 32],
            registered_at: epoch,
            expires_at: Some(expires),
        }
    }

    #[test]
    fn test_registry_register_and_lookup() {
        let mut reg = VerifierRegistry::new();
        let commitment = [1u8; 32];
        assert!(reg.register(commitment, make_entry(ProofSystemId::STWO, 100)));
        assert!(!reg.register(commitment, make_entry(ProofSystemId::STWO, 200))); // update
        assert_eq!(reg.len(), 1);

        let entry = reg.lookup(&commitment).unwrap();
        assert_eq!(entry.proof_system, ProofSystemId::STWO);
        assert_eq!(entry.registered_at, 200); // updated
    }

    #[test]
    fn test_registry_remove() {
        let mut reg = VerifierRegistry::new();
        let commitment = [2u8; 32];
        reg.register(commitment, make_entry(ProofSystemId::PLONK, 100));
        assert!(reg.remove(&commitment));
        assert!(!reg.remove(&commitment)); // already gone
        assert!(reg.is_empty());
    }

    #[test]
    fn test_registry_evict_expired() {
        let mut reg = VerifierRegistry::new();
        reg.register(
            [1u8; 32],
            make_expiring_entry(ProofSystemId::STWO, 100, 200),
        );
        reg.register(
            [2u8; 32],
            make_expiring_entry(ProofSystemId::PLONK, 100, 500),
        );
        reg.register(
            [3u8; 32],
            make_entry(ProofSystemId::Groth16, 100), // never expires
        );

        assert_eq!(reg.len(), 3);
        let evicted = reg.evict_expired(300);
        assert_eq!(evicted, 1); // only [1u8;32] expired
        assert_eq!(reg.len(), 2);
        assert!(reg.lookup(&[1u8; 32]).is_none());
        assert!(reg.lookup(&[2u8; 32]).is_some());
        assert!(reg.lookup(&[3u8; 32]).is_some());
    }

    #[test]
    fn test_registry_deterministic_ordering() {
        let mut reg = VerifierRegistry::new();
        reg.register([3u8; 32], make_entry(ProofSystemId::Cairo, 100));
        reg.register([1u8; 32], make_entry(ProofSystemId::STWO, 100));
        reg.register([2u8; 32], make_entry(ProofSystemId::PLONK, 100));

        // BTreeMap iterates in sorted key order
        let keys: Vec<[u8; 32]> = reg.entries.keys().copied().collect();
        assert_eq!(keys, vec![[1u8; 32], [2u8; 32], [3u8; 32]]);
    }

    #[test]
    fn test_registry_lookup_nonexistent() {
        let reg = VerifierRegistry::new();
        assert!(reg.lookup(&[0xFFu8; 32]).is_none());
    }
}
