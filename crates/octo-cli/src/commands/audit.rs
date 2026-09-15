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
    /// Restrict to receipts whose `router_id` equals the supplied
    /// canonical DID wire form (RFC-0016 KEEP; RFC-0016-a §6.6).
    /// Substrate-faithful: `AuditFilter::router_id` has been in the
    /// substrate since RFC-0016 KEEP — the CLI just hadn't exposed
    /// a flag for it. R2.5 F5 closes the substrate-faithful gap.
    #[arg(long)]
    pub router_id: Option<String>,
    /// Restrict to one or more `ReceiptStatus` values
    /// (`ok | partial | reject | unknown`; case-insensitive;
    /// multiple values OR'd per RFC-0011-a §Filters `status`).
    ///
    /// **Auditor-mode constraint:** silently no-op'd under
    /// `--mode auditor` (RFC-0011-a §Security Considerations row 2)
    /// so reject rows can never be hidden from the auditor view.
    ///
    /// **No-reject-hiding gate (ALL non-Auditor modes):** any
    /// `--status` value excluding `reject` requires either
    /// `--include-reject` (UNION forward reject rows into the result
    /// set per M4 finding) OR `--confirm-acknowledge` (explicit
    /// confirmation of the reject-hiding intent); without either
    /// flag the CLI exits 16 `InvalidFilter` (RFC-0011-a §Security
    /// Considerations row 2 cross-mode consistency gate).
    #[arg(long, value_parser = parse_status, num_args = 1..)]
    pub status: Vec<ReceiptStatus>,
    /// Max rows returned; clamped to the substrate hard ceiling
    /// `MAX_LIMIT = 10_000` (RFC-0011-a §Filters `limit`).
    #[arg(long, value_parser = parse_limit)]
    pub limit: Option<u32>,
    /// Forward reject rows into the result set (UNION semantics):
    /// the visible row set is `status-filter ∪ reject-rows` so the
    /// operator still sees reject rows alongside the requested
    /// status filter. RFC-0011-a §Filters `--include-reject`.
    #[arg(long, requires = "status")]
    pub include_reject: bool,
    /// Explicitly acknowledge the reject-hiding intent of a
    /// non-reject-only `--status` filter. RFC-0011-a §Filters
    /// `--confirm-acknowledge`. Substrate-faithful: the
    /// confirmation happens at the CLI dispatch boundary; the
    /// substrate remains unaware of operator confirmation.
    #[arg(long, requires = "status")]
    pub confirm_acknowledge: bool,
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
    ///
    /// `u64` (NOT `usize`) for the same determinism rationale as
    /// `ReceiptRecordOutput::router_sig_bytes` — `usize` JSON
    /// serializes as 4 or 8 bytes depending on target, breaking
    /// consumer parsers. `u64` keeps the wire form deterministic
    /// across 32-bit and 64-bit platforms (RFC-0011 §Determinism
    /// Requirements). The MAX_LIMIT ceiling is `u32` so the cast is
    /// always safe.
    ///
    /// Note: Phase 1 has no cursor pagination per
    /// `AuditFilter::cursor` forward-compat field. Operators learn
    /// they hit the ceiling via the existing
    /// `AuditResponseTooLarge` exit-19 error path, which fires ONLY
    /// when the substrate returns an overflow signal (the CLI does
    /// not infer "more rows above ceiling" from the visible row
    /// count — that inference is unreliable, see RFC-0011-a §Output
    /// Envelope amendment R2.5 F3).
    pub count_returned: u64,
}

