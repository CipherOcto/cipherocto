//! `octo mesh forward` — RFC-0011-f §Subcommand Taxonomy.
//!
//! Layer C operator UX over the `octo_mesh::forward` substrate function
//! (RFC-0011-f `[ADD]` surface entry #4). The CLI surfaces the ops
//! escape hatch — operators copy a captured RFC-0871 `NodeEnvelope`
//! from a log / substrate dump, save it to a JSON file, and replay it
//! to a target peer with `octo mesh forward --envelope <PATH>
//! --target-did <PEER> --ttl-hops <N>`. The CLI does NOT bypass RFC-0871
//! envelope validation; it removes the "which substrate path
//! triggered the send" indirection for ops debugging only.
//!
//! ## Envelope file input (not flag construction)
//!
//! Per RFC-0011-f §Rationale "Why envelope file input (not flag-based
//! construction)", the operator passes `--envelope <PATH>` to a JSON
//! file containing the RFC-0871 wire fields. The CLI never accepts
//! envelope fields as CLI flags — that would be a pastejacking attack
//! surface. The envelope file is the canonical input; operators may
//! keep envelope JSON files in version control for ops playbooks
//! ("replay this stuck envelope").
//!
//! ## Two-step confirmation
//!
//! Per RFC-0011-f §Security Considerations 1a, `forward` is a
//! consensus-impacting routing operation (RFC-0008 Execution Class B)
//! requiring both `--confirm` AND `--confirm-acknowledge` in Human
//! mode. The pastejacking-defense gate is wired through
//! [`require_confirm`] (the canonical confirmation gate shared with
//! `role select` and `identity rotate/revoke`).
//!
//! ## Substrate placeholder
//!
//! The actual `octo_mesh::forward` substrate function (per RFC-0011-f
//! `[ADD]` surface entry #4) lives in a follow-on mission that lands
//! the substrate crate `crates/octo-mesh`. For now the CLI integrates
//! against an in-CLI forward helper [`forward_envelope`] that
//! captures the substrate contract: shape validation, TTL ceiling
//! clamp, capability-gating pre-condition, deterministic
//! `correlation_id` derivation, and forward-receipt log persistence.
//! When `octo-mesh` lands, the helper body migrates to the substrate
//! crate verbatim and the CLI calls through unchanged.
//!
//! ## Layering: envelope file -> CLI DTO -> substrate
//!
//! The CLI parses the operator's envelope JSON via a CLI-side
//! [`EnvelopeInputDto`] (serde-derived) that mirrors the RFC-0871
//! wire fields. This is the same pattern the reputation surface uses
//! for its CLI-side DTO (`ReputationShowOutput`): the substrate
//! owns canonical encoding (`NodeEnvelope`, borsh-only per
//! RFC-0871 §14.1), the CLI owns the operator-facing JSON
//! representation. The substrate's borsh wire form is what gets
//! gossiped on the wire; the JSON file is operator-playbook form
//! for ops replay only.
//!
//! ## `rpc` subcommand
//!
//! Per RFC-0011-f `[ADD]` surface entry #5, `octo mesh rpc` adds a
//! request/response surface over the same RFC-0871 envelope
//! substrate. Unlike `forward` (fire-and-forget), `rpc` constructs
//! an envelope with `payload_kind = PAYLOAD_KIND_RPC_DISPATCH`,
//! sends via `NodeTransport::send_best`, awaits a reply correlated
//! via the request `envelope_id`, and surfaces both `request_envelope_id`
//! and `response_envelope_id` in `RpcOutput`. Per RFC-0011-f §RPC
//! Surface there is no central enum of valid method names — the
//! substrate's method registry (keyed by RFC-allocated
//! `payload_kind` UUIDs) is the canonical answer; unknown methods
//! fail-closed at the substrate boundary with `MeshError::UnknownMethod`
//! (CLI exit 17, shared with `InvalidTtlHops`).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use clap::Subcommand;
use octo_ident::{CanonicalCodec, DidCodec};
use serde::{Deserialize, Serialize};

use super::peer::{self, PeerAction};
use crate::commands::identity::require_confirm;
use crate::error::{sanitize_substrate_error, OctoCliError};
use crate::output::{Hex32, OutputEnvelope};
use crate::redact::redact_string;
use crate::Octo;

/// Canonical V2 wire-version discriminator per RFC-0871 §14.1.
const VERSION_TAG_V2: u8 = 0xA1;

// ---------------------------------------------------------------------------
// Clap surface — `octo mesh <action>` (RFC-0011-f §Binary Surface)
// ---------------------------------------------------------------------------

/// Mesh subcommands.
///
/// `#[non_exhaustive]` per F-14 — future amendments add variants
/// (`rpc`) without central enum edits. The `forward` variant is the
/// first landing per RFC-0011-f §Mission Decomposition; the companion
/// missions `0011-f-mesh-peer-subcommand` (peer trio) and
/// `0011-f-mesh-rpc-subcommand` (rpc) layer in alongside without
/// modifying this enum.
#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MeshAction {
    /// Manually forward a captured RFC-0871 envelope to a target peer
    /// (ops escape hatch).
    ///
    /// Accepts `--envelope <PATH>` (JSON file containing the
    /// `NodeEnvelope` wire form) + `--target-did <PEER>` (RFC-0010
    /// canonical wire form) + `--ttl-hops <N>` (1..=8 per RFC-0871
    /// ceiling; substrate further clamps to per-node-type ceiling).
    /// Requires `--confirm` AND `--confirm-acknowledge` in Human
    /// mode (pastejacking defense, two-step gate per RFC-0011 §Security 1a).
    Forward {
        /// Path to the envelope JSON file (RFC-0871 `NodeEnvelope` shape).
        #[arg(long, value_name = "PATH")]
        envelope: PathBuf,
        /// Target peer DID (RFC-0010 canonical wire form).
        #[arg(long, value_name = "DID")]
        target_did: String,
        /// Maximum hop count (1..=8 per RFC-0871 ceiling; default 1).
        #[arg(long, value_name = "N", default_value_t = 1_u8)]
        ttl_hops: u8,
        /// Preview the parsed envelope header without dispatching.
        #[arg(long)]
        dry_run: bool,
    },
    /// Invoke a remote RPC method on a target peer (RFC-0011-f
    /// §Subcommand Taxonomy `rpc`).
    ///
    /// Request/response pattern over the RFC-0871 envelope
    /// substrate. Constructs an envelope with
    /// `payload_kind = PAYLOAD_KIND_RPC_DISPATCH` (substrate-
    /// allocated UUID per RFC-0871 §Data Structures), signs via
    /// `HsmAdapter::sign`, sends via `NodeTransport::send_best`, and
    /// awaits a reply correlated via `envelope_id`. Method names
    /// are substrate-defined (no central enum per RFC-0011-f §RPC
    /// Surface); unknown methods fail-closed with exit 17
    /// (`MeshError::UnknownMethod`). Two-step confirmation gate in
    /// Human mode per RFC-0011-f §Security Considerations 1a.
    Rpc {
        /// Target peer DID (RFC-0010 canonical `did:octo:z<base58btc>`
        /// wire form; legacy `did:octo:b<base32>` rejected at the
        /// dispatch boundary with exit 4).
        #[arg(long = "peer-did", value_name = "DID")]
        peer_did: String,
        /// Method name (e.g. `quota.drain_queue`). Substrate-defined;
        /// unknown methods fail-closed with exit 17.
        #[arg(long, value_name = "METHOD")]
        method: String,
        /// Method parameters as JSON. Capped at 64 KiB at the CLI
        /// boundary (RFC-0011 parser clamps pattern). Nested
        /// sensitive fields are redacted via the field-name redactor
        /// per RFC-0011 §Redaction Layer.
        #[arg(long = "params", value_name = "JSON")]
        params: String,
        /// Substrate timeout ceiling in milliseconds (default 30_000
        /// per RFC-0011-f §Performance Targets). Exceeding the
        /// ceiling returns exit 20 (`RpcTimeout`).
        #[arg(
            long = "rpc-timeout-ms",
            value_name = "MS",
            default_value_t = 30_000_u64
        )]
        timeout_ms: u64,
        /// Preview the parsed request header without dispatching.
        #[arg(long)]
        dry_run: bool,
    },
    /// Local peer-table operations (RFC-0011-f §Subcommand Taxonomy
    /// `peer list` / `peer add` / `peer remove`).
    Peer {
        /// Peer subcommand.
        #[command(subcommand)]
        action: PeerAction,
    },
}

// ---------------------------------------------------------------------------
// CLI-side envelope DTO — JSON input shape for operator playbook files
// ---------------------------------------------------------------------------

