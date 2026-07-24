//! Capability redemption: subgraph check vs. parent policy (RFC-0967 §5).
//!
//! `redeem_capability` enforces `capability ⊆ policy` — the capability's
//! effective surface (models / providers / per-axis caps / total spend)
//! must be a subgraph of the policy it claims to authorize. Without this
//! check, a holder with a `PolicyReference` caveat could mint a capability
//! that violates the parent policy.

use std::collections::HashMap;

use cipherocto_policy::PolicyObject;
use thiserror::Error;

/// Capability redemption against a policy catalog (RFC-0967 §5).
///
/// Implementations look up the policy by `policy_id` and return `None` if
/// absent. The wallet does NOT contact the network or DB itself; the caller
/// decides where the catalog is backed.
pub trait PolicyCatalog {
    /// Fetch a `PolicyObject` by its content-addressed `policy_id`.
    fn get(&self, policy_id: &[u8; 32]) -> Option<&PolicyObject>;
}

/// Test-only in-memory `PolicyCatalog`. Mirrors the `InMemoryCatalog`
/// pattern used by [`crate::capability::macaroon::CapabilityCatalog`].
#[derive(Debug, Default, Clone)]
pub struct InMemoryPolicyCatalog {
    pub(crate) by_id: HashMap<[u8; 32], PolicyObject>,
}

impl InMemoryPolicyCatalog {
    /// Construct an empty catalog.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a policy under its `policy_id`.
    pub fn insert(&mut self, policy: PolicyObject) {
        let id = policy.policy_id;
        self.by_id.insert(id, policy);
    }
}

impl PolicyCatalog for InMemoryPolicyCatalog {
    fn get(&self, policy_id: &[u8; 32]) -> Option<&PolicyObject> {
        self.by_id.get(policy_id)
    }
}

/// Redemption errors (RFC-0967 §5).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RedemptionError {
    /// Holder signature verification failed (Ed25519 over
    /// `canonical_ser(root_id || caveats)`).
    #[error("holder signature error: {0}")]
    HolderSig(String),

    /// Capability's surface is not contained in the policy's surface
    /// (e.g. amount cap exceeds policy, model not in policy allowed set).
    #[error("capability {cap_id:?} not a subgraph of policy {policy_id:?}")]
    PolicyNotSuperseded {
        cap_id: [u8; 32],
        policy_id: [u8; 32],
    },

    /// Capability has no `PolicyReference` caveat — redemption requires
    /// a parent policy to check against.
    #[error("missing PolicyReference caveat on capability")]
    MissingPolicyReference,

    /// Catalog has no entry for the policy id referenced by the capability.
    #[error("policy {policy_id:?} not found in catalog")]
    PolicyNotFound { policy_id: [u8; 32] },
}
