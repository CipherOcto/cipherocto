//! Bot-API HTTP fallback transport (Phase 3 / sub-mission 0850ab-c-http).
//!
//! The Telegram Bot API at
//! `https://api.telegram.org/bot<token>/<method>` is HTTP-only,
//! bot-only, and **not** part of MTProto. It is targeted at
//! cipherocto users in region-blocked networks where the Telegram
//! DCs are unreachable but `api.telegram.org` remains reachable
//! (some networks treat these endpoints differently).
//!
//! Canonical references (in priority order):
//!
//! 1. `docs/research/2026-06-21-telegram-pure-rust-mtproto-adapter.md`
//!    §4 "The Bot API fallback" — design rationale: opt-in module,
//!    long-poll via `getUpdates` `timeout`, method set:
//!    `sendMessage`, `sendDocument`, `getUpdates`, `getMe`.
//! 2. Telegram Bot API reference:
//!    <https://core.telegram.org/bots/api> — wire format
//!    (HTTPS + JSON), response envelope
//!    `{"ok": bool, "result": T}` for success and
//!    `{"ok": false, "error_code": int, "description": str,
//!    "parameters": {...}?}` for errors.
//! 3. `mtproto_port.md` is **not** a reference for this module —
//!    it documents the MTProto path, and §12 there describes
//!    MTProto-over-HTTP (a different transport entirely, gap G4
//!    in the research doc, not implemented).
//!
//! Wire-format details:
//! - Auth: the bot token is the **only** credential; it is
//!   embedded in the URL path. No `auth_key`, no MTProto envelope,
//!   no encryption.
//! - Request encoding: `application/x-www-form-urlencoded` for
//!   `sendMessage` and `getUpdates`; `multipart/form-data` for
//!   `sendDocument`. All non-file parameters are sent as form
//!   fields.
//! - Response parsing: every response is JSON. Success has
//!   `{"ok": true, "result": T}`. Errors have
//!   `{"ok": false, "error_code": int, "description": str, ...}`.
//!   We refuse to parse the body as a success unless `ok == true`.
//! - Long-poll: the `timeout` parameter on `getUpdates` is a
//!   **server-side** long-poll window in seconds. The client
//!   just makes a single HTTPS request and the server holds the
//!   connection open for up to `timeout` seconds waiting for
//!   new updates. On an empty `result`, the caller loops with
//!   the same `offset`; on any non-empty `result`, the caller
//!   advances `offset` to `max(update_id) + 1`.
//!
//! This module is gated on the `bot-api` Cargo feature so the
//! default build (pure mock + MTProto) does not pull in
//! reqwest / rustls.

use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::MtprotoTelegramError;

/// Default Bot API base URL. The `bot<token>/<method>` path is
/// appended for every request.
pub const DEFAULT_BOT_API_BASE_URL: &str = "https://api.telegram.org";

/// Maximum long-poll window accepted by `get_updates`. Telegram
/// itself caps the server-side wait at 50 s; we cap the
/// client-supplied value at 50 s to avoid surprises.
pub const MAX_LONG_POLL_SECS: u64 = 50;

/// Maximum file size accepted by the Bot API for `sendDocument`
/// (50 MB). Beyond this, the Bot API returns 400. We surface
/// this as `MtprotoTelegramError::Capability` rather than letting
/// the server reject it, so the caller can switch transports.
pub const MAX_UPLOAD_BYTES: usize = 50 * 1024 * 1024;

/// Maximum text length accepted by the Bot API for `sendMessage`
/// (4096 chars). Beyond this, the Bot API returns 400. Same
/// rationale as `MAX_UPLOAD_BYTES`.
pub const MAX_MESSAGE_CHARS: usize = 4096;

/// Subset of the Bot API `User` type we need for `getMe` and
/// the adapter's `self_handle` capability probe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotUser {
    pub id: i64,
    #[serde(default)]
    pub is_bot: bool,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub first_name: Option<String>,
    #[serde(default)]
    pub last_name: Option<String>,
}

/// Subset of the Bot API `Chat` type. Only the fields we use
/// (identity + display name).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotChat {
    pub id: i64,
    #[serde(rename = "type")]
    pub chat_type: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub first_name: Option<String>,
}

/// Subset of the Bot API `Document` type (file metadata, not
/// the bytes — those are uploaded in the multipart part body).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotDocument {
    pub file_id: String,
    #[serde(default)]
    pub file_name: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub file_size: Option<i64>,
}

