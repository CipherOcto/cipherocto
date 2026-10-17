//! Audit-side lint module: `no-raw-format-err`
//! (RFC-0012-v3 §FW2 — clippy lint prelude).
//!
//! ## Detection target
//!
//! Flags raw `format!("{e}")` chains at DOMAIN adapter boundaries
//! where `e: AuditError` (or any `octo_audit_core::AuditError`
//! re-export). The lint enforces the DOMAIN adapter contract per
//! RFC-0012-v3 §S5.1.1: every raw `format!("{e}")` chain in
//! `crates/*/src/storage/*.rs` MUST go through
//! `scrub_adapter_error_with(s, ADAPTER_TYPES)` (Pattern 6 registry).
//!
//! ## Activation posture (v1.0)
//!
//! Per RFC-0012-v3 §FW2: **v3.x activation deferred**. The v1.0
//! module exposes metadata only. Runtime detection is wired at the
//! next paired-acceptance amendment round (v3.1) that flips the
//! gate from off-by-default to on.

use crate::LintModule;

/// Module metadata for the audit-side `no-raw-format-err` lint.
#[must_use]
pub fn module() -> LintModule {
    LintModule {
        name: "no_raw_format_err",
        description: "forbid raw `format!(\"{e}\")` chains on AuditError at DOMAIN adapter boundaries; route through scrub_adapter_error_with",
        scope: "crates/*/src/storage/*.rs",
        gate: "lint-no-raw-format-err",
    }
}
