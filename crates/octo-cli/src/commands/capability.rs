//! `octo capability` — RFC-0011 §Subcommand Taxonomy CapabilityAction.
//!
//! Wave 3 implementation per mission `0011-capability-commands`.
//!
//! | Command                          | Read/Write | Exit codes                |
//! |----------------------------------|------------|---------------------------|
//! | `octo capability list`           | read       | 0, 2, 16, 64              |
//! | `octo capability mint`           | write      | 0, 2, 7, 8, 9, 11, 12, 64 |
//! | `octo capability attenuate <ID>` | write      | 0, 2, 7, 8, 10, 12, 64    |
//!
//! Layer C/D orchestrator. Every capability primitive is consumed from the
//! Layer B substrate (`octo_cap_macaroon`): the caveat catalog
//! ([`Caveat`] — 27 RFC-0964 variants), the canonical caveat encoding
//! ([`Caveat::canonical_ser`], RFC-0126 deterministic JSON), the
//! narrowing predicate ([`set_subsumes`]), and the three CLI facade free
//! functions (`list_active` / `mint` / `attenuate`). The CLI defines **no**
//! caveat variants of its own — adding one requires an RFC-0011 amendment
//! per mission §Caveat catalog consumed.
//!
//! `holder_sig` never reaches stdout: it is wrapped in [`RedactedHex`],
//! which serializes to `[REDACTED:sig]` unconditionally. `body_hash` is a
//! public digest and is rendered as plain lowercase hex.

use clap::Subcommand;
use serde::Serialize;

use octo_cap_macaroon::{
    blake3_hash, set_subsumes, CapabilityToken, Caveat, CaveatName, CompositeCapabilityCatalog,
    MintError,
};

use crate::error::OctoCliError;
use crate::output::OutputEnvelope;
use crate::redact::{redact_string, RedactedHex};
use crate::Octo;

// ---------------------------------------------------------------------------
// Parser clamps — RFC-0011 §Caveat Catalog
// ---------------------------------------------------------------------------

/// Maximum accepted `--caveats` payload size.
const MAX_CAVEAT_JSON_BYTES: usize = 64 * 1024;
/// Maximum accepted JSON nesting depth inside `--caveats`.
const MAX_CAVEAT_JSON_DEPTH: usize = 32;
/// Maximum number of caveats in a single `--caveats` payload.
const MAX_CAVEATS: usize = 16;
/// Maximum serialized size of any single caveat in `--caveats`.
const MAX_CAVEAT_PAYLOAD_BYTES: usize = 4 * 1024;

/// Domain prefix of every [`CaveatName::as_str`] wire identifier. Stripped
/// to recover the short serde tag when a caveat cannot be canonicalized.
const CAVEAT_NAME_PREFIX: &str = "cipherocto/cap/v1/caveat/";

/// Placeholder `cap_id` for `--dry-run` previews. The real identifier is
/// `BLAKE3(CAPABILITY_ID_DOMAIN || canonical_ser_unsigned(macaroon))`, which
/// depends on the root secret — unavailable (by design) on a preview path
/// that must not touch the signing key.
const PREVIEW_CAP_ID: &str = "(preview)";

/// Filter fields accepted by `octo capability list --filter`.
const FILTER_FIELDS: [&str; 3] = ["cap_id", "root_id", "caveat"];

/// Length in hex characters of a 32-byte capability identifier.
const CAP_ID_HEX_LEN: usize = 64;

/// Canonical DID method prefix (RFC-0010 form).
const DID_PREFIX: &str = "did:octo:";

// ---------------------------------------------------------------------------
// Clap surface — RFC-0011 §Subcommand Taxonomy CapabilityAction table
// ---------------------------------------------------------------------------

/// Capability subcommands.
#[derive(Subcommand, Debug)]
pub enum CapabilityAction {
    /// List active capabilities.
    List {
        /// Filter as `field=value` (repeatable). Accepted fields:
        /// `cap_id`, `root_id`, `caveat`.
        #[arg(long)]
        filter: Vec<String>,
    },
    /// Mint a new capability.
    Mint {
        /// Caveat expression.
        #[arg(long)]
        caveats: String,
        /// Holder DID.
        #[arg(long)]
        holder: String,
        /// Root capability identifier.
        #[arg(long)]
        root: Option<String>,
        /// Acknowledge that minting grants authority.
        #[arg(long, requires = "confirm")]
        confirm_acknowledge: bool,
    },
    /// Attenuate an existing capability.
    Attenuate {
        /// Parent capability identifier.
        cap_id: String,
        /// Additional caveats to apply.
        #[arg(long)]
        caveats: String,
        /// Acknowledge that attenuation issues a new capability.
        #[arg(long, requires = "confirm")]
        confirm_acknowledge: bool,
    },
}

// ---------------------------------------------------------------------------
// Output structs — RFC-0011 §Subcommand Taxonomy entries #8/#9
// ---------------------------------------------------------------------------

/// `octo capability list` payload.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct CapabilityListOutput {
    /// Active capabilities held by the signed-in identity, post-filter.
    pub capabilities: Vec<CapabilitySummaryView>,
}