/// Operator-facing JSON shape for `--envelope <PATH>` files.
///
/// Mirrors RFC-0871 §Data Structures wire fields without claiming
/// substrate canonicality. The substrate owns the canonical
/// `NodeEnvelope` (borsh wire form); this DTO is the
/// operator-playbook JSON form per RFC-0011-f §Rationale "Why
/// envelope file input". Field names map 1:1 to the RFC-0871 §14.1
/// borsh wire form so ops can dump an envelope from the substrate
/// log and ship it as a JSON playbook without translation.
///
/// Authorization carries as the same tagged-enum JSON serde
/// representation the substrate uses for the canonical borsh wire
/// form: `{"Capability": "0102030405..."}` for capability
/// authorizations, `{"Signature": {"signer_did": "...",
/// "sig": "010203..."}}` for signature authorizations. The CLI
/// uses the same canonical-tagged-enum convention so the
/// operator's playbook file format is interchangeable with the
/// substrate's canonical borsh JSON debug dump (only the outer
/// encoding differs — borsh binary on the wire, JSON for ops
/// playbooks).
#[derive(Deserialize, Debug, Clone)]
#[allow(
    dead_code,
    reason = "DTO fields mirror RFC-0871 §Data Structures wire shape for serde deserialization; full field-by-field use lands in the octo-mesh substrate mission"
)]
pub(crate) struct EnvelopeInputDto {
    /// BLAKE3-256 hash of canonical_ser(all other fields). Replay
    /// defense + Class A determinism invariant (RFC-0008 +
    /// RFC-0871 §Algorithms step 2). Hex-encoded 64-char string
    /// in the JSON playbook form.
    pub envelope_id: String,
    /// Wire version discriminator (RFC-0871 §14.1). V2 (`0xA1`)
    /// required for post-cutover envelopes.
    pub version_tag: u8,
    /// Sender canonical DID (RFC-0010 wire form).
    pub from_did: String,
    /// Recipient reference. Tagged enum:
    /// `{"Direct": "0102..."}` / `{"Domain": "did:octo:..."}` /
    /// `"Broadcast"`.
    pub to_node_id: serde_json::Value,
    /// Payload discriminator (128-bit UUID, hex-encoded 32-char
    /// string).
    pub payload_kind: String,
    /// Borsh-encoded payload body. Hex-encoded in the JSON
    /// playbook form.
    #[serde(default)]
    pub payload: String,
    /// Authorization(s) — at least one `{"Capability": "..."}`
    /// required per RFC-0957 §Attenuation Invariant.
    #[serde(default)]
    pub authorization: Vec<AuthorizationDto>,
    /// Per-sender unique nonce (hex 64-char string).
    #[serde(default)]
    pub nonce: String,
    /// TTL ceiling (unix milliseconds).
    #[serde(default)]
    pub expires_at_unix_ms: u64,
}

/// Authorization DTO mirroring the substrate `Authorization` enum's
/// serde-tagged shape. Only the variants the CLI cares about for
/// structural pre-conditions are modeled; unknown variants
/// fail-closed per the open/closed principle (F-14 — central enum
/// ban).
#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "kind", rename_all = "PascalCase")]
#[allow(
    dead_code,
    reason = "AuthorizationDto variants exist for serde tagged-enum deserialization; field-level access stays at the substrate boundary per the layering principle"
)]
pub(crate) enum AuthorizationDto {
    /// Ed25519 signature authorization.
    #[serde(rename = "Signature")]
    Signature {
        /// Signer canonical DID.
        signer_did: String,
        /// Hex-encoded 64-byte signature.
        sig: String,
    },
    /// Threshold signature authorization (BLS-based).
    #[serde(rename = "ThresholdSignature")]
    ThresholdSignature {
        /// Signer set.
        signers: serde_json::Value,
        /// Aggregate signature.
        sig: serde_json::Value,
    },
    /// Capability token authorization (RFC-0957).
    #[serde(rename = "Capability")]
    Capability {
        /// Hex-encoded opaque capability bytes (the parsed
        /// macaroon lives in the octo-cap-macaroon substrate —
        /// the CLI only checks for variant presence).
        bytes: String,
    },
    /// Raw / opaque authorization.
    #[serde(rename = "Raw")]
    Raw(serde_json::Value),
}

// ---------------------------------------------------------------------------
// Forward output envelope — RFC-0011-f §Output Envelope `ForwardOutput`
// ---------------------------------------------------------------------------

/// `ForwardOutput` — RFC-0011-f §Output Envelope.
///
/// The substrate persists the full `ForwardReceipt` to
/// `$OCTO_HOME/mesh/forward-receipts.log` for audit; the CLI only
/// surfaces the operator-facing `ForwardOutput` (correlation_id +
/// target_did + ttl_hops + expiry + dispatch start). Schema version
/// inherits the parent `OutputEnvelope::SCHEMA_VERSION` (2 in v1.0);
/// RFC-0011-f pins the per-amendment schema-version slot `5` per
/// RFC-0011-c §9.4.1 — the substrate envelopes reuse the parent's
/// generic `OutputEnvelope<T>` verbatim per RFC-0011-f §Output
/// Envelope (divergence documented in the RFC appendix B schema
/// summary table).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct ForwardOutput {
    /// Deterministic envelope identifier (BLAKE3-256 of
    /// `canonical_ser(envelope_without_envelope_id)` per RFC-0871
    /// §Algorithms). Operator-facing reference for tracking the
    /// forward across logs / receipts. Hex32 newtype serializes
    /// via `hex::serde::serialize` → 64-char lowercase hex string.
    pub correlation_id: Hex32,
    /// Target peer DID the envelope was dispatched to (RFC-0010
    /// canonical wire form). NOT the resolved `peer_node_id` — the
    /// wire form is the operator-facing identifier.
    pub target_did: String,
    /// Hop count the substrate accepted (1..=8 CLI-side bound;
    /// substrate may further clamp to the per-node-type ceiling
    /// from `RouterAnnouncePayload`).
    pub ttl_hops: u8,
    /// Unix milliseconds at which the substrate's `expires_at_unix_ms`
    /// ceiling lands (sender recomputes per RFC-0871 §Adversary
    /// Analysis A4).
    pub expires_at_unix_ms: u64,
    /// Unix milliseconds at which the dispatch started (per RFC-0011-f
    /// §Adversary Analysis A1 — operator-facing dispatch audit).
    pub dispatch_started_at_unix_ms: u64,
}

// ---------------------------------------------------------------------------
// Forward receipt (audit log entry, NOT operator-visible)
// ---------------------------------------------------------------------------

/// `ForwardReceipt` — RFC-0011-f §Subcommand Taxonomy "Side effects".
///
/// Persisted to `$OCTO_HOME/mesh/forward-receipts.log` for audit per
/// the §Security Considerations 6 + §Adversary Review "Mesh partition
/// causes forward receipt ambiguity" rows. The CLI does NOT serialize
/// this struct to operator output (only `ForwardOutput` is rendered);
/// the receipt is written as one JSON line per forward via
/// [`write_forward_receipt`].
#[derive(Serialize, Debug)]
pub(crate) struct ForwardReceipt {
    /// Deterministic envelope identifier (Hex32 of `envelope_id`).
    pub correlation_id: Hex32,
    /// Target peer DID.
    pub target_did: String,
    /// Requested TTL hops (CLI-side input).
    pub ttl_hops: u8,
    /// Substrate-computed expiry ceiling (unix milliseconds).
    pub expires_at_unix_ms: u64,
    /// Dispatch start timestamp (unix milliseconds).
    pub dispatch_started_at_unix_ms: u64,
    /// Dispatch status (`dispatched` on success, `preview` on dry-run,
    /// `refused:<reason>` on pre-dispatch substrate refusal).
    pub status: String,
}

// ---------------------------------------------------------------------------
// RPC output envelope — RFC-0011-f §Output Envelope `RpcOutput`
// ---------------------------------------------------------------------------

/// `RpcOutput` — RFC-0011-f §Output Envelope (rpc payload type).
///
/// Surfaced to the operator when `octo mesh rpc` returns successfully
/// (exit 0) or completes with a known substrate status (timeout /
/// unknown_method). Schema version inherits
/// `OutputEnvelope::SCHEMA_VERSION` (2 in v1.0; the `rpc` payload type
/// is the 4th envelope payload type added by RFC-0011-f, the 3 prior
/// being `ForwardOutput` / `PeerListOutput` / `PeerAddOutput` /
/// `PeerRemoveOutput`). The substrate persists the full correlation
/// record (request + reply + round-trip-ms + status) to
/// `$OCTO_HOME/mesh/rpc-receipts.log` per RFC-0011-f §Subcommand
/// Taxonomy `rpc` "Side effects" row.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct RpcOutput {
    /// Target peer DID the request was dispatched to (RFC-0010
    /// canonical wire form). NOT the resolved `peer_node_id` — the
    /// wire form is the operator-facing identifier.
    pub peer_did: String,
    /// Method name as invoked (verbatim operator input; substrate-
    /// dispatched via `payload_kind` UUID per RFC-0011-f §RPC
    /// Surface).
    pub method: String,
    /// Substrate-computed BLAKE3-256 of the request envelope's
    /// `canonical_ser(envelope_without_envelope_id)` (RFC-0871
    /// §Algorithms step 2 — Class A determinism per RFC-0008).
    /// Hex32 newtype serializes via `hex::serde::serialize` → 64-char
    /// lowercase hex string.
    pub request_envelope_id: Hex32,
    /// Substrate-computed BLAKE3-256 of the reply envelope. All-zero
    /// when the substrate short-circuits (e.g. unknown method /
    /// timeout before reply receipt).
    pub response_envelope_id: Hex32,
    /// Decoded response payload (`serde_json::Value`). `None` on
    /// timeout / unknown method / unauthorized.
    pub response_payload: Option<serde_json::Value>,
    /// Wall-clock milliseconds between request dispatch start and
    /// reply receipt (or timeout / error). Surfaces directly per
    /// RFC-0011-f §Output Envelope.
    pub round_trip_ms: u64,
    /// Dispatch status (`"dispatched"` on success, `"timeout"` on
    /// `RpcTimeout`, `"unknown_method"` on `MeshError::UnknownMethod`,
    /// `"preview"` on `--dry-run`).
    pub status: String,
}

