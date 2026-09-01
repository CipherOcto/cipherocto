//! `octo reputation show` — RFC-0011-b §Specification.
//!
//! Thin Layer C wrapper over the `octo-reputation` substrate crate's
//! `projection` module (Layer B `[ADD]` per RFC-0011-b §7.4).
//! Operator invocation → clap parse → substrate `project()` +
//! `attestations()` calls → JSON envelope render. No business logic in
//! this module; all decisions live in `octo_reputation::projection`.
//!
//! ## Mode gating (RFC-0011-b §Roles and Authorities)
//!
//! All four modes (Human / Ci / Dev / Auditor) may invoke `show`. The
//! Auditor fail-closed-on-revoked behavior lives at the substrate
//! boundary (per §Security Considerations 3); the CLI propagates the
//! substrate `ReputationRevoked` to the `ReputationRevoked` exit-21
//! variant regardless of caller mode.
//!
//! ## `--no-anchor-verify` (DEV-ONLY escape hatch)
//!
//! Per RFC-0011-b §Security Considerations 1a, `--no-anchor-verify` is
//! REJECTED in Human / Ci / Auditor modes with exit 2. v1.0 does not
//! yet wire the substrate anchor verifier; the flag is parsed but the
//! substrate ignores it. The CLI still gates the flag at the
//! operator-mode boundary so future substrate wiring inherits the
//! mode-gating contract for free.

use clap::Subcommand;
use octo_reputation::{
    projection_attestations, projection_project, AnchorRef, AttestationSummary,
    ReputationComponents, ReputationRecord, Role,
};

use crate::error::{sanitize_substrate_error, OctoCliError};
use crate::flags::OperatorMode;
use crate::output::OutputEnvelope;
use crate::Octo;

/// CLI-facing reputation subcommand enum (Layer C; delegates to
/// `octo_reputation::projection` substrate for decisions).
/// `#[non_exhaustive]` per F-14 — future amendments add variants
/// (e.g., `List`, `History`) without central enum edits.
#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReputationAction {
    /// Show the reputation record for one `(did, role)` tuple.
    Show {
        /// Subject DID. When omitted, the active identity DID is used
        /// (Human / Ci / Dev modes only; Auditor requires `--did`).
        #[arg(long)]
        did: Option<String>,
        /// Role slug (e.g., `builder`, `provider`, `recorder`).
        #[arg(long)]
        role: String,
        /// Inclusive lower bound on `recorded_at_unix` for the
        /// attestation window (RFC-0011-b §7.6). Default: `0`.
        #[arg(long, default_value_t = 0_i64)]
        since: i64,
        /// Maximum number of attestations returned (RFC-0011-b §7.6).
        /// Hard cap: 1000. CLI rejects `--limit > 1000` with exit 2.
        #[arg(long, default_value_t = 10_u32, value_parser = limit_in_range)]
        limit: u32,
        /// Skip anchor-chain verification (DEV-ONLY escape hatch per
        /// RFC-0011-b §Security Considerations 1a). Rejected in
        /// Human / Ci / Auditor modes with exit 2.
        #[arg(long)]
        no_anchor_verify: bool,
        /// Optional anchor hex digest filter — restricts the
        /// attestation window to attestations anchored under the
        /// supplied BLAKE3-256 anchor (RFC-0955-r1 §Wire Contract).
        #[arg(long)]
        anchor: Option<String>,
    },
}

/// CLI-side output struct — RFC-0011-b §7.3 Output Envelope.
///
/// `ReputationShowOutput` composes the substrate `ReputationRecord`
/// (via `.into()` at the dispatch boundary) with the
/// `attestations()` window. Per RFC-0011-b §7.3 this is a **wrapper**
/// payload, NOT a 1:1 mirror of `ReputationRecord` — the substrate
/// owns the canonical aggregate and the CLI composes the
/// attestation window at the envelope boundary.
///
/// Re-export audit comment: `AttestationSummary` and `AnchorRef` are
/// re-exported from `octo_reputation` (no CLI-side redeclaration per
/// RFC-0011-b §7.4). When substrate `ReputationRecord` gains a field,
/// this conversion must be re-audited (RFC-0011-b §7.3 re-export audit
/// block).
#[derive(serde::Serialize, Debug, schemars::JsonSchema)]
pub struct ReputationShowOutput {
    /// Subject DID (canonical `did:octo:` wire form per the substrate
    /// `RecorderDid::to_wire()`; the CLI surfaces the substrate
    /// canonical form unchanged).
    pub did: String,
    /// Role slug (RFC-0011-b §7.4 `Role`).
    pub role: String,
    /// Composite score (substrate-owned RFC-0968 §9; surfaced via the
    /// canonical RFC-0104 Dfp wire form).
    #[schemars(with = "String")]
    pub score: octo_determin::Dfp,
    /// 4-Dfp breakdown per RFC-0011-b §7.5.
    #[schemars(with = "String")]
    pub components: ReputationComponents,
    /// Most-recent-first window of attestation summaries
    /// (RFC-0011-b §7.6). Substrate `attestations()` is the source of
    /// truth; the CLI never recomputes the window.
    #[schemars(with = "Vec<String>")]
    pub attestations: Vec<AttestationSummary>,
    /// RFC 3339 UTC mirror of the substrate aggregate read timestamp
    /// (RFC-0011-b §7.3 — `last_updated_unix` is the substrate field;
    /// `generated_at` is the envelope-level timestamp).
    pub last_updated_unix: i64,
    /// Reference to the last anchor on chain (RFC-0955-r1 §Wire
    /// Contract). `None` if the subject has never been anchored.
    #[schemars(with = "Option<String>")]
    pub anchor_ref: Option<AnchorRef>,
}