/// CLI projection of the substrate `CapabilitySummary`.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct CapabilitySummaryView {
    /// Capability identifier (truncated hex, per substrate list view).
    pub cap_id: String,
    /// Macaroon root identifier (truncated hex, per substrate list view).
    pub root_id: String,
    /// Caveat chain in attenuation order.
    pub caveats: Vec<CaveatSummaryView>,
    /// Remaining budget, when a budget caveat is present.
    pub remaining_budget: Option<u64>,
    /// Expiry timestamp, when an expiry caveat is present.
    pub expires_at_unix: Option<i64>,
}

/// CLI projection of the substrate `CaveatSummary`.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct CaveatSummaryView {
    /// Short serde tag of the caveat (`before`, `model`, `amount_max`, ...).
    pub kind: String,
    /// Caveat payload in RFC-0964 canonical form.
    pub body: serde_json::Value,
}

/// `octo capability mint` payload.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct CapabilityMintOutput {
    /// Minted capability identifier (lowercase hex), or `(preview)` on a
    /// `--dry-run` invocation.
    pub cap_id: String,
    /// Lowercase-hex BLAKE3 digest over the canonical caveat body. Public —
    /// deliberately NOT wrapped in [`RedactedHex`].
    pub body_hash: String,
    /// Caveats bound into the capability.
    pub caveats: Vec<CaveatSummaryView>,
    /// Holder Ed25519 signature — always rendered as `[REDACTED:sig]`.
    #[schemars(with = "String")]
    pub holder_sig_hex: RedactedHex,
}

/// `octo capability attenuate` payload.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct CapabilityAttenuateOutput {
    /// Identifier of the newly issued child capability, or `(preview)` on a
    /// `--dry-run` invocation.
    pub child_cap_id: String,
    /// `cap_id` of the parent the child was narrowed from.
    pub narrowed_from: String,
    /// Caveats appended by this attenuation.
    pub caveats: Vec<CaveatSummaryView>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `octo capability list` — enumerate active capabilities.
///
/// Read-only, no side effects. Filters are validated before the wallet is
/// opened so a malformed `--filter` fails fast with `InvalidFilter`
/// (exit 16) rather than after an identity lookup.
pub fn list(filters: &[String], cli: &Octo) -> Result<(), OctoCliError> {
    let filters = parse_filters(filters)?;
    let store = octo_wallet::WalletStore::open()
        .map_err(|e| OctoCliError::Internal(format!("wallet store open: {e}")))?;
    let key = octo_wallet::active_identity(&store).map_err(|e| match e {
        octo_wallet::WalletError::NotActive { .. } => OctoCliError::NoActiveIdentity,
        other => OctoCliError::Internal(other.to_string()),
    })?;
    let summaries = octo_cap_macaroon::list_active(&key)
        .map_err(|e| OctoCliError::Internal(sanitize_mint_error(&e)))?;
    let capabilities: Vec<CapabilitySummaryView> = summaries
        .into_iter()
        .map(|s| CapabilitySummaryView {
            cap_id: s.cap_id,
            root_id: s.root_id,
            caveats: s
                .caveats
                .into_iter()
                .map(|c| CaveatSummaryView {
                    kind: c.kind,
                    body: c.body,
                })
                .collect(),
            remaining_budget: s.remaining_budget,
            expires_at_unix: s.expires_at_unix,
        })
        .filter(|v| matches_filters(v, &filters))
        .collect();
    let env = OutputEnvelope::new(CapabilityListOutput { capabilities }, 0);
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| OctoCliError::Internal(format!("render envelope: {e}")))
}

