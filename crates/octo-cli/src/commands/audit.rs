//! `octo audit` — RFC-0011-a §Subcommand Taxonomy.
//!
//! Layer C/D orchestrator over the Layer-B `octo-audit` substrate
//! (`list_receipts`, `get_receipt`, `AuditFilter`, `audit_home`).
//! The handlers consume the substrate-faithful read surface per
//! RFC-0011-a §Substrate-truth dependency. Output renders through
//! `OutputEnvelope<T>` per RFC-0011 §Output Envelope.
//!
//! ## Substrate reality (Phase 1)
//!
//! `list_receipts(&AuditFilter) -> Vec<u64>` returns integer
//! `receipt_id` keys (not the typed `ReceiptId(pub u64)` newtype).
//! The CLI iterates each id via `get_receipt(&ReceiptId::new(id))`
//! and maps to `ReceiptSummary` via `from_canonical` per
//! RFC-0016-a §6.5 substrate projection. The typed newtype
//! migration (`ReceiptId(pub [u8; 32])`) is paired with
//! RFC-0014-v2 acceptance (deferred per mission §Substrate still
//! missing + RFC-0016-a §6.4).
//!
//! ## Operator-mode Auditor constraint
//!
//! RFC-0011-a §Security Considerations row 2: under `--mode
//! auditor`, an explicit `--status` filter is silently no-op'd so
//! reject rows can never be hidden from the auditor view. The CLI
//! enforces this at the dispatch boundary (NOT via the substrate)
//! per the audit amendment contract — the substrate-faithful path
//! always returns the FULL set when the filter is dropped.

#![allow(
    clippy::module_name_repetitions,
    reason = "intentional repetition for canonical CLI surface types: AuditAction / AuditListOutput / AuditShowOutput / ReceiptSummaryOutput mirror the substrate names verbatim so the CLI envelope reads substrate-faithfully (per RFC-0011-a §Output Envelope)."
)]

use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;
use serde::Serialize;

use crate::error::{sanitize_substrate_error, OctoCliError};
use crate::flags::OperatorMode;
use crate::output::OutputEnvelope;
use crate::Octo;

use octo_audit::{
    audit_home, get_receipt, list_receipts, AuditError, AuditFilter, ReceiptSummary, MAX_LIMIT,
};
use octo_settlement::{Receipt, ReceiptId, ReceiptStatus};

/// CLI-facing audit subcommand enum (Layer C/D). `#[non_exhaustive]`
/// per F-14 — future amendments add `Redact | Export | Watch`
/// variants per RFC-0011-a §Future Work without central enum edits.
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuditAction {
    /// List settlement receipts matching a typed filter set
    /// (RFC-0011-a §Filters).
    List(ListArgs),
    /// Point-lookup a settlement receipt by canonical `receipt_id`
    /// (RFC-0011-a §Subcommand Taxonomy `show` Args).
    Show(ShowArgs),
}

