//! Error types for the Telegram adapter.
//! Mission AC line 100: "thiserror error types (TelegramError, AuthError, FileError)"

use thiserror::Error;

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

    #[error("TDLib client error: {0}")]
    TdlibClient(String),

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

    #[error("send failed: {0}")]
    SendFailed(String),

    #[error("unimplemented: {0}")]
    Unimplemented(String),
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
