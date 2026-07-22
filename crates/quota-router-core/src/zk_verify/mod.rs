//! ZK capability verify module (S05 — RFC-0958).
//!
//! Verifies STARK proofs over capability tokens. Per RFC-0958 §Algorithms:
//! - `verify_capability_zk(proof, expected_public_inputs) → Result<(), ZkVerifyError>`
//! - Public inputs: (ask_id, axes_consumed, cap_root_hash, invocation_hash,
//!   holder_did, current_unix_time, output_hash: Option<[u8; 32]>)
//! - CASM hash verified at verify time (drift detection).
//! - STWO verify is constant-time.
//!
//! For S05 MVP: defines the API + error types + gating logic. The actual
//! STWO + Cairo integration lives in the stoolap fork (`feat/blockchain-sql`)
//! per master plan §0 cross-repo coordination. The `verify_capability_zk`
//! function here is a stub that delegates to a registered verifier.

pub mod capability;

use serde::{Deserialize, Serialize};

/// ZK capability class (RFC-0958 §Data Structures).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityClass {
    /// RFC-0957 v1 macaroon only; no STARK proof.
    V1,
    /// RFC-0958 ZK subclass; `proof_bundle` MUST be Some.
    ZKBearing,
}

/// Public inputs to the ZK circuit (RFC-0958 §Data Structures).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicInputs {
    pub ask_id: [u8; 32],
    pub axes_consumed: Vec<(String, u64)>,
    pub cap_root_hash: [u8; 32],
    pub invocation_hash: [u8; 32],
    pub holder_did: String,
    pub current_unix_time: u64,
    /// Self-host mode only; None for Wholesale / Hybrid.
    pub output_hash: Option<[u8; 32]>,
}

/// Proof bundle (RFC-0958 §Data Structures).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofBundle {
    pub stark_proof: Vec<u8>,
    pub public_inputs: PublicInputs,
    pub casm_hash: [u8; 32],
    pub security_bits: u8,
}

/// ZK verification error (RFC-0958 §Error Handling).
#[derive(Debug, thiserror::Error)]
pub enum ZkVerifyError {
    #[error("public input mismatch: {0}")]
    PublicInputMismatch(String),

    #[error("CASM hash mismatch at verify time: expected {expected:02x?}, got {got:02x?}")]
    CasmHashMismatch { expected: [u8; 32], got: [u8; 32] },

    #[error("STWO verify failed: {0}")]
    StwoVerifyError(String),

    #[error("proof bundle missing for ZKBearing capability class")]
    ProofBundleMissing,

    #[error("unsupported Cairo version: proof={proof_version}, verifier={verifier_version}")]
    UnsupportedCairoVersion {
        proof_version: String,
        verifier_version: String,
    },
}

/// ZK mint error (RFC-0958 §Error Handling).
#[derive(Debug, thiserror::Error)]
pub enum ZkMintError {
    #[error("NodeType::Wholesale cannot mint ZK-bearing capability (fail-closed)")]
    NodeTypeCannotMintZKCap,

    #[error("capability class MUST be ZKBearing; got V1")]
    ClassMismatch,

    #[error("SelfHost NodeType requires inference_trace in witness")]
    MissingInferenceTrace,

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
}
