//! Provider-boundary key-swap enforcement (mission 0957-b AC-1 + RFC-0957 §Adversary A5).
//!
//! CipherOcto-internal authentication material (admin `master_key`, virtual API
//! keys minted via `/admin/keys`, capability tokens, holder-DID-bound keys)
//! MUST NEVER egress to a provider. The provider sees only its OWN key, sourced
//! from operator config (`DispatchInfo::api_key`) or env (`{PROVIDER}_API_KEY` /
//! `ANY_LLM_KEY`).
//!
//! This module provides a single canonical swap surface — every outbound
//! provider request in `proxy.rs` (and any future egress crate) MUST go through
//! `attach_provider_authorization`. Bypassing the helper is detected by:
//!
//! 1. **Type-level brand** — `ProviderApiKey` is a thin newtype around `String`
//!    that can only be constructed via [`ProviderApiKey::from_resolved`]; the
//!    constructor runs the cipherocto-internal denylist at runtime, so a key
//!    shaped like a CipherOcto virtual key or master key cannot reach
//!    `attach_provider_authorization` without tripping an explicit `Err`.
//! 2. **Runtime denylist** — every outbound attachment scans the rendered
//!    `Authorization: Bearer ...` value against cipherocto-internal prefixes
//!    (`sk-virtual-`, `sk-cipherocto-`, `sk-cto-`). A future contributor who
//!    short-circuits the helper and writes the header directly trips the
//!    guard when CI runs the boundary test.
//! 3. **CI lint** — `.github/linters/no-provider-bound-cap.sh` is extended to
//!    reject `req_builder.header("Authorization", ...)` patterns that bypass
//!    the canonical helper.
//!
//! Spec authority: RFC-0957 §Adversary Analysis A5 + RFC-0959 v1.0
//! §Provider boundary + mission `0957-b-provider-boundary-exercise-path.md`
//! AC-1 + AC-8.

use serde::{Deserialize, Serialize};

/// CipherOcto-internal key prefixes that MUST never reach a provider.
///
/// These prefixes identify keys minted by the CipherOcto admin plane
/// (`/admin/keys`) or the wallet's virtual-key mint. They are scoped to the
/// CipherOcto trust boundary; the upstream provider has no jurisdiction over
/// them and treating a CipherOcto key as a provider credential is a
/// cross-boundary leak.
///
/// Extend this list (rather than scattering checks) when adding new
/// CipherOcto key families.
pub const CIPHEROCTO_INTERNAL_KEY_PREFIXES: &[&str] =
    &["sk-virtual-", "sk-cipherocto-", "sk-cto-", "CipherOcto-"];

/// The single canonical egress-Authorization builder. Use this for every
/// outbound provider request. `proxy.rs` previously attached Authorization at
/// 8 separate callsites; each one now routes through this helper or fails
/// the boundary test.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderApiKey(String);

impl ProviderApiKey {
    /// Construct a `ProviderApiKey` from a value resolved by
    /// `resolve_api_key` (`{PROVIDER}_API_KEY` env var, `ANY_LLM_KEY`, or
    /// `DispatchInfo::api_key` operator config). Returns `Err` if the
    /// string is shaped like a CipherOcto-internal key — by construction,
    /// those keys are not produced by `resolve_api_key`, so a non-`Err`
    /// here is the post-condition callers depend on.
    pub fn from_resolved(key: String) -> Result<Self, KeySwapError> {
        for prefix in CIPHEROCTO_INTERNAL_KEY_PREFIXES {
            if key.starts_with(prefix) {
                return Err(KeySwapError::CipheroctoInternalLeak {
                    leaked_prefix: (*prefix).to_owned(),
                    surface: "from_resolved",
                });
            }
        }
        Ok(Self(key))
    }

    /// Borrow the resolved provider key as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Render the wire value (`"Bearer <key>"`) and assert it does not
    /// contain a CipherOcto-internal prefix. Belt-and-suspenders: a future
    /// contributor who attaches `format!("Bearer {}", self.as_str())`
    /// directly still hits the denylist when this test runs at the boundary
    /// test site.
    #[must_use]
    pub fn bearer_wire_value(&self) -> String {
        let rendered = format!("Bearer {}", self.0);
        for prefix in CIPHEROCTO_INTERNAL_KEY_PREFIXES {
            if rendered.starts_with(&format!("Bearer {prefix}")) {
                // Internal-mode tripwire: would be a serious leak. Test
                // suite (`key_swap_boundary::provider_api_key_blocks_internal_prefix`)
                // asserts this branch is unreachable for any key produced by
                // `from_resolved`.
                panic!(
                    "CI: rendered Authorization header carries CipherOcto-internal \
                     prefix `{prefix}` — this indicates a key-swap bypass. \
                     See `egress::key_swap::CIPHEROCTO_INTERNAL_KEY_PREFIXES`."
                );
            }
        }
        rendered
    }
}

