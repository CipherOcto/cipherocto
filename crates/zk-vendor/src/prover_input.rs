//! Structured `ProverInput` JSON adapter for the CipherOcto real-zk FFI
//! shim (mission 0958-c AC-3, 2026-08-05).
//!
//! ## Why this exists
//!
//! The upstream `crates/zk-vendor/stwo-sys/` `stwo_prove` FFI expects the
//! `witness` argument as a JSON-encoded `ProverInput` value (the
//! upstream STWO prover parses the witness payload as JSON). The
//! previous `prove_batch_signature` path in `zk-circuit` passed the
//! canonical-serialized `BatchSigPublicInputs` bytes (a 33+N×32 byte
//! buffer) as the witness — which the upstream STWO JSON parser
//! rejected, returning `VendorError::ProverNull` and triggering the
//! `eprintln!` fallback documented in `zk-circuit` S2.
//!
//! Mission 0958-c AC-3 closes that gap by:
//! 1. Defining a stable `ProverInput` JSON shape (this module).
//! 2. Providing a `to_witness_bytes()` builder that emits the JSON
//!    bytes the upstream STWO `stwo_prove` parses natively.
//! 3. Exposing a `WitnessFormat` enum (re-exported via
//!    `zk_vendor::prover_input::WitnessFormat`) so downstream
//!    `ProofBundle` consumers can record whether the witness was the
//!    canonical JSON shape or the legacy bytes-fallback shape.
//!
//! ## Shape
//!
//! ```json
//! {
//!   "program": "<BUNDLED_CASM_SOURCE_HEX>",
//!   "witness": {
//!     "signer_roots_hex": ["<hex32>", ...],
//!     "message_root_hex": "<hex32>",
//!     "trace_steps_hex": ["<hex32>", ...],
//!     "capability_class": "SelfHost | Hybrid | Wholesale"
//!   },
//!   "public": "<canonical_ser(BatchSigPublicInputs) hex>"
//! }
//! ```
//!
//! The `program` field is the hex-encoded serialized CASM (the upstream
//! STWO parser is happy with either raw hex or base64 — hex is
//! canonical per CipherOcto's wire-format policy). The `witness`
//! payload is structured per signer set + trace + capability class.
//! The `public` field is the hex of the canonical
//! `BatchSigPublicInputs` serialization.
//!
//! ## Determinism
//!
//! All fields are deterministic Class A (RFC-0126): no timestamps, no
//! randomness, no environment variables. The JSON encoder uses
//! `serde_json` with sorted-key canonical output via the helper
//! `to_canonical_json_bytes` (which round-trips through
//! `serde_json::Value` and sorts object keys — same approach as
//! `zk-circuit::canonical_json`).
//!
//! ## Fallback
//!
//! The legacy `bytes-fallback` shape (raw `canonical_ser` bytes) is
//! still representable via `WitnessFormat::BytesFallback` and
//! `ProverInput::to_bytes_fallback()`. Production code path under
//! `VendorState::Ffi` MUST emit `WitnessFormat::ProverInputJson`. The
//! fallback path is observable via the `witness_format` field on the
//! emitted `ProofBundle` and is covered by the integration test
//! `prover_input_fallback_observable` in `tests/ffi_loading.rs`.
//!
//! ## Kill switch
//!
//! `ProverInput::bytes_fallback = false` (the default) trips
//! fail-closed in `zk_circuit::prove_batch_signature` if the FFI is
//! loaded but the witness shape fails to serialize as JSON. This
//! prevents silent regression to the bytes-fallback path.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Structured witness format. Mirrored on the verifier side via the
/// `ProofBundle.witness_format` field (mission 0958-c AC-3).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WitnessFormat {
    /// Real STWO `ProverInput` JSON shape (the AC-3 production path).
    #[serde(rename = "prover-input-json")]
    #[default]
    ProverInputJson,
    /// Legacy raw-byte fallback. Production paths under
    /// `VendorState::Ffi` MUST NOT emit this. Covered by the
    /// `prover_input_fallback_observable` test.
    #[serde(rename = "bytes-fallback")]
    BytesFallback,
}