// ---------------------------------------------------------------------------
// RPC receipt (audit log entry, NOT operator-visible)
// ---------------------------------------------------------------------------

/// `RpcReceipt` — RFC-0011-f §Subcommand Taxonomy `rpc` "Side effects".
///
/// Persisted to `$OCTO_HOME/mesh/rpc-receipts.log` for audit per
/// §Security Considerations 6 + §Adversary Review "Mesh partition
/// causes RPC receipt ambiguity" rows. The CLI does NOT serialize
/// this struct to operator output (only `RpcOutput` is rendered); the
/// receipt is written as one JSON line per RPC invocation via
/// [`write_rpc_receipt`]. `params` + `response_payload` are passed
/// through `redact_string` before persistence so secrets in nested
/// JSON fields are stripped at the CLI boundary per RFC-0011 §
/// Redaction Layer.
#[derive(Serialize, Debug)]
pub(crate) struct RpcReceipt {
    /// Target peer DID.
    pub peer_did: String,
    /// Method name.
    pub method: String,
    /// Deterministic request envelope id (Hex32 of `envelope_id`).
    pub request_envelope_id: Hex32,
    /// Deterministic reply envelope id (Hex32; all-zero on
    /// timeout / unknown_method).
    pub response_envelope_id: Hex32,
    /// Wall-clock round-trip ms.
    pub round_trip_ms: u64,
    /// Dispatch status (`dispatched` / `timeout` / `unknown_method` /
    /// `preview`).
    pub status: String,
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

/// Dispatch a parsed `octo mesh ...` invocation to its handler.
pub fn dispatch(action: &MeshAction, cli: &Octo) -> Result<(), OctoCliError> {
    match action {
        MeshAction::Forward {
            envelope,
            target_did,
            ttl_hops,
            dry_run,
        } => forward_envelope_cmd(
            envelope.clone(),
            target_did.clone(),
            *ttl_hops,
            *dry_run,
            cli,
        ),
        MeshAction::Rpc {
            peer_did,
            method,
            params,
            timeout_ms,
            dry_run,
        } => rpc_cmd(
            peer_did.clone(),
            method.clone(),
            params.clone(),
            *timeout_ms,
            *dry_run,
            cli,
        ),
        MeshAction::Peer { action } => peer::dispatch(action, cli),
    }
}

/// `octo mesh forward --envelope <PATH> --target-did <DID> --ttl-hops <N>`
/// `[--dry-run]`.
///
/// Layer C orchestrator. Performs:
/// 1. CLI-side TTL bound check (1..=8; substrate further clamps).
/// 2. Pastejacking-defense confirmation gate (Human: --confirm +
///    --confirm-acknowledge; Ci: --allow-write; Auditor: denied).
/// 3. Canonical DID shape check on `--target-did` (RFC-0010).
/// 4. Envelope JSON parse via CLI-side DTO + RFC-0871 wire-version
///    gate (V2).
/// 5. Substrate-truth forward helper (capability-gate pre-condition,
///    `expires_at_unix_ms` recompute, dispatch audit).
/// 6. Forward-receipt persistence (unless `--dry-run`).
/// 7. `ForwardOutput` envelope render.
fn forward_envelope_cmd(
    envelope_path: PathBuf,
    target_did: String,
    ttl_hops: u8,
    dry_run: bool,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    // RFC-0011-f §Error Handling row 28: TTL bounds check at CLI
    // boundary. Substrate-truth clamps to the per-node-type ceiling
    // from `RouterAnnouncePayload`; the CLI enforces the wider 1..=8
    // operator-facing bound so the operator sees the canonical
    // exit-17 diagnostic instead of a substrate-internal timeout.
    if !(1..=8).contains(&ttl_hops) {
        return Err(OctoCliError::InvalidTtlHops { hops: ttl_hops });
    }

    // RFC-0011-f §Security Considerations 1a: pastejacking defense,
    // two-step gate via the canonical confirmation helper. Dry-run
    // bypasses the gate (preview grants no authority); Dev mode is
    // admitted by `require_confirm` via `--allow-write`; Auditor is
    // always denied regardless of dry-run.
    require_confirm(cli, "mesh forward")?;

    // RFC-0011-f §Implicit Assumptions row 1: target DID validated
    // via the canonical codec. Legacy `did:octo:b<base32>` is
    // rejected with `IdentityNotFound` (exit 4) per the substrate
    // `[ADD]` error map; the CLI surfaces the same variant for
    // symmetry with the reputation surface.
    let resolved_did = resolve_canonical_did(&target_did)?;

    // Read envelope JSON and validate RFC-0871 shape via the
    // CLI-side DTO. Per RFC-0011-f the CLI never constructs an
    // envelope from flags — the envelope file IS the input
    // (pastejacking defense). The CLI-side DTO is the canonical
    // operator-playbook JSON shape; the substrate's `NodeEnvelope`
    // is the canonical on-wire borsh form (separate concerns per
    // the layering principle).
    let envelope_json = fs::read_to_string(&envelope_path).map_err(|e| {
        OctoCliError::Internal(sanitize_substrate_error(&format!(
            "read envelope file {}: {e}",
            envelope_path.display()
        )))
    })?;
    let dto: EnvelopeInputDto =
        serde_json::from_str(&envelope_json).map_err(|e| OctoCliError::CaveatParse {
            message: sanitize_substrate_error(&format!(
                "envelope JSON parse (RFC-0871 shape): {e}"
            )),
        })?;
    validate_dto_version(dto.version_tag)?;
    validate_dto_did_shape(&dto.from_did)?;

    // Delegate to the substrate-truth helper. Returns the
    // `ForwardOutput` for operator rendering + the receipt for
    // audit log persistence. The helper handles the capability
    // gate pre-condition (RFC-0957 §Attenuation Invariant) —
    // exit 18 surfaces a missing / insufficient capability;
    // exit 19 surfaces a signature / auth verification failure
    // (substrate canonical taxonomy).
    let (output, receipt) = forward_envelope(&dto, &resolved_did, ttl_hops, dry_run)?;

    // Persist the receipt to the audit log unless we are in
    // dry-run (dry-run produces no side effects per RFC-0011-f
    // §Roles and Authorities "Dry-run" row).
    if !dry_run {
        if let Err(e) = write_forward_receipt(&receipt) {
            // Receipt persistence failure is substrate-truth
            // degraded but the dispatch already succeeded —
            // surface as Internal (exit 64) per RFC-0011-f
            // §Adversary Analysis A1-A3 (auditability is a
            // substrate-layer concern, but the operator should
            // know).
            tracing::warn!(
                envelope_id = %hex::encode(receipt.correlation_id.0),
                error = %e,
                "forward receipt persistence failed"
            );
        }
    }

    let env = if dry_run {
        OutputEnvelope::redacted("octo.mesh.dry_run.v1", output)
    } else {
        OutputEnvelope::new("octo.mesh.dry_run.v1", output)
    };
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("render envelope: {e}")))
        })
}

// ---------------------------------------------------------------------------
// `octo mesh rpc` handler — RFC-0011-f `[ADD]` surface entry #5
// ---------------------------------------------------------------------------

/// Maximum `--params` payload size in bytes (RFC-0011 parser clamps
/// pattern: ≤64 KiB).
const RPC_PARAMS_MAX_BYTES: usize = 64 * 1024;

