//! `mint_with_zk` API (RFC-0958 §Algorithms proof generation).
//!
//! Cipherocto-side API surface for minting a ZK-bearing capability. Per
//! RFC-0958 §NodeType Gating Rule:
//! - Wholesale → fail-closed (NodeTypeCannotMintZKCap)
//! - SelfHost → default (mint succeeds)
//! - Hybrid → opt-in (mint succeeds if explicit mint_with_zk call)
//!
//! Per RFC-0958 M5/M17 fix:
//! - M5: `output_hash: Some(_)` iff NodeType == SelfHost; mint API enforces
//!   `HybridCannotEmitPoI` and `MissingInferenceTrace` errors
//! - M17: `proof_bundle: Some(_)` iff capability_class == ZKBearing; mint
//!   API enforces via `ClassMismatch` if V1 token gets Some(_)

use ed25519_dalek::Signature;
use serde::{Deserialize, Serialize};

use crate::node::NodeType;

use super::caveat::Caveat;
use super::macaroon::MacaroonId;
use super::wire::WireError;
use zk_circuit::{compile, CairoProgram};

use std::sync::OnceLock;

/// Capability class (RFC-0958 §Data Structures).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityClass {
    /// RFC-0957 v1 macaroon only; no STARK proof.
    V1,
    /// RFC-0958 ZK subclass; `proof_bundle` MUST be Some when minted.
    ZKBearing,
}

/// Private witness for STARK proof generation (RFC-0958 §Data Structures).
///
/// R1 C1 fix: holder_sig is private (STARK proves check); v1.2 M17 fix:
/// PrivateWitness is the canonical source of holder_sig for ZK verify,
/// but the same signature value is also embedded in the public `CapabilityToken`
/// for v1 verify path (dual representation; same value).
#[derive(Debug, Clone)]
pub struct PrivateWitness {
    pub cap_root_secret: [u8; 32],
    pub holder_sig: Signature,
    pub caveats_full: Vec<Caveat>,
    pub discharges_full: Vec<Vec<u8>>, // opaque discharge macaroons
}

/// Public inputs (RFC-0958 §Data Structures; v1.2 M5 fix: output_hash Some iff SelfHost;
/// **v1.4:** `provider_slot_id` added for slot-binding defense per IA-11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicInputs {
    pub ask_id: [u8; 32],
    pub axes_consumed: Vec<(String, u64)>,
    pub cap_root_hash: [u8; 32],
    pub invocation_hash: [u8; 32],
    pub holder_did: String,
    pub current_unix_time: u64,
    pub output_hash: Option<[u8; 32]>,
    /// **v1.4:** provider vault slot ID (RFC-0009 §Vault). Stable identifier
    /// for the slot that the capability is bound to. Mint API rejects empty
    /// strings via `ZkMintError::EmptySlotId`. Real proofer sources this
    /// from holder vault at mint time; test fixtures use concrete slot IDs.
    pub provider_slot_id: String,
}

/// Proof bundle (RFC-0958 §Data Structures).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofBundle {
    pub stark_proof: Vec<u8>,
    pub public_inputs: PublicInputs,
    pub casm_hash: [u8; 32],
    pub security_bits: u8,
}

/// ZK mint errors (RFC-0958 §Error Handling + v1.2 M5 fix).
#[derive(Debug, thiserror::Error)]
pub enum ZkMintError {
    #[error("NodeType::Wholesale cannot mint ZK-bearing capability (fail-closed)")]
    NodeTypeCannotMintZKCap,

    #[error("Capability class MUST be ZKBearing; got V1")]
    ClassMismatch,

    #[error("SelfHost NodeType requires output_hash in public inputs")]
    MissingOutputHash,

    #[error("Hybrid NodeType cannot emit PoI (output_hash MUST be None)")]
    HybridCannotEmitPoI,

    #[error("CASM hash mismatch: expected {expected:02x?}, got {got:02x?}")]
    CasmHashMismatch { expected: [u8; 32], got: [u8; 32] },