/// `octo capability mint` — issue a new capability to `holder_did`.
///
/// Gate order (each stage is reachable independently of the ones after it,
/// so every exit code in the RFC-0011 table is observable):
/// 1. confirmation + acknowledgement gates → exit 2
/// 2. caveat parse + catalog clamps → exit 7
/// 3. holder DID form → exit 9
/// 4. `--root` form → exit 12
/// 5. `--dry-run` short-circuit → exit 0, `preview_only: true`
/// 6. active identity → exit 2
/// 7. substrate mint → exit 7 / 8 / 11 / 64
pub fn mint(
    caveats_json: &str,
    holder_did: &str,
    root: Option<&str>,
    acknowledge: bool,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    super::identity::require_confirm(cli, "capability mint")?;
    require_acknowledge(cli, acknowledge, "capability mint")?;
    let caveats = parse_caveats(caveats_json)?;
    validate_holder_did(holder_did)?;
    if let Some(root_id) = root {
        validate_cap_id(root_id)?;
    }

    let views = caveat_views(&caveats);
    let body_hash = hex::encode(caveat_body_hash(&caveats));

    // `--dry-run` must not reach the signing key: previews are rendered
    // straight from the validated caveat set. No wallet open, no HSM touch,
    // no substrate mutation.
    if cli.mode.dry_run {
        let output = CapabilityMintOutput {
            cap_id: PREVIEW_CAP_ID.to_string(),
            body_hash,
            caveats: views,
            holder_sig_hex: RedactedHex(Vec::new()),
        };
        return OutputEnvelope::preview_only(output, 0)
            .render(cli.output.json, cli.output.no_color)
            .map_err(|e| OctoCliError::Internal(format!("render envelope: {e}")));
    }

    let store = octo_wallet::WalletStore::open()
        .map_err(|e| OctoCliError::Internal(format!("wallet store open: {e}")))?;
    let key = octo_wallet::active_identity(&store).map_err(|e| match e {
        octo_wallet::WalletError::NotActive { .. } => OctoCliError::NoActiveIdentity,
        other => OctoCliError::Internal(other.to_string()),
    })?;

    // Root-secret derivation is a substrate concern (RFC-0957 §Root Secret).
    // `octo_cap_macaroon::mint` is a Phase-1 facade stub that rejects every
    // input, so no secret is consumed on this path today. The placeholder is
    // replaced by the wallet-side derivation when the Phase-2 substrate
    // amendment lands (mission §Layer direction).
    let root_secret = [0u8; 32];
    let token = octo_cap_macaroon::mint(&root_secret, &key, holder_did, &caveats)
        .map_err(map_mint_error)?;

    let output = CapabilityMintOutput {
        cap_id: hex::encode(token.macaroon.id),
        body_hash,
        caveats: views,
        holder_sig_hex: RedactedHex(token.holder_sig.to_bytes().to_vec()),
    };
    OutputEnvelope::new(output, 0)
        .render(cli.output.json, cli.output.no_color)
        .map_err(|e| OctoCliError::Internal(format!("render envelope: {e}")))
}

/// `octo capability attenuate <cap_id>` — narrow an existing capability.
///
/// Gate order:
/// 1. confirmation + acknowledgement gates → exit 2
/// 2. caveat parse + catalog clamps → exit 7
/// 3. `cap_id` form → exit 12
/// 4. `--dry-run` short-circuit → exit 0, `preview_only: true`
/// 5. parent lookup → exit 12
/// 6. narrowing check → exit 10
/// 7. substrate attenuate → exit 7 / 8 / 10 / 64
pub fn attenuate(
    cap_id: &str,
    caveats_json: &str,
    acknowledge: bool,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    super::identity::require_confirm(cli, "capability attenuate")?;
    require_acknowledge(cli, acknowledge, "capability attenuate")?;
    let caveats = parse_caveats(caveats_json)?;
    validate_cap_id(cap_id)?;

    let views = caveat_views(&caveats);

    if cli.mode.dry_run {
        let output = CapabilityAttenuateOutput {
            child_cap_id: PREVIEW_CAP_ID.to_string(),
            narrowed_from: cap_id.to_string(),
            caveats: views,
        };
        return OutputEnvelope::preview_only(output, 0)
            .render(cli.output.json, cli.output.no_color)
            .map_err(|e| OctoCliError::Internal(format!("render envelope: {e}")));
    }

    let parent = resolve_parent(cap_id)?;
    check_attenuation(&parent, &caveats)?;

    let store = octo_wallet::WalletStore::open()
        .map_err(|e| OctoCliError::Internal(format!("wallet store open: {e}")))?;
    let key = octo_wallet::active_identity(&store).map_err(|e| match e {
        octo_wallet::WalletError::NotActive { .. } => OctoCliError::NoActiveIdentity,
        other => OctoCliError::Internal(other.to_string()),
    })?;
    let catalog = resolve_catalog()?;

    let child = octo_cap_macaroon::attenuate(&parent, &caveats, &key, &catalog)
        .map_err(map_attenuate_error)?;

    let output = CapabilityAttenuateOutput {
        child_cap_id: hex::encode(child.macaroon.id),
        narrowed_from: cap_id.to_string(),
        caveats: views,
    };
    OutputEnvelope::new(output, 0)
        .render(cli.output.json, cli.output.no_color)
        .map_err(|e| OctoCliError::Internal(format!("render envelope: {e}")))
}

// ---------------------------------------------------------------------------
// Caveat parsing — RFC-0964 envelope
// ---------------------------------------------------------------------------

