//! `tracing`-based logging setup for the MTProto Telegram
//! onboard CLI.
//!
//! Mirrors the shape of the TDLib `octo-telegram-onboard`
//! crate's `logging` module so operators get a familiar
//! experience: `RUST_LOG`-controlled env-filter with a
//! sensible default of `info,octo_telegram_mtproto_onboard=debug`.
//!
//! ## Secret redaction (R26-OPS-1)
//!
//! The default `tracing_subscriber::fmt` layer would emit
//! any field whose value contains a secret verbatim
//! (`bot_token`, `api_hash`, `password`, `phone`, the
//! session key bytes). A malformed log call like
//! `tracing::info!(bot_token = %token, "...")` would
//! leak the token to stderr.
//!
//! The redaction layer in this module intercepts event
//! rendering (via a custom `FormatEvent`) and replaces
//! the value with `***` for any field whose name is in
//! [`REDACTED_FIELD_NAMES`]. It also walks the rendered
//! line for `key=value` and `key: value` patterns so a
//! `tracing::info!("bot_token=... ...")` message body
//! (without a structured field) is also scrubbed.

use std::fmt as stdfmt;

use tracing::field::{Field, Visit};
use tracing::Event;
use tracing_subscriber::fmt as tsfmt;
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::fmt::FmtContext;
use tracing_subscriber::{prelude::*, EnvFilter};

/// Field names whose values must be redacted. Match the
/// convention used by `octo-telegram-onboard` and
/// `octo-telegram-mtproto-onboard-core` so an operator
/// doesn't have to learn two sets.
///
/// R2-ARCH-12: the previous list omitted `code`,
/// `session_path`, and `auth_string`. The first two are
/// field names that appear in our `tracing::info!` /
/// `tracing::error!` calls (e.g. an operator-visible log
/// line like `code=*** still pending`); `auth_string` is
/// the canonical name for a one-time auth token in the
/// QR login flow. Without these in the redaction list, a
/// log line containing `code=12345` (an SMS code) or
/// `auth_string=ABCDEF` (a QR token) would render in
/// cleartext. The fix adds them to the canonical list
/// alongside `bot_token`, `api_hash`, etc.
pub const REDACTED_FIELD_NAMES: &[&str] = &[
    "bot_token",
    "api_hash",
    "password",
    "phone",
    "session_key",
    "auth_key",
    "token",
    "secret",
    "code",
    "session_path",
    "auth_string",
];

fn is_sensitive_key(name: &str) -> bool {
    // Case-insensitive match so `Bot_Token`, `bot_token`,
    // `BOT_TOKEN` all get redacted. (The convention is
    // snake_case so the common case is the exact match,
    // but the case-fold catches accidental title-casing.)
    let lower = name.to_ascii_lowercase();
    REDACTED_FIELD_NAMES.iter().any(|s| *s == lower)
}

/// Custom `Visit` that captures field values but
/// substitutes `***` for sensitive field names.
struct RedactingVisitor<'a> {
    buf: &'a mut String,
    first: bool,
}

impl<'a> Visit for RedactingVisitor<'a> {
    fn record_debug(&mut self, field: &Field, value: &dyn stdfmt::Debug) {
        if self.first {
            self.buf.push_str("  ");
            self.first = false;
        } else {
            self.buf.push(' ');
        }
        if is_sensitive_key(field.name()) {
            self.buf.push_str(&format!("{}=***", field.name()));
        } else {
            self.buf.push_str(&format!("{}={:?}", field.name(), value));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if self.first {
            self.buf.push_str("  ");
            self.first = false;
        } else {
            self.buf.push(' ');
        }
        if is_sensitive_key(field.name()) {
            self.buf.push_str(&format!("{}=***", field.name()));
        } else {
            self.buf.push_str(&format!("{}={}", field.name(), value));
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.record_debug(field, &value);
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.record_debug(field, &value);
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.record_debug(field, &value);
    }
}

/// Render an event by walking its fields through
/// [`RedactingVisitor`], then post-process the line for
/// `key=value` patterns in the body.
struct RedactingFormat;

impl<S, N> FormatEvent<S, N> for RedactingFormat
where
    S: tracing::Subscriber + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
    N: for<'w> FormatFields<'w> + 'static,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> stdfmt::Result {
        let mut buf = String::new();
        {
            let mut visitor = RedactingVisitor {
                buf: &mut buf,
                first: true,
            };
            event.record(&mut visitor);
        }
        // Post-process the rendered line for `key=value`
        // and `key: value` patterns in the body (a log
        // call like `tracing::info!("bot_token=foo ...")`).
        let redacted = redact_body_substrings(&buf);
        write!(writer, "{}", redacted)?;
        writeln!(writer)
    }
}