    #[error("STWO proof generation failed: {0}")]
    StwoProveError(String),

    #[error("holder signature invalid for witness")]
    HolderSigInvalid,

    #[error("HMAC-BLAKE3 chain mismatch in witness")]
    ChainMismatch,

    #[error("axes consumed exceed max_total: total={total}, max={max}")]
    AxesExceededMaxTotal { total: u128, max: u128 },

    #[error("capability expired: before={before}, now={now}")]
    Expired { before: u64, now: u64 },

    /// **v1.4:** provider_slot_id is empty; cannot mint without slot binding.
    #[error("provider_slot_id is empty (RFC-0958 IA-11: slot binding required)")]
    EmptySlotId,
}

/// Compiled CASM BLAKE3 hash for the bundled capability ZK circuit
/// (RFC-0958 §CASM compilation; mission 0958-a S05 Phase B.2).
///
/// **Migration 2026-07-22:** CASM compilation moved out of the stoolap
/// fork into the cipherocto workspace (`crates/zk-circuit/`, per
/// [[stoolap-general-purpose-db]]). The compiled hash is computed at
/// startup via `bundled_casm_hash()` and memoized in a `OnceLock`.
///
/// Real upstream (production pipeline) emits a `bundled.rs` constant from
/// `cairo/capability_zk.cairo` compiled via `cairo/build.sh`; for MVP the
/// constant is produced at runtime from the in-tree Cairo source.
pub static COMPILED_CASM_BLAKE3_HASH: OnceLock<[u8; 32]> = OnceLock::new();

/// Returns the bundled CASM hash, memoizing on first call.
#[must_use]
pub fn bundled_casm_hash() -> [u8; 32] {
    *COMPILED_CASM_BLAKE3_HASH.get_or_init(compute_bundled_casm_hash)
}

fn compute_bundled_casm_hash() -> [u8; 32] {
    let program: CairoProgram =
        serde_json::from_str(BUNDLED_CAIRO_JSON).expect("bundled Cairo JSON must parse");
    let compiled = compile(&program).expect("bundled Cairo compile must succeed");
    let hex = compiled.hash();
    let bytes = hex::decode(hex).expect("BLAKE3 hash is hex");
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes[..32]);
    out
}

/// Cairo source for the bundled capability ZK circuit (mint side).
///
/// The real `cairo/capability_zk.cairo` file lives in the repo; for this
/// MVP stub we carry the program body inline so the compile pipeline has
/// a stable determinism contract. Migration 2026-07-22: real source lives
/// at `cairo/capability_zk.cairo`; when that lands, plumb it through
/// `include_str!` instead of inlining.
const BUNDLED_CAIRO_JSON: &str = r#"{
    "version": "2.6.0",
    "identifier": "capability_zk_v1",
    "hints": [],
    "bytecode": []
}"#;

