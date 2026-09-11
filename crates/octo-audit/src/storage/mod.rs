//! DOMAIN-layer storage adapters for `AppendOnlyAuditSink`.
//!
//! The Stoolap adapter (`StoolapAuditSink`) lives in `stoolap.rs`
//! (mission 0012-audit-stoolap-sink substrate-extraction per RFC-0012
//! §Trait G3 mitigation). Keeping the adapter here (DOMAIN) instead of
//! in `octo-audit-core` (Layer A frozen substrate) preserves the layer
//! direction rule from CLAUDE.md §Architectural Principles: the
//! substrate stays free of Stoolap-specific types.

#![allow(clippy::module_inception)]

pub mod stoolap;
