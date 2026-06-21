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
                let before_ok = i == 0
                    || !input.as_bytes()[i - 1].is_ascii_alphanumeric();
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
        let ch = input[i..].chars().next().unwrap_or(char::REPLACEMENT_CHARACTER);
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
#[derive(Debug, Error)]
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
}

impl MtprotoTelegramError {
    /// True if the error is recoverable (transient network blip,
    /// rate-limit, TLS renegotiation) — the adapter will retry
    /// automatically per the retry config.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Network(_) => true,
            Self::Rpc { code, .. } => *code == 429 || *code == 500,
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
        let r = MtprotoTelegramError::Rpc { code: 429, message: "flood".into() };
        assert!(r.is_retryable());
        let r = MtprotoTelegramError::Rpc { code: 400, message: "bad".into() };
        assert!(!r.is_retryable());
    }
}
