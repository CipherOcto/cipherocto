//! Tracing-subscriber init with a token-redaction layer.
//!
//! Mission AC §Tracing redaction. R3-H1: removed `pn` from REDACT_KEYS
//! (device's own phone number is logged unredacted with `+E164` prefix).
//! R2-H2: the redaction layer is a custom `FormatEvent` impl
//! (RedactingFormat), installed as the `event_format` on the
//! `fmt::Layer`.
//!
//! R1-C1: `RedactingFormat` MUST render the event message (format
//! string) — the previous version wrote only `target` + `level` +
//! `meta.name()` + fields, silently dropping the message body. Most
//! `tracing` calls are format-string style (`tracing::info!("hello {}", x)`),
//! not structured-field style, so the message is the only thing the
//! operator sees in the log.

use std::fmt;

use tracing::field::Visit;
use tracing::Event;
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::fmt::FmtContext;
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::EnvFilter;

use crate::cli::Cli;

/// R3-H1: redacted keys. `pn` is intentionally NOT here — the
/// device's own phone number is the operator's own phone, not a
/// secret. `pair_code` is here as defense-in-depth (R3-L1: nothing
/// currently logs it, but a future adapter change might).
const REDACT_KEYS: &[&str] = &[
    "session_path",
    "pair_phone",
    "pair_code",
    "ws_url",
    "access_token",
    "noise_key",
    "identity_key",
    "signed_pre_key",
    "prekey",
    "sender_key",
];

fn is_redact_key(key: &str) -> bool {
    REDACT_KEYS.iter().any(|k| {
        key.len() >= k.len()
            && key
                .as_bytes()
                .windows(k.len())
                .any(|w| w.iter().zip(k.as_bytes()).all(|(a, b)| a.eq_ignore_ascii_case(b)))
    })
}

/// R1-C1: custom `FormatEvent` that renders the event message
/// (the format string with args substituted) and structured
/// fields. The standard `Format` struct in tracing-subscriber
/// handles the message rendering, but emits fields unredacted —
/// so we capture fields via `Visit`, redact, then render the
/// message + redacted fields ourselves.
pub struct RedactingFormat;

impl RedactingFormat {
    pub fn new() -> Self {
        Self
    }
}

impl<S, N> FormatEvent<S, N> for RedactingFormat
where
    S: tracing::Subscriber + for<'lookup> LookupSpan<'lookup>,
    N: for<'writer> FormatFields<'writer> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let meta = event.metadata();
        // Header: target + level + event name (mirrors the
        // standard Format's prefix).
        write!(writer, "{} {} {}", meta.target(), meta.level(), meta.name())?;

        // Visit fields. Capture them into a string first (with
        // redaction), then write the redacted string to the writer.
        // This way the field rendering is consistent regardless of
        // the writer's encoding.
        let mut field_buf = String::new();
        let mut visitor = FieldCollector {
            buf: &mut field_buf,
            started: false,
        };
        event.record(&mut visitor);

        // Render the span context (e.g., `in span: foo`). This
        // matches tracing-subscriber's default Format behavior.
        if let Some(scope) = ctx.event_scope() {
            for span in scope.from_root() {
                write!(writer, " >{}", span.metadata().name())?;
            }
        }

        // Now render the message. The Event itself doesn't
        // expose the format string or args directly via Visit, but
        // the field visitor captures them if the format args are
        // passed as fields (tracing's `tracing::info!("hello {x}",
        // x = 5)`) — see `RecordFields` for the dual path.
        //
        // For the common case (`tracing::info!("hello {x}", x)`),
        // the message is stored on the Event and rendered via the
        // subscriber's internal display mechanism. We approximate
        // this by rendering the visitor's captured fields as the
        // message body, which is what the operator sees.

        // Render captured fields as ` key=value` pairs (space-
        // separated). Secret keys are replaced with `<redacted>`.
        if !field_buf.is_empty() {
            write!(writer, " {field_buf}")?;
        }

        writeln!(writer)
    }
}

/// Field visitor that renders all captured fields into a `String`
/// buffer with redaction applied.
struct FieldCollector<'a> {
    buf: &'a mut String,
    started: bool,
}

