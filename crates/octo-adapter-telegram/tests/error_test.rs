//! Tests for the error type taxonomy.
//! Mission AC line 100: "thiserror error types (TelegramError, AuthError, FileError)"

use octo_adapter_telegram::TelegramError;

#[test]
fn test_error_display_includes_context() {
    let err = TelegramError::Auth("invalid api_id".into());
    let msg = format!("{}", err);
    assert!(msg.contains("auth"));
    assert!(msg.contains("invalid api_id"));
}

#[test]
fn test_error_from_io_error() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
    let tg_err: TelegramError = io_err.into();
    let msg = format!("{}", tg_err);
    assert!(msg.contains("io") || msg.contains("file"));
}

#[test]
fn test_error_from_serde_json_error() {
    let json_err = serde_json::from_str::<i32>("not a number").unwrap_err();
    let tg_err: TelegramError = json_err.into();
    let msg = format!("{}", tg_err);
    assert!(msg.contains("json") || msg.contains("parse"));
}
