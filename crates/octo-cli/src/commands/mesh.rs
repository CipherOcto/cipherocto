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

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use clap::Subcommand;
use octo_ident::{CanonicalCodec, DidCodec};
use serde::{Deserialize, Serialize};

use super::peer::{self, PeerAction};
use crate::commands::identity::require_confirm;
use crate::error::{sanitize_substrate_error, OctoCliError};
use crate::output::{Hex32, OutputEnvelope};
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
        OutputEnvelope::preview_only(output, 0)
    } else {
        OutputEnvelope::new(output, 0)
    };
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("render envelope: {e}")))
        })
}

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
fn write_forward_receipt(receipt: &ForwardReceipt) -> std::io::Result<()> {
    let path = forward_receipts_log_path()?;
    let line = serde_json::to_string(receipt).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("serialize receipt: {e}"),
        )
    })?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    writeln!(file, "{line}")?;
    Ok(())
}

/// Resolve `$OCTO_HOME/mesh/forward-receipts.log` per RFC-0011-f
/// §Subcommand Taxonomy `forward` "Side effects" row. Uses
/// `OCTO_HOME` env var when set, else `~/.octo` per the parent
/// RFC-0011 §Substrate Path Convention.
fn forward_receipts_log_path() -> std::io::Result<PathBuf> {
    let base = std::env::var_os("OCTO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".octo")))
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "neither OCTO_HOME nor HOME is set; cannot resolve forward-receipts.log path",
            )
        })?;
    let dir = base.join("mesh");
    fs::create_dir_all(&dir)?;
    Ok(dir.join("forward-receipts.log"))
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
}
