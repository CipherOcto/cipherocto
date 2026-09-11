//! DOMAIN-layer storage adapters for `AppendOnlyAuditSink`.
//!
//! Phase 1 ships this module as an empty placeholder. The Stoolap
//! adapter (`StoolapAuditSink`) lands in the follow-on
//! `0012-audit-stoolap-sink` substrate-extraction mission per RFC-0012
//! §Trait G3 mitigation. Keeping the adapter here (DOMAIN) instead of
//! in `octo-audit-core` (Layer A frozen substrate) preserves the
//! layer direction rule from CLAUDE.md §Architectural Principles:
//! the substrate stays free of Stoolap-specific types.

#![allow(clippy::module_inception)]
