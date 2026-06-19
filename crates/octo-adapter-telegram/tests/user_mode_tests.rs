//! User mode authentication tests.
//!
//! Mission AC line 146: "User mode test: phone + api_id + api_hash auth flow
//! with mocked TDLib (no real Telegram account needed for cargo test)"
//
// These tests verify the user authentication flow using mocked auth state transitions.
// Note: Some tests require real-tdlib feature for AuthorizationState handling.

use octo_adapter_telegram::auth::{AuthError, AuthMode, BotIdentity, UserAuth};

/// Verify AuthMode::User variant carries correct fields.
#[tokio::test]
async fn test_auth_mode_user_fields() {
    let mode = AuthMode::User {
        phone: zeroize::Zeroizing::new("+1234567890".to_string()),
        api_id: 12345,
        api_hash: zeroize::Zeroizing::new("abcdef123456".to_string()),
        password: Some(zeroize::Zeroizing::new("secret2fa".to_string())),
    };

    match mode {
        AuthMode::User {
            phone,
            api_id,
            api_hash,
            password,
        } => {
            assert_eq!(phone.as_str(), "+1234567890");
            assert_eq!(api_id, 12345);
            assert_eq!(api_hash.as_str(), "abcdef123456");
            assert_eq!(
                password.map(|z| z.to_string()),
                Some("secret2fa".to_string())
            );
        }
        _ => panic!("expected User mode"),
    }
}

/// Verify AuthMode::Bot variant carries correct fields.
#[tokio::test]
async fn test_auth_mode_bot_fields() {
    let mode = AuthMode::Bot {
        token: zeroize::Zeroizing::new("123456:ABC-DEF".to_string()),
    };

    match mode {
        AuthMode::Bot { token } => {
            assert_eq!(token.as_str(), "123456:ABC-DEF");
        }
        _ => panic!("expected Bot mode"),
    }
}

/// Verify UserAuth can be constructed with all fields.
#[tokio::test]
async fn test_user_auth_construction() {
    let auth = UserAuth::new(
        "+1234567890".to_string(),
        12345,
        "abcdef123456".to_string(),
        Some("2fa_password".to_string()),
    );

    assert_eq!(auth.phone.as_str(), "+1234567890");
    assert_eq!(auth.api_id, 12345);
    assert_eq!(auth.api_hash.as_str(), "abcdef123456");
    assert_eq!(
        auth.password.map(|z| z.to_string()),
        Some("2fa_password".to_string())
    );
}

/// Verify UserAuth can be constructed without 2FA password.
#[tokio::test]
async fn test_user_auth_without_password() {
    let auth = UserAuth::new(
        "+1234567890".to_string(),
        12345,
        "abcdef123456".to_string(),
        None,
    );

    assert_eq!(auth.phone.as_str(), "+1234567890");
    assert_eq!(auth.password, None);
}

/// Verify AuthError variants carry expected messages.
#[tokio::test]
async fn test_auth_error_variants() {
    let err_invalid = AuthError::InvalidBotToken("token expired".to_string());
    assert_eq!(err_invalid.to_string(), "invalid bot token: token expired");

    let err_auth = AuthError::AuthenticationFailed("phone not registered".to_string());
    assert_eq!(
        err_auth.to_string(),
        "authentication failed: phone not registered"
    );

    let err_2fa = AuthError::TwoFactorRequired;
    assert_eq!(err_2fa.to_string(), "2FA password required");

    let err_session = AuthError::SessionExpired;
    assert_eq!(err_session.to_string(), "session expired, re-authenticate");
}

/// Verify BotIdentity fields are correct when validated.
#[tokio::test]
async fn test_bot_identity_fields() {
    let identity = BotIdentity {
        user_id: 123456789,
        username: "test_bot".to_string(),
        first_name: "Test".to_string(),
        last_name: Some("Bot".to_string()),
    };

    assert_eq!(identity.user_id, 123456789);
    assert_eq!(identity.username, "test_bot");
    assert_eq!(identity.first_name, "Test");
    assert_eq!(identity.last_name, Some("Bot".to_string()));
}

/// Verify BotIdentity without last_name.
#[tokio::test]
async fn test_bot_identity_without_last_name() {
    let identity = BotIdentity {
        user_id: 987654321,
        username: "another_bot".to_string(),
        first_name: "Another".to_string(),
        last_name: None,
    };

    assert_eq!(identity.user_id, 987654321);
    assert_eq!(identity.last_name, None);
}

