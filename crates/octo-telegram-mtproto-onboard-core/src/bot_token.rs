//! Bot-token onboarding flow.
//!
//! Validates a bot token (non-empty, plausible shape), invokes
//! `MtprotoTelegramAdapter::connect_bot_token`, waits for the
//! lifecycle to reach `Ready`, captures the `MtprotoSelfHandle`,
//! and writes a `SessionRecord` to the data dir.
//!
//! ## Production wiring
//!
//! The `run` function is generic over the `MtprotoTelegramClient`
//! trait so it can be exercised by both the production `connect_real`
//! factory and the test-only `MockTelegramMtprotoClient`. Production
//! callers obtain the adapter via [`crate::connect`] (which uses
//! `connect_real` under the hood); tests obtain one via the
//! `#[cfg(test)]` `connect_mock_for_test` helper.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use octo_adapter_telegram_mtproto::{MtprotoTelegramAdapter, MtprotoTelegramClient};
use tokio::sync::oneshot;
use tracing::{debug, info};

use crate::auth::auth_state_name;
use crate::error::OnboardError;
use crate::output::{OnboardMode, OnboardOutput};
use crate::session::SessionRecord;

/// Validates a bot token shape. Telegram bot tokens are
/// `<bot_id>:<47-ish random chars>` (e.g.
/// `123456789:AAEhBOweik6ad9JQB...`). We do a cheap structural
/// check (non-empty, contains `:`) — the adapter's
/// `sign_in_bot` does the real `auth.botSignIn` RPC.
pub fn validate_bot_token(token: &str) -> Result<(), OnboardError> {
    if token.is_empty() {
        return Err(OnboardError::InvalidInput("bot token is empty".to_string()));
    }
    if !token.contains(':') {
        return Err(OnboardError::InvalidInput(
            "bot token must be in the form '<bot_id>:<auth>'".to_string(),
        ));
    }
    Ok(())
}

/// Run the bot-token onboarding flow to completion.
///
/// `bot_token` is the raw token (e.g. `123456789:AA...`).
/// `data_dir` is the on-disk location where the session and
/// config files will be written.
///
/// On success returns a populated `OnboardOutput` and the path
/// to the written config JSON. On failure returns
/// `OnboardError::NotReady` with the last-observed lifecycle
/// state, or a more specific variant for I/O / API errors.
///
/// The function is generic over the client impl so the same
/// code path drives the real grammers-backed client (in
/// production) and the in-memory mock (in unit tests).
pub async fn run<C>(
    adapter: Arc<MtprotoTelegramAdapter<C>>,
    bot_token: &str,
    data_dir: &Path,
) -> Result<(OnboardOutput, PathBuf), OnboardError>
where
    C: MtprotoTelegramClient + 'static,
{
    validate_bot_token(bot_token)?;
    let start = Instant::now();
    info!(path = "bot_token", "starting bot-token onboarding");
    debug!(data_dir = %data_dir.display(), "using data dir");

    // Drive the connect. We use the public `connect_bot_token`
    // method on the adapter (no interactive channels needed for
    // bot mode).
    if let Err(e) = adapter.connect_bot_token(bot_token).await {
        return Err(map_adapter_error(&auth_state_name(&adapter), e));
    }

    if !adapter.has_valid_session() {
        return Err(OnboardError::NotReady {
            last_state: auth_state_name(&adapter),
        });
    }

    let identity = adapter
        .self_handle_ref()
        .get()
        .ok_or_else(|| OnboardError::NotReady {
            last_state: auth_state_name(&adapter),
        })?;
    let elapsed = start.elapsed();

    let record = SessionRecord::from_identity(&identity, "bot_token", unix_now_secs());
    let _session_path = record.write_to(data_dir)?;
    let config_path = data_dir.join("config.json");

    let output = OnboardOutput {
        schema_version: OnboardOutput::SCHEMA_VERSION,
        mode: OnboardMode::BotToken,
        self_id: identity.user_id,
        self_username: identity.username.clone(),
        is_bot: true,
        data_dir: data_dir.display().to_string(),
        config_path: config_path.display().to_string(),
        elapsed_ms: elapsed.as_millis() as u64,
    };
    info!(
        user_id = identity.user_id,
        elapsed_ms = output.elapsed_ms,
        "bot-token onboarding complete"
    );
    Ok((output, config_path))
}

/// Map an adapter error to the most specific `OnboardError`
/// variant. Pure function, easily testable.
fn map_adapter_error(
    last_state: &str,
    err: octo_adapter_telegram_mtproto::MtprotoTelegramError,
) -> OnboardError {
    use octo_adapter_telegram_mtproto::MtprotoTelegramError as E;
    match err {
        E::Config(_) => OnboardError::Config(err.to_string()),
        E::Auth(_) => OnboardError::TelegramApi(err.to_string()),
        E::Rpc { .. } => OnboardError::TelegramApi(err.to_string()),
        E::RateLimited { .. } => OnboardError::TelegramApi(err.to_string()),
        E::Session(_) => OnboardError::Adapter(err.to_string()),
        E::Network(_) => OnboardError::Network(err.to_string()),
        E::Capability(_) => OnboardError::Adapter(err.to_string()),
        E::NotReady(_) => OnboardError::NotReady {
            last_state: last_state.to_string(),
        },
        E::Envelope(_) => OnboardError::Adapter(err.to_string()),
        E::Internal(_) => OnboardError::Adapter(err.to_string()),
        // QrLoginHandle can never come out of connect_bot_token
        // (that's only emitted by the QR flow), but the
        // match must be exhaustive (the enum is
        // #[non_exhaustive] upstream).
        E::QrLoginHandle { .. } => OnboardError::Adapter(err.to_string()),
        // Forward-compatible: any future variants land here.
        other => OnboardError::Adapter(other.to_string()),
    }
}

fn unix_now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// `oneshot` is re-exported here for callers that want to wire
// the bot_token flow into a custom runtime.
#[allow(dead_code)]
fn _ensure_oneshot_in_scope() -> oneshot::Sender<()> {
    let (tx, _rx) = oneshot::channel();
    tx
}

#[allow(dead_code)]
const _DURATION_TYPECHECK: Duration = Duration::from_secs(0);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::mock_adapter_for_test;
    use tempfile::tempdir;

    #[test]
    fn validate_bot_token_rejects_empty() {
        let e = validate_bot_token("").unwrap_err();
        assert_eq!(e.kind(), "invalid_input");
    }

    #[test]
    fn validate_bot_token_rejects_no_colon() {
        let e = validate_bot_token("no-colon-here").unwrap_err();
        assert_eq!(e.kind(), "invalid_input");
    }

    #[test]
    fn validate_bot_token_accepts_canonical_form() {
        validate_bot_token("123456789:AAEhBOweik6ad9JQBxxx").unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn run_succeeds_for_fresh_adapter() {
        // The mock client accepts any token and resolves a
        // self-handle. This is the *test* path — production
        // uses the real client behind the `real-network`
        // feature, which performs the actual `auth.botSignIn`
        // RPC.
        let tmp = tempdir().unwrap();
        let adapter = mock_adapter_for_test(tmp.path());
        let (out, _cfg_path) = run(adapter, "999:AAA", tmp.path())
            .await
            .expect("bot-token run should succeed against mock");
        assert!(out.is_bot);
        assert!(out.self_id != 0);
        assert_eq!(out.mode, OnboardMode::BotToken);
        // Session file was written.
        assert!(tmp.path().join("session.json").exists());
    }
}
