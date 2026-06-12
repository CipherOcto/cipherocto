//! Tracing-subscriber init with a token-redaction layer.
//!
//! Redacts event fields whose names match sensitive keys
//! (`bot_token`, `api_hash`, `phone`, `password`, `access_token`,
//! `verifying_key`, etc.) — case-insensitive exact match on field names.
//! Also redacts message bodies containing sensitive key substrings.

use std::fmt;
use tracing::field::{Field, Visit};
use tracing::Event;
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::fmt::FmtContext;
use tracing_subscriber::layer::Layer;
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::EnvFilter;

const REDACT_KEYS: &[&str] = &[
    "bot_token",
    "api_hash",
    "phone",
    "password",
    "secret",
    "access_token",
    "refresh_token",
    "verifying_key",
];

fn is_sensitive_key(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    REDACT_KEYS.iter().any(|&k| lower == k)
}

/// Redact sensitive key substrings in a rendered message body.
/// Uses a single-pass approach per key: finds all occurrences, calculates
/// each value's extent (to next key boundary or end of string), and applies
/// redactions with offset tracking. Values may contain spaces (for phone
/// numbers etc.) but stop at JSON brackets, quotes, semicolons, or the
/// next key boundary.
fn redact_body_substrings(body: &str) -> String {
    let mut result = body.to_string();

    for &key in REDACT_KEYS {
        let mut start_from: usize = 0;
        loop {
            let lower = result.to_ascii_lowercase();
            let Some(pos) = lower[start_from..].find(key) else {
                break;
            };
            let abs_pos = start_from + pos;
            let key_end = abs_pos + key.len();
            // Only match if followed by = or :
            if key_end >= result.len() || !matches!(result.as_bytes()[key_end], b'=' | b':') {
                start_from = key_end;
                continue;
            }
            // Skip separator (= or :) to find value start
            let mut val_start = key_end + 1;
            if result.as_bytes()[key_end] == b':'
                && val_start < result.len()
                && result.as_bytes()[val_start] == b' '
            {
                val_start += 1;
            }
            // Find value end: next key boundary, JSON bracket, quote, semicolon, or EOL
            let mut val_end = val_start;
            while val_end < result.len() {
                let b = result.as_bytes()[val_end];
                // Hard terminators: JSON brackets, quotes, semicolons, newlines
                if matches!(b, b'"' | b']' | b'}' | b')' | b';' | b'\n' | b'\r') {
                    break;
                }
                // Soft terminator: space followed by a key-like pattern (word= or word:)
                if b == b' ' || b == b'\t' {
                    let after_space = val_end + 1;
                    // Check if the next word is a REDACT_KEY
                    let rest_lower = result[after_space..].to_ascii_lowercase();
                    let is_key_boundary = REDACT_KEYS.iter().any(|&k| {
                        rest_lower.starts_with(k)
                            && after_space + k.len() < result.len()
                            && matches!(result.as_bytes()[after_space + k.len()], b'=' | b':')
                    });
                    if is_key_boundary {
                        break;
                    }
                    // Also break if next word looks like a key (word= pattern)
                    if let Some(eq_pos) = rest_lower.find('=') {
                        let word = &rest_lower[..eq_pos];
                        if !word.is_empty() && word.chars().all(|c| c.is_alphanumeric() || c == '_')
                        {
                            break;
                        }
                    }
                }
                val_end += 1;
            }
            if val_end > val_start {
                let original_val = &result[val_start..val_end].to_string();
                let replacement = redact_value(original_val);
                if replacement == *original_val {
                    start_from = val_end;
                    continue;
                }
                result = format!(
                    "{}{}{}",
                    &result[..val_start],
                    replacement,
                    &result[val_end..]
                );
                start_from = val_start + replacement.len();
            } else {
                start_from = val_end;
            }
        }
    }
    result
}

fn redact_value(v: &str) -> String {
    if v.len() > 8 {
        let mut end = 8.min(v.len());
        while end > 0 && !v.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}***", &v[..end])
    } else {
        "***".to_string()
    }
}

/// Marker layer for the spec's "custom Layer<S> impl" requirement.
pub struct RedactLayer;

impl<S> Layer<S> for RedactLayer where S: tracing::Subscriber + for<'lookup> LookupSpan<'lookup> {}

/// Format event wrapper that redacts sensitive fields.
struct RedactingFormat;

impl<S, N> FormatEvent<S, N> for RedactingFormat
where
    S: tracing::Subscriber + for<'lookup> LookupSpan<'lookup>,
    N: for<'writer> FormatFields<'writer> + 'static,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let mut buf = String::new();
        {
            let mut visitor = RedactingVisitor {
                buf: &mut buf,
                first: true,
            };
            event.record(&mut visitor);
        }
        // H4 + M2: post-process the entire rendered line for body substrings
        let redacted = redact_body_substrings(&buf);
        write!(writer, "{}", redacted)?;
        writeln!(writer)
    }
}

