//! Mesh RPC dispatch substrate (RFC-0011-f `[ADD]` surface entry #5).
//!
//! Owns the typed request/reply correlation types + the `rpc_invoke`
//! wrapper that the CLI builds against. Layer C substrate per
//! [[cipherocto-design-principles]]; depends only on stable Layer 1
//! substrate (`octo-ident` for canonical DID + `octo-protocol` for
//! `PayloadKindId` + borsh wire form).
//!
//! ## Why a substrate module for an in-CLI helper?
//!
//! The CLI's `mesh rpc` handler is the operator-facing surface; the
//! actual RFC-0011-f `[ADD]` substrate function is `rpc_invoke()` here.
//! Per RFC-0011-f §Subcommand Taxonomy `[ADD]` #5:
//!
//! > `octo_mesh::rpc(peer_did: &Did, method: &str, params: &serde_json::Value) -> Result<serde_json::Value, MeshError>`
//!
//! In Phase 1 the function captures the contract (request envelope_id +
//! reply envelope_id + round-trip-ms + substrate error map); in
//! follow-on missions the body migrates to the real `NodeTransport`
//! substrate when the request/reply substrate lands. The CLI calls
//! through unchanged.
//!
//! ## No central enum for RPC methods
//!
//! Per RFC-0011-f §RPC Surface + §Architectural Principles
//! "Extension over enumeration (no central enums)", method names are
//! substrate-defined via RFC-allocated `payload_kind` UUIDs
//! (RFC-0871 §Data Structures `PayloadKindId`). The CLI does NOT
//! carry a central enum of valid methods; new methods land via
//! substrate additions without CLI changes. Unknown methods fail at
//! the substrate layer with `MeshError::UnknownMethod` (CLI exit 17).
//!
//! ## Redaction
//!
//! The substrate accepts `params` as an arbitrary `serde_json::Value`
//! — operators may pass secrets (API keys, capability bytes) depending
//! on the RPC method. The CLI's redaction layer (RFC-0011 §Redaction
//! Layer field-name redactor) covers 11 sensitive field names; nested
//! `params` JSON values are walked against the same field table at the
//! CLI boundary so the substrate never sees (or persists) redacted
//! bytes.

use serde::{Deserialize, Serialize};

use crate::error::MeshError;
// Bring `DidCodec::parse` into scope — the trait is implemented on
// `CanonicalCodec` but the method is a trait method, not an inherent
// associated function.
use octo_ident::DidCodec;

/// Substrate-truth request/reply correlation record (RFC-0011-f
/// `[ADD]` #5 surface).
///
/// Returned to the CLI for both operator-facing rendering
/// (`RpcOutput`) and audit-log persistence
/// (`$OCTO_HOME/mesh/rpc-receipts.log`). Both `request_envelope_id`
/// and `response_envelope_id` are surfaced per RFC-0011-f
/// §RFC-0871 Envelope Mapping (CLI View ↔ Substrate).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpcCorrelation {
    /// Substrate-computed BLAKE3-256 of the request envelope's
    /// `canonical_ser(envelope_without_envelope_id)` (RFC-0871
    /// §Algorithms step 2 — Class A determinism per RFC-0008).
    pub request_envelope_id: [u8; 32],
    /// Substrate-computed BLAKE3-256 of the reply envelope (or
    /// zeros when the substrate short-circuits — e.g. unknown method
    /// before envelope construction). Non-zero in the
    /// `RpcTimeout` and `UnknownMethod` exit paths only when the
    /// reply envelope was partially constructed.
    pub response_envelope_id: [u8; 32],
    /// Wall-clock milliseconds between request dispatch start and
    /// reply receipt (or timeout / error). The CLI surfaces this as
    /// `round_trip_ms` per RFC-0011-f §Output Envelope.
    pub round_trip_ms: u64,
    /// Response payload body (JSON-decoded) on success. `None`
    /// when the substrate returned an error (timeout / unknown
    /// method / authorization failure).
    pub response_payload: Option<serde_json::Value>,
    /// Dispatch status: `"dispatched"`, `"timeout"`,
    /// `"unknown_method"`, `"unauthorized"`, `"preview"` (dry-run).
    pub status: String,
}