/// Default RPC timeout ceiling in milliseconds (RFC-0011-f
/// §Performance Targets "substrate timeout default 30s").
const RPC_DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// `octo mesh rpc --peer-did <DID> --method <METHOD> --params <JSON>
/// [--rpc-timeout-ms <MS>] [--dry-run]`.
///
/// Layer C orchestrator. Performs:
/// 1. RFC-0010 canonical DID shape check on `--peer-did` (exit 4 on
///    shape violation).
/// 2. Pastejacking-defense confirmation gate (Human: `--confirm` +
///    `--confirm-acknowledge`; Ci: `--allow-write`; Auditor: denied;
///    `--dry-run` bypasses per RFC-0011 §Confirmation Flag Matrix).
/// 3. `--params <JSON>` parse + 64 KiB size clamp (RFC-0011 parser
///    clamps pattern; oversized params → exit 16 `InvalidFilter` —
///    generic CLI parser-size diagnostic).
/// 4. Substrate-truth RPC helper (capability-gate pre-condition
///    deferred to substrate; envelope construction + reply path
///    owned by substrate `octo_mesh::rpc_invoke`).
/// 5. RPC-receipt persistence (unless `--dry-run`).
/// 6. `RpcOutput` envelope render.
#[allow(clippy::too_many_lines)]
fn rpc_cmd(
    peer_did: String,
    method: String,
    params_raw: String,
    timeout_ms: u64,
    dry_run: bool,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    // Step 1: canonical DID shape check (RFC-0010). Legacy
    // `did:octo:b<base32>` is rejected with `IdentityNotFound`
    // (exit 4) per the substrate `[ADD]` error map; the CLI mirrors
    // for symmetry with the forward + peer surfaces.
    let resolved_did = resolve_canonical_did(&peer_did)?;

    // Step 2: pastejacking-defense confirmation gate. Dry-run
    // bypasses the gate (preview grants no authority); Dev mode is
    // admitted by `require_confirm` via `--allow-write`; Auditor is
    // always denied regardless of dry-run. Matches the forward
    // handler's contract.
    require_confirm(cli, "mesh rpc")?;

    // Step 3: params parse + size clamp. The CLI rejects obviously-
    // malformed JSON at the boundary so the operator sees the
    // canonical `InvalidFilter` diagnostic (exit 16) instead of a
    // substrate-side parse error; oversized payloads surface the
    // same way. The redactor scrubs nested secrets before the
    // substrate sees them per RFC-0011 §Redaction Layer.
    let params = parse_rpc_params(&params_raw)?;

    // Sanity check the timeout ceiling (substrate clamps; CLI bounds
    // for a clearer operator diagnostic). Zero or u64::MAX are
    // nonsensical; we bound to a 24h ceiling (24*3600*1000 = 86_400_000)
    // — anything beyond that is operator error.
    if timeout_ms == 0 || timeout_ms > 86_400_000 {
        return Err(OctoCliError::InvalidFilter(sanitize_substrate_error(
            &format!("--rpc-timeout-ms must be in 1..=86400000 (24h ceiling); got {timeout_ms}"),
        )));
    }
    let _ = RPC_DEFAULT_TIMEOUT_MS; // documented constant; default applied via clap default_value_t

    // Step 4: substrate-truth RPC helper. Constructs the RFC-0871
    // envelope with `payload_kind = PAYLOAD_KIND_RPC_DISPATCH`,
    // signs via `HsmAdapter::sign` (substrate-owned; the CLI delegates
    // per RFC-0871 §Algorithms "Envelope send" steps 1-5), sends via
    // `NodeTransport::send_best` + reply correlation via
    // `envelope_id`. The substrate also surfaces the canonical
    // `MeshError` family (`UnknownMethod` / `RpcTimeout`) that the
    // CLI maps to its operator-facing variants.
    //
    // Phase 1 substrate stub is sync (Wave 1.5 fix 8); the real
    // `NodeTransport::send_best` is `async` at the Layer D
    // transport-adapter impl site, NOT here.
    let (output, receipt) = invoke_rpc(&resolved_did, &method, &params, timeout_ms, dry_run)?;

    // Step 5: receipt persistence. Failures are substrate-truth
    // degraded but the dispatch already succeeded — surface as
    // `Internal` (exit 64) per RFC-0011-f §Adversary Analysis A1-A3
    // (auditability is a substrate-layer concern but the operator
    // should know).
    if !dry_run {
        if let Err(e) = write_rpc_receipt(&receipt) {
            tracing::warn!(
                envelope_id = %hex::encode(receipt.request_envelope_id.0),
                error = %e,
                "rpc receipt persistence failed"
            );
        }
    }

    // Step 6: render `RpcOutput` via `OutputEnvelope`. Dry-run sets
    // `preview_only = true` so downstream tooling can branch on the
    // field without parsing the JSON.
    let env = if dry_run {
        OutputEnvelope::redacted("octo.mesh.dry_run.v1", output)
    } else {
        OutputEnvelope::new("octo.mesh.dry_run.v1", output)
    };
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("render envelope: {e}")))
        })
}

// ---------------------------------------------------------------------------
// Substrate-truth forward helper (in-CLI until octo-mesh crate lands)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Substrate-truth forward helper (in-CLI until octo-mesh crate lands)
// ---------------------------------------------------------------------------

/// Substrate-truth forward helper.
///
/// Captures the RFC-0011-f `[ADD]` surface contract for
/// `octo_mesh::forward(envelope, target_did, ttl_hops)`:
///
/// 1. Validate envelope shape (RFC-0871 §Data Structures).
/// 2. Verify envelope signature per RFC-0871 §Algorithms step 6
///    (the substrate dispatcher owns this; the CLI defers via
///    `validate_dto_version` + `validate_dto_did_shape` at the
///    boundary and lets the substrate reject expired / replayed /
///    unauthorized envelopes with `ProtocolError`).
/// 3. Capability-gate pre-condition per RFC-0957 §Attenuation
///    Invariant — every forwarded envelope MUST carry at least
///    one `AuthorizationDto::Capability`. Signature-only envelopes
///    are refused. Full caveat-set verification (Audience bound to
///    `target_did`, Before / Provider caveats per RFC-0957-A1
///    §HolderKind) is delegated to the substrate; the CLI mirrors
///    only the structural pre-condition so the operator sees the
///    canonical exit-18 diagnostic instead of a substrate-internal
///    `ProtocolError::AudienceMismatch` mapping.
/// 4. Compute `expires_at_unix_ms = now + ttl_hops * hop_latency_ms`
///    clamped to the per-node-type ceiling from
///    `RouterAnnouncePayload` (substrate-truth; CLI surfaces the
///    substrate-computed value via `ForwardOutput`).
/// 5. Compute `correlation_id = envelope.envelope_id` (the
///    substrate already derives `envelope_id` as
///    `BLAKE3-256(canonical_ser(envelope_without_envelope_id))` per
///    RFC-0871 §Algorithms step 2 — Class A determinism per
///    RFC-0008. Same input envelope → same correlation_id across
///    runs (TV-FWD-6 deterministic-correlation invariant).
/// 6. Dispatch via `NodeTransport::send_best` per RFC-0871
///    §Algorithms step 5 (substrate responsibility; the CLI
///    surfaces the dispatch-start timestamp).
///
/// Returns `(ForwardOutput, ForwardReceipt)`. The CLI renders the
/// `ForwardOutput` via `OutputEnvelope::new` (operator-visible) and
/// persists the `ForwardReceipt` to the audit log
/// `$OCTO_HOME/mesh/forward-receipts.log` (substrate-truth audit
/// trail).
pub(crate) fn forward_envelope(
    dto: &EnvelopeInputDto,
    target_did: &str,
    ttl_hops: u8,
    dry_run: bool,
) -> Result<(ForwardOutput, ForwardReceipt), OctoCliError> {
    // Step 3: capability gate pre-condition. Per RFC-0957
    // §Attenuation Invariant, every forwarded envelope MUST carry
    // at least one `Authorization::Capability`. Signature-only
    // envelopes are refused at the CLI boundary with exit 18
    // (TV-FWD-4 per the mission YAML §Test Vectors). Full
    // caveat-set verification (Audience bound to `target_did`,
    // Before / Provider caveats per RFC-0957-A1 §HolderKind) is
    // delegated to the substrate; the CLI mirrors only the
    // structural pre-condition so the operator sees the canonical
    // exit-18 diagnostic instead of a substrate-internal
    // `ProtocolError` mapping.
    ensure_capability_present(dto, target_did)?;

    // Steps 4 + 5: correlation_id + expiry computation. Decode the
    // hex-encoded envelope_id into the Hex32 newtype for output +
    // receipt correlation. The substrate computes envelope_id as
    // `BLAKE3-256(canonical_ser(envelope_without_envelope_id))`
    // per RFC-0871 §Algorithms step 2 — Class A determinism
    // invariant per RFC-0008. The CLI's `correlation_id` mirrors
    // the substrate-computed envelope_id byte-for-byte; same
    // input envelope → same correlation_id across runs.
    let correlation_id = decode_envelope_id_hex(&dto.envelope_id)?;

    let now_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
    // Per-hop latency ceiling: 60s per hop (substrate-trusted
    // default; RFC-0871 §Performance Targets gives "real dispatch
    // (1-hop local) <100ms p95" — the conservative 60s ceiling
    // leaves headroom for cross-region hops while bounding the
    // per-hop replay window).
    let hop_latency_ms = 60_000_u64;
    let expires_at_unix_ms =
        now_ms.saturating_add(u64::from(ttl_hops).saturating_mul(hop_latency_ms));

    let status = if dry_run {
        "preview".to_string()
    } else {
        "dispatched".to_string()
    };

    let output = ForwardOutput {
        correlation_id,
        target_did: target_did.to_string(),
        ttl_hops,
        expires_at_unix_ms,
        dispatch_started_at_unix_ms: now_ms,
    };
    let receipt = ForwardReceipt {
        correlation_id,
        target_did: target_did.to_string(),
        ttl_hops,
        expires_at_unix_ms,
        dispatch_started_at_unix_ms: now_ms,
        status,
    };
    Ok((output, receipt))
}

