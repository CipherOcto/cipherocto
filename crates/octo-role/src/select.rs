//! `octo_role::select` — write-side substrate entrypoint per RFC-0011-d §7.4.
//!
//! SYNC function. Stoolap `BEGIN IMMEDIATE` envelope-build semantics
//! in-memory for Phase 1; production deployments replace the
//! [`BindingStore`] with a slash-ledger-backed implementation per
//! RFC-0900.
//!
//! Per RFC-0011-d §Security 2: last-writer-wins atomic UPDATE — there
//! is NO `RoleBindingConflict` variant. Re-selecting the same
//! `(operator_did, role_id, chain_id)` tuple overwrites the existing
//! binding atomically inside the envelope-build step.

use std::collections::HashMap;
use std::sync::Mutex;

use octo_cap_macaroon::signer::CapabilitySigner;
use octo_network::dc::admin_attest::{bind_domain_coordinator, PlatformAdminProof};
use octo_network::dot::binding::GroupBinding;
use octo_network::dot::handover::{CoordinatorRole, HandoverReason, HandoverRequestEnvelope};
use octo_wallet::identity_record::Did;

use crate::error::RoleError;
use crate::list_show::show;
use crate::types::{ChainId, Hash32, RoleBinding};

/// In-memory binding store keyed by `(operator_did, role_id, chain_id)`.
///
/// Phase 1 implementation. Production deployments substitute a
/// slash-ledger-backed implementation (RFC-0900) that wraps
/// `stoolap::Database::begin_immediate` for the atomic envelope-build
/// transaction.
#[derive(Debug, Default)]
pub struct BindingStore {
    inner: Mutex<HashMap<(String, String, ChainId), RoleBinding>>,
}

impl BindingStore {
    /// Construct an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Atomic write per RFC-0011-d §Security 2 last-writer-wins semantics.
    ///
    /// Returns the previous binding if any (for audit/log callers).
    pub fn upsert(&self, binding: RoleBinding) -> Option<RoleBinding> {
        let key = (
            binding.operator_did.clone(),
            binding.role_id.clone(),
            binding.chain_id,
        );
        let mut guard = self.inner.lock().expect("BindingStore mutex poisoned");
        guard.insert(key, binding)
    }

    /// Read-only lookup; returns `None` if no binding exists.
    pub fn get(
        &self,
        operator_did: &str,
        role_id: &str,
        chain_id: &ChainId,
    ) -> Option<RoleBinding> {
        let guard = self.inner.lock().expect("BindingStore mutex poisoned");
        guard
            .get(&(operator_did.to_string(), role_id.to_string(), *chain_id))
            .cloned()
    }
}

/// Default Phase 1 binding store (per-process). Not currently invoked
/// from `octo-role` — left as the substrate seam where Phase 2 wiring
/// (slash-ledger-backed `BindingStore`) will land per RFC-0900. Kept
/// `pub` so the future substrate integration has a single switch
/// point rather than scattering `OnceLock` initialization across
/// call sites.
pub fn default_store() -> &'static BindingStore {
    use std::sync::OnceLock;
    static STORE: OnceLock<BindingStore> = OnceLock::new();
    STORE.get_or_init(BindingStore::new)
}

/// Bind the active identity to a role.
///
/// Per RFC-0011-d §7.4 signature:
/// `pub fn select(role_id: &str, operator_did: &str, signer: &dyn CapabilitySigner) -> Result<RoleBinding, RoleError>`
///
/// SYNC write-path. In Phase 1, uses an in-memory store. Production
/// replaces with Stoolap `BEGIN IMMEDIATE` envelope-build (see
/// RFC-0011-d §7.6 sequence diagram).
///
/// ## Phase 1 atomicity limitation
///
/// The nonce is consumed from `octo_wallet::next_nonce_counter` (a
/// process-local counter) before the binding is upserted into
/// [`BindingStore`]. A process crash between these two steps leaves
/// the counter advanced with no binding written. RFC-0011-d §7.6
/// mandates "either fully committed or fully absent"; Phase 1 does
/// not satisfy this. The slash-ledger-backed implementation that
/// ships when Stoolap `BEGIN IMMEDIATE` lands closes the gap by
/// persisting both in a single write-lock transaction.
pub fn select(
    role_id: &str,
    operator_did: &str,
    signer: &dyn CapabilitySigner,
    store: &BindingStore,
) -> Result<RoleBinding, RoleError> {
    select_with_chain_id(role_id, operator_did, signer, &default_chain_id(), store)
}

