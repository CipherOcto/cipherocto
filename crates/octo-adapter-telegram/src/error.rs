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

    #[error("TDLib client error: {0}")]
    TdlibClient(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("base64 decode error: {0}")]
    Base64(#[from] base64::DecodeError),

    #[error("envelope error: {0}")]
    Envelope(String),

    #[error("config error: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, TelegramError>;