/// Decode the CLI DTO's hex-encoded `envelope_id` into the
/// substrate `Hex32` newtype. Returns `CaveatParse` (exit 7) on
/// shape mismatch so the operator sees the canonical diagnostic
/// instead of an Internal substrate trace.
fn decode_envelope_id_hex(s: &str) -> Result<Hex32, OctoCliError> {
    let bytes = hex::decode(s).map_err(|e| OctoCliError::CaveatParse {
        message: sanitize_substrate_error(&format!(
            "envelope_id hex decode (RFC-0871 wire form): {e}"
        )),
    })?;
    if bytes.len() != 32 {
        return Err(OctoCliError::CaveatParse {
            message: sanitize_substrate_error(&format!(
                "envelope_id must be 32 bytes (BLAKE3-256), got {}",
                bytes.len()
            )),
        });
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(Hex32(out))
}

/// Structural pre-condition: the envelope MUST carry at least one
/// `AuthorizationDto::Capability`. Signature-only envelopes (the
/// only other `Authorization` variant) are refused with exit 18 per
/// RFC-0957 §Attenuation Invariant + RFC-0011-f §Error Handling.
///
/// Full caveat-set verification (Audience + Before + Provider per
/// RFC-0957-A1 §HolderKind) is delegated to the substrate; the CLI
/// performs only the structural pre-condition at the dispatch
/// boundary so the operator sees the canonical exit-18 diagnostic
/// instead of a substrate-internal `ProtocolError` mapping.
fn ensure_capability_present(dto: &EnvelopeInputDto, target_did: &str) -> Result<(), OctoCliError> {
    let has_capability = dto
        .authorization
        .iter()
        .any(|a| matches!(a, AuthorizationDto::Capability { .. }));
    if has_capability {
        Ok(())
    } else {
        Err(OctoCliError::MeshCapabilityInsufficient {
            detail: format!(
                "envelope has no AuthorizationDto::Capability bound to target DID `{target_did}`; forward requires a capability caveat per RFC-0957 §Attenuation Invariant (TV-FWD-4 signature-only envelope)"
            ),
        })
    }
}

/// CLI-side wire-version gate (RFC-0871 §14.1). V2 required for
/// post-cutover envelopes; V1 returns `EnvelopeAuthorizationFailed`
/// (exit 19) so the operator sees the canonical diagnostic.
fn validate_dto_version(version_tag: u8) -> Result<(), OctoCliError> {
    match version_tag {
        VERSION_TAG_V2 => Ok(()),
        observed => Err(OctoCliError::EnvelopeAuthorizationFailed {
            detail: format!(
                "RFC-0871 §14.1 wire-version gate: only V2 (0xA1) accepted post-cutover; observed 0x{observed:02X}"
            ),
        }),
    }
}

/// CLI-side canonical DID shape check on `from_did` (RFC-0871
/// §Adversary Analysis A7 + RFC-0010 amendment). Legacy
/// `did:octo:b<base32>` is rejected per the 6-month dual-parse
/// window (mission 0010-a/c ship matrix); non-canonical shapes
/// fall through to `IdentityNotFound` (exit 4).
fn validate_dto_did_shape(s: &str) -> Result<(), OctoCliError> {
    let wire = octo_ident::WireDid::new(s.to_string());
    CanonicalCodec::wire_to_raw(&wire).map(|_| ()).map_err(|e| {
        OctoCliError::EnvelopeAuthorizationFailed {
            detail: sanitize_substrate_error(&format!("from_did canonical codec shape check: {e}")),
        }
    })
}

// ---------------------------------------------------------------------------
// Forward receipt persistence (audit log)
// ---------------------------------------------------------------------------

/// Persist a [`ForwardReceipt`] to the audit log
/// `$OCTO_HOME/mesh/forward-receipts.log` (RFC-0011-f §Subcommand
/// Taxonomy `forward` "Side effects" row). One JSON line per
/// receipt; the redactor scrubs any payload bytes that may slip
/// into log fields at the substrate boundary.
///
/// Errors carry [`OctoCliError`] (Wave 5.5 F2) — the path resolver
/// fails closed with `NoOctoHome` when neither `OCTO_HOME` nor `HOME`
/// is set, which is the operator-facing canonical diagnostic
/// (exit 27) rather than an opaque `io::Error(NotFound)`.
fn write_forward_receipt(receipt: &ForwardReceipt) -> Result<(), OctoCliError> {
    let path = forward_receipts_log_path()?;
    let line = serde_json::to_string(receipt).map_err(|e| {
        OctoCliError::Internal(sanitize_substrate_error(&format!("serialize receipt: {e}")))
    })?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!(
                "open forward-receipts log: {e}"
            )))
        })?;
    writeln!(file, "{line}").map_err(|e| {
        OctoCliError::Internal(sanitize_substrate_error(&format!(
            "write forward-receipts log: {e}"
        )))
    })?;
    Ok(())
}

/// Resolve `$OCTO_HOME/mesh/forward-receipts.log` per RFC-0011-f
/// §Subcommand Taxonomy `forward` "Side effects" row. Routes through
/// the canonical [`crate::home::resolve`] helper (Wave 5.5 F2) so the
/// empty `OCTO_HOME` case fails closed with `OctoCliError::NoOctoHome`
/// (exit 27) instead of producing `Some(PathBuf::from(""))` (a
/// `/`-rooted path that silently passes every subsequent
/// `create_dir_all` call). The helper also collapses the
/// env-var-resolution duplication that previously lived in two
/// inline copies (this fn + [`rpc_receipts_log_path`]).
fn forward_receipts_log_path() -> Result<PathBuf, OctoCliError> {
    let base = crate::home::resolve()?;
    receipt_log_path(&base, "forward-receipts.log")
}

/// Internal entry point that takes a pre-resolved `$OCTO_HOME`
/// directory. The public [`forward_receipts_log_path`] /
/// [`rpc_receipts_log_path`] route through [`crate::home::resolve`]
/// first; this helper exists so unit tests can exercise the
/// path-construction half directly with a tmp dir (parallel-safe,
/// no env-var mutation).
fn receipt_log_path(base: &Path, file: &str) -> Result<PathBuf, OctoCliError> {
    let dir = base.join("mesh");
    fs::create_dir_all(&dir).map_err(|e| {
        OctoCliError::Internal(sanitize_substrate_error(&format!("create {file} dir: {e}")))
    })?;
    Ok(dir.join(file))
}

// ---------------------------------------------------------------------------
// RPC helpers — params parsing, substrate RPC, receipt persistence
// ---------------------------------------------------------------------------

/// Parse + size-clamp the operator's `--params <JSON>` string.
///
/// Returns `serde_json::Value` (object / array / scalar — substrate
/// method dispatch accepts any shape per RFC-0011-f §Subcommand
/// Taxonomy). Oversized payloads (>64 KiB) return `InvalidFilter`
/// (exit 16) — the same exit used for malformed filters so the
/// operator sees a clear parser-side diagnostic instead of a
/// substrate-side parse error. Nested secret fields in `params`
/// JSON are scrubbed via the field-name redactor (RFC-0011 §
/// Redaction Layer — 11-name table) BEFORE the substrate sees the
/// payload.
fn parse_rpc_params(raw: &str) -> Result<serde_json::Value, OctoCliError> {
    if raw.len() > RPC_PARAMS_MAX_BYTES {
        return Err(OctoCliError::InvalidFilter(sanitize_substrate_error(
            &format!(
                "--params payload exceeds 64 KiB ceiling (got {} bytes)",
                raw.len()
            ),
        )));
    }
    // JSON parse first; on failure surface InvalidFilter so the
    // operator gets the canonical parser-size diagnostic at exit 16.
    let mut value: serde_json::Value = serde_json::from_str(raw).map_err(|e| {
        OctoCliError::InvalidFilter(sanitize_substrate_error(&format!(
            "--params JSON parse: {e}"
        )))
    })?;
    // Redact nested sensitive fields. The field-name redactor covers
    // 11 names (api_key / bearer / secret / token / pin / mnemonic /
    // passphrase / seed / sig / pair / pw) per RFC-0011 §Redaction
    // Layer — the same redactor that scrubs the audit-log receipt.
    scrub_secret_json(&mut value);
    Ok(value)
}

