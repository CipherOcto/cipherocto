//! Single-source-of-truth for operator `$OCTO_HOME` resolution.
//!
//! Per Wave 4.5 Lens-2 findings 3+4: the previous `resolve_octo_home()`
//! helpers in `commands/peer.rs` and `commands/vault.rs` both fell back
//! to `/tmp/.octo` when neither `OCTO_HOME` nor `$HOME` was set — a
//! world-readable/writable directory on shared hosts where any local
//! user could race the operator's writes or inject peer-table entries.
//! Dedupe + fail-closed: every command that needs the home directory
//! routes through this module, which returns `OctoCliError::NoOctoHome`
//! (exit 27) when neither env var is set.
//!
//! The two implementations were byte-identical (modulo comments) so a
//! single shared helper eliminates the divergence hazard as well.

use std::path::PathBuf;

use crate::error::OctoCliError;

/// Source of operator env-var values. Production calls go through
/// [`OsEnv`]; tests inject a fixture without mutating the process
/// environment (parallel-safe).
pub trait Env {
    /// Look up an env var by name. Returns `Some(value)` if set
    /// (including empty strings), `None` if unset.
    fn var(&self, key: &str) -> Option<String>;
}

/// Production [`Env`] impl: reads from the process environment.
pub struct OsEnv;

impl Env for OsEnv {
    fn var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

/// Resolve the operator's `$OCTO_HOME` override (`OCTO_HOME` env var,
/// otherwise `$HOME/.octo`).
///
/// Returns [`OctoCliError::NoOctoHome`] (exit 27) when neither
/// `OCTO_HOME` nor `HOME` is set OR when `OCTO_HOME` is set to the
/// empty string. The CLI never defaults to `/tmp/.octo` — a
/// shared-host directory that any local user can write to, which
/// would let an attacker race the operator's peer-table or vault
/// entries.
///
/// # Layer discipline
///
/// Layer C-only concern — `octo-cli` is the canonical place that
/// resolves operator `$OCTO_HOME` (substrates accept an explicit
/// `&Path` so the substrate stays free of env-var reads).
pub fn resolve() -> Result<PathBuf, OctoCliError> {
    resolve_with(&OsEnv)
}

/// Internal entry point that takes an [`Env`] source. The public
/// [`resolve`] routes through this with [`OsEnv`]; tests inject a
/// fixture (parallel-safe; no process-env mutation).
fn resolve_with(env: &dyn Env) -> Result<PathBuf, OctoCliError> {
    if let Some(p) = env.var("OCTO_HOME") {
        if p.is_empty() {
            return Err(OctoCliError::NoOctoHome);
        }
        return Ok(PathBuf::from(p));
    }
    match env.var("HOME") {
        Some(h) if !h.is_empty() => Ok(PathBuf::from(h).join(".octo")),
        _ => Err(OctoCliError::NoOctoHome),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixture [`Env`] impl backed by a `Vec<(name, value)>` table —
    /// fully parallel-safe (no process-env mutation).
    #[derive(Default)]
    struct FixtureEnv {
        entries: Vec<(String, String)>,
    }

    impl FixtureEnv {
        fn with(mut self, k: &str, v: &str) -> Self {
            self.entries.push((k.to_string(), v.to_string()));
            self
        }
    }

    impl Env for FixtureEnv {
        fn var(&self, key: &str) -> Option<String> {
            self.entries
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
        }
    }

    /// `HOME` set → `$HOME/.octo`.
    #[test]
    fn resolve_uses_home_when_octo_home_unset() {
        let env = FixtureEnv::default().with("HOME", "/tmp/fake-home-for-test");
        let p = resolve_with(&env).unwrap();
        assert_eq!(p, PathBuf::from("/tmp/fake-home-for-test/.octo"));
    }

    /// Explicit `OCTO_HOME` override beats `HOME` default.
    #[test]
    fn resolve_prefers_octo_home_when_set() {
        let env = FixtureEnv::default()
            .with("OCTO_HOME", "/var/lib/octo-custom")
            .with("HOME", "/tmp/should-be-ignored");
        let p = resolve_with(&env).unwrap();
        assert_eq!(p, PathBuf::from("/var/lib/octo-custom"));
    }

    /// Empty `OCTO_HOME` fails closed (no `""` directory).
    #[test]
    fn resolve_rejects_empty_octo_home() {
        let env = FixtureEnv::default()
            .with("OCTO_HOME", "")
            .with("HOME", "/tmp/fake-home-for-test");
        assert!(matches!(resolve_with(&env), Err(OctoCliError::NoOctoHome)));
    }

    /// Both env vars unset → fail closed (no `/tmp/.octo`).
    #[test]
    fn resolve_fails_closed_when_neither_env_var_set() {
        let env = FixtureEnv::default();
        assert!(matches!(resolve_with(&env), Err(OctoCliError::NoOctoHome)));
    }

    /// Empty `HOME` falls through to `NoOctoHome` (no `""` join).
    #[test]
    fn resolve_rejects_empty_home() {
        let env = FixtureEnv::default().with("HOME", "");
        assert!(matches!(resolve_with(&env), Err(OctoCliError::NoOctoHome)));
    }
}
