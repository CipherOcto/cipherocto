//! Settlement-side lint module: `no-raw-to-string`
//! (RFC-0014-v3 §FW4 — clippy lint prelude).
//!
//! ## Detection target
//!
//! Flags raw `.to_string()` chains on `SettlementError` (shadow 8-variant
//! at `quota-router-sm-engine` + canonical 7-variant substrate at
//! `octo-settlement-core`) at DOMAIN-boundary modules. The lint enforces
//! the DOMAIN adapter contract per RFC-0014-v3 §S5.1.1: every raw
//! `.to_string()` chain on a `SettlementError` in
//! `crates/quota-router-sm-engine/**/*.rs` MUST go through
//! `scrub_adapter_error_with(s, ADAPTER_TYPES)` (Pattern 6 registry).
//!
//! ## Activation posture (v1.0)
//!
//! Per RFC-0014-v3 §FW4: **v3.x activation deferred**. The v1.0
//! module exposes metadata only. Runtime detection is wired at the
//! next paired-acceptance amendment round (v3.1) that flips the
//! gate from off-by-default to on.
//!
//! ## Distinct from audit-side `no_raw_format_err`
//!
//! The two lints detect **semantically different patterns**:
//! `format!("{e}")` (audit-side) vs `e.to_string()` (settlement-side).
//! They live in separate modules so each can be enabled / disabled
//! independently without coupling the audit-side façade to the
//! settlement-side façade (CLAUDE.md §Layer E per-extension
//! isolation rule).

use crate::LintModule;

/// Module metadata for the settlement-side `no-raw-to-string` lint.
#[must_use]
pub fn module() -> LintModule {
    LintModule {
        name: "no_raw_to_string",
        description: "forbid raw `.to_string()` chains on SettlementError at DOMAIN-boundary modules; route through scrub_adapter_error_with",
        scope: "crates/quota-router-sm-engine/**/*.rs + crates/octo-settlement/**/*.rs",
        gate: "lint-no-raw-to-string",
    }
}
