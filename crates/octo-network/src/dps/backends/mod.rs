//! Proof Backend Implementations (RFC-0854 §3, Phase 2)
//!
//! Concrete implementations of the `DeterministicProofSystem` trait
//! for STARK (STWO) and PLONK proof backends.

pub mod plonk;
pub mod stark;

use std::collections::BTreeMap;

use super::suite::{ProofCircuitModel, ProofSuiteId, ProofSystemId};
use super::DpsError;

/// Backend entry in the registry.
#[derive(Debug, Clone)]
pub struct BackendEntry {
    /// Proof system identifier
    pub system_id: ProofSystemId,
    /// Circuit model
    pub circuit_model: ProofCircuitModel,
    /// Display name
    pub name: &'static str,
    /// Properties description
    pub properties: &'static str,
    /// Typical verification time in microseconds
    pub typical_verify_us: u64,
    /// Typical proof size in bytes
    pub typical_proof_size: usize,
}

/// Backend registry — maps ProofSystemId to backend metadata.
///
/// Uses BTreeMap for deterministic iteration order (Class A requirement).
#[derive(Debug, Clone)]
pub struct BackendRegistry {
    entries: BTreeMap<u16, BackendEntry>,
}

impl BackendRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// Create a registry with all standard backends registered.
    pub fn with_defaults() -> Self {
        let mut reg = Self::new();
        reg.register(stark::stark_backend_entry());
        reg.register(plonk::plonk_backend_entry());
        reg
    }

    /// Register a backend.
    pub fn register(&mut self, entry: BackendEntry) {
        self.entries.insert(entry.system_id as u16, entry);
    }

    /// Look up a backend by system ID.
    pub fn lookup(&self, system_id: u16) -> Option<&BackendEntry> {
        self.entries.get(&system_id)
    }

    /// Check if a backend is registered.
    pub fn is_registered(&self, system_id: u16) -> bool {
        self.entries.contains_key(&system_id)
    }

    /// Number of registered backends.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get all registered system IDs.
    pub fn registered_systems(&self) -> Vec<u16> {
        self.entries.keys().copied().collect()
    }
}

impl Default for BackendRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// Select the best backend for a given proof suite.
///
/// Returns the ProofSystemId of the matching backend, or an error
/// if no backend is registered for the given suite.
pub fn select_backend(
    registry: &BackendRegistry,
    suite: &ProofSuiteId,
) -> Result<ProofSystemId, DpsError> {
    ProofSystemId::from_u16(suite.proof_system).ok_or(DpsError::UnsupportedBackend {
        system_id: suite.proof_system,
    })?;
    if registry.is_registered(suite.proof_system) {
        ProofSystemId::from_u16(suite.proof_system).ok_or(DpsError::UnsupportedBackend {
            system_id: suite.proof_system,
        })
    } else {
        Err(DpsError::UnsupportedBackend {
            system_id: suite.proof_system,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_registry_defaults() {
        let reg = BackendRegistry::with_defaults();
        assert!(reg.is_registered(ProofSystemId::STWO as u16));
        assert!(reg.is_registered(ProofSystemId::PLONK as u16));
        assert_eq!(reg.len(), 2);
    }

    #[test]
    fn test_backend_registry_lookup() {
        let reg = BackendRegistry::with_defaults();
        let stark = reg.lookup(ProofSystemId::STWO as u16).unwrap();
        assert_eq!(stark.name, "STARK (STWO)");
        assert_eq!(stark.circuit_model, ProofCircuitModel::AIR);
    }

    #[test]
    fn test_backend_registry_not_found() {
        let reg = BackendRegistry::with_defaults();
        assert!(reg.lookup(ProofSystemId::Cairo as u16).is_none());
    }

    #[test]
    fn test_select_backend_stwo() {
        let reg = BackendRegistry::with_defaults();
        let suite = ProofSuiteId::new(ProofSystemId::STWO as u16, 0x0001, 0x0001, 0x0001);
        let backend = select_backend(&reg, &suite).unwrap();
        assert_eq!(backend, ProofSystemId::STWO);
    }

    #[test]
    fn test_select_backend_unsupported() {
        let reg = BackendRegistry::with_defaults();
        let suite = ProofSuiteId::new(ProofSystemId::Cairo as u16, 0x0001, 0x0001, 0x0001);
        assert!(select_backend(&reg, &suite).is_err());
    }

    #[test]
    fn test_backend_registry_deterministic_order() {
        let reg = BackendRegistry::with_defaults();
        let systems = reg.registered_systems();
        // BTreeMap ensures ascending order
        assert!(systems.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn test_backend_entry_properties() {
        let reg = BackendRegistry::with_defaults();
        let stark = reg.lookup(ProofSystemId::STWO as u16).unwrap();
        assert!(stark.properties.contains("transparent"));
        assert!(stark.typical_verify_us > 0);

        let plonk = reg.lookup(ProofSystemId::PLONK as u16).unwrap();
        assert!(plonk.properties.contains("succinct"));
    }
}