/// Substrate-truth request side input (CLI constructs + hands to the
/// substrate).
///
/// The substrate owns `request_envelope_id` derivation (per RFC-0871
/// §Algorithms step 2); the CLI passes the operator-supplied
/// canonical DID + method + params and the substrate returns the
/// `RpcCorrelation` for rendering.
#[derive(Debug, Clone)]
pub struct RpcRequest<'a> {
    /// Target peer DID (RFC-0010 canonical wire form).
    pub peer_did: &'a str,
    /// RPC method name (e.g. `quota.drain_queue`). Substrate-defined
    /// via `payload_kind` UUIDs per RFC-0011-f §RPC Surface.
    pub method: &'a str,
    /// Method parameters (JSON). Capped at 64 KiB by the CLI per
    /// RFC-0011 parser clamps; the substrate trusts the CLI gate.
    pub params: &'a serde_json::Value,
}

/// `octo_mesh::rpc` substrate function (RFC-0011-f `[ADD]` #5).
///
/// Captures the contract:
///
/// 1. Validate `peer_did` canonical wire form
///    (`octo_ident::CanonicalCodec::parse(s, allow_legacy_bare=false)`)
///    → `MeshError::InvalidDidShape` on shape mismatch.
/// 2. Substrate method registry lookup for `method` →
///    `MeshError::UnknownMethod` when the target does not serve the
///    method per its `payload_kind` UUID (no central enum; the
///    substrate's method registry is the canonical answer per
///    RFC-0871 §Specialized Node Lifecycle).
/// 3. Phase 1 placeholder: synthesize the `RpcCorrelation` so the
///    CLI can render `RpcOutput` + persist the receipt. The actual
///    `NodeTransport::send_best` request/reply path lands in a
///    follow-on mission when the request/reply substrate
///    (RFC-0870k AC-6) ships.
/// 4. On timeout (default 30s substrate ceiling per RFC-0011-f
///    §Performance Targets) → `MeshError::RpcTimeout`.
///
/// # Errors
///
/// - `MeshError::InvalidDidShape` when `peer_did` is not canonical
///   `did:octo:z<base58btc>` (legacy `did:octo:b<base32>` rejected).
/// - `MeshError::UnknownMethod` when the target does not serve
///   `method`. CLI maps to exit 17 (shared with `InvalidTtlHops`).
/// - `MeshError::RpcTimeout` when the reply does not arrive within
///   `timeout_ms`. CLI maps to exit 20.
#[allow(clippy::unused_async)] // async signature reserved for follow-on NodeTransport wiring
pub async fn rpc_invoke(
    request: RpcRequest<'_>,
    timeout_ms: u64,
) -> Result<RpcCorrelation, MeshError> {
    // Step 1: canonical DID shape check (RFC-0010). The CLI also
    // gates this at the dispatch boundary; the substrate re-validates
    // for defense-in-depth (the CLI is not the only caller — a future
    // SDK could call this substrate directly).
    octo_ident::CanonicalCodec::parse(request.peer_did, false)
        .map_err(|e| MeshError::InvalidDidShape(format!("{}: {e}", request.peer_did)))?;

    // Step 2: method registry lookup. Phase 1 has no live registry;
    // the placeholder recognizes a single registered method name
    // (`ping`) and refuses all others with `UnknownMethod`. New
    // methods land via substrate additions to the registry; the CLI
    // does NOT need updates (RFC-0011-f §RPC Surface "no central
    // enum" rationale).
    if request.method.is_empty() || request.method != "ping" {
        return Err(MeshError::UnknownMethod {
            method: request.method.to_string(),
        });
    }

    // Step 3: synthesize the correlation. Phase 1 placeholder —
    // generates a deterministic request_envelope_id from the request
    // fields via blake3 (Routed through `octo_cap_macaroon::blake3_hash`
    // per [[cipherocto-design-principles]] "no parallel abstractions").
    // Real dispatch via `NodeTransport::send_best` lands in the
    // follow-on mission when the request/reply substrate ships.
    let request_envelope_id = derive_request_envelope_id(request.peer_did, request.method);
    let correlation = RpcCorrelation {
        request_envelope_id,
        // Phase 1 placeholder: no reply envelope yet (the substrate
        // does not yet own a request/reply transport). Production
        // wires `NodeTransport::send_best` + correlation receive.
        response_envelope_id: [0u8; 32],
        round_trip_ms: 0,
        response_payload: None,
        status: "preview".to_string(),
    };

    // Step 4: timeout gate. Phase 1 never blocks so timeout is
    // unreachable in the placeholder; the variant is wired so the
    // CLI maps it to exit 20 when the real transport lands.
    let _ = timeout_ms;

    Ok(correlation)
}

