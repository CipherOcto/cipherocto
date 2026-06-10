//! Tests for the error type taxonomy.
//! Mission AC line 100: "thiserror error types (TelegramError, AuthError, FileError)"

use octo_adapter_telegram::{redact_credentials, TelegramError};

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

// =============================================================================
// R6 TEST-C1: redact_credentials tests
// =============================================================================

/// Token in the middle of a sentence should be redacted.
#[test]
fn test_redact_middle_of_sentence() {
    let input = "error: bot token 1234567890:ABCdefGHIjklMNOpqrsTUVwxyz-_ABCDE is invalid";
    let result = redact_credentials(input);
    assert!(result.contains("<redacted>"), "should redact the token");
    assert!(
        !result.contains("1234567890:ABCdefGHIjklMNOpqrsTUVwxyz-_ABCDE"),
        "raw token should not appear in output"
    );
    assert!(result.starts_with("error: bot token "));
    assert!(result.ends_with(" is invalid"));
}

/// Token at the start of the string.
#[test]
fn test_redact_token_at_start() {
    let input = "1234567890:ABCdefGHIjklMNOpqrsTUVwxyz-_ABCDE is here";
    let result = redact_credentials(input);
    assert_eq!(result, "<redacted> is here");
}

/// Token at the end of the string.
#[test]
fn test_redact_token_at_end() {
    let input = "token is 1234567890:ABCdefGHIjklMNOpqrsTUVwxyz-_ABCDE";
    let result = redact_credentials(input);
    assert_eq!(result, "token is <redacted>");
}

/// No token in input — output should equal input.
#[test]
fn test_redact_no_token() {
    let input = "this is a normal error message without any tokens";
    let result = redact_credentials(input);
    assert_eq!(result, input);
}

/// Empty string.
#[test]
fn test_redact_empty() {
    assert_eq!(redact_credentials(""), "");
}

/// Multiple tokens should all be redacted.
#[test]
fn test_redact_multiple_tokens() {
    let input = "first: 1111111111:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA second: 2222222222:BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
    let result = redact_credentials(input);
    assert_eq!(result, "first: <redacted> second: <redacted>");
}

/// Too-short digit prefix (7 digits) should NOT be redacted.
#[test]
fn test_redact_too_short_digits() {
    let input = "short: 1234567:ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij";
    let result = redact_credentials(input);
    // 7 digits is below the 8-digit minimum
    assert!(!result.contains("<redacted>"));
    assert_eq!(result, input);
}

/// Too-long token (41 chars after colon) should NOT be redacted.
/// The scanner greedily consumes all alphanumeric chars, so a 41-char token
/// has token_len=41 which is out of the 30..=40 range.
#[test]
fn test_redact_too_long_token() {
    let input = "long: 12345678:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let result = redact_credentials(input);
    // 41 chars after colon is above the 40-char max
    assert!(!result.contains("<redacted>"), "41-char token should not be redacted, got: {}", result);
}

/// UTF-8 multi-byte characters should pass through unmodified.
#[test]
fn test_redact_utf8_preserved() {
    let input = "café résumé 1234567890:ABCdefGHIjklMNOpqrsTUVwxyz-_ABCDE 中文";
    let result = redact_credentials(input);
    assert!(result.contains("<redacted>"), "should redact the token");
    // UTF-8 text should be preserved around the redacted token
    assert!(result.starts_with("café résumé "), "UTF-8 before token should be preserved");
    assert!(result.ends_with(" 中文"), "UTF-8 after token should be preserved");
    // The original chars should still be valid
    assert_eq!(result.chars().filter(|&c| c == 'é').count(), 3, "é should appear three times (café + résumé)");
    assert_eq!(result.chars().filter(|&c| c == '中').count(), 1, "中 should appear once");
}

/// Token with alphanumeric prefix should not be redacted (word boundary check).
/// The 'c' before '1234567890' means no word boundary break.
#[test]
fn test_redact_word_boundary_prefix() {
    let input = "abc1234567890:ABCdefGHIjklMNOpqrsTUVwxyz-_ABCDE";
    let result = redact_credentials(input);
    // "c" before the digits means no word boundary — not a standalone token
    assert!(!result.contains("<redacted>"), "prefix 'c' prevents word boundary, got: {}", result);
}

/// 'extra' makes the greedy token 40 chars (35 + 5), which IS in the 30..=40 range.
/// The redactor conservatively redacts it since it matches the token pattern.
#[test]
fn test_redact_word_boundary_suffix() {
    let input = "1234567890:ABCdefGHIjklMNOpqrsTUVwxyz-_ABCDEextra";
    let result = redact_credentials(input);
    // Greedy consumption includes 'extra' (40 chars total), so it is redacted
    assert!(result.contains("<redacted>"), "should redact when token+extra fits 30-40 range");
    assert_eq!(result, "<redacted>");
}