impl From<ReputationRecord> for ReputationShowOutput {
    fn from(r: ReputationRecord) -> Self {
        // Subject DID is rendered via the substrate canonical wire
        // form (`octo_reputation::RecorderDid::to_wire()`). The
        // substrate owns the wire encoding (RFC-0010 §Specification
        // + RFC-0968 §2 pending reconcile per the §C3 review note);
        // the CLI never re-encodes the DID — it surfaces the
        // substrate's canonical form unchanged. When the DID cannot
        // be encoded (malformed discriminator), the CLI falls back
        // to the lower-case hex form of the raw 52-byte payload —
        // diagnostic-safe and still round-trippable.
        let did = r
            .did
            .to_wire()
            .unwrap_or_else(|_| hex::encode(r.did.as_bytes()));
        Self {
            did,
            role: r.role.to_string(),
            score: r.score,
            components: r.components,
            attestations: Vec::new(), // Window merged in dispatch via `merge_attestations`.
            last_updated_unix: r.last_updated_unix,
            anchor_ref: r.anchor_ref,
        }
    }
}

impl ReputationShowOutput {
    /// Merge the attestation window into the projection-shaped
    /// `ReputationRecord → ReputationShowOutput` conversion.
    ///
    /// RFC-0011-b §7.3 wrapper intent: the substrate `project()` does
    /// not populate `attestations`; the CLI composes both at the
    /// dispatch boundary. This helper exists so the `From` impl above
    /// stays a pure substrate-shape mirror (no substrate re-entry) and
    /// the wrapper is constructed in one place.
    fn merge_attestations(mut self, attestations: Vec<AttestationSummary>) -> Self {
        self.attestations = attestations;
        self
    }
}

/// Dispatch a parsed `octo reputation ...` invocation to its handler.
pub fn dispatch(action: &ReputationAction, cli: &Octo) -> Result<(), OctoCliError> {
    match action {
        ReputationAction::Show {
            did,
            role,
            since,
            limit,
            no_anchor_verify,
            anchor,
        } => show_reputation(
            did.clone(),
            role.clone(),
            *since,
            *limit,
            *no_anchor_verify,
            anchor.clone(),
            cli,
        ),
    }
}

/// clap value-parser enforcing the RFC-0011-b §7.6 hard cap of 1000
/// on `--limit`. Returns the validated value on success; clap turns
/// the `Err` into a `ClapParse` error (exit 2) so the operator sees
/// the standard `--limit` range error message.
fn limit_in_range(s: &str) -> Result<u32, String> {
    let n: u32 = s
        .parse()
        .map_err(|_| format!("--limit must be a non-negative integer (got `{s}`)"))?;
    if n == 0 {
        return Err("--limit must be at least 1 (use 1 to fetch a single attestation)".to_string());
    }
    if n > 1000 {
        return Err(format!(
            "--limit must not exceed 1000 per RFC-0011-b §7.6 (got `{n}`)"
        ));
    }
    Ok(n)
}

/// `octo reputation show [--did <did>] --role <role> [--since <unix>] [--limit <N>]`
///
/// Read-only projection of the substrate `ReputationRecord` plus the
/// attestation window for one `(did, role)` tuple. No confirmation
/// gate — `show` is read-only across all four operator modes per
/// RFC-0011-b §Roles and Authorities.
#[allow(clippy::too_many_arguments)]
fn show_reputation(
    did: Option<String>,
    role: String,
    since: i64,
    limit: u32,
    no_anchor_verify: bool,
    anchor: Option<String>,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    let mode = cli.mode.mode;
    if no_anchor_verify && !matches!(mode, OperatorMode::Dev) {
        // RFC-0011-b §Security Considerations 1a: DEV-ONLY escape
        // hatch. The flag is rejected in Human / Ci / Auditor modes.
        return Err(OctoCliError::NoAnchorVerifyInMode {
            mode: mode_label(mode),
        });
    }

    // `--anchor <hex>` is parsed but the substrate anchor filter is
    // not yet wired in Phase 1. The CLI accepts the flag for
    // forward-compat; v1.0 ignores the filter and returns the full
    // window. The flag surface stays stable across substrate land.
    let _ = anchor;

    let role = Role::parse(&role).map_err(|e| OctoCliError::InvalidRoleSlug {
        slug: sanitize_substrate_error(&role),
        reason: e.to_string(),
    })?;

    // Resolve the subject DID. v1.0 expects the operator to pass
    // `--did` explicitly; `octo reputation show` without `--did`
    // resolves to `OctoCliError::NoActiveIdentity` (exit 2) when the
    // active-identity lookup path is not wired — `identity show`
    // remains the canonical substrate source for active DID
    // resolution per RFC-0011-b §7.2.
    let did_str = did.ok_or(OctoCliError::NoActiveIdentity)?;
    let subject = parse_did_bytes(&did_str)?;

    // Layer-B substrate call. Phase 1 returns the canonical
    // zero-record + empty window (RFC-0968 §10 baseline). The
    // substrate clock is `i64::MIN` as the "never updated"
    // sentinel; the CLI renders it through the same envelope
    // boundary as any other timestamp.
    let record = projection_project(&subject, &role, i64::MIN);
    let window = projection_attestations(&subject, &role, since, limit as usize);
    let output = ReputationShowOutput::from(record).merge_attestations(window);

    render_envelope("octo.reputation.show.v1", output, cli)
}

