//! Cross-platform admin attestation (mission 0855p-c-admin-attestation).
//!
//! Each DomainCoordinator periodically publishes a
//! `PlatformAdminAttest` envelope on the libp2p mesh under
//! `/dot/admin/{domain_id}/{platform}` containing a fresh proof
//! of admin status. Other DomainCoordinators verify and challenge
//! invalid attestations.
//!
//! ## Freshness
//!
//! - `MAX_ATTEST_AGE_EPOCHS = 100` (~100 minutes at 1-min epochs)
//! - `ATTEST_PERIOD_EPOCHS = 50` (publish every 50 minutes)
//! - `CHALLENGE_RESPONSE_EPOCHS = 10` (DC must respond within 10 epochs)
//!
//! ## Domain Coordinator binding (RFC-0855p-c + RFC-0011-d v1.7.1 §7.4)
//!
//! [`PlatformAdminProof`] is the typed envelope consumed by
//! [`bind_domain_coordinator`] when an operator binds the
//! `domain-coordinator` role to a transport group binding
//! (RFC-0850p-c §4). The proof carries freshness (`MAX_ATTEST_AGE_EPOCHS`),
//! DC pubkey, adapter signature, nonce, and signed_at_epoch — replacing
//! the v1.7 over-broad `platform_admin_id: &str` argument with a typed
//! proof envelope. Adapter signature verification is delegated to the
//! adapter at the substrate boundary per [[cipherocto-design-principles]]
//! §Push complexity to edges.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::dot::binding::{BindingError, GroupBinding, GroupState};

/// Maximum age of an attestation (per mission spec).
pub const MAX_ATTEST_AGE_EPOCHS: u64 = 100;
/// Period between attestations.
pub const ATTEST_PERIOD_EPOCHS: u64 = 50;
/// Time for a DC to respond to a challenge.
pub const CHALLENGE_RESPONSE_EPOCHS: u64 = 10;

/// The platform identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Platform {
    WhatsApp,
    Telegram,
    Matrix,
    Slack,
    Discord,
    Nostr,
    Custom,
}

impl Platform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Platform::WhatsApp => "whatsapp",
            Platform::Telegram => "telegram",
            Platform::Matrix => "matrix",
            Platform::Slack => "slack",
            Platform::Discord => "discord",
            Platform::Nostr => "nostr",
            Platform::Custom => "custom",
        }
    }
}

/// A `PlatformAdminAttest` envelope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformAdminAttest {
    pub domain_id: String,
    pub platform: Platform,
    pub platform_group_id: String,
    pub dc_pubkey: Vec<u8>,
    /// A signed response from the platform API.
    pub proof: Vec<u8>,
    pub signed_at_epoch: u64,
}

impl PlatformAdminAttest {
    /// Returns true if the attest is fresh (within MAX_ATTEST_AGE_EPOCHS).
    pub fn is_fresh(&self, current_epoch: u64) -> bool {
        current_epoch.saturating_sub(self.signed_at_epoch) <= MAX_ATTEST_AGE_EPOCHS
    }

    /// Returns the age of the attest in epochs.
    pub fn age_epochs(&self, current_epoch: u64) -> u64 {
        current_epoch.saturating_sub(self.signed_at_epoch)
    }

    /// Returns true if a new attest should be published
    /// (older than ATTEST_PERIOD_EPOCHS).
    pub fn needs_renewal(&self, current_epoch: u64) -> bool {
        self.age_epochs(current_epoch) >= ATTEST_PERIOD_EPOCHS
    }
}

/// A challenge against a DC's attest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttestChallenge {
    pub domain_id: String,
    pub dc_pubkey: Vec<u8>,
    pub reason: String,
    pub evidence: Vec<u8>,
    pub issued_at_epoch: u64,
    /// The deadline by which the DC must respond.
    pub response_deadline_epoch: u64,
}

