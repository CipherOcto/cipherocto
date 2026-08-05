//! Capability redemption: subgraph check vs. parent policy (RFC-0967 §5).
//!
//! `redeem_capability` enforces `capability ⊆ policy` — the capability's
//! effective surface (models / providers / per-axis caps / total spend)
//! must be a subgraph of the policy it claims to authorize. Without this
//! check, a holder with a `PolicyReference` caveat could mint a capability
//! that violates the parent policy.
//!
//! ## Catalog ownership
//!
//! The catalog is owned by the **caller** (envelope / network verifier),
//! not by `octo-wallet`. `redeem_capability` takes `&&dyn PolicyCatalog`
//! as a parameter — the same pattern as `Macaroon::attenuate(_, &&dyn
//! CapabilityCatalog)`. This keeps `octo-wallet` storage-agnostic and
//! lets the verifier plug in a SQLite-backed, in-memory, or network-backed
//! catalog without recompilation.
//!
//! ## Task 1.3 wiring (DEFERRED — see `docs/plans/2026-07-24-seven-gap-impl.md`)
//!
//! The canonical entry point for capability redemption is
//! `CapabilityToken::redeem(&&dyn PolicyCatalog)`. **However, no
//! production code in `crates/octo-network` calls `redeem_capability` or
//! `CapabilityToken::redeem` today.** The only `verify_capability` in
//! `octo-network` (`mon/nostr_bootstrap.rs:161`) is a Nostr-specific
//! `DotCapabilityClaim` checker (different type, different namespace).
//!
//! TODO: wire `CapabilityToken::redeem` into the envelope / network
//! verifier when a `verify_capability(cap: &&CapabilityToken, ...) -> Result<...>`
//! hook lands in `crates/octo-network/src/dot/pce/` (likely alongside
//! Gap 2 — MultiEnvelope nesting — per RFC-0962 §7 R8-F5). Until then,
//! callers must invoke `redeem_capability` directly from any new envelope
//! verifier path.

#[cfg(test)]
use std::collections::HashMap;
use std::collections::HashSet;

use cipherocto_policy::{is_subgraph, PolicyObject, PolicySurface};
use thiserror::Error;

use super::caveat::Caveat;
use super::CapabilityToken;

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
/// `#[cfg(test)]`-gated for symmetry with `InMemoryCatalog` — integration
/// tests should inline a `PolicyCatalog` stub.
#[cfg(test)]
#[derive(Debug, Default, Clone)]
pub struct InMemoryPolicyCatalog {
    pub(crate) by_id: HashMap<[u8; 32], PolicyObject>,
}

#[cfg(test)]
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

#[cfg(test)]
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

/// Redeem a capability against a policy catalog (RFC-0967 §5).
///
/// Steps:
/// 1. Require a `PolicyReference` caveat on the capability.
/// 2. Look up the referenced policy in the catalog.
/// 3. Derive a `PolicySurface` from the capability's caveats.
/// 4. Reject iff `!is_subgraph(&&cap_surface, &&policy.surface)`.
///
/// # Errors
/// - `RedemptionError::MissingPolicyReference` if no `PolicyReference`
///   caveat is present.
/// - `RedemptionError::PolicyNotFound { policy_id }` if the catalog
///   has no entry for the referenced policy.
/// - `RedemptionError::PolicyNotSuperseded { cap_id, policy_id }` if
///   the capability's surface is not contained in the policy's surface.
pub fn redeem_capability(
    cap: &CapabilityToken,
    catalog: &dyn PolicyCatalog,
) -> Result<(), RedemptionError> {
    let policy_ref = cap
        .macaroon
        .caveats
        .iter()
        .find_map(|c| match c {
            Caveat::PolicyReference { policy_id, .. } => Some(*policy_id),
            _ => None,
        })
        .ok_or(RedemptionError::MissingPolicyReference)?;

    let policy = catalog
        .get(&policy_ref)
        .ok_or(RedemptionError::PolicyNotFound {
            policy_id: policy_ref,
        })?;

    if !is_subgraph(&cap.to_policy_object(), policy) {
        return Err(RedemptionError::PolicyNotSuperseded {
            cap_id: cap.macaroon.id,
            policy_id: policy_ref,
        });
    }

    Ok(())
}

