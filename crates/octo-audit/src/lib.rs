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
// deferred to v2.1+).
pub mod scrub;
pub use scrub::{scrub_adapter_error, scrub_adapter_error_with, scrub_registry_validate};

// RFC-0016 §6.2 read-path surface (list_receipts + get_receipt +
// audit_home + AuditFilter). Phase 1 process-global registry; Stoolap
// DOMAIN adapter lands with RFC-0016-a paired-acceptance unblock.
pub mod receipt_read;
pub use receipt_read::{get_receipt, insert_receipt, list_receipts, AuditFilter};

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
