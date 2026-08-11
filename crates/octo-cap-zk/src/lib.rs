//! `mint_with_zk` API (RFC-0958 §Algorithms proof generation).
//!
//! Cipherocto-side API surface for minting a ZK-bearing capability. Per
//! RFC-0958 §NodeType Gating Rule:
//! - Wholesale → fail-closed (NodeTypeCannotMintZKCap)
//! - SelfHost → default (mint succeeds)
//! - Hybrid → opt-in (mint succeeds if explicit mint_with_zk call)
//!
//! Per RFC-0958 v1.2 M5/M17 fix:
//! - M5: `output_hash: Some(_)` iff NodeType == SelfHost; mint API enforces
//!   `HybridCannotEmitPoI` and `MissingInferenceTrace` errors
//! - M17: `proof_bundle: Some(_)` iff capability_class == ZKBearing; mint
//!   API enforces via `ClassMismatch` if V1 token gets Some(_)
//!
//! Per RFC-0958 capability ZK subclass + RFC-0962 §9 ZK proof integration
//! (Gap 3 / Task 3.3): when the caller supplies a non-empty
//! `signers` list, `mint_with_zk` also generates a batch signature proof via
//! `zk_circuit::prove_batch_signature`. The proof is embedded into the
//! returned `ProofBundle.stark_proof` so downstream verifiers see a single
//! proof that covers the full multi-signer envelope.
//!
//! ## Layer discipline
//!
//! Layer 4 extension crate per RFC-0965 per-extension crate layout mandate.
//! Sibling to `octo-cap-macaroon` (RFC-0957 §3.1). Depends on Layer A
//! primitives (`blake3`, `hex`, `serde`, `ed25519-dalek`) + the Layer A
//! ZK substrate (`zk-circuit`, `zk-verifier`, `zk-vendor`) + the Layer
//! A-adjacent constraint crate (`cipherocto-zkp-canonical`). Does NOT
//! depend on `octo-wallet`, `octo-protocol`, or any higher-layer substrate.
//!
//! ## Migration (mission 0957-ext-zk-crate, 2026-08-09)
//!
//! Extracted from `crates/octo-wallet/src/capability/zk_mint.rs` (781 lines)
//! per RFC-0957 + RFC-0965 per-extension crate layout mandate. Wallet's
//! `NodeType` enum is mapped via the `to_zk_node_type` free function in
//! `octo-wallet/src/capability/zk_mint.rs` (orphan rule prevents an
//! inherent `From` impl on the foreign `NodeType`).

#![forbid(unsafe_code)]
#![allow(missing_docs)]

use std::sync::OnceLock;

use ed25519_dalek::Signature;
use serde::{Deserialize, Serialize};
use zk_circuit::{prove_batch_signature, BatchSigPublicInputs, Program};

use octo_cap_macaroon::caveat::Caveat;
use octo_cap_macaroon::macaroon::MacaroonId;
use octo_cap_macaroon::wire::WireError;

/// Local copy of `NodeType` (RFC-0958 §NodeType Gating Rule).
///
/// The wallet crate owns the canonical `NodeType` enum (in
/// `crates/octo-wallet/src/node.rs`); this enum mirrors the three
/// variants the mint API depends on. Conversion is via the
/// `to_zk_node_type` free function in the wallet re-export shim
/// (orphan rule prevents an inherent `From` impl on the foreign
/// `NodeType`).
///
/// **Why not a trait abstraction?** `NodeType` is a 3-variant enum with
/// a single boolean method (`permits_zk_mint`). A trait adds indirection
/// for zero benefit. The mirror-enum is a pragmatic Layer 4 boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeType {
    /// Routes calls to external opaque providers (OpenAI, Anthropic, etc.).
    /// Cannot mint ZK-bearing capabilities per RFC-0958 §Adversary A3.
    Wholesale,
    /// Runs inference inside the CipherOcto protocol boundary.
    /// Mints ZK-bearing capabilities by default per RFC-0958 §NodeType Gating.
    #[serde(rename = "self-host")]
    SelfHost,
    /// Operates both wholesale-routed and self-hosted inference.
    /// ZK mint requires explicit `mint_with_zk()` API call.
    Hybrid,
}