/// `octo audit list` arguments (RFC-0011-a §Filters + §Subcommand
/// Taxonomy `list`).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct ListArgs {
    /// Lower-bound duration (`<n>d|<n>h|<n>m|<n>s`) — converted to
    /// unix-seconds-from-now at the dispatch boundary. Per
    /// RFC-0011-a §Filters grammar.
    #[arg(long, value_parser = parse_duration_secs)]
    pub since: Option<u64>,
    /// Upper-bound duration (same grammar as `--since`).
    #[arg(long, value_parser = parse_duration_secs)]
    pub until: Option<u64>,
    /// Restrict to receipts whose 32-byte capability-root BLAKE3
    /// digest matches the supplied 64 lowercase hex chars (RFC-0011-a
    /// §Filters `capability-root`).
    #[arg(long, value_parser = parse_capability_root_hex)]
    pub capability_root: Option<[u8; 32]>,
    /// Restrict to one model identifier (exact match per RFC-0011-a
    /// §Filters `model`).
    #[arg(long)]
    pub model: Option<String>,
    /// Restrict to one or more `ReceiptStatus` values
    /// (`ok | partial | reject | unknown`; case-insensitive;
    /// multiple values OR'd per RFC-0011-a §Filters `status`).
    ///
    /// **Auditor-mode constraint:** silently no-op'd under
    /// `--mode auditor` (RFC-0011-a §Security Considerations row 2)
    /// so reject rows can never be hidden from the auditor view.
    #[arg(long, value_parser = parse_status, num_args = 1..)]
    pub status: Vec<ReceiptStatus>,
    /// Max rows returned; clamped to the substrate hard ceiling
    /// `MAX_LIMIT = 10_000` (RFC-0011-a §Filters `limit`).
    #[arg(long, value_parser = parse_limit)]
    pub limit: Option<u32>,
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// `octo audit show` arguments (RFC-0011-a §Subcommand Taxonomy
/// `show` Args).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct ShowArgs {
    /// Canonical `receipt_id` (decimal `u64` form per
    /// RFC-0016-a §6.4 — paired with `octo_settlement::ReceiptId`).
    #[arg(value_parser = parse_receipt_id)]
    pub receipt_id: ReceiptId,
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// Render payload for `octo audit list` (RFC-0011-a §Output Envelope
/// — declaration order canonical per RFC-0011 §Determinism
/// Requirements).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct AuditListOutput {
    /// Per-row summary projection (`ReceiptSummary` from
    /// RFC-0016-a §6.5). Sorted by `executed_at_unix DESC` with
    /// `receipt_id ASC` tiebreaker (substrate-faithful per
    /// `list_receipts`).
    pub receipts: Vec<ReceiptSummaryOutput>,
    /// Row count in `receipts` (post-substrate-truncation). The
    /// substrate's `list_receipts` clamps to `MAX_LIMIT` before
    /// returning so this field always equals the visible row count;
    /// the pre-truncation total is not exposed by the substrate
    /// (forward-compat Phase 2 cursor path).
    pub count_returned: usize,
    /// `true` when `count_returned == limit` and the substrate may
    /// have more rows above the ceiling. Operators narrow the
    /// filter and re-invoke (Phase 1 has no cursor pagination per
    /// `AuditFilter::cursor` forward-compat field).
    pub has_more: bool,
}

/// Mirror of `octo_audit::ReceiptSummary` for the CLI envelope
/// boundary (RFC-0011-a §Output Envelope — canonical wire form).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct ReceiptSummaryOutput {
    /// Canonical `receipt_id` (decimal `u64` form).
    pub receipt_id: String,
    /// Hex-form `ask_id` digest (32-byte BLAKE3 derived; lowercase
    /// hex per `ReceiptSummary::from_canonical`).
    pub ask_id: String,
    /// Model identifier (RFC-0014-v2 extension; default = empty
    /// for pre-extension receipts).
    #[serde(default)]
    pub model: String,
    /// Cost in DQA (RFC-0014-v2 extension; default = 0 for
    /// pre-extension receipts).
    #[serde(default)]
    pub cost_dqa: u64,
    /// Hex-encoded 32-byte BLAKE3 capability-root digest
    /// (RFC-0014-v2 extension).
    pub capability_root: String,
    /// Subject DID (canonical wire form per RFC-0010; default =
    /// empty for pre-extension receipts).
    #[serde(default)]
    pub subject_did: String,
    /// Execution timestamp (unix seconds; canonical substrate field
    /// aliasing `Receipt::timestamp_unix`).
    pub executed_at_unix: u64,
    /// Receipt status (`unknown | ok | partial | reject`; canonical
    /// `ReceiptStatus` enum re-exported via `octo_settlement`).
    pub status: String,
}

/// Render payload for `octo audit show <id>` (RFC-0011-a §Output
/// Envelope).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct AuditShowOutput {
    /// Full `ReceiptRecord` projection (canonical substrate fields;
    /// `octo_settlement::Receipt` re-export).
    pub receipt: ReceiptRecordOutput,
    /// Audit home path the CLI resolved at dispatch time
    /// (RFC-0011-a §Configuration — diagnostic context for the
    /// operator; substrate-faithful resolution per
    /// `audit_home()`).
    pub audit_home: String,
}