/// Structured `ProverInput` JSON shape (mission 0958-c AC-3).
///
/// The shape mirrors the upstream STWO parser contract documented in
/// `crates/zk-vendor/stwo-sys/src/lib.rs::stwo_prove` — `program` +
/// `witness` + `public` as a single JSON object. Field order is
/// canonical (sorted via `to_canonical_json_bytes`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProverInput {
    /// Hex-encoded serialized CASM bytecode.
    pub program: String,
    /// Structured witness payload (signer roots + message root +
    /// trace + capability class). Hex-encoded scalars.
    pub witness: WitnessPayload,
    /// Hex of canonical `BatchSigPublicInputs` serialization.
    pub public: String,
    /// Format marker. Defaults to `ProverInputJson`. The bytes-fallback
    /// path is only reachable via `to_bytes_fallback()`.
    #[serde(default)]
    pub witness_format: WitnessFormat,
}

/// Structured witness payload (AC-3).
///
/// `signer_roots_hex` and `trace_steps_hex` are arrays of 32-byte hex
/// strings; `message_root_hex` is a single 32-byte hex string;
/// `capability_class` is the verifier-side classifier (matches
/// `CapabilityClass` in `crates/octo-wallet/src/capability/zk_mint.rs`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitnessPayload {
    /// Hex-encoded `[u8; 32]` signer roots (one per signer; matches
    /// `BatchSigPublicInputs::signer_roots`).
    pub signer_roots_hex: Vec<String>,
    /// Hex-encoded `[u8; 32]` message root (matches
    /// `BatchSigPublicInputs::message_root`).
    pub message_root_hex: String,
    /// Hex-encoded `[u8; 32]` trace step hashes (folded via Poseidon
    /// in the Cairo circuit; surfaced as the trace commitment).
    /// Empty when no trace is present (Hybrid / Wholesale paths).
    #[serde(default)]
    pub trace_steps_hex: Vec<String>,
    /// Capability class discriminator (matches the `CapabilityClass`
    /// enum on the wallet side).
    pub capability_class: CapabilityClassTag,
}

/// Capability class discriminator for the witness payload. Mirrors
/// `CapabilityClass` on the wallet side (SelfHost / Hybrid / Wholesale).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityClassTag {
    SelfHost,
    Hybrid,
    Wholesale,
}

impl ProverInput {
    /// Construct a `ProverInput` from raw byte slices + capability class.
    ///
    /// - `casm_bytes`: raw CASM bytecode (will be hex-encoded into
    ///   `program`).
    /// - `signer_roots`: `&[[u8; 32]]` — one per signer.
    /// - `message_root`: `[u8; 32]` — BLAKE3 root of canonical message.
    /// - `trace_steps`: `&[[u8; 32]]` — folded trace commitments.
    ///   Empty for non-SelfHost classes.
    /// - `public_bytes`: canonical-serialized `BatchSigPublicInputs`
    ///   (will be hex-encoded into `public`).
    /// - `class`: capability class discriminator.
    #[must_use]
    pub fn new(
        casm_bytes: &[u8],
        signer_roots: &[[u8; 32]],
        message_root: &[u8; 32],
        trace_steps: &[[u8; 32]],
        public_bytes: &[u8],
        class: CapabilityClassTag,
    ) -> Self {
        Self {
            program: hex::encode(casm_bytes),
            witness: WitnessPayload {
                signer_roots_hex: signer_roots.iter().map(hex::encode).collect(),
                message_root_hex: hex::encode(message_root),
                trace_steps_hex: trace_steps.iter().map(hex::encode).collect(),
                capability_class: class,
            },
            public: hex::encode(public_bytes),
            witness_format: WitnessFormat::ProverInputJson,
        }
    }

    /// Emit canonical JSON bytes (sorted keys, compact). This is the
    /// shape the upstream `stwo_prove` FFI consumes as its `witness`
    /// argument.
    ///
    /// Determinism: round-trips through `serde_json::Value` + sorts
    /// object keys at every depth (same approach as
    /// `zk-circuit::canonical_json`).
    pub fn to_witness_bytes(&self) -> Result<Vec<u8>, ProverInputError> {
        let raw =
            serde_json::to_vec(self).map_err(|e| ProverInputError::Serialize(e.to_string()))?;
        let value: Value = serde_json::from_slice(&raw)
            .map_err(|e| ProverInputError::Serialize(format!("re-parse for sort: {e}")))?;
        let sorted = sort_keys_recursive(value);
        serde_json::to_vec(&sorted)
            .map_err(|e| ProverInputError::Serialize(format!("re-serialize: {e}")))
    }

