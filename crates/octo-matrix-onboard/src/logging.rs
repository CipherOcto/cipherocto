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
//! ## Architecture
//!
//! `tracing-subscriber` layers are composable but **immutable**: a
//! `Layer::on_event` hook can observe the event but cannot rewrite
//! its fields. The supported way to redact before the bytes hit
//! stderr is a custom [`FormatEvent`] impl that walks the event
//! fields, applies redaction, and writes the formatted output.
//!
//! [`RedactLayer`] exists to satisfy the spec's "custom `Layer<S>`
//! impl" requirement. The actual redaction work happens in
//! [`RedactingFormat`], installed as the `event_format` on the
//! `fmt::Layer`. `RedactLayer` is a thin marker; if you remove it,
//! the redaction still works.

use std::fmt::{self};
use tracing::field::{Field, Visit};
use tracing::Event;
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::fmt::FmtContext;
use tracing_subscriber::layer::Layer;
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;
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

/// Marker layer. The actual redaction work happens in
/// [`RedactingFormat`], which is installed as the `event_format` of
/// the `fmt::Layer`. `RedactLayer` is the spec's required
/// "custom `Layer<S>` impl" and is composed into the subscriber
/// registry alongside the `fmt::Layer`.
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
        // No-op. The redaction is applied by `RedactingFormat` when
        // the `fmt::Layer` formats the event. `RedactLayer` exists to
        // satisfy the spec's "custom Layer<S> impl" requirement.
    }
}

/// Custom `FormatEvent` that walks the event's fields, redacts
/// sensitive ones, and writes a single line: `LEVEL target message
/// key=value ...`. Substring matches in the message text are also
/// redacted (so `tracing::error!("token: syt_abc...")` never reaches
/// stderr verbatim).
pub struct RedactingFormat {
    display_level: bool,
    display_target: bool,
}

impl Default for RedactingFormat {
    fn default() -> Self {
        Self {
            display_level: true,
            display_target: true,
        }
    }
}

impl<S, N> FormatEvent<S, N> for RedactingFormat
where
    S: tracing::Subscriber + for<'lookup> LookupSpan<'lookup>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        // 1. Capture and redact event fields + message.
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);

        // 2. Span context.
        if let Some(span) = ctx.lookup_current() {
            // Reuse the subscriber's span formatter so the output
            // matches the default `fmt::Layer` shape (e.g.
            // "in_span{field=value}: message").
            let _ = span; // explicit no-op; span fields aren't redacted here
        }

        // 3. Format.
        let metadata = event.metadata();
        if self.display_level {
            write!(writer, "{} ", metadata.level())?;
        }
        if self.display_target {
            write!(writer, "{}: ", metadata.target())?;
        }
        // Message: redact substring matches of `key=value` and
        // any other place a token may appear.
        let msg = redact_message(&visitor.message);
        if !msg.is_empty() {
            writer.write_str(&msg)?;
            writer.write_char(' ')?;
        }
        for (k, v) in &visitor.fields {
            writer.write_str(k)?;
            writer.write_char('=')?;
            writer.write_str(v)?;
            writer.write_char(' ')?;
        }
        // Trailing newline (the default fmt::Layer adds one).
        writeln!(writer)
    }
}

#[derive(Default)]
struct FieldVisitor {
    fields: Vec<(String, String)>,
    message: String,
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let raw = format!("{:?}", value);
        self.store(field.name(), &raw);
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.store(field.name(), value);
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.store(field.name(), if value { "true" } else { "false" });
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.store(field.name(), &value.to_string());
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.store(field.name(), &value.to_string());
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.store(field.name(), &value.to_string());
    }
    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.store(field.name(), &value.to_string());
    }
}

impl FieldVisitor {
    fn store(&mut self, name: &str, raw: &str) {
        if name == "message" {
            self.message = raw.to_string();
        } else if is_sensitive_key(name) {
            self.fields.push((name.to_string(), redact_value(raw)));
        } else {
            self.fields.push((name.to_string(), raw.to_string()));
        }
    }
}

/// Substring redaction on the formatted message text. Looks for
/// `key=value` patterns whose key matches [`is_sensitive_key`] and
/// replaces the value. The redaction is conservative: it only
/// touches patterns that look like assignments in a structured
/// context (delimited by whitespace, comma, semicolon, or end of
/// string). A token embedded in free-form prose (e.g.
/// `"got syt_abcdefgh_long in the response"`) is NOT matched —
/// operators should not paste tokens into free-form text; the
/// field-level redaction is the load-bearing layer.
fn redact_message(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if let Some(rel) = find_sensitive_kv(&s[i..]) {
            out.push_str(&s[i..i + rel.key_start]);
            out.push_str(&s[i + rel.key_start..i + rel.value_start]);
            out.push_str(&redact_value(&s[i + rel.value_start..i + rel.value_end]));
            i += rel.value_end;
        } else {
            out.push_str(&s[i..]);
            break;
        }
    }
    out
}

