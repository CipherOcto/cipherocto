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

use crate::adapter_error;
use crate::auth::auth_state_name;
use crate::error::OnboardError;
use crate::output::{OnboardMode, OnboardOutput};
use crate::session::SessionRecord;

/// Validates a bot token shape. Telegram bot tokens are
/// `<bot_id>:<47-ish random chars>` (e.g.
/// `123456789:AAEhBOweik6ad9JQB...`). We do a cheap structural
/// check — both halves of the colon-separated pair must be
/// non-empty, the bot_id must be all digits, and the auth
/// half must be 30+ characters of `[A-Za-z0-9_-]`. The
/// adapter's `sign_in_bot` does the real `auth.botSignIn`
/// RPC.
///
/// IE-1 (R26): the prior version only checked `is_empty` +
/// `contains(':')`, which let through tokens like `":"`,
/// `"::abc"`, or `"123:"` (empty auth half). The bot API
/// would later reject these with a 401, but the failure
/// surfaces only after we've opened a network connection
/// and the operator has typed something — better to catch
/// obvious typos at the prompt.
pub fn validate_bot_token(token: &str) -> Result<(), OnboardError> {
    if token.is_empty() {
        return Err(OnboardError::InvalidInput("bot token is empty".to_string()));
    }
    // The canonical form has exactly ONE colon. Reject
    // extra colons, leading/trailing colons, and embedded
    // double colons (`"::"`, `":foo"`, `"foo:"`,
    // `"a::b"`).
    let colon_count = token.bytes().filter(|b| *b == b':').count();
    if colon_count != 1 {
        return Err(OnboardError::InvalidInput(format!(
            "bot token must contain exactly one ':' separator (got {} colons)",
            colon_count
        )));
    }
    // Split on the colon and validate both halves.
    let (id_part, auth_part) = token
        .split_once(':')
        .expect("colon_count == 1 implies split_once succeeds");
    if id_part.is_empty() {
        return Err(OnboardError::InvalidInput(
            "bot token: bot id (before ':') is empty".to_string(),
        ));
    }
    if auth_part.is_empty() {
        return Err(OnboardError::InvalidInput(
            "bot token: auth secret (after ':') is empty".to_string(),
        ));
    }
    // The bot id is a positive integer (Telegram's actual
    // bot ids are 8-10 digits, but we don't enforce an
    // upper bound here — only that the part is numeric).
    if !id_part.bytes().all(|b| b.is_ascii_digit()) {
        return Err(OnboardError::InvalidInput(format!(
            "bot token: bot id '{}' must be all digits",
            id_part
        )));
    }
    // The auth half is base64url-ish: per Telegram's
    // @BotFather format spec, the canonical form is
    // EXACTLY 35 characters of `[A-Za-z0-9_-]`.
    //
    // R2-PROTO-3: round 1 accepted any length >= 30 to
    // accommodate shorter test fixtures. That permissiveness
    // is wrong for production: an operator who pastes a
    // truncated token (e.g. a 32-char copy from a log
    // snippet) survives the pre-flight check and reaches
    // grammers, which then returns a 401 with a less
    // actionable error. The fix tightens to exactly 35,
    // matching the canonical @BotFather format. The mock
    // client's `sign_in_bot` already accepts any string,
    // so tests use 35-char auth halves to match production
    // (the round 1 "permissive 30+" exception is removed;
    // a fixture that wants a non-canonical token should
    // bypass `validate_bot_token` in `cfg(test)`, not
    // weaken the production validator).
    if auth_part.len() != 35 {
        return Err(OnboardError::InvalidInput(format!(
            "bot token: auth secret must be exactly 35 chars (got {}); \
             the canonical @BotFather format is <bot_id>:<35 chars of [A-Za-z0-9_-]>",
            auth_part.len()
        )));
    }
    if !auth_part
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(OnboardError::InvalidInput(
            "bot token: auth secret must be [A-Za-z0-9_-]".to_string(),
        ));
    }
    // R2-PROTO-16: reject leading/trailing `_` or `-` —
    // these are not produced by @BotFather and are a sign
    // of an OCR / copy-paste error.
    if let Some(first) = auth_part.bytes().next() {
        if first == b'_' || first == b'-' {
            return Err(OnboardError::InvalidInput(
                "bot token: auth secret must not start with '_' or '-'".to_string(),
            ));
        }
    }
    if let Some(last) = auth_part.bytes().last() {
        if last == b'_' || last == b'-' {
            return Err(OnboardError::InvalidInput(
                "bot token: auth secret must not end with '_' or '-'".to_string(),
            ));
        }
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
/// `OnboardError::Lifecycle` with the last-observed lifecycle
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
        // R2-ARCH-4 / R2-IE-12: use the shared
        // `adapter_error::map` instead of the inline
        // `map_adapter_error` (the round-1 inline copy was
        // duplicated in three places; the central helper is
        // the single source of truth for the
        // `MtprotoTelegramError` → `OnboardError` mapping).
        return Err(adapter_error::map(e, &auth_state_name(&adapter)));
    }

    if !adapter.has_valid_session() {
        return Err(OnboardError::Lifecycle {
            state: auth_state_name(&adapter),
        });
    }

    let identity = adapter
        .self_handle_ref()
        .get()
        .ok_or_else(|| OnboardError::Lifecycle {
            state: auth_state_name(&adapter),
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
    fn validate_bot_token_rejects_only_colon() {
        // IE-1 (R26): old code accepted ":" as containing
        // a colon. The new code rejects it.
        let e = validate_bot_token(":").unwrap_err();
        assert_eq!(e.kind(), "invalid_input");
        assert!(e.to_string().contains("empty") || e.to_string().contains("bot id"));
    }

    #[test]
    fn validate_bot_token_rejects_double_colon() {
        // IE-1 (R26): "::abc" — two colons, empty id_part.
        let e = validate_bot_token("::abc").unwrap_err();
        assert_eq!(e.kind(), "invalid_input");
    }

    #[test]
    fn validate_bot_token_rejects_trailing_colon() {
        // IE-1 (R26): "123:" — empty auth half.
        let e = validate_bot_token("123:").unwrap_err();
        assert_eq!(e.kind(), "invalid_input");
    }

    #[test]
    fn validate_bot_token_rejects_leading_colon() {
        // IE-1 (R26): ":abc" — empty id_part.
        let e = validate_bot_token(":abc").unwrap_err();
        assert_eq!(e.kind(), "invalid_input");
    }

    #[test]
    fn validate_bot_token_rejects_three_colons() {
        // IE-1 (R26): "a:b:c" — too many separators.
        let e = validate_bot_token("a:b:c").unwrap_err();
        assert_eq!(e.kind(), "invalid_input");
        assert!(e.to_string().contains("exactly one"));
    }

    #[test]
    fn validate_bot_token_rejects_non_digit_id() {
        // Bot id must be all digits.
        let e = validate_bot_token("abc:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").unwrap_err();
        assert_eq!(e.kind(), "invalid_input");
    }

    #[test]
    fn validate_bot_token_rejects_short_auth() {
        // 5 chars — far below the canonical 35.
        let e = validate_bot_token("123:short").unwrap_err();
        assert_eq!(e.kind(), "invalid_input");
        assert!(e.to_string().contains("35") || e.to_string().contains("exactly"));
    }

    /// R2-PROTO-3: round 1 accepted any length >= 30 to
    /// accommodate shorter test fixtures. The fix tightens
    /// to exactly 35; this test confirms 30 and 34 are now
    /// both rejected (would have been accepted in round 1).
    #[test]
    fn validate_bot_token_rejects_30_char_auth() {
        // 30 chars — would have been accepted in round 1
        // (the round-1 validator required `>= 30`).
        let e = validate_bot_token("123:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").unwrap_err();
        assert_eq!(e.kind(), "invalid_input");
        assert!(e.to_string().contains("35"));
    }

    #[test]
    fn validate_bot_token_rejects_34_char_auth() {
        // 34 chars — also would have been accepted in round
        // 1 but is NOT canonical @BotFather.
        let e = validate_bot_token("123:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").unwrap_err();
        assert_eq!(e.kind(), "invalid_input");
        assert!(e.to_string().contains("35"));
    }

    /// R2-PROTO-16: leading / trailing `_` and `-` are
    /// not produced by @BotFather. Reject them so an OCR
    /// / copy-paste error doesn't survive pre-flight. (The
    /// auth half must be exactly 35 chars AND not start
    /// with `_`/`-`; the test uses a 35-char string whose
    /// first character is `_`.)
    #[test]
    fn validate_bot_token_rejects_leading_underscore() {
        // 35 chars: `_` + 34 `A`. Length passes; the
        // leading-underscore check fires.
        let e = validate_bot_token("123:_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").unwrap_err();
        assert_eq!(e.kind(), "invalid_input");
        assert!(e.to_string().contains("start"));
    }

    #[test]
    fn validate_bot_token_rejects_trailing_hyphen() {
        // 35 chars: 34 `A` + `-`. Length passes; the
        // trailing-hyphen check fires.
        let e = validate_bot_token("123:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA-").unwrap_err();
        assert_eq!(e.kind(), "invalid_input");
        assert!(e.to_string().contains("end"));
    }

    #[test]
    fn validate_bot_token_rejects_bad_auth_chars() {
        // Auth must be [A-Za-z0-9_-], not e.g. ':' or '!'.
        let e = validate_bot_token("123:AAAAAA!AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").unwrap_err();
        assert_eq!(e.kind(), "invalid_input");
    }

    #[test]
    fn validate_bot_token_accepts_canonical_form() {
        // R2-PROTO-3: the canonical form is exactly 35
        // chars in the auth half.
        validate_bot_token("123456789:AAEhBOweik6ad9JQBxxx_xyz-test-12345").unwrap();
    }

    #[test]
    fn validate_bot_token_accepts_exactly_35_char_auth() {
        // 35 chars exactly — the canonical @BotFather
        // length.
        validate_bot_token("123:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").unwrap();
    }

    #[test]
    fn validate_bot_token_rejects_36_char_auth() {
        // 36 chars — one over the canonical length; would
        // never be produced by @BotFather.
        let e = validate_bot_token("123:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").unwrap_err();
        assert_eq!(e.kind(), "invalid_input");
        assert!(e.to_string().contains("35"));
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
        let (out, _cfg_path) = run(
            adapter,
            "999:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", // 35-char auth half (R2-PROTO-3)
            tmp.path(),
        )
        .await
        .expect("bot-token run should succeed against mock");
        assert!(out.is_bot);
        assert!(out.self_id != 0);
        assert_eq!(out.mode, OnboardMode::BotToken);
        // Session file was written.
        assert!(tmp.path().join("session.json").exists());
    }
}
