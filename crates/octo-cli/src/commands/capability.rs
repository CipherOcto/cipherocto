//! `octo capability` — RFC-0011 §Subcommand Taxonomy CapabilityAction.
//!
//! Wave 3 implementation per mission `0011-capability-commands`.
//!
//! | Command                          | Read/Write | Exit codes                |
//! |----------------------------------|------------|---------------------------|
//! | `octo capability list`           | read       | 0, 2, 16, 64              |
//! | `octo capability mint`           | write      | 0, 2, 5, 7, 8, 9, 11, 12, 64 |
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
use std::io::IsTerminal;

use octo_cap_macaroon::{
    blake3_hash, set_subsumes, CapabilityToken, Caveat, CaveatName, CompositeCapabilityCatalog,
    MintError,
};

use crate::error::{sanitize_substrate_error, OctoCliError};
use crate::output::{Hex32, OutputEnvelope};
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

/// Substrate amendment marker — a caveat payload that fails the substrate's
/// RFC-0960 catalog combination check (exit 8). The substrate amendment
/// (LAYER-01) removes `MintError::InvalidCaveat`; the CLI classifies by
/// inspecting the message prefix.
const SUBSTRATE_PARSE_MARKER: &str = "parse:";
const SUBSTRATE_CATALOG_MARKER: &str = "catalog:";

// ---------------------------------------------------------------------------
// Clap surface — RFC-0011 §Subcommand Taxonomy CapabilityAction table
// ---------------------------------------------------------------------------

/// Capability subcommands.
#[derive(Subcommand, Debug)]
pub enum CapabilityAction {
    /// List active capabilities.
    List {
        /// Filter as `field=value` (repeatable, comma-separated). Accepted
        /// fields: `cap_id`, `root_id`, `caveat`.
        #[arg(long, value_delimiter = ',')]
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
    /// Remaining budget, when a budget caveat is present. v1.0 always
    /// `None` (per RFC-0011 §`octo capability list` reduction note:
    /// `remaining_budget_dqa` deferred to the audit-window
    /// sub-amendment; substrate dropped storage dependency in Phase
    /// 2c-2). Reserved for the audit-window sub-amendment that
    /// surfaces the substrate's per-cap budget remaining.
    pub remaining_budget: Option<u64>,
    /// Expiry timestamp (Unix epoch seconds), when an expiry caveat is present.
    pub expires_at: Option<i64>,
}

/// CLI projection of the substrate `CaveatSummary`.
///
/// `kind` is a typed discriminator (`CaveatKind`) not a free-form string;
/// the substrate `CaveatName` enum is the source of truth (RFC-0964 +
/// RFC-0965 variants), and the CLI projects each variant into a `&'static
/// str` tag rather than reimplementing the table. Extension caveats
/// (substrate `Raw`) land as `CaveatKind::Custom` — fail-closed in the
/// UI layer if the operator wants strict tagging.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct CaveatSummaryView {
    /// Typed discriminator of the caveat (RFC-0964 short serde tag).
    pub kind: CaveatKind,
    /// Caveat payload in RFC-0964 canonical form, augmented with the
    /// scale annotation when a budget caveat is present.
    pub body: serde_json::Value,
}

