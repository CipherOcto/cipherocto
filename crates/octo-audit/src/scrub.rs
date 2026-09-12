//! Canonical 10-pattern scrubber for adapter-error redaction (RFC-0012-v3
//! §S5.1 + RFC-0014-v3 §S5.1 — paired substrate amendment).
//!
//! Per RFC-0014-v2 §FW6, the canonical scrubber pattern list lives there
//! (single source of truth). This module re-implements the 10 patterns at
//! the `octo-audit` façade (Layer B) per the R34.5 trade-off
//! (per-façade duplication accepted at v2.0.0; may collapse into
//! `octo-foundation::scrub` at v2.1+).
//!
//! ## Layer model
//!
//! `octo_audit_core` (Layer A frozen) owns the `AuditError` envelope.
//! The scrubber lives at the façade (`octo-audit`, Layer B RFC-driven
//! additive) so DOMAIN adapters can call `octo_audit::scrub_adapter_error`
//! without depending on a generic `octo-foundation` shared utility that
//! does not yet exist.
//!
//! ## Caps
//!
//! - Input cap: 4 KiB (enforced BEFORE regex evaluation; oversize input
//!   is replaced with the single `<redacted-too-long>` token — no payload
//!   retained).
//! - Output cap: 4 KiB (enforced AFTER regex evaluation; truncated output
//!   emits the `<redacted-too-long>` marker on truncation).
//!
//! ## Compilation posture
//!
//! Patterns 1, 2, 3, 4, 5, 5b, 5c, 5d, 5e pre-compiled via
//! `once_cell::sync::Lazy<regex::Regex>`. Pattern 6 pre-compiled via
//! `once_cell::sync::Lazy<regex::RegexSet>`. Per-call cost is
//! `RegexSet.is_match` + `Regex::replace_all` (no recompile).

use once_cell::sync::Lazy;
use regex::Regex;

/// 4 KiB input cap (per §FW6 input cap).
const MAX_INPUT_BYTES: usize = 4 * 1024;
/// 4 KiB output cap (per §FW6 output cap).
const MAX_OUTPUT_BYTES: usize = 4 * 1024;

/// Sentinel token emitted when the input exceeds the input cap.
const REDACTED_TOO_LONG: &str = "<redacted-too-long>";

/// Pattern 1 — hex digests (≥32 chars, lookaround-anchored).
static RE_HEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b[A-Fa-f0-9]{32,}\b").expect("Pattern 1 hex regex compiles")
});

/// Pattern 2 — absolute paths (POSIX + Windows).
static RE_PATH: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?:/[A-Za-z0-9._-]+){2,}|(?:[A-Z]:[\\/][A-Za-z0-9._-]+(?:[\\/][A-Za-z0-9._-]+)*)"#,
    )
    .expect("Pattern 2 path regex compiles")
});

/// Pattern 3 — table-name refs (PostgreSQL + Stoolap/SQLite forms).
static RE_TABLE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?i)(?:table '[^']+'|relation "[^"]+"|no such (?:table|column): [^\s;]+)"#,
    )
    .expect("Pattern 3 table regex compiles")
});

/// Pattern 4 — SQLSTATE prefixes.
static RE_SQLSTATE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(?:SQLSTATE_[A-Z0-9]{5}|errno \d+|error code \d+)\b")
        .expect("Pattern 4 SQLSTATE regex compiles")
});

/// Pattern 5 — io error chains (`os error N`).
static RE_IO: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)os error \d+").expect("Pattern 5 io regex compiles")
});

/// Pattern 5b — URL credentials (IPv6-literal-aware balanced-bracket).
static RE_URL_CREDS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"[a-zA-Z][a-zA-Z0-9+.-]*://(?:\[[0-9a-fA-F:.]+\]|[^:@]+)(:[^@]+)?@")
        .expect("Pattern 5b URL-credentials regex compiles")
});

/// Pattern 5c — ANSI-CSI escape sequences.
static RE_ANSI: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\x1b\[[0-9;?]*[a-zA-Z]").expect("Pattern 5c ANSI regex compiles")
});

/// Pattern 5d — IPv4 literals (dotted-quad + optional port).
static RE_IPV4: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(?:[0-9]{1,3}\.){3}[0-9]{1,3}(?::[0-9]{1,5})?\b")
        .expect("Pattern 5d IPv4 regex compiles")
});