impl<'a> Visit for FieldCollector<'a> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        use std::fmt::Write as _;
        // R3-H1: the `message` field carries the formatted message
        // body. Render it without the `message=` prefix so the
        // operator sees natural log output. Other fields render
        // as `key=value` pairs as before.
        if field.name() == "message" {
            let _ = write!(self.buf, " {:?}", value);
            return;
        }
        if !self.started {
            self.buf.push(' ');
            self.started = true;
        }
        let key = field.name();
        if is_redact_key(key) {
            let _ = write!(self.buf, "{}={:?}", key, "<redacted>");
        } else {
            let _ = write!(self.buf, "{}={:?}", key, value);
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        use std::fmt::Write as _;
        // R3-H1: same special-case for the `message` field
        // (record_str is called for &str values, which `tracing`
        // uses for plain string fields).
        if field.name() == "message" {
            let _ = write!(self.buf, " {:?}", value);
            return;
        }
        if !self.started {
            self.buf.push(' ');
            self.started = true;
        }
        let key = field.name();
        if is_redact_key(key) {
            let _ = write!(self.buf, "{}={:?}", key, "<redacted>");
        } else {
            let _ = write!(self.buf, "{}={:?}", key, value);
        }
    }
}

/// R2-H2: install the redaction layer. `verbose=true` flips INFO → DEBUG.
pub fn init(cli: &Cli) {
    let filter = if cli.verbose {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"))
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };

    let fmt_layer = tracing_subscriber::fmt::layer()
        .event_format(RedactingFormat::new())
        .with_writer(std::io::stderr);

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacted_keys_list_does_not_contain_pn() {
        // R3-H1: pn is removed; the device's own phone number is
        // logged unredacted with +E164 prefix.
        assert!(
            !REDACT_KEYS.contains(&"pn"),
            "REDACT_KEYS should not contain 'pn'"
        );
    }

    #[test]
    fn redacted_keys_list_contains_session_path() {
        assert!(REDACT_KEYS.contains(&"session_path"));
        assert!(REDACT_KEYS.contains(&"pair_phone"));
        assert!(REDACT_KEYS.contains(&"pair_code"));
    }

    #[test]
    fn redacted_keys_list_contains_signal_keys() {
        for k in ["noise_key", "identity_key", "signed_pre_key", "prekey"] {
            assert!(REDACT_KEYS.contains(&k), "{k} should be in REDACT_KEYS");
        }
    }

    #[test]
    fn case_insensitive_match_works() {
        assert!(is_redact_key("Session_Path"));
        assert!(is_redact_key("session_path"));
        assert!(is_redact_key("SESSION_PATH"));
    }

    #[test]
    fn non_redacted_field_does_not_match() {
        assert!(!is_redact_key("self_phone"));
    }

    // R3-H1: verify the message field renders without the
    // `message=` prefix. We exercise the visitor via a real
    // tracing Event by capturing the rendered output to a
    // buffer.
    #[test]
    fn message_field_renders_without_prefix() {
        use std::sync::{Arc, Mutex};
        use tracing_subscriber::fmt::MakeWriter;

        // A `MakeWriter` impl that captures to a shared `Vec<u8>`.
        #[derive(Clone)]
        struct CaptureWriter(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for CaptureWriter {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().write(buf)
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
        let _ = tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .event_format(RedactingFormat::new())
                    .with_writer(writer),
            )
            .try_init();

        // Emit a message via the standard `info!` macro.
        tracing::info!("resolved bot identity: +1 555 123 4567");

        let captured = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        // The rendered output should contain the message body
        // WITHOUT the `message=` prefix.
        assert!(
            captured.contains("resolved bot identity: +1 555 123 4567"),
            "captured log: {captured}"
        );
        assert!(
            !captured.contains("message="),
            "captured log should NOT contain 'message=' prefix: {captured}"
        );
        // R4-M1 regression check: the message should NOT be
        // surrounded by Debug's `\"...\"` quotes. The standard
        // tracing-subscriber Format uses format_args! for messages
        // (no Debug wrapping). For `&str` fields, the rendered
        // body is the unwrapped string. If a future maintainer
        // uses `{:?}` for the message, the body would be
        // `"resolved bot identity: +1 555 123 4567"` (with
        // surrounding quotes). Pin against this.
        assert!(
            !captured.contains("\"resolved bot identity: +1 555 123 4567\""),
            "message should NOT be surrounded by Debug quotes: {captured}"
        );
    }
}
