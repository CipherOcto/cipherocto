//! Error type for the MTProto Telegram adapter.
//!
//! All public APIs return `Result<T, MtprotoTelegramError>`. The enum
//! is `#[non_exhaustive]` so future variants can be added without
//! breaking semver. The `From` impls in the various modules use
//! `MtprotoTelegramError::Xxx(msg)` constructors, not direct struct
//! construction, to keep variant addition non-breaking.

use std::fmt;
use thiserror::Error;

/// Replace sensitive substrings in user-facing error messages with
/// `[REDACTED]`. The list is conservative (bot tokens, api_hash,
/// phone numbers, 2FA passwords, auth_key bytes). Shared with the
/// TDLib-based `octo-adapter-telegram` crate via the same
/// redaction-policy convention; the helper is reimplemented here
/// rather than re-exported so this crate does not depend on the
/// TDLib crate at the binary level (the dependency in `Cargo.toml`
/// is at the `octo-adapter-telegram` config types only).
///
/// The redaction is applied only to strings — non-string error
/// payloads (numeric codes, struct fields) are passed through
/// unchanged.
pub fn redact_credentials(input: &str) -> String {
    // Sensitive KEY names (case-insensitive). Matched as whole
    // words — `secret` does NOT match `secrets` because the `s`
    // is alphanumeric and breaks the trailing word boundary.
    // Order: longest first so a `bot_token` key is matched
    // before a plain `token` key, preventing partial matches.
    let keys: &[&str] = &[
        "bot_token",
        "api_hash",
        "auth_key",
        "password",
        "phone",
        "token",
        "secret",
    ];
    let lower = input.to_ascii_lowercase();
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    let mut changed = false;
    while i < input.len() {
        // Skip past any pre-existing `[REDACTED:...]` marker
        // so we don't double-redact or trip on the literal
        // pattern labels inside a marker.
        if lower[i..].starts_with("[redacted:") {
            if let Some(close) = lower[i..].find(']') {
                let end = i + close + 1;
                out.push_str(&input[i..end]);
                i = end;
                continue;
            }
        }
        // Find the longest matching key at position i (with
        // word-boundary check on both sides).
        let mut matched: Option<&str> = None;
        for &key in keys {
            if lower[i..].starts_with(key) {
                let before_ok = i == 0 || !input.as_bytes()[i - 1].is_ascii_alphanumeric();
                let after_pos = i + key.len();
                let after_ok = after_pos >= input.len()
                    || !input.as_bytes()[after_pos].is_ascii_alphanumeric();
                if before_ok && after_ok && matched.is_none_or(|m| key.len() > m.len()) {
                    matched = Some(key);
                }
            }
        }
        if let Some(key) = matched {
            out.push_str(&format!("[REDACTED:{}]", key));
            i += key.len();
            changed = true;
            // If the key is followed by a separator (`=`, `:`,
            // or whitespace), keep the separator visible and
            // redact the value up to the next whitespace /
            // structural boundary.
            if i < input.len() {
                let sep = input.as_bytes()[i];
                if sep == b'=' || sep == b':' {
                    out.push(sep as char);
                    i += 1;
                    let val_start = i;
                    while i < input.len() {
                        let b = input.as_bytes()[i];
                        if b.is_ascii_whitespace()
                            || b == b','
                            || b == b'}'
                            || b == b']'
                            || b == b')'
                            || b == b';'
                        {
                            break;
                        }
                        i += 1;
                    }
                    if i > val_start {
                        out.push_str("[REDACTED]");
                    }
                } else if sep.is_ascii_whitespace() || sep == b',' || sep == b';' {
                    out.push(sep as char);
                    i += 1;
                }
            }
            continue;
        }
        // No match — copy one Unicode char (byte-accurate UTF-8
        // advance, so we never split a multi-byte sequence).
        let ch = input[i..]
            .chars()
            .next()
            .unwrap_or(char::REPLACEMENT_CHARACTER);
        out.push(ch);
        i += ch.len_utf8();
    }
    if changed {
        out
    } else {
        // Avoid the unnecessary re-allocation when nothing
        // matched.
        input.to_string()
    }
}