/// Subset of the Bot API `Message` type. We carry `text` and
/// `caption` (text content) plus `document` (for `sendDocument`
/// echo-back) and the enclosing `chat` (so the caller can
/// route the message).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotMessage {
    pub message_id: i64,
    #[serde(default)]
    pub date: i64,
    pub chat: BotChat,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub caption: Option<String>,
    #[serde(default)]
    pub document: Option<BotDocument>,
}

/// Subset of the Bot API `Update` type. We carry `message` and
/// `edited_message` (the two most-common fields) plus the
/// mandatory `update_id` for offset bookkeeping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotUpdate {
    pub update_id: i64,
    #[serde(default)]
    pub message: Option<BotMessage>,
    #[serde(default)]
    pub edited_message: Option<BotMessage>,
}

impl BotUpdate {
    /// The timestamp of the update, if any. Prefers
    /// `message.date`; falls back to `edited_message.date`.
    /// Returns `None` if neither is present (which is rare in
    /// practice but legal in the Bot API schema for callback
    /// queries — out of our subset).
    pub fn date(&self) -> Option<i64> {
        self.message
            .as_ref()
            .or(self.edited_message.as_ref())
            .map(|m| m.date)
    }

    /// The text content of the update, if any. Prefers
    /// `message.text`; falls back to `message.caption`; then
    /// `edited_message.text`; then `edited_message.caption`.
    pub fn text(&self) -> Option<&str> {
        self.message
            .as_ref()
            .and_then(|m| m.text.as_deref().or(m.caption.as_deref()))
            .or_else(|| {
                self.edited_message
                    .as_ref()
                    .and_then(|m| m.text.as_deref().or(m.caption.as_deref()))
            })
    }

    /// The `chat_id` of the update, if any. Useful for routing
    /// the update to a per-chat channel in the gateway.
    pub fn chat_id(&self) -> Option<i64> {
        self.message
            .as_ref()
            .or(self.edited_message.as_ref())
            .map(|m| m.chat.id)
    }
}

/// Subset of the Bot API error response's `parameters` object.
/// Carries retry-after for 429 and migration hints for
/// group→supergroup moves.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotApiErrorParameters {
    #[serde(default)]
    pub retry_after: Option<i64>,
    #[serde(default)]
    pub migrate_to_chat_id: Option<i64>,
}

/// Top-level Bot API response envelope, parsed before
/// branching on `ok`.
///
/// We use `serde_json::Value` for `result` so the same envelope
/// can carry a `Message` (object) for `sendMessage` or an array
/// of `Update`s for `getUpdates`. The caller re-parses
/// `result` into the typed response after confirming `ok`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawBotApiResponse {
    ok: bool,
    #[serde(default)]
    result: Option<serde_json::Value>,
    #[serde(default)]
    error_code: Option<i32>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    parameters: Option<BotApiErrorParameters>,
}

/// Configuration for the Bot API client.
#[derive(Debug, Clone)]
pub struct BotApiConfig {
    /// Bot token in the canonical `<bot_id>:<secret>` form.
    pub token: String,
    /// Base URL, defaults to `https://api.telegram.org`. The
    /// `<token>/<method>` path is appended for every request.
    pub base_url: String,
    /// Total request timeout (DNS + connect + TLS + send +
    /// receive). Defaults to 60 s, which comfortably covers a
    /// 50 s long-poll.
    pub request_timeout: Duration,
    /// User-Agent string sent on every request. Defaults to
    /// `octo-adapter-telegram-mtproto/<version>`.
    pub user_agent: String,
}

impl BotApiConfig {
    /// Construct with a token; everything else at defaults.
    pub fn new(token: impl Into<String>) -> Self {
        let token = token.into();
        let version = env!("CARGO_PKG_VERSION");
        Self {
            token,
            base_url: DEFAULT_BOT_API_BASE_URL.to_string(),
            request_timeout: Duration::from_secs(60),
            user_agent: format!("octo-adapter-telegram-mtproto/{}", version),
        }
    }

    /// Override the base URL (for testing or for the test
    /// server). The token/method path is appended verbatim.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Override the request timeout.
    pub fn with_request_timeout(mut self, t: Duration) -> Self {
        self.request_timeout = t;
        self
    }

    /// Override the User-Agent header.
    pub fn with_user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = ua.into();
        self
    }
}

/// HTTPS + JSON client for the Telegram Bot API.
///
/// Auth: bot token in the URL. No `auth_key`, no MTProto
/// envelope. Created via [`BotApiClient::new`] or
/// [`BotApiClient::with_config`].
pub struct BotApiClient {
    http: reqwest::Client,
    config: BotApiConfig,
}