/// Walk `body` for `key=value` and `key: value` patterns
/// and replace the value with `***` for any key in
/// [`REDACTED_FIELD_NAMES`]. Cheap heuristic; not a
/// full parser.
fn redact_body_substrings(body: &str) -> String {
    let mut result = body.to_string();
    for &key in REDACTED_FIELD_NAMES {
        // Case-insensitive substring search.
        let mut start_from: usize = 0;
        loop {
            let lower = result.to_ascii_lowercase();
            let Some(pos) = lower[start_from..].find(key) else {
                break;
            };
            let abs_pos = start_from + pos;
            let key_end = abs_pos + key.len();
            // Need a word boundary + separator (`=` or
            // `:`) after the key to consider this a
            // redaction target. Avoids false positives
            // like `auth_key_padding`.
            if key_end >= result.len() || !matches!(result.as_bytes()[key_end], b'=' | b':') {
                start_from = key_end;
                continue;
            }
            // Skip the separator.
            let mut val_start = key_end + 1;
            // For "key: value" (colon followed by space),
            // skip the space too.
            if key_end < result.len()
                && result.as_bytes()[key_end] == b':'
                && val_start < result.len()
                && matches!(result.as_bytes()[val_start], b' ' | b'\t')
            {
                val_start += 1;
            }
            // Find value end: next whitespace or end of
            // string.
            let val_end = result[val_start..]
                .find(|c: char| c.is_whitespace() || c == '\0')
                .map(|p| val_start + p)
                .unwrap_or(result.len());
            if val_end > val_start {
                let replacement = "***";
                result = format!(
                    "{}{}{}",
                    &result[..val_start],
                    replacement,
                    &result[val_end..]
                );
                // Advance past the replacement.
                start_from = val_start + replacement.len();
            } else {
                start_from = val_end;
            }
        }
    }
    result
}