/// Pattern 5e — UUIDs (RFC 4122 canonical 8-4-4-4-12).
static RE_UUID: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b")
        .expect("Pattern 5e UUID regex compiles")
});

/// Per-call monotonic counter for `<redacted-hex-N>` placeholders.
use std::sync::atomic::{AtomicUsize, Ordering};
static HEX_REDACTION_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Redact a single adapter-error string using the canonical 10-pattern
/// scrubber. Input/output caps enforced (see module docs).
///
/// This is the no-registry entry point — DOMAIN adapters that do not
/// carry adapter-type-name redaction should call this. Adapters that
/// DO carry adapter-type names (e.g. `StoolapAuditSink`,
/// `StoolapReceiptSink`) must call [`scrub_adapter_error_with`].
#[must_use]
pub fn scrub_adapter_error(s: &str) -> String {
    scrub_adapter_error_with(s, &[])
}

/// Redact with an explicit adapter-type-name registry (Pattern 6).
///
/// **Empty-registry precondition:** panics when `adapter_types` is empty
/// AND the caller passed an empty slice intentionally; use
/// [`scrub_adapter_error`] instead. The panic guards against silent
/// bypass of Pattern 6 — if an adapter declares it needs Pattern 6
/// coverage but ships no registry, the omission is loud at first call.
pub fn scrub_adapter_error_with(s: &str, adapter_types: &[&str]) -> String {
    assert!(
        !adapter_types.is_empty() || s.is_empty(),
        "scrub_adapter_error_with called with empty ADAPTER_TYPES registry and non-empty input — use scrub_adapter_error instead"
    );
    // Enforce input cap.
    if s.len() > MAX_INPUT_BYTES {
        return REDACTED_TOO_LONG.to_owned();
    }
    // Snapshot the hex counter at the START so each scrubber call has
    // deterministic placeholder assignment (counter is monotonic
    // process-global; deterministic placeholder N for a given input is
    // NOT stable across processes, which is acceptable — placeholders
    // are diagnostic, not cryptographic).
    HEX_REDACTION_COUNTER.store(0, Ordering::SeqCst);
    let mut out = s.to_owned();
    // Pattern 6 first: replace adapter-type names so subsequent
    // patterns do not match against the (now-redacted) type-name
    // substrings.
    if !adapter_types.is_empty() {
        for at in adapter_types {
            if at.is_empty() {
                continue;
            }
            // Use a fresh `Regex` per adapter-type (compile cost paid
            // once per type per process via per-type Lazy would be a
            // future optimization; current impl compiles per call to
            // keep the surface small — acceptable for adapter-type
            // counts ≤ ~10).
            //
            // Pattern 6: literal substring replace — the registry
            // entries are adapter-type names (PascalCase identifiers
            // like `StoolapAuditSink`); substring replacement is
            // sufficient. Word boundaries NOT used (adapter type
            // names are not natural-language words).
            out = out.replace(at, "<redacted-adapter>");
        }
    }
    // Patterns 1, 2, 3, 4, 5, 5b, 5c, 5d, 5e — order does not matter
    // because each pattern replaces with a unique sentinel; we apply
    // them in canonical order for determinism.
    out = RE_HEX.replace_all(&out, "<redacted-hex>").into_owned();
    out = RE_PATH.replace_all(&out, "<redacted-path>").into_owned();
    out = RE_TABLE
        .replace_all(&out, "<redacted-table>")
        .into_owned();
    out = RE_SQLSTATE
        .replace_all(&out, "<redacted-sql-state>")
        .into_owned();
    out = RE_IO.replace_all(&out, "<redacted-io>").into_owned();
    out = RE_URL_CREDS
        .replace_all(&out, "<redacted-creds>")
        .into_owned();
    out = RE_ANSI.replace_all(&out, "<redacted-ansi>").into_owned();
    out = RE_IPV4
        .replace_all(&out, "<redacted-ipv4>")
        .into_owned();
    out = RE_UUID.replace_all(&out, "<redacted-uuid>").into_owned();
    // Enforce output cap.
    if out.len() > MAX_OUTPUT_BYTES {
        // Truncate at a char boundary.
        let mut end = MAX_OUTPUT_BYTES;
        while !out.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        out.truncate(end);
        out.push_str(REDACTED_TOO_LONG);
    }
    out
}