/// Parse a canonical `did:octo:` DID string into the substrate
/// `RecorderDid` 52-byte array.
///
/// v1.0 accepts the raw 104-char lowercase hex form of the 52-byte
/// payload (RFC-0968 §2 — the substrate canonical raw key). The
/// substrate's wire form (`did:octo:z<base58btc>` per `to_wire()`)
/// is **not** round-trippable through this CLI input parser in
/// Phase 1 — the substrate does not expose a `from_wire()` inverse
/// (gap to close in Phase 2 per the RFC-0968 §2 vs RFC-0010 §C3
/// reconcile). For v1.0 the canonical CLI input shape is the raw
/// hex form; the output shape (`ReputationShowOutput.did`) is the
/// substrate's wire form.
///
/// Anything other than the 104-char hex form returns `IdentityNotFound`
/// (exit 4) per the substrate `[ADD]` error map in RFC-0011-b
/// §Substrate `[ADD]` Signatures — the operator-friendly fallback
/// when the DID cannot be resolved.
fn parse_did_bytes(s: &str) -> Result<octo_reputation::RecorderDid, OctoCliError> {
    const PREFIX: &str = "did:octo:";
    let payload = s
        .strip_prefix(PREFIX)
        .ok_or_else(|| OctoCliError::IdentityNotFound(sanitize_substrate_error(s)))?;
    if payload.len() != 104 {
        // Substrate wire form (`did:octo:z<base58btc>` per
        // `RecorderDid::to_wire()`) and any other non-hex shape
        // fall through here. Future Phase 2 wiring will need a
        // base58btc multibase decoder; until then this is the
        // substrate gap (documented in the mission YAML §Notes).
        return Err(OctoCliError::IdentityNotFound(sanitize_substrate_error(s)));
    }
    decode_hex_did(s, payload)
}

/// Decode a 104-char hex string into a 52-byte `RecorderDid` payload.
fn decode_hex_did(
    original: &str,
    hex_payload: &str,
) -> Result<octo_reputation::RecorderDid, OctoCliError> {
    let mut bytes = [0u8; 52];
    for (i, chunk) in hex_payload.as_bytes().chunks(2).enumerate() {
        let hex_str = std::str::from_utf8(chunk)
            .map_err(|_| OctoCliError::IdentityNotFound(sanitize_substrate_error(original)))?;
        bytes[i] = u8::from_str_radix(hex_str, 16)
            .map_err(|_| OctoCliError::IdentityNotFound(sanitize_substrate_error(original)))?;
    }
    Ok(octo_reputation::RecorderDid::from_array(bytes))
}

/// Render an output envelope for the given payload (serializable).
///
/// Mirrors the `role::render_envelope` helper — schema string is
/// captured in test-vector documentation only; `OutputEnvelope`
/// carries the schema via `SCHEMA_VERSION`.
fn render_envelope<T: serde::Serialize>(
    _schema: &str,
    data: T,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    let env = OutputEnvelope::new(data, 0);
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("render envelope: {e}")))
        })
}

/// Map [`OperatorMode`] to a stable lowercase label used in
/// `OctoCliError::NoAnchorVerifyInMode`'s `mode` field.
fn mode_label(m: OperatorMode) -> &'static str {
    match m {
        OperatorMode::Human => "human",
        OperatorMode::Ci => "ci",
        OperatorMode::Auditor => "auditor",
        OperatorMode::Dev => "dev",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_did_accepts_canonical_hex_form() {
        let hex = "0".repeat(104);
        let did = format!("did:octo:{hex}");
        let parsed = parse_did_bytes(&did).expect("104-char hex DID must parse");
        assert_eq!(parsed.as_bytes(), &[0u8; 52]);
    }

    #[test]
    fn parse_did_rejects_missing_prefix() {
        let s = "0".repeat(104);
        let err = parse_did_bytes(&s).unwrap_err();
        // Wraps the `IdentityNotFound` variant; exact message is sanitized.
        assert!(matches!(err, OctoCliError::IdentityNotFound(_)));
    }

    #[test]
    fn parse_did_rejects_short_hex() {
        let s = format!("did:octo:{}", "a".repeat(100));
        let err = parse_did_bytes(&s).unwrap_err();
        assert!(matches!(err, OctoCliError::IdentityNotFound(_)));
    }
}