/// Parse `--caveats` into the substrate caveat catalog.
///
/// Accepts either a single RFC-0964 caveat object
/// (`{"type": "before", "value": 1700000000}`) or an array of them. The
/// serde shape is owned by `octo_cap_macaroon::caveat::Caveat`
/// (`#[serde(tag = "type", content = "value")]`) — the CLI never mirrors
/// the tag table, so a substrate caveat addition is picked up for free.
///
/// Clamps per RFC-0011 §Caveat Catalog: ≤ 64 KiB total, ≤ 32 JSON levels,
/// ≤ 16 caveats, ≤ 4 KiB per caveat. Every diagnostic passes through the
/// redactor so a rejected payload cannot echo secret material verbatim.
pub fn parse_caveats(s: &str) -> Result<Vec<Caveat>, OctoCliError> {
    if s.len() > MAX_CAVEAT_JSON_BYTES {
        return Err(caveat_parse_error(format!(
            "caveat payload is {} bytes, limit is {MAX_CAVEAT_JSON_BYTES}",
            s.len()
        )));
    }
    let json: serde_json::Value =
        serde_json::from_str(s).map_err(|e| caveat_parse_error(format!("invalid JSON: {e}")))?;
    let depth = json_depth(&json);
    if depth > MAX_CAVEAT_JSON_DEPTH {
        return Err(caveat_parse_error(format!(
            "caveat JSON nests {depth} levels, limit is {MAX_CAVEAT_JSON_DEPTH}"
        )));
    }
    let items: Vec<serde_json::Value> = match json {
        serde_json::Value::Array(items) => items,
        obj @ serde_json::Value::Object(_) => vec![obj],
        _ => {
            return Err(caveat_parse_error(
                "caveats must be a JSON object or an array of JSON objects".to_string(),
            ))
        }
    };
    if items.len() > MAX_CAVEATS {
        return Err(caveat_parse_error(format!(
            "{} caveats supplied, limit is {MAX_CAVEATS}",
            items.len()
        )));
    }
    let mut out = Vec::with_capacity(items.len());
    for (idx, item) in items.into_iter().enumerate() {
        let payload_len = serde_json::to_vec(&item).map(|v| v.len()).unwrap_or(0);
        if payload_len > MAX_CAVEAT_PAYLOAD_BYTES {
            return Err(caveat_parse_error(format!(
                "caveat #{idx} is {payload_len} bytes, limit is {MAX_CAVEAT_PAYLOAD_BYTES}"
            )));
        }
        let caveat: Caveat = serde_json::from_value(item)
            .map_err(|e| caveat_parse_error(format!("caveat #{idx}: {e}")))?;
        // Round-trip through the canonical form (RFC-0126 deterministic
        // JSON) so a shape that deserializes but does not canonicalize is
        // rejected at the CLI boundary rather than inside the HMAC chain.
        serde_json::from_slice::<serde_json::Value>(&caveat.canonical_ser())
            .map_err(|e| caveat_parse_error(format!("caveat #{idx} canonical form: {e}")))?;
        out.push(caveat);
    }
    Ok(out)
}

/// Build a [`OctoCliError::CaveatParse`] with the diagnostic redacted.
fn caveat_parse_error(message: String) -> OctoCliError {
    OctoCliError::CaveatParse {
        message: redact_string(&message).into_owned(),
    }
}

/// Maximum nesting depth of a JSON value. Iterative — a recursive walk
/// would inherit the input's depth as stack depth.
fn json_depth(v: &serde_json::Value) -> usize {
    let mut max = 0usize;
    let mut stack = vec![(v, 1usize)];
    while let Some((node, depth)) = stack.pop() {
        max = max.max(depth);
        match node {
            serde_json::Value::Array(items) => {
                stack.extend(items.iter().map(|i| (i, depth + 1)));
            }
            serde_json::Value::Object(map) => {
                stack.extend(map.values().map(|i| (i, depth + 1)));
            }
            _ => {}
        }
    }
    max
}

/// Project a caveat into its CLI summary form via the canonical encoding.
pub fn caveat_view(c: &Caveat) -> CaveatSummaryView {
    let canonical: serde_json::Value =
        serde_json::from_slice(&c.canonical_ser()).unwrap_or(serde_json::Value::Null);
    let kind = canonical
        .get("type")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| short_caveat_tag(c.name()).to_owned());
    let body = canonical
        .get("value")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    CaveatSummaryView { kind, body }
}

/// Project a caveat slice into CLI summary form.
fn caveat_views(caveats: &[Caveat]) -> Vec<CaveatSummaryView> {
    caveats.iter().map(caveat_view).collect()
}

/// Short serde tag of a caveat name (`cipherocto/cap/v1/caveat/before` →
/// `before`).
fn short_caveat_tag(name: CaveatName) -> &'static str {
    name.as_str()
        .strip_prefix(CAVEAT_NAME_PREFIX)
        .unwrap_or(name.as_str())
}

/// Public BLAKE3 digest over the canonical caveat body. Deterministic for a
/// given caveat set per RFC-0126, and independent of the root secret — this
/// is what makes it safe to surface on the `--dry-run` preview path.
fn caveat_body_hash(caveats: &[Caveat]) -> [u8; 32] {
    let mut buf = Vec::new();
    for c in caveats {
        buf.extend_from_slice(&c.canonical_ser());
    }
    blake3_hash(&buf)
}

// ---------------------------------------------------------------------------
// Filter parsing
// ---------------------------------------------------------------------------

/// Parse `--filter field=value` pairs.
///
/// Rejects anything without a single `=`, an empty side, or a field outside
/// [`FILTER_FIELDS`] with `InvalidFilter` (exit 16).
pub fn parse_filters(raw: &[String]) -> Result<Vec<(String, String)>, OctoCliError> {
    let mut out = Vec::with_capacity(raw.len());
    for entry in raw {
        let Some((field, value)) = entry.split_once('=') else {
            return Err(OctoCliError::InvalidFilter(entry.clone()));
        };
        if field.is_empty() || value.is_empty() || !FILTER_FIELDS.contains(&field) {
            return Err(OctoCliError::InvalidFilter(entry.clone()));
        }
        out.push((field.to_owned(), value.to_owned()));
    }
    Ok(out)
}

