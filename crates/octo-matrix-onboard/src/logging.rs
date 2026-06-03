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
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::field::{Field, Visit};
use tracing::Event;
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::fmt::FmtContext;
use tracing_subscriber::layer::Layer;
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::EnvFilter;

const REDACT_KEYS: &[&str] = &[
    "access_token",
    "refresh_token",
    "password",
    "secret",
    "recovery_key",
];

/// R2-L7: format the current wall-clock time as an RFC 3339-ish
/// string (`YYYY-MM-DDThh:mm:ss.nnnnnnnnnZ`). The stdlib doesn't
/// expose a `strftime`-style formatter, so we hand-roll it from
/// `SystemTime` + `Duration`. Pulling in `chrono` or `time` just
/// for a log prefix would be heavyweight. The output is always UTC
/// and always 30 characters wide; anything that would make the
/// output malformed (e.g. clock skew giving a negative duration)
/// falls back to `<unknown-ts>` so a logging bug never breaks
/// startup.
fn format_rfc3339_now() -> String {
    let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d,
        Err(_) => return "<unknown-ts>".to_string(),
    };
    let secs = now.as_secs();
    let nanos = now.subsec_nanos();
    // Days since epoch → year/month/day using the proleptic
    // Gregorian calendar (1970-01-01 = day 0). This is sufficient
    // for log timestamps; we don't need a full date library.
    let (year, month, day) = epoch_days_to_ymd((secs / 86_400) as i64);
    let hh = (secs / 3600) % 24;
    let mm = (secs / 60) % 60;
    let ss = secs % 60;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:09}Z",
        year, month, day, hh, mm, ss, nanos
    )
}

/// R3-L1: format a Unix epoch (seconds since 1970-01-01) as an RFC
/// 3339 UTC string with no sub-second precision: `YYYY-MM-DDTHH:MM:SSZ`.
/// Used by `session list` to render the `LAST_USED` column in a
/// format `date -d` and other RFC 3339 parsers can consume. Negative
/// epochs (pre-1970) and zero are rendered as `<unknown>` so the
/// column doesn't carry a misleading 1969-12-31 timestamp for rows
/// where the field was never written.
pub(crate) fn format_rfc3339_secs(epoch_secs: i64) -> String {
    if epoch_secs <= 0 {
        return "<unknown>".to_string();
    }
    let secs = epoch_secs as u64;
    let (year, month, day) = epoch_days_to_ymd((secs / 86_400) as i64);
    let hh = (secs / 3600) % 24;
    let mm = (secs / 60) % 60;
    let ss = secs % 60;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hh, mm, ss
    )
}

/// R2-L7: convert a day count since 1970-01-01 to (year, month, day)
/// in the proleptic Gregorian calendar. Civil-from-days algorithm
/// from Howard Hinnant's `date` library (public domain).
fn epoch_days_to_ymd(days_since_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y } as i32;
    (year, m as u32, d as u32)
}

fn is_sensitive_key(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    REDACT_KEYS.iter().any(|k| lower.contains(k))
}

/// Redact a string for safe display in tracing output. R2-L7: the
/// shape is "first 8 bytes (walked back to a UTF-8 char boundary) +
/// ***" for strings longer than 8 bytes, and "***" for short
/// strings. R2-H2 follow-on: byte slicing is replaced with
/// char-boundary walking so a multi-byte UTF-8 boundary in the 8th
/// byte can't panic.
///
/// R6-L2: this is one of FOUR `redact_*` implementations across
/// the four mission crates. Each site has a deliberately different
/// format policy because each display context calls for a
/// different balance of brevity and operator-recognizability:
///
/// - `crates/octo-matrix-onboard/src/logging.rs` (THIS FUNCTION) —
///   tracing-subscriber `FormatEvent` redaction. Walks the 8th
///   byte back to the nearest char boundary so non-ASCII input
///   (e.g. a 4S recovery key with Unicode chars) can't panic.
///   Shape: "first ≤8 bytes + ***" / "***". This is the
///   only site that walks back — the other three assume ASCII
///   (Matrix tokens) or use char-based slicing (the adapter).
/// - `crates/octo-adapter-matrix-sdk/src/lib.rs:80` — free-form
///   diagnostic output (error messages, debug logs). Char-based
///   slicing so a non-ASCII token gets the first 8 / last 4 CHARS.
///   3-tier shape: `first8...last4` / `all***` / `***`.
/// - `crates/octo-matrix-onboard-core/src/lib.rs:169` — the
///   one-time "logged in" confirmation message
///   (`Session::access_token_preview`). 2-tier shape:
///   `first8...last4` / `first4...`.
/// - `crates/octo-matrix-onboard/src/modes/session.rs:77` —
///   tabular `session list` output. R6-M2 fixed the byte-slicing
///   (R2-H2 missed this site) so the slice is now char-boundary
///   safe. Shape: `first ≤8 bytes + ***` / `***`.
///
/// R5-L1 named "three" sites; R6-L2 added this one. The four-way
/// divergence is deliberate: the formats serve different display
/// contexts. The cross-reference is the only thing tying them
/// together.
fn redact_value(v: &str) -> String {
    if v.len() > 8 {
        // R2-H2 follow-on: `&v[..8]` slices by BYTES, which would
        // panic if byte 8 falls inside a multi-byte UTF-8 codepoint.
        // Walk back from byte 8 to the nearest char boundary so the
        // slice is safe even on non-ASCII input. The result is at
        // most 8 bytes long (always fewer than the original).
        let mut end = 8.min(v.len());
        while end > 0 && !v.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}***", &v[..end])
    } else {
        "***".to_string()
    }
}