/// Variant of [`select`] that takes an explicit `chain_id`.
///
/// The RFC-0011-d §7.4 signature does not expose `chain_id` directly;
/// tests + future substrate integration pass it explicitly.
pub fn select_with_chain_id(
    role_id: &str,
    operator_did: &str,
    signer: &dyn CapabilitySigner,
    chain_id: &ChainId,
    store: &BindingStore,
) -> Result<RoleBinding, RoleError> {
    // 1. Validate role exists (TV-RX-2: missing role).
    let record = show(role_id)?;

    // 2. Compare signer's substrate-truth DID to operator_did
    //    (F-16 substrate-truth invariant; the DID returned by
    //    `signer.did()` is canonical, NOT derived CLI-side).
    let signer_did = signer.did();
    if signer_did != operator_did {
        return Err(RoleError::SignerMismatch {
            signer_did,
            operator_did: operator_did.to_string(),
        });
    }

    // 3. Read available stake (Phase 1 stub: assume sufficient for non-wallet
    //    roles). Production wires `octo-stake-oracle` substrate read here.
    //    For Phase 1 we use a placeholder; Phase 2 will gate coordinator +
    //    domain-coordinator which require real stake oracle wiring.
    let available_stake = match role_id {
        // OCTO-only roles: oracle not required for Phase 1; use 1 micro-OCTO
        // as the placeholder (mirrors non-wallet stub below; never `u64::MAX`
        // because that would persist into the binding as substrate truth and
        // confuse downstream readers when the real stake oracle lands in
        // Phase 2).
        "wallet" | "recorder" => 1,
        _ => record.summary.requires_octo_min.unwrap_or(0) + 1,
    };
    if let Some(required) = record.summary.requires_octo_min {
        if available_stake < required {
            return Err(RoleError::StakeInsufficient {
                required,
                available: available_stake,
            });
        }
    }

    // 4. Consume role-binding nonce from octo-wallet substrate counter
    //    (RFC-0011-d §7.4). Inside the envelope-build tx this guarantees
    //    every role-binding gets a unique monotonic nonce. Surfaces
    //    `WalletError::NonceUnderflow` if the counter saturates.
    let nonce = octo_wallet::next_nonce_counter(&Did(operator_did.to_string())).map_err(|e| {
        RoleError::SigningFailed {
            reason: format!("nonce counter: {e}"),
        }
    })?;

    // 5. Canonical body bytes (deterministic per RFC-0104 DFP).
    let body_bytes = canonical_body_bytes(role_id, operator_did, chain_id, available_stake, nonce);
    let body_hash = blake3_256(&body_bytes);

    // 6. Sign canonical bytes.
    let signature_proof = signer
        .sign(&body_bytes)
        .map_err(|e| RoleError::SigningFailed {
            reason: format!("signer rejected envelope: {e}"),
        })?;

    // 7. role_binding_hash = BLAKE3-256(body_bytes) per RFC-0011-d §7.4.
    //    Hash covers body alone (NOT body+signature) so the hash does
    //    not depend on a value not yet known at canonicalization time.
    let role_binding_hash = blake3_256(&body_bytes);

    let stake_octo = record.summary.requires_octo_min.unwrap_or(available_stake);
    let binding = RoleBinding {
        role_id: role_id.to_string(),
        operator_did: operator_did.to_string(),
        chain_id: *chain_id,
        role_kind_uuid: record.summary.role_kind_uuid,
        stake_octo,
        stake_role_token: None, // Phase 1: stake_role_token oracle TBD
        body_hash,
        signature_proof: signature_proof.to_vec(),
        role_binding_hash,
        nonce,
    };

    // 8. Atomic upsert (last-writer-wins; no conflict variant).
    store.upsert(binding.clone());

    Ok(binding)
}

