//! CI mode detection helper (RFC-0011-h §Substrate-Additions row G25).
//!
//! Layer C CLI dispatch surface (NOT Layer B substrate) per
//! RFC-0011-l Phase 4 row G25. Determines whether the operator
//! invoking `octo network ...` is an interactive operator or a CI
//! agent. CI agents are blocked from the 6 CI-DENY-default write
//! subcommands (the rebind-* trio + mode set + authority rotate +
//! slash apply) per RFC-0011-h §Confirmation Flag + Per-Axis Exit
//! Code Matrix rows 138-140.
//!
//! ## Detection inputs
//!
//! The helper combines TWO independent probes:
//!
//! 1. `OCTO_CLI_CI` env-var: when set to a non-empty value
//!    (case-insensitive `1`, `true`, `yes` → CI agent; anything
//!    else → no effect)
//! 2. `[ -t 0 ]` stdin TTY probe: when stdin is NOT a terminal
//!    (e.g. piped from another process) → CI agent
//!
//! The two probes OR together — either positive flips the
//! detection to `CiAgent`. The TTY probe alone catches the
//! "CI agent without env-var set" case (most CI systems do not
//! set `OCTO_CLI_CI` but pipe stdin from a build script).
//!
//! ## Escape hatch
//!
//! `--allow-ci-deny-default` (DEBUG-ONLY, hidden from `--help`
//! per RFC-0011 §Security Considerations experimental-flag
//! contract; surfaces in `octo network ... --help-all`) lets
//! CI agents opt into the 6 CI-DENY-default subcommands when
//! the CI pipeline has explicit human approval. Surfaces as
//! stable contract: experimental, removed before v1.0.
//!
//! ## Why a separate helper module
//!
//! Putting the CI detection inside the network module would
//! force every rebind-* arm to duplicate the env-var + TTY
//! probe + escape-hatch check. Centralising the helper
//! keeps the 5 caller arms (rebind-prepare + rebind-commit +
//! rebind-abort + mode set + authority rotate + slash apply)
//! DRY and lets operator switch tables grep on a single
//! `CiDetection::detect` boundary.

use std::env;
use std::io::IsTerminal;

/// The detected operator mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CiMode {
    /// Operator is a CI agent (env-var set OR stdin not a TTY).
    /// Blocked from the 6 CI-DENY-default subcommands unless
    /// `--allow-ci-deny-default` is passed.
    CiAgent,
    /// Operator is an interactive terminal user.
    Interactive,
}

/// Inputs to CI mode detection. Decoupled from the live
/// `std::env::var` + `std::io::stdin().is_terminal()` reads so
/// the helper is unit-testable without environment mutation.
///
/// Construct via `CiDetection::detect()` in production code;
/// construct directly in tests via `CiDetection::from_parts`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CiDetection {
    /// The `OCTO_CLI_CI` env-var value, if set.
    ci_env: Option<String>,
    /// Whether stdin is a terminal at probe time.
    stdin_is_tty: bool,
}

impl CiDetection {
    /// Build a `CiDetection` from explicit parts (test-only path).
    ///
    /// Production callers use `detect()` which reads live env +
    /// stdin. Tests pass fixed values via `from_parts`.
    #[must_use]
    pub fn from_parts(ci_env: Option<String>, stdin_is_tty: bool) -> Self {
        Self {
            ci_env,
            stdin_is_tty,
        }
    }

    /// Live detection: read `OCTO_CLI_CI` env-var + probe stdin
    /// TTY state. Returns the resolved `CiMode`.
    #[must_use]
    pub fn detect() -> CiMode {
        Self::from_parts(env::var("OCTO_CLI_CI").ok(), std::io::stdin().is_terminal()).resolve()
    }

    /// Resolve the `CiMode` from the configured inputs.
    ///
    /// Rules (in order):
    /// 1. `OCTO_CLI_CI` set to truthy value (`1`, `true`, `yes`,
    ///    case-insensitive) → `CiAgent`
    /// 2. stdin is NOT a terminal → `CiAgent`
    /// 3. Otherwise → `Interactive`
    #[must_use]
    pub fn resolve(&self) -> CiMode {
        if Self::env_is_truthy(self.ci_env.as_deref()) {
            return CiMode::CiAgent;
        }
        if !self.stdin_is_tty {
            return CiMode::CiAgent;
        }
        CiMode::Interactive
    }

    /// Whether the `OCTO_CLI_CI` env-var carries a truthy value.
    /// Returns false for unset, empty, or non-truthy values.
    #[must_use]
    pub fn env_is_truthy(value: Option<&str>) -> bool {
        match value {
            Some(v) => matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_ci_env_unset_interactive_stdin_is_interactive() {
        let d = CiDetection::from_parts(None, true);
        assert_eq!(d.resolve(), CiMode::Interactive);
    }

    #[test]
    fn t_ci_env_unset_non_tty_stdin_is_ci_agent() {
        let d = CiDetection::from_parts(None, false);
        assert_eq!(d.resolve(), CiMode::CiAgent);
    }

    #[test]
    fn t_ci_env_set_to_one_overrides_tty() {
        let d = CiDetection::from_parts(Some("1".into()), true);
        assert_eq!(d.resolve(), CiMode::CiAgent);
    }

    #[test]
    fn t_ci_env_set_to_true_lowercase() {
        let d = CiDetection::from_parts(Some("true".into()), true);
        assert_eq!(d.resolve(), CiMode::CiAgent);
    }

    #[test]
    fn t_ci_env_set_to_yes_uppercase() {
        let d = CiDetection::from_parts(Some("YES".into()), false);
        assert_eq!(d.resolve(), CiMode::CiAgent);
    }

    #[test]
    fn t_ci_env_set_to_zero_is_not_truthy() {
        let d = CiDetection::from_parts(Some("0".into()), true);
        assert_eq!(d.resolve(), CiMode::Interactive);
    }

    #[test]
    fn t_ci_env_set_to_empty_is_not_truthy() {
        let d = CiDetection::from_parts(Some("".into()), true);
        assert_eq!(d.resolve(), CiMode::Interactive);
    }

    #[test]
    fn t_ci_env_set_to_arbitrary_string_is_not_truthy() {
        let d = CiDetection::from_parts(Some("maybe".into()), true);
        assert_eq!(d.resolve(), CiMode::Interactive);
    }

    #[test]
    fn t_env_is_truthy_truth_table() {
        let cases = [
            (Some("1"), true),
            (Some("true"), true),
            (Some("TRUE"), true),
            (Some("True"), true),
            (Some("yes"), true),
            (Some("YES"), true),
            (Some("Yes"), true),
            (Some(" 1 "), true),
            (Some("0"), false),
            (Some("false"), false),
            (Some("no"), false),
            (Some(""), false),
            (None, false),
        ];
        for (input, expected) in cases {
            assert_eq!(
                CiDetection::env_is_truthy(input),
                expected,
                "input={input:?} expected={expected}"
            );
        }
    }

    #[test]
    fn t_resolve_does_not_mutate_inputs() {
        let d = CiDetection::from_parts(Some("yes".into()), false);
        let _ = d.resolve();
        let _ = d.resolve();
        assert_eq!(d.ci_env.as_deref(), Some("yes"));
        assert!(!d.stdin_is_tty);
    }
}