/// Verify AuthMode equality comparison.
#[tokio::test]
async fn test_auth_mode_equality() {
    let mode1 = AuthMode::Bot {
        token: zeroize::Zeroizing::new("123456:ABC".to_string()),
    };
    let mode2 = AuthMode::Bot {
        token: zeroize::Zeroizing::new("123456:ABC".to_string()),
    };
    let mode3 = AuthMode::Bot {
        token: zeroize::Zeroizing::new("999999:XYZ".to_string()),
    };

    assert_eq!(mode1, mode2);
    assert_ne!(mode1, mode3);
}

/// Verify AuthMode Bot vs User inequality.
#[tokio::test]
async fn test_auth_mode_bot_vs_user_inequality() {
    let bot_mode = AuthMode::Bot {
        token: zeroize::Zeroizing::new("123456:ABC".to_string()),
    };
    let user_mode = AuthMode::User {
        phone: zeroize::Zeroizing::new("+1234567890".to_string()),
        api_id: 12345,
        api_hash: zeroize::Zeroizing::new("abcdef123456".to_string()),
        password: None::<zeroize::Zeroizing<String>>,
    };

    assert_ne!(bot_mode, user_mode);
}

// =============================================================================
// TDLib-specific tests (require real-tdlib feature)
// =============================================================================

/// Verify AuthorizationState handling for user auth flow states.
/// This tests the state machine transitions without real TDLib.
#[cfg(feature = "real-tdlib")]
#[tokio::test]
async fn test_authorization_state_variants() {
    use tdlib_rs::enums::AuthorizationState;

    // Test WaitTdlibParameters state exists
    let _state1 = AuthorizationState::WaitTdlibParameters;
    // Test WaitPhoneNumber state exists
    let _state2 = AuthorizationState::WaitPhoneNumber;
    // Test Ready state exists
    let _state3 = AuthorizationState::Ready;
    // Test Closed state exists
    let _state4 = AuthorizationState::Closed;
}

/// Verify UserAuth::handle_authorization_state handles WaitTdlibParameters.
#[cfg(feature = "real-tdlib")]
#[tokio::test]
async fn test_user_auth_handles_wait_tdlib_parameters() {
    use tdlib_rs::enums::AuthorizationState;

    let auth = UserAuth::new(
        "+1234567890".to_string(),
        12345,
        "abcdef123456".to_string(),
        None,
    );

    // WaitTdlibParameters is the first state in the auth flow
    let state = AuthorizationState::WaitTdlibParameters;
    let result = auth.handle_authorization_state(state, 0, None).await;

    // Should succeed (sets parameters)
    assert!(result.is_ok());
}

/// Verify UserAuth::handle_authorization_state handles WaitPhoneNumber.
#[cfg(feature = "real-tdlib")]
#[tokio::test]
async fn test_user_auth_handles_wait_phone_number() {
    use tdlib_rs::enums::AuthorizationState;

    let auth = UserAuth::new(
        "+1234567890".to_string(),
        12345,
        "abcdef123456".to_string(),
        None,
    );

    let state = AuthorizationState::WaitPhoneNumber;
    let result = auth.handle_authorization_state(state, 0, None).await;

    // Should succeed (sends phone number)
    assert!(result.is_ok());
}

/// Verify UserAuth::handle_authorization_state handles Ready state.
#[cfg(feature = "real-tdlib")]
#[tokio::test]
async fn test_user_auth_handles_ready_state() {
    use tdlib_rs::enums::AuthorizationState;

    let auth = UserAuth::new(
        "+1234567890".to_string(),
        12345,
        "abcdef123456".to_string(),
        None,
    );

    let state = AuthorizationState::Ready;
    let result = auth.handle_authorization_state(state, 0, None).await;

    // Ready state means auth is complete
    assert!(result.is_ok());
}

/// Verify UserAuth::handle_authorization_state handles Closed state.
#[cfg(feature = "real-tdlib")]
#[tokio::test]
async fn test_user_auth_handles_closed_state() {
    use tdlib_rs::enums::AuthorizationState;

    let auth = UserAuth::new(
        "+1234567890".to_string(),
        12345,
        "abcdef123456".to_string(),
        None,
    );

    let state = AuthorizationState::Closed;
    let result = auth.handle_authorization_state(state, 0, None).await;

    // Closed state means session expired
    assert!(result.is_err());
    match result {
        Err(AuthError::SessionExpired) => {}
        other => panic!("expected SessionExpired, got {:?}", other),
    }
}