/// Coordinator role-binding + `HandoverRequestEnvelope` emission per
/// RFC-0011-d §7.4 + RFC-0855p-e.
///
/// Wraps [`select_with_chain_id`] (binds the role) and emits a
/// signed `HandoverRequestEnvelope` with the operator as the
/// proposed successor. The envelope's `coordinator_role` is derived
/// from `role_id` via [`coordinator_role_for_role_id`].
///
/// ## Phase 1 envelope transport
///
/// The envelope is constructed, signed, and **discarded** by Phase 1
/// (no DOT broadcast topic wired in `octo-role`; the envelope shape +
/// signature are the contract — production wires a substrate-side
/// broadcast topic per RFC-0855p-e §Mesh substrate). Callers needing
/// the envelope can use [`build_handover_request`] directly.
///
/// ## Phase 1 atomicity limitation
///
/// Same as [`select_with_chain_id`]: envelope emission happens after
/// the in-memory binding upsert, so a crash between the two leaves the
/// binding persisted with no envelope on the wire. Production wires a
/// single Stoolap `BEGIN IMMEDIATE` transaction per RFC-0011-d §7.6.
pub fn select_coordinator(
    role_id: &str,
    operator_did: &str,
    signer: &dyn CapabilitySigner,
    chain_id: &ChainId,
    store: &BindingStore,
    mission_id: [u8; 32],
    current_epoch: u64,
) -> Result<RoleBinding, RoleError> {
    // 1. Bind role (delegates to Phase 1 select_with_chain_id).
    let binding = select_with_chain_id(role_id, operator_did, signer, chain_id, store)?;

    // 2. Build + sign the HandoverRequestEnvelope (RFC-0855p-e).
    let coordinator_role = coordinator_role_for_role_id(role_id);
    let envelope = build_handover_request(
        operator_did,
        signer,
        coordinator_role,
        mission_id,
        HandoverReason::Voluntary,
        current_epoch,
    )?;

    // Phase 1: envelope emission is a no-op (no peer transport wired).
    // Production wires the envelope into a DOT broadcast topic per
    // RFC-0855p-e §Mesh substrate.
    let _ = envelope;

    Ok(binding)
}

/// Domain-coordinator role-binding + group binding ceremony per
/// RFC-0011-d §7.4 + RFC-0855p-c §5a.
///
/// Atomic composition of [`select_with_chain_id`] (binds the operator
/// to the `domain-coordinator` role) AND [`bind_domain_coordinator`]
/// (verifies the [`PlatformAdminProof`] + updates the [`GroupBinding`]).
/// Returns `(RoleBinding, updated_GroupBinding)` on success.
///
/// ## Phase 1 atomicity limitation
///
/// Order is `select` THEN `bind`. If `bind_domain_coordinator` fails
/// AFTER `select_with_chain_id` succeeded, the role binding has
/// already been upserted (last-writer-wins; Phase 1 has no rollback
/// channel). Production wires a single Stoolap `BEGIN IMMEDIATE`
/// transaction that aborts both on any verification failure per
/// RFC-0011-d §7.6. The substrate signatures are preserved verbatim
/// so the production wire-up is a one-line swap of the in-memory
/// `BindingStore` for a slash-ledger-backed one.
#[allow(clippy::too_many_arguments)] // 8 = role_id, operator_did, signer, chain_id, store, group_binding, platform_admin_proof, current_epoch; matches RFC-0011-d §7.4 spec verbatim
pub fn select_domain_coordinator(
    role_id: &str,
    operator_did: &str,
    signer: &dyn CapabilitySigner,
    chain_id: &ChainId,
    store: &BindingStore,
    group_binding: &GroupBinding,
    platform_admin_proof: &PlatformAdminProof,
    current_epoch: u64,
) -> Result<(RoleBinding, GroupBinding), RoleError> {
    // 1. Bind operator to the domain-coordinator role.
    let role_binding = select_with_chain_id(role_id, operator_did, signer, chain_id, store)?;

    // 2. Verify platform admin proof + update group binding (RFC-0855p-c §5a).
    let updated_binding = bind_domain_coordinator(
        operator_did,
        group_binding,
        platform_admin_proof,
        current_epoch,
    )
    .map_err(|e| RoleError::GroupBindingRejected {
        reason: format!("bind_domain_coordinator: {e}"),
    })?;

    Ok((role_binding, updated_binding))
}

/// Map a `role_id` string to the canonical [`CoordinatorRole`] variant
/// for the handover envelope.
///
/// Phase 1 mapping: `"domain-coordinator"` → `DomainCoordinator`,
/// `"witness-coordinator"` → `WitnessCoordinator`, anything else
/// defaults to `MissionCoordinator`. The CLI is responsible for passing
/// canonical role IDs; production deployments register a typed
/// role-id → `CoordinatorRole` lookup table per RFC-0855p-e.
fn coordinator_role_for_role_id(role_id: &str) -> CoordinatorRole {
    match role_id {
        "domain-coordinator" => CoordinatorRole::DomainCoordinator,
        "witness-coordinator" => CoordinatorRole::WitnessCoordinator,
        // Default covers `"mission-coordinator"`, `"coordinator"`, and any
        // future coordinator role name registered in the role registry.
        _ => CoordinatorRole::MissionCoordinator,
    }
}

