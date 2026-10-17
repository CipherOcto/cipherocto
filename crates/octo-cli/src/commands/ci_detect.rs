//! CI mode detection helper (RFC-0011-h §Substrate-Additions row G25).
//!
//! Layer C CLI dispatch surface (NOT Layer B substrate) per
//! RFC-0011-l Phase 4 row G25. Determines whether the operator
//! invoking `octo network ...` is an interactive operator or a CI
//! agent.
//!
//! ## THIS MODULE ENFORCES NOTHING. Read this before trusting it.
//!
//! The detection logic below is implemented and unit-tested, and it is
//! correct for what it measures. But **no command calls it**: as of this
//! revision the only references to `CiDetection::detect` outside this
//! file are the `pub mod ci_detect;` re-export in `commands/mod.rs` and
//! a comment in `network.rs` recording that the gate is deferred.
//!
//! Consequently there is no CI write-denial today. The 6
//! CI-DENY-default write subcommands (the rebind-* trio + mode set +
//! authority rotate + slash apply) execute for a CI agent exactly as
//! they do for a human. An earlier revision of this doc comment stated
//! the opposite in the present tense — that CI agents "are blocked",
//! that an escape hatch "lets CI agents opt into" the 6 subcommands, and
//! that this module keeps N caller arms DRY. All three described a
//! control that was never wired, and a security control documented as
//! enforced is worse than one documented as absent: an operator who
//! believes the gate holds will not set the one thing that would have
//! protected them.
//!
//! The deferral is not a bug being papered over — it is the recorded
//! plan. `network.rs` says so at the gate site: "CI gate (slot 90
//! `NetworkCIDenyDefault`) is forward-looking … the slot 90 error variant
//! is deferred to Phase 5/6 per RFC-0011-l row 50."
//!
//! When the gate does land, three things must change together, and this
//! comment should change with them: the 6 handlers must call
//! `CiDetection::detect()` and map `CiAgent` to the slot-90 exit code;
//! `allow_ci_deny_default` must be READ in a production condition (it is
//! currently only ever read by a test assertion); and only then does the
//! "escape hatch" below describe anything real.
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
//! ## Escape hatch — declared, not honoured
//!
//! `--allow-ci-deny-default` (DEBUG-ONLY, hidden from `--help`
//! per RFC-0011 §Security Considerations experimental-flag
//! contract; surfaces in `octo network ... --help-all`) is declared
//! as a clap field on the three network arg structs that will carry
//! the gate. **It is not read in any production condition**, so today
//! it gates nothing; it parses, and then nothing consults it. It is
//! kept as a stable experimental contract: experimental, removed
//! before v1.0.
//!
//! ## Why a separate helper module
//!
//! Putting the CI detection inside the network module would
//! force every rebind-* arm to duplicate the env-var + TTY
//! probe + escape-hatch check. Centralising the helper is what
//! lets the 6 future gate sites share one `CiDetection::detect`
//! boundary. Today it has zero, which is the honest count.

use std::env;
use std::io::IsTerminal;

/// The detected operator mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CiMode {
    /// Operator is a CI agent (env-var set OR stdin not a TTY).
    ///
    /// WARNING: nothing currently branches on this variant — see the
    /// module header. The value is correct; the enforcement is absent.
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