/// Mirror of `octo_settlement::Receipt` for the CLI envelope
/// boundary. Full canonical field projection (RFC-0014-v2
/// extensions included; default-empty for pre-extension receipts).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct ReceiptRecordOutput {
    /// Canonical `receipt_id` (decimal `u64`).
    pub receipt_id: String,
    /// Hex-form 32-byte `ask_id` digest.
    pub ask_id: String,
    /// Hex-form 32-byte `settlement_hash` digest.
    pub settlement_hash: String,
    /// Router node DID (RFC-0010 canonical wire form).
    pub router_id: String,
    /// Router signature length in bytes. Deliberately a fixed-size
    /// `u64` (NOT substrate-faithful `usize`) so the wire form is
    /// deterministic across platforms (RFC-0011 §Determinism
    /// Requirements — `usize` JSON serializes as 4 or 8 bytes
    /// depending on target, breaking consumer parsers). The raw
    /// signature bytes themselves stay substrate-internal; the CLI
    /// surfaces only the byte count to avoid leaking signature
    /// material. `u64` admits PQ forward-compat (signatures
    /// >2^32 bytes) while remaining deterministic.
    pub router_sig_bytes: u64,
    /// Wall-clock timestamp in unix seconds.
    pub timestamp_unix: u64,
    /// Model identifier (RFC-0014-v2 extension).
    #[serde(default)]
    pub model: String,
    /// Cost in DQA (RFC-0014-v2 extension).
    #[serde(default)]
    pub cost_dqa: u64,
    /// Hex-encoded 32-byte capability-root digest.
    pub capability_root: String,
    /// Subject DID (RFC-0014-v2 extension).
    #[serde(default)]
    pub subject_did: String,
    /// Receipt status (`unknown | ok | partial | reject`).
    pub status: String,
}

/// CLI-side AuditFilter builder from `ListArgs`. Enforces the
/// Auditor-mode `--status` no-op constraint (RFC-0011-a §Security
/// Considerations row 2) before reaching the substrate.
///
/// Returns the canonical `AuditFilter` (Layer B substrate-faithful)
/// plus the effective limit (for `count_returned` derivation).
fn build_audit_filter(args: &ListArgs, mode: OperatorMode) -> AuditFilter {
    // Auditor-mode constraint (RFC-0011-a §Security Considerations
    // row 2): silently drop the `--status` filter so reject rows
    // can never be hidden from the auditor view. The CLI enforces
    // this at the dispatch boundary; the substrate is
    // substrate-faithful and never sees the filter when dropped.
    let status = if mode == OperatorMode::Auditor {
        Vec::new()
    } else {
        args.status.clone()
    };

    // Convert `--since` / `--until` duration-seconds-from-now to
    // absolute unix timestamps. The substrate `since_unix` /
    // `until_unix` fields are ABSOLUTE unix-seconds (per
    // `octo_audit::list_receipts` filter semantics); CLI surfaces
    // the duration grammar per RFC-0011-a §Filters but must
    // materialise absolute values at the dispatch boundary so the
    // filter actually narrows the rowset. Without this conversion
    // `--since 7d` would filter `timestamp_unix >= 604800` (Jan
    // 1970) and silently no-op.
    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let since_unix = args.since.map(|secs| now_unix.saturating_sub(secs));
    let until_unix = args.until.map(|secs| now_unix.saturating_sub(secs));

    AuditFilter {
        router_id: None,
        timestamp_unix_gte: None,
        timestamp_unix_lte: None,
        since_unix,
        until_unix,
        subject_did: None,
        status,
        model: args.model.clone(),
        capability_root: args.capability_root,
        limit: args.limit,
        cursor: None,
    }
}

