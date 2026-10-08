//! CapabilityClass registry (RFC-0958 §NodeType Gating Rule, layer 2 of 3).
//!
//! Three-layer defense for Wholesale fail-closed (RFC-0958 §Adversary A3):
//! 1. `mint_with_zk` returns `NodeTypeCannotMintZKCap` for Wholesale
//! 2. `CapabilityClassRegistry` rejects `ZKBearing` registration for Wholesale
//! 3. CI lint forbids `mint_with_zk` calls in `NodeType::Wholesale` paths
//!
//! This module provides layer 2.
//!
//! # STATUS: NO PRODUCTION CALL SITE (RFC-0973 §Phase 4, decided)
//!
//! **Layer 2 does not run in production.** As of this decision, every
//! instantiation of [`CapabilityClassRegistry`] is in a test — this module's own
//! `mod tests`, or `tests/zk_vectors.rs`. No production code path constructs one,
//! so the Wholesale registration rule below is **not enforced at runtime**.
//!
//! The Wholesale fail-closed property therefore rests on layers 1 and 3 only:
//! the `mint_with_zk` guard, which does run, and the CI lint. That is enough to
//! hold the property today, but it is worth being exact about which layer is
//! carrying it, because this type reads like an enforced gate and is not one.
//!
//! ## Why it is documented rather than wired
//!
//! RFC-0973 §Phase 4 owns the decision between promoting this to a real call
//! site and documenting the projection. **Documented, deliberately.** Two
//! reasons:
//!
//! 1. The gap is **pre-existing**. It is not introduced by RFC-0973 and fixing
//!    it is not this RFC's to do. Landing a call site here would be an
//!    unrelated behavioural change to a years-stable Layer B type, in a phase
//!    whose subject is attestation.
//!
//! 2. Wiring it correctly needs a decision that has not been made. A registry
//!    keyed by `node_did` is **stored** state — someone must own it, decide its
//!    lifetime, and decide who may write to it. RFC-0973's own role model treats
//!    the capability class as a **computed projection** rather than stored state.
//!    Until that ownership question is settled, wiring the registry would mean
//!    inventing an authority and storing derived data in it.
//!
//! ## What this means for a reader
//!
//! Do not treat the `WholesaleCannotRegisterZK` rule below as a runtime
//! guarantee. It is a validated invariant that production currently does not
//! exercise. If a future phase needs the rule enforced, it needs a call site
//! **and** the ownership decision above — not just the constructor.
//!
//! ## Interaction with mission `0973-v36-d-dependency-direction`
//!
//! `NodeType` is declared in `octo-cap-zk`, not in the macaroon crate, so the
//! wallet's `to_zk_node_type` projection has nothing to re-point at once the
//! wallet's dependency on `octo-cap-zk` is removed. That mission owns the
//! disposition. Do not read "retained in the wallet" as settled.

use std::collections::HashMap;

use crate::node::NodeType;

// `CapabilityClass` is declared in `octo-cap-macaroon` (Layer A) and
// re-exported by `octo-cap-zk` (Layer E). The wallet imports it from the Layer-A
// declaring crate, not through the Layer-E strategy crate — RFC-0973 §Phase 5
// removed the wallet's dependency on `octo-cap-zk`, and this import was
// re-pointed rather than left dangling.
use octo_cap_macaroon::CapabilityClass;

/// CapabilityClass registry entry: maps `node_did → CapabilityClass`.
#[derive(Debug, Clone)]
pub struct RegistryEntry {
    pub node_did: String,
    pub node_type: NodeType,
    pub capability_class: CapabilityClass,
}

/// CapabilityClass registry (layer 2 of Wholesale fail-closed defense).
///
/// **No production call site** — see the module documentation. This type is a
/// validated invariant that production does not currently exercise.
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
    ///
    /// **Not enforced at runtime** — see the module documentation.
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