impl AttestChallenge {
    /// Create a new challenge with the standard response deadline.
    pub fn new(
        domain_id: impl Into<String>,
        dc_pubkey: Vec<u8>,
        reason: impl Into<String>,
        evidence: Vec<u8>,
        issued_at_epoch: u64,
    ) -> Self {
        Self {
            domain_id: domain_id.into(),
            dc_pubkey,
            reason: reason.into(),
            evidence,
            issued_at_epoch,
            response_deadline_epoch: issued_at_epoch + CHALLENGE_RESPONSE_EPOCHS,
        }
    }

    /// Returns true if the deadline has elapsed (DC failed to
    /// respond in time).
    pub fn is_expired(&self, current_epoch: u64) -> bool {
        current_epoch > self.response_deadline_epoch
    }
}

/// Errors from attest verification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlatformAdminAttestError {
    /// The attest is too old.
    Stale { age_epochs: u64, max: u64 },
    /// The platform API proof is invalid.
    InvalidProof,
    /// The DC pubkey in the attest does not match the expected DC.
    WrongDc,
}

/// Verify an attest is fresh and matches the expected DC.
pub fn verify_attest(
    attest: &PlatformAdminAttest,
    expected_dc_pubkey: &[u8],
    current_epoch: u64,
) -> Result<(), PlatformAdminAttestError> {
    let age = attest.age_epochs(current_epoch);
    if age > MAX_ATTEST_AGE_EPOCHS {
        return Err(PlatformAdminAttestError::Stale {
            age_epochs: age,
            max: MAX_ATTEST_AGE_EPOCHS,
        });
    }
    if attest.dc_pubkey != expected_dc_pubkey {
        return Err(PlatformAdminAttestError::WrongDc);
    }
    // Real proof verification is per-platform (WhatsApp, Telegram,
    // Matrix each have different admin verification APIs). This
    // module provides the freshness + DC-pubkey check; the
    // platform-specific proof check is delegated to the platform
    // adapter (out of scope for this mission).
    Ok(())
}

/// Derive the libp2p gossip topic for a DC's attest.
pub fn attest_topic(domain_id: &str, platform: Platform) -> String {
    assert!(!domain_id.is_empty(), "domain_id must not be empty");
    format!("/dot/admin/{}/{}", domain_id, platform.as_str())
}

/// Current Unix epoch in seconds (for diagnostics / operator
/// visibility).
///
/// WARNING: this is the Unix time in seconds, not a network
/// consensus epoch. Use a consensus-epoch clock (e.g.,
/// derived from a VDF or governance rotation) for any
/// value stored in a `signed_at_epoch` field that participates
/// in freshness checks like `MAX_ATTEST_AGE_EPOCHS`.
pub fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Deprecated alias for [`now_unix_seconds`]. Returns Unix
/// seconds (not a network epoch).
#[deprecated(note = "renamed to now_unix_seconds for clarity")]
pub fn now_epoch() -> u64 {
    now_unix_seconds()
}

// ---------------------------------------------------------------------------
// Domain Coordinator binding — RFC-0855p-c + RFC-0011-d v1.7.1 §7.4
// ---------------------------------------------------------------------------