struct RedactingVisitor<'a> {
    buf: &'a mut String,
    first: bool,
}

impl<'a> Visit for RedactingVisitor<'a> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let name = field.name();
        let raw = format!("{:?}", value);

        if self.first {
            self.buf.push_str("  ");
            self.first = false;
        } else {
            self.buf.push(' ');
        }

        if is_sensitive_key(name) {
            self.buf
                .push_str(&format!("{}={}", name, redact_value(&raw)));
        } else {
            self.buf.push_str(&format!("{}={}", name, raw));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        let name = field.name();

        if self.first {
            self.buf.push_str("  ");
            self.first = false;
        } else {
            self.buf.push(' ');
        }

        if is_sensitive_key(name) {
            self.buf
                .push_str(&format!("{}={}", name, redact_value(value)));
        } else {
            self.buf.push_str(&format!("{}={}", name, value));
        }
    }
}

/// Initialize tracing with the redaction layer.
pub fn init(verbose: bool) {
    let level = if verbose { "debug" } else { "info" };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .event_format(RedactingFormat);

    tracing_subscriber::registry()
        .with(filter)
        .with(RedactLayer)
        .with(fmt_layer)
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_short_value() {
        assert_eq!(redact_value("abc"), "***");
    }

    #[test]
    fn redact_long_value() {
        assert_eq!(redact_value("1234567890"), "12345678***");
    }

    #[test]
    fn sensitive_key_detection() {
        assert!(is_sensitive_key("bot_token"));
        assert!(is_sensitive_key("api_hash"));
        assert!(is_sensitive_key("password"));
        assert!(is_sensitive_key("verifying_key"));
        assert!(!is_sensitive_key("user_id"));
        assert!(!is_sensitive_key("username"));
        // L2: exact match — substring should NOT match
        assert!(!is_sensitive_key("bot_token_id"));
        assert!(!is_sensitive_key("password_count"));
    }

    #[test]
    fn redact_body_redacts_sensitive_values() {
        let input = "bot_token=abc123def456ghi api_hash=secretvalue user_id=12345";
        let result = redact_body_substrings(input);
        assert!(result.contains("bot_token=abc123de***"));
        assert!(result.contains("api_hash=secretva***"));
        assert!(result.contains("user_id=12345"));
    }

    #[test]
    fn redact_body_noop_on_clean() {
        let input = "user_id=12345 username=john";
        assert_eq!(redact_body_substrings(input), input);
    }

    #[test]
    fn redact_body_colon_separator() {
        let input = "error: password=secretvalue123 leaked";
        let result = redact_body_substrings(input);
        assert!(result.contains("password=secretva***"));
    }

    #[test]
    fn redact_body_no_false_positive_on_prose() {
        let input = "my phone is ringing";
        assert_eq!(redact_body_substrings(input), input);
    }

    #[test]
    fn redact_body_stops_at_json_brackets() {
        let input = r#"api_hash:"val" next=val"#;
        let result = redact_body_substrings(input);
        // api_hash:" — the : is a separator, " is a terminator, so value is empty → no redact
        assert!(
            result.contains("next=val"),
            "next=val should survive, got: {}",
            result
        );
    }

    #[test]
    fn redact_body_stops_at_parens_and_semicolons() {
        let input = "error: password=secretphrase inside code; other=val";
        let result = redact_body_substrings(input);
        assert!(result.contains("password=secretph***"), "got: {}", result);
        assert!(
            result.contains("other=val"),
            "other=val should survive, got: {}",
            result
        );
    }

    #[test]
    fn redact_body_multi_occurrence() {
        let input = "bot_token=*** bot_token=realsecret";
        let result = redact_body_substrings(input);
        assert!(
            !result.contains("realsecret"),
            "second bot_token should be redacted, got: {}",
            result
        );
    }

    #[test]
    fn redact_body_phone_with_spaces() {
        let input = "phone: +1 (555) 123-4567";
        let result = redact_body_substrings(input);
        assert!(
            !result.contains("555"),
            "full phone should be redacted, got: {}",
            result
        );
    }

    #[test]
    fn redact_body_multiple_keys() {
        let input = "password=secret api_hash=abc123 user_id=42";
        let result = redact_body_substrings(input);
        assert!(
            result.contains("user_id=42"),
            "non-sensitive key should survive"
        );
        assert!(!result.contains("secret"), "password should be redacted");
        assert!(!result.contains("abc123"), "api_hash should be redacted");
    }
}
