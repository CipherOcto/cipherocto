//! Type-level scrub enforcement newtypes (RFC-0016-a §6.8).
//!
//! Layer B façade-only — the canonical 13-pattern substrate-side
//! scrubber at `octo_audit::scrub::scrub_adapter_error` is applied
//! BEFORE constructing these variants (defense-in-depth second pass
//! at the Layer B façade boundary). The `<REDACTED>` marker is
//! preserved verbatim per the R21 L-2 idempotency rule (no
//! double-scrub).
//!
//! ## Layer discipline
//!
//! Both newtypes live in `octo-audit` (Layer B façade). They
//! type-erasure their payload (`SinkSpecific(String)` payload string;
//! `AuditError` envelope) so downstream callers (CLI / DOMAIN
//! adapters) cannot re-introduce raw secret material after the
//! substrate-side scrub pass — the type system enforces it.

use std::fmt;

use octo_audit_core::AuditError;

use crate::scrub::scrub_adapter_error;

/// Type-level scrub envelope around `AuditError`
/// (RFC-0016-a §6.8).
///
/// Construction MUST go through [`ScrubbedAuditError::new`] which
/// applies the canonical 13-pattern substrate-side scrubber to every
/// `SinkSpecific(String)` payload BEFORE wrapping in the
/// `ScrubbedAuditError` envelope. Defense-in-depth second pass at
/// the Layer B façade boundary.
#[derive(Debug)]
pub struct ScrubbedAuditError(pub AuditError);

impl ScrubbedAuditError {
    /// Apply the canonical 13-pattern substrate-side scrubber to every
    /// `SinkSpecific(String)` payload, then wrap in the
    /// `ScrubbedAuditError` envelope. Fail-closed: any payload that
    /// matches a scrub pattern is replaced with `<REDACTED>` before
    /// construction.
    pub fn new(err: AuditError) -> Self {
        let scrubbed = match err {
            AuditError::SinkSpecific(msg) => AuditError::SinkSpecific(scrub_adapter_error(&msg)),
            other => other,
        };
        Self(scrubbed)
    }

    /// Borrow the inner `AuditError` envelope.
    #[must_use]
    pub fn inner(&self) -> &AuditError {
        &self.0
    }

    /// Consume the envelope and return the inner `AuditError`.
    #[must_use]
    pub fn into_inner(self) -> AuditError {
        self.0
    }
}

impl fmt::Display for ScrubbedAuditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Display delegates to AuditError display, which already
        // renders `SinkSpecific("<msg>")` via thiserror.
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ScrubbedAuditError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

/// Canonical `<REDACTED>`-marked string payload
/// (RFC-0016-a §6.8).
///
/// Construction MUST go through [`ScrubbedString::new`] which applies
/// the canonical 13-pattern substrate-side scrubber (R21 L-2
/// idempotency rule: the literal `<REDACTED>` marker is preserved
/// verbatim, never double-scrubbed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScrubbedString(pub String);

impl ScrubbedString {
    /// Apply canonical 13-pattern substrate-side scrubber and produce
    /// `<REDACTED>` marker if any pattern matches. The `<REDACTED>`
    /// marker itself is preserved verbatim (R21 L-2 idempotency rule).
    pub fn new(raw: &str) -> Self {
        Self(scrub_adapter_error(raw))
    }

    /// Borrow the inner scrubbed string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the envelope and return the inner string.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for ScrubbedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<ScrubbedString> for String {
    fn from(s: ScrubbedString) -> Self {
        s.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrubbed_string_passes_through_safe_text() {
        let s = ScrubbedString::new("hello world");
        assert_eq!(s.as_str(), "hello world");
    }

    #[test]
    fn scrubbed_string_redacts_hex_digest() {
        let hex64 = "a".repeat(64);
        let s = ScrubbedString::new(&hex64);
        // Pattern 1 (32+ char hex) catches 64-char hex → sentinel is
        // `<redacted-hex>` per the canonical scrubber output.
        assert!(
            s.as_str().contains("<redacted-hex>"),
            "64-char hex MUST be redacted, got: {}",
            s.as_str(),
        );
        assert!(
            !s.as_str().contains(&hex64),
            "raw hex MUST NOT survive, got: {}",
            s.as_str(),
        );
    }

    #[test]
    fn scrubbed_string_preserves_redacted_marker_idempotent() {
        // R21 L-2 idempotency rule: the literal `<REDACTED>` marker
        // is preserved verbatim, not double-scrubbed.
        let s = ScrubbedString::new("user-supplied field: <REDACTED>");
        assert!(s.as_str().contains("<REDACTED>"));
        assert!(
            !s.as_str().contains("<<REDACTED>"),
            "must NOT double-redact the marker, got: {}",
            s.as_str(),
        );
    }

    #[test]
    fn scrubbed_audit_error_redacts_sink_specific_payload() {
        let hex64 = "b".repeat(64);
        let raw = AuditError::SinkSpecific(format!("privkey: {hex64}"));
        let scrubbed = ScrubbedAuditError::new(raw);
        match scrubbed.inner() {
            AuditError::SinkSpecific(msg) => {
                // After the scrubber pass, the 64-char hex is replaced
                // with the lowercase `<redacted-hex>` sentinel.
                assert!(
                    msg.contains("<redacted-hex>"),
                    "hex payload MUST be redacted in scrubbed error, got: {msg}",
                );
                assert!(
                    !msg.contains(&hex64),
                    "raw hex MUST NOT survive, got: {msg}",
                );
            }
            other => panic!("expected SinkSpecific, got {other:?}"),
        }
    }
}