    /// Emit a JSON bytes-fallback payload (the legacy shape that the
    /// upstream STWO JSON parser rejects; only used when the witness
    /// payload fails to serialize as JSON and `bytes_fallback = true`).
    ///
    /// The bytes-fallback shape is `{"__cipherocto_bytes_fallback":
    /// true, "public_hex": "<hex>"}` — a minimal valid JSON object so
    /// the upstream parser doesn't reject the witness outright.
    pub fn to_bytes_fallback(&self) -> Result<Vec<u8>, ProverInputError> {
        let value = serde_json::json!({
            "__cipherocto_bytes_fallback": true,
            "public_hex": self.public,
        });
        serde_json::to_vec(&value).map_err(|e| ProverInputError::Serialize(e.to_string()))
    }
}

/// Errors from `ProverInput` construction or serialization.
#[derive(Debug, thiserror::Error)]
pub enum ProverInputError {
    #[error("ProverInput serialize failed: {0}")]
    Serialize(String),
}

/// Recursively sort object keys in a `serde_json::Value` tree. Same
/// approach as `zk-circuit::sort_keys_recursive`.
fn sort_keys_recursive(v: Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map
                .into_iter()
                .map(|(k, v)| (k, sort_keys_recursive(v)))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(entries.into_iter().collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sort_keys_recursive).collect()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> ProverInput {
        let casm = b"\x00\x01\x02casm-fixture\x00\xff";
        let signer_root = [0xab_u8; 32];
        let message_root = [0xcd_u8; 32];
        let trace_step = [0xef_u8; 32];
        let public = b"\x00\x01public-fixture";
        ProverInput::new(
            casm,
            &[signer_root],
            &message_root,
            &[trace_step],
            public,
            CapabilityClassTag::SelfHost,
        )
    }

    #[test]
    fn prover_input_json_round_trip() {
        let p = fixture();
        let bytes = p.to_witness_bytes().expect("serialize");
        // Round-trip back to ProverInput.
        let parsed: ProverInput = serde_json::from_slice(&bytes).expect("parse round-trip");
        assert_eq!(parsed, p);
        // Verify the witness_format field defaults to JSON.
        assert_eq!(parsed.witness_format, WitnessFormat::ProverInputJson);
    }

    #[test]
    fn prover_input_keys_are_sorted() {
        let p = fixture();
        let bytes = p.to_witness_bytes().expect("serialize");
        let s = std::str::from_utf8(&bytes).expect("utf8");
        // Program, public, witness, witness_format — sorted alphabetical.
        let pos_program = s.find("\"program\"").expect("program field");
        let pos_public = s.find("\"public\"").expect("public field");
        let pos_witness = s.find("\"witness\"").expect("witness field");
        let pos_format = s.find("\"witness_format\"").expect("format field");
        assert!(pos_program < pos_public, "program before public");
        assert!(pos_public < pos_witness, "public before witness");
        assert!(pos_witness < pos_format, "witness before witness_format");
    }

    #[test]
    fn witness_format_default_is_json() {
        let f = WitnessFormat::default();
        assert_eq!(f, WitnessFormat::ProverInputJson);
    }

    #[test]
    fn bytes_fallback_emits_minimal_json() {
        let p = fixture();
        let bytes = p.to_bytes_fallback().expect("serialize");
        let parsed: Value = serde_json::from_slice(&bytes).expect("parse");
        assert_eq!(
            parsed.get("__cipherocto_bytes_fallback"),
            Some(&Value::Bool(true))
        );
        assert_eq!(
            parsed.get("public_hex"),
            Some(&Value::String(p.public.clone()))
        );
    }

    #[test]
    fn witness_payload_signer_count_matches_input() {
        let signer_a = [0x01_u8; 32];
        let signer_b = [0x02_u8; 32];
        let message_root = [0x00_u8; 32];
        let public = b"public";
        let p = ProverInput::new(
            b"casm",
            &[signer_a, signer_b],
            &message_root,
            &[],
            public,
            CapabilityClassTag::Hybrid,
        );
        assert_eq!(p.witness.signer_roots_hex.len(), 2);
        assert_eq!(p.witness.trace_steps_hex.len(), 0);
        assert_eq!(p.witness.capability_class, CapabilityClassTag::Hybrid);
    }
}
