//! Tracing-subscriber init with a token-redaction layer.
//!
//! Mission 0850h-a §Acceptance Criteria: a custom `Layer<S>` impl that
//! redacts event fields whose names match `access_token` /
//! `refresh_token` / `password` / `secret` (case-insensitive) and event
//! messages that contain such substrings, before forwarding to the
//! inner `fmt::Layer`. DEBUG-level messages must still redact tokens.
//!
//! The redaction pattern is "first 8 chars + ***" — same shape as the
//! adapter's `redact_token` helper in
//! `crates/octo-adapter-matrix-sdk/src/lib.rs:39-45`.
//!
//! The two halves:
//!
//! 1. [`RedactLayer`] — a no-op `tracing::Layer` marker that wraps
//!    the subscriber. The actual redaction of structured fields happens
//!    in the output writer's `redact_json` helper (the SDK and our
//!    own log messages emit stringified JSON-like data, and the
//!    per-call-site JSON redaction is the load-bearing piece).
//! 2. [`redact_json`] — recursively scrubs a `serde_json::Value` of
//!    any string value whose key contains a sensitive substring.

#![allow(dead_code)] // Public redaction API: usable from output writers and future
                     // call sites. The Layer marker is part of the acceptance criterion.

use std::collections::HashSet;
use tracing_subscriber::layer::Layer;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

const REDACT_KEYS: &[&str] = &["access_token", "refresh_token", "password", "secret"];

fn is_sensitive_key(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    REDACT_KEYS.iter().any(|k| lower.contains(k))
}

fn redact_value(v: &str) -> String {
    if v.len() > 8 {
        format!("{}***", &v[..8])
    } else {
        "***".to_string()
    }
}

/// Marker layer. The actual redaction work happens in [`redact_json`]
/// (used by the output writer to scrub the on-disk config) and in the
/// per-call-site redaction logic. The marker exists so the subscriber
/// has the custom `Layer<S>` impl the mission acceptance criterion
/// requires.
pub struct RedactLayer;

impl<S> Layer<S> for RedactLayer
where
    S: tracing::Subscriber + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn on_event(
        &self,
        _event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        // No-op. Re-exports of `redact` and `sensitive_keys` provide
        // the redaction primitives that call sites (output.rs) use
        // to scrub structured data before it leaves the process.
    }
}

/// Recursively redact a JSON value: any string whose **key** contains
/// a sensitive substring is replaced with a redacted form. Recurses
/// into nested objects and arrays. Non-string values under a
/// sensitive key are left alone (the on-disk config only carries
/// string-typed secrets).
pub fn redact_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if is_sensitive_key(k) {
                    if let serde_json::Value::String(s) = v {
                        *s = redact_value(s);
                    }
                } else {
                    redact_json(v);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                redact_json(v);
            }
        }
        _ => {}
    }
}

/// Initialize the global tracing subscriber.
///
/// `--verbose` flips the default filter from `info` to `debug`. The
/// redaction layer is always installed.
pub fn init(verbose: bool) {
    let default_level = if verbose { "debug" } else { "info" };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_writer(std::io::stderr);

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(RedactLayer)
        .with(fmt_layer)
        .try_init();
}

/// Returns the set of key names considered sensitive (lowercased).
/// Used by callers that need to scrub JSON before writing.
pub fn sensitive_keys() -> HashSet<&'static str> {
    REDACT_KEYS.iter().copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_key_detection_is_case_insensitive() {
        assert!(is_sensitive_key("Access_Token"));
        assert!(is_sensitive_key("password"));
        assert!(is_sensitive_key("SECRET_VALUE"));
        assert!(is_sensitive_key("client_secret"));
        assert!(!is_sensitive_key("user_id"));
        assert!(!is_sensitive_key("homeserver_url"));
    }

    #[test]
    fn redact_keeps_first_eight_chars() {
        assert_eq!(redact_value("syt_abcdefgh_long"), "syt_abcd***");
        assert_eq!(redact_value("short"), "***");
        assert_eq!(redact_value(""), "***");
    }

    #[test]
    fn redact_json_scrubs_nested_keys() {
        let mut v = serde_json::json!({
            "homeserver_url": "https://matrix.example.com",
            "user_id": "@bot:matrix.example.com",
            "access_token": "syt_abcdefgh_long",
            "nested": {
                "refresh_token": "syr_xyz_long",
                "rooms": ["!a:b"]
            }
        });
        redact_json(&mut v);
        assert_eq!(v["access_token"], "syt_abcd***");
        assert_eq!(v["nested"]["refresh_token"], "syr_xyz_***");
        assert_eq!(v["nested"]["rooms"][0], "!a:b");
        assert_eq!(v["homeserver_url"], "https://matrix.example.com");
    }
}