/// Marker layer. The actual redaction work happens in
/// [`RedactingFormat`], which is installed as the `event_format` of
/// the `fmt::Layer`. `RedactLayer` is the spec's required
/// "custom `Layer<S>` impl" and is composed into the subscriber
/// registry alongside the `fmt::Layer`.
///
/// R2-H3: `on_new_span` and `on_record` capture span fields into
/// the span's extension storage. `RedactingFormat` reads them back
/// and applies the same redaction as event fields. Without this,
/// tokens placed in span fields (e.g.,
/// `tracing::info_span!("auth", access_token = %tok)`) bypass the
/// redaction layer and reach stderr unredacted.
pub struct RedactLayer;

impl<S> Layer<S> for RedactLayer
where
    S: tracing::Subscriber + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut visitor = FieldVisitor::default();
        attrs.record(&mut visitor);
        // `FieldVisitor::store` treats `name == "message"` as the
        // event body and routes it to `self.message` instead of
        // `self.fields`. Spans don't have a message body, so the
        // only way a span could lose a field is if a span happened
        // to use the field name "message" (uncommon; if it does,
        // the field goes to the extension's `self.message`, which
        // is dropped when we extract just the `fields` vec).
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(SpanFields {
                fields: visitor.fields,
            });
        }
    }

    fn on_record(
        &self,
        span_id: &tracing::span::Id,
        values: &tracing::span::Record<'_>,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        // Late-recorded fields (via `Span::record("k", &v)`) need
        // to be merged into the extension. The SDK uses this for
        // refresh-token rotation; if a span was created with
        // `access_token = ""` and later updated via `record`, we
        // want the redacted final value, not the empty initial one.
        let mut visitor = FieldVisitor::default();
        values.record(&mut visitor);
        if let Some(span) = ctx.span(span_id) {
            let mut ext = span.extensions_mut();
            if let Some(existing) = ext.get_mut::<SpanFields>() {
                for (k, v) in visitor.fields {
                    if let Some(slot) = existing.fields.iter_mut().find(|(name, _)| name == &k) {
                        *slot = (k, v);
                    } else {
                        existing.fields.push((k, v));
                    }
                }
            } else {
                ext.insert(SpanFields {
                    fields: visitor.fields,
                });
            }
        }
    }

    fn on_event(
        &self,
        _event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        // The event-field redaction is applied by `RedactingFormat`
        // when the `fmt::Layer` formats the event. The span-field
        // redaction is enabled by `on_new_span` / `on_record` above.
    }
}

/// Per-span field storage attached to the span's extensions. The
/// redaction is applied at capture time, so `RedactingFormat` can
/// just read the redacted form.
#[derive(Default)]
struct SpanFields {
    fields: Vec<(String, String)>,
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

        // 2. R2-H3: walk the span chain and pull each span's
        // redacted fields from its extension (populated by
        // `RedactLayer::on_new_span` / `on_record`). Without this,
        // tokens placed in span fields reach stderr unredacted.
        let mut span_segments: Vec<String> = Vec::new();
        if let Some(scope) = ctx.event_scope() {
            for span in scope.from_root() {
                let name = span.name();
                let ext = span.extensions();
                if let Some(stored) = ext.get::<SpanFields>() {
                    if !stored.fields.is_empty() {
                        let rendered = stored
                            .fields
                            .iter()
                            .map(|(k, v)| format!("{k}={v}"))
                            .collect::<Vec<_>>()
                            .join(" ");
                        span_segments.push(format!("{name}{{{rendered}}}"));
                    } else {
                        span_segments.push(name.to_string());
                    }
                } else {
                    span_segments.push(name.to_string());
                }
            }
        }
        let span_prefix = if span_segments.is_empty() {
            String::new()
        } else {
            format!("{}: ", span_segments.join(": "))
        };