/// `octo audit list` handler (RFC-0011-a §Subcommand Taxonomy
/// `list`). Read-only projection over `list_receipts` +
/// `get_receipt` + `ReceiptSummary::from_canonical` per RFC-0016-a
/// §6.5.
pub fn list(args: &ListArgs, cli: &Octo) -> Result<(), OctoCliError> {
    // Pre-flight trust-boundary check (RFC-0016-a §6.7): substrate
    // `audit_home()` exercises its `$OCTO_HOME` resolution + 0700
    // permission check before the actual read path runs. The
    // PathBuf itself is unused by the JSON envelope (operators can
    // resolve via `octo audit show` diagnostics); the call exists
    // purely to fail fast on a misconfigured home dir.
    let _ = audit_home().map_err(map_audit_error)?;

    let filter = build_audit_filter(args, cli.mode.mode);

    // Capture the substrate hard ceiling before the call so
    // `has_more` derives from a substrate-faithful reference.
    let effective_limit = filter.limit.unwrap_or(MAX_LIMIT);

    let ids = list_receipts(&filter).map_err(map_audit_error)?;

    // Note: `AuditResponseTooLarge` (exit 19) is a forward-compat
    // variant. Currently unreachable from CLI input — the
    // substrate's `list_receipts` clamps to `MAX_LIMIT` before
    // returning, and `parse_limit` rejects over-ceiling values at
    // the dispatch boundary. The variant is reserved for a future
    // Phase 2 cursor path or a substrate-ceiling raise; the
    // `tv_err4_exit_code_mapping` test pins the canonical exit
    // code either way. We do NOT add a redundant guard here
    // because the substrate is already the source of truth for
    // the limit.

    // For each id, fetch the canonical `Receipt` + project to
    // `ReceiptSummary`. Per-id misses (`AuditError::ReceiptNotFound`)
    // map to `ReceiptNotFound` so the operator sees the offending
    // id verbatim.
    let mut summaries: Vec<ReceiptSummaryOutput> = Vec::with_capacity(ids.len());
    for id in &ids {
        let receipt_id = ReceiptId::new(*id);
        let receipt = get_receipt(&receipt_id).map_err(map_audit_error)?;
        summaries.push(receipt_summary_to_output(&ReceiptSummary::from_canonical(
            receipt,
        )));
    }

    let count_returned = summaries.len();
    let has_more = count_returned == effective_limit as usize && count_returned > 0;

    let payload = AuditListOutput {
        receipts: summaries,
        count_returned,
        has_more,
    };
    let env = OutputEnvelope::new("octo.audit.list.v1", payload);
    env.render(args.json || cli.output.json, cli.output.no_color)
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("render failed: {e}")))
        })
}

/// `octo audit show <receipt_id>` handler (RFC-0011-a §Subcommand
/// Taxonomy `show`). Read-only point lookup via `get_receipt`.
pub fn show(args: &ShowArgs, cli: &Octo) -> Result<(), OctoCliError> {
    let home = audit_home().map_err(map_audit_error)?;
    let receipt = get_receipt(&args.receipt_id).map_err(map_audit_error)?;
    let payload = AuditShowOutput {
        receipt: receipt_to_output(&receipt),
        audit_home: home.display().to_string(),
    };
    let env = OutputEnvelope::new("octo.audit.show.v1", payload);
    env.render(args.json || cli.output.json, cli.output.no_color)
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("render failed: {e}")))
        })
}

/// Dispatch an `AuditAction` to its handler.
pub fn dispatch(action: &AuditAction, cli: &Octo) -> Result<(), OctoCliError> {
    match action {
        AuditAction::List(args) => list(args, cli),
        AuditAction::Show(args) => show(args, cli),
    }
}

// === Substrate → CLI error envelope mapping (RFC-0016-a §6.7) ===