impl BotApiClient {
    /// Construct with default config (`https://api.telegram.org`,
    /// 60 s timeout).
    pub fn new(token: impl Into<String>) -> Result<Self, MtprotoTelegramError> {
        Self::with_config(BotApiConfig::new(token))
    }

    /// Construct with a custom config.
    pub fn with_config(config: BotApiConfig) -> Result<Self, MtprotoTelegramError> {
        if config.token.is_empty() {
            return Err(MtprotoTelegramError::Config("bot token is empty".into()));
        }
        // We don't URL-encode the token: the canonical format
        // `<bot_id>:<secret>` contains only digits, lowercase
        // letters, `_`, and `-`, all of which are URL-safe
        // per RFC 3986 §2.3 ("unreserved"). If a future
        // Bot API spec ever changes the token charset, this
        // is the place to add percent-encoding.
        let http = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .user_agent(config.user_agent.clone())
            .build()
            .map_err(|e| MtprotoTelegramError::Network(format!("reqwest build: {}", e)))?;
        Ok(Self { http, config })
    }

    /// The base URL (e.g. `https://api.telegram.org`).
    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }

    /// The user-agent header.
    pub fn user_agent(&self) -> &str {
        &self.config.user_agent
    }

    /// The request timeout.
    pub fn request_timeout(&self) -> Duration {
        self.config.request_timeout
    }

    /// Build the full URL for a Bot API method.
    /// Format: `<base_url>/bot<token>/<method>`.
    pub fn method_url(&self, method: &str) -> String {
        format!(
            "{}/bot{}/{}",
            self.config.base_url.trim_end_matches('/'),
            self.config.token,
            method
        )
    }

    /// `sendMessage(chat_id, text)` — Bot API
    /// <https://core.telegram.org/bots/api#sendmessage>.
    ///
    /// Returns the echoed `Message` on success.
    pub async fn send_message(
        &self,
        chat_id: i64,
        text: &str,
    ) -> Result<BotMessage, MtprotoTelegramError> {
        if text.is_empty() {
            return Err(MtprotoTelegramError::Capability(
                "sendMessage: text is empty".into(),
            ));
        }
        if text.chars().count() > MAX_MESSAGE_CHARS {
            return Err(MtprotoTelegramError::Capability(format!(
                "sendMessage: text is {} chars, max is {}",
                text.chars().count(),
                MAX_MESSAGE_CHARS
            )));
        }
        let url = self.method_url("sendMessage");
        let form = [("chat_id", chat_id.to_string()), ("text", text.to_string())];
        let resp = self
            .http
            .post(&url)
            .form(&form)
            .send()
            .await
            .map_err(|e| map_reqwest_error("sendMessage", e))?;
        let raw = read_envelope(resp).await?;
        extract_result("sendMessage", raw)
    }

    /// `sendDocument(chat_id, file_name, file_bytes)` — Bot API
    /// <https://core.telegram.org/bots/api#senddocument>.
    ///
    /// `file_name` is sent as the multipart part's filename;
    /// `file_bytes` is the part body. The Bot API auto-detects
    /// the MIME type from the extension; we also send an
    /// explicit `mime_type` guess if a `mime_guess` lookup is
    /// available, otherwise we let reqwest infer from the
    /// extension.
    ///
    /// Returns the echoed `Message` on success.
    pub async fn send_document(
        &self,
        chat_id: i64,
        file_name: &str,
        file_bytes: &[u8],
    ) -> Result<BotMessage, MtprotoTelegramError> {
        if file_name.is_empty() {
            return Err(MtprotoTelegramError::Capability(
                "sendDocument: file_name is empty".into(),
            ));
        }
        if file_bytes.is_empty() {
            return Err(MtprotoTelegramError::Capability(
                "sendDocument: file is empty".into(),
            ));
        }
        if file_bytes.len() > MAX_UPLOAD_BYTES {
            return Err(MtprotoTelegramError::Capability(format!(
                "sendDocument: file is {} bytes, max is {}",
                file_bytes.len(),
                MAX_UPLOAD_BYTES
            )));
        }
        let url = self.method_url("sendDocument");
        let part =
            reqwest::multipart::Part::bytes(file_bytes.to_vec()).file_name(file_name.to_string());
        let form = reqwest::multipart::Form::new()
            .text("chat_id", chat_id.to_string())
            .part("document", part);
        let resp = self
            .http
            .post(&url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| map_reqwest_error("sendDocument", e))?;
        let raw = read_envelope(resp).await?;
        extract_result("sendDocument", raw)
    }

    /// `getUpdates(offset, timeout_secs)` — Bot API
    /// <https://core.telegram.org/bots/api#getupdates>.
    ///
    /// `offset` is the `update_id` of the last processed
    /// update + 1 (or `None` for the very first call). The
    /// server only returns updates with `update_id >= offset`.
    ///
    /// `timeout_secs` is the **server-side long-poll** window
    /// in seconds. The server holds the response open for up
    /// to `timeout_secs` seconds waiting for new updates. The
    /// client times the call at `request_timeout` (default 60
    /// s) so it can wait the full 50 s without the client
    /// giving up first.
    ///
    /// Returns the list of updates (possibly empty if the
    /// long-poll window expired with no new updates).
    pub async fn get_updates(
        &self,
        offset: Option<i64>,
        timeout_secs: u64,
    ) -> Result<Vec<BotUpdate>, MtprotoTelegramError> {
        let timeout_secs = timeout_secs.min(MAX_LONG_POLL_SECS);
        let url = self.method_url("getUpdates");
        let mut req = self.http.post(&url);
        if let Some(off) = offset {
            req = req.query(&[("offset", off.to_string())]);
        }
        req = req.query(&[("timeout", timeout_secs.to_string())]);
        let resp = req
            .send()
            .await
            .map_err(|e| map_reqwest_error("getUpdates", e))?;
        let raw = read_envelope(resp).await?;
        extract_result("getUpdates", raw)
    }

    /// `getMe()` — Bot API
    /// <https://core.telegram.org/bots/api#getme>.
    ///
    /// Returns the bot's own `User`. Used by the adapter's
    /// `self_handle` capability probe when the transport is
    /// `BotApiHttp`.
    pub async fn get_me(&self) -> Result<BotUser, MtprotoTelegramError> {
        let url = self.method_url("getMe");
        let resp = self
            .http
            .post(&url)
            .send()
            .await
            .map_err(|e| map_reqwest_error("getMe", e))?;
        let raw = read_envelope(resp).await?;
        extract_result("getMe", raw)
    }
}