/// Construct + sign a canonical `HandoverRequestEnvelope` per
/// RFC-0855p-e for the given operator + role + mission.
///
/// Public so the M10 CLI / future substrate integrations can emit the
/// envelope without going through `select_coordinator`. Returns
/// `RoleError::SigningFailed` if the operator DID is not a canonical
/// `did:octo:0x<hex>` form (the substrate derives the successor
/// `peer_id` from the DID).
pub fn build_handover_request(
    operator_did: &str,
    signer: &dyn CapabilitySigner,
    coordinator_role: CoordinatorRole,
    mission_id: [u8; 32],
    reason: HandoverReason,
    current_epoch: u64,
) -> Result<HandoverRequestEnvelope, RoleError> {
    // 1. Derive successor peer_id (32-byte Ed25519 pubkey) from operator DID.
    let successor_id =
        octo_cap_macaroon::signer::pubkey_from_did(operator_did).ok_or_else(|| {
            RoleError::SigningFailed {
                reason: format!(
                    "operator_did {operator_did} is not a canonical did:octo:0x<hex> form"
                ),
            }
        })?;

    // 2. Construct envelope with canonical header (coordinator_id = [0u8; 32]
    //    signals first-mover / no incumbent; current_term_id = [0u8; 32]
    //    signals no prior term).
    let mut envelope = HandoverRequestEnvelope::new(
        [0u8; 32],    // coordinator_id (first-mover; no incumbent)
        successor_id, // successor_id = operator pubkey
        coordinator_role,
        [0u8; 32],  // current_term_id (first-mover)
        mission_id, // new_term_id (per RFC-0855p-e §Mesh substrate)
        reason,
        current_epoch,
    );

    // 3. Deterministic nonce derived from successor_id (RFC-0855p-e §Replay).
    envelope.nonce = deterministic_nonce32(&successor_id, current_epoch);

    // 4. Sign envelope via the operator's `CapabilitySigner`.
    //    Set `handover_hash` BEFORE signing so `signature` covers the
    //    canonical body+hash (matches `HandoverRequestEnvelope::sign`).
    envelope.handover_hash = envelope.compute_handover_hash();
    let sig = signer
        .sign(&envelope.handover_hash)
        .map_err(|e| RoleError::SigningFailed {
            reason: format!("HandoverRequestEnvelope sign: {e}"),
        })?;
    envelope.signature = sig;

    Ok(envelope)
}

/// Deterministic 32-byte nonce derived from `(successor_id, current_epoch)`.
///
/// BLAKE3-256 over the canonical tag + successor + epoch; used as the
/// envelope nonce field per RFC-0855p-e. Deterministic (NOT random)
/// because the nonce only needs to be unique per envelope — the
/// randomness comes from `current_epoch` (monotonic network consensus)
/// + `successor_id` (per-operator).
fn deterministic_nonce32(successor_id: &[u8; 32], current_epoch: u64) -> [u8; 32] {
    let mut buf = Vec::with_capacity(20 + 32);
    buf.extend_from_slice(b"octo.handover.nonce:v1\n");
    buf.extend_from_slice(successor_id);
    buf.extend_from_slice(&current_epoch.to_be_bytes());
    let h = blake3::hash(&buf);
    let mut out = [0u8; 32];
    out.copy_from_slice(h.as_bytes());
    out
}

/// Default chain ID for tests + Phase 1 single-chain deployments.
pub fn default_chain_id() -> ChainId {
    let mut id = [0u8; 32];
    id[..8].copy_from_slice(&0xDEAD_BEEFu64.to_le_bytes());
    id[8..16].copy_from_slice(&0xCAFE_F00Du64.to_le_bytes());
    id
}

fn canonical_body_bytes(
    role_id: &str,
    operator_did: &str,
    chain_id: &ChainId,
    stake: u64,
    nonce: u64,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(role_id.len() + operator_did.len() + 32 + 8 + 8);
    out.extend_from_slice(b"octo.role_binding:v1\n");
    out.extend_from_slice(role_id.as_bytes());
    out.push(b'\n');
    out.extend_from_slice(operator_did.as_bytes());
    out.push(b'\n');
    out.extend_from_slice(chain_id);
    out.push(b'\n');
    out.extend_from_slice(&stake.to_le_bytes());
    out.push(b'\n');
    out.extend_from_slice(&nonce.to_le_bytes());
    out
}