/// Manual mapping for audit substrate errors surfaced at the
/// dispatch boundary. Routes the canonical 4-variant shape
/// (`ReceiptNotFound`, `InvalidFilter`, `PermissionDenied`,
/// `AuditAppendFailed`) through the established
/// `From<octo_audit::AuditError>` impl in error.rs (which
/// preserves the §6.8 `redact_substrate_error` defense-in-depth
/// scrubber pass + canonical exit codes), then re-routes ONLY
/// `#[non_exhaustive]` additive variants to `AuditReadFailed`
/// (exit 18) per RFC-0011-a amendment-chain convention — the
/// audit surface has a canonical exit-code slot rather than
/// collapsing to generic `Internal` (exit 64).
fn map_audit_error(e: AuditError) -> OctoCliError {
    // Capture the substrate Debug repr BEFORE consuming `e` via
    // the canonical From impl (preserves the §6.8 scrubber pass).
    let src_debug = format!("{e:?}");
    let canonical = OctoCliError::from(e);
    // Canonical 4 → canonical `OctoCliError` variant
    // (preserves §6.8 scrubber + exit code).
    // Additive `#[non_exhaustive]` variants → `Internal` (exit 64)
    // per the canonical From impl. Re-route those to
    // `AuditReadFailed` (exit 18) so the audit amendment chain has
    // its own canonical slot.
    if matches!(canonical, OctoCliError::Internal(_)) {
        OctoCliError::AuditReadFailed(sanitize_substrate_error(&format!(
            "audit substrate read failure: {src_debug}"
        )))
    } else {
        canonical
    }
}

// === Output projection helpers ===

fn receipt_summary_to_output(s: &ReceiptSummary) -> ReceiptSummaryOutput {
    ReceiptSummaryOutput {
        receipt_id: s.receipt_id.to_string(),
        ask_id: s.ask_id.clone(),
        model: s.model.clone(),
        cost_dqa: s.cost_dqa,
        capability_root: hex::encode(s.capability_root),
        subject_did: s.subject_did.clone(),
        executed_at_unix: s.executed_at_unix,
        status: s.status.as_str().to_string(),
    }
}

fn receipt_to_output(r: &Receipt) -> ReceiptRecordOutput {
    ReceiptRecordOutput {
        receipt_id: r.receipt_id.to_string(),
        ask_id: hex::encode(r.ask_id),
        settlement_hash: hex::encode(r.settlement_hash),
        router_id: r.router_id.clone(),
        // Cast `usize` → `u64` per `ReceiptRecordOutput.router_sig_bytes`
        // doc-block (deterministic wire-form guarantee).
        router_sig_bytes: r.router_sig.len() as u64,
        timestamp_unix: r.timestamp_unix,
        model: r.model.clone(),
        cost_dqa: r.cost_dqa,
        capability_root: hex::encode(r.capability_root),
        subject_did: r.subject_did.clone(),
        status: r.status.as_str().to_string(),
    }
}

// === clap value_parser helpers ===

/// `--status` value_parser: case-insensitive lowercase normalize +
/// canonical 4-variant validation (`unknown | ok | partial |
/// reject`). Mirrors `ReceiptStatus::as_str()` so the wire form is
/// substrate-faithful (RFC-0016-a §6.5).
fn parse_status(s: &str) -> Result<ReceiptStatus, String> {
    let lower = s.to_ascii_lowercase();
    match lower.as_str() {
        "unknown" => Ok(ReceiptStatus::Unknown),
        "ok" => Ok(ReceiptStatus::Ok),
        "partial" => Ok(ReceiptStatus::Partial),
        "reject" => Ok(ReceiptStatus::Reject),
        _ => Err(format!(
            "status must be one of unknown|ok|partial|reject (got `{s}`)"
        )),
    }
}