/// Initialise the global tracing subscriber. Idempotent — a
/// no-op on subsequent calls. Returns `true` if the subscriber
/// was installed by this call, `false` if one was already
/// present.
pub fn init(verbose: u8) -> bool {
    let default = match verbose {
        0 => "info,octo_telegram_mtproto_onboard=debug",
        1 => "debug",
        _ => "trace",
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));
    let layer = tsfmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(false)
        .with_file(false)
        .event_format(RedactingFormat);
    // `try_init` returns Err if a subscriber is already set;
    // we treat that as a no-op success.
    tracing_subscriber::registry()
        .with(filter)
        .with(layer)
        .try_init()
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn init_is_idempotent() {
        // First call may or may not succeed depending on
        // whether a previous test in the same process set
        // a subscriber. Both outcomes are acceptable; the
        // important property is that the *second* call
        // does not panic.
        let _ = init(0);
        let _ = init(1);
    }

    #[test]
    fn is_sensitive_key_matches_snake_case() {
        assert!(is_sensitive_key("bot_token"));
        assert!(is_sensitive_key("api_hash"));
        assert!(is_sensitive_key("password"));
        assert!(is_sensitive_key("phone"));
        assert!(is_sensitive_key("auth_key"));
    }

    /// R2-ARCH-12: the redaction list now also covers
    /// `code`, `session_path`, and `auth_string`. A log
    /// line like `code=12345 auth_string=ABCDEF` would
    /// otherwise render in cleartext.
    #[test]
    fn is_sensitive_key_covers_r2_arch_12_additions() {
        assert!(is_sensitive_key("code"));
        assert!(is_sensitive_key("session_path"));
        assert!(is_sensitive_key("auth_string"));
        // Case-insensitive.
        assert!(is_sensitive_key("CODE"));
        assert!(is_sensitive_key("Auth_String"));
    }

    #[test]
    fn is_sensitive_key_is_case_insensitive() {
        assert!(is_sensitive_key("Bot_Token"));
        assert!(is_sensitive_key("API_HASH"));
        assert!(is_sensitive_key("Password"));
    }

    #[test]
    fn is_sensitive_key_rejects_unrelated() {
        assert!(!is_sensitive_key("user_id"));
        assert!(!is_sensitive_key("data_dir"));
        assert!(!is_sensitive_key("mode"));
        assert!(!is_sensitive_key("elapsed_ms"));
    }

    #[test]
    fn redact_body_substrings_replaces_key_value() {
        let input = "bot_token=123456789:AAAA api_id=42";
        let out = redact_body_substrings(input);
        assert!(!out.contains("123456789:AAAA"), "out = {}", out);
        assert!(out.contains("bot_token=***"), "out = {}", out);
        // Non-sensitive keys pass through.
        assert!(out.contains("api_id=42"), "out = {}", out);
    }

    #[test]
    fn redact_body_substrings_replaces_key_colon_value() {
        let input = "api_hash: 0123456789abcdef data_dir=/tmp";
        let out = redact_body_substrings(input);
        assert!(!out.contains("0123456789abcdef"), "out = {}", out);
        assert!(
            out.contains("api_hash=***") || out.contains("api_hash: ***"),
            "out = {}",
            out
        );
    }

    /// R26-OPS-1 (TV-11/TV-12): capture tracing output
    /// for a real `tracing::info!` event that includes a
    /// secret-shaped field, and verify the secret bytes
    /// are NOT in the captured output. This is the
    /// integration test for the redaction layer.
    ///
    /// We don't go through `init()` (that installs a
    /// global subscriber and breaks under cargo test's
    /// parallel runner). Instead we install a
    /// thread-local subscriber with a `Vec<u8>` writer
    /// and capture the formatted output.
    #[test]
    fn redacting_format_strips_secret_field_values() {
        use std::sync::{Arc, Mutex};
        use tracing_subscriber::fmt::MakeWriter;

        // A writer that captures everything written to it
        // into a `Vec<u8>` we can inspect after the
        // event.
        #[derive(Clone, Default)]
        struct CaptureWriter(Arc<Mutex<Vec<u8>>>);
        impl Write for CaptureWriter {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl<'a> MakeWriter<'a> for CaptureWriter {
            type Writer = CaptureWriter;
            fn make_writer(&'a self) -> Self::Writer {
                self.clone()
            }
        }

        let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
        let writer = CaptureWriter(buf.clone());
        let layer = tsfmt::layer()
            .with_writer(writer)
            .event_format(RedactingFormat);
        let subscriber = tracing_subscriber::registry().with(layer);

        // TV-11: log an event with a bot-token-shaped
        // value. The literal "123456789:AAAA..." should
        // NOT appear in the captured output.
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(
                bot_token = "123456789:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                "test event"
            );
        });
        let captured = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        assert!(
            !captured.contains("123456789:AAAA"),
            "secret bytes leaked: {}",
            captured
        );
        assert!(
            captured.contains("bot_token=***"),
            "redaction marker missing: {}",
            captured
        );

        // Reset and test TV-12: log an event with an
        // api_hash-shaped value. The hex string should
        // NOT appear in the captured output.
        buf.lock().unwrap().clear();
        let writer2 = CaptureWriter(buf.clone());
        let layer2 = tsfmt::layer()
            .with_writer(writer2)
            .event_format(RedactingFormat);
        let subscriber2 = tracing_subscriber::registry().with(layer2);
        tracing::subscriber::with_default(subscriber2, || {
            tracing::info!(api_hash = "0123456789abcdef0123456789abcdef", "test event");
        });
        let captured2 = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        assert!(
            !captured2.contains("0123456789abcdef"),
            "secret bytes leaked: {}",
            captured2
        );
        assert!(
            captured2.contains("api_hash=***"),
            "redaction marker missing: {}",
            captured2
        );
    }

    /// R26-OPS-1 (TV-12 extended): a log call that
    /// embeds the secret in the *message body* (not as
    /// a structured field) is also scrubbed. This is
    /// the common bug where a developer writes
    /// `tracing::info!("got bot_token=... from user")`
    /// instead of using a structured field.
    #[test]
    fn redacting_format_strips_secret_in_message_body() {
        use std::sync::{Arc, Mutex};
        use tracing_subscriber::fmt::MakeWriter;

        #[derive(Clone, Default)]
        struct CaptureWriter(Arc<Mutex<Vec<u8>>>);
        impl Write for CaptureWriter {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl<'a> MakeWriter<'a> for CaptureWriter {
            type Writer = CaptureWriter;
            fn make_writer(&'a self) -> Self::Writer {
                self.clone()
            }
        }

        let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
        let writer = CaptureWriter(buf.clone());
        let layer = tsfmt::layer()
            .with_writer(writer)
            .event_format(RedactingFormat);
        let subscriber = tracing_subscriber::registry().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("got api_hash=0123456789abcdef0123456789abcdef from user");
        });
        let captured = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        assert!(
            !captured.contains("0123456789abcdef"),
            "secret bytes leaked: {}",
            captured
        );
        assert!(
            captured.contains("api_hash=***"),
            "redaction marker missing: {}",
            captured
        );
    }
}