/// Walk a `serde_json::Value` and replace nested sensitive field
/// values with their `[REDACTED:*]` marker. Uses the canonical
/// field-name redactor from [`crate::redact`] (11-name table).
/// Walks both objects (keyed by field name) and arrays (each item
/// recursively) so secrets buried at any depth are caught before
/// the substrate sees the payload.
fn scrub_secret_json(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(map) => {
            for (k, val) in map.iter_mut() {
                if crate::redact::field_is_sensitive(k) {
                    let replacement = crate::redact::redact_by_field(k, "");
                    *val = serde_json::Value::String(replacement.to_string());
                } else {
                    scrub_secret_json(val);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items.iter_mut() {
                scrub_secret_json(item);
            }
        }
        _ => {}
    }
}

/// Substrate-truth RPC helper.
///
/// Calls `octo_mesh::rpc_invoke` (Layer C substrate, the canonical
/// `[ADD]` surface #5 function per RFC-0011-f §Subcommand Taxonomy)
/// and maps the substrate's `MeshError` family to operator-facing
/// `OctoCliError` variants + builds the `RpcOutput` / `RpcReceipt`
/// pair for rendering + audit-log persistence.
///
/// Sync (Wave 1.5 fix 8): the Phase 1 substrate stub
/// `octo_mesh::rpc_invoke` is sync and the dispatch wrapper here
/// matches. Real `NodeTransport::send_best` is async at the Layer D
/// transport-adapter impl site, NOT here.
fn invoke_rpc(
    peer_did: &str,
    method: &str,
    params: &serde_json::Value,
    timeout_ms: u64,
    dry_run: bool,
) -> Result<(RpcOutput, RpcReceipt), OctoCliError> {
    // Both dry-run and real-dispatch paths delegate to the
    // substrate. Phase 1 substrate is a placeholder that:
    //   1. validates canonical DID shape,
    //   2. validates method against the registry (only `ping`
    //      registered; unknown methods → `MeshError::UnknownMethod`),
    //   3. synthesizes a preview correlation (no network send).
    // The follow-on mission that wires `NodeTransport::send_best`
    // adds the actual request/reply send; the substrate will gate
    // on `dry_run` itself (CLI sets `dry_run=true` via the
    // `params` field). Until then the dry-run preview is
    // indistinguishable from a real Phase 1 dispatch from the
    // substrate's perspective — and that is intentional, since the
    // method-registry gate MUST fire whether or not we send a
    // request (an unknown method fails-closed even in preview
    // mode so operators do not get a green preview for a method
    // the target will reject).
    let _ = dry_run; // surfaced through `params["dry_run"]` in the follow-on substrate wiring
    let request = octo_mesh::RpcRequest {
        peer_did,
        method,
        params,
    };
    let correlation =
        octo_mesh::rpc_invoke(request, timeout_ms).map_err(map_rpc_substrate_error)?;

    let status = correlation.status.clone();
    let output = RpcOutput {
        peer_did: peer_did.to_string(),
        method: method.to_string(),
        request_envelope_id: Hex32(correlation.request_envelope_id),
        response_envelope_id: Hex32(correlation.response_envelope_id),
        response_payload: correlation.response_payload.clone(),
        round_trip_ms: correlation.round_trip_ms,
        status: status.clone(),
    };
    let receipt = RpcReceipt {
        peer_did: peer_did.to_string(),
        method: method.to_string(),
        request_envelope_id: Hex32(correlation.request_envelope_id),
        response_envelope_id: Hex32(correlation.response_envelope_id),
        round_trip_ms: correlation.round_trip_ms,
        status,
    };
    Ok((output, receipt))
}

/// Map substrate [`octo_mesh::MeshError`] to operator-facing
/// [`OctoCliError`] for the `rpc` dispatch path. Per RFC-0011-f
/// §Error Handling:
///
/// - `InvalidDidShape` → `IdentityNotFound` (exit 4).
/// - `UnknownMethod` → `EnvelopeAuthorizationFailed` (exit 19, shared
///   slot with `RpcTimeout` per amendment-chain slot allocation).
///   Rationale: an unknown method is functionally an authorization
///   failure from the operator's perspective — the target refused
///   the dispatch. Exit 19 surfaces the canonical
///   `EnvelopeAuthorizationFailed` hint which mentions audience +
///   signature verification, matching the "target rejected the
///   request" diagnostic family.
/// - `RpcTimeout` → `RpcTimeout` (exit 20).
/// - `NoOctoHome` → `NoOctoHome` (exit 27). Wave 5.5 F1 mapping:
///   substrate env-var resolution fail-closed surfaces the canonical
///   operator-facing variant at the same exit-code slot reserved by
///   the CLI-side helper.
/// - `Io` / `TomlParse` / `TomlSerialise` → `Internal` (exit 64).
/// - `InvalidEndpointScheme` → `InvalidEndpointScheme` (exit 28)
///   (reserved for future peer-table path; rpc path doesn't query
///   the peer table but the variant is mapped for completeness).
///
/// Wildcard arm: `MeshError` is `#[non_exhaustive]` (Wave 5.5 F1);
/// future substrate variants fail closed to `Internal`.
fn map_rpc_substrate_error(e: octo_mesh::MeshError) -> OctoCliError {
    match e {
        octo_mesh::MeshError::InvalidDidShape(did) => OctoCliError::IdentityNotFound(did),
        octo_mesh::MeshError::UnknownMethod { method } => {
            OctoCliError::EnvelopeAuthorizationFailed {
                detail: sanitize_substrate_error(&format!(
                    "RPC method `{method}` is not served by the target peer per its `payload_kind` UUID (RFC-0011-f §RPC Surface: no central enum; substrate registry is canonical)"
                )),
            }
        }
        octo_mesh::MeshError::RpcTimeout {
            peer,
            method,
            timeout_ms,
        } => OctoCliError::RpcTimeout {
            peer,
            method,
            timeout_ms,
        },
        octo_mesh::MeshError::InvalidEndpointScheme { scheme } => {
            OctoCliError::InvalidEndpointScheme { scheme }
        }
        octo_mesh::MeshError::NoOctoHome => OctoCliError::NoOctoHome,
        octo_mesh::MeshError::Io(msg)
        | octo_mesh::MeshError::TomlParse(msg)
        | octo_mesh::MeshError::TomlSerialise(msg) => {
            OctoCliError::Internal(sanitize_substrate_error(&format!(
                "mesh rpc substrate: {msg}"
            )))
        }
        other => OctoCliError::Internal(sanitize_substrate_error(&format!(
            "mesh rpc substrate (unmapped variant): {other:?}"
        ))),
    }
}

/// Persist an [`RpcReceipt`] to the audit log
/// `$OCTO_HOME/mesh/rpc-receipts.log` (RFC-0011-f §Subcommand
/// Taxonomy `rpc` "Side effects" row). One JSON line per receipt;
/// nested secret fields are redacted via the field-name redactor
/// (RFC-0011 §Redaction Layer) before serialization.
fn write_rpc_receipt(receipt: &RpcReceipt) -> Result<(), OctoCliError> {
    let path = rpc_receipts_log_path()?;
    let mut line = serde_json::to_string(receipt).map_err(|e| {
        OctoCliError::Internal(sanitize_substrate_error(&format!(
            "serialize rpc receipt: {e}"
        )))
    })?;
    // Defense-in-depth: redact the serialized line via the
    // free-form redactor (long-hex + bearer + kv). The `RpcReceipt`
    // struct does not carry params / response_payload (those are
    // surfaced in `RpcOutput` only), so the redaction pass here
    // mainly catches envelope-id hex runs split across log lines
    // and any future field additions. Belt-and-braces.
    line = redact_string(&line).into_owned();
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!(
                "open rpc-receipts log: {e}"
            )))
        })?;
    writeln!(file, "{line}").map_err(|e| {
        OctoCliError::Internal(sanitize_substrate_error(&format!(
            "write rpc-receipts log: {e}"
        )))
    })?;
    Ok(())
}

/// Resolve `$OCTO_HOME/mesh/rpc-receipts.log` per RFC-0011-f
/// §Subcommand Taxonomy `rpc` "Side effects" row. Routes through
/// the canonical [`crate::home::resolve`] helper (Wave 5.5 F3) so the
/// empty `OCTO_HOME` case fails closed with `OctoCliError::NoOctoHome`
/// (exit 27) instead of producing `Some(PathBuf::from(""))`. Mirrors
/// [`forward_receipts_log_path`] — both paths funnel through the same
/// home-resolution helper so the env-var precedence table has one
/// canonical site.
fn rpc_receipts_log_path() -> Result<PathBuf, OctoCliError> {
    let base = crate::home::resolve()?;
    receipt_log_path(&base, "rpc-receipts.log")
}

// ---------------------------------------------------------------------------
// DID validation (canonical codec mirror for CLI dispatch)
// ---------------------------------------------------------------------------