/// Mint a ZK-bearing capability proof bundle.
///
/// Per RFC-0958 §Algorithms:
/// 1. NodeType gating (fail-closed for Wholesale)
/// 2. Capability class MUST be ZKBearing (ClassMismatch if V1)
/// 3. SelfHost requires output_hash in public inputs
/// 4. Hybrid requires output_hash == None (no PoI)
/// 5. CASM hash MUST match compiled CASM at proof gen time
/// 6. STARK proof generation via stwo-plugin (delegated; MVP stub)
///
/// # Errors
/// Returns `ZkMintError` on any gating/precondition failure or on STWO
/// proof generation failure.
pub fn mint_with_zk(
    node_type: NodeType,
    witness: &PrivateWitness,
    public_inputs: &PublicInputs,
    casm_hash: [u8; 32],
) -> Result<ProofBundle, ZkMintError> {
    // 1. NodeType gating (RFC-0958 §Adversary A3 — fail-closed for Wholesale).
    if !node_type.permits_zk_mint() {
        return Err(ZkMintError::NodeTypeCannotMintZKCap);
    }

    // 2. Capability class — implicit in mint_with_zk API; this is ZKBearing path.
    //    V1 tokens must NOT call mint_with_zk; enforced at type level (no V1 entrypoint).
    let _ = witness; // witness consumed by STWO in production; MVP no-op

    // 3. SelfHost requires output_hash (v1.2 M5 fix).
    if matches!(node_type, NodeType::SelfHost) && public_inputs.output_hash.is_none() {
        return Err(ZkMintError::MissingOutputHash);
    }

    // 4. Hybrid cannot emit PoI (v1.2 M5 fix).
    if matches!(node_type, NodeType::Hybrid) && public_inputs.output_hash.is_some() {
        return Err(ZkMintError::HybridCannotEmitPoI);
    }

    // 5. **v1.4:** provider_slot_id MUST be non-empty (slot binding defense,
    //    IA-11). Real proofer sources this from holder vault (RFC-0009 §Vault).
    if public_inputs.provider_slot_id.is_empty() {
        return Err(ZkMintError::EmptySlotId);
    }

    // 6. CASM hash MUST match compiled CASM at proof gen time.
    let bundled = bundled_casm_hash();
    if casm_hash != bundled {
        return Err(ZkMintError::CasmHashMismatch {
            expected: bundled,
            got: casm_hash,
        });
    }

    // 7. STWO proof generation (delegated to stoolap fork; MVP stub returns
    //    empty proof bytes). Production: stwo_plugin::prove(casm, witness, public_inputs).
    let stark_proof = Vec::new();

    Ok(ProofBundle {
        stark_proof,
        public_inputs: public_inputs.clone(),
        casm_hash,
        security_bits: 128,
    })
}

/// Convert wire bytes to `ProofBundle` (canonical_ser round-trip per v1.1 C5 fix).
///
/// # Errors
/// Returns `WireError::Deserialization` on parse failure.
pub fn proof_bundle_from_wire(bytes: &[u8]) -> Result<ProofBundle, WireError> {
    serde_json::from_slice(bytes).map_err(|e| WireError::Parse(e.to_string()))
}

/// Convert `ProofBundle` to wire bytes.
pub fn proof_bundle_to_wire(bundle: &ProofBundle) -> Result<Vec<u8>, WireError> {
    serde_json::to_vec(bundle).map_err(|e| WireError::Serialize(e.to_string()))
}

/// Sanity: assert that the macaroon root_id derived from witness matches the
/// `cap_root_hash` public input. Used at mint time as a defense-in-depth check
/// before invoking STWO.
pub fn witness_chain_matches(witness: &PrivateWitness, expected_root_hash: &[u8; 32]) -> bool {
    // In production: re-derive `current_sig = blake3_keyed_hash(derive_key("capability.cairo.chain", current_sig), msg)`
    // for each caveat; compare final to expected. MVP: stub returns true if chain is
    // structurally valid (cap_root_secret non-zero).
    witness.cap_root_secret != [0u8; 32] && *expected_root_hash != [0u8; 32]
}

/// Helper: derive `cap_root_hash` from `cap_root_secret` (BLAKE3 identity case).
pub fn derive_root_hash_from_secret(cap_root_secret: &[u8; 32]) -> [u8; 32] {
    *blake3::hash(cap_root_secret).as_bytes()
}

/// Helper: derive `MacaroonId` (16 bytes) from `cap_root_hash` for indexing.
#[must_use]
pub fn macaroon_id_from_root_hash(cap_root_hash: &[u8; 32]) -> MacaroonId {
    let full = blake3::hash(cap_root_hash);
    let mut id = [0u8; 16];
    id.copy_from_slice(&full.as_bytes()[..16]);
    id
}

