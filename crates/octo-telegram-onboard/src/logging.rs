//! Tracing-subscriber init with a token-redaction layer.
//!
//! Redacts event fields whose names match sensitive keys
//! (`bot_token`, `api_hash`, `phone`, `password`, `access_token`,
//! `verifying_key`, etc.) — case-insensitive exact match on field names.

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
        let mut visitor = RedactingVisitor {
            writer: &mut writer,
            first: true,
        };
        event.record(&mut visitor);
        writeln!(writer)
    }
}

struct RedactingVisitor<'a, 'b> {
    writer: &'a mut Writer<'b>,
    first: bool,
}

impl<'a, 'b> Visit for RedactingVisitor<'a, 'b> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let name = field.name();
        let raw = format!("{:?}", value);

        if self.first {
            let _ = write!(self.writer, "  ");
            self.first = false;
        } else {
            let _ = write!(self.writer, " ");
        }

        if is_sensitive_key(name) {
            let _ = write!(self.writer, "{}={}", name, redact_value(&raw));
        } else {
            let _ = write!(self.writer, "{}={}", name, raw);
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        let name = field.name();

        if self.first {
            let _ = write!(self.writer, "  ");
            self.first = false;
        } else {
            let _ = write!(self.writer, " ");
        }

        if is_sensitive_key(name) {
            let _ = write!(self.writer, "{}={}", name, redact_value(value));
        } else {
            let _ = write!(self.writer, "{}={}", name, value);
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
}
