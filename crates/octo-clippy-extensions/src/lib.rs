//! Layer E per-extension clippy lint module registry
//! (CLAUDE.md §User extensibility — Registry pattern).
//!
//! ## Scope
//!
//! Two lint modules register here under one shared crate root. The
//! audit-side module (`no_raw_format_err`) covers RFC-0012-v3 §FW2. The
//! settlement-side module (`no_raw_to_string`) covers RFC-0014-v3 §FW4.
//! The two modules detect semantically different patterns and require
//! two distinct registry entries — see the per-module doc-blocks for
//! rationale.
//!
//! ## Activation posture (v1.0)
//!
//! Per RFC-0012-v3 §FW2 + RFC-0014-v3 §FW4: activation is **deferred
//! to v3.x**. The v1.0 registry exposes the lint module metadata
//! (`LintModule { name, scope, gate }`) but does NOT perform any
//! AST detection at runtime. Acceptance of a future amendment round
//! flips the gate to ON and wires `dylint` / `clippy_lints` for
//! runtime detection. This posture preserves the Layer A frozen
//! rule (registry crate carries no IO + no detection logic) while
//! giving downstream consumers a stable name to depend on today.

#![warn(missing_debug_implementations)]

pub mod no_raw_format_err;
pub mod no_raw_to_string;

/// Lint module metadata exposed via the registry.
///
/// Each per-extension lint module (per `no_raw_format_err` and
/// `no_raw_to_string`) implements the surface described by this
/// struct: name, scope (DOMAIN-boundary module glob), and gate
/// feature flag. The registry collects every module's metadata so
/// consumers (acceptance-gate missions, future `dylint` host) can
/// inspect the catalogue without depending on each module's
/// internals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LintModule {
    /// Stable lint name (matches the feature gate suffix).
    pub name: &'static str,
    /// Short description (one line) for `cargo clippy --explain`.
    pub description: &'static str,
    /// DOMAIN-boundary glob the lint applies to (e.g.
    /// `crates/*/src/storage/*.rs` + `crates/quota-router-sm-engine/**`).
    pub scope: &'static str,
    /// Feature gate flag that enables this lint's runtime detection.
    pub gate: &'static str,
}

/// Registry catalogue — every lint module registers here at
/// `static_init` time (no IO, no mutation post-init).
///
/// Consumers iterate via [`all_modules`] to enumerate the catalogue.
/// Returns an owned `Vec<LintModule>` (allocated per call) to avoid
/// the static-lifetime trap on returning a reference to a temporary
/// array literal — see compile-error E0515. The allocation cost is
/// negligible (2-element vec) and matches the registry's read-mostly
/// posture.
#[must_use]
pub fn all_modules() -> Vec<LintModule> {
    vec![no_raw_format_err::module(), no_raw_to_string::module()]
}