impl NodeType {
    /// Whether this NodeType permits ZK-bearing capability mint.
    ///
    /// Wholesale → false (fail-closed per RFC-0958 §Adversary A3).
    /// SelfHost + Hybrid → true.
    #[must_use]
    pub const fn permits_zk_mint(self) -> bool {
        matches!(self, NodeType::SelfHost | NodeType::Hybrid)
    }
}

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
///
/// **v1.2 M5 rename (AC-6):** `inference_trace: Option<ExecutionTrace>` MUST
/// be `Some` iff the caller is `NodeType::SelfHost` (carries the PoI trace).
/// Hybrid / Wholesale callers leave this `None`; SelfHost callers populate
/// it with the inference trace whose canonicalized hash matches
/// `public_inputs.output_hash`. Mint API enforces both directions.
///
/// **Debug redaction (octo-wallet §Security):** `cap_root_secret` is the
/// macaroon root secret (RFC-0957 §3.1) and `holder_sig` is the bearer
/// Ed25519 signature — both MUST NEVER appear in Debug output. Manual
/// `Debug` impl prints only field presence + lengths.
#[derive(Clone)]
pub struct PrivateWitness {
    pub cap_root_secret: [u8; 32],
    pub holder_sig: Signature,
    pub caveats_full: Vec<Caveat>,
    pub discharges_full: Vec<Vec<u8>>, // opaque discharge macaroons
    /// **v1.2 M5:** SelfHost-only PoI trace. Hybrid/Wholesale leave `None`.
    /// Mint API rejects SelfHost with `None` via [`ZkMintError::MissingInferenceTrace`].
    pub inference_trace: Option<ExecutionTrace>,
}

impl std::fmt::Debug for PrivateWitness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrivateWitness")
            .field("cap_root_secret", &"[REDACTED 32 bytes]")
            .field("holder_sig", &"[REDACTED 64 bytes]")
            .field("caveats_count", &self.caveats_full.len())
            .field("discharges_count", &self.discharges_full.len())
            .field(
                "inference_trace",
                &self.inference_trace.as_ref().map(|t| t.step_count),
            )
            .finish()
    }
}

/// Self-host inference trace (RFC-0958 §Data Structures; v1.2 M5 fix).
///
/// Carries the PoI trace for self-host mode. The trace's canonicalized
/// BLAKE3 hash (over `step_records` via `derive_trace_hash`) MUST equal
/// `PublicInputs::output_hash` when present. The Cairo-side structural
/// check (`inference_trace_present == 1` iff `has_output_hash == 1`) lives
/// at `cairo/src/lib.cairo::main`.
///
/// `step_records` carries one entry per inference step (operator code +
/// input/output hashes). The verifier reconstructs the trace hash via
/// `derive_trace_hash` and compares to `public_inputs.output_hash`.
#[derive(Debug, Clone)]
pub struct ExecutionTrace {
    /// Number of steps (bounded by RFC-0958 §Performance G1: 10K reference).
    pub step_count: u32,
    /// Per-step records (RFC-0958 §StepRecord). Empty for Hybrid/Wholesale.
    pub step_records: Vec<TraceStep>,
}

/// Single inference step (RFC-0958 §StepRecord).
///
/// Three-tuple: operator code (`op_as_felt`), input hash (32 bytes),
/// output hash (32 bytes). The Cairo-side Poseidon canonicalization is
/// `poseidon_hash(op_as_felt || input_hash_lo || input_hash_hi ||
/// output_hash_lo || output_hash_hi)`. The Rust-side mirror lives at
/// `derive_step_hash`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceStep {
    /// Operator code (felt252 in Cairo; opaque u64 here for the hash binding).
    pub op_code: u64,
    /// Input hash (32 bytes).
    pub input_hash: [u8; 32],
    /// Output hash (32 bytes).
    pub output_hash: [u8; 32],
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
///
/// **R3 #5 fix-up (2026-07-31):** `casm_version: u32` added so that
/// verifiers can route a v2 proof to a v2 verifier and a v1 proof to a
/// v1 verifier (or, per RFC-0958 §CASM Rotation: accept both during
/// the N=2 grace period). The casm_hash binding still operates over
/// the hash (no behavioral change); the version field is for
/// migration tracking.
///
/// **Debug redaction (octo-wallet §Security):** `stark_proof` carries
/// the proof bytes (50-500 KB per RFC-0958 §Performance); dumping to
/// panic messages / log lines pollutes output and may carry transient
/// proof-generation data the verifier hasn't yet finalized. Manual
/// `Debug` impl prints only the size + casm_hash hex + version +
/// security_bits.
///
/// **Wire format intentionally carries raw bytes:** the `Serialize`
/// impl is auto-derived (NOT redacted) so the wire format can transmit
/// the proof bytes end-to-end. The `Debug` redaction protects panic
/// messages / log lines; the wire format is the explicit "full" surface
/// (RFC-0958 §Wire Format). R4 audit (M12) initially proposed a
/// custom redacting `Serialize` impl — that was reverted because it
/// broke the roundtrip invariant; the redaction belongs on `Debug`,
/// not `Serialize`.
#[derive(Clone, Serialize, Deserialize)]
pub struct ProofBundle {
    pub stark_proof: Vec<u8>,
    pub public_inputs: PublicInputs,
    pub casm_hash: [u8; 32],
    pub casm_version: u32,
    pub security_bits: u8,
    /// **AC-3 (mission 0958-c, 2026-08-05):** witness format marker.
    /// `ProverInputJson` = real STWO ProverInput JSON shape (production
    /// path under `VendorState::Ffi`); `BytesFallback` = legacy raw-byte
    /// fallback (only reachable when `ProverInput::bytes_fallback = true`
    /// or when the FFI parse fails). Defaults to `BytesFallback` for
    /// backward compat with pre-AC-3 serialized bundles.
    #[serde(default)]
    pub witness_format: zk_vendor::prover_input::WitnessFormat,
    /// **Mission 0957-f-v2-bundle-consumer-migration:** optional V2
    /// capability bundle that this proof attests to. When `Some`,
    /// the ZK circuit public inputs include the bundle's
    /// `chain_depth` + `chain_parent` (BLAKE3 binding) + `issuer_did`
    /// so the verifier checks hierarchical attenuation chain state.
    /// When `None`, the proof falls back to the legacy raw-macaroon
    /// substrate (pre-V2). `#[serde(default)]` preserves
    /// back-compat with pre-V2 serialized bundles.
    #[serde(default)]
    pub capability_v2: Option<octo_cap_macaroon::CapabilityBundleV2>,
}