/// Typed discriminator for CLI caveat summary view.
///
/// [ADD] amendment track — Layer C concession: this enum re-enumerates
/// the substrate `CaveatName` variants one-for-one so the CLI can attach
/// a stable `Display` tag and a `schemars::JsonSchema`. The follow-on
/// substrate amendment `CaveatKind::from(CaveatName)` (Layer B additive,
/// no breaking changes) lets the CLI consume the substrate's typed
/// discriminator directly; when that amendment lands this enum collapses
/// to a thin newtype. Until then, every substrate `Caveat` variant is
/// mapped here in [`CaveatKind::from_caveat`] and unknown wire names
/// fail-closed to [`CaveatKind::Custom`] (the substrate `Raw` arm),
/// preserving the fail-closed UI invariant.
///
/// Each variant carries the canonical RFC-0964 short serde tag via
/// [`Display`]; the CLI never re-encodes the tag table from free-form
/// strings. Adding a new substrate caveat variant is a `match` arm here
/// and a `Display` impl — no `body` parsing required.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CaveatKind {
    /// `Caveat::AmountMax` (budget cap, RFC-0964 §1. Constraint variant enumeration).
    AmountMax,
    /// `Caveat::PerAxisMax` (per-axis cap).
    PerAxisMax,
    /// `Caveat::Model` (allowed model).
    Model,
    /// `Caveat::Provider` (any-of provider list).
    Provider,
    /// `Caveat::Before` (Unix-time expiry).
    Before,
    /// `Caveat::Audience` (bound DID).
    Audience,
    /// `Caveat::RateLimit` (rate envelope).
    RateLimit,
    /// `Caveat::InvocationHashBind` (request body hash).
    InvocationHashBind,
    /// `Caveat::Jurisdiction` (whitelist).
    Jurisdiction,
    /// `Caveat::CacheStrategy` (cache policy).
    CacheStrategy,
    /// `Caveat::AskBinding` (specific ask id).
    AskBinding,
    /// `Caveat::ThirdParty` (discharge macaroon).
    ThirdParty,
    /// `Caveat::Raw` / unknown wire names (forward-compat).
    Raw,
    /// `Caveat::Vault` (RFC-0960 §2.2 Capability).
    Vault,
    /// `Caveat::Permission` (RFC-0965 §3.2 Permission (0x11)).
    Permission,
    /// `Caveat::ValidRange` (time range).
    ValidRange,
    /// `Caveat::MaxPerTx` (per-transaction cap).
    MaxPerTx,
    /// `Caveat::AuditWindow`.
    AuditWindow,
    /// `Caveat::MaxUses`.
    MaxUses,
    /// `Caveat::WrappedOnly`.
    WrappedOnly,
    /// `Caveat::Factory`.
    Factory,
    /// `Caveat::PolicyReference`.
    PolicyReference,
    /// `Caveat::ValidAfter`.
    ValidAfter,
    /// `Caveat::RedemptionContext`.
    RedemptionContext,
    /// `Caveat::Sharded`.
    Sharded,
    /// `Caveat::Payment` (RFC-0965 v2.1 §2 PaymentCaveat Specification).
    Payment,
    /// `Caveat::AssetBinding` (RFC-0965 v2.1 §5 PermissionKind Co-Bound Caveat).
    AssetBinding,
    /// Substrate-added variant not yet projected here (CLI unknown).
    Custom,
}

impl CaveatKind {
    /// Map a [`Caveat`] to its CLI [`CaveatKind`]. For `Caveat::Raw` the
    /// tag is derived from the wire name and the result is `Custom` when
    /// it does not match any known variant — the discriminator stays
    /// typed either way.
    pub fn from_caveat(c: &Caveat) -> Self {
        match c {
            Caveat::AmountMax(_) => Self::AmountMax,
            Caveat::PerAxisMax(_) => Self::PerAxisMax,
            Caveat::Model(_) => Self::Model,
            Caveat::Provider(_) => Self::Provider,
            Caveat::Before(_) => Self::Before,
            Caveat::Audience(_) => Self::Audience,
            Caveat::RateLimit(_) => Self::RateLimit,
            Caveat::InvocationHashBind(_) => Self::InvocationHashBind,
            Caveat::Jurisdiction(_) => Self::Jurisdiction,
            Caveat::CacheStrategy(_) => Self::CacheStrategy,
            Caveat::AskBinding(_) => Self::AskBinding,
            Caveat::ThirdParty(_) => Self::ThirdParty,
            Caveat::Raw(_) => Self::Raw,
            Caveat::Vault(_) => Self::Vault,
            Caveat::Permission(_) => Self::Permission,
            Caveat::ValidRange { .. } => Self::ValidRange,
            Caveat::MaxPerTx(_) => Self::MaxPerTx,
            Caveat::AuditWindow { .. } => Self::AuditWindow,
            Caveat::MaxUses { .. } => Self::MaxUses,
            Caveat::WrappedOnly { .. } => Self::WrappedOnly,
            Caveat::Factory(_) => Self::Factory,
            Caveat::PolicyReference { .. } => Self::PolicyReference,
            Caveat::ValidAfter { .. } => Self::ValidAfter,
            Caveat::RedemptionContext { .. } => Self::RedemptionContext,
            Caveat::Sharded { .. } => Self::Sharded,
            Caveat::Payment(_) => Self::Payment,
            Caveat::AssetBinding(_) => Self::AssetBinding,
            _ => Self::Custom,
        }
    }
}

impl std::fmt::Display for CaveatKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.tag())
    }
}

