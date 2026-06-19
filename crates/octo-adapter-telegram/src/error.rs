//! Error types for the Telegram adapter.
//! Mission AC line 100: "thiserror error types (TelegramError, AuthError, FileError)"
//!
//! R4 Auth H3: The `redact_credentials` helper masks bot-token-shaped
//! substrings (`<digits>:<base64chars>`) from any string, intended for
//! use on TDLib error messages before they reach `tracing::debug!` or
//! `Display` impls.

use thiserror::Error;

/// Mask bot-token-shaped patterns (`<8-12 digits>:<30-40 base64 chars>`)
/// in a string, replacing them with `<redacted>`. This is intended for
/// sanitising TDLib error messages and log strings that could contain
/// the bot token echoed by upstream systems.
///
/// The pattern matches the Telegram Bot API token format:
/// `1234567890:ABCdefGHIjklMNOpqrsTUVwxyz-_ABCDE` (roughly 8–12 digits
/// followed by a colon and 30–40 base64url-safe characters).
///
/// This is not a cryptographic redaction — it is a best-effort scrubber
/// to reduce accidental credential leakage in logs and error displays.
pub fn redact_credentials(s: &str) -> String {
    // Manual scanner: walk the string looking for `<digits>:` patterns
    // Bot tokens are 8-10 digits (R7 CRYPTO-L1), followed by 30-40 base64url chars (alphanumeric + `-` + `_`).
    // Avoids pulling in a regex dependency for a simple pattern.
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        // Check for a digit-colon boundary: 8–12 consecutive digits
        // immediately followed by `:`.
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            let digit_len = i - start;
            if (8..=10).contains(&digit_len) && i < bytes.len() && bytes[i] == b':' {
                let colon_pos = i;
                i += 1; // skip `:`
                        // Consume base64url-safe chars (alphanumeric, `-`, `_`).
                        // We consume greedily then check bounds so the length range
                        // check correctly rejects tokens > 40 chars.
                let token_start = i;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_')
                {
                    i += 1;
                }
                let token_len = i - token_start;
                if (30..=40).contains(&token_len) {
                    // Check that the match is bounded by a word boundary:
                    // the preceding char (if any) should not be alphanumeric or `-`/`_`,
                    // and the following char (if any) should not be alphanumeric or `-`/`_`.
                    // Because we limited consumption to at most 40 chars, `bytes[i]`
                    // is either (a) a non-alphanumeric boundary or (b) end-of-string
                    // or (c) an alphanumeric char beyond 40 which means no boundary.
                    let preceded_by_word_char = start > 0
                        && (bytes[start - 1].is_ascii_alphanumeric()
                            || bytes[start - 1] == b'-'
                            || bytes[start - 1] == b'_');
                    let followed_by_word_char = i < bytes.len()
                        && (bytes[i].is_ascii_alphanumeric()
                            || bytes[i] == b'-'
                            || bytes[i] == b'_');
                    if !preceded_by_word_char && !followed_by_word_char {
                        out.push_str("<redacted>");
                        continue;
                    }
                }
                // Not a valid token match — backtrack.
                // Push everything from colon back through the consumed region.
                out.push_str(&s[start..=colon_pos]);
                out.push_str(&s[colon_pos + 1..i]);
                continue;
            }
            // Not a digit-colon sequence, push the digits we consumed.
            out.push_str(&s[start..i]);
            continue;
        }
        // R6 TEST-C1: Use `.chars().next()` instead of `bytes[i] as char`
        // to avoid UTF-8 byte-splitting corruption. Single-byte `as char`
        // cast is technically valid for u8→char (all u8 values 0-255 are
        // valid Unicode scalar values), but encoding-wise, non-ASCII bytes
        // that are part of multi-byte UTF-8 sequences would produce garbled
        // output. Using char-level iteration preserves the original encoding.
        let ch = s[i..].chars().next().unwrap_or(char::REPLACEMENT_CHARACTER);
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