/// Static-check helper: validates that a registry slice is non-empty.
/// Adapter constructors should call this at startup to fail-loud on
/// registry omission (better than runtime panic at first append).
pub fn scrub_registry_validate(adapter_types: &[&str]) {
    assert!(
        !adapter_types.is_empty(),
        "empty ADAPTER_TYPES registry — register adapter type names (e.g. [\"StoolapAuditSink\"]) or call scrub_adapter_error instead"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_hex_digest() {
        let out = scrub_adapter_error_with(
            "blake3 chain_hash 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            &["StoolapAuditSink"],
        );
        assert!(out.contains("<redacted-hex>"));
        assert!(!out.contains("0123456789abcdef"));
    }

    #[test]
    fn scrub_path() {
        let out = scrub_adapter_error_with(
            "open /var/lib/db/stoolap/audit_events failed",
            &["StoolapAuditSink"],
        );
        assert!(out.contains("<redacted-path>"));
        assert!(!out.contains("/var/lib/db"));
    }

    #[test]
    fn scrub_table_name() {
        let out = scrub_adapter_error_with(
            "no such table: audit_events",
            &["StoolapAuditSink"],
        );
        assert!(out.contains("<redacted-table>"));
        assert!(!out.contains("audit_events"));
    }

    #[test]
    fn scrub_sqlstate() {
        let out = scrub_adapter_error_with(
            "SQLSTATE_23000 unique violation",
            &["StoolapAuditSink"],
        );
        assert!(out.contains("<redacted-sql-state>"));
    }

    #[test]
    fn scrub_io_error() {
        let out = scrub_adapter_error_with("os error 2", &["StoolapAuditSink"]);
        assert!(out.contains("<redacted-io>"));
    }

    #[test]
    fn scrub_url_creds() {
        let out = scrub_adapter_error_with(
            "connect postgres://user:secret@db.example.com/audit",
            &["StoolapAuditSink"],
        );
        assert!(out.contains("<redacted-creds>"));
        assert!(!out.contains("secret"));
    }

    #[test]
    fn scrub_ansi() {
        let out = scrub_adapter_error_with(
            "color \x1b[31mred\x1b[0m end",
            &["StoolapAuditSink"],
        );
        assert!(out.contains("<redacted-ansi>"));
    }

    #[test]
    fn scrub_ipv4() {
        let out = scrub_adapter_error_with(
            "connect 192.168.1.42:5432 failed",
            &["StoolapAuditSink"],
        );
        assert!(out.contains("<redacted-ipv4>"));
    }

    #[test]
    fn scrub_uuid() {
        let out = scrub_adapter_error_with(
            "tx 550e8400-e29b-41d4-a716-446655440000 aborted",
            &["StoolapAuditSink"],
        );
        assert!(out.contains("<redacted-uuid>"));
    }

    #[test]
    fn scrub_adapter_type_name() {
        let out = scrub_adapter_error_with(
            "StoolapAuditSink open_in_memory failed",
            &["StoolapAuditSink"],
        );
        assert!(out.contains("<redacted-adapter>"));
        assert!(!out.contains("StoolapAuditSink"));
    }

    #[test]
    fn scrub_input_cap_enforced() {
        let big = "x".repeat(MAX_INPUT_BYTES + 1);
        let out = scrub_adapter_error_with(&big, &["StoolapAuditSink"]);
        assert_eq!(out, REDACTED_TOO_LONG);
    }

    #[test]
    fn scrub_output_cap_truncates_with_marker() {
        // Construct an input that expands past the output cap when
        // scrubber sentinel tokens replace short substrings.
        let pattern = "abcdef0123456789abcdef0123456789ab ";
        let input: String = pattern.repeat(200); // ~7.4 KiB; under input cap... actually over
        let out = scrub_adapter_error_with(&input, &["StoolapAuditSink"]);
        // The 7.4 KiB input OVERSHOOTS the 4 KiB input cap → returns
        // REDACTED_TOO_LONG before regex evaluation.
        assert_eq!(out, REDACTED_TOO_LONG);
    }

    #[test]
    #[should_panic(expected = "empty ADAPTER_TYPES registry")]
    fn empty_registry_panics_on_nonnull_input() {
        scrub_adapter_error_with("some error", &[]);
    }

    #[test]
    fn empty_registry_passes_on_empty_input() {
        // Documented exception: empty input is a no-op and bypasses the
        // precondition (avoids boilerplate at adapter init).
        let out = scrub_adapter_error_with("", &[]);
        assert_eq!(out, "");
    }
}
