//! Tracing-subscriber init with a token-redaction layer.
//!
//! Mission AC §Tracing redaction. R3-H1: removed `pn` from REDACT_KEYS
//! (device's own phone number is logged unredacted with `+E164` prefix).
//! R2-H2: the redaction layer is a custom `FormatEvent` impl
//! (RedactingFormat), installed as the `event_format` on the
//! `fmt::Layer`. `RedactLayer` is a thin marker for spec compliance.

use std::fmt;

use tracing::field::Visit;
use tracing::Event;
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::fmt::FmtContext;
use tracing_subscriber::layer::Layer;
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

/// Marker layer for spec compliance (R2-H2: same as
/// `octo-matrix-onboard/src/logging.rs:21-25`).
pub struct RedactLayer;

impl<S> Layer<S> for RedactLayer
where
    S: tracing::Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_event(&self, _event: &Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {}
}

/// R3-H1: custom `FormatEvent` that walks the event fields and
/// applies redaction before writing to the writer.
pub struct RedactingFormat<F = tracing_subscriber::fmt::format::DefaultFields> {
    inner: F,
    display_target: bool,
    display_level: bool,
}

impl<F> RedactingFormat<F> {
    pub fn new(inner: F) -> Self {
        Self {
            inner,
            display_target: true,
            display_level: true,
        }
    }
}

impl<S, N, F> FormatEvent<S, N> for RedactingFormat<F>
where
    S: tracing::Subscriber + for<'lookup> LookupSpan<'lookup>,
    N: for<'writer> FormatFields<'writer> + 'static,
    F: 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let meta = event.metadata();
        if self.display_target {
            write!(writer, "{} ", meta.target())?;
        }
        if self.display_level {
            write!(writer, "{} ", meta.level())?;
        }
        write!(writer, "{}", meta.name())?;

        // Visit fields; emit them, redacting any field whose name
        // matches a REDACT_KEYS entry.
        struct FieldVisitor<'a> {
            writer: Writer<'a>,
            redacted: bool,
        }
        impl<'a> Visit for FieldVisitor<'a> {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                let key = field.name();
                let is_secret = REDACT_KEYS
                    .iter()
                    .any(|k| key.to_ascii_lowercase().contains(&k.to_ascii_lowercase()));
                if is_secret {
                    if !self.redacted {
                        let _ = write!(self.writer, " ");
                        self.redacted = true;
                    }
                    let _ = write!(self.writer, "{}={:?}", key, "<redacted>");
                } else {
                    if !self.redacted {
                        let _ = write!(self.writer, " ");
                        self.redacted = true;
                    }
                    let _ = write!(self.writer, "{}={:?}", key, value);
                }
            }
        }
        let mut visitor = FieldVisitor {
            writer: writer.by_ref(),
            redacted: false,
        };
        event.record(&mut visitor);

        writeln!(writer)
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
        .event_format(RedactingFormat::new(
            tracing_subscriber::fmt::format::DefaultFields::new(),
        ))
        .with_writer(std::io::stderr);

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(RedactLayer)
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
        // The matcher is case-insensitive
        let key = "Session_Path";
        let is_secret = REDACT_KEYS
            .iter()
            .any(|k| key.to_ascii_lowercase().contains(&k.to_ascii_lowercase()));
        assert!(is_secret);
    }

    #[test]
    fn non_redacted_field_does_not_match() {
        let key = "self_phone";
        let is_secret = REDACT_KEYS
            .iter()
            .any(|k| key.to_ascii_lowercase().contains(&k.to_ascii_lowercase()));
        // self_phone is NOT in REDACT_KEYS — it's the operator's own
        // phone, not a secret.
        assert!(!is_secret, "self_phone should NOT be redacted");
    }
}