/// Logical-AND over every supplied filter.
fn matches_filters(view: &CapabilitySummaryView, filters: &[(String, String)]) -> bool {
    filters.iter().all(|(field, value)| match field.as_str() {
        "cap_id" => view.cap_id == *value,
        "root_id" => view.root_id == *value,
        "caveat" => view.caveats.iter().any(|c| c.kind == *value),
        // Unreachable: `parse_filters` rejects unknown fields. Fail closed.
        _ => false,
    })
}

// ---------------------------------------------------------------------------
// Attenuation + substrate resolution
// ---------------------------------------------------------------------------

/// Reject an attenuation that would widen the parent's authority.
///
/// `child` is the set of caveats being **appended** — macaroon attenuation
/// is append-only, so the effective child set is `parent ∪ child` and a
/// caveat can never be dropped by this path. What remains to check is that
/// no appended caveat grants more than the parent already allows, which is
/// exactly [`set_subsumes`] (RFC-0957 §Attenuation Rules).
pub fn check_attenuation(parent: &CapabilityToken, child: &[Caveat]) -> Result<(), OctoCliError> {
    if set_subsumes(&parent.macaroon.caveats, child) {
        return Ok(());
    }
    let widened: Vec<&str> = child
        .iter()
        .filter(|c| !set_subsumes(&parent.macaroon.caveats, std::slice::from_ref(c)))
        .map(|c| short_caveat_tag(c.name()))
        .collect();
    Err(OctoCliError::AttenuationViolation(format!(
        "caveat(s) not implied by the parent: {}",
        widened.join(", ")
    )))
}

/// Resolve the parent capability named by `cap_id`.
///
/// v1.0: the CLI has no capability store. The substrate index behind
/// `octo_cap_macaroon::list_active` is a Phase-1 stub that reports an empty
/// active set, so no `cap_id` resolves and every lookup is
/// `ParentCapNotFound` (exit 12) per RFC-0011 §Exit Code table. Replaced by
/// a holder-registry read when the Phase-2 substrate amendment lands.
fn resolve_parent(cap_id: &str) -> Result<CapabilityToken, OctoCliError> {
    Err(OctoCliError::ParentCapNotFound(cap_id.to_string()))
}

/// Resolve the capability catalog backing attenuation.
///
/// v1.0: `CompositeCapabilityCatalog` needs a storage backend **and** a
/// gossip backend (RFC-0959 §Phase 3). Neither is wired into the operator
/// CLI, and fabricating a no-op gossip here would silently drop
/// buyer-notification envelopes. Fails closed until the catalog wiring
/// amendment lands.
fn resolve_catalog() -> Result<CompositeCapabilityCatalog, OctoCliError> {
    Err(OctoCliError::Internal(
        "capability catalog backend not wired".to_string(),
    ))
}

// ---------------------------------------------------------------------------
// Validation + error mapping
// ---------------------------------------------------------------------------

/// Reject a holder DID that is not in RFC-0010 canonical form.
///
/// Form-level only: whether a well-formed DID is *registered* is a holder
/// registry question the CLI defers to the substrate.
fn validate_holder_did(did: &str) -> Result<(), OctoCliError> {
    match did.strip_prefix(DID_PREFIX) {
        Some(suffix) if !suffix.is_empty() => Ok(()),
        _ => Err(OctoCliError::HolderNotFound(did.to_string())),
    }
}

/// Reject a capability identifier that is not 32 lowercase-hex bytes.
fn validate_cap_id(cap_id: &str) -> Result<(), OctoCliError> {
    let well_formed = cap_id.len() == CAP_ID_HEX_LEN
        && cap_id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if well_formed {
        Ok(())
    } else {
        Err(OctoCliError::ParentCapNotFound(cap_id.to_string()))
    }
}

/// Require `--confirm-acknowledge` alongside `--confirm` on mutating
/// capability commands (the atomic pastejacking gate, mission sub-step 6).
///
/// `--dry-run` bypasses it: a preview grants no authority.
fn require_acknowledge(cli: &Octo, acknowledge: bool, command: &str) -> Result<(), OctoCliError> {
    if cli.mode.dry_run || acknowledge {
        return Ok(());
    }
    Err(OctoCliError::ConfirmationRequired {
        command: command.to_string(),
    })
}

/// Strip substrate internals from a `MintError` before it reaches an
/// operator-visible message.
fn sanitize_mint_error(e: &MintError) -> String {
    crate::error::sanitize_substrate_error(&e.to_string())
}

/// Map a substrate mint failure onto the RFC-0011 exit-code table.
fn map_mint_error(e: MintError) -> OctoCliError {
    match e {
        MintError::InvalidCaveat(msg) => OctoCliError::CaveatParse {
            message: redact_string(&msg).into_owned(),
        },
        MintError::Signer(_) | MintError::HolderSig(_) => {
            OctoCliError::SigningFailed(sanitize_mint_error(&e))
        }
        MintError::Macaroon(_) => OctoCliError::Internal(sanitize_mint_error(&e)),
    }
}

