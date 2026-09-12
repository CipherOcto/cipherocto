//! Layer A frozen audit substrate (RFC-0012 §Specification).
//!
//! This crate owns the canonical audit primitives:
//!
//! - [`AuditEvent`] + [`AuditEventKind`] — canonical event struct (7 fields
//!   per RFC-0957-A1 §F3 + RFC-0012 §Module Layout `event`).
//! - [`AppendOnlyAuditSink`] trait — type-level append-only enforcement via
//!   `&mut self` requirement (RFC-0012 §Trait G3).
//! - [`verify_chain`] + [`compute_chain_hash`] — chain-integrity helpers
//!   over canonical serialization (RFC-0012 §Design Goals G5).
//! - [`AuditChainError`] — gap / mismatch / regression error variants.
//! - [`AuditError`] — cross-trait error envelope.
//!
//! ## Layer discipline (per CLAUDE.md §Architectural Principles)
//!
//! Per RFC-0012 §Security Considerations, this crate is **RFC-frozen +
//! semver-major only**. Deps are restricted to Layer A primitives
//! (`blake3` + `serde` + `thiserror`); no Layer B substrate deps.
//! DID canonical-form conversion happens at the domain call boundary
//! (the substrate holds `node_did: String`; the wallet-domain or audit
//! storage adapter does the canonical-form conversion before/after
//! crossing the substrate boundary).
//!
//! PQC migration blast radius is confined to this crate. A future PQC
//! transition touches `compute_chain_hash` only; CLI + domain consumers
//! are unaffected.
//!
//! ## Module layout (RFC-0012 §Module Layout)
//!
//! - [`event`] — `AuditEvent` struct + `AuditEventKind` enum + manual `Debug`
//! - [`sink`] — `AppendOnlyAuditSink` trait
//! - [`chain`] — `verify_chain` + `compute_chain_hash` + `AuditChainError`
//! - [`error`] — `AuditError` cross-trait envelope

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod chain;
pub mod error;
pub mod event;
pub mod sink;

pub use chain::{compute_chain_hash, verify_chain};
pub use error::{AuditChainError, AuditError, TimestampOpaque};
pub use event::{AuditEvent, AuditEventKind};
pub use sink::AppendOnlyAuditSink;