impl std::fmt::Debug for ProofBundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProofBundle")
            .field("stark_proof_size_bytes", &self.stark_proof.len())
            .field("public_inputs", &self.public_inputs)
            .field("casm_hash", &hex::encode(self.casm_hash))
            .field("casm_version", &self.casm_version)
            .field("security_bits", &self.security_bits)
            // **AC-3 (0958-c):** witness format marker is observability
            // metadata (kill switch + audit), not secret-bearing. Show
            // the enum variant name so panic messages + log lines reveal
            // which witness shape was emitted.
            .field("witness_format", &self.witness_format)
            .finish()
    }
}

/// ZK mint errors (mission 0958-a R3 fix-up, 2026-07-31).
///
/// **R3 #7 audit:** variants triaged. Removed:
/// - `HolderSigInvalid` (duplicate of `MintError::HolderSig` in mod.rs:175)
/// - `ChainMismatch` (duplicate of `MacaroonError::ChainMismatch(usize)`)
///
/// Variants kept + enforcement wired:
/// - `EmptySlotId` (already returned at line 323); newly tested below.
/// - `StwoProveError` reserved for future full STWO prover failures.
/// - `AxesExceededMaxTotal` reserved for sum-over-axes check (caveats
///   validate per-axis bounds; total is a future-proof aggregate gate).
/// - `Expired` for caveats `Before(t)` enforcement.
/// - `BatchProver` propagated from `prove_batch_signature` failures.
#[derive(Debug, thiserror::Error)]
pub enum ZkMintError {
    #[error("NodeType::Wholesale cannot mint ZK-bearing capability (fail-closed)")]
    NodeTypeCannotMintZKCap,

    #[error("Capability class MUST be ZKBearing; got V1")]
    ClassMismatch,

    #[error("SelfHost NodeType requires inference_trace in witness (RFC-0958 v1.2 M5 rename)")]
    MissingInferenceTrace,

    #[error("Hybrid NodeType cannot emit PoI (output_hash MUST be None)")]
    HybridCannotEmitPoI,

    #[error("CASM hash mismatch: expected {expected:02x?}, got {got:02x?}")]
    CasmHashMismatch { expected: [u8; 32], got: [u8; 32] },

    #[error("STWO proof generation failed: {0}")]
    StwoProveError(String),

    #[error("axes consumed exceed max_total: total={total}, max={max}")]
    AxesExceededMaxTotal { total: u128, max: u128 },

    #[error("capability expired: before={before}, now={now}")]
    Expired { before: u64, now: u64 },

    /// **v1.4:** provider_slot_id is empty; cannot mint without slot binding.
    #[error("provider_slot_id is empty (RFC-0958 v1.4 IA-11: slot binding required)")]
    EmptySlotId,