/// `<n>d|<n>h|<n>m|<n>s>` duration parser for `--since` /
/// `--until` (RFC-0011-a §Filters grammar). Returns unix-seconds-
/// from-now (negative durations are rejected so the empty-result
/// path is never silently taken on an inverted range).
fn parse_duration_secs(s: &str) -> Result<u64, String> {
    if s.is_empty() {
        return Err("duration must be non-empty (`<n>d|<n>h|<n>m|<n>s`)".into());
    }
    let (num_str, unit) = s.split_at(s.len() - 1);
    let n: u64 = num_str
        .parse()
        .map_err(|_| format!("duration `{s}` must be `<n>d|<n>h|<n>m|<n>s`"))?;
    if num_str.is_empty() || n == 0 {
        return Err(format!(
            "duration `{s}` must be `<n>d|<n>h|<n>m|<n>s` with n >= 1"
        ));
    }
    let secs = match unit {
        "s" => n,
        "m" => n
            .checked_mul(60)
            .ok_or_else(|| "duration overflow".to_string())?,
        "h" => n
            .checked_mul(60 * 60)
            .ok_or_else(|| "duration overflow".to_string())?,
        "d" => n
            .checked_mul(60 * 60 * 24)
            .ok_or_else(|| "duration overflow".to_string())?,
        _ => {
            return Err(format!(
                "duration unit must be one of d|h|m|s (got `{unit}` from `{s}`)"
            ))
        }
    };
    Ok(secs)
}

/// `--capability-root` value_parser: 64 lowercase hex chars
/// (32-byte BLAKE3 digest per RFC-0011-a §Filters). Mixed-case
/// input is rejected (pastejacking defense + pastejacking parity
/// with `policy.rs::is_lower_hex_kind`).
fn parse_capability_root_hex(s: &str) -> Result<[u8; 32], String> {
    if s.len() != 64 {
        return Err(format!(
            "capability-root: expected 64 lowercase hex chars, got {}",
            s.len()
        ));
    }
    if !s
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("capability-root: must be lowercase hex (no uppercase, no non-hex)".into());
    }
    let mut out = [0u8; 32];
    hex::decode_to_slice(s, &mut out).map_err(|e| format!("capability-root decode failed: {e}"))?;
    Ok(out)
}

/// `--limit` value_parser: 1..=`octo_audit::MAX_LIMIT` (substrate
/// hard ceiling per RFC-0011-a §Filters + TV-AUD-4c). The constant
/// is sourced from the substrate facade so a future ceiling raise
/// propagates without a CLI-side change (no parallel abstraction).
/// Zero / over-ceiling input is rejected at the CLI boundary so the
/// substrate never sees an inverted range.
fn parse_limit(s: &str) -> Result<u32, String> {
    let n: u32 = s
        .parse()
        .map_err(|_| format!("limit must be a positive integer (got `{s}`)"))?;
    if n == 0 {
        return Err("limit must be >= 1".into());
    }
    if n > MAX_LIMIT {
        return Err(format!("limit must be <= {MAX_LIMIT}"));
    }
    Ok(n)
}