/// Map a substrate attenuate failure onto the RFC-0011 exit-code table.
///
/// Differs from [`map_mint_error`] in the `InvalidCaveat` arm: on the
/// attenuate path a rejected caveat means the caveat set combines illegally
/// against the RFC-0960 catalog (exit 8), not that it failed to parse.
fn map_attenuate_error(e: MintError) -> OctoCliError {
    match e {
        MintError::InvalidCaveat(msg) => OctoCliError::InvalidCaveatCombination {
            detail: redact_string(&msg).into_owned(),
        },
        MintError::Signer(_) | MintError::HolderSig(_) => {
            OctoCliError::SigningFailed(sanitize_mint_error(&e))
        }
        MintError::Macaroon(_) => OctoCliError::AttenuationViolation(sanitize_mint_error(&e)),
    }
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

/// Route a `CapabilityAction` to its handler.
pub fn dispatch(action: &CapabilityAction, cli: &Octo) -> Result<(), OctoCliError> {
    match action {
        CapabilityAction::List { filter } => list(filter, cli),
        CapabilityAction::Mint {
            caveats,
            holder,
            root,
            confirm_acknowledge,
        } => mint(caveats, holder, root.as_deref(), *confirm_acknowledge, cli),
        CapabilityAction::Attenuate {
            cap_id,
            caveats,
            confirm_acknowledge,
        } => attenuate(cap_id, caveats, *confirm_acknowledge, cli),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_cap_macaroon::Dqa;

    fn caveat_json(c: &Caveat) -> String {
        serde_json::to_string(c).expect("caveat serializes")
    }

    /// TV-CAP1 (payload contract) — an empty active set renders as
    /// `"capabilities":[]` inside the envelope.
    #[test]
    fn tv_cap1_list_empty_payload() {
        let env = OutputEnvelope::new(
            CapabilityListOutput {
                capabilities: Vec::new(),
            },
            0,
        );
        let json = serde_json::to_string(&env).expect("envelope serializes");
        assert!(json.contains("\"capabilities\":[]"), "{json}");
        assert!(json.contains("\"schema_version\":2"), "{json}");
    }

    /// TV-CAP8 — malformed JSON is a parse error (exit 7), never a panic.
    #[test]
    fn tv_cap8_caveat_json_syntax_error() {
        let e = parse_caveats("{not_json").expect_err("must reject");
        assert_eq!(e.exit_code(), 7, "{e}");
    }

    #[test]
    fn caveat_scalar_payload_rejected() {
        let e = parse_caveats("42").expect_err("must reject");
        assert_eq!(e.exit_code(), 7, "{e}");
    }

    #[test]
    fn caveat_unknown_tag_rejected() {
        let e = parse_caveats(r#"{"type":"foo","value":1}"#).expect_err("must reject");
        assert_eq!(e.exit_code(), 7, "{e}");
    }

    #[test]
    fn caveat_empty_array_parses_to_empty_set() {
        assert!(parse_caveats("[]").expect("empty array parses").is_empty());
    }

    /// TV-CAP9 — budget caveat (`Caveat::AmountMax(Dqa)`, RFC-0964 canonical
    /// form). Scale is carried on the wire per mission §Scale-binding; the
    /// Dqa serde derives normalize to a single canonical (value, scale) per
    /// numeric value, so the constructor value pins the wire form.
    #[test]
    fn tv_cap9_caveat_budget() {
        let c = Caveat::AmountMax(Dqa::new(1, 3).expect("dqa"));
        let parsed = parse_caveats(&caveat_json(&c)).expect("budget caveat parses");
        assert!(matches!(parsed[0], Caveat::AmountMax(_)), "{parsed:?}");
        assert_eq!(caveat_view(&parsed[0]).kind, "amount_max");
    }

    /// TV-CAP10 — expiry caveat.
    #[test]
    fn tv_cap10_caveat_before() {
        let c = Caveat::Before(1_700_000_000);
        let parsed = parse_caveats(&caveat_json(&c)).expect("before caveat parses");
        assert_eq!(parsed, vec![c]);
        assert_eq!(caveat_view(&parsed[0]).kind, "before");
    }

    /// TV-CAP11 — vesting caveat.
    #[test]
    fn tv_cap11_caveat_valid_after() {
        let c = Caveat::ValidAfter {
            not_before_unix: 1_700_000_000,
        };
        let parsed = parse_caveats(&caveat_json(&c)).expect("valid_after caveat parses");
        assert_eq!(parsed, vec![c]);
        assert_eq!(caveat_view(&parsed[0]).kind, "valid_after");
    }

    /// TV-CAP12 — max-uses caveat (single-use is `count = 1`).
    #[test]
    fn tv_cap12_caveat_max_uses() {
        let c = Caveat::MaxUses { count: 1 };
        let parsed = parse_caveats(&caveat_json(&c)).expect("max_uses caveat parses");
        assert_eq!(parsed, vec![c]);
        assert_eq!(caveat_view(&parsed[0]).kind, "max_uses");
    }

    /// TV-CAP13 — model caveat.
    #[test]
    fn tv_cap13_caveat_model() {
        let c = Caveat::Model("gpt-4".to_owned());
        let parsed = parse_caveats(&caveat_json(&c)).expect("model caveat parses");
        assert_eq!(parsed, vec![c]);
        assert_eq!(caveat_view(&parsed[0]).body, serde_json::json!("gpt-4"));
    }

    /// TV-CAP14 — provider caveat (bare array, any-of).
    #[test]
    fn tv_cap14_caveat_provider() {
        let c = Caveat::Provider(vec!["openai".to_owned(), "anthropic".to_owned()]);
        let parsed = parse_caveats(&caveat_json(&c)).expect("provider caveat parses");
        assert_eq!(parsed, vec![c]);
        // `canonical_ser` sorts the provider list for HMAC stability.
        assert_eq!(
            caveat_view(&parsed[0]).body,
            serde_json::json!(["anthropic", "openai"])
        );
    }

    /// TV-CAP15 — audit-window caveat.
    #[test]
    fn tv_cap15_caveat_audit_window() {
        let c = Caveat::AuditWindow { duration_secs: 0 };
        let parsed = parse_caveats(&caveat_json(&c)).expect("audit_window caveat parses");
        assert_eq!(parsed, vec![c]);
        assert_eq!(caveat_view(&parsed[0]).kind, "audit_window");
    }

    /// Multiple caveats compose with logical-AND semantics.
    #[test]
    fn caveat_array_parses_all_entries() {
        let json = format!(
            "[{},{}]",
            caveat_json(&Caveat::Before(1)),
            caveat_json(&Caveat::MaxUses { count: 2 })
        );
        assert_eq!(parse_caveats(&json).expect("array parses").len(), 2);
    }

    #[test]
    fn caveat_count_clamp_enforced() {
        let one = caveat_json(&Caveat::Before(1));
        let json = format!(
            "[{}]",
            std::iter::repeat_n(one.as_str(), MAX_CAVEATS + 1)
                .collect::<Vec<_>>()
                .join(",")
        );
        let e = parse_caveats(&json).expect_err("must reject");
        assert_eq!(e.exit_code(), 7, "{e}");
    }

    #[test]
    fn caveat_byte_clamp_enforced() {
        let json = "0".repeat(MAX_CAVEAT_JSON_BYTES + 1);
        let e = parse_caveats(&json).expect_err("must reject");
        assert_eq!(e.exit_code(), 7, "{e}");
    }

    #[test]
    fn caveat_depth_clamp_enforced() {
        let depth = MAX_CAVEAT_JSON_DEPTH + 2;
        let json = format!("{}{}", "[".repeat(depth), "]".repeat(depth));
        let e = parse_caveats(&json).expect_err("must reject");
        assert_eq!(e.exit_code(), 7, "{e}");
    }

    #[test]
    fn json_depth_counts_nesting() {
        assert_eq!(json_depth(&serde_json::json!(1)), 1);
        assert_eq!(json_depth(&serde_json::json!([1])), 2);
        assert_eq!(json_depth(&serde_json::json!({"a": {"b": 1}})), 3);
    }

    /// TV-CAP16 — `--filter field=value` names no known field → exit 16.
    #[test]
    fn tv_cap16_filter_parsing() {
        let e = parse_filters(&["field=value".to_owned()]).expect_err("must reject");
        assert_eq!(e.exit_code(), 16, "{e}");
        let e = parse_filters(&["cap_id".to_owned()]).expect_err("must reject");
        assert_eq!(e.exit_code(), 16, "{e}");
        let e = parse_filters(&["cap_id=".to_owned()]).expect_err("must reject");
        assert_eq!(e.exit_code(), 16, "{e}");
        let ok = parse_filters(&["cap_id=abcd".to_owned(), "caveat=before".to_owned()])
            .expect("well-formed filters parse");
        assert_eq!(ok.len(), 2);
    }

    #[test]
    fn filters_compose_with_logical_and() {
        let view = CapabilitySummaryView {
            cap_id: "abcd".to_owned(),
            root_id: "ef01".to_owned(),
            caveats: vec![caveat_view(&Caveat::Before(1))],
            remaining_budget: None,
            expires_at_unix: None,
        };
        let f = parse_filters(&["cap_id=abcd".to_owned(), "caveat=before".to_owned()]).unwrap();
        assert!(matches_filters(&view, &f));
        let f = parse_filters(&["cap_id=abcd".to_owned(), "caveat=model".to_owned()]).unwrap();
        assert!(!matches_filters(&view, &f));
    }

    /// TV-CAP7 — a holder DID outside the RFC-0010 form is exit 9.
    #[test]
    fn tv_cap7_holder_not_found() {
        assert_eq!(
            validate_holder_did("not-a-did")
                .expect_err("must reject")
                .exit_code(),
            9
        );
        assert_eq!(
            validate_holder_did(DID_PREFIX)
                .expect_err("must reject")
                .exit_code(),
            9
        );
        validate_holder_did("did:octo:zAbc").expect("canonical DID accepted");
    }

    /// TV-CAP5 — an unresolvable parent identifier is exit 12.
    #[test]
    fn tv_cap5_attenuate_parent_not_found() {
        assert_eq!(
            validate_cap_id("cap_test_id")
                .expect_err("must reject")
                .exit_code(),
            12
        );
        let well_formed = "ab".repeat(32);
        validate_cap_id(&well_formed).expect("64 hex chars accepted");
        assert_eq!(
            resolve_parent(&well_formed)
                .expect_err("no capability store in v1.0")
                .exit_code(),
            12
        );
    }

    /// The catalog backend fails closed rather than fabricating a no-op
    /// gossip sink.
    #[test]
    fn catalog_backend_fails_closed() {
        match resolve_catalog() {
            Ok(_) => panic!("catalog backend must not be wired in v1.0"),
            Err(e) => assert_eq!(e.exit_code(), 64, "{e}"),
        }
    }

    /// TV-CAP4 — a widening caveat set is rejected with exit 10.
    #[test]
    fn tv_cap4_attenuate_widens_rejected() {
        let parent = fixture_token(&[Caveat::Before(1_000)]);
        // Later deadline than the parent's → widening.
        let e = check_attenuation(&parent, &[Caveat::Before(2_000)]).expect_err("must reject");
        assert_eq!(e.exit_code(), 10, "{e}");
        assert!(e.to_string().contains("before"), "{e}");
    }

    /// Narrowing (earlier deadline) and appending an unrelated axis are both
    /// accepted.
    #[test]
    fn attenuation_narrowing_accepted() {
        let parent = fixture_token(&[Caveat::Before(1_000)]);
        check_attenuation(&parent, &[Caveat::Before(500)]).expect("narrower deadline accepted");
        check_attenuation(&parent, &[]).expect("empty append accepted");
    }

    /// Mint failures map onto the RFC-0011 exit-code table.
    #[test]
    fn mint_error_mapping() {
        assert_eq!(
            map_mint_error(MintError::InvalidCaveat("bad".into())).exit_code(),
            7
        );
        assert_eq!(
            map_mint_error(MintError::HolderSig("hsm gone".into())).exit_code(),
            11
        );
    }

    /// Attenuate failures map onto the RFC-0011 exit-code table — the
    /// `InvalidCaveat` arm is exit 8 here, not exit 7.
    #[test]
    fn attenuate_error_mapping() {
        assert_eq!(
            map_attenuate_error(MintError::InvalidCaveat("bad".into())).exit_code(),
            8
        );
        assert_eq!(
            map_attenuate_error(MintError::HolderSig("hsm gone".into())).exit_code(),
            11
        );
    }

    /// `holder_sig` must never reach stdout in any form.
    #[test]
    fn mint_output_redacts_holder_sig() {
        let output = CapabilityMintOutput {
            cap_id: "ab".repeat(32),
            body_hash: hex::encode(caveat_body_hash(&[Caveat::Before(1)])),
            caveats: caveat_views(&[Caveat::Before(1)]),
            holder_sig_hex: RedactedHex(vec![0xde; 64]),
        };
        let json = serde_json::to_string(&output).expect("serializes");
        assert!(json.contains("[REDACTED:sig]"), "{json}");
        assert!(!json.contains("dede"), "{json}");
    }

    /// `body_hash` is a public digest — deterministic and caveat-derived, so
    /// the `--dry-run` preview and the applied mint agree.
    #[test]
    fn body_hash_is_deterministic() {
        let a = caveat_body_hash(&[Caveat::Before(1), Caveat::MaxUses { count: 2 }]);
        let b = caveat_body_hash(&[Caveat::Before(1), Caveat::MaxUses { count: 2 }]);
        assert_eq!(a, b);
        assert_ne!(a, caveat_body_hash(&[Caveat::Before(2)]));
    }

    #[test]
    fn short_caveat_tag_strips_domain_prefix() {
        assert_eq!(short_caveat_tag(CaveatName::Before), "before");
        assert_eq!(short_caveat_tag(CaveatName::AmountMax), "amount_max");
    }

    /// Build a parent token for the attenuation checks without touching the
    /// wallet: only `macaroon.caveats` is read by `check_attenuation`.
    fn fixture_token(caveats: &[Caveat]) -> CapabilityToken {
        use octo_cap_macaroon::CapabilitySigner;

        struct FixtureSigner([u8; 32]);
        impl CapabilitySigner for FixtureSigner {
            fn sign(
                &self,
                msg: &[u8],
            ) -> Result<[u8; 64], octo_cap_macaroon::CapabilitySignerError> {
                let mut out = [0u8; 64];
                let h = blake3_hash(msg);
                out[..32].copy_from_slice(&h);
                out[32..].copy_from_slice(&h);
                Ok(out)
            }
            fn public_key_bytes(&self) -> [u8; 32] {
                self.0
            }
        }

        CapabilityToken::mint(
            &[0x11; 32],
            &FixtureSigner([0x22; 32]),
            "did:octo:zTest",
            caveats,
        )
        .expect("fixture token mints")
    }
}