#[derive(Debug, Error)]
pub enum TelegramError {
    #[error("auth error: {0}")]
    Auth(String),

    #[error("file transfer error: {0}")]
    File(String),

    #[error("rate limited: retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    /// M6: transient/recoverable error (5xx, "connection failed", etc.).
    /// `send_with_retry` retries this with the same exponential-backoff
    /// policy as `RateLimited`. Once `max_retries` is exhausted the adapter
    /// surfaces a `PlatformAdapterError::Unreachable` to the caller.
    #[error("transient error: {0}")]
    Transient(String),

    /// TDLib client error. The `code` field carries the TDLib error code
    /// (e.g. 403 for CHAT_WRITE_FORBIDDEN, 429 for FLOOD_WAIT) so operators
    /// can distinguish permanent errors from transient ones (R7 OBS-C2, OBS-H5).
    #[error("TDLib client error (code={code}): {message}")]
    TdlibClient { code: u16, message: String },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("base64 decode error: {0}")]
    Base64(#[from] base64::DecodeError),

    #[error("envelope error: {0}")]
    Envelope(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("invalid chat id: {0}")]
    InvalidChatId(String),

    #[error("invalid file id: {0}")]
    InvalidFileId(String),

    #[error("send failed: {0}")]
    SendFailed(String),

    #[error("unimplemented: {0}")]
    Unimplemented(String),
}

impl TelegramError {
    /// Returns `true` if this error is recoverable via retry.
    ///
    /// R4 M9: `with_retry` uses this method instead of variant-matching
    /// so that adding a new recoverable variant does not require editing
    /// the retry loop — the compiler will remind us to update this method.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            TelegramError::RateLimited { .. } | TelegramError::Transient(_)
        )
    }

    /// Returns the name of the enum variant as a static string (e.g.
    /// `"InvalidChatId"`, `"Auth"`). Used by `From<TelegramError> for
    /// PlatformAdapterError` to preserve the discriminant in error messages
    /// (R5 error-prop-C1, H3).
    pub fn variant_name(&self) -> &'static str {
        match self {
            TelegramError::Auth(_) => "Auth",
            TelegramError::File(_) => "File",
            TelegramError::RateLimited { .. } => "RateLimited",
            TelegramError::Transient(_) => "Transient",
            TelegramError::TdlibClient { .. } => "TdlibClient",
            TelegramError::Io(_) => "Io",
            TelegramError::Json(_) => "Json",
            TelegramError::Base64(_) => "Base64",
            TelegramError::Envelope(_) => "Envelope",
            TelegramError::Config(_) => "Config",
            TelegramError::InvalidChatId(_) => "InvalidChatId",
            TelegramError::InvalidFileId(_) => "InvalidFileId",
            TelegramError::SendFailed(_) => "SendFailed",
            TelegramError::Unimplemented(_) => "Unimplemented",
        }
    }
}

pub type Result<T> = std::result::Result<T, TelegramError>;

/// File transfer error types. Available in both feature sets; the
/// `real-tdlib`-specific `Tdlib` variant is feature-gated so callers using
/// `--no-default-features` still get a useful error type.
#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error("file not found: {0}")]
    NotFound(String),

    #[error("file too large: {size} bytes (max: {max} bytes)")]
    TooLarge { size: u64, max: u64 },

    #[error("download failed: {0}")]
    DownloadFailed(String),

    #[error("upload failed: {0}")]
    UploadFailed(String),

    #[error("invalid file id: {0}")]
    InvalidFileId(String),

    #[error("read error: {0}")]
    ReadError(String),

    #[error("write error: {0}")]
    WriteError(String),

    #[error("unimplemented: {0}")]
    Unimplemented(String),

    #[cfg(feature = "real-tdlib")]
    #[error("TDLib error: {message}")]
    Tdlib { message: String },
}

pub type FileResult<T> = std::result::Result<T, FileError>;