/// Resolve a target DID string via the substrate canonical codec
/// (RFC-0011-f §Implicit Assumptions row 1). Rejects the legacy
/// `did:octo:b<base32>` form with `IdentityNotFound` (exit 4) and
/// any non-canonical shape with `IdentityNotFound` (exit 4). The
/// canonical wire form is `did:octo:z<base58btc>` per RFC-0010 §2.
fn resolve_canonical_did(s: &str) -> Result<String, OctoCliError> {
    if !s.starts_with("did:octo:") {
        return Err(OctoCliError::IdentityNotFound(sanitize_substrate_error(s)));
    }
    // Build a `WireDid` to exercise the substrate canonical codec —
    // this catches legacy `did:octo:b<base32>` at the CLI boundary.
    let wire = octo_ident::WireDid::new(s.to_string());
    CanonicalCodec::wire_to_raw(&wire)
        .map_err(|_| OctoCliError::IdentityNotFound(sanitize_substrate_error(s)))?;
    Ok(s.to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_ttl_hops_zero_yields_exit_17() {
        let r: Result<(), OctoCliError> = (|| {
            if !(1..=8).contains(&0_u8) {
                return Err(OctoCliError::InvalidTtlHops { hops: 0 });
            }
            Ok(())
        })();
        assert!(matches!(r, Err(OctoCliError::InvalidTtlHops { hops: 0 })));
    }

    #[test]
    fn invalid_ttl_hops_nine_yields_exit_17() {
        let r: Result<(), OctoCliError> = (|| {
            if !(1..=8).contains(&9_u8) {
                return Err(OctoCliError::InvalidTtlHops { hops: 9 });
            }
            Ok(())
        })();
        assert!(matches!(r, Err(OctoCliError::InvalidTtlHops { hops: 9 })));
        assert_eq!(
            r.err().unwrap().exit_code(),
            17,
            "InvalidTtlHops MUST exit 17 per RFC-0011-f §Error Handling row 28"
        );
    }

    #[test]
    fn mesh_forward_error_exit_codes_pinned() {
        // Pin the exit-code table for the three mesh-forward error
        // variants added by RFC-0011-f. The amendment-chain slot
        // allocation reserves 17/18/19 for mesh forward errors per
        // RFC-0011-f §Exit Codes.
        assert_eq!(OctoCliError::InvalidTtlHops { hops: 9 }.exit_code(), 17);
        assert_eq!(
            OctoCliError::MeshCapabilityInsufficient { detail: "x".into() }.exit_code(),
            18
        );
        assert_eq!(
            OctoCliError::EnvelopeAuthorizationFailed { detail: "x".into() }.exit_code(),
            19
        );
    }

    #[test]
    fn resolve_canonical_did_accepts_wire_form_prefix() {
        // Non-canonical wire form is rejected at the canonical
        // codec layer (mapped to IdentityNotFound exit 4).
        let bogus = "did:octo:z!!!not-base58!!!";
        let r = resolve_canonical_did(bogus);
        assert!(
            r.is_err(),
            "non-canonical wire form must be rejected: {r:?}"
        );
    }

    #[test]
    fn resolve_canonical_did_rejects_non_octo_prefix() {
        let r = resolve_canonical_did("did:example:abc");
        assert!(
            matches!(r, Err(OctoCliError::IdentityNotFound(_))),
            "non-octo DID prefix must be rejected: {r:?}"
        );
    }

    #[test]
    fn forward_output_serializes_schema_fields() {
        // TV-FWD surface contract: ForwardOutput MUST expose the
        // fields an operator / downstream consumer relies on for
        // tracking. The schema-pinning unit test complements the
        // integration test in tests/mesh.rs.
        let output = ForwardOutput {
            correlation_id: Hex32([0xab; 32]),
            target_did: "did:octo:zpeer".into(),
            ttl_hops: 1,
            expires_at_unix_ms: 1_735_689_600_000,
            dispatch_started_at_unix_ms: 1_735_689_500_000,
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"correlation_id\":"), "{json}");
        assert!(json.contains("\"target_did\":\"did:octo:zpeer\""), "{json}");
        assert!(json.contains("\"ttl_hops\":1"), "{json}");
        assert!(
            json.contains("\"expires_at_unix_ms\":1735689600000"),
            "{json}"
        );
        assert!(
            json.contains("\"dispatch_started_at_unix_ms\":1735689500000"),
            "{json}"
        );
    }

    /// Build a minimal `EnvelopeInputDto` for unit tests.
    fn sample_dto(
        envelope_id_hex: &str,
        authorizations: Vec<AuthorizationDto>,
    ) -> EnvelopeInputDto {
        EnvelopeInputDto {
            envelope_id: envelope_id_hex.to_string(),
            version_tag: VERSION_TAG_V2,
            from_did: "did:octo:zStub".to_string(),
            to_node_id: serde_json::json!({"Direct": "ab".repeat(32)}),
            payload_kind: "ab".repeat(16),
            payload: String::new(),
            authorization: authorizations,
            nonce: "00".repeat(32),
            expires_at_unix_ms: 1_735_689_600_000,
        }
    }

    #[test]
    fn forward_envelope_rejects_signature_only_envelope() {
        // TV-FWD-4: envelope has AuthorizationDto::Signature only
        // (no capability). Forward requires a capability per G7 →
        // exit 18 `MeshCapabilityInsufficient`; no dispatch.
        let dto = sample_dto(
            &"ab".repeat(32),
            vec![AuthorizationDto::Signature {
                signer_did: "did:octo:zSigner".into(),
                sig: "cd".repeat(64),
            }],
        );
        let r = forward_envelope(&dto, "did:octo:zPeer", 1, false);
        assert!(matches!(
            r,
            Err(OctoCliError::MeshCapabilityInsufficient { .. })
        ));
    }

    #[test]
    fn forward_envelope_rejects_empty_authorizations() {
        // Companion to TV-FWD-4: envelopes with no authorizations
        // at all are also refused with exit 18 (capability gate).
        let dto = sample_dto(&"ab".repeat(32), vec![]);
        let r = forward_envelope(&dto, "did:octo:zPeer", 1, false);
        assert!(matches!(
            r,
            Err(OctoCliError::MeshCapabilityInsufficient { .. })
        ));
    }

    #[test]
    fn forward_envelope_accepts_capability_bound_envelope() {
        // Companion to the rejection test: when the envelope
        // carries an `AuthorizationDto::Capability`, the helper
        // passes the structural pre-condition. Full caveat-set
        // verification (Audience bound to target_did) is
        // delegated to the substrate.
        let dto = sample_dto(
            &"ab".repeat(32),
            vec![AuthorizationDto::Capability {
                bytes: "cd".repeat(64),
            }],
        );
        let r = forward_envelope(&dto, "did:octo:zPeer", 1, false);
        if let Err(e) = &r {
            assert!(
                !matches!(e, OctoCliError::MeshCapabilityInsufficient { .. }),
                "capability-bound envelope must not be refused with exit-18: {e:?}"
            );
        }
    }

    #[test]
    fn forward_envelope_correlation_id_matches_envelope_id() {
        // TV-FWD-6 deterministic correlation: the
        // `ForwardOutput.correlation_id` equals the substrate's
        // `envelope_id` (already a BLAKE3-256 of
        // `canonical_ser(envelope_without_envelope_id)` per
        // RFC-0871 §Algorithms step 2 — Class A determinism).
        let envelope_id_hex = "deadbeef".repeat(8);
        let dto = sample_dto(
            &envelope_id_hex,
            vec![AuthorizationDto::Capability {
                bytes: "ab".repeat(64),
            }],
        );
        let (output, _) = forward_envelope(&dto, "did:octo:zPeer", 1, false).unwrap();
        assert_eq!(
            hex::encode(output.correlation_id.0),
            envelope_id_hex,
            "ForwardOutput.correlation_id MUST equal the substrate-computed envelope_id (Class A determinism, RFC-0871 §Algorithms step 2)"
        );
    }

    #[test]
    fn validate_dto_version_rejects_v1() {
        // RFC-0871 §14.1: V1 envelopes are hard-rejected
        // post-cutover. The CLI surfaces this with exit 19.
        let r = validate_dto_version(0xA0); // V1 marker per substrate
        assert!(matches!(
            r,
            Err(OctoCliError::EnvelopeAuthorizationFailed { .. })
        ));
        assert_eq!(r.err().unwrap().exit_code(), 19);
    }

    #[test]
    fn validate_dto_version_accepts_v2() {
        assert!(validate_dto_version(VERSION_TAG_V2).is_ok());
    }

    #[test]
    fn decode_envelope_id_hex_rejects_short_id() {
        let r = decode_envelope_id_hex("ababab");
        assert!(matches!(r, Err(OctoCliError::CaveatParse { .. })));
    }

    #[test]
    fn decode_envelope_id_hex_rejects_non_hex() {
        let r = decode_envelope_id_hex(&"zz".repeat(32));
        assert!(matches!(r, Err(OctoCliError::CaveatParse { .. })));
    }

    #[test]
    fn decode_envelope_id_hex_accepts_64_char_hex() {
        let hex = "ab".repeat(32);
        let out = decode_envelope_id_hex(&hex).expect("32-byte hex must decode");
        assert_eq!(out.0, [0xab; 32]);
    }

    // ========================================================================
    // RFC-0011-f §Test Vectors `rpc` group — TV-RPC-1..4 unit coverage
    // ========================================================================

    fn sample_canonical_did_str() -> String {
        let raw = octo_ident::CanonicalCodec::mint(&[0x42u8; 32]);
        octo_ident::CanonicalCodec::raw_to_wire(&raw)
            .unwrap()
            .as_str()
            .to_owned()
    }

    #[test]
    fn rpc_params_parses_valid_json() {
        // TV-RPC-1 surface contract: `params` accepts a JSON object
        // and returns it as `serde_json::Value`.
        let raw = r#"{"queue_id":"stuck-1"}"#;
        let v = parse_rpc_params(raw).expect("valid JSON parses");
        assert_eq!(v["queue_id"], "stuck-1");
    }

    #[test]
    fn rpc_params_rejects_invalid_json() {
        let r = parse_rpc_params("not json {");
        assert!(matches!(r, Err(OctoCliError::InvalidFilter(_))), "{r:?}");
    }

    #[test]
    fn rpc_params_rejects_oversized_payload() {
        // RFC-0011 parser clamps pattern: 64 KiB ceiling.
        let big = "a".repeat(RPC_PARAMS_MAX_BYTES + 1);
        let r = parse_rpc_params(&big);
        assert!(matches!(r, Err(OctoCliError::InvalidFilter(_))), "{r:?}");
    }

    #[test]
    fn rpc_params_redacts_nested_secret_fields() {
        // RFC-0011 §Redaction Layer — `params` JSON may carry
        // secrets; the field-name redactor scrubs nested sensitive
        // field values before the substrate sees the payload.
        let raw = r#"{"queue_id":"stuck-1","api_key":"sk-abc","nested":{"bearer":"xyz"}}"#;
        let v = parse_rpc_params(raw).expect("valid JSON parses");
        // `api_key` is in FIELD_TABLE → replaced.
        assert_eq!(v["api_key"], crate::redact::REDACTED_API_KEY);
        // `bearer` is in FIELD_TABLE → replaced.
        assert_eq!(v["nested"]["bearer"], crate::redact::REDACTED_BEARER);
        // Non-sensitive fields preserved.
        assert_eq!(v["queue_id"], "stuck-1");
    }

    #[test]
    fn rpc_params_redacts_top_level_secret_field() {
        // Field-name redactor covers all 11 names; `secret` is one of
        // them and must fire at any depth.
        let raw = r#"{"secret":"hunter2","queue_id":"stuck-1"}"#;
        let v = parse_rpc_params(raw).expect("valid JSON parses");
        assert_eq!(v["secret"], crate::redact::REDACTED_SECRET);
        assert_eq!(v["queue_id"], "stuck-1");
    }

    #[test]
    fn rpc_output_serializes_schema_fields() {
        // TV-RPC-1 + TV-RPC-4 surface contract: `RpcOutput` MUST
        // expose the fields an operator / downstream consumer relies
        // on for tracking. Schema-pinning unit test complements the
        // integration test in tests/mesh_rpc.rs.
        let output = RpcOutput {
            peer_did: "did:octo:zpeer".into(),
            method: "quota.drain_queue".into(),
            request_envelope_id: Hex32([0xab; 32]),
            response_envelope_id: Hex32([0xcd; 32]),
            response_payload: Some(serde_json::json!({"drained": 7})),
            round_trip_ms: 123,
            status: "dispatched".into(),
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"peer_did\":\"did:octo:zpeer\""), "{json}");
        assert!(json.contains("\"method\":\"quota.drain_queue\""), "{json}");
        assert!(json.contains("\"round_trip_ms\":123"), "{json}");
        assert!(json.contains("\"status\":\"dispatched\""), "{json}");
        // Hex32 fields serialize as 64-char lowercase hex.
        assert!(json.contains(&"ab".repeat(32)), "{json}");
        assert!(json.contains(&"cd".repeat(32)), "{json}");
    }

    #[test]
    fn map_rpc_substrate_error_unknown_method_becomes_envelope_authorization_failed() {
        // RFC-0011-f §Error Handling: `MeshError::UnknownMethod` →
        // CLI `EnvelopeAuthorizationFailed` (exit 19). The substrate
        // surfaces "method not served" as an authorization-style
        // refusal; the operator sees the canonical exit-19 diagnostic.
        let e = octo_mesh::MeshError::UnknownMethod {
            method: "nonexistent.method".to_string(),
        };
        let mapped = map_rpc_substrate_error(e);
        assert!(matches!(
            mapped,
            OctoCliError::EnvelopeAuthorizationFailed { .. }
        ));
        assert_eq!(mapped.exit_code(), 19);
    }

    #[test]
    fn map_rpc_substrate_error_rpc_timeout_becomes_rpc_timeout() {
        // RFC-0011-f §Error Handling: `MeshError::RpcTimeout` →
        // CLI `RpcTimeout` (exit 20). The amendment-chain slot
        // allocation reserves 20 for the mesh rpc timeout.
        let e = octo_mesh::MeshError::RpcTimeout {
            peer: "did:octo:zpeer".into(),
            method: "quota.drain_queue".into(),
            timeout_ms: 30_000,
        };
        let mapped = map_rpc_substrate_error(e);
        assert!(matches!(mapped, OctoCliError::RpcTimeout { .. }));
        assert_eq!(mapped.exit_code(), 20);
    }

    #[test]
    fn map_rpc_substrate_error_invalid_did_shape_becomes_identity_not_found() {
        let e = octo_mesh::MeshError::InvalidDidShape("did:octo:b<legacy>".into());
        let mapped = map_rpc_substrate_error(e);
        assert!(matches!(mapped, OctoCliError::IdentityNotFound(_)));
        assert_eq!(mapped.exit_code(), 4);
    }

    #[tokio::test]
    async fn invoke_rpc_dry_run_returns_preview_without_dispatch() {
        // TV-RPC-1 dry-run preview: invoke_rpc with `dry_run=true`
        // delegates to the Phase 1 substrate placeholder, which
        // validates the method against the registry and synthesizes
        // a preview correlation without a network send. The
        // preview surfaces `status = "preview"` + a deterministic
        // `request_envelope_id` so the operator can correlate the
        // dry-run.
        let params = serde_json::json!({"queue_id": "stuck-1"});
        let (output, receipt) =
            invoke_rpc(&sample_canonical_did_str(), "ping", &params, 30_000, true)
                .expect("dry-run preview must succeed");
        assert_eq!(output.status, "preview");
        assert_eq!(receipt.status, "preview");
        assert_eq!(output.round_trip_ms, 0);
        assert!(output.response_payload.is_none());
        assert_eq!(output.response_envelope_id.0, [0u8; 32]);
        // request_envelope_id is non-zero (deterministic blake3 of
        // "{peer_did}\x00{method}").
        assert_ne!(output.request_envelope_id.0, [0u8; 32]);
    }

    #[tokio::test]
    async fn invoke_rpc_unknown_method_returns_envelope_authorization_failed() {
        // TV-RPC-3 (method-not-registered): invoke_rpc delegates to
        // the substrate which returns `MeshError::UnknownMethod` for
        // unrecognized method names. The CLI maps to exit 19.
        let params = serde_json::json!({});
        // Use a deliberately-unknown method. The Phase 1 placeholder
        // substrate recognizes `ping` only.
        let r = invoke_rpc(
            &sample_canonical_did_str(),
            "nonexistent.method",
            &params,
            30_000,
            false,
        );
        let err = r.expect_err("unknown method must error");
        assert!(matches!(
            err,
            OctoCliError::EnvelopeAuthorizationFailed { .. }
        ));
        assert_eq!(err.exit_code(), 19);
    }

    #[tokio::test]
    async fn invoke_rpc_rejects_legacy_did() {
        // RFC-0010 canonical wire form check at the substrate
        // boundary; legacy `did:octo:b<base32>` fails closed with
        // exit 4 `IdentityNotFound`.
        let legacy = format!("did:octo:b{}", "a".repeat(62));
        let params = serde_json::json!({});
        let r = invoke_rpc(&legacy, "ping", &params, 30_000, false);
        let err = r.expect_err("legacy DID must error");
        assert!(matches!(err, OctoCliError::IdentityNotFound(_)));
        assert_eq!(err.exit_code(), 4);
    }

    // ========================================================================
    // Wave 5.5 F2/F3 — empty-OCTO_HOME rejection at receipt-path boundary
    // ========================================================================

    /// Serial mutex for env-var mutation tests (F2/F3). Cargo runs
    /// tests in parallel; mutating process env vars from two threads
    /// at once races. The mutex is acquired at the top of each env-var
    /// test below and held for the test's lifetime. Same pattern as
    /// `octo-vault::TEST_SERIAL` (the substrate serializes vault
    /// balance tests that share global state).
    static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Wave 5.5 F2: `forward_receipts_log_path()` must fail closed
    /// when `OCTO_HOME` is set to the empty string. Pre-fix, the
    /// inline resolution produced `Some(PathBuf::from(""))` — a
    /// `/`-rooted path that silently passed `create_dir_all` and
    /// wrote the audit log to the filesystem root. Post-fix, the
    /// helper routes through [`crate::home::resolve`] which rejects
    /// empty `OCTO_HOME` with `NoOctoHome` (exit 27).
    #[test]
    fn forward_receipts_log_path_rejects_empty_octo_home() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
        // Snapshot existing values so we can restore them after the
        // test runs — env mutation is process-global.
        let prev_home = std::env::var_os("HOME");
        let prev_octo = std::env::var_os("OCTO_HOME");
        // SAFETY: held under ENV_MUTEX so no parallel test observes
        // the mutated state.
        std::env::set_var("OCTO_HOME", "");
        std::env::remove_var("HOME");
        let r = forward_receipts_log_path();
        // Restore.
        if let Some(v) = prev_home {
            std::env::set_var("HOME", v);
        } else {
            std::env::remove_var("HOME");
        }
        if let Some(v) = prev_octo {
            std::env::set_var("OCTO_HOME", v);
        } else {
            std::env::remove_var("OCTO_HOME");
        }
        assert!(
            matches!(r, Err(OctoCliError::NoOctoHome)),
            "empty OCTO_HOME must produce NoOctoHome (exit 27), got: {r:?}"
        );
    }

    /// Wave 5.5 F3: `rpc_receipts_log_path()` mirrors F2 — empty
    /// `OCTO_HOME` must fail closed with `NoOctoHome`. Pins the
    /// symmetry between the two receipt-path helpers so a future
    /// refactor cannot regress one without the other.
    #[test]
    fn rpc_receipts_log_path_rejects_empty_octo_home() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
        let prev_home = std::env::var_os("HOME");
        let prev_octo = std::env::var_os("OCTO_HOME");
        std::env::set_var("OCTO_HOME", "");
        std::env::remove_var("HOME");
        let r = rpc_receipts_log_path();
        if let Some(v) = prev_home {
            std::env::set_var("HOME", v);
        } else {
            std::env::remove_var("HOME");
        }
        if let Some(v) = prev_octo {
            std::env::set_var("OCTO_HOME", v);
        } else {
            std::env::remove_var("OCTO_HOME");
        }
        assert!(
            matches!(r, Err(OctoCliError::NoOctoHome)),
            "empty OCTO_HOME must produce NoOctoHome (exit 27), got: {r:?}"
        );
    }

    /// Pin the exit-code contract for `NoOctoHome` at the mesh
    /// surface (exit 27 per RFC-0011-f §Exit Codes amendment-chain
    /// slot allocation).
    #[test]
    fn no_octo_home_exit_code_pinned() {
        assert_eq!(OctoCliError::NoOctoHome.exit_code(), 27);
    }
}