/// Typed envelope consumed by [`bind_domain_coordinator`] when an operator
/// binds the `domain-coordinator` role to a transport group binding
/// (RFC-0850p-c §4). Replaces the v1.7 over-broad `platform_admin_id: &str`
/// argument with a structured proof carrying freshness, DC pubkey, adapter
/// signature, nonce, and signed-at-epoch.
///
/// **Adapter signature:** the adapter is the platform-trust-root and signs
/// the proof payload. Signature verification is delegated to the adapter at
/// the substrate boundary per [[cipherocto-design-principles]] §Push
/// complexity to edges; the substrate stores the bytes but does not perform
/// per-platform cryptographic verification (WhatsApp / Matrix / Telegram have
/// different admin verification APIs).
///
/// **`#[non_exhaustive]`:** allows future fields (e.g., post-quantum
/// adapter signatures per RFC-0104) without breaking semver.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub struct PlatformAdminProof {
    /// Platform identifier (canonical RFC-0855p-c enum).
    pub platform: Platform,
    /// Canonical `participant_id` per RFC-0850p-c §Appendix A
    /// (WhatsApp participant_id / Matrix user_id / Telegram user_id).
    pub platform_admin_id: String,
    /// Operator's Ed25519 public key (32 bytes). MUST equal the pubkey
    /// derived from the canonical `did:octo:0x<hex>` operator DID via
    /// `octo_cap_macaroon::signer::pubkey_from_did`.
    pub dc_pubkey: Vec<u8>,
    /// Adapter signature over the proof payload. Adapter is
    /// platform-trust-root; verification delegated to adapter.
    /// `Vec<u8>` (not `[u8; 64]`) to match the existing
    /// `PlatformAdminAttest` byte-array convention in this crate.
    pub adapter_signature: Vec<u8>,
    /// 32-byte random nonce (replay protection). `Vec<u8>` to match
    /// the existing `PlatformAdminAttest` byte-array convention.
    pub nonce: Vec<u8>,
    /// Network consensus epoch at proof emission (NOT Unix epoch —
    /// matches `MAX_ATTEST_AGE_EPOCHS` semantics on `PlatformAdminAttest`).
    pub signed_at_epoch: u64,
}

impl PlatformAdminProof {
    /// Construct a new `PlatformAdminProof`. Canonical constructor for
    /// external crate consumers — required because `PlatformAdminProof`
    /// is `#[non_exhaustive]` (drift-fix C: future-proofs against
    /// additive fields without breaking semver).
    #[must_use]
    pub fn new(
        platform: Platform,
        platform_admin_id: String,
        dc_pubkey: Vec<u8>,
        adapter_signature: Vec<u8>,
        nonce: Vec<u8>,
        signed_at_epoch: u64,
    ) -> Self {
        Self {
            platform,
            platform_admin_id,
            dc_pubkey,
            adapter_signature,
            nonce,
            signed_at_epoch,
        }
    }

    /// Returns true if the proof is fresh (within `MAX_ATTEST_AGE_EPOCHS`).
    pub fn is_fresh(&self, current_epoch: u64) -> bool {
        current_epoch.saturating_sub(self.signed_at_epoch) <= MAX_ATTEST_AGE_EPOCHS
    }

    /// Returns the age of the proof in epochs.
    pub fn age_epochs(&self, current_epoch: u64) -> u64 {
        current_epoch.saturating_sub(self.signed_at_epoch)
    }
}

/// Verify a [`PlatformAdminProof`] is fresh and its DC pubkey matches
/// the expected operator pubkey.
///
/// **Phase 1 stub:** the adapter signature field is recorded but NOT
/// cryptographically verified here — production adapters verify at the
/// substrate boundary per drift-fix B (per-adapter local verification;
/// different admin verification APIs per platform). Returns
/// `PlatformAdminAttestError::InvalidProof` if the adapter signature is
/// empty (catches the "no signature" failure mode); a non-empty signature
/// is accepted by this substrate helper.
pub fn verify_platform_admin_proof(
    proof: &PlatformAdminProof,
    expected_dc_pubkey: &[u8; 32],
    current_epoch: u64,
) -> Result<(), PlatformAdminAttestError> {
    // 1. Freshness check (canonical MAX_ATTEST_AGE_EPOCHS).
    let age = proof.age_epochs(current_epoch);
    if age > MAX_ATTEST_AGE_EPOCHS {
        return Err(PlatformAdminAttestError::Stale {
            age_epochs: age,
            max: MAX_ATTEST_AGE_EPOCHS,
        });
    }
    // 2. DC pubkey match (binds proof to role_binding).
    if proof.dc_pubkey.as_slice() != expected_dc_pubkey {
        return Err(PlatformAdminAttestError::WrongDc);
    }
    // 3. Adapter signature presence check (delegated verification).
    if proof.adapter_signature.is_empty() {
        return Err(PlatformAdminAttestError::InvalidProof);
    }
    Ok(())
}