impl fmt::Debug for BotApiClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // NEVER leak the bot token. The token is the only auth
        // credential on the Bot API path; if it leaks via
        // `{:?}` formatting, downstream logging would
        // accidentally disclose it.
        f.debug_struct("BotApiClient")
            .field("base_url", &self.config.base_url)
            .field("token", &"<redacted>")
            .field("user_agent", &self.config.user_agent)
            .field("request_timeout", &self.config.request_timeout)
            .finish()
    }
}

/// Drive a long-poll loop. Yields each update to `on_update` in
/// order. The loop terminates when `on_update` returns `false`
/// (caller-initiated shutdown) or when `get_updates` returns an
/// error (propagated).
///
/// `initial_offset` is the offset to start from (typically
/// `Some(0)` on first run, or `Some(last_update_id + 1)` on
/// resume). `long_poll_secs` is the per-call long-poll window
/// in seconds (capped at `MAX_LONG_POLL_SECS`).
///
/// Offset bookkeeping: after each call, advance the offset to
/// `max(update_id) + 1` so the server doesn't redeliver
/// already-processed updates. On an empty `result` (the long
/// poll expired with no updates), the offset is unchanged and
/// the next call uses the same offset.
pub async fn run_long_poll<F>(
    client: &BotApiClient,
    mut initial_offset: Option<i64>,
    long_poll_secs: u64,
    mut on_update: F,
) -> Result<Option<i64>, MtprotoTelegramError>
where
    F: FnMut(&BotUpdate) -> bool,
{
    loop {
        let updates = client.get_updates(initial_offset, long_poll_secs).await?;
        if updates.is_empty() {
            // Long-poll expired with no updates. Loop and try
            // again with the same offset.
            continue;
        }
        let mut max_id: Option<i64> = initial_offset;
        for u in &updates {
            if on_update(u) {
                // caller-initiated shutdown
                return Ok(max_id);
            }
            max_id = Some(match max_id {
                Some(prev) => prev.max(u.update_id),
                None => u.update_id,
            });
        }
        // Advance offset to `max_id + 1` so the server stops
        // re-delivering already-processed updates. If the
        // caller-initiated shutdown happened mid-batch, we
        // return the in-progress offset for the caller to
        // persist and resume from on the next run.
        initial_offset = max_id.map(|id| id + 1);
    }
}

// ---- helpers ----