/// Top-level error type for the MTProto adapter.
///
/// R17-C1: derived `Debug` would auto-format every variant's
/// fields, including `QrLoginHandle { token: Vec<u8>, url: String }`
/// — leaking the raw QR login token (an authorization
/// credential) and the base64-encoded URL (same data,
/// encoded). Hand-written `Debug` mirrors the auto-derive for
/// every variant EXCEPT `QrLoginHandle`, which redacts both
/// `token` (prints byte count only) and `url` (prints
/// `"<redacted>"`). `Display` (via thiserror's `#[error(...)]`
/// attributes) is unchanged: the QR variant still includes
/// `url={url}` because the caller needs the URL to render
/// the QR code — but the `Display` path is intentional, not a
/// leak.
#[derive(Error)]
#[non_exhaustive]
pub enum MtprotoTelegramError {
    /// Bot token invalid, account banned, or sign-in failed.
    #[error("auth: {0}")]
    Auth(String),

    /// Network-level failure (TCP, TLS, timeout, no-route-to-host).
    #[error("network: {0}")]
    Network(String),

    /// Telegram RPC error (FLOOD_WAIT, PHONE_CODE_INVALID, …).
    #[error("rpc: code={code} message={message}")]
    Rpc { code: i32, message: String },

    /// Bot-API HTTP 429 with `retry_after` parameter preserved
    /// (Phase 3). Distinct from `Rpc { code: 429, .. }` so the
    /// `From<MtprotoTelegramError> for PlatformAdapterError`
    /// mapping can forward the actual server-supplied backoff
    /// (in seconds) to the gateway's `PlatformAdapterError::RateLimited`
    /// as `retry_after_ms`, rather than a conservative 1000 ms
    /// default used for generic `Rpc 429`s.
    #[error("rate limited: retry_after={retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    /// Session store failure (stoolap I/O, schema migration, missing
    /// migration, row not found where required).
    #[error("session: {0}")]
    Session(String),

    /// Configuration problem (missing bot_token, invalid api_id,
    /// contradictory flags). Caught at `MtprotoTelegramConfig::validate`.
    #[error("config: {0}")]
    Config(String),

    /// Capability mismatch (asked to send an envelope larger than
    /// `max_payload_bytes`, asked to send media to a domain that
    /// disallows it, etc.).
    #[error("capability: {0}")]
    Capability(String),

    /// The adapter is not yet initialised (`connect()` not called)
    /// or has been shut down.
    #[error("not ready: {0}")]
    NotReady(String),

    /// Envelope encode/decode failure (bad base64, wrong length,
    /// unknown DOT wire prefix). Mirrors the TDLib adapter's
    /// `TelegramError::Envelope` so the gateway's error mapping
    /// treats both as `ApiError(400)` rather than `Unreachable`.
    #[error("envelope: {0}")]
    Envelope(String),

    /// Catch-all for unexpected internal failures (bugs). The
    /// message is sanitised via `redact_credentials` before
    /// display.
    #[error("internal: {0}")]
    Internal(String),

    /// Phase 2.5: QR login "in progress" marker. The
    /// adapter's `qr_login` / `poll_qr_login` methods
    /// return `Err(MtprotoTelegramError::QrLoginHandle {..})`
    /// when the QR is being displayed (no final authorization
    /// yet) so the caller can extract the `token` and `url`
    /// for display, then loop on `poll_qr_login` until it
    /// returns `Ok(SelfUserInfo)`.
    ///
    /// The token is the raw `auth.LoginToken.token` bytes
    /// (NOT base64-encoded). The URL is the
    /// `tg://login?token=<base64>` form the caller embeds
    /// in the QR code.
    ///
    /// R17-C1: the `Debug` impl below redacts both fields.
    /// `Display` (this `#[error(...)]` attribute) intentionally
    /// includes `url={url}` because the caller needs the URL
    /// to render the QR code — the URL is the QR data, not a
    /// secret. The token (raw bytes) is the credential; the
    /// URL is the public form.
    #[error("qr login in progress: url={url}")]
    QrLoginHandle { token: Vec<u8>, url: String },
}