impl CaveatKind {
    /// Stable RFC-0964 short serde tag for this caveat variant.
    pub fn tag(self) -> &'static str {
        match self {
            Self::AmountMax => "amount_max",
            Self::PerAxisMax => "per_axis_max",
            Self::Model => "model",
            Self::Provider => "provider",
            Self::Before => "before",
            Self::Audience => "audience",
            Self::RateLimit => "rate_limit",
            Self::InvocationHashBind => "invocation_hash_bind",
            Self::Jurisdiction => "jurisdiction",
            Self::CacheStrategy => "cache_strategy",
            Self::AskBinding => "ask_binding",
            Self::ThirdParty => "third_party",
            Self::Raw => "raw",
            Self::Vault => "vault",
            Self::Permission => "permission",
            Self::ValidRange => "valid_range",
            Self::MaxPerTx => "max_per_tx",
            Self::AuditWindow => "audit_window",
            Self::MaxUses => "max_uses",
            Self::WrappedOnly => "wrapped_only",
            Self::Factory => "factory",
            Self::PolicyReference => "policy_reference",
            Self::ValidAfter => "valid_after",
            Self::RedemptionContext => "redemption_context",
            Self::Sharded => "sharded",
            Self::Payment => "payment",
            Self::AssetBinding => "asset_binding",
            Self::Custom => "custom",
        }
    }

    /// Parse a wire tag from the substrate `CapabilitySummary.kind`
    /// string. Unknown tags fail-closed to [`CaveatKind::Custom`] —
    /// the CLI never crashes on a forward-compat caveat the substrate
    /// doesn't yet surface in its typed table.
    pub fn from_tag(tag: &str) -> Self {
        match tag {
            "amount_max" => Self::AmountMax,
            "per_axis_max" => Self::PerAxisMax,
            "model" => Self::Model,
            "provider" => Self::Provider,
            "before" => Self::Before,
            "audience" => Self::Audience,
            "rate_limit" => Self::RateLimit,
            "invocation_hash_bind" => Self::InvocationHashBind,
            "jurisdiction" => Self::Jurisdiction,
            "cache_strategy" => Self::CacheStrategy,
            "ask_binding" => Self::AskBinding,
            "third_party" => Self::ThirdParty,
            "raw" => Self::Raw,
            "vault" => Self::Vault,
            "permission" => Self::Permission,
            "valid_range" => Self::ValidRange,
            "max_per_tx" => Self::MaxPerTx,
            "audit_window" => Self::AuditWindow,
            "max_uses" => Self::MaxUses,
            "wrapped_only" => Self::WrappedOnly,
            "factory" => Self::Factory,
            "policy_reference" => Self::PolicyReference,
            "valid_after" => Self::ValidAfter,
            "redemption_context" => Self::RedemptionContext,
            "sharded" => Self::Sharded,
            "payment" => Self::Payment,
            "asset_binding" => Self::AssetBinding,
            _ => Self::Custom,
        }
    }
}

impl serde::Serialize for CaveatKind {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.tag())
    }
}