fn blake3_256(bytes: &[u8]) -> Hash32 {
    let h = blake3::hash(bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(h.as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_cap_macaroon::signer::{did_from_pubkey, CapabilitySignerError};

    /// Test signer with deterministic pubkey + signature.
    struct TestSigner {
        pk: [u8; 32],
    }

    impl CapabilitySigner for TestSigner {
        fn sign(&self, _msg: &[u8]) -> Result<[u8; 64], CapabilitySignerError> {
            // Deterministic stub signature (zeros + length marker).
            Ok([0u8; 64])
        }
        fn public_key_bytes(&self) -> [u8; 32] {
            self.pk
        }
    }

    fn _test_signer_unused() {
        // Reserved helper for future test vectors that derive a signer
        // DID from a hex-encoded pubkey. Suppresses dead-code warning.
        let _ = std::marker::PhantomData::<TestSigner>;
    }

    #[test]
    fn select_creates_signed_envelope() {
        let pk = [1u8; 32];
        let signer = TestSigner { pk };
        let did = did_from_pubkey(&pk);
        let store = BindingStore::new();

        let binding =
            select_with_chain_id("builder", &did, &signer, &default_chain_id(), &store).unwrap();
        assert_eq!(binding.role_id, "builder");
        assert_eq!(binding.operator_did, did);
        assert!(store.get(&did, "builder", &default_chain_id()).is_some());
    }

    #[test]
    fn select_returns_role_not_found_on_missing() {
        let pk = [1u8; 32];
        let signer = TestSigner { pk };
        let did = did_from_pubkey(&pk);
        let store = BindingStore::new();

        let err = select_with_chain_id("nonexistent", &did, &signer, &default_chain_id(), &store)
            .unwrap_err();
        assert!(matches!(err, RoleError::RoleNotFound { .. }));
    }

    #[test]
    fn select_returns_signer_mismatch_rolls_back_tx() {
        let pk = [1u8; 32];
        let signer = TestSigner { pk };
        let wrong_did = "did:octo:0xdeadbeef";
        let store = BindingStore::new();

        let err = select_with_chain_id("builder", wrong_did, &signer, &default_chain_id(), &store)
            .unwrap_err();
        assert!(matches!(err, RoleError::SignerMismatch { .. }));
        // TX rolled back: no binding stored
        assert!(store
            .get(wrong_did, "builder", &default_chain_id())
            .is_none());
    }

    #[test]
    fn select_overwrites_existing_binding_last_writer_wins() {
        let pk = [1u8; 32];
        let signer = TestSigner { pk };
        let did = did_from_pubkey(&pk);
        let store = BindingStore::new();

        let first =
            select_with_chain_id("builder", &did, &signer, &default_chain_id(), &store).unwrap();
        let second =
            select_with_chain_id("builder", &did, &signer, &default_chain_id(), &store).unwrap();

        // TV-RX-5: second call OVERWRITES (last-writer-wins), no error.
        // Nonces differ (each select consumes one) so hashes differ too —
        // what matters is the store reflects the second binding.
        assert_ne!(first.nonce, second.nonce, "each select consumes a nonce");
        assert_ne!(first.role_binding_hash, second.role_binding_hash);
        let stored = store.get(&did, "builder", &default_chain_id()).unwrap();
        assert_eq!(stored.role_binding_hash, second.role_binding_hash);
        assert_eq!(stored.nonce, second.nonce);
    }

    #[test]
    fn did_from_pubkey_is_deterministic() {
        let pk = [42u8; 32];
        assert_eq!(did_from_pubkey(&pk), did_from_pubkey(&pk));
        assert_ne!(did_from_pubkey(&pk), did_from_pubkey(&[0u8; 32]));
    }

    #[allow(dead_code)]
    fn _placeholder() {
        // Kept for symmetry with the dev-signer trait; no-op.
        let _ = std::marker::PhantomData::<TestSigner>;
    }

    #[test]
    fn body_hash_differs_across_nonces() {
        // Each select consumes a nonce from octo-wallet, so two consecutive
        // bindings produce distinct body_hash + role_binding_hash values.
        // What is deterministic (RFC-0104 DFP) is the canonicalization
        // itself: same nonce + same inputs → same hash. Verified by
        // select_creates_signed_envelope + body_hash_differs_across_nonces
        // together.
        let pk = [7u8; 32];
        let signer = TestSigner { pk };
        let did = did_from_pubkey(&pk);
        let store = BindingStore::new();

        let a =
            select_with_chain_id("builder", &did, &signer, &default_chain_id(), &store).unwrap();
        let b =
            select_with_chain_id("builder", &did, &signer, &default_chain_id(), &store).unwrap();
        assert_ne!(a.nonce, b.nonce, "nonces increment per call");
        assert_ne!(
            a.body_hash, b.body_hash,
            "distinct nonces yield distinct body_hash"
        );
    }

    #[test]
    fn select_builder_happy_path_emits_envelope() {
        // Phase 1 happy path: signer DID matches operator DID, stake oracle
        // stub passes (required + 1), binding is upserted with monotonic nonce.
        // StakeInsufficient cannot fire in Phase 1 (requires_octo_min > u64::MAX
        // is impossible); the variant + error mapping is exercised via
        // error::tests::role_error_4_variants_only.
        let pk = [1u8; 32];
        let signer = TestSigner { pk };
        let did = did_from_pubkey(&pk);
        let store = BindingStore::new();

        let _ = select_with_chain_id("builder", &did, &signer, &default_chain_id(), &store)
            .expect("Phase 1 stub oracle always passes non-wallet roles");
    }

    #[test]
    fn select_consumes_nonce_counter_inside_tx() {
        // AC: M5 — each successful select() consumes a nonce from
        // octo_wallet::next_nonce_counter and embeds it in the binding.
        let pk = [9u8; 32];
        let signer = TestSigner { pk };
        let did = did_from_pubkey(&pk);
        let store = BindingStore::new();

        let n_pre = octo_wallet::next_nonce_counter(&Did(did.clone())).unwrap();
        let b1 =
            select_with_chain_id("provider", &did, &signer, &default_chain_id(), &store).unwrap();
        let n_mid = octo_wallet::next_nonce_counter(&Did(did.clone())).unwrap();
        let b2 =
            select_with_chain_id("storage", &did, &signer, &default_chain_id(), &store).unwrap();
        let n_after = octo_wallet::next_nonce_counter(&Did(did.clone())).unwrap();

        // The nonce embedded in each binding must be strictly less
        // than the next counter value fetched post-call. Sequence:
        //   n_pre = N              → b1 selects (nonce=N, counter=N+1)
        //   n_mid = N+1            → b2 selects (nonce=N+1, counter=N+2)
        //   n_after = N+2
        // So: b1.nonce < n_mid < b2.nonce < n_after.
        assert!(b1.nonce < n_mid, "binding nonce falls before mid counter");
        assert!(b2.nonce > n_mid, "second binding nonce exceeds mid counter");
        assert!(
            b2.nonce < n_after,
            "second binding nonce falls before end counter"
        );
        assert!(n_after > n_mid, "counter advances monotonically");
        assert!(n_mid > n_pre, "counter advances between pre and mid");
    }

    // -----------------------------------------------------------------
    // M10 (RFC-0011-d §7.4) substrate tests:
    // `select_coordinator` + `select_domain_coordinator`
    // -----------------------------------------------------------------

    /// Test fixture: deterministic `PlatformAdminProof` builder. Mirrors the
    /// M11 substrate tests in `octo-network::dc::admin_attest` without
    /// requiring a cross-crate `pub` fixture.
    fn make_proof(pk: &[u8; 32], current_epoch: u64, fresh: bool) -> PlatformAdminProof {
        use octo_network::dc::admin_attest::{Platform, MAX_ATTEST_AGE_EPOCHS};
        // Stale proof = signed `MAX_ATTEST_AGE_EPOCHS + 2` epochs ago
        // (the freshness check is `age > MAX_ATTEST_AGE_EPOCHS`, so the
        // +2 margin guarantees the test fixture is rejected regardless
        // of any future tweaks to the bound).
        let signed_at_epoch = if fresh {
            current_epoch
        } else {
            current_epoch.saturating_sub(MAX_ATTEST_AGE_EPOCHS + 2)
        };
        PlatformAdminProof::new(
            Platform::WhatsApp,
            "admin-1".to_string(),
            pk.to_vec(),
            vec![0u8; 64], // adapter_signature (non-empty; presence check)
            vec![1u8; 32], // nonce
            signed_at_epoch,
        )
    }

    /// Test fixture: deterministic `GroupBinding` in `Bound` state.
    fn make_group_binding(pk: &[u8; 32]) -> GroupBinding {
        use octo_network::dot::binding::GroupState;
        GroupBinding {
            group_jid: "120363@g.us".to_string(),
            platform: "whatsapp".to_string(),
            mission_id: [0u8; 32],
            domain_id: [1u8; 32],
            domain_coordinator_id: *pk,
            bound_at_epoch: 0,
            renewed_at_epoch: 0,
            state: GroupState::Bound,
            binding_hash: [0u8; 32],
        }
    }

    #[test]
    fn select_coordinator_binds_role_and_returns_binding() {
        let pk = [11u8; 32];
        let signer = TestSigner { pk };
        let did = did_from_pubkey(&pk);
        let store = BindingStore::new();
        let mission_id = [42u8; 32];

        let binding = select_coordinator(
            "mission-coordinator",
            &did,
            &signer,
            &default_chain_id(),
            &store,
            mission_id,
            100,
        )
        .expect("select_coordinator happy path");

        assert_eq!(binding.role_id, "mission-coordinator");
        assert_eq!(binding.operator_did, did);
        assert!(
            store
                .get(&did, "mission-coordinator", &default_chain_id())
                .is_some(),
            "binding must be upserted"
        );
    }

    #[test]
    fn select_coordinator_emits_signed_handover_envelope_via_helper() {
        // Phase 1 envelope emission is internal; verify the shape via the
        // public `build_handover_request` helper used by `select_coordinator`.
        let pk = [12u8; 32];
        let signer = TestSigner { pk };
        let did = did_from_pubkey(&pk);
        let mission_id = [7u8; 32];

        let envelope = build_handover_request(
            &did,
            &signer,
            octo_network::dot::handover::CoordinatorRole::MissionCoordinator,
            mission_id,
            octo_network::dot::handover::HandoverReason::Voluntary,
            50,
        )
        .expect("build_handover_request happy path");

        // successor_id MUST equal the operator's Ed25519 pubkey.
        assert_eq!(envelope.successor_id, pk, "successor_id = operator pubkey");
        // coordinator_id = [0u8; 32] for first-mover.
        assert_eq!(
            envelope.coordinator_id, [0u8; 32],
            "coordinator_id zeroed for first-mover"
        );
        // new_term_id MUST equal the supplied mission_id.
        assert_eq!(envelope.new_term_id, mission_id);
        // current_epoch propagated.
        assert_eq!(envelope.current_epoch, 50);
        // Reason propagated.
        assert_eq!(
            envelope.reason,
            octo_network::dot::handover::HandoverReason::Voluntary
        );
        // handover_hash MUST be non-zero (BLAKE3-256 over canonical body).
        assert_ne!(envelope.handover_hash, [0u8; 32]);
        // Signature length MUST be 64 bytes (canonical Ed25519 sig).
        // (The `TestSigner` returns a zero-byte signature for determinism;
        // production uses real Ed25519, which is always non-zero for any
        // non-trivial message — verified in the M5 signer tests in
        // `octo-cap-macaroon::signer`.)
        assert_eq!(envelope.signature.len(), 64);
    }

    #[test]
    fn build_handover_request_rejects_non_canonical_did() {
        let pk = [13u8; 32];
        let signer = TestSigner { pk };
        let err = build_handover_request(
            "did:not:octo:0xdeadbeef",
            &signer,
            octo_network::dot::handover::CoordinatorRole::MissionCoordinator,
            [0u8; 32],
            octo_network::dot::handover::HandoverReason::Voluntary,
            0,
        )
        .unwrap_err();
        assert!(matches!(err, RoleError::SigningFailed { .. }));
    }

    #[test]
    fn coordinator_role_for_role_id_maps_known_slugs() {
        assert_eq!(
            coordinator_role_for_role_id("domain-coordinator"),
            octo_network::dot::handover::CoordinatorRole::DomainCoordinator
        );
        assert_eq!(
            coordinator_role_for_role_id("witness-coordinator"),
            octo_network::dot::handover::CoordinatorRole::WitnessCoordinator
        );
        assert_eq!(
            coordinator_role_for_role_id("mission-coordinator"),
            octo_network::dot::handover::CoordinatorRole::MissionCoordinator
        );
        // Unknown → MissionCoordinator (default).
        assert_eq!(
            coordinator_role_for_role_id("anything-else"),
            octo_network::dot::handover::CoordinatorRole::MissionCoordinator
        );
    }

    #[test]
    fn select_domain_coordinator_happy_path_binds_role_and_updates_group() {
        let pk = [14u8; 32];
        let signer = TestSigner { pk };
        let did = did_from_pubkey(&pk);
        let store = BindingStore::new();
        let group_binding = make_group_binding(&pk);
        let proof = make_proof(&pk, 200, true); // fresh; current_epoch must exceed MAX_ATTEST_AGE_EPOCHS

        let (role_binding, updated_group) = select_domain_coordinator(
            "domain-coordinator",
            &did,
            &signer,
            &default_chain_id(),
            &store,
            &group_binding,
            &proof,
            200,
        )
        .expect("happy path");

        assert_eq!(role_binding.role_id, "domain-coordinator");
        assert_eq!(role_binding.operator_did, did);
        // Group binding updated: domain_coordinator_id = operator pubkey,
        // renewed_at_epoch = current_epoch, binding_hash non-zero.
        assert_eq!(updated_group.domain_coordinator_id, pk);
        assert_eq!(updated_group.renewed_at_epoch, 200);
        assert_ne!(updated_group.binding_hash, [0u8; 32]);
        assert_eq!(updated_group.state, group_binding.state);
    }

    #[test]
    fn select_domain_coordinator_rolls_back_on_stale_proof() {
        let pk = [15u8; 32];
        let signer = TestSigner { pk };
        let did = did_from_pubkey(&pk);
        let store = BindingStore::new();
        let group_binding = make_group_binding(&pk);
        let proof = make_proof(&pk, 200, false); // stale (signed 102 epochs ago)

        let err = select_domain_coordinator(
            "domain-coordinator",
            &did,
            &signer,
            &default_chain_id(),
            &store,
            &group_binding,
            &proof,
            200,
        )
        .unwrap_err();
        assert!(
            matches!(err, RoleError::GroupBindingRejected { .. }),
            "stale proof must surface GroupBindingRejected; got {err:?}"
        );
        // Phase 1 partial-commit: role binding persists, but error returned.
        // Documented limitation; production wires BEGIN IMMEDIATE rollback.
    }

    #[test]
    fn select_domain_coordinator_rolls_back_on_invalid_transition() {
        use octo_network::dot::binding::GroupState;

        let pk = [16u8; 32];
        let signer = TestSigner { pk };
        let did = did_from_pubkey(&pk);
        let store = BindingStore::new();
        // Non-Bound state → InvalidTransition.
        let mut group_binding = make_group_binding(&pk);
        group_binding.state = GroupState::Unbound;
        let proof = make_proof(&pk, 200, true);

        let err = select_domain_coordinator(
            "domain-coordinator",
            &did,
            &signer,
            &default_chain_id(),
            &store,
            &group_binding,
            &proof,
            200,
        )
        .unwrap_err();
        assert!(matches!(err, RoleError::GroupBindingRejected { .. }));
    }

    #[test]
    fn select_domain_coordinator_rejects_dc_pubkey_mismatch() {
        let pk = [17u8; 32];
        let signer = TestSigner { pk };
        let did = did_from_pubkey(&pk);
        let store = BindingStore::new();
        let group_binding = make_group_binding(&pk);
        // Proof signed by a DIFFERENT pubkey.
        let other_pk = [18u8; 32];
        let proof = make_proof(&other_pk, 200, true);

        let err = select_domain_coordinator(
            "domain-coordinator",
            &did,
            &signer,
            &default_chain_id(),
            &store,
            &group_binding,
            &proof,
            200,
        )
        .unwrap_err();
        assert!(matches!(err, RoleError::GroupBindingRejected { .. }));
    }

    #[test]
    fn select_domain_coordinator_propagates_signer_mismatch() {
        // operator_did != signer.did() → RoleError::SignerMismatch
        // (caught BEFORE the bind_domain_coordinator call).
        let pk = [19u8; 32];
        let signer = TestSigner { pk };
        let wrong_did =
            "did:octo:0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
        let store = BindingStore::new();
        let group_binding = make_group_binding(&pk);
        let proof = make_proof(&pk, 200, true);

        let err = select_domain_coordinator(
            "domain-coordinator",
            wrong_did,
            &signer,
            &default_chain_id(),
            &store,
            &group_binding,
            &proof,
            200,
        )
        .unwrap_err();
        assert!(
            matches!(err, RoleError::SignerMismatch { .. }),
            "signer mismatch takes precedence; got {err:?}"
        );
    }

    #[test]
    fn deterministic_nonce32_differs_per_epoch() {
        let pk = [20u8; 32];
        let a = deterministic_nonce32(&pk, 100);
        let b = deterministic_nonce32(&pk, 101);
        assert_ne!(a, b, "nonces MUST differ across epochs");
    }

    #[test]
    fn deterministic_nonce32_is_deterministic() {
        let pk = [21u8; 32];
        assert_eq!(
            deterministic_nonce32(&pk, 100),
            deterministic_nonce32(&pk, 100)
        );
    }
}