/// Bind an operator's `domain-coordinator` role to a transport group
/// binding (RFC-0850p-c §4). Atomically verifies the platform admin
/// proof + updates the [`GroupBinding`] (idempotent if already Bound)
/// + emits RFC-0855p-c §5a `PlatformEvent::AdminTransfer` envelope
///   post-commit (Layer D side-effect; non-transactional).
///
/// **Atomicity:** returns updated [`GroupBinding`] on success; on any
/// verification failure returns canonical [`BindingError`] (NOT fictional
/// `DomainCoordinatorError`; Layer C error at
/// `crates/octo-network/src/dot/binding.rs:648`). Phase 2 production
/// wires this into a single Stoolap `BEGIN IMMEDIATE` transaction covering
/// BOTH the role binding (from `octo_role::select`) AND the binding
/// ceremony update — see M10 `select_domain_coordinator`.
///
/// **Substrate home:** `crates/octo-network/src/dc/admin_attest.rs`
/// (REAL; co-located with existing `PlatformAdminAttestError` per
/// RFC-0855p-c). Drift-fix v1.7.1 re-located from non-existent
/// `octo_network::mon::domain_coordinator` per RFC-0855p-c drift-fix
/// B/C (commit `205f1434`).
pub fn bind_domain_coordinator(
    operator_did: &str,
    group_binding: &GroupBinding,
    platform_admin_proof: &PlatformAdminProof,
    current_epoch: u64,
) -> Result<GroupBinding, BindingError> {
    // 1. Verify platform admin proof (freshness + DC pubkey + adapter sig presence).
    //    The DC pubkey in the proof MUST equal the operator's pubkey derived
    //    from the canonical `did:octo:0x<hex>` form (Phase 1 `did_from_pubkey`).
    let operator_pubkey =
        octo_cap_macaroon::signer::pubkey_from_did(operator_did).ok_or_else(|| {
            BindingError::SignatureInvalid {
                reason: format!(
                    "operator_did {operator_did} is not a canonical did:octo:0x<hex> form"
                ),
            }
        })?;
    verify_platform_admin_proof(platform_admin_proof, &operator_pubkey, current_epoch).map_err(
        |e| match e {
            PlatformAdminAttestError::Stale { age_epochs, max } => BindingError::SignatureInvalid {
                reason: format!("PlatformAdminProof stale: age {age_epochs} epochs > max {max}"),
            },
            PlatformAdminAttestError::WrongDc => BindingError::SignatureInvalid {
                reason: "PlatformAdminProof dc_pubkey does not match operator pubkey".into(),
            },
            PlatformAdminAttestError::InvalidProof => BindingError::SignatureInvalid {
                reason: "PlatformAdminProof adapter_signature missing".into(),
            },
        },
    )?;

    // 2. Verify group_binding.state == Bound (no transition from
    //    Unbound / Quarantined / ReBinding).
    if group_binding.state != GroupState::Bound {
        return Err(BindingError::InvalidTransition {
            from: group_binding.state,
            to: GroupState::Bound,
        });
    }

    // 3. Atomic update (idempotent if already Bound by operator):
    //    - domain_coordinator_id ← operator pubkey (the new DC)
    //    - renewed_at_epoch ← current_epoch
    //    - binding_hash ← BLAKE3-256 of canonical serialization
    let mut new_binding = group_binding.clone();
    new_binding.domain_coordinator_id = operator_pubkey;
    new_binding.renewed_at_epoch = current_epoch;
    new_binding.binding_hash = compute_binding_hash(&new_binding);

    // 4. Post-commit emit PlatformEvent::AdminTransfer envelope
    //    (Layer D side-effect; non-transactional; fire-and-forget).
    //    In Phase 1, logged via tracing; production wires this to the
    //    adapter-bound `PlatformEvent` channel per RFC-0855p-c §5a.
    tracing::info!(
        target: "octo_network::dc::admin_attest",
        platform = %platform_admin_proof.platform.as_str(),
        platform_admin_id = %platform_admin_proof.platform_admin_id,
        domain_id = %hex::encode(group_binding.domain_id),
        group_jid = %group_binding.group_jid,
        "PlatformEvent::AdminTransfer emitted (RFC-0855p-c §5a)"
    );

    Ok(new_binding)
}

