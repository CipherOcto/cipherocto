//! Layer B audit facade for the cipherocto workspace (RFC-0012 §Module
//! Layout).
//!
//! This facade re-exports the substrate surface (`octo-audit-core`)
//! verbatim. Concrete storage adapters (e.g. `StoolapAuditSink`) live
//! under `crates/octo-audit/src/storage/` — kept at the DOMAIN layer so
//! the Layer A frozen substrate stays free of Stoolap-specific types
//! per CLAUDE.md §Architectural Principles layer direction rule.
//!
//! ## Layer model
//!
//! Per `cipherocto-design-principles` Layer B, this crate is
//! **RFC-driven, additive only**. The re-export surface mirrors the
//! substrate's public API byte-for-byte. New types land in the substrate
//! first; this facade pulls them in on the next semver minor of the
//! underlying crate.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

// Explicit curated re-export (RFC-0012 §Module Layout).
pub use octo_audit_core::compute_chain_hash;
pub use octo_audit_core::verify_chain;
pub use octo_audit_core::AppendOnlyAuditSink;
pub use octo_audit_core::AuditChainError;
pub use octo_audit_core::AuditError;
pub use octo_audit_core::AuditEvent;
pub use octo_audit_core::AuditEventKind;

// Storage adapter module — concrete sinks (e.g. Stoolap) live here at
// the DOMAIN layer. Per RFC-0012 §Trait G3 mitigation, concrete impls
// keep the `&mut self` requirement to preserve type-level append-only
// enforcement. The module is empty in Phase 1; concrete adapter impls
// land in follow-on substrate-extraction missions (e.g.
// `0012-audit-stoolap-sink`).
pub mod storage;

// RFC-0012-v3 substrate amendment: per-façade 10-pattern scrubber
// (defect 1a — DOMAIN adapter error-chain redaction). Re-exports the
// `scrub_adapter_error` + `scrub_adapter_error_with` entry points so
// DOMAIN adapters can call them without depending on a generic
// shared-utility crate (which the R34.5 trade-off explicitly
// deferred to v2.1+). RFC-0016-a §6.9 paired-acceptance extends to
// 13 patterns additive (Patterns 11/12/13 = PGP / OpenSSH / PEM
// private-key blocks).
pub mod scrub;
pub use scrub::{scrub_adapter_error, scrub_adapter_error_with, scrub_registry_validate};

// RFC-0016 §6.2 read-path surface (list_receipts + get_receipt +
// audit_home + AuditFilter). Phase 1 process-global registry; Stoolap
// DOMAIN adapter lands with RFC-0016-a paired-acceptance unblock.
// RFC-0016-a §6.6 extends `AuditFilter` additively (subject_did,
// status UNION, model, capability_root, since_unix / until_unix).
pub mod receipt_read;
pub use receipt_read::{audit_home, get_receipt, insert_receipt, list_receipts, AuditFilter};

// RFC-0016-a §6.2 + §6.3 paired-acceptance write-path surface:
// `ChainHash(pub [u8; 32])` canonical BLAKE3 chain-hash newtype with
// `Display` (lowercase hex) + `append_audit_event(sink: &mut, event)
// -> Result<ChainHash, AuditError>` Layer B façade write function.
// The Rust borrow checker enforces single-writer per sink instance
// at the type level (`&mut self`) per RFC-0012 §Trait G3 +
// RFC-0016-a §6.11 read-stall-while-write invariant.
pub mod audit_event_v2;
pub use audit_event_v2::{append_audit_event, ChainHash};

// RFC-0016-a §6.5 paired-acceptance: `ReceiptSummary` canonical
// projection struct + `from_canonical(Receipt) -> Self` mapper. The
// CLI list command renders rows as `ReceiptSummary` without exposing
// every canonical `Receipt` field.
pub mod receipt_summary;
pub use receipt_summary::ReceiptSummary;

// RFC-0016-a §6.8 paired-acceptance: substrate-side second-pass
// scrubber applied at the façade boundary. `redact_substrate_error`
// collapses any payload matching one of the 13 scrubber patterns
// to the canonical `<REDACTED>` marker (per spec §6.8); verbatim
// strings pass through unchanged. CLI envelopes route substrate
// `SinkSpecific` / `InvalidFilter` / `ReceiptNotFound` /
// `PermissionDenied` payloads through this function before
// constructing CLI-shape variants, so the type system can no
// longer carry raw secret material past the façade boundary.
pub mod scrub_newtypes;
pub use scrub_newtypes::redact_substrate_error;

// RFC-0016-a §6.6 paired-acceptance: `StatusRef` type alias for the
// canonical Layer A frozen `ReceiptStatus` enum re-exported through
// the `octo-settlement` Layer B façade. Used by `AuditFilter.status`
// (UNION semantics over multiple status values).
/// Canonical `ReceiptStatus` type alias for `AuditFilter::status`
/// list elements (RFC-0016-a §6.6).
pub type StatusRef = octo_settlement::ReceiptStatus;

// RFC-0015-a §6.1 paired-acceptance bridge: audit write-path façade.
// Process-global sink registry + `append_agent_transition_event`
// helper. Gated behind `octo-audit-internal` feature per RFC-0015-a
// §6.4 paired-acceptance bridge contract; the whole module is
// INVISIBLE in default builds (the substrate `AuditEventKind` does
// not expose `AgentTransition` without the feature). Permanent
// once RFC-0012-v2 lands.
#[cfg(feature = "octo-audit-internal")]
pub mod audit_write;
#[cfg(feature = "octo-audit-internal")]
pub use audit_write::{append_agent_transition_event, register_audit_sink, AgentTransitionPayload};