/// Deterministic 32-byte request envelope id from the canonical
/// request triple. Uses the substrate's blake3 hash via
/// `octo_cap_macaroon::blake3_hash` (Layer B substrate wrapper; no
/// parallel `blake3` crate dep in this layer per
/// [[cipherocto-design-principles]]).
fn derive_request_envelope_id(peer_did: &str, method: &str) -> [u8; 32] {
    let preimage = format!("{peer_did}\x00{method}");
    octo_cap_macaroon::blake3_hash(preimage.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn canonical_did_str() -> String {
        let raw = octo_ident::CanonicalCodec::mint(&[0x42u8; 32]);
        octo_ident::CanonicalCodec::raw_to_wire(&raw)
            .unwrap()
            .as_str()
            .to_owned()
    }

    #[tokio::test]
    async fn rpc_invoke_accepts_canonical_did() {
        let peer = canonical_did_str();
        let req = RpcRequest {
            peer_did: &peer,
            method: "ping",
            params: &json!({}),
        };
        let out = rpc_invoke(req, 30_000)
            .await
            .expect("canonical DID accepted");
        // Phase 1 placeholder: status=preview + response_payload=None.
        assert_eq!(out.status, "preview");
        assert!(out.response_payload.is_none());
        // request_envelope_id is the deterministic blake3 of "did\x00ping".
        assert_ne!(out.request_envelope_id, [0u8; 32]);
    }

    #[tokio::test]
    async fn rpc_invoke_rejects_legacy_did() {
        let legacy = format!("did:octo:b{}", "a".repeat(62));
        let req = RpcRequest {
            peer_did: &legacy,
            method: "ping",
            params: &json!({}),
        };
        let err = rpc_invoke(req, 30_000)
            .await
            .expect_err("legacy DID must fail");
        assert!(matches!(err, MeshError::InvalidDidShape(_)), "{err:?}");
    }

    #[tokio::test]
    async fn rpc_invoke_rejects_empty_method() {
        let peer = canonical_did_str();
        let req = RpcRequest {
            peer_did: &peer,
            method: "",
            params: &json!({}),
        };
        let err = rpc_invoke(req, 30_000)
            .await
            .expect_err("empty method must fail");
        assert!(matches!(err, MeshError::UnknownMethod { .. }), "{err:?}");
    }

    #[tokio::test]
    async fn rpc_invoke_request_envelope_id_is_deterministic() {
        let peer = canonical_did_str();
        let req1 = RpcRequest {
            peer_did: &peer,
            method: "ping",
            params: &json!({}),
        };
        let req2 = RpcRequest {
            peer_did: &peer,
            method: "ping",
            params: &json!({}),
        };
        let a = rpc_invoke(req1, 30_000).await.unwrap();
        let b = rpc_invoke(req2, 30_000).await.unwrap();
        // Class A determinism (RFC-0008): same input triple
        // (peer_did, method, params) → same envelope_id across runs.
        assert_eq!(a.request_envelope_id, b.request_envelope_id);
    }
}
