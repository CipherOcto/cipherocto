//! Pair-code-based linking.
//!
//! R5-H1: use the shared `wait_for_connected` helper. R5-M2:
//! sidecar-first ordering. R1-C2: the `custom_code` is passed to
//! `WhatsAppWebAdapter` for the link, then **dropped** after
//! `Event::Connected`. It never enters the on-disk config, the
//! sidecar, or the `WhatsAppSession` struct.
//!
//! R1-M3: the field name is `custom_code` to match the SDK's
//! `PairCodeOptions::custom_code`. The flag is `--pair-code` and the
//! env var is `$OCTO_WHATSAPP_PAIR_CODE` for operator familiarity.

use octo_adapter_whatsapp::{passkey::PasskeyAuthenticator, WhatsAppConfig, WhatsAppWebAdapter};
use octo_network::dot::adapters::PlatformAdapter;

use crate::error::{CoreError, Result};
use crate::output::PairLinkArgs;
use crate::output::WhatsAppSession;
use crate::sidecar::{write_sidecar, SidecarMode};
use std::sync::Arc;

/// Run the pair-link flow: validate phone, build adapter with
/// `pair_phone` and `custom_code`, start bot, wait for
/// `Event::Connected`, write sidecar + session.
///
/// R1-M4: takes `&&PairLinkArgs` (by reference) so the binary can
/// pass the args struct directly without `clone()`-ing the
/// `OutputArgs` field.
///
/// Session 12: `passkey_authenticator` is supplied by the binary
/// (same shape as qr-link). When `Event::PairPasskeyRequest`
/// fires, wacore invokes it inline; FIDO QR appears on stderr at
/// that moment. No separate `companion-link` subcommand.
pub async fn run(
    args: &PairLinkArgs,
    passkey_authenticator: Option<Arc<dyn PasskeyAuthenticator>>,
) -> Result<WhatsAppSession> {
    validate_pair_link_args(args)?;

    let config = WhatsAppConfig {
        session_path: format!("{}", args.session_path.display()),
        pair_phone: Some(args.phone.clone()),
        pair_code: args.custom_code.clone(),
        ws_url: args.ws_url.clone(),
        groups: args.groups.clone(),
        sender_allowlist: Default::default(),
        passkey_authenticator,
    };
    config
        .validate()
        .map_err(|e| CoreError::InvalidSessionPath {
            path: args.session_path.clone(),
            reason: e,
        })?;

    let adapter = WhatsAppWebAdapter::new(config);
    adapter
        .start_bot()
        .await
        .map_err(|e| CoreError::Adapter(anyhow::anyhow!("start_bot: {e}")))?;

    let phone = crate::session::wait_for_connected(
        &adapter,
        std::time::Duration::from_secs(args.timeout_secs),
    )
    .await?;

    let session = WhatsAppSession {
        self_phone: Some(phone),
        session_path: args.session_path.clone(),
        groups: args.groups.clone(),
        pair_phone: Some(args.phone.clone()),
    };

    // R5-M2: sidecar first, before any config write.
    write_sidecar(&args.session_path, &session, SidecarMode::PairLink)?;

    // Shut down the adapter to close the WebSocket and stop
    // background tasks so the CLI process can exit cleanly.
    let _ = adapter.shutdown().await;

    Ok(session)
}

fn validate_pair_link_args(args: &PairLinkArgs) -> Result<()> {
    validate_phone(&args.phone)?;
    crate::validate_session_args(&args.session_path)
}

/// E.164 phone validation: `+` followed by 7-15 ASCII digits,
/// no leading 0 after `+`. Mirrors the adapter's `is_e164` helper.
fn validate_phone(phone: &str) -> Result<()> {
    if !phone.starts_with('+') {
        return Err(CoreError::InvalidPhone {
            value: phone.to_string(),
            reason: "missing leading +".to_string(),
        });
    }
    let digits = &phone[1..];
    if digits.is_empty() || digits.len() < 7 || digits.len() > 15 {
        return Err(CoreError::InvalidPhone {
            value: phone.to_string(),
            reason: format!("expected 7-15 digits, got {}", digits.len()),
        });
    }
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(CoreError::InvalidPhone {
            value: phone.to_string(),
            reason: "non-digit character".to_string(),
        });
    }
    if digits.starts_with('0') {
        return Err(CoreError::InvalidPhone {
            value: phone.to_string(),
            reason: "leading 0 after +".to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_phone_accepts_e164() {
        assert!(validate_phone("+15551234567").is_ok());
        assert!(validate_phone("+1234567").is_ok());
    }

    #[test]
    fn validate_phone_rejects_malformed() {
        for bad in [
            "5551234",        // no +
            "+0123456789",    // leading 0
            "+1-555-1234567", // non-digit
            "+",              // no digits
            "+abcdefg",       // non-digit
        ] {
            assert!(
                validate_phone(bad).is_err(),
                "phone {bad:?} should be rejected"
            );
        }
    }
}
