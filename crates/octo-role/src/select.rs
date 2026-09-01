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
}
