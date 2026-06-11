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
/// For each REDACT_KEY found as a substring, redacts the value that follows
/// (the next token after `=` or `:`). Only matches keys that are immediately
/// followed by a separator (=, :, whitespace) to avoid false positives on
/// already-redacted values or key substrings.
fn redact_body_substrings(body: &str) -> String {
    let mut result = body.to_string();
    for &key in REDACT_KEYS {
        loop {
            let lower = result.to_ascii_lowercase();
            let Some(pos) = lower.find(key) else {
                break;
            };
            let key_end = pos + key.len();
            // Only match if followed by a separator (=, :, or whitespace)
            if key_end >= result.len()
                || !matches!(result.as_bytes()[key_end], b'=' | b':' | b' ' | b'\t')
            {
                break;
            }
            // Skip separator to find value start
            let mut val_start = key_end;
            while val_start < result.len()
                && matches!(result.as_bytes()[val_start], b'=' | b':' | b' ' | b'\t')
            {
                val_start += 1;
            }
            // Find the end of the value (next whitespace or end of string)
            let mut val_end = val_start;
            while val_end < result.len()
                && !matches!(result.as_bytes()[val_end], b' ' | b'\t' | b'\n' | b',')
            {
                val_end += 1;
            }
            if val_end > val_start {
                let original_val = &result[val_start..val_end].to_string();
                let replacement = redact_value(original_val);
                if replacement == *original_val {
                    break;
                }
                result = format!(
                    "{}{}{}",
                    &result[..val_start],
                    replacement,
                    &result[val_end..]
                );
            } else {
                break;
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
}