/// Mirror of `octo_audit::ReceiptSummary` for the CLI envelope
/// boundary (RFC-0011-a §Output Envelope — canonical wire form).
///
/// **RFC-0014 extension fields (F4):** when RFC-0014 extension
/// fields are absent from the receipt, the corresponding fields
/// render as their type-default (`model: ""`, `cost_dqa: 0`,
/// `subject_did: ""`). Distinguishing `absent` from
/// `populated-as-empty` requires a future pre-extension
/// discriminator per RFC-0014 §Future Work (deferred).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct ReceiptSummaryOutput {
    /// Canonical `receipt_id` (decimal `u64` form).
    pub receipt_id: String,
    /// Hex-form `ask_id` digest (32-byte BLAKE3 derived; lowercase
    /// hex per `ReceiptSummary::from_canonical`).
    pub ask_id: String,
    /// Model identifier (RFC-0014-v2 extension; type-default empty
    /// string when the receipt pre-dates the extension).
    #[serde(default)]
    pub model: String,
    /// Cost in DQA (RFC-0014-v2 extension; type-default 0 when the
    /// receipt pre-dates the extension).
    #[serde(default)]
    pub cost_dqa: u64,
    /// Hex-encoded 32-byte BLAKE3 capability-root digest
    /// (RFC-0014-v2 extension; pre-extension receipts carry the
    /// zero-digest as a substrate-faithful marker).
    pub capability_root: String,
    /// Subject DID (canonical wire form per RFC-0010; type-default
    /// empty string when the receipt pre-dates the extension).
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
}

/// Mirror of `octo_settlement::Receipt` for the CLI envelope
/// boundary. Full canonical field projection (RFC-0014-v2
/// extensions included).
///
/// **RFC-0014 extension fields (F4):** when RFC-0014 extension
/// fields are absent from the receipt, the corresponding fields
/// render as their type-default (`model: ""`, `cost_dqa: 0`,
/// `subject_did: ""`). Distinguishing `absent` from
/// `populated-as-empty` requires a future pre-extension
/// discriminator per RFC-0014 §Future Work (deferred).
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
    /// depending on target, breaking consumer parsers).
    ///
    /// **DESIGN NOTE: redaction.** The raw signature bytes
    /// themselves stay substrate-internal; the CLI surfaces only the
    /// byte count to avoid leaking signature material — the
    /// `router_sig` field on `Receipt` is never projected to the
    /// CLI envelope. This is a derived metric (length, not content)
    /// rather than a 1:1 substrate field, which is intentional per
    /// RFC-0011-a §Output Envelope ("redact signature material").
    /// `u64` admits PQ forward-compat (signatures > 2^32 bytes) while
    /// remaining deterministic.
    pub router_sig_bytes: u64,
    /// Wall-clock timestamp in unix seconds.
    pub timestamp_unix: u64,
    /// Model identifier (RFC-0014-v2 extension; type-default empty
    /// string when the receipt pre-dates the extension).
    #[serde(default)]
    pub model: String,
    /// Cost in DQA (RFC-0014-v2 extension; type-default 0 when the
    /// receipt pre-dates the extension).
    #[serde(default)]
    pub cost_dqa: u64,
    /// Hex-encoded 32-byte capability-root digest.
    pub capability_root: String,
    /// Subject DID (canonical wire form per RFC-0010; type-default
    /// empty string when the receipt pre-dates the extension).
    #[serde(default)]
    pub subject_did: String,
    /// Receipt status (`unknown | ok | partial | reject`).
    pub status: String,
}

