//! Substrate-side second-pass scrubber applied at the façade
//! boundary (RFC-0016-a §6.8).
//!
//! Layer B façade-only — the canonical 18-pattern substrate-side
//! scrubber at `octo_audit::scrub::scrub_adapter_error` is applied
//! by `redact_substrate_error` BEFORE constructing CLI envelopes.
//! When ANY of the 18 patterns matches, the entire payload is
//! replaced with the canonical `<REDACTED>` marker per RFC-0016-a
//! §6.8 (the marker is preserved verbatim on subsequent passes per
//! the R21 L-2 idempotency rule).

use crate::scrub::scrub_adapter_error;

/// Canonical `<REDACTED>` marker emitted when any of the 18
/// substrate-side scrubber patterns matches the input (RFC-0016-a
/// §6.8). The marker is preserved verbatim on subsequent scrub
/// passes per the R21 L-2 idempotency rule.
const REDACTED_MARKER: &str = "<REDACTED>";

/// Apply the canonical 18-pattern substrate-side scrubber to `raw`
/// (RFC-0016-a §6.8 second-pass defense-in-depth at the façade
/// boundary).
///
/// - If `raw` matches NO scrub pattern, the verbatim string is
///   returned (no allocation).
/// - If `raw` DOES match any pattern, the canonical `<REDACTED>`
///   marker is returned (replaces the entire payload, not just the
///   matched span — the spec mandates "any pattern match → emit
///   `<REDACTED>`" rather than per-sentinel substitution at the
///   façade boundary).
/// - The `<REDACTED>` marker itself is preserved verbatim on
///   subsequent passes (R21 L-2 idempotency).
#[must_use]
pub fn redact_substrate_error(raw: &str) -> String {
    let scrubbed = scrub_adapter_error(raw);
    if scrubbed == raw {
        raw.to_string()
    } else {
        REDACTED_MARKER.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_text_passes_through_verbatim() {
        let s = redact_substrate_error("hello world");
        assert_eq!(s, "hello world");
    }

    #[test]
    fn hex_digest_collapses_to_redacted_marker() {
        // 64-char hex matches Pattern 1 (≥32 hex). Per spec §6.8
        // the entire payload collapses to `<REDACTED>` rather than
        // being replaced by the per-pattern `<redacted-hex>`
        // sentinel (that sentinel is the substrate-side form; the
        // façade boundary collapses to the canonical marker).
        let hex64 = "a".repeat(64);
        let s = redact_substrate_error(&hex64);
        assert_eq!(
            s, "<REDACTED>",
            "matched pattern MUST collapse to <REDACTED>, got: {s}",
        );
        assert!(!s.contains(&hex64), "raw hex MUST NOT survive, got: {s}");
    }

    #[test]
    fn redacted_marker_is_idempotent() {
        // R21 L-2 idempotency rule: the literal `<REDACTED>` marker
        // is preserved verbatim, not double-scrubbed.
        let s = redact_substrate_error("user-supplied field: <REDACTED>");
        assert!(s.contains("<REDACTED>"));
        assert!(
            !s.contains("<<REDACTED>"),
            "must NOT double-redact the marker, got: {s}",
        );
    }

    #[test]
    fn pem_block_collapses_to_redacted_marker() {
        let pem = "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAA\n-----END RSA PRIVATE KEY-----";
        let s = redact_substrate_error(pem);
        assert_eq!(
            s, "<REDACTED>",
            "PEM block MUST collapse to <REDACTED>, got: {s}",
        );
    }

    #[test]
    fn jwt_three_segment_collapses_to_redacted_marker() {
        let jwt =
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ1c2VyIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        let s = redact_substrate_error(jwt);
        assert_eq!(s, "<REDACTED>", "JWT MUST collapse to <REDACTED>, got: {s}");
    }
}