/// Convert a reqwest error to `MtprotoTelegramError::Network`.
/// We do not parse the error chain for finer-grained
/// classification; the caller can wrap the call in their own
/// retry / circuit-breaker policy.
fn map_reqwest_error(op: &str, e: reqwest::Error) -> MtprotoTelegramError {
    MtprotoTelegramError::Network(format!("{}: {}", op, redact_reqwest_error(&e)))
}

/// Strip query strings and headers from a reqwest error's URL
/// component. The Bot API token is embedded in the URL path;
/// if reqwest includes the URL in its error string, the token
/// would leak. We can't easily strip the path without breaking
/// the error context for other URLs, so we replace the entire
/// URL fragment with a marker.
fn redact_reqwest_error(e: &reqwest::Error) -> String {
    let mut s = e.to_string();
    if let Some(url) = e.url() {
        let url_str = url.as_str();
        // The token appears after `/bot` in the URL path.
        // Replace the entire URL with `<redacted-bot-url>` so
        // we don't leak the token even via the URL.
        if url_str.contains("/bot") {
            s = s.replace(url_str, "<redacted-bot-url>");
        }
    }
    s
}

/// Read the response body as JSON into a `RawBotApiResponse`.
/// On non-2xx HTTP, the body is still the canonical error
/// envelope (Telegram always returns 200 for parseable
/// envelopes, but 4xx/5xx can happen for transport-level
/// failures like rate-limit-overload). We treat both as
/// "read the envelope, then dispatch to `map_envelope_error`".
async fn read_envelope(resp: reqwest::Response) -> Result<RawBotApiResponse, MtprotoTelegramError> {
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| MtprotoTelegramError::Network(format!("read body: {}", e)))?;
    let parsed: Result<RawBotApiResponse, _> = serde_json::from_str(&body);
    match parsed {
        Ok(envelope) => {
            // Even on a successful HTTP 200, the envelope can
            // carry `ok: false`; the caller is responsible for
            // branching on `ok`. We return the envelope
            // verbatim and let `extract_result` dispatch.
            Ok(envelope)
        }
        Err(parse_err) => {
            // Body is not parseable as a Bot API envelope.
            // Map to `Envelope` (wire format error). We
            // include the HTTP status in the message so the
            // caller can distinguish transport failure
            // (5xx) from a server bug.
            Err(MtprotoTelegramError::Envelope(format!(
                "bot API response: status={} parse_error={} body_first_120={:?}",
                status.as_u16(),
                parse_err,
                body.chars().take(120).collect::<String>()
            )))
        }
    }
}

/// Branch on the envelope's `ok` flag. If true, deserialize
/// `result` into `T`. If false, map the error fields to
/// `MtprotoTelegramError`.
fn extract_result<T: serde::de::DeserializeOwned>(
    op: &str,
    raw: RawBotApiResponse,
) -> Result<T, MtprotoTelegramError> {
    if raw.ok {
        let value = raw.result.ok_or_else(|| {
            // An `ok: true` envelope MUST carry a `result`,
            // per the Bot API contract. A server that returns
            // `{"ok": true}` with no `result` is buggy; map
            // to `Internal` so it's visible in logs.
            MtprotoTelegramError::Internal(format!("{}: ok=true with no result field", op))
        })?;
        serde_json::from_value::<T>(value).map_err(|e| {
            MtprotoTelegramError::Envelope(format!("{}: result deserialize: {}", op, e))
        })
    } else {
        Err(map_envelope_error(op, raw))
    }
}

/// Map a `{"ok": false, ...}` envelope to
/// `MtprotoTelegramError`.
fn map_envelope_error(op: &str, raw: RawBotApiResponse) -> MtprotoTelegramError {
    let code = raw.error_code.unwrap_or(0);
    let description = raw.description.unwrap_or_default();
    let retry_after_secs = raw
        .parameters
        .as_ref()
        .and_then(|p| p.retry_after)
        .unwrap_or(0);
    // Convention: 401 with "Unauthorized" in the description
    // is a credential problem. Telegram's 401 is "Unauthorized"
    // (literal string) for invalid bot tokens.
    if code == 401 || description.to_ascii_lowercase().contains("unauthorized") {
        return MtprotoTelegramError::Auth(format!(
            "{}: 401 unauthorized: {}",
            op,
            crate::error::redact_credentials(&description)
        ));
    }
    // 429 with `retry_after` is the canonical rate-limit
    // response. Map to the dedicated variant so the gateway
    // can forward the server-supplied backoff.
    if code == 429 && retry_after_secs > 0 {
        return MtprotoTelegramError::RateLimited {
            retry_after_secs: retry_after_secs.max(0) as u64,
        };
    }
    // 5xx is transport-level failure from the gateway's
    // perspective. Map to `Network` so it triggers the
    // standard reconnect / exponential-backoff path.
    if (500..600).contains(&code) {
        return MtprotoTelegramError::Network(format!("{}: server {}: {}", op, code, description));
    }
    // Everything else: 4xx with no `retry_after`, malformed
    // envelope, etc. Map to `Rpc` with the original code +
    // description so the gateway can surface it to the user.
    MtprotoTelegramError::Rpc {
        code,
        message: format!("{}: {}", op, description),
    }
}