/// CLI-side AuditFilter builder from `ListArgs`. Enforces the
/// Auditor-mode `--status` no-op constraint (RFC-0011-a §Security
/// Considerations row 2) before reaching the substrate.
///
/// Returns the canonical `AuditFilter` (Layer B substrate-faithful).
///
/// **Clock-epoch contract (H1):** takes `now_unix: u64` as a
/// parameter so the caller resolves the wall-clock ONCE (and can
/// reject pre-epoch clocks via `AuditReadFailed` before any filter
/// arithmetic happens). The substrate's `since_unix > until_unix`
/// guard surfaces inverted ranges as `InvalidFilter`; this
/// constructor does NOT pre-check the range — the inverted range
/// path is the caller's responsibility (per R2.5 H3 doc-block on
/// `parse_duration_secs`).
///
/// **Legacy RFC-0016 KEEP-form fields (F6):** `timestamp_unix_gte`
/// and `timestamp_unix_lte` are deliberately NOT populated. The
/// CLI uses the RFC-0016-a additive form (`since_unix`,
/// `until_unix`) only; the substrate honors whichever form is
/// tighter per `list_receipts` filter semantics. The KEEP-form
/// fields are reserved for substrate-emitted filters (e.g. a
/// future `octo audit watch` cursor that preserves an anchor
/// window).
fn build_audit_filter(
    args: &ListArgs,
    mode: OperatorMode,
    now_unix: u64,
) -> Result<AuditFilter, OctoCliError> {
    // Auditor-mode constraint (RFC-0011-a §Security Considerations
    // row 2): silently drop the `--status` filter so reject rows
    // can never be hidden from the auditor view. The CLI enforces
    // this at the dispatch boundary; the substrate is
    // substrate-faithful and never sees the filter when dropped.
    //
    // Defer: Auditor `--status` silent-drop stderr-warn is Phase 2
    // per RFC-0011-a §Future Work. Today the operator gets no
    // signal that the filter was dropped — acceptable per the
    // security contract (the auditor always sees the full set) but
    // not yet operator-friendly.
    let mut status = if mode == OperatorMode::Auditor {
        Vec::new()
    } else {
        args.status.clone()
    };

    // Client-side UNION forward per RFC-0011-a §Filters
    // --include-reject; substrate AuditFilter.status is UNION
    // over members (RFC-0016-a §6.6). Auditor-mode unaffected:
    // status already Vec::new() above.
    if mode != OperatorMode::Auditor
        && args.include_reject
        && !status.contains(&ReceiptStatus::Reject)
    {
        status.push(ReceiptStatus::Reject);
    }

    // Convert `--since` / `--until` duration-seconds-from-now to
    // absolute unix timestamps. The substrate `since_unix` /
    // `until_unix` fields are ABSOLUTE unix-seconds (per
    // `octo_audit::list_receipts` filter semantics); CLI surfaces
    // the duration grammar per RFC-0011-a §Filters but must
    // materialise absolute values at the dispatch boundary so the
    // filter actually narrows the rowset. Without this conversion
    // `--since 7d` would filter `timestamp_unix >= 604800` (Jan
    // 1970) and silently no-op.
    //
    // Inverted `since_unix > until_unix` produces an empty-result
    // set on the substrate (NOT an error here). The substrate's
    // `InvalidFilter` arm is for *strictly* inverted ranges
    // (`since_unix > until_unix`) per TV-AUD-4b; an inverted range
    // that arises from operator input is the caller's
    // responsibility to catch at the parse layer (which today we
    // don't — see H3 doc-block on `parse_duration_secs`).
    let since_unix = args.since.map(|secs| now_unix.saturating_sub(secs));
    let until_unix = args.until.map(|secs| now_unix.saturating_sub(secs));

    Ok(AuditFilter {
        router_id: args.router_id.clone(),
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
    })
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

    // No-reject-hiding gate (RFC-0011-a §Security Considerations
    // row 2 cross-mode consistency gate). Any `--status` value
    // excluding `Reject` in non-Auditor modes requires either
    // `--include-reject` (UNION forward reject rows) OR
    // `--confirm-acknowledge` (explicit confirmation of the
    // reject-hiding intent). Without either the CLI exits 16
    // `InvalidFilter`. Auditor mode silently no-ops ALL `--status`
    // values per the symmetric-semantics clause and bypasses the
    // gate (the auditor MUST see every receipt).
    if cli.mode.mode != OperatorMode::Auditor
        && !args.status.is_empty()
        && !args.status.contains(&ReceiptStatus::Reject)
        && !args.include_reject
        && !args.confirm_acknowledge
    {
        return Err(OctoCliError::InvalidFilter(
            "non-reject-only --status filter requires --include-reject \
             (forward reject rows) OR --confirm-acknowledge (explicit \
             confirmation); RFC-0011-a §Security Considerations row 2 \
             no-reject-hiding gate"
                .into(),
        ));
    }

    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| {
            OctoCliError::AuditReadFailed(
                "system clock before UNIX epoch — refusing to apply relative time filter".into(),
            )
        })?;
    let filter = build_audit_filter(args, cli.mode.mode, now_unix)?;

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

    // Cast `usize` → `u64` per `AuditListOutput::count_returned`
    // doc-block (deterministic wire-form guarantee).
    let count_returned = summaries.len() as u64;

    let payload = AuditListOutput {
        receipts: summaries,
        count_returned,
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
    // Pre-flight trust-boundary check (RFC-0016-a §6.7): substrate
    // `audit_home()` exercises its `$OCTO_HOME` resolution + 0700
    // permission check before the actual read path runs. The
    // resolved path is intentionally DISCARDED — single source of
    // truth (the `list` handler also discards it, and the JSON
    // envelope no longer carries the field). The call exists purely
    // to fail fast on a misconfigured home dir.
    let _ = audit_home().map_err(map_audit_error)?;
    let receipt = get_receipt(&args.receipt_id).map_err(map_audit_error)?;
    let payload = AuditShowOutput {
        receipt: receipt_to_output(&receipt),
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

/// Direct mapping for audit substrate errors surfaced at the
/// dispatch boundary (RFC-0016-a §6.7). Per-variant mapping keeps
/// the substrate → CLI envelope substrate-faithful without
/// round-tripping through `From<AuditError>` (which would route
/// additive `#[non_exhaustive]` variants through `Internal` then
/// re-route here — H4 simplifies to a single direct match).
///
/// **Defense-in-depth scrub pass (RFC-0016-a §6.8 + R1 reviewer
/// HIGH findings C8 + C9):** the three payload-bearing canonical
/// variants (ReceiptNotFound, InvalidFilter, PermissionDenied)
/// route through `octo_audit::redact_substrate_error` before
/// constructing the CLI envelope. The wildcard arm uses the
/// lighter `sanitize_substrate_error` pipeline (3 string markers
/// in `ERROR_MARKERS` + `crates/octo-` path prefix). Format-on-
/// failure (the `src_debug` allocation) is gated inside the
/// wildcard arm only — the canonical 3 payload-bearing arms
/// carry their payload verbatim (after redaction) so eager alloc
/// is avoided on the hot path.
///
/// Exit-code table per RFC-0016-a §6.7:
///
/// | Substrate variant                       | CLI variant                              | Exit |
/// | --------------------------------------- | ---------------------------------------- | ---- |
/// | `AuditError::ReceiptNotFound(s)`        | `OctoCliError::ReceiptNotFound(redact)`  | 17   |
/// | `AuditError::InvalidFilter(s)`          | `OctoCliError::InvalidFilter(redact)`    | 16   |
/// | `AuditError::PermissionDenied(s)`       | `OctoCliError::PermissionDenied(redact)` | 13   |
/// | `AuditError::AuditAppendFailed(_)`      | `OctoCliError::AuditSubstrateNotReady`   | 52   |
/// | `AuditError::SequenceGap { .. }`        | `OctoCliError::AuditReadFailed(reason)`  | 18   |
/// | `AuditError::AlreadyExists(_)`          | `OctoCliError::AuditReadFailed(reason)`  | 18   |
/// | `AuditError::ChainHashMismatch { .. }`  | `OctoCliError::AuditReadFailed(reason)`  | 18   |
/// | `AuditError::SinkSpecific(_)`           | `OctoCliError::AuditReadFailed(reason)`  | 18   |
/// | `#[non_exhaustive]` future variant      | `OctoCliError::AuditReadFailed(reason)`  | 18   |
///
/// Note: the original RFC-0016-a §6.7 table routed the
/// non-canonical variants to `Internal` (exit 64); H4 re-routes
/// them to `AuditReadFailed` (exit 18) directly so the audit
/// amendment chain has its own canonical slot. The change is
/// purely a slot-mapping re-shuffle — the wildcard arm still
/// allocates the substrate Debug repr for diagnostics.
fn map_audit_error(e: AuditError) -> OctoCliError {
    let redact = |payload: &str| octo_audit::redact_substrate_error(payload);
    match e {
        AuditError::ReceiptNotFound(s) => OctoCliError::ReceiptNotFound(redact(&s)),
        AuditError::InvalidFilter(s) => OctoCliError::InvalidFilter(redact(&s)),
        AuditError::PermissionDenied(s) => OctoCliError::PermissionDenied(redact(&s)),
        // Substrate-faithful: `AuditAppendFailed(reason)` collapses
        // to operator-facing `AuditSubstrateNotReady` (unit variant)
        // — the reason is substrate-internal and intentionally NOT
        // surfaced to the CLI per RFC-0016-a §6.7 table footnote.
        AuditError::AuditAppendFailed(_) => OctoCliError::AuditSubstrateNotReady,
        // SequenceGap, AlreadyExists, ChainHashMismatch, SinkSpecific,
        // and any `#[non_exhaustive]` future additive variant all
        // map to `AuditReadFailed` (exit 18) — the audit amendment
        // chain's canonical slot. The substrate-side scrubber
        // applies the lightweight 3-string-marker pattern plus the
        // `crates/octo-` path prefix so an unknown future variant
        // that accidentally carries a key/path leaks only
        // `<redacted-*>` markers.
        other => {
            let reason = format!("audit substrate read failure: {other:?}");
            OctoCliError::AuditReadFailed(sanitize_substrate_error(&reason))
        }
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
/// from-now (i.e. a duration, not an absolute timestamp).
///
/// **Inverted-range contract (H3):** this constructor parses a
/// SINGLE duration field — it does NOT know about the other
/// bound. An inverted `since > until` range (both supplied, with
/// `since` larger than `until`) is the caller's responsibility
/// to catch; the substrate's `InvalidFilter` arm rejects strict
/// inversions per TV-AUD-4b, but the CLI does not pre-check here.
/// An inverted range produces an empty-result set on the
/// substrate, which is the intended substrate-faithful behaviour
/// (the operator narrows the filter and re-invokes).
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
        // Med-2: substrate-tracked constants — no parallel-abstraction
        // literal duplication. A future ceiling raise propagates via
        // `octo_audit::MAX_LIMIT` without a CLI-side test change.
        assert_eq!(parse_limit("1").unwrap(), 1);
        assert_eq!(parse_limit(&MAX_LIMIT.to_string()).unwrap(), MAX_LIMIT);
        assert!(parse_limit("0").is_err());
        assert!(parse_limit(&(MAX_LIMIT + 1).to_string()).is_err());
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
            router_id: None,
            status: vec![ReceiptStatus::Reject],
            limit: None,
            include_reject: false,
            confirm_acknowledge: false,
            json: false,
        };
        // Deterministic clock: H1 contract — `build_audit_filter`
        // takes `now_unix` as a parameter so the test owns the
        // wall-clock resolution (no flakiness from `SystemTime::now`).
        let now_unix: u64 = 1_700_000_000;
        // Auditor mode: status filter dropped (RFC-0011-a
        // §Security Considerations row 2).
        let f = build_audit_filter(&args, OperatorMode::Auditor, now_unix)
            .expect("filter construction never fails today");
        assert!(
            f.status.is_empty(),
            "Auditor-mode status filter must be silently no-op'd, got {:?}",
            f.status
        );
        // Human mode: status filter retained.
        let f = build_audit_filter(&args, OperatorMode::Human, now_unix)
            .expect("filter construction never fails today");
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
            router_id: None,
            status: vec![ReceiptStatus::Ok],
            limit: Some(50),
            include_reject: false,
            confirm_acknowledge: false,
            json: false,
        };
        // Deterministic clock: H1 contract — `build_audit_filter`
        // takes `now_unix` as a parameter so the test owns the
        // wall-clock resolution (no flakiness from `SystemTime::now`).
        let now_unix: u64 = 1_700_000_000;
        let f = build_audit_filter(&args, OperatorMode::Auditor, now_unix)
            .expect("filter construction never fails today");
        // Auditor-mode drops ONLY the status filter — other
        // fields pass through so the operator can still narrow
        // by time window / model / capability-root without
        // accidentally hiding reject rows via the dropped
        // status filter alone. The since/until values are
        // durations (seconds) per RFC-0011-a §Filters grammar
        // — the dispatch boundary converts to absolute
        // timestamps relative to now.
        assert_eq!(f.since_unix, Some(now_unix - 100));
        assert_eq!(f.until_unix, Some(now_unix - 200));
        assert_eq!(f.capability_root, Some(cap_root));
        assert_eq!(f.model.as_deref(), Some("llama-3.1-8b"));
        assert_eq!(f.limit, Some(50));
        assert!(f.status.is_empty());
    }

    #[test]
    fn build_audit_filter_propagates_router_id() {
        // F5: `--router-id` flag was missing from the CLI surface
        // despite the substrate carrying `AuditFilter::router_id`
        // since RFC-0016 KEEP. This test pins the substrate-faithful
        // propagation: the operator-supplied router DID reaches
        // `AuditFilter::router_id` verbatim.
        let args = ListArgs {
            since: None,
            until: None,
            capability_root: None,
            model: None,
            router_id: Some("did:octo:router-a".into()),
            status: vec![],
            limit: None,
            include_reject: false,
            confirm_acknowledge: false,
            json: false,
        };
        let now_unix: u64 = 1_700_000_000;
        let f = build_audit_filter(&args, OperatorMode::Human, now_unix)
            .expect("filter construction never fails today");
        assert_eq!(f.router_id.as_deref(), Some("did:octo:router-a"));
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
            router_id: None,
            status: vec![],
            limit: None,
            include_reject: false,
            confirm_acknowledge: false,
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

    // Gate tests for the no-reject-hiding constraint
    // (RFC-0011-a §Security Considerations row 2 cross-mode
    // consistency gate). The gate fires on non-Auditor modes
    // when `--status` excludes `reject` AND neither
    // `--include-reject` nor `--confirm-acknowledge` is set.

    fn make_args_with_status(
        status: Vec<ReceiptStatus>,
        include_reject: bool,
        confirm_acknowledge: bool,
    ) -> ListArgs {
        ListArgs {
            since: None,
            until: None,
            capability_root: None,
            model: None,
            router_id: None,
            status,
            limit: None,
            include_reject,
            confirm_acknowledge,
            json: false,
        }
    }

    fn human_filter(args: &ListArgs) -> AuditFilter {
        build_audit_filter(args, OperatorMode::Human, 1_000_000).unwrap()
    }

    fn build_cli_with_mode(mode: OperatorMode) -> Octo {
        // Build a minimal CLI with the requested mode baked in. The
        // parse should always succeed (the args are well-formed);
        // if it doesn't, the test setup itself is broken and a
        // panic is the correct diagnostic.
        let mut cli = Octo::try_parse_from(["octo", "audit", "list"])
            .expect("test harness: octo audit list must parse");
        cli.mode.mode = mode;
        cli
    }

    #[test]
    fn reject_hiding_gate_fires_in_human_mode_for_status_ok() {
        // MED-5 (now CRITICAL per RFC text): any `--status`
        // value excluding `reject` in a non-Auditor mode WITHOUT
        // `--include-reject` or `--confirm-acknowledge` exits 16
        // `InvalidFilter`. This is the gate that protects
        // compromised operator sessions from hiding reject rows.
        let args = make_args_with_status(vec![ReceiptStatus::Ok], false, false);
        let cli = build_cli_with_mode(OperatorMode::Human);
        let r = list(&args, &cli);
        match r {
            Err(OctoCliError::InvalidFilter(msg)) => {
                assert!(
                    msg.contains("no-reject-hiding gate"),
                    "gate message should mention the no-reject-hiding gate; got `{msg}`"
                );
            }
            Err(other) => panic!("expected InvalidFilter, got {other:?}"),
            Ok(()) => panic!("expected gate to fire in Human mode, but list succeeded"),
        }
    }

    #[test]
    fn reject_hiding_gate_fires_in_human_mode_for_status_partial() {
        let args = make_args_with_status(vec![ReceiptStatus::Partial], false, false);
        let cli = build_cli_with_mode(OperatorMode::Human);
        let r = list(&args, &cli);
        assert!(
            matches!(r, Err(OctoCliError::InvalidFilter(_))),
            "gate must fire for --status partial in Human mode, got {r:?}"
        );
    }

    #[test]
    fn reject_hiding_gate_does_not_fire_for_status_reject_only() {
        // `--status reject` is exempt from the gate (no hiding
        // intent — reject-only view is the auditor threat model's
        // legitimate equivalent). The gate check is "does --status
        // exclude reject", so a Reject-only filter does not
        // exclude anything.
        let args = make_args_with_status(vec![ReceiptStatus::Reject], false, false);
        let cli = build_cli_with_mode(OperatorMode::Human);
        // Does NOT assert Ok because the substrate is empty in
        // tests; only asserts the gate was NOT triggered
        // (specifically: NOT an InvalidFilter error).
        if let Err(OctoCliError::InvalidFilter(msg)) = list(&args, &cli) {
            panic!("gate must NOT fire for reject-only filter, got InvalidFilter({msg})");
        }
    }

    #[test]
    fn reject_hiding_gate_bypassed_by_include_reject() {
        let args = make_args_with_status(vec![ReceiptStatus::Ok], true, false);
        let cli = build_cli_with_mode(OperatorMode::Human);
        if let Err(OctoCliError::InvalidFilter(msg)) = list(&args, &cli) {
            panic!("gate must NOT fire when --include-reject is set, got InvalidFilter({msg})");
        }
    }

    #[test]
    fn include_reject_unions_reject_into_substrate_status() {
        // R3 substrate-faithfulness: the gate lets `--include-reject`
        // through, but the flag's documented UNION semantics
        // (`result_set = filter-result ∪ reject-rows` per
        // RFC-0011-a §Filters `--include-reject`) require that
        // `ReceiptStatus::Reject` actually reach the substrate
        // `AuditFilter.status` Vec. Verify via direct `build_audit_filter`
        // call — the substrate fixture is empty in tests so an
        // end-to-end row assertion would not surface a defect.
        let args = make_args_with_status(vec![ReceiptStatus::Ok], true, false);
        let filter = human_filter(&args);
        assert!(
            filter.status.contains(&ReceiptStatus::Reject),
            "UNION-forward contract: --include-reject must push Reject into AuditFilter.status; got {:?}",
            filter.status
        );
        assert!(
            filter.status.contains(&ReceiptStatus::Ok),
            "UNION-forward must preserve the original --status filter; got {:?}",
            filter.status
        );
    }

    #[test]
    fn include_reject_no_op_when_status_already_contains_reject() {
        // Idempotent: if the operator already passed `--status reject`,
        // `--include-reject` is a no-op (no duplicate push). The
        // substrate `AuditFilter.status` is a UNION over its members,
        // so duplicates would not change the result set but would
        // waste a Vec slot and confuse substrate-faithful readers.
        let args = make_args_with_status(vec![ReceiptStatus::Reject], true, false);
        let filter = human_filter(&args);
        assert_eq!(
            filter
                .status
                .iter()
                .filter(|s| **s == ReceiptStatus::Reject)
                .count(),
            1,
            "UNION-forward must not duplicate Reject; got {:?}",
            filter.status
        );
    }

    #[test]
    fn include_reject_no_op_without_flag() {
        // Without --include-reject the filter must NOT silently
        // include Reject rows (that would defeat the no-reject-hiding
        // gate's whole purpose — the operator would see reject rows
        // even when they explicitly asked for non-reject only).
        let args = make_args_with_status(vec![ReceiptStatus::Ok], false, false);
        let filter = human_filter(&args);
        assert!(
            !filter.status.contains(&ReceiptStatus::Reject),
            "without --include-reject, AuditFilter.status must NOT contain Reject; got {:?}",
            filter.status
        );
    }

    #[test]
    fn reject_hiding_gate_bypassed_by_confirm_acknowledge() {
        let args = make_args_with_status(vec![ReceiptStatus::Ok], false, true);
        let cli = build_cli_with_mode(OperatorMode::Human);
        if let Err(OctoCliError::InvalidFilter(msg)) = list(&args, &cli) {
            panic!(
                "gate must NOT fire when --confirm-acknowledge is set, got InvalidFilter({msg})"
            );
        }
    }

    #[test]
    fn reject_hiding_gate_does_not_apply_to_auditor_mode() {
        // Auditor mode silently no-ops ALL --status values
        // (symmetric semantics per H5 finding). The no-reject-hiding
        // gate is bypassed entirely in Auditor mode — the auditor
        // MUST see every receipt regardless of which status
        // filter is requested (audit-trail invariant).
        let args = make_args_with_status(vec![ReceiptStatus::Ok], false, false);
        let cli = build_cli_with_mode(OperatorMode::Auditor);
        if let Err(OctoCliError::InvalidFilter(msg)) = list(&args, &cli) {
            panic!(
                "gate must NOT fire in Auditor mode (silent no-op overrides gate), got InvalidFilter({msg})"
            );
        }
    }

    #[test]
    fn reject_hiding_gate_does_not_apply_for_empty_status() {
        // No status filter at all means the operator is not
        // hiding anything — the gate does not fire.
        let args = make_args_with_status(vec![], false, false);
        let cli = build_cli_with_mode(OperatorMode::Human);
        if let Err(OctoCliError::InvalidFilter(msg)) = list(&args, &cli) {
            panic!("gate must NOT fire when no --status is supplied, got InvalidFilter({msg})");
        }
    }
}