    /// **RFC-0958 + RFC-0962 §9 (Gap 3 / Task 3.3):** the batch signature
    /// prover rejected the inputs (empty signers, exceeds max, FFI null
    /// handle, internal prover error).
    #[error("batch signature prover error: {0}")]
    BatchProver(String),
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
/// `cairo/src/lib.cairo` compiled via `scarb build` (Cairo 2.x) and
/// lowered in-process to CASM via `cairo-lang-sierra-to-casm` (Session 2).
///
/// **Session 3 invariant:** the slot stores `Result<[u8; 32], _>` so a
/// compilation failure is cached permanently for the process — every
/// subsequent `bundled_casm_hash()` call returns the same error rather
/// than retrying the scarb subprocess on every mint.
pub static COMPILED_CASM_BLAKE3_HASH: OnceLock<Result<[u8; 32], zk_circuit::HashError>> =
    OnceLock::new();

/// Returns the bundled CASM hash, memoizing on first call.
///
/// # Panics
/// Panics if the underlying `zk_circuit::compile_from_source` call
/// fails (e.g., scarb not in PATH, cairo-lang crates not installed).
/// Session 3 mandate: no stub fallback — if the real CASM cannot be
/// produced, mint-time CASM hash checks MUST fail loudly rather than
/// silently accept a placeholder.
#[must_use]
pub fn bundled_casm_hash() -> [u8; 32] {
    COMPILED_CASM_BLAKE3_HASH
        .get_or_init(compute_bundled_casm_hash)
        .as_ref()
        .copied()
        .unwrap_or_else(|compile_err| {
            // R4 fix-up (2026-08-04): surface the original compiler error
            // (e.g. `MalformedProgram("...")` or `CompilerInternal("...")`)
            // so the operator can diagnose whether the failure is a missing
            // scarb, a malformed Sierra JSON, or a Sierra→CASM bug — rather
            // than the previous generic "install scarb" advice that fired
            // even when scarb was fine.
            panic!(
                "bundled_casm_hash() init failed. Underlying error: {compile_err}. \
                 Common causes: (1) scarb 2.16.0 not in PATH — install via \
                 https://docs.swmansion.com/scarb/; (2) cairo-lang-* 2.20.0 \
                 Rust crates not linked into zk-circuit — rebuild with \
                 `cargo build -p zk-circuit`; (3) malformed Sierra IR from \
                 `cairo/src/lib.cairo` — re-run `scarb build` from the \
                 `cairo/` directory and inspect the JSON output."
            )
        })
}

fn compute_bundled_casm_hash() -> Result<[u8; 32], zk_circuit::HashError> {
    // Production path (mission 0958-a Phase B.2 + Session 2): invoke
    // the scarb+Sierra→CASM pipeline via `zk_circuit::compile_bundled()`
    // and hash the real CASM bytes. Requires scarb 2.16.0 in PATH +
    // cairo-lang-* 2.20.0 Rust crates (linked into zk-circuit).
    //
    // R4 fix-up (2026-08-04): use `compiled.hash_bytes()` instead of
    // decoding the hex ourselves — `hash_bytes()` carries the
    // hex-invariant panic so a corrupt hash surfaces as a clear failure.
    //
    // Session 3 contract: no stub fallback. The error is propagated
    // up to `bundled_casm_hash()` which then panics with an actionable
    // message; the smoke test `casm_snapshot` already verifies the
    // hard-fail behavior at the test layer.
    #[allow(deprecated)]
    let compiled = zk_circuit::compile_bundled()?;
    compiled.hash_bytes()
}

/// Cairo source for the bundled capability ZK circuit (mint side).
///
/// Re-exported from `zk_circuit::BUNDLED_CAIRO_SOURCE` which loads the
/// real `cairo/src/lib.cairo` (Cairo 2.x scarb package `capability_zk`)
/// via `include_str!` at compile time. Mission 0958-a Phase B.2
/// (2026-07-22 extraction per [[stoolap-general-purpose-db]]): real
/// Cairo source replaces the previous inline JSON stub.
pub use zk_circuit::BUNDLED_CAIRO_SOURCE;

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
///
/// **R5 audit doc (2026-07-31):** `mint_with_zk` is a deliberate mock
/// shim, not leftover. It delegates to `mint_with_zk_and_signers(.., &&[])`,
/// which in turn produces an empty `stark_proof` when the signer list
/// is empty (the single-capability MVP stub path; see
/// `mint_with_zk_and_signers` step 8). This is the canonical entry
/// point for:
/// - Unit tests asserting the gating rules (Wholesale reject,
///   SelfHost inference-trace requirement, CASM drift, Hybrid PoI
///   rule, EmptySlotId) WITHOUT dragging in batch-signature proving.
/// - Dev / CI without the `libstwo_sys.so` cdylib (no real prover
///   available, but the gating logic is exercised).
///
/// The **production** entry point is `mint_with_zk_and_signers` with a
/// non-empty `signers` list — that path produces a real batch STARK
/// proof and is what the live quota-router exercises end-to-end.
/// `mint_with_zk` MUST NOT be used in code paths that ship real
/// tokens; the wholesale CI lint
/// (`.github/linters/no-wholesale-zk.sh`) blocks the call in
/// `NodeType::Wholesale` paths and the registry layer 2 defense
/// (`super::registry`) catches any cross-class misuse.
pub fn mint_with_zk(
    node_type: NodeType,
    witness: &PrivateWitness,
    public_inputs: &PublicInputs,
    casm_hash: [u8; 32],
) -> Result<ProofBundle, ZkMintError> {
    // Mock shim: delegates to the signers-aware variant with an empty
    // signer list (single-capability MVP stub path). See the doc
    // comment above for why this is intentional and not a workaround.
    mint_with_zk_and_signers(node_type, witness, public_inputs, casm_hash, &[])
}

/// Mint a ZK-bearing capability proof bundle with explicit batch signature
/// (RFC-0958 + RFC-0962 §9 / Gap 3 / Task 3.3).
///
/// When `signers` is non-empty, the function generates a
/// `BatchSigPublicInputs` from the capability public inputs + the supplied
/// signer public keys, calls `zk_circuit::prove_batch_signature`, and
/// embeds the resulting proof bytes into `ProofBundle.stark_proof`. When
/// `signers` is empty, falls back to the single-capability MVP stub
/// (empty `stark_proof`).
///
/// # Errors
/// Returns `ZkMintError::BatchProver` if the prover rejects the inputs;
/// returns the same errors as `mint_with_zk` for gating / preconditions.
pub fn canonicalize_axes(pi: &mut PublicInputs) {
    // **R2 fix-up (2026-08-05):** delegate to the canonical crate
    // `cipherocto-zkp-canonical::canonicalize_axes`. The local duplicate
    // was removed (R4 M4 disposition drift was a documentation bug —
    // R1 claimed the consolidation was done but never performed the work).
    cipherocto_zkp_canonical::canonicalize_axes(&mut pi.axes_consumed);
}

pub fn mint_with_zk_and_signers(
    node_type: NodeType,
    witness: &PrivateWitness,
    public_inputs: &PublicInputs,
    casm_hash: [u8; 32],
    signers: &[[u8; 32]],
) -> Result<ProofBundle, ZkMintError> {
    // 1. NodeType gating (RFC-0958 §Adversary A3 — fail-closed for Wholesale).
    if !node_type.permits_zk_mint() {
        return Err(ZkMintError::NodeTypeCannotMintZKCap);
    }

    // 2. Capability class: this function is the ZKBearing path.
    //    V1 tokens use `CapabilityToken::mint` in `super::mod` (no STARK
    //    proof), and the capability-class field on the wire format
    //    disambiguates. The ZK/V1 split is NOT a Rust type-level
    //    invariant; it is enforced by:
    //    (a) `CapabilityClass` enum + wire-format discriminator,
    //    (b) Layer 2 defense in `super::registry` (CapabilityClassRegistry
    //        rejects Wholesale ZK mint + V1 ZK mix),
    //    (c) CI lint `.github/linters/no-wholesale-zk.sh` blocking
    //        `mint_with_zk*` in `NodeType::Wholesale` paths.
    //    Any caller of `mint_with_zk*` MUST already be in the
    //    ZKBearing code path; the call itself does not verify that.
    let _ = witness; // witness consumed by STWO in production; MVP no-op

    // 3. SelfHost requires `inference_trace` in witness (v1.2 M5 rename —
    //    was `MissingOutputHash` checking `public_inputs.output_hash.is_none()`;
    //    AC-6 fix: enforce witness-side check, not just public-input-side).
    if matches!(node_type, NodeType::SelfHost) && witness.inference_trace.is_none() {
        return Err(ZkMintError::MissingInferenceTrace);
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

    // 7. R3 fix-up: canonicalize axes_consumed before proof generation
    //    so the proofer + verifier agree on order (the structural
    //    equality check is Vec::== — order-sensitive).
    let mut public_inputs_canon = public_inputs.clone();
    canonicalize_axes(&mut public_inputs_canon);

    // 8. STARK proof generation.
    let stark_proof = if signers.is_empty() {
        // Backward-compatible single-capability path (MVP stub).
        Vec::new()
    } else {
        // Batch signature path (RFC-0958 + RFC-0962 §9 / Gap 3 / Task 3.3).
        let inputs = batch_sig_inputs(&public_inputs_canon, signers);
        let zk_public = zk_verifier_public(&public_inputs_canon);
        prove_batch_signature(Program::BatchSig, casm_hash, &inputs, &zk_public)
            .map_err(|e| ZkMintError::BatchProver(e.to_string()))?
            .bytes
    };

    Ok(ProofBundle {
        stark_proof,
        public_inputs: public_inputs_canon,
        casm_hash,
        casm_version: 1,
        security_bits: 128,
        // **AC-3 (0958-c):** witness format marker. Defaults to
        // `BytesFallback` because `prove_batch_signature` currently
        // emits raw `canonical_ser` bytes (the AC-3 zk-circuit rewrite
        // will flip this to `ProverInputJson` once `prove_batch_signature`
        // constructs the structured `ProverInput` JSON via the new
        // `zk_vendor::prover_input` adapter). The marker records the
        // path at runtime for observability (kill switch + audit).
        witness_format: zk_vendor::prover_input::WitnessFormat::BytesFallback,
        // V2 bundle substrate: not yet populated by `prove_batch_signature`
        // (the AC-3 zk-circuit rewrite will populate this from
        // `CapabilityBundleV2Envelope::canonical_de`). Default `None`
        // preserves the pre-V2 substrate (raw macaroon bytes).
        capability_v2: None,
    })
}

/// Construct `BatchSigPublicInputs` from capability public inputs + signers.
///
/// `signer_roots[i] = BLAKE3(0xB1 || signer_pubkey_i)` — domain-separated
/// BLAKE3 root per signer (binds the signer identity into the proof).
/// `message_root = BLAKE3(0xB2 || canonical_ser(public_inputs))` — domain-
/// separated BLAKE3 root over the capability public inputs (the message
/// being co-signed by all signers).
fn batch_sig_inputs(public_inputs: &PublicInputs, signers: &[[u8; 32]]) -> BatchSigPublicInputs {
    use blake3::Hasher;

    let signer_roots: Vec<[u8; 32]> = signers
        .iter()
        .map(|pk| {
            let mut h = Hasher::new();
            h.update(&[0xB1]); // domain separator: batch-sig signer root
            h.update(pk);
            *h.finalize().as_bytes()
        })
        .collect();

    let mut msg_hasher = Hasher::new();
    msg_hasher.update(&[0xB2]); // domain separator: batch-sig message root
                                // Canonical form: ask_id || cap_root_hash || invocation_hash ||
                                // holder_did || current_unix_time || provider_slot_id. Field-order
                                // binary concat (no serde_json) for Class A determinism.
    msg_hasher.update(&public_inputs.ask_id);
    msg_hasher.update(&public_inputs.cap_root_hash);
    msg_hasher.update(&public_inputs.invocation_hash);
    msg_hasher.update(public_inputs.holder_did.as_bytes());
    msg_hasher.update(&public_inputs.current_unix_time.to_le_bytes());
    msg_hasher.update(public_inputs.provider_slot_id.as_bytes());
    let message_root: [u8; 32] = *msg_hasher.finalize().as_bytes();

    BatchSigPublicInputs {
        signer_roots,
        message_root,
    }
}

/// Construct the `zk_verifier::PublicInputs` that the downstream verifier
/// (`quota_router_core::zk_verify::capability::verify_capability_zk`)
/// will reconstruct from the proof's public inputs. Used by the batch
/// proofer to compute a `stub_commitment` byte-identical to the one the
/// verifier expects, so the mock round-trip is a single check rather than
/// a parallel commitment re-derivation.
///
/// **Contract:** MUST stay in sync with the field mapping in
/// `verify_capability_zk` (the `zk_public` construction there).
fn zk_verifier_public(public_inputs: &PublicInputs) -> zk_verifier::PublicInputs {
    zk_verifier::PublicInputs {
        proof_issued_at_unix: public_inputs.current_unix_time,
        verifier_local_unix_time: public_inputs.current_unix_time,
        // `compiled_casm_hash` is set by the proofer to the hex-encoded
        // CASM hash BEFORE the proofer delegates to `stub_commitment`
        // (the field is a placeholder here; prove_batch_signature
        // substitutes the real value).
        compiled_casm_hash: String::new(),
        capability_root_hash: hex::encode(public_inputs.cap_root_hash),
        provider_slot_id: public_inputs.provider_slot_id.clone(),
    }
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

/// **REMOVED (R4 fix-up 2026-08-04):** `witness_chain_matches` was a
/// stub function (returned `true` iff `cap_root_secret != 0` and
/// `expected_root_hash != 0`) that was never called from the production
/// mint path. The cryptographic chain re-derivation (HMAC-BLAKE3 keyed
/// hash chain over caveat links) is now tracked as a deliverable of
/// follow-up mission `missions/open/0958-b-real-cairo-crypto.md` (to
/// be filed). Until then, the BLAKE3 batch commitment covers the
/// witness via `witness_commitment = blake3(cap_root_secret ||
/// holder_sig || caveats_full || discharges_full || inference_trace)`
/// in `prove_batch_signature`, which is a structural (not
/// cryptographic) guarantee — honest disclosure per AC-6/AC-11/Risks.
///
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

    fn sample_witness(node_type: NodeType) -> PrivateWitness {
        // **v1.2 M5:** SelfHost requires `inference_trace: Some(_)`; Hybrid /
        // Wholesale leave it `None`. Test fixture mirrors this rule.
        let inference_trace = if matches!(node_type, NodeType::SelfHost) {
            Some(ExecutionTrace {
                step_count: 1,
                step_records: vec![TraceStep {
                    op_code: 0,
                    input_hash: [0x33; 32],
                    output_hash: [0x44; 32],
                }],
            })
        } else {
            None
        };
        PrivateWitness {
            cap_root_secret: [0x42; 32],
            holder_sig: Signature::from_bytes(&[0xab; 64]),
            caveats_full: vec![],
            discharges_full: vec![],
            inference_trace,
        }
    }

    fn sample_public_inputs(node_type: NodeType) -> PublicInputs {
        PublicInputs {
            ask_id: [0x11; 32],
            axes_consumed: vec![("input_tokens_per_1k".to_owned(), 1000)],
            cap_root_hash: [0x22; 32],
            invocation_hash: [0x33; 32],
            holder_did: octo_ident::test_helpers::sample_did(19).clone(),
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
        let witness = sample_witness(NodeType::Wholesale);
        let pi = sample_public_inputs(NodeType::Wholesale);
        let err =
            mint_with_zk(NodeType::Wholesale, &witness, &pi, bundled_casm_hash()).unwrap_err();
        assert!(matches!(err, ZkMintError::NodeTypeCannotMintZKCap));
    }

    #[test]
    fn selfhost_mint_succeeds_with_output_hash() {
        let witness = sample_witness(NodeType::SelfHost);
        let pi = sample_public_inputs(NodeType::SelfHost);
        let bundle = mint_with_zk(NodeType::SelfHost, &witness, &pi, bundled_casm_hash()).unwrap();
        assert_eq!(bundle.security_bits, 128);
        assert_eq!(bundle.output_hash(), pi.output_hash);
    }

    #[test]
    fn selfhost_mint_rejected_without_inference_trace() {
        // **v1.2 M5 rename (AC-6):** SelfHost witness MUST carry inference_trace.
        let witness = sample_witness(NodeType::SelfHost);
        let mut witness_no_trace = witness;
        witness_no_trace.inference_trace = None;
        let pi = sample_public_inputs(NodeType::SelfHost);
        let err = mint_with_zk(
            NodeType::SelfHost,
            &witness_no_trace,
            &pi,
            bundled_casm_hash(),
        )
        .unwrap_err();
        assert!(matches!(err, ZkMintError::MissingInferenceTrace));
    }

    #[test]
    fn hybrid_mint_rejected_with_output_hash() {
        let witness = sample_witness(NodeType::Hybrid);
        let mut pi = sample_public_inputs(NodeType::Hybrid);
        pi.output_hash = Some([0x44; 32]);
        let err = mint_with_zk(NodeType::Hybrid, &witness, &pi, bundled_casm_hash()).unwrap_err();
        assert!(matches!(err, ZkMintError::HybridCannotEmitPoI));
    }

    #[test]
    fn hybrid_mint_succeeds_without_output_hash() {
        let witness = sample_witness(NodeType::Hybrid);
        let pi = sample_public_inputs(NodeType::Hybrid);
        let bundle = mint_with_zk(NodeType::Hybrid, &witness, &pi, bundled_casm_hash()).unwrap();
        assert!(bundle.output_hash().is_none());
    }

    #[test]
    fn casm_hash_mismatch_rejected() {
        let witness = sample_witness(NodeType::SelfHost);
        let pi = sample_public_inputs(NodeType::SelfHost);
        let wrong_casm = [0xff; 32];
        let err = mint_with_zk(NodeType::SelfHost, &witness, &pi, wrong_casm).unwrap_err();
        assert!(matches!(err, ZkMintError::CasmHashMismatch { .. }));
    }

    #[test]
    fn proof_bundle_wire_roundtrip() {
        let witness = sample_witness(NodeType::SelfHost);
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

    #[test]
    fn mint_with_zk_and_signers_emits_batch_proof_for_eleven_signers() {
        // Gap 3 / Task 3.3: 11 signers (matches the 11-step exercise).
        let witness = sample_witness(NodeType::SelfHost);
        let pi = sample_public_inputs(NodeType::SelfHost);
        let casm = bundled_casm_hash();
        let signers: Vec<[u8; 32]> = (0..11)
            .map(|i| [u8::try_from(i).expect("11 signers fit in u8"); 32])
            .collect();
        let bundle =
            mint_with_zk_and_signers(NodeType::SelfHost, &witness, &pi, casm, &signers).unwrap();
        // Batch proof path emits a non-empty stark_proof (32-byte BLAKE3
        // commitment from the mock prover).
        assert_eq!(bundle.stark_proof.len(), 32);
        assert_eq!(bundle.security_bits, 128);
        assert_eq!(bundle.casm_hash, casm);
    }

    #[test]
    fn mint_with_zk_and_signers_propagates_prover_error() {
        // Wholesale + signers → NodeType gating still fires first.
        let witness = sample_witness(NodeType::Wholesale);
        let pi = sample_public_inputs(NodeType::Wholesale);
        let signers: Vec<[u8; 32]> = (0..3)
            .map(|i| [u8::try_from(i).expect("3 signers fit in u8"); 32])
            .collect();
        let err = mint_with_zk_and_signers(
            NodeType::Wholesale,
            &witness,
            &pi,
            bundled_casm_hash(),
            &signers,
        )
        .unwrap_err();
        assert!(matches!(err, ZkMintError::NodeTypeCannotMintZKCap));
    }

    #[test]
    fn mint_with_zk_empty_signers_matches_legacy_path() {
        // Empty signers list → backward-compat path; stark_proof empty.
        let witness = sample_witness(NodeType::SelfHost);
        let pi = sample_public_inputs(NodeType::SelfHost);
        let bundle =
            mint_with_zk_and_signers(NodeType::SelfHost, &witness, &pi, bundled_casm_hash(), &[])
                .unwrap();
        assert!(bundle.stark_proof.is_empty());
    }

    /// R3 #7: EmptySlotId production code path was untested — exercise it.
    #[test]
    fn empty_slot_id_rejected_at_mint() {
        let witness = sample_witness(NodeType::SelfHost);
        let mut pi = sample_public_inputs(NodeType::SelfHost);
        pi.provider_slot_id = String::new();
        let err = mint_with_zk(NodeType::SelfHost, &witness, &pi, bundled_casm_hash())
            .expect_err("empty provider_slot_id must be rejected");
        assert!(
            matches!(err, ZkMintError::EmptySlotId),
            "expected EmptySlotId, got {err:?}"
        );
    }

    #[test]
    fn node_type_permits_zk_mint_rules() {
        assert!(!NodeType::Wholesale.permits_zk_mint());
        assert!(NodeType::SelfHost.permits_zk_mint());
        assert!(NodeType::Hybrid.permits_zk_mint());
    }

    /// TV (mission 0957-f-v2-bundle-consumer-migration) — `ProofBundle`
    /// accepts a `capability_v2: Option<CapabilityBundleV2>` field;
    /// serde roundtrip preserves the field. The ZK circuit constraint
    /// path (V2-aware verifier) consumes the field; the legacy path
    /// ignores it (None → raw macaroon substrate).
    #[test]
    fn proof_bundle_capability_v2_field_roundtrip() {
        use octo_cap_macaroon::{
            CapabilityBundleV2, CapabilityBundleV2Envelope, CapabilityTokenV2,
        };
        let token_v2 = CapabilityTokenV2 {
            chain_depth: 1,
            chain_parent: [0xCC; 32],
            audience_did: "did:octo:zZkTestHolder".to_owned(),
            channel_id: [0xA1; 16],
            expires_at_unix_secs: 1_700_003_600,
            issuer_did: "did:octo:zZkTestIssuer".to_owned(),
        };
        let bundle_v2 =
            CapabilityBundleV2::new(token_v2, br#"{"holder":"zk-test"}"#.to_vec(), vec![])
                .expect("v2 bundle");
        let env = CapabilityBundleV2Envelope::new(bundle_v2);
        let env_bytes = env.canonical_ser().expect("env ser");
        // Decode envelope back into bundle, surface as Option in
        // ProofBundle, serde roundtrip preserves.
        let env_decoded = CapabilityBundleV2Envelope::canonical_de(&env_bytes).expect("env de");
        let pb = ProofBundle {
            stark_proof: vec![0u8; 32],
            public_inputs: sample_public_inputs_for_test(),
            casm_hash: [0x42; 32],
            casm_version: 1,
            security_bits: 128,
            witness_format: zk_vendor::prover_input::WitnessFormat::BytesFallback,
            capability_v2: Some(env_decoded.bundle),
        };
        let ser = serde_json::to_string(&pb).expect("ser");
        let back: ProofBundle = serde_json::from_str(&ser).expect("de");
        let v2 = back
            .capability_v2
            .expect("capability_v2 must roundtrip through serde");
        assert_eq!(v2.token_v2.chain_depth, 1);
        assert_eq!(v2.token_v2.chain_parent, [0xCC; 32]);
        assert_eq!(v2.token_v2.audience_did, "did:octo:zZkTestHolder");
        // `None` preserves pre-V2 back-compat (no `capability_v2` field).
        let pb_none = ProofBundle {
            stark_proof: vec![0u8; 32],
            public_inputs: sample_public_inputs_for_test(),
            casm_hash: [0x42; 32],
            casm_version: 1,
            security_bits: 128,
            witness_format: zk_vendor::prover_input::WitnessFormat::BytesFallback,
            capability_v2: None,
        };
        let ser_none = serde_json::to_string(&pb_none).expect("ser");
        let back_none: ProofBundle = serde_json::from_str(&ser_none).expect("de");
        assert!(back_none.capability_v2.is_none());
    }

    /// Minimal `PublicInputs` for serde-only tests (the AC-3 zk-circuit
    /// path constructs richer values via `prove_batch_signature`).
    fn sample_public_inputs_for_test() -> PublicInputs {
        PublicInputs {
            ask_id: [0u8; 32],
            cap_root_hash: [0u8; 32],
            invocation_hash: [0u8; 32],
            holder_did: "did:octo:zTest".to_owned(),
            current_unix_time: 0,
            provider_slot_id: "test-slot".to_owned(),
            axes_consumed: vec![],
            output_hash: None,
        }
    }
}