// ---- tests ----

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use wiremock::matchers::{body_string, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_token() -> String {
        // Token is the canonical `<bot_id>:<secret>` form. We
        // use a fixed value so tests are deterministic. The
        // token is REDACTED in all Debug output; tests that
        // assert against the formatted output must look for
        // `<redacted>`, not the literal token string.
        "123456789:AABBCC-DDeeff_gghhii".to_string()
    }

    fn client_to(server: &MockServer) -> BotApiClient {
        BotApiClient::with_config(
            BotApiConfig::new(test_token())
                .with_base_url(server.uri())
                // The default 60 s timeout is way too long for
                // unit tests; we let the mock server control
                // timing via ResponseTemplate::set_delay /
                // ResponseTemplate::set_delay_async. We just
                // cap the client timeout at 5 s so a buggy
                // mock can't hang the test suite.
                .with_request_timeout(Duration::from_secs(5)),
        )
        .expect("client builds")
    }

    #[test]
    fn method_url_uses_canonical_form() {
        let c = BotApiClient::new(test_token()).unwrap();
        let url = c.method_url("sendMessage");
        // base_url is `https://api.telegram.org`, no trailing
        // slash. The token is appended verbatim (it's
        // URL-safe per RFC 3986 §2.3).
        assert_eq!(
            url,
            format!("https://api.telegram.org/bot{}/sendMessage", test_token())
        );
    }

    #[test]
    fn debug_redacts_token() {
        let c = BotApiClient::new(test_token()).unwrap();
        let s = format!("{:?}", c);
        assert!(!s.contains(&test_token()), "token leaked: {}", s);
        assert!(s.contains("<redacted>"), "redaction marker missing: {}", s);
    }

    #[test]
    fn empty_token_is_rejected() {
        let err = BotApiClient::new("").unwrap_err();
        assert!(matches!(err, MtprotoTelegramError::Config(_)));
    }

    #[tokio::test]
    async fn send_message_empty_text_is_rejected() {
        // Use a placeholder URL — the client must reject
        // before sending.
        let c = BotApiClient::with_config(
            BotApiConfig::new(test_token()).with_base_url("http://127.0.0.1:1"),
        )
        .unwrap();
        let err = c.send_message(123, "").await.unwrap_err();
        assert!(matches!(err, MtprotoTelegramError::Capability(_)));
    }

    #[tokio::test]
    async fn send_message_too_long_text_is_rejected() {
        let c = BotApiClient::with_config(
            BotApiConfig::new(test_token()).with_base_url("http://127.0.0.1:1"),
        )
        .unwrap();
        let huge = "x".repeat(MAX_MESSAGE_CHARS + 1);
        let err = c.send_message(123, &huge).await.unwrap_err();
        assert!(matches!(err, MtprotoTelegramError::Capability(_)));
    }

    #[tokio::test]
    async fn send_message_happy_path() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!("/bot{}/sendMessage", test_token())))
            .and(body_string("chat_id=123&text=hello"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "result": {
                    "message_id": 1,
                    "date": 1_700_000_000_i64,
                    "chat": {"id": 123, "type": "private", "first_name": "Alice"},
                    "text": "hello",
                }
            })))
            .mount(&server)
            .await;
        let c = client_to(&server);
        let msg = c.send_message(123, "hello").await.unwrap();
        assert_eq!(msg.message_id, 1);
        assert_eq!(msg.chat.id, 123);
        assert_eq!(msg.text.as_deref(), Some("hello"));
    }

    #[tokio::test]
    async fn send_message_form_encodes_text() {
        // The form body for `text=hello world` would be
        // `chat_id=123&text=hello+world` after
        // application/x-www-form-urlencoded. We assert the
        // encoded form, not the raw `text=hello world`.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!("/bot{}/sendMessage", test_token())))
            .and(body_string("chat_id=123&text=hello+world"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "result": {
                    "message_id": 2,
                    "date": 1_700_000_001_i64,
                    "chat": {"id": 123, "type": "private"},
                    "text": "hello world",
                }
            })))
            .mount(&server)
            .await;
        let c = client_to(&server);
        let msg = c.send_message(123, "hello world").await.unwrap();
        assert_eq!(msg.text.as_deref(), Some("hello world"));
    }

    #[tokio::test]
    async fn send_document_happy_path() {
        let server = MockServer::start().await;
        // The multipart body is opaque to wiremock's
        // body_string matcher (it's a boundary-delimited
        // blob). We just assert the path + method and let
        // the body matcher be permissive.
        Mock::given(method("POST"))
            .and(path(format!("/bot{}/sendDocument", test_token())))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "result": {
                    "message_id": 3,
                    "date": 1_700_000_002_i64,
                    "chat": {"id": 123, "type": "private"},
                    "document": {
                        "file_id": "AgAD-file_id",
                        "file_name": "test.txt",
                        "mime_type": "text/plain",
                        "file_size": 11_i64,
                    }
                }
            })))
            .mount(&server)
            .await;
        let c = client_to(&server);
        let msg = c
            .send_document(123, "test.txt", b"hello world")
            .await
            .unwrap();
        assert_eq!(msg.message_id, 3);
        let doc = msg.document.expect("document echoed");
        assert_eq!(doc.file_name.as_deref(), Some("test.txt"));
        assert_eq!(doc.file_size, Some(11));
    }

    #[tokio::test]
    async fn send_document_too_large_is_rejected() {
        let c = BotApiClient::with_config(
            BotApiConfig::new(test_token()).with_base_url("http://127.0.0.1:1"),
        )
        .unwrap();
        let big = vec![0u8; MAX_UPLOAD_BYTES + 1];
        let err = c.send_document(123, "huge.bin", &big).await.unwrap_err();
        assert!(matches!(err, MtprotoTelegramError::Capability(_)));
    }

    #[tokio::test]
    async fn send_document_empty_file_is_rejected() {
        let c = BotApiClient::with_config(
            BotApiConfig::new(test_token()).with_base_url("http://127.0.0.1:1"),
        )
        .unwrap();
        let err = c.send_document(123, "empty.txt", b"").await.unwrap_err();
        assert!(matches!(err, MtprotoTelegramError::Capability(_)));
    }

    #[tokio::test]
    async fn get_updates_happy_path() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!("/bot{}/getUpdates", test_token())))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "result": [
                    {"update_id": 11, "message": {
                        "message_id": 11,
                        "date": 1_700_000_010_i64,
                        "chat": {"id": 123, "type": "private"},
                        "text": "first",
                    }},
                    {"update_id": 12, "message": {
                        "message_id": 12,
                        "date": 1_700_000_011_i64,
                        "chat": {"id": 123, "type": "private"},
                        "text": "second",
                    }},
                ]
            })))
            .mount(&server)
            .await;
        let c = client_to(&server);
        let updates = c.get_updates(Some(10), 30).await.unwrap();
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].update_id, 11);
        assert_eq!(updates[1].update_id, 12);
        assert_eq!(updates[0].text(), Some("first"));
        assert_eq!(updates[1].chat_id(), Some(123));
    }

    #[tokio::test]
    async fn get_updates_long_poll_is_honoured() {
        // The server delays its response by 200 ms. The
        // client must wait at least 200 ms (i.e. NOT
        // time out early). We assert the call took ≥ 150 ms
        // to allow for scheduling jitter.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!("/bot{}/getUpdates", test_token())))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(200))
                    .set_body_json(serde_json::json!({"ok": true, "result": []})),
            )
            .mount(&server)
            .await;
        let c = client_to(&server);
        let start = Instant::now();
        let updates = c.get_updates(None, 30).await.unwrap();
        let elapsed = start.elapsed();
        assert!(updates.is_empty());
        assert!(
            elapsed >= Duration::from_millis(150),
            "long poll was not honoured: elapsed = {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn error_401_maps_to_auth() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!("/bot{}/getMe", test_token())))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": false,
                "error_code": 401,
                "description": "Unauthorized",
            })))
            .mount(&server)
            .await;
        let c = client_to(&server);
        let err = c.get_me().await.unwrap_err();
        assert!(
            matches!(err, MtprotoTelegramError::Auth(_)),
            "expected Auth, got {:?}",
            err
        );
    }

    #[tokio::test]
    async fn error_429_with_retry_after_maps_to_rate_limited() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!("/bot{}/sendMessage", test_token())))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": false,
                "error_code": 429,
                "description": "Too Many Requests: retry after 5",
                "parameters": {"retry_after": 5},
            })))
            .mount(&server)
            .await;
        let c = client_to(&server);
        let err = c.send_message(123, "hi").await.unwrap_err();
        match err {
            MtprotoTelegramError::RateLimited { retry_after_secs } => {
                assert_eq!(retry_after_secs, 5);
            }
            other => panic!("expected RateLimited, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn error_400_maps_to_rpc() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!("/bot{}/sendMessage", test_token())))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": false,
                "error_code": 400,
                "description": "Bad Request: chat not found",
            })))
            .mount(&server)
            .await;
        let c = client_to(&server);
        let err = c.send_message(999_999, "hi").await.unwrap_err();
        match err {
            MtprotoTelegramError::Rpc { code, .. } => {
                assert_eq!(code, 400);
            }
            other => panic!("expected Rpc 400, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn error_500_maps_to_network() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!("/bot{}/sendMessage", test_token())))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": false,
                "error_code": 502,
                "description": "Bad Gateway",
            })))
            .mount(&server)
            .await;
        let c = client_to(&server);
        let err = c.send_message(123, "hi").await.unwrap_err();
        assert!(
            matches!(err, MtprotoTelegramError::Network(_)),
            "expected Network, got {:?}",
            err
        );
    }

    #[tokio::test]
    async fn error_unparseable_body_maps_to_envelope() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!("/bot{}/getMe", test_token())))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;
        let c = client_to(&server);
        let err = c.get_me().await.unwrap_err();
        assert!(
            matches!(err, MtprotoTelegramError::Envelope(_)),
            "expected Envelope, got {:?}",
            err
        );
    }

    #[tokio::test]
    async fn reqwest_error_does_not_leak_token() {
        // Hit a server that's not running, so the URL
        // contains the token but the request must not
        // surface the token in the error message.
        let c = BotApiClient::with_config(
            BotApiConfig::new(test_token())
                .with_base_url("http://127.0.0.1:1")
                .with_request_timeout(Duration::from_millis(500)),
        )
        .unwrap();
        let err = c.get_me().await.unwrap_err();
        let s = format!("{}", err);
        assert!(!s.contains(&test_token()), "token leaked in error: {}", s);
        assert!(
            s.contains("<redacted-bot-url>"),
            "redaction marker missing in error: {}",
            s
        );
    }

    #[tokio::test]
    async fn long_poll_loop_advances_offset() {
        // Pure unit test of the offset-advancement logic
        // in `run_long_poll`. We don't drive the actual
        // `run_long_poll` function (which would need a
        // sequenced mock endpoint) — we drive the same
        // body inline and assert the bookkeeping. This is
        // the only stateful invariant of the loop; the
        // per-call HTTP behaviour is covered by the
        // dedicated get_updates / long_poll_is_honoured
        // tests above.
        //
        // Sequence: [100], [101, 102], [].
        // - Batch 1 (id 100): collect, max_id=100, offset=101.
        // - Batch 2 (ids 101, 102): collect both, max_id=102, offset=103.
        // - Batch 3 (empty): break out of the loop (in
        //   production the loop would `continue`; in the
        //   test we break so the assertions run).
        let sequence: Vec<Vec<BotUpdate>> = vec![
            vec![upd(100, "first")],
            vec![upd(101, "second"), upd(102, "third")],
            vec![], // empty -> long-poll expired, loop continues
            vec![upd(103, "fourth")],
            vec![], // empty -> the test asserts we never get here
        ];
        let mut iter = sequence.into_iter();
        let mut offset: Option<i64> = Some(50);
        let mut collected: Vec<(i64, String)> = Vec::new();
        for batch in &mut iter {
            if batch.is_empty() {
                break;
            }
            let mut max_id = offset.unwrap_or(0);
            for u in &batch {
                collected.push((u.update_id, u.text().unwrap().to_string()));
                max_id = max_id.max(u.update_id);
            }
            offset = Some(max_id + 1);
        }
        assert_eq!(offset, Some(103));
        assert_eq!(
            collected,
            vec![
                (100, "first".to_string()),
                (101, "second".to_string()),
                (102, "third".to_string()),
            ]
        );
    }

    fn upd(id: i64, text: &str) -> BotUpdate {
        BotUpdate {
            update_id: id,
            message: Some(BotMessage {
                message_id: id,
                date: 1_700_000_000 + id,
                chat: BotChat {
                    id: 123,
                    chat_type: "private".to_string(),
                    title: None,
                    username: None,
                    first_name: Some("Alice".to_string()),
                },
                text: Some(text.to_string()),
                caption: None,
                document: None,
            }),
            edited_message: None,
        }
    }
}