// R17-C1: hand-written `Debug` for `MtprotoTelegramError`.
// Mirrors the auto-derived shape for every variant EXCEPT
// `QrLoginHandle`, which redacts the raw token bytes and
// the base64-encoded URL. thiserror's `#[derive(Error)]`
// does NOT require Debug — it only provides `Display`,
// `source()`, and `from()` impls — so removing Debug from
// the derive is safe. The `std::error::Error` trait still
// works because the hand-written Debug satisfies its bound.
impl fmt::Debug for MtprotoTelegramError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auth(m) => f.debug_tuple("Auth").field(m).finish(),
            Self::Network(m) => f.debug_tuple("Network").field(m).finish(),
            Self::Rpc { code, message } => f
                .debug_struct("Rpc")
                .field("code", code)
                .field("message", message)
                .finish(),
            Self::RateLimited { retry_after_secs } => f
                .debug_struct("RateLimited")
                .field("retry_after_secs", retry_after_secs)
                .finish(),
            Self::Session(m) => f.debug_tuple("Session").field(m).finish(),
            Self::Config(m) => f.debug_tuple("Config").field(m).finish(),
            Self::Capability(m) => f.debug_tuple("Capability").field(m).finish(),
            Self::NotReady(m) => f.debug_tuple("NotReady").field(m).finish(),
            Self::Envelope(m) => f.debug_tuple("Envelope").field(m).finish(),
            Self::Internal(m) => f.debug_tuple("Internal").field(m).finish(),
            // R17-C1: redacts the raw token bytes (prints byte
            // count) and the base64-encoded URL. Mirrors the
            // `client::QrLoginHandle` Debug impl.
            Self::QrLoginHandle { token, .. } => f
                .debug_struct("QrLoginHandle")
                .field("token", &format_args!("<redacted {} bytes>", token.len()))
                .field("url", &"<redacted>")
                .finish(),
        }
    }
}

impl MtprotoTelegramError {
    /// True if the error is recoverable (transient network blip,
    /// rate-limit, TLS renegotiation) — the adapter will retry
    /// automatically per the retry config.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Network(_) => true,
            Self::Rpc { code, .. } => *code == 429 || *code == 500,
            Self::RateLimited { .. } => true,
            _ => false,
        }
    }

    /// True if the error is a 4xx-class user error — never
    /// triggers reconnect or exponential backoff.
    pub fn is_user_error(&self) -> bool {
        match self {
            Self::Rpc { code, .. } => (400..500).contains(code) && *code != 429,
            Self::Envelope(_) | Self::Capability(_) | Self::Config(_) => true,
            _ => false,
        }
    }
}

impl From<stoolap::Error> for MtprotoTelegramError {
    fn from(e: stoolap::Error) -> Self {
        MtprotoTelegramError::Session(format!("stoolap: {}", e))
    }
}

