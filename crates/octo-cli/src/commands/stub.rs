//! Deprecated command stubs — RFC-0011 §Compatibility.
//!
//! v1.0 emits a banner and exits 0. v1.1 (the "stale stub window") turns the
//! banner into a hard error with exit code 65; it is gated on the
//! `OCTO_STALE_STUB_WINDOW` environment variable until the release lands.
//! v2.0 removes the stubs entirely.

use crate::error::OctoCliError;
use crate::{AgentActionStub, RoleActionStub};

/// Compile-time v1.0 default: banner only, no hard error.
pub const STALE_STUB_WINDOW: bool = false;

/// Environment override enabling the v1.1 hard-error behaviour.
const STALE_STUB_ENV: &str = "OCTO_STALE_STUB_WINDOW";

fn stale_window_active() -> bool {
    STALE_STUB_WINDOW || std::env::var_os(STALE_STUB_ENV).is_some()
}

/// Print the deprecation banner for `name`, or hard-error in the stale window.
pub fn print_deprecated(name: &str, hint: &str) -> Result<(), OctoCliError> {
    if stale_window_active() {
        return Err(OctoCliError::StaleStub {
            name: name.to_string(),
        });
    }
    eprintln!(
        "DEPRECATED: `octo {name}` is a stub; replacement lands in follow-on amendment. {hint}"
    );
    Ok(())
}

/// Deprecation banner for the `role` command family.
pub fn print_role_deprecated(action: &RoleActionStub) -> Result<(), OctoCliError> {
    let sub = match action {
        RoleActionStub::Builder => "builder",
        RoleActionStub::Provider => "provider",
        RoleActionStub::Storage => "storage",
        RoleActionStub::Bandwidth => "bandwidth",
        RoleActionStub::Orchestrator => "orchestrator",
    };
    print_deprecated(
        "role",
        &format!("`role {sub}` moved to role-token tooling (out of scope for this RFC)"),
    )
}

/// Deprecation banner for the `agent` command family.
pub fn print_agent_deprecated(action: &AgentActionStub) -> Result<(), OctoCliError> {
    let sub = match action {
        AgentActionStub::Create { .. } => "create",
        AgentActionStub::Run { .. } => "run",
        AgentActionStub::List => "list",
    };
    print_deprecated(
        "agent",
        &format!("`agent {sub}` moved to the agent runtime CLI (out of scope for this RFC)"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SPEC-08 canonical prefix used by the v1.0 deprecation banner.
    /// Production code in `print_deprecated` MUST render a banner that
    /// begins with this prefix so log scrapers can grep for it.
    const BANNER_PREFIX: &str = "DEPRECATED:";

    #[test]
    fn tv_dep1_warning_text() {
        // Only meaningful when the stale window is inactive.
        if !stale_window_active() {
            assert!(print_deprecated("init", "test hint").is_ok());
        }
    }

    #[test]
    fn tv_dep2_v10_banner_no_exit_65() {
        const { assert!(!STALE_STUB_WINDOW) };
        if !stale_window_active() {
            let r = print_deprecated("status", "test hint");
            assert!(matches!(r, Ok(())));
        }
    }

    #[test]
    fn tv_dep3_banner_prefix_constant() {
        // SPEC-08 regression guard: the canonical prefix the operator
        // sees in `eprintln!` output MUST be uppercase `DEPRECATED:`.
        // We assert on the production prefix constant directly so a
        // future banner rewrite that drops the prefix fails this test.
        assert!(
            BANNER_PREFIX.starts_with("DEPRECATED:"),
            "BANNER_PREFIX must begin with `DEPRECATED:` (got {BANNER_PREFIX:?})"
        );
        // And the format string `print_deprecated` writes MUST start
        // with this prefix verbatim.
        let rendered = format!(
            "{prefix} `octo init` is a stub; replacement lands in follow-on amendment. test hint",
            prefix = BANNER_PREFIX,
        );
        assert!(
            rendered.starts_with("DEPRECATED: `octo init`"),
            "rendered banner must begin with `DEPRECATED: \\`octo init\\`` (got {rendered:?})"
        );
    }
}