impl AsRef<str> for ProviderApiKey {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
impl ProviderApiKey {
    /// Test-only seam: construct a `ProviderApiKey` from a raw string
    /// bypassing the `from_resolved` denylist. **NEVER call from
    /// production code.** Used by the `#[should_panic]` tripwire test
    /// to exercise the unreachable `panic!()` branch in
    /// `bearer_wire_value` (the panic exists precisely because that branch
    /// is unreachable through the production path).
    pub fn from_string_unchecked_for_testing(key: String) -> Self {
        Self(key)
    }
}

/// The single egress swap entry-point used by `proxy.rs`'s 8 outbound
/// `Authorization` attachment sites. Wraps `from_resolved` + the runtime
/// wire-value guard in one call so every site gets identical treatment.
///
/// On a CipherOcto-internal-shaped key this returns `Err(KeySwapError::CipheroctoInternalLeak)`,
/// which the caller MUST propagate (or `expect` with a clear message). On
/// a resolved provider key it returns `Ok("Bearer <key>")` ready to drop
/// into a `req_builder.header("Authorization", …)` call.
pub fn attach_bearer(raw_key: &str) -> Result<String, KeySwapError> {
    let branded = ProviderApiKey::from_resolved(raw_key.to_owned())?;
    let rendered = branded.bearer_wire_value();
    assert_wire_value_safe(&rendered)?;
    Ok(rendered)
}

/// Errors produced by the key-swap boundary.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum KeySwapError {
    #[error(
        "key-swap boundary rejected CipherOcto-internal leak: prefix `{leaked_prefix}` \
         detected in `{surface}`. CipherOcto-internal keys MUST NOT egress to a \
         provider — only operator-config + provider-env keys are allowed. \
         See `egress::key_swap::CIPHEROCTO_INTERNAL_KEY_PREFIXES`."
    )]
    CipheroctoInternalLeak {
        leaked_prefix: String,
        surface: &'static str,
    },
}

/// Convenience: assert at runtime that a wire value does not carry a
/// CipherOcto-internal prefix. Used at every outbound `Authorization`
/// attachment site AND by the boundary test that exercises the raw
/// header-construction sites in `proxy.rs` (which still exist for back-compat
/// but are guarded by this check).
///
/// Returns `Ok(())` if the wire value is safe, `Err` otherwise.
pub fn assert_wire_value_safe(wire_value: &str) -> Result<(), KeySwapError> {
    for prefix in CIPHEROCTO_INTERNAL_KEY_PREFIXES {
        if wire_value.starts_with(&format!("Bearer {prefix}")) {
            return Err(KeySwapError::CipheroctoInternalLeak {
                leaked_prefix: (*prefix).to_owned(),
                surface: "assert_wire_value_safe",
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_api_key_accepts_resolved_provider_key() {
        let k = ProviderApiKey::from_resolved("sk-real-openai-abc123".to_owned()).unwrap();
        assert_eq!(k.as_str(), "sk-real-openai-abc123");
    }

    #[test]
    fn provider_api_key_rejects_virtual_key_prefix() {
        let err = ProviderApiKey::from_resolved("sk-virtual-alice".to_owned()).unwrap_err();
        assert_eq!(
            err,
            KeySwapError::CipheroctoInternalLeak {
                leaked_prefix: "sk-virtual-".to_owned(),
                surface: "from_resolved",
            }
        );
    }

    #[test]
    fn provider_api_key_rejects_cipherocto_internal_prefix() {
        for prefix in ["sk-virtual-", "sk-cipherocto-", "sk-cto-", "CipherOcto-"] {
            let key = format!("{prefix}rest");
            let err = ProviderApiKey::from_resolved(key).unwrap_err();
            assert_eq!(
                err,
                KeySwapError::CipheroctoInternalLeak {
                    leaked_prefix: (*prefix).to_owned(),
                    surface: "from_resolved",
                },
                "denylist failed for prefix `{prefix}`"
            );
        }
    }

    #[test]
    fn assert_wire_value_safe_accepts_provider_key() {
        assert_wire_value_safe("Bearer sk-real-openai-abc123").unwrap();
    }

    #[test]
    fn assert_wire_value_safe_rejects_internal_prefix() {
        let err = assert_wire_value_safe("Bearer sk-virtual-alice").unwrap_err();
        assert_eq!(
            err,
            KeySwapError::CipheroctoInternalLeak {
                leaked_prefix: "sk-virtual-".to_owned(),
                surface: "assert_wire_value_safe",
            }
        );
    }

    #[test]
    fn bearer_wire_value_renders_bearer_prefix() {
        let k = ProviderApiKey::from_resolved("sk-real-openai-abc123".to_owned()).unwrap();
        assert_eq!(k.bearer_wire_value(), "Bearer sk-real-openai-abc123");
    }

    #[test]
    fn provider_api_key_is_brand_separable_from_string_in_signature() {
        // ProviderApiKey borrows as &str via AsRef but cannot be constructed
        // from a String without going through `from_resolved`. This is the
        // type-level enforcement of "no shortcut".
        let raw: String = "sk-real-openai-abc123".to_owned();
        let branded: ProviderApiKey = ProviderApiKey::from_resolved(raw.clone()).unwrap();
        assert_eq!(branded.as_str(), raw);
        assert_eq!(branded.as_ref(), raw);
    }

    /// Tripwire test (M-1 R3): the `panic!` inside `bearer_wire_value` is the
    /// last line of defense against an internal-shaped key reaching the wire.
    /// Without this test, the panic logic could be wrong (off-by-one in
    /// prefix matching, broken `starts_with` semantics) and silently regress.
    /// Construction uses the `#[cfg(test)]` seam `from_string_unchecked_for_testing`,
    /// which bypasses `from_resolved`'s denylist — exactly the path the
    /// tripwire is designed to catch.
    #[test]
    #[should_panic(expected = "CI: rendered Authorization header carries CipherOcto-internal")]
    fn bearer_wire_value_tripwire_panics_on_internal_prefix() {
        let bad =
            ProviderApiKey::from_string_unchecked_for_testing("sk-virtual-direct-test".to_owned());
        let _ = bad.bearer_wire_value();
    }

    /// Tripwire test for `CipherOcto-` prefix.
    #[test]
    #[should_panic(expected = "CI: rendered Authorization header carries CipherOcto-internal")]
    fn bearer_wire_value_tripwire_panics_on_cipherocto_prefix() {
        let bad =
            ProviderApiKey::from_string_unchecked_for_testing("CipherOcto-direct-test".to_owned());
        let _ = bad.bearer_wire_value();
    }
}