/// Derive a `PolicySurface` from a capability's caveats (RFC-0967 §5).
///
/// Maps relevant caveats to the surface fields used by `is_subgraph`:
/// - `AmountMax` → `max_total_spend` (min across all `AmountMax` caveats)
/// - `Model` → `allowed_models` (set of allowed models)
/// - `Provider` → `allowed_providers` (set of allowed providers)
/// - `PerAxisMax` → `per_axis_caps` (one entry per axis; each entry is
///   checked independently by `is_subgraph` — conflicting entries on the
///   same axis will be rejected by the subgraph check; attenuation that
///   multi-caps an axis must narrow via `set_subsumes` at mint time)
///
/// Caveats that don't map to a surface field are ignored at this
/// layer (they remain enforced by the per-caveat subsumption checks
/// in `octo-wallet::capability::caveat::set_subsumes`).
#[must_use]
pub(crate) fn capability_to_surface(cap: &CapabilityToken) -> PolicySurface {
    let mut allowed_models: Option<HashSet<String>> = None;
    let mut allowed_providers: Option<HashSet<String>> = None;
    let mut max_total_spend: Option<u128> = None;
    let mut per_axis_caps: Vec<(String, u128)> = Vec::new();

    for caveat in &cap.macaroon.caveats {
        match caveat {
            Caveat::AmountMax(amount) => {
                max_total_spend = Some(max_total_spend.map_or(*amount, |m| m.min(*amount)));
            }
            Caveat::Model(m) => {
                allowed_models
                    .get_or_insert_with(HashSet::new)
                    .insert(m.clone());
            }
            Caveat::Provider(p) => {
                allowed_providers
                    .get_or_insert_with(HashSet::new)
                    .extend(p.iter().cloned());
            }
            Caveat::PerAxisMax(p) => {
                per_axis_caps.push((p.axis.clone(), p.max_per_1k));
            }
            _ => {}
        }
    }

    PolicySurface {
        allowed_models,
        allowed_providers,
        per_axis_caps,
        max_total_spend,
        audit_window_secs: 0,
        allowed_destinations: None,
    }
}

impl CapabilityToken {
    /// Mint a `PolicyObject` snapshot from the capability's caveats.
    ///
    /// Used by `redeem_capability` to perform the subgraph check. The
    /// returned `PolicyObject` is a transient surface derivation — it
    /// does not carry a real signature or audit reference and is not
    /// intended for storage. The `audit_ref` is all zero and the
    /// timestamp is zero so the result is deterministic.
    #[must_use]
    pub fn to_policy_object(&self) -> PolicyObject {
        let surface = capability_to_surface(self);
        PolicyObject::mint_surface(surface, [0u8; 32], 0)
    }

    /// Full redemption: verify holder signature + subgraph check (RFC-0967 §5).
    ///
    /// This is the canonical entry point for envelope-side verification.
    /// It runs:
    /// 1. `verify_holder_sig` — Ed25519 over `canonical_ser(root_id || caveats)`.
    /// 2. `redeem_capability` — subgraph check against the policy catalog.
    ///
    /// # Errors
    /// - `RedemptionError::HolderSig(msg)` if the holder signature fails.
    /// - `RedemptionError::PolicyNotSuperseded / MissingPolicyReference /
    ///   PolicyNotFound` from the subgraph check (see [`redeem_capability`]).
    pub fn redeem(&self, catalog: &dyn PolicyCatalog) -> Result<(), RedemptionError> {
        self.verify_holder_sig()
            .map_err(|e| RedemptionError::HolderSig(e.to_string()))?;
        redeem_capability(self, catalog)
    }
}