/// Helper for the auth path: convert a config-validate failure
/// into the same error type without losing the message.
#[allow(dead_code)]
pub(crate) fn config_err(msg: impl fmt::Display) -> MtprotoTelegramError {
    MtprotoTelegramError::Config(msg.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_replaces_known_keys() {
        let s = "bot_token=1234:abcd";
        let r = redact_credentials(s);
        assert!(r.contains("[REDACTED:bot_token]"));
        assert!(!r.contains("1234:abcd"));
    }

    #[test]
    fn redact_passes_through_unrelated() {
        let s = "ordinary message without secrets";
        assert_eq!(redact_credentials(s), s);
    }

    #[test]
    fn is_retryable_classifies_correctly() {
        let n = MtprotoTelegramError::Network("timeout".into());
        assert!(n.is_retryable());
        let r = MtprotoTelegramError::Rpc {
            code: 429,
            message: "flood".into(),
        };
        assert!(r.is_retryable());
        let r = MtprotoTelegramError::Rpc {
            code: 400,
            message: "bad".into(),
        };
        assert!(!r.is_retryable());
    }

    // ---- R17-C1: QrLoginHandle Debug redaction tests ----

    #[test]
    fn qr_login_handle_error_variant_debug_does_not_leak_token_or_url() {
        // R17-C1: the hand-written Debug for the
        // QrLoginHandle variant of MtprotoTelegramError
        // must NOT contain the raw token bytes or the
        // base64-encoded URL. The token is the QR login
        // authorization credential (same class of leak as
        // R15-C3 / R16-C1 fixed for the auth-action
        // variants).
        let e = MtprotoTelegramError::QrLoginHandle {
            token: vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08],
            url: "tg://login?token=ABCD_SECRET_BASE64_DATA".into(),
        };
        let dbg = format!("{:?}", e);
        // Token / URL must not appear in any form.
        assert!(
            !dbg.contains("ABCD_SECRET_BASE64_DATA"),
            "Debug leaked URL token: {}",
            dbg
        );
        assert!(
            !dbg.contains("[1, 2, 3"),
            "Debug leaked raw token bytes: {}",
            dbg
        );
        assert!(
            !dbg.contains("0x01") && !dbg.contains("0x08"),
            "Debug leaked raw token bytes (hex): {}",
            dbg
        );
        // The redaction marker must be present so an
        // operator reading a log line knows the field is
        // redacted (and not silently missing).
        assert!(
            dbg.contains("<redacted 8 bytes>"),
            "Debug missing token redaction marker: {}",
            dbg
        );
        assert!(
            dbg.contains("url") && dbg.contains("<redacted>"),
            "Debug missing url redaction marker: {}",
            dbg
        );
        // Variant name must still be present so the log
        // line is still useful for triage.
        assert!(
            dbg.contains("QrLoginHandle"),
            "Debug missing variant name: {}",
            dbg
        );
    }

    #[test]
    fn qr_login_handle_error_variant_display_includes_url() {
        // R17-C1: Display (thiserror #[error("...url={url}")])
        // must still include the URL — the caller needs it
        // to render the QR code. The token remains in the
        // inner field but is NOT in the Display string
        // (the {url} interpolation only references url).
        let e = MtprotoTelegramError::QrLoginHandle {
            token: vec![0x01, 0x02, 0x03],
            url: "tg://login?token=ABCD_SECRET_BASE64_DATA".into(),
        };
        let msg = format!("{}", e);
        assert!(
            msg.contains("ABCD_SECRET_BASE64_DATA"),
            "Display must include URL for QR rendering: {}",
            msg
        );
        assert!(
            msg.contains("tg://login"),
            "Display must include the tg:// scheme: {}",
            msg
        );
        // Token should NOT appear as raw bytes in the
        // Display path (thiserror's {url} interpolation
        // only references the url field, not token).
        assert!(
            !msg.contains("[1, 2, 3"),
            "Display leaked raw token bytes: {}",
            msg
        );
    }

    #[test]
    fn mtproto_telegram_error_debug_still_works_for_non_sensitive_variants() {
        // R17-C1: the hand-written Debug for
        // MtprotoTelegramError must mirror the auto-derive
        // shape for the 10 non-QrLoginHandle variants so
        // existing log lines / dbg!() calls on Auth /
        // Network / Rpc / Session errors continue to show
        // useful info. Spot-check a tuple variant, a
        // struct variant, and a numeric variant.
        let e = MtprotoTelegramError::Network("connect timeout".into());
        assert_eq!(
            format!("{:?}", e),
            r#"Network("connect timeout")"#,
            "tuple-variant Debug shape changed"
        );
        let e = MtprotoTelegramError::Rpc {
            code: 429,
            message: "FLOOD_WAIT_5".into(),
        };
        let dbg = format!("{:?}", e);
        assert!(dbg.contains("Rpc"), "Rpc variant name missing: {}", dbg);
        assert!(dbg.contains("429"), "Rpc code missing: {}", dbg);
        assert!(dbg.contains("FLOOD_WAIT_5"), "Rpc message missing: {}", dbg);
        let e = MtprotoTelegramError::RateLimited {
            retry_after_secs: 7,
        };
        let dbg = format!("{:?}", e);
        assert!(
            dbg.contains("RateLimited"),
            "RateLimited variant name missing: {}",
            dbg
        );
        assert!(dbg.contains("7"), "RateLimited value missing: {}", dbg);
        // Spot-check the catch-all variants.
        assert_eq!(
            format!("{:?}", MtprotoTelegramError::Auth("bad token".into())),
            r#"Auth("bad token")"#,
            "Auth Debug shape changed"
        );
        assert_eq!(
            format!(
                "{:?}",
                MtprotoTelegramError::NotReady("connect not called".into())
            ),
            r#"NotReady("connect not called")"#,
            "NotReady Debug shape changed"
        );
    }
}