/// `octo audit show <id>` positional value_parser: decimal `u64`.
/// Format violations surface via clap's `value_parser` failure path
/// → `OctoCliError::ClapParse` (exit 2) per `error.rs` exit-code
/// mapping. NOT exit 16 (`InvalidFilter`) — script authors must
/// consult the clap failure shape, not the substrate's InvalidFilter
/// variant (which only fires for substrate-side filter validation
/// failures, not CLI argument parsing).
fn parse_receipt_id(s: &str) -> Result<ReceiptId, String> {
    let n: u64 = s
        .parse()
        .map_err(|_| format!("receipt_id: expected decimal u64 (got `{s}`)"))?;
    Ok(ReceiptId::new(n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// Test-only CLI parser harness mirroring the production shape
    /// without the dispatch-side `Octo` struct (which requires a
    /// full clap root).
    #[derive(Parser, Debug)]
    struct TestCli {
        #[command(subcommand)]
        action: AuditAction,
    }

    #[test]
    fn parse_status_lowercases_input() {
        assert_eq!(parse_status("OK").unwrap(), ReceiptStatus::Ok);
        assert_eq!(parse_status("Reject").unwrap(), ReceiptStatus::Reject);
        assert_eq!(parse_status("PARTIAL").unwrap(), ReceiptStatus::Partial);
        assert_eq!(parse_status("unknown").unwrap(), ReceiptStatus::Unknown);
    }

    #[test]
    fn parse_status_rejects_unknown_label() {
        let err = parse_status("Bogus").unwrap_err();
        assert!(err.contains("unknown|ok|partial|reject"), "{err}");
    }

    #[test]
    fn parse_duration_secs_accepts_all_units() {
        assert_eq!(parse_duration_secs("30s").unwrap(), 30);
        assert_eq!(parse_duration_secs("5m").unwrap(), 300);
        assert_eq!(parse_duration_secs("2h").unwrap(), 7_200);
        assert_eq!(parse_duration_secs("7d").unwrap(), 604_800);
    }

    #[test]
    fn parse_duration_secs_rejects_malformed() {
        assert!(parse_duration_secs("").is_err());
        assert!(parse_duration_secs("5").is_err()); // no unit
        assert!(parse_duration_secs("0d").is_err()); // n >= 1
        assert!(parse_duration_secs("5y").is_err()); // bad unit
        assert!(parse_duration_secs("-1d").is_err()); // negative
    }

    #[test]
    fn parse_capability_root_hex_round_trip() {
        let hex_str = "0".repeat(64);
        let bytes = parse_capability_root_hex(&hex_str).unwrap();
        assert_eq!(bytes, [0u8; 32]);
    }

    #[test]
    fn parse_capability_root_hex_rejects_short_input() {
        let err = parse_capability_root_hex("01ab").unwrap_err();
        assert!(err.contains("expected 64 lowercase hex chars"), "{err}");
        assert!(err.contains("got 4"), "{err}");
    }

    #[test]
    fn parse_capability_root_hex_rejects_uppercase() {
        let s = "A".repeat(64);
        let err = parse_capability_root_hex(&s).unwrap_err();
        assert!(err.contains("lowercase hex"), "{err}");
    }

    #[test]
    fn parse_limit_enforces_bounds() {
        assert_eq!(parse_limit("1").unwrap(), 1);
        assert_eq!(parse_limit("10000").unwrap(), 10_000);
        assert!(parse_limit("0").is_err());
        assert!(parse_limit("10001").is_err());
        assert!(parse_limit("notanumber").is_err());
    }

    #[test]
    fn parse_receipt_id_accepts_decimal_u64() {
        let id = parse_receipt_id("12345").unwrap();
        assert_eq!(id.as_u64(), 12345);
    }

    #[test]
    fn parse_receipt_id_rejects_non_decimal() {
        assert!(parse_receipt_id("not-hex").is_err());
        assert!(parse_receipt_id("0x1234").is_err());
    }

    #[test]
    fn clap_parses_list_subcommand() {
        let cli = TestCli::try_parse_from([
            "test", "list", "--since", "7d", "--limit", "10", "--status", "ok", "--status",
            "reject",
        ])
        .unwrap();
        match cli.action {
            AuditAction::List(args) => {
                assert_eq!(args.since, Some(604_800));
                assert_eq!(args.until, None);
                assert_eq!(args.limit, Some(10));
                assert_eq!(args.status.len(), 2);
            }
            _ => panic!("expected List"),
        }
    }

    #[test]
    fn clap_parses_show_subcommand() {
        let cli = TestCli::try_parse_from(["test", "show", "12345"]).unwrap();
        match cli.action {
            AuditAction::Show(args) => {
                assert_eq!(args.receipt_id.as_u64(), 12345);
                assert!(!args.json);
            }
            _ => panic!("expected Show"),
        }
    }

    #[test]
    fn clap_parses_show_subcommand_with_json() {
        let cli = TestCli::try_parse_from(["test", "show", "42", "--json"]).unwrap();
        match cli.action {
            AuditAction::Show(args) => assert!(args.json),
            _ => panic!("expected Show"),
        }
    }

    #[test]
    fn build_audit_filter_drops_status_in_auditor_mode() {
        let args = ListArgs {
            since: None,
            until: None,
            capability_root: None,
            model: None,
            status: vec![ReceiptStatus::Reject],
            limit: None,
            json: false,
        };
        // Auditor mode: status filter dropped (RFC-0011-a
        // §Security Considerations row 2).
        let f = build_audit_filter(&args, OperatorMode::Auditor);
        assert!(
            f.status.is_empty(),
            "Auditor-mode status filter must be silently no-op'd, got {:?}",
            f.status
        );
        // Human mode: status filter retained.
        let f = build_audit_filter(&args, OperatorMode::Human);
        assert_eq!(f.status.len(), 1);
    }

    #[test]
    fn build_audit_filter_preserves_other_fields_in_auditor_mode() {
        let cap_root = [0xAB; 32];
        let args = ListArgs {
            since: Some(100),
            until: Some(200),
            capability_root: Some(cap_root),
            model: Some("llama-3.1-8b".into()),
            status: vec![ReceiptStatus::Ok],
            limit: Some(50),
            json: false,
        };
        let f = build_audit_filter(&args, OperatorMode::Auditor);
        // Auditor-mode drops ONLY the status filter — other
        // fields pass through so the operator can still narrow
        // by time window / model / capability-root without
        // accidentally hiding reject rows via the dropped
        // status filter alone. The since/until values are
        // durations (seconds) per RFC-0011-a §Filters grammar
        // — the dispatch boundary converts to absolute
        // timestamps relative to now.
        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        assert_eq!(f.since_unix, now_unix.checked_sub(100));
        assert_eq!(f.until_unix, now_unix.checked_sub(200));
        assert_eq!(f.capability_root, Some(cap_root));
        assert_eq!(f.model.as_deref(), Some("llama-3.1-8b"));
        assert_eq!(f.limit, Some(50));
        assert!(f.status.is_empty());
    }

    #[test]
    fn receipt_summary_output_status_uses_canonical_lowercase() {
        let r = Receipt {
            receipt_id: 42,
            ask_id: [0xAA; 32],
            settlement_hash: [0xBB; 32],
            router_id: "did:octo:router-a".into(),
            router_sig: vec![0xCC; 64],
            timestamp_unix: 1_700_000_042,
            model: "llama-3.1-8b".into(),
            cost_dqa: 1234,
            capability_root: [0xDD; 32],
            subject_did: "did:octo:zSubject".into(),
            status: ReceiptStatus::Partial,
        };
        let summary = ReceiptSummary::from_canonical(r);
        let output = receipt_summary_to_output(&summary);
        assert_eq!(output.status, "partial");
        assert_eq!(output.executed_at_unix, 1_700_000_042);
        assert_eq!(output.receipt_id, "42");
        assert_eq!(output.cost_dqa, 1234);
    }

    #[test]
    fn receipt_record_output_hides_signature_bytes() {
        let r = Receipt {
            receipt_id: 1,
            ask_id: [0u8; 32],
            settlement_hash: [0u8; 32],
            router_id: "did:octo:router-a".into(),
            router_sig: vec![0xCC; 64],
            timestamp_unix: 100,
            status: ReceiptStatus::Ok,
            ..Default::default()
        };
        let output = receipt_to_output(&r);
        assert_eq!(output.router_sig_bytes, 64);
        // signature bytes are NOT serialized — only the byte
        // count is exposed so signature material does not leak.
    }

    #[test]
    fn audit_action_is_non_exhaustive_safe() {
        // Compile-time pin: `AuditAction` carries `#[non_exhaustive]`
        // so future amendments add variants without breaking
        // downstream exhaustive matches. The `match` in `dispatch`
        // uses a wildcard fallback (none needed today — every
        // variant is matched).
        let _: AuditAction = AuditAction::List(ListArgs {
            since: None,
            until: None,
            capability_root: None,
            model: None,
            status: vec![],
            limit: None,
            json: false,
        });
    }

    #[test]
    fn tv_audit_action_subcommand_taxonomy() {
        // Pin: `octo audit {list,show}` are the Phase 1 surface;
        // `Redact | Export | Watch` land in a follow-on amendment.
        let list = TestCli::try_parse_from(["test", "list"]).unwrap();
        let show = TestCli::try_parse_from(["test", "show", "1"]).unwrap();
        assert!(matches!(list.action, AuditAction::List(_)));
        assert!(matches!(show.action, AuditAction::Show(_)));
    }
}