        // 3. R2-L7: render an ISO-style timestamp at the head of
        //    the line, mirroring what `tracing_subscriber::fmt`'s
        //    default `FormatEvent` does. The previous version
        //    omitted the timestamp entirely, so operators tailing
        //    logs lost time correlation across lines. The format
        //    is `YYYY-MM-DDThh:mm:ss.nsZ` (RFC 3339 with
        //    nanosecond precision and explicit UTC), derived
        //    without pulling in `chrono` / `time` (this crate
        //    deliberately has no time-formatting dep).
        let timestamp = format_rfc3339_now();
        write!(writer, "{timestamp} ")?;

        // 4. Format.
        let metadata = event.metadata();
        if self.display_level {
            write!(writer, "{} ", metadata.level())?;
        }
        if self.display_target {
            write!(writer, "{}: ", metadata.target())?;
        }
        if !span_prefix.is_empty() {
            writer.write_str(&span_prefix)?;
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
            // R2-M8: quote values that contain whitespace, `=`, or
            // `"` so the rendered line is unambiguous (a value
            // containing a space would otherwise look like a
            // separator between two adjacent fields). The quoting
            // is intentionally simple — escape `"` as `\"` and
            // wrap in double quotes when needed. We don't worry
            // about newlines or other special chars because the
            // `fmt::Layer` would already have replaced them.
            if v.contains(|c: char| c.is_whitespace() || c == '=' || c == '"') {
                writer.write_char('"')?;
                for c in v.chars() {
                    if c == '"' {
                        writer.write_str("\\\"")?;
                    } else {
                        writer.write_char(c)?;
                    }
                }
                writer.write_char('"')?;
            } else {
                writer.write_str(v)?;
            }
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
        // `find_sensitive_kv` returns byte positions that are all
        // guaranteed char boundaries (see its doc); `i` is also a
        // char boundary (initialized to 0, then advanced by
        // `value_end` which is a char boundary).
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

/// Scan `s` for a sensitive `key=value` pattern and return the byte
/// positions of the key/value, or None if no match.
///
/// R2-H2: the previous implementation used
/// `prefix.rfind(|c: char| ...).map(|n| n + 1)` to walk back from
/// `=` to the start of the word. `rfind` returns the byte index of
/// the start of the matching char, so `n + 1` is the byte after
/// the FIRST byte of that char. If the matching char is
/// multi-byte UTF-8 (e.g. the comma `，` is 3 bytes), `n + 1` is
/// in the middle of that char's bytes and the subsequent
/// `&s[word_start..eq_rel]` slice panics with "byte index N is
/// not a char boundary".
///
/// The fix is to walk back by `char_indices().rev()` and take the
/// first byte position of the leftmost trailing alphanumeric/
/// underscore char; that position is, by construction, a char
/// boundary. The returned `Match` positions are all char
/// boundaries, so the slicing in `redact_message` is safe even on
/// non-ASCII input.
fn find_sensitive_kv(s: &str) -> Option<Match> {
    for (eq_rel, _) in s.match_indices('=') {
        let prefix = &s[..eq_rel];
        // Walk back from the end of `prefix` over the trailing run
        // of alphanumeric-or-underscore chars. The leftmost such
        // char's byte position is `word_start` (a char boundary).
        let word_start = prefix
            .char_indices()
            .rev()
            .take_while(|(_, c)| c.is_alphanumeric() || *c == '_')
            .last()
            .map(|(n, _)| n)
            .unwrap_or(0);
        let key = &s[word_start..eq_rel];
        if !is_sensitive_key(key) {
            continue;
        }
        let after = &s[eq_rel + 1..];
        // Value: from `eq_rel + 1` to next whitespace/comma/semicolon/
        // brace/end. The found byte position is the start of the
        // next char (a char boundary by construction).
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

    /// R3-L1: smoke tests for the RFC 3339 helper. The full
    /// epoch_days_to_ymd table is exercised indirectly via
    /// `format_rfc3339_now`; here we only need to confirm the
    /// no-fractional-seconds shape and the `<unknown>` fallback.
    #[test]
    fn format_rfc3339_secs_renders_a_known_epoch() {
        // 2026-01-01T00:00:00Z = 1767225600 seconds since 1970-01-01.
        let rendered = format_rfc3339_secs(1_767_225_600);
        assert_eq!(rendered, "2026-01-01T00:00:00Z", "got {rendered}");
    }

    #[test]
    fn format_rfc3339_secs_treats_zero_as_unknown() {
        // Rows in the session store carry `last_used = 0` until
        // `set_latest_session` is called. Rendering that as
        // 1970-01-01 would be misleading.
        assert_eq!(format_rfc3339_secs(0), "<unknown>");
    }

    #[test]
    fn format_rfc3339_secs_treats_negative_as_unknown() {
        assert_eq!(format_rfc3339_secs(-1), "<unknown>");
    }

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
        let subscriber = tracing_subscriber::registry()
            .with(filter)
            .with(RedactLayer)
            .with(fmt_layer);
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
        assert!(is_sensitive_key("recovery_key"));
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
    fn redact_message_handles_non_ascii_boundaries() {
        // R2-H2: a multi-byte UTF-8 char immediately before a
        // sensitive `=` must not panic. `，` (FULLWIDTH COMMA, U+FF0C)
        // is 3 bytes in UTF-8; the old `rfind(...).map(|n| n + 1)`
        // returned byte 1 (mid-codepoint), which panicked on
        // `&s[word_start..eq_rel]`. The char_indices-based walk
        // returns the char boundary instead.
        let out = redact_message("homeserver：，access_token=syt_abcdefgh_long done");
        assert!(!out.contains("syt_abcdefgh_long"), "leak: {out}");
        assert!(out.contains("syt_abcd***"), "redacted form missing: {out}");
        assert!(out.contains("done"), "trailing context lost: {out}");
    }

    #[test]
    fn redact_message_preserves_non_ascii_text_around_sensitive() {
        // Cyrillic / CJK / emoji near a `key=value` pair must not
        // be eaten by the byte-slicing. R2-H2 regression.
        // After the char-boundary fix in `redact_value`, the
        // first 8 BYTES of `syt_вгатая_hello` are
        // `s y t _ в г` = 4 ASCII + 2 Cyrillic chars (4 bytes
        // total). The slice ends at the char boundary BEFORE 8
        // (byte 8 would fall in the middle of `а`, so we step
        // back to byte 6 = end of `г`). Result: `syt_вг***`.
        let out = redact_message("用户 access_token=syt_вгатая_hello конец");
        assert!(!out.contains("syt_вгатая_hello"), "leak: {out}");
        assert!(out.contains("syt_вг***"), "redacted form missing: {out}");
        assert!(out.starts_with("用户 "), "prefix lost: {out}");
        assert!(out.contains(" конец"), "suffix lost: {out}");
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

    #[test]
    fn layer_redacts_sensitive_field_in_span() {
        // R2-H3: tokens placed in span fields must be redacted just
        // like event fields. The default `fmt::Layer` would render
        // them unredacted; the custom `RedactingFormat` walks the
        // span chain (via `ctx.event_scope().from_root()`) and
        // reads the redacted forms from the per-span extension
        // populated by `RedactLayer::on_new_span`.
        let output = run_with_capture(|| {
            let span = tracing::info_span!(
                "auth",
                access_token = "syt_span_token_xyz",
                user_id = "@bot:hs"
            );
            let _e = span.enter();
            tracing::warn!("logged in");
        });
        assert!(
            !output.contains("syt_span_token_xyz"),
            "raw span access_token leaked: {output}"
        );
        assert!(
            output.contains("syt_span***"),
            "expected redacted span access_token, got: {output}"
        );
        // Non-sensitive span field passes through.
        assert!(
            output.contains("@bot:hs"),
            "span user_id should pass through, got: {output}"
        );
        // Span name is rendered in the output.
        assert!(
            output.contains("auth"),
            "span name 'auth' should appear in output, got: {output}"
        );
    }

    #[test]
    fn layer_redacts_nested_span_chain() {
        // R2-H3: nested spans — both inner and outer — get their
        // fields redacted independently.
        // `hunter2_inner_pw` first 8 BYTES = `hunter2_` (8 ASCII
        // chars), so the redacted form is `hunter2_***` (not
        // `hunter2***` — the 8th char IS the underscore).
        let output = run_with_capture(|| {
            let outer = tracing::info_span!("outer", refresh_token = "syr_outer_token_xyz");
            let _oe = outer.enter();
            let inner = tracing::info_span!("inner", password = "hunter2_inner_pw");
            let _ie = inner.enter();
            tracing::warn!("nested event");
        });
        assert!(
            !output.contains("syr_outer_token_xyz"),
            "raw outer refresh_token leaked: {output}"
        );
        assert!(
            !output.contains("hunter2_inner_pw"),
            "raw inner password leaked: {output}"
        );
        assert!(
            output.contains("syr_oute***"),
            "expected redacted outer refresh_token, got: {output}"
        );
        assert!(
            output.contains("hunter2_***"),
            "expected redacted inner password, got: {output}"
        );
    }
}