/// Compute `binding_hash = BLAKE3-256(canonical_serialization)`.
///
/// Canonical serialization: BLAKE3-256 of the concatenation of the
/// `GroupBinding` fields in declaration order, each length-prefixed by
/// a big-endian `u32` for variable-length fields, or emitted verbatim
/// for fixed-size byte arrays. This matches the existing
/// `BindEnvelope::body_bytes` convention (RFC-0850p-c §Canonical body
/// serialization) so the substrate stays consistent across the
/// binding family.
fn compute_binding_hash(binding: &GroupBinding) -> [u8; 32] {
    let mut buf = Vec::with_capacity(256);
    // group_jid (string, length-prefixed u32 BE)
    buf.extend_from_slice(&(binding.group_jid.len() as u32).to_be_bytes());
    buf.extend_from_slice(binding.group_jid.as_bytes());
    // platform (string, length-prefixed u32 BE)
    buf.extend_from_slice(&(binding.platform.len() as u32).to_be_bytes());
    buf.extend_from_slice(binding.platform.as_bytes());
    // mission_id (32 bytes, verbatim)
    buf.extend_from_slice(&binding.mission_id);
    // domain_id (32 bytes, verbatim)
    buf.extend_from_slice(&binding.domain_id);
    // domain_coordinator_id (32 bytes, verbatim)
    buf.extend_from_slice(&binding.domain_coordinator_id);
    // bound_at_epoch (u64 BE)
    buf.extend_from_slice(&binding.bound_at_epoch.to_be_bytes());
    // renewed_at_epoch (u64 BE)
    buf.extend_from_slice(&binding.renewed_at_epoch.to_be_bytes());
    // state (u8)
    buf.push(binding.state.as_byte());
    *blake3::hash(&buf).as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_attest(epoch: u64) -> PlatformAdminAttest {
        PlatformAdminAttest {
            domain_id: "d1".into(),
            platform: Platform::WhatsApp,
            platform_group_id: "g1".into(),
            dc_pubkey: vec![0xAA],
            proof: vec![0xBB],
            signed_at_epoch: epoch,
        }
    }

    #[test]
    fn fresh_attest_passes() {
        let a = fresh_attest(100);
        assert!(a.is_fresh(150));
    }

    #[test]
    fn stale_attest_rejected() {
        let a = fresh_attest(100);
        assert!(!a.is_fresh(201)); // 101 epochs > 100
    }

    #[test]
    fn needs_renewal_after_period() {
        let a = fresh_attest(100);
        assert!(!a.needs_renewal(140)); // 40 epochs < 50
        assert!(a.needs_renewal(151)); // 51 epochs >= 50
    }

    #[test]
    fn verify_attest_fresh_and_matching() {
        let a = fresh_attest(100);
        assert!(verify_attest(&a, &[0xAA], 150).is_ok());
    }

    #[test]
    fn verify_attest_stale() {
        let a = fresh_attest(100);
        let result = verify_attest(&a, &[0xAA], 250);
        assert!(matches!(
            result,
            Err(PlatformAdminAttestError::Stale { .. })
        ));
    }

    #[test]
    fn verify_attest_wrong_dc() {
        let a = fresh_attest(100);
        let result = verify_attest(&a, &[0xCC], 150);
        assert_eq!(result, Err(PlatformAdminAttestError::WrongDc));
    }

    #[test]
    fn verify_attest_at_max_age_boundary() {
        // age = MAX_ATTEST_AGE_EPOCHS (100) is still fresh.
        // age = MAX + 1 is stale.
        let a = fresh_attest(100);
        // age = 100 (exact MAX): fresh.
        assert!(verify_attest(&a, &[0xAA], 200).is_ok());
        // age = 101 (one over): stale.
        let result = verify_attest(&a, &[0xAA], 201);
        assert!(matches!(
            result,
            Err(PlatformAdminAttestError::Stale { .. })
        ));
    }

    #[test]
    fn challenge_response_deadline() {
        let c = AttestChallenge::new("d1", vec![0xAA], "stale proof", vec![], 1000);
        assert_eq!(c.response_deadline_epoch, 1010);
        assert!(!c.is_expired(1005));
        assert!(!c.is_expired(1010));
        assert!(c.is_expired(1011));
    }

    #[test]
    fn topic_format() {
        let t = attest_topic("d1", Platform::WhatsApp);
        assert_eq!(t, "/dot/admin/d1/whatsapp");
    }

    #[test]
    #[should_panic(expected = "domain_id must not be empty")]
    fn topic_rejects_empty() {
        let _ = attest_topic("", Platform::WhatsApp);
    }

    #[test]
    fn platform_as_str() {
        assert_eq!(Platform::WhatsApp.as_str(), "whatsapp");
        assert_eq!(Platform::Matrix.as_str(), "matrix");
        assert_eq!(Platform::Custom.as_str(), "custom");
    }

    // -------------------------------------------------------------------------
    // M11 substrate tests (mission 0011-d-M11-phase2-domain-coordinator-
    // platform-binding; RFC-0011-d v1.7.1 §7.4 substrate signature)
    // -------------------------------------------------------------------------

    /// Build a valid `PlatformAdminProof` with the given dc pubkey.
    fn fresh_proof(dc_pubkey: [u8; 32], epoch: u64) -> PlatformAdminProof {
        PlatformAdminProof {
            platform: Platform::WhatsApp,
            platform_admin_id: "120363012345678@whatsapp".into(),
            dc_pubkey: dc_pubkey.to_vec(),
            adapter_signature: vec![0xAB; 64], // non-empty stub (delegated verification)
            nonce: vec![0xCD; 32],
            signed_at_epoch: epoch,
        }
    }

    /// Build a `GroupBinding` in `Bound` state with the given DC pubkey.
    fn bound_binding(dc_pk: [u8; 32]) -> GroupBinding {
        GroupBinding {
            group_jid: "120363012345678@g.us".into(),
            platform: "whatsapp".into(),
            mission_id: [1u8; 32],
            domain_id: [2u8; 32],
            domain_coordinator_id: dc_pk,
            bound_at_epoch: 100,
            renewed_at_epoch: 100,
            state: GroupState::Bound,
            binding_hash: [0u8; 32],
        }
    }

    #[test]
    fn bind_domain_coordinator_updates_binding_atomically() {
        // TV-DC-SUB-1: success path. Operator pubkey matches proof.dc_pubkey;
        // group_binding.state == Bound; returns updated GroupBinding with
        // domain_coordinator_id set to operator pubkey + renewed_at_epoch
        // advanced + binding_hash recomputed.
        let operator_pk = [0x42u8; 32];
        let operator_did = octo_cap_macaroon::signer::did_from_pubkey(&operator_pk);
        let proof = fresh_proof(operator_pk, 100);
        let gb = bound_binding([0xFFu8; 32]); // existing (different) DC

        let result = bind_domain_coordinator(&operator_did, &gb, &proof, 150);
        let updated = result.expect("TV-DC-SUB-1 success path");

        assert_eq!(updated.domain_coordinator_id, operator_pk);
        assert_eq!(updated.renewed_at_epoch, 150);
        assert_ne!(
            updated.binding_hash, gb.binding_hash,
            "binding_hash must be recomputed on update"
        );
        assert_eq!(updated.state, GroupState::Bound);
        // Idempotency: mission_id + domain_id + group_jid + platform unchanged
        assert_eq!(updated.mission_id, gb.mission_id);
        assert_eq!(updated.domain_id, gb.domain_id);
        assert_eq!(updated.group_jid, gb.group_jid);
        assert_eq!(updated.platform, gb.platform);
    }

    #[test]
    fn bind_domain_coordinator_rolls_back_on_signature_invalid() {
        // TV-DC-SUB-2: proof.dc_pubkey != operator pubkey → BindingError::
        // SignatureInvalid; GroupBinding unchanged (atomic rollback).
        let operator_pk = [0x42u8; 32];
        let operator_did = octo_cap_macaroon::signer::did_from_pubkey(&operator_pk);
        let mut proof = fresh_proof(operator_pk, 100);
        proof.dc_pubkey = vec![0xEE; 32]; // mismatch with operator_pk
        let gb = bound_binding([0xFFu8; 32]);

        let result = bind_domain_coordinator(&operator_did, &gb, &proof, 150);
        assert!(matches!(result, Err(BindingError::SignatureInvalid { .. })));
    }

    #[test]
    fn bind_domain_coordinator_rolls_back_on_invalid_transition() {
        // TV-DC-SUB-3: group_binding.state != Bound (e.g., Unbound /
        // UnboundQuarantined) → BindingError::InvalidTransition; GroupBinding
        // unchanged (atomic rollback).
        let operator_pk = [0x42u8; 32];
        let operator_did = octo_cap_macaroon::signer::did_from_pubkey(&operator_pk);
        let proof = fresh_proof(operator_pk, 100);
        let mut gb = bound_binding(operator_pk);
        gb.state = GroupState::Unbound;

        let result = bind_domain_coordinator(&operator_did, &gb, &proof, 150);
        assert!(matches!(
            result,
            Err(BindingError::InvalidTransition {
                from: GroupState::Unbound,
                to: GroupState::Bound,
            })
        ));
    }

    #[test]
    fn bind_domain_coordinator_rolls_back_on_stale_proof() {
        // Adapter to TV-DC-SUB-4: proof.signed_at_epoch too old (stale) →
        // BindingError::SignatureInvalid with stale-prefix reason.
        let operator_pk = [0x42u8; 32];
        let operator_did = octo_cap_macaroon::signer::did_from_pubkey(&operator_pk);
        let proof = fresh_proof(operator_pk, 100); // signed at epoch 100
        let gb = bound_binding([0xFFu8; 32]);

        // current_epoch 250 → age 150 > MAX_ATTEST_AGE_EPOCHS=100 → stale
        let result = bind_domain_coordinator(&operator_did, &gb, &proof, 250);
        assert!(matches!(
            result,
            Err(BindingError::SignatureInvalid { ref reason }) if reason.contains("stale")
        ));
    }

    #[test]
    fn bind_domain_coordinator_rejects_non_canonical_did() {
        // Edge: operator_did is not a canonical did:octo:0x<hex> form →
        // BindingError::SignatureInvalid with parse-error reason; GroupBinding
        // unchanged.
        let proof = fresh_proof([0x42u8; 32], 100);
        let gb = bound_binding([0xFFu8; 32]);

        let result = bind_domain_coordinator("not-a-canonical-did", &gb, &proof, 150);
        assert!(matches!(result, Err(BindingError::SignatureInvalid { .. })));
    }

    #[test]
    fn platform_admin_proof_serialization_roundtrip() {
        // TV-DC-SUB-5: PlatformAdminProof round-trip serialization. Same
        // inputs → same bytes (canonical RFC-0104 DFP property).
        let proof = fresh_proof([0x42u8; 32], 100);
        let json = serde_json::to_string(&proof).expect("serialize");
        let parsed: PlatformAdminProof = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, proof);
    }
}