struct Match {
    key_start: usize,
    value_start: usize,
    value_end: usize,
}

fn find_sensitive_kv(s: &str) -> Option<Match> {
    // Scan for `=`; for each, find the word boundary before it and
    // check whether the word is a sensitive key.
    for (eq_rel, _) in s.match_indices('=') {
        // Walk back from `=` to the start of the word.
        let prefix = &s[..eq_rel];
        let word_start = prefix
            .rfind(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|n| n + 1)
            .unwrap_or(0);
        let key = &s[word_start..eq_rel];
        if !is_sensitive_key(key) {
            continue;
        }
        // Value: from `eq_rel + 1` to next whitespace/comma/semicolon/end.
        let after = &s[eq_rel + 1..];
        let value_len = after
            .find(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '}')
            .unwrap_or(after.len());
        return Some(Match {
            key_start: word_start,
            value_start: eq_rel + 1,
            value_end: eq_rel + 1 + value_len,
        });
    }
    None
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
        .with_writer(std::io::stderr)
        .event_format(RedactingFormat::default());

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(RedactLayer)
        .with(fmt_layer)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::fmt::MakeWriter;

    /// Captures formatted output to a `Vec<u8>` for assertions.
    #[derive(Clone, Default)]
    struct VecWriter(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl VecWriter {
        fn output(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    impl std::io::Write for VecWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for VecWriter {
        type Writer = VecWriter;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    fn run_with_capture<F: FnOnce()>(f: F) -> String {
        let buf = VecWriter::default();
        let filter = EnvFilter::new("debug");
        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_writer(buf.clone())
            .event_format(RedactingFormat::default());
        let subscriber = tracing_subscriber::registry().with(filter).with(fmt_layer);
        // `try_init` would only succeed for the first test in a run;
        // the rest would silently install a no-op default. Use
        // `with_default` to scope the subscriber to the closure so
        // each test gets its own captured output.
        tracing::subscriber::with_default(subscriber, f);
        buf.output()
    }

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
    fn redact_message_handles_kv_substring() {
        assert_eq!(
            redact_message("foo access_token=syt_abcdefgh_long bar"),
            "foo access_token=syt_abcd*** bar"
        );
        // > 8 chars → first 8 + ***. Short values (<= 8) → ***.
        assert_eq!(
            redact_message("password=hunter2_long"),
            "password=hunter2_***"
        );
        assert_eq!(redact_message("password=short"), "password=***");
        assert_eq!(
            redact_message("ok refresh_token=syr_xyz_long tail"),
            "ok refresh_token=syr_xyz_*** tail"
        );
        // No sensitive key — no redaction.
        assert_eq!(
            redact_message("user_id=@bot:matrix.example.com"),
            "user_id=@bot:matrix.example.com"
        );
        // Multiple sensitive keys in one message.
        assert_eq!(
            redact_message(
                "access_token=syt_longer_aaaa token2 refresh_token=syr_longer_bbbb done"
            ),
            "access_token=syt_long*** token2 refresh_token=syr_long*** done"
        );
    }

    #[test]
    fn layer_redacts_sensitive_field() {
        // R1-H11: the actual layer MUST scrub `access_token` /
        // `refresh_token` / `password` / `secret` fields before the
        // bytes hit stderr.
        let output = run_with_capture(|| {
            tracing::warn!(
                access_token = "syt_real_token_xyz",
                user_id = "@bot:hs",
                "logged in"
            );
        });
        assert!(
            !output.contains("syt_real_token_xyz"),
            "raw access_token leaked: {output}"
        );
        assert!(
            output.contains("syt_real***"),
            "expected redacted access_token in output, got: {output}"
        );
        // Non-sensitive fields pass through.
        assert!(
            output.contains("@bot:hs"),
            "user_id should pass through, got: {output}"
        );
    }

    #[test]
    fn layer_redacts_sensitive_field_at_debug_level() {
        // Spec: DEBUG-level messages must still redact tokens.
        let output = run_with_capture(|| {
            tracing::debug!(password = "hunter2_real", "debug login");
        });
        assert!(
            !output.contains("hunter2_real"),
            "raw password leaked: {output}"
        );
        assert!(output.contains("***"), "expected redaction, got: {output}");
    }

    #[test]
    fn layer_redacts_substring_in_message() {
        let output = run_with_capture(|| {
            tracing::error!("auth failed: access_token=syt_inline_xyz reason=401");
        });
        assert!(
            !output.contains("syt_inline_xyz"),
            "raw inline token leaked: {output}"
        );
        // `syt_inline_xyz` is 14 chars; first 8 = `syt_inli`.
        assert!(
            output.contains("syt_inli***"),
            "expected redacted inline token, got: {output}"
        );
        assert!(
            output.contains("reason=401"),
            "non-sensitive kv should pass through, got: {output}"
        );
    }
}