/// `octo capability mint` payload.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct CapabilityMintOutput {
    /// Minted capability identifier (lowercase hex), or `(preview)` on a
    /// `--dry-run` invocation. RFC-0011 §Subcommand Taxonomy row
    /// `capability mint` names this `capability_id`.
    pub capability_id: String,
    /// Lowercase-hex BLAKE3 digest over the canonical caveat body. Public —
    /// deliberately NOT wrapped in [`RedactedHex`].
    pub body_hash: Hex32,
    /// Caveats bound into the capability.
    pub caveats: Vec<CaveatSummaryView>,
    /// Holder Ed25519 signature — always rendered as `[REDACTED:sig]` via
    /// the redaction layer (SEC-13). RFC-0011 §Subcommand Taxonomy row
    /// `capability mint` names this `holder_sig`.
    #[schemars(with = "String")]
    pub holder_sig: RedactedHex,
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
        .map_err(|e| map_capability_internal(format!("wallet store open: {e}")))?;
    let key = octo_wallet::active_identity(&store).map_err(|e| match e {
        octo_wallet::WalletError::NotActive { .. } => OctoCliError::NoActiveIdentity,
        other => map_capability_internal(other),
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
                    kind: CaveatKind::from_tag(&c.kind),
                    body: c.body,
                })
                .collect(),
            remaining_budget: s.remaining_budget,
            expires_at: s.expires_at_unix,
        })
        .filter(|v| matches_filters(v, &filters))
        .collect();
    let env = OutputEnvelope::new(CapabilityListOutput { capabilities }, 0);
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| map_capability_internal(format!("render envelope: {e}")))
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
/// 7. substrate mint → exit 5 / 7 / 8 / 11 / 64
pub fn mint(
    caveats_json: &str,
    holder_did: &str,
    root: Option<&str>,
    acknowledge: bool,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    super::identity::require_confirm(cli, "capability mint", std::io::stdin().is_terminal())?;
    require_acknowledge(cli, acknowledge, "capability mint")?;
    let caveats = parse_caveats(caveats_json)?;
    validate_holder_did(holder_did)?;
    if let Some(root_id) = root {
        validate_cap_id(root_id)?;
    }

    let views = caveat_views(&caveats);
    let body_hash_bytes = caveat_body_hash(&caveats);
    let body_hash = Hex32(body_hash_bytes);

    // Pastejacking defense (RFC-0011 §Subcommand Taxonomy entry #13):
    // echo the canonical caveat set + holder DID to stderr before any
    // signing operation. The operator (or paste-jacking detector) has a
    // last-mile chance to see what is about to be authorized. The echo
    // fires BEFORE the `--dry-run` short-circuit so previews still see
    // the canonical payload — the redactor stream is the contract, not
    // a preview-only side channel. (R1 review CORR-12 / Wave 5A finding:
    // an earlier draft claimed the echo lived after the dry-run gate;
    // the runtime test `tv_cap17b_mint_dry_run_stderr_echo` pins the
    // current, pre-gate behaviour.)
    eprintln!(
        "would mint: holder={}, caveats={}, root={}",
        holder_did,
        redact_string(
            &serde_json::to_string(&views).unwrap_or_else(|_| "<unprintable>".to_owned())
        ),
        root.unwrap_or("<wallet-root>")
    );

    // `--dry-run` must not reach the signing key: previews are rendered
    // straight from the validated caveat set. No wallet open, no HSM touch,
    // no substrate mutation.
    if cli.mode.dry_run {
        let output = CapabilityMintOutput {
            capability_id: PREVIEW_CAP_ID.to_string(),
            body_hash,
            caveats: views,
            holder_sig: RedactedHex(Vec::new()),
        };
        return OutputEnvelope::preview_only(output, 0)
            .render(cli.output.json, cli.output.no_color)
            .map_err(|e| map_capability_internal(format!("render envelope: {e}")));
    }

    // SEC-03: refuse to mint until the wallet-side root-secret
    // derivation substrate amendment lands. The Phase-1 facade accepts
    // but does not consume `root_secret`, so the `[0u8; 32]` placeholder
    // would let an attacker forge a known `cap_id`. The guard fires
    // BEFORE wallet open so a missing identity surfaces this error (exit
    // 64) rather than masking it as `NoActiveIdentity` (exit 2). The
    // single `return` is required because the `#[cfg(test)]` branch below
    // is the actual tail expression of this function.
    #[cfg(not(test))]
    {
        #[allow(clippy::needless_return)]
        return Err(OctoCliError::Internal(
            "root secret derivation not wired; defer until substrate amendment lands".to_string(),
        ));
    }

    #[cfg(test)]
    {
        #[allow(clippy::needless_return)]
        {
            let store = octo_wallet::WalletStore::open()
                .map_err(|e| map_capability_internal(format!("wallet store open: {e}")))?;
            let key = octo_wallet::active_identity(&store).map_err(|e| match e {
                octo_wallet::WalletError::NotActive { .. } => OctoCliError::NoActiveIdentity,
                other => map_capability_internal(other),
            })?;
            // Test surface only: synthetic root_secret kept so the
            // `fixture_token` helper can mint a synthetic parent for
            // attenuation checks. The hardening guard above makes this
            // unreachable from release builds.
            let root_secret = [0u8; 32];
            let _ = &root_secret;
            let token: CapabilityToken =
                octo_cap_macaroon::mint(&[0u8; 32], &key, holder_did, &caveats)
                    .map_err(map_mint_error)?;

            let output = CapabilityMintOutput {
                capability_id: hex::encode(token.macaroon.id),
                body_hash,
                caveats: views,
                holder_sig: RedactedHex(token.holder_sig.to_bytes().to_vec()),
            };
            return OutputEnvelope::new(output, 0)
                .render(cli.output.json, cli.output.no_color)
                .map_err(|e| map_capability_internal(format!("render envelope: {e}")));
        }
    }
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
    super::identity::require_confirm(cli, "capability attenuate", std::io::stdin().is_terminal())?;
    require_acknowledge(cli, acknowledge, "capability attenuate")?;
    let caveats = parse_caveats(caveats_json)?;
    validate_cap_id(cap_id)?;

    let views = caveat_views(&caveats);

    // Pastejacking defense (RFC-0011 §Subcommand Taxonomy entry #13):
    // echo the parent + canonical caveat set to stderr before any
    // signing operation. Like the mint handler, this echo fires BEFORE
    // the `--dry-run` short-circuit so previews still see the canonical
    // payload — see the long-form note on the mint handler for the
    // Wave 5A comment-drift correction. (The two handlers are kept
    // symmetric on purpose.)
    eprintln!(
        "would attenuate: narrowed_from={}, caveats={}",
        cap_id,
        redact_string(
            &serde_json::to_string(&views).unwrap_or_else(|_| "<unprintable>".to_owned())
        )
    );

    if cli.mode.dry_run {
        let output = CapabilityAttenuateOutput {
            child_cap_id: PREVIEW_CAP_ID.to_string(),
            narrowed_from: cap_id.to_string(),
            caveats: views,
        };
        return OutputEnvelope::preview_only(output, 0)
            .render(cli.output.json, cli.output.no_color)
            .map_err(|e| map_capability_internal(format!("render envelope: {e}")));
    }

    let parent = resolve_parent(cap_id)?;
    check_attenuation(&parent, &caveats)?;

    let store = octo_wallet::WalletStore::open()
        .map_err(|e| map_capability_internal(format!("wallet store open: {e}")))?;
    let key = octo_wallet::active_identity(&store).map_err(|e| match e {
        octo_wallet::WalletError::NotActive { .. } => OctoCliError::NoActiveIdentity,
        other => map_capability_internal(other),
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
        .map_err(|e| map_capability_internal(format!("render envelope: {e}")))
}

// ---------------------------------------------------------------------------
// Caveat parsing — RFC-0965 §2 Caveat envelope encoding
// ---------------------------------------------------------------------------

/// Parse `--caveats` into the substrate caveat catalog.
///
/// Accepts either a single RFC-0965 §2 caveat object
/// (`{"type": "before", "value": 1700000000}`) or an array of them. The
/// serde shape is owned by `octo_cap_macaroon::caveat::Caveat`
/// (`#[serde(tag = "type", content = "value")]`) — the CLI never mirrors
/// the tag table, so a substrate caveat addition is picked up for free.
///
/// ## Layer model — RFC-0011 §Subcommand Taxonomy entry #13
///
/// This is **Phase-1 scaffolding**. The RFC-0011 amendment tracks a
/// substrate `[ADD] caveat::validate_canonical_form` function (Layer B)
/// that owns parsing + canonical-form validation. When that amendment
/// lands, this body delegates to it and the CLI keeps only the operator
/// gate (byte/depth/count/serialized-size clamps + pastejacking echo).
/// No CLI-side body content is *unique* — every diagnostic here is
/// already codified substrate behaviour, the CLI is only the gate.
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
    let body_value = canonical
        .get("value")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let body = augment_budget_body(c, body_value);
    CaveatSummaryView {
        kind: CaveatKind::from_caveat(c),
        body,
    }
}

/// For `Caveat::AmountMax`, augment the JSON body with `{amount_dqa, scale}`
/// so the operator sees the scale annotation without doing the 16-byte
/// DqaEncoding decode manually. Prevents the 1M× widening bug where the
/// operator reads a raw `scale=0` payload as $X when the value is actually
/// $X·10^-scale. Non-budget caveats are returned unchanged.
fn augment_budget_body(c: &Caveat, body: serde_json::Value) -> serde_json::Value {
    let Caveat::AmountMax(dqa) = c else {
        return body;
    };
    let original = body;
    let value_field = match &original {
        serde_json::Value::Object(m) => m.get("value").cloned().unwrap_or(serde_json::Value::Null),
        other => other.clone(),
    };
    serde_json::json!({
        "amount_dqa": dqa.value,
        "scale": dqa.scale,
        "value": value_field,
    })
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
/// LAYER-06 Phase-1 concession: the CLI parses `--filter` strings here
/// rather than delegating to a substrate `CapabilityFilter::parse`
/// helper. The substrate amendment that surfaces a typed
/// `CapabilityFilter` parser (Layer B additive) lands in the follow-on
/// substrate move; until then, this routine is the canonical filter
/// parser and [`matches_filters`] is the canonical matcher. Both are
/// pure functions so the substrate-side move is a copy-without-change.
///
/// Rejects anything without a single `=`, an empty side, or a field outside
/// [`FILTER_FIELDS`] with `InvalidFilter` (exit 16). Clap's
/// `value_delimiter = ','` already splits `--filter foo,bar` into two
/// entries; this routine also tolerates entries that contain literal `,`
/// characters by re-splitting defensively (the substrate never emits
/// commas, so this is a no-op for well-formed clients).
pub fn parse_filters(raw: &[String]) -> Result<Vec<(String, String)>, OctoCliError> {
    let mut out = Vec::with_capacity(raw.len());
    for entry in raw {
        for part in split_filter_entry(entry) {
            let Some((field, value)) = part.split_once('=') else {
                return Err(OctoCliError::InvalidFilter(
                    crate::redact::redact_string(entry).into_owned(),
                ));
            };
            if field.is_empty() || value.is_empty() || !FILTER_FIELDS.contains(&field) {
                return Err(OctoCliError::InvalidFilter(
                    crate::redact::redact_string(entry).into_owned(),
                ));
            }
            out.push((field.to_owned(), value.to_owned()));
        }
    }
    Ok(out)
}

/// Split a single `--filter` entry by `,`. Empty splits are dropped so
/// `--filter foo=bar,,baz=qux` becomes two valid pairs.
fn split_filter_entry(entry: &str) -> Vec<String> {
    entry
        .split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Logical-AND over every supplied filter.
fn matches_filters(view: &CapabilitySummaryView, filters: &[(String, String)]) -> bool {
    filters.iter().all(|(field, value)| match field.as_str() {
        "cap_id" => view.cap_id == *value,
        "root_id" => view.root_id == *value,
        "caveat" => view.caveats.iter().any(|c| c.kind.to_string() == *value),
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
/// exactly [`set_subsumes`] (RFC-0957 §Attenuation Invariant).
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
/// Per RFC-0011 §Security 1a + Appendix A: when `--confirm` and a
/// complex-payload flag (e.g. `--caveats <json>`) are passed on the
/// SAME invocation, the CLI requires an additional `--confirm-acknowledge`
/// flag before proceeding. This breaks the paste-then-spray class of
/// attacks where a clipboard hijacker swaps the payload after the
/// operator has confirmed. The clap schema enforces the dependency via
/// `#[arg(requires = "confirm")]` on `--confirm-acknowledge`: clap
/// rejects any invocation where `--confirm-acknowledge` is set without
/// `--confirm` (exit 2). This dispatch-time check in `require_acknowledge`
/// is the second layer (clap-level dependency + semantic check) so
/// non-clap entry points (library callers, future RPC adapters) still
/// fail closed.
///
/// `--dry-run` bypasses it: a preview grants no authority.
// Second-step gate on `capability mint` / `capability attenuate`. The TTY
// pastejacking check lives in `require_confirm` (always invoked first on the
// same call sites); this function is the `--confirm-acknowledge` flag gate.
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

/// Build an `OctoCliError::Internal` whose text is sanitized for operator
/// consumption.
///
/// Mirrors `map_wallet_open_error` in `commands/identity.rs` (R1 review
/// SEC-11): every substrate error that reaches an `Internal(...)` site
/// here MUST go through `sanitize_substrate_error` so SQL markers,
/// `crates/octo-*` paths, and `src/` references never reach the operator
/// envelope. Use this helper at every `OctoCliError::Internal(format!(...))`
/// site in this module — Wave 5A + Wave 5B found ≥9 raw sites that leaked
/// substrate paths.
fn map_capability_internal(e: impl std::fmt::Display) -> OctoCliError {
    OctoCliError::Internal(sanitize_substrate_error(&e.to_string()))
}

/// Classify a substrate `MintError::HolderSig` message into the
/// RFC-0011 exit-code table.
///
/// Per substrate amendment LAYER-01, `MintError::InvalidCaveat` was
/// removed from the central enum and the CLI classifies the underlying
/// failure by inspecting the message prefix:
///
/// | Prefix     | Meaning                                                | Exit |
/// |------------|--------------------------------------------------------|------|
/// | `parse:`   | caveat payload failed serde/canonical-form validation  | 7    |
/// | `catalog:` | caveat set failed RFC-0960 catalog combination rules   | 8    |
/// | (other)    | unclassified substrate failure                         | 64   |
///
/// When `MintError::Signer` is surfaced (HSM transport failure), it
/// maps to `HsmUnavailable` (exit 5) — distinct from `HolderSig` which
/// maps to `SigningFailed` (exit 11) per CORR-05.
///
/// [ADD] amendment track — `MintError::classify() -> MintErrorKind`
/// substrate amendment (Layer B additive) lets the CLI dispatch on a
/// typed discriminant rather than parsing the message prefix. Until
/// that amendment lands, [`classify_message`] uses string-prefix
/// classification against `SUBSTRATE_PARSE_MARKER` / `SUBSTRATE_CATALOG_MARKER`
/// which is a Phase-1 concession: the markers are part of the
/// substrate's display contract, not its type contract, so a future
/// substrate reformat of the message string would silently re-route the
/// error class. The amendment pins the classification to the typed
/// discriminant and removes the prefix match.
fn classify_message(message: &str) -> OctoCliError {
    let redacted = redact_string(message).into_owned();
    let sanitized = sanitize_substrate_error(&redacted);
    if let Some(rest) = sanitized.strip_prefix(SUBSTRATE_PARSE_MARKER) {
        return OctoCliError::CaveatParse {
            message: rest.trim().to_owned(),
        };
    }
    if let Some(rest) = sanitized.strip_prefix(SUBSTRATE_CATALOG_MARKER) {
        return OctoCliError::InvalidCaveatCombination {
            detail: rest.trim().to_owned(),
        };
    }
    OctoCliError::Internal(sanitized)
}

/// Map a substrate mint failure onto the RFC-0011 exit-code table.
///
/// Substrate amendment LAYER-01 removes the `MintError::InvalidCaveat`
/// variant; the CLI classifies by message prefix (see
/// [`classify_message`]). Until the substrate amendment that emits
/// `parse:` / `catalog:` prefixes lands, every `HolderSig` message
/// surfaces as `Internal` (exit 64) rather than crashing or being
/// miscategorized.
#[cfg(test)]
fn map_mint_error(e: MintError) -> OctoCliError {
    match e {
        MintError::Signer(_) => OctoCliError::HsmUnavailable(sanitize_mint_error(&e)),
        MintError::HolderSig(msg) => classify_message(&msg),
        MintError::Macaroon(msg) => {
            // Macaroon-level failures are internal substrate pathology;
            // surface as `Internal` after sanitization.
            OctoCliError::Internal(sanitize_mint_error(&MintError::HolderSig(msg.to_string())))
        }
    }
}

/// Map a substrate attenuate failure onto the RFC-0011 exit-code table.
///
/// Differs from [`map_mint_error`] in the message-prefix classification:
/// on the attenuate path, a `parse:` prefix is also exit 7 (a malformed
/// caveat cannot become valid via attenuation), but `catalog:` rules
/// distinguishing widening from a fresh combination failure happen at
/// the substrate layer and surface here as exit 8. The Macaroon arm is
/// routed to `AttenuationViolation` (exit 10) so the attenuation gate
/// is reachable even when the substrate raises a substrate-level Macaroon
/// error.
fn map_attenuate_error(e: MintError) -> OctoCliError {
    match e {
        MintError::Signer(_) => OctoCliError::HsmUnavailable(sanitize_mint_error(&e)),
        MintError::HolderSig(msg) => classify_message(&msg),
        MintError::Macaroon(msg) => OctoCliError::AttenuationViolation(sanitize_mint_error(
            &MintError::HolderSig(msg.to_string()),
        )),
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

    /// TV-CAP9 — budget caveat (`Caveat::AmountMax(Dqa)`, RFC-0965 §2
    /// canonical form). Scale is carried on the wire per mission §Scale-binding; the
    /// Dqa serde derives normalize to a single canonical (value, scale) per
    /// numeric value, so the constructor value pins the wire form.
    /// CORR-14: the CLI summary view augments the body with
    /// `{amount_dqa, scale}` to prevent the 1M× widening bug.
    #[test]
    fn tv_cap9_caveat_budget() {
        let c = Caveat::AmountMax(Dqa::new(1, 3).expect("dqa"));
        let parsed = parse_caveats(&caveat_json(&c)).expect("budget caveat parses");
        assert!(matches!(parsed[0], Caveat::AmountMax(_)), "{parsed:?}");
        let view = caveat_view(&parsed[0]);
        assert_eq!(view.kind, CaveatKind::AmountMax);
        // Scale annotation must surface so a downstream consumer can't
        // misread `value` as a whole-unit amount.
        assert_eq!(view.body["amount_dqa"], serde_json::json!(1));
        assert_eq!(view.body["scale"], serde_json::json!(3));
    }

    /// TV-CAP10 — expiry caveat.
    #[test]
    fn tv_cap10_caveat_before() {
        let c = Caveat::Before(1_700_000_000);
        let parsed = parse_caveats(&caveat_json(&c)).expect("before caveat parses");
        assert_eq!(parsed, vec![c]);
        assert_eq!(caveat_view(&parsed[0]).kind, CaveatKind::Before);
    }

    /// TV-CAP11 — vesting caveat.
    #[test]
    fn tv_cap11_caveat_valid_after() {
        let c = Caveat::ValidAfter {
            not_before_unix: 1_700_000_000,
        };
        let parsed = parse_caveats(&caveat_json(&c)).expect("valid_after caveat parses");
        assert_eq!(parsed, vec![c]);
        assert_eq!(caveat_view(&parsed[0]).kind, CaveatKind::ValidAfter);
    }

    /// TV-CAP12 — max-uses caveat (single-use is `count = 1`).
    #[test]
    fn tv_cap12_caveat_max_uses() {
        let c = Caveat::MaxUses { count: 1 };
        let parsed = parse_caveats(&caveat_json(&c)).expect("max_uses caveat parses");
        assert_eq!(parsed, vec![c]);
        assert_eq!(caveat_view(&parsed[0]).kind, CaveatKind::MaxUses);
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
        assert_eq!(caveat_view(&parsed[0]).kind, CaveatKind::AuditWindow);
    }

    /// SPEC-15 — Audience caveat (binding DID).
    ///
    /// `Caveat::Audience("did:octo:abc...")` parses to the canonical
    /// envelope `{"type":"audience","value":"<id>"}` and surfaces
    /// `CaveatKind::Audience` in the CLI summary view.
    #[test]
    fn tv_cap_caveat_audience() {
        let c = Caveat::Audience("did:octo:zAudience".to_owned());
        let payload = serde_json::json!({
            "type": "audience",
            "value": "did:octo:zAudience",
        });
        let parsed = parse_caveats(&payload.to_string())
            .expect("audience caveat parses from canonical envelope");
        assert!(matches!(parsed[0], Caveat::Audience(_)), "{parsed:?}");
        assert_eq!(parsed, vec![c.clone()]);

        // Direct construction round-trip via serde_json.
        let direct =
            parse_caveats(&caveat_json(&c)).expect("audience caveat parses from native serde");
        assert_eq!(direct, vec![c]);

        // CLI summary view surfaces the typed discriminator + payload.
        let view = caveat_view(&direct[0]);
        assert_eq!(view.kind, CaveatKind::Audience);
        assert_eq!(view.kind.to_string(), "audience");
        assert_eq!(view.body, serde_json::json!("did:octo:zAudience"));
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
    /// CORR-09 — `--filter foo,bar` splits into two entries.
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

        // Comma-separated filters expand into two entries each.
        let csv = parse_filters(&["cap_id=abcd,caveat=before".to_owned()]).expect("csv parses");
        assert_eq!(csv.len(), 2);
        assert_eq!(csv[0].0, "cap_id");
        assert_eq!(csv[1].0, "caveat");
    }

    #[test]
    fn filters_compose_with_logical_and() {
        let view = CapabilitySummaryView {
            cap_id: "abcd".to_owned(),
            root_id: "ef01".to_owned(),
            caveats: vec![caveat_view(&Caveat::Before(1))],
            remaining_budget: None,
            expires_at: None,
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
    ///
    /// Substrate amendment LAYER-01 removes `MintError::InvalidCaveat`; the
    /// CLI classifies `HolderSig` messages by prefix. Until the substrate
    /// emits the `parse:` / `catalog:` markers, every HolderSig reaches
    /// `Internal` (exit 64).
    #[test]
    fn mint_error_mapping() {
        // CORR-05: Signer → HsmUnavailable (exit 5), not SigningFailed (11).
        let signer_err =
            octo_cap_macaroon::signer::CapabilitySignerError::Signer("hsm offline".to_owned());
        assert_eq!(map_mint_error(MintError::Signer(signer_err)).exit_code(), 5);

        // HolderSig without a marker → Internal (64).
        assert_eq!(
            map_mint_error(MintError::HolderSig("not yet wired".into())).exit_code(),
            64
        );

        // HolderSig with `parse:` prefix → CaveatParse (7).
        assert_eq!(
            map_mint_error(MintError::HolderSig("parse: bad caveat".into())).exit_code(),
            7
        );

        // HolderSig with `catalog:` prefix → InvalidCaveatCombination (8).
        assert_eq!(
            map_mint_error(MintError::HolderSig("catalog: conflicting".into())).exit_code(),
            8
        );
    }

    /// Attenuate failures map onto the RFC-0011 exit-code table.
    #[test]
    fn attenuate_error_mapping() {
        let signer_err =
            octo_cap_macaroon::signer::CapabilitySignerError::Signer("hsm offline".to_owned());
        assert_eq!(
            map_attenuate_error(MintError::Signer(signer_err)).exit_code(),
            5
        );
        assert_eq!(
            map_attenuate_error(MintError::HolderSig("parse: invalid".into())).exit_code(),
            7
        );
        assert_eq!(
            map_attenuate_error(MintError::HolderSig("catalog: widening".into())).exit_code(),
            8
        );
    }

    /// `holder_sig` must never reach stdout in any form.
    #[test]
    fn mint_output_redacts_holder_sig() {
        let output = CapabilityMintOutput {
            capability_id: "ab".repeat(32),
            body_hash: Hex32(caveat_body_hash(&[Caveat::Before(1)])),
            caveats: caveat_views(&[Caveat::Before(1)]),
            holder_sig: RedactedHex(vec![0xde; 64]),
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

    /// `Hex32` round-trips through Serialize → Deserialize as lowercase hex.
    #[test]
    fn hex32_serde_roundtrip() {
        let h = Hex32([0xab; 32]);
        let s = serde_json::to_string(&h).expect("serialize");
        assert_eq!(s, format!("\"{}\"", "ab".repeat(32)), "{s}");
        let back: Hex32 = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(back, h);
    }

    /// `CaveatKind` enumerates every substrate variant with a stable
    /// `Display` tag matching the RFC-0964 serde rename.
    #[test]
    fn caveat_kind_display_tags() {
        assert_eq!(CaveatKind::AmountMax.to_string(), "amount_max");
        assert_eq!(CaveatKind::Before.to_string(), "before");
        assert_eq!(CaveatKind::Audience.to_string(), "audience");
        assert_eq!(CaveatKind::MaxUses.to_string(), "max_uses");
        assert_eq!(CaveatKind::ValidAfter.to_string(), "valid_after");
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