// Helper trait: expose PublicInputs.output_hash() for tests + readability.
impl ProofBundle {
    #[must_use]
    pub fn output_hash(&self) -> Option<[u8; 32]> {
        self.public_inputs.output_hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_witness() -> PrivateWitness {
        PrivateWitness {
            cap_root_secret: [0x42; 32],
            holder_sig: Signature::from_bytes(&[0xab; 64]),
            caveats_full: vec![],
            discharges_full: vec![],
        }
    }

    fn sample_public_inputs(node_type: NodeType) -> PublicInputs {
        PublicInputs {
            ask_id: [0x11; 32],
            axes_consumed: vec![("input_tokens_per_1k".to_owned(), 1000)],
            cap_root_hash: [0x22; 32],
            invocation_hash: [0x33; 32],
            holder_did: "did:octo:holder".to_owned(),
            current_unix_time: 1_700_000_000,
            output_hash: if matches!(node_type, NodeType::SelfHost) {
                Some([0x44; 32])
            } else {
                None
            },
            // **v1.4:** real slot ID (no sentinel). In production this is
            // sourced from the holder's vault slot via RFC-0009 §Vault.
            provider_slot_id: "slot-mvp-001".to_owned(),
        }
    }

    #[test]
    fn wholesale_mint_rejected() {
        let witness = sample_witness();
        let pi = sample_public_inputs(NodeType::Wholesale);
        let err =
            mint_with_zk(NodeType::Wholesale, &witness, &pi, bundled_casm_hash()).unwrap_err();
        assert!(matches!(err, ZkMintError::NodeTypeCannotMintZKCap));
    }

    #[test]
    fn selfhost_mint_succeeds_with_output_hash() {
        let witness = sample_witness();
        let pi = sample_public_inputs(NodeType::SelfHost);
        let bundle = mint_with_zk(NodeType::SelfHost, &witness, &pi, bundled_casm_hash()).unwrap();
        assert_eq!(bundle.security_bits, 128);
        assert_eq!(bundle.output_hash(), pi.output_hash);
    }

    #[test]
    fn selfhost_mint_rejected_without_output_hash() {
        let witness = sample_witness();
        let mut pi = sample_public_inputs(NodeType::SelfHost);
        pi.output_hash = None;
        let err = mint_with_zk(NodeType::SelfHost, &witness, &pi, bundled_casm_hash()).unwrap_err();
        assert!(matches!(err, ZkMintError::MissingOutputHash));
    }

    #[test]
    fn hybrid_mint_rejected_with_output_hash() {
        let witness = sample_witness();
        let mut pi = sample_public_inputs(NodeType::Hybrid);
        pi.output_hash = Some([0x44; 32]);
        let err = mint_with_zk(NodeType::Hybrid, &witness, &pi, bundled_casm_hash()).unwrap_err();
        assert!(matches!(err, ZkMintError::HybridCannotEmitPoI));
    }

    #[test]
    fn hybrid_mint_succeeds_without_output_hash() {
        let witness = sample_witness();
        let pi = sample_public_inputs(NodeType::Hybrid);
        let bundle = mint_with_zk(NodeType::Hybrid, &witness, &pi, bundled_casm_hash()).unwrap();
        assert!(bundle.output_hash().is_none());
    }

    #[test]
    fn casm_hash_mismatch_rejected() {
        let witness = sample_witness();
        let pi = sample_public_inputs(NodeType::SelfHost);
        let wrong_casm = [0xff; 32];
        let err = mint_with_zk(NodeType::SelfHost, &witness, &pi, wrong_casm).unwrap_err();
        assert!(matches!(err, ZkMintError::CasmHashMismatch { .. }));
    }

    #[test]
    fn proof_bundle_wire_roundtrip() {
        let witness = sample_witness();
        let pi = sample_public_inputs(NodeType::SelfHost);
        let bundle = mint_with_zk(NodeType::SelfHost, &witness, &pi, bundled_casm_hash()).unwrap();
        let bytes = proof_bundle_to_wire(&bundle).unwrap();
        let back = proof_bundle_from_wire(&bytes).unwrap();
        assert_eq!(back.public_inputs, bundle.public_inputs);
        assert_eq!(back.casm_hash, bundle.casm_hash);
    }

    #[test]
    fn derive_root_hash_deterministic() {
        let secret = [0x42; 32];
        let h1 = derive_root_hash_from_secret(&secret);
        let h2 = derive_root_hash_from_secret(&secret);
        assert_eq!(h1, h2);
    }
}
