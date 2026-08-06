//! CapabilityClass registry (RFC-0958 §NodeType Gating Rule, layer 2 of 3).
//!
//! Three-layer defense for Wholesale fail-closed (RFC-0958 §Adversary A3):
//! 1. `mint_with_zk` returns `NodeTypeCannotMintZKCap` for Wholesale
//! 2. `CapabilityClassRegistry` rejects `ZKBearing` registration for Wholesale
//! 3. CI lint forbids `mint_with_zk` calls in `NodeType::Wholesale` paths
//!
//! This module provides layer 2.

use std::collections::HashMap;

use crate::node::NodeType;

use super::zk_mint::CapabilityClass;

/// CapabilityClass registry entry: maps `node_did → CapabilityClass`.
#[derive(Debug, Clone)]
pub struct RegistryEntry {
    pub node_did: String,
    pub node_type: NodeType,
    pub capability_class: CapabilityClass,
}

/// CapabilityClass registry (layer 2 of Wholesale fail-closed defense).
#[derive(Debug, Default)]
pub struct CapabilityClassRegistry {
    entries: HashMap<String, RegistryEntry>,
}

impl CapabilityClassRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a (node_did, node_type) → CapabilityClass mapping.
    /// Returns Err if Wholesale node is registered with ZKBearing.
    pub fn register(
        &mut self,
        node_did: impl Into<String>,
        node_type: NodeType,
        capability_class: CapabilityClass,
    ) -> Result<(), RegistryError> {
        let node_did = node_did.into();
        // Wholesale nodes cannot register as ZKBearing (layer 2 defense).
        if matches!(node_type, NodeType::Wholesale)
            & matches!(capability_class, CapabilityClass::ZKBearing)
        {
            return Err(RegistryError::WholesaleCannotRegisterZK);
        }
        self.entries.insert(
            node_did.clone(),
            RegistryEntry {
                node_did,
                node_type,
                capability_class,
            },
        );
        Ok(())
    }

    /// Lookup the registered CapabilityClass for a node.
    #[must_use]
    pub fn lookup(&self, node_did: &str) -> Option<&RegistryEntry> {
        self.entries.get(node_did)
    }

    /// Lookup the registered NodeType for a node.
    #[must_use]
    pub fn node_type_of(&self, node_did: &str) -> Option<NodeType> {
        self.entries.get(node_did).map(|e| e.node_type)
    }

    /// Lookup the registered CapabilityClass for a node.
    #[must_use]
    pub fn capability_class_of(&self, node_did: &str) -> Option<CapabilityClass> {
        self.entries.get(node_did).map(|e| e.capability_class)
    }

    /// Returns true if a node is registered as Wholesale (defense layer 2 cross-check).
    #[must_use]
    pub fn is_wholesale(&self, node_did: &str) -> bool {
        matches!(self.node_type_of(node_did), Some(NodeType::Wholesale))
    }

    /// Number of registered entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Registry errors (RFC-0958 §NodeType Gating Rule layer 2).
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("NodeType::Wholesale cannot register CapabilityClass::ZKBearing (fail-closed)")]
    WholesaleCannotRegisterZK,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wholesale_v1_registration_succeeds() {
        let mut reg = CapabilityClassRegistry::new();
        reg.register(
            "did:octo:provider:openai",
            NodeType::Wholesale,
            CapabilityClass::V1,
        )
        .unwrap();
        assert!(reg.is_wholesale("did:octo:provider:openai"));
    }

    #[test]
    fn wholesale_zkbearing_registration_rejected() {
        let mut reg = CapabilityClassRegistry::new();
        let err = reg
            .register(
                "did:octo:provider:openai",
                NodeType::Wholesale,
                CapabilityClass::ZKBearing,
            )
            .unwrap_err();
        assert!(matches!(err, RegistryError::WholesaleCannotRegisterZK));
    }

    #[test]
    fn selfhost_zkbearing_registration_succeeds() {
        let mut reg = CapabilityClassRegistry::new();
        reg.register(
            "did:octo:selfhost:openai",
            NodeType::SelfHost,
            CapabilityClass::ZKBearing,
        )
        .unwrap();
        assert_eq!(
            reg.capability_class_of("did:octo:selfhost:openai"),
            Some(CapabilityClass::ZKBearing)
        );
    }

    #[test]
    fn hybrid_v1_registration_succeeds() {
        let mut reg = CapabilityClassRegistry::new();
        reg.register(
            "did:octo:hybrid:org1",
            NodeType::Hybrid,
            CapabilityClass::V1,
        )
        .unwrap();
        assert_eq!(
            reg.node_type_of("did:octo:hybrid:org1"),
            Some(NodeType::Hybrid)
        );
    }

    #[test]
    fn hybrid_zkbearing_registration_succeeds() {
        let mut reg = CapabilityClassRegistry::new();
        reg.register(
            "did:octo:hybrid:org1",
            NodeType::Hybrid,
            CapabilityClass::ZKBearing,
        )
        .unwrap();
        assert_eq!(
            reg.capability_class_of("did:octo:hybrid:org1"),
            Some(CapabilityClass::ZKBearing)
        );
    }

    #[test]
    fn lookup_unknown_returns_none() {
        let reg = CapabilityClassRegistry::new();
        assert!(reg
            .lookup(&octo_ident::test_helpers::sample_did(37))
            .is_none());
    }

    #[test]
    fn three_layer_defense_invariant() {
        // Layer 2 defense: even if mint_with_zk is bypassed (layer 1), the
        // registry rejects ZKBearing registration for Wholesale. This is the
        // second layer of RFC-0958 §Adversary A3 defense-in-depth.
        let mut reg = CapabilityClassRegistry::new();
        assert!(reg
            .register(
                octo_ident::test_helpers::sample_did(176),
                NodeType::Wholesale,
                CapabilityClass::ZKBearing
            )
            .is_err());
        // Wholesale nodes default to V1 class.
        reg.register(
            "did:octo:provider:openai",
            NodeType::Wholesale,
            CapabilityClass::V1,
        )
        .unwrap();
        assert_eq!(
            reg.capability_class_of("did:octo:provider:openai"),
            Some(CapabilityClass::V1)
        );
    }
}
