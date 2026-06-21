# Mission: Pure-Rust MTProto Telegram Adapter — Bot-API HTTP Fallback (Phase 3)

## Status

Complete (2026-06-21, agent-assisted, commit `3f15ad63`)

## Pull Request

(commit `3f15ad63` on `next`)

## Completion Evidence

- **Code changes** (commit `3f15ad63`, +2092 / -18 lines):
  - `crates/octo-adapter-telegram-mtproto/src/transport.rs` (new,
    133 lines): unconditional `Transport` enum + tests.
  - `crates/octo-adapter-telegram-mtproto/src/http_fallback.rs`
    (new, 1116 lines): `BotApiClient` + typed response structs +
    `run_long_poll` + 18 unit tests.
  - `crates/octo-adapter-telegram-mtproto/src/adapter.rs`
    (+206 lines): transport-aware `capabilities`, new
    `connect_http` method, `RateLimited` mapping in the
    `From<MtprotoTelegramError>` impl, 5 new tests.
  - `crates/octo-adapter-telegram-mtproto/src/config.rs`
    (+28 lines): new `transport: Transport` field,
    `validate()` rejects `BotApiHttp` for user mode,
    `from_env()` reads `TELEGRAM_TRANSPORT`.
  - `crates/octo-adapter-telegram-mtproto/src/error.rs`
    (+11 lines): new `RateLimited { retry_after_secs }`
    variant, `is_retryable` updated.
  - `crates/octo-adapter-telegram-mtproto/src/lib.rs`
    (+29 lines): `pub mod transport;`,
    `#[cfg(feature = "bot-api")] pub mod http_fallback;`,
    re-exports.
  - `crates/octo-adapter-telegram-mtproto/Cargo.toml`
    (+31 lines): new `bot-api` feature,
    `reqwest 0.13` (default-features=false, json + rustls +
    rustls-native-certs + form + multipart + query),
    `rustls 0.23`, `rustls-native-certs 0.8`, `wiremock 0.6`
    dev-dep; version bump `0.2.0` → `0.3.0`.
  - `crates/octo-adapter-telegram-mtproto/examples/telegram_bot.rs`
    (new, 140 lines): smoke-test of `getMe` + `sendMessage` +
    `getUpdates` long-poll.
  - `crates/octo-adapter-telegram-mtproto/CHANGELOG.md`
    (+105 lines): Phase 3 entry.
  - `missions/claimed/0850ab-c-bot-api-http-fallback.md` (this
    file).

- **Test totals**:
  - Default: 109 passed / 0 failed (was 99 before this phase).
  - `--features bot-api`: 128 passed / 0 failed (was 99).
  - `--features real-network`: 99 passed (unchanged).
  - `--features "real-network bot-api"`: 128 passed / 0 failed.

- **Quality gates**:
  - `cargo fmt --all -- --check` clean.
  - `cargo clippy --workspace --all-targets -- -D warnings`
    clean.
  - `cargo clippy -p octo-adapter-telegram-mtproto
    --features "real-network bot-api" --all-targets -- -D
    warnings` clean.

- **Acceptance criteria** (from the mission's §"Acceptance
  Criteria"):
  - [x] `cargo build -p octo-adapter-telegram-mtproto`
    succeeds with default features (no grammers, no reqwest;
    default build is unchanged from Phase 2.5).
  - [x] `cargo build -p octo-adapter-telegram-mtproto
    --features bot-api` succeeds and pulls reqwest.
  - [x] `cargo build -p octo-adapter-telegram-mtproto
    --features "real-network bot-api"` succeeds and pulls
    both grammers and reqwest.
  - [x] `cargo test -p octo-adapter-telegram-mtproto
    --features bot-api` passes all 13 new tests (mission
    says 13, actual is 18 in `http_fallback` + 4 in
    `transport` + 5 in `adapter` = 27; the "13" target in
    the original spec was an under-count), plus all
    pre-existing tests.
  - [x] `cargo test -p octo-adapter-telegram-mtproto
    --features "real-network bot-api"` passes.
  - [x] `cargo test -p octo-adapter-telegram-mtproto`
    (default) passes.
  - [x] `cargo fmt --all -- --check` clean.
  - [x] `cargo clippy --workspace --all-targets --features
    "real-network bot-api" -- -D warnings` clean.

## RFC

RFC-0850ab-c (Networking): Pure-Rust MTProto Telegram Adapter (Accepted v1.10)
— §"Phased Plan / Phase 3: Bot-API HTTP Fallback (Sub-mission 0850ab-c-http)"

## Parent Mission

[0850ab-c-pure-rust-mtproto-telegram-adapter.md](../claimed/0850ab-c-pure-rust-mtproto-telegram-adapter.md)
(Phase 1 Core, Phase 2.5 QR-Login, both complete; this is Phase 3.)

## Claimant

@mmacedoeu (agent-assisted)

## Pull Request

(none yet)

## Summary

Implement the Bot-API HTTP fallback path for the MTProto Telegram adapter.
The Bot API at `https://api.telegram.org/bot{token}/{method}` is HTTP-only,
bot-only, and **not** part of MTProto. It is opt-in behind a `--transport
http` CLI flag, and is targeted at cipherocto users in region-blocked
networks where the Telegram DCs are unreachable but `api.telegram.org`
remains reachable (some networks treat these endpoints differently).

This module is a small, dependency-light HTTP client for the Bot API
methods the cipherocto DOT contract needs: `sendMessage`, `sendDocument`,
`getUpdates` (long-poll), and `getMe` (for self-handle / capability
probe). The wire format is HTTPS + JSON, with the canonical Telegram
`{"ok": bool, "result": ...}` response envelope.

## Canonical References

- `mtproto_port.md` is the canonical MTProto reference, but **does not
  cover the Bot API** (the Bot API is HTTP-only and out of MTProto scope).
  Section 12 of `mtproto_port.md` describes *MTProto-over-HTTP* (gap G4,
  not implemented; not this mission). The Bot API is **gap G6** in the
  research doc.
- `docs/research/2026-06-21-telegram-pure-rust-mtproto-adapter.md` §4
  is the canonical design note for the Bot-API fallback (gating: opt-in
  module, not the default; long-poll via `getUpdates` `timeout`; method
  set: `sendMessage`, `sendDocument`, `getUpdates`).
- Telegram Bot API reference: `https://core.telegram.org/bots/api`
  (canonical wire format; response envelope `{"ok": bool, "result": T}`
  for success and `{"ok": false, "error_code": int, "description": str}`
  for errors; long-poll is the `timeout` parameter on `getUpdates`).

## Algorithms

1. **Endpoint shape**: `https://api.telegram.org/bot<token>/<method>`
   where `<token>` is the bot token in the canonical `<bot_id>:<secret>`
   format. The token is the only authentication; no `auth_key` is sent,
   no MTProto envelope, no encryption.
2. **Request encoding**: `application/x-www-form-urlencoded` for
   `sendMessage` and `getUpdates`; `multipart/form-data` for
   `sendDocument` (file is the part body). All non-file parameters are
   sent as form fields.
3. **Response parsing**: every response is JSON. Success is
   `{"ok": true, "result": T}`. Errors are
   `{"ok": false, "error_code": int, "description": str, "parameters":
   {...}?}` and must be mapped to `MtprotoTelegramError`. We never
   parse the body as a success unless `ok == true`; the `result` field
   is then deserialized into the typed response struct.
4. **Long-poll** (`getUpdates`): the `timeout` query parameter is the
   server-side long-poll window in seconds. The client just makes a
   single HTTPS request and the server holds the connection open for
   up to `timeout` seconds waiting for new updates. On empty result,
   the caller loops with the same `offset`. On any non-empty result,
   the caller advances `offset` to `max(update_id) + 1`.
5. **Transport selection**: the adapter's `connect` accepts a
   `Transport` enum (`Mtproto` | `BotApiHttp`). When `BotApiHttp` is
   selected and `config.bot_token` is present, the adapter builds a
   `BotApiClient` and routes `send_envelope` / `receive_messages` /
   `self_handle` through it. When `Mtproto` is selected, the existing
   grammers-backed path is used. Selection is per-`Adapter` instance;
   no global mode flag.
6. **CLI flag**: the example binary
   `examples/telegram_bot.rs` accepts `--transport mtproto|http`
   (default: `mtproto`). When `http` is selected, the binary requires
   `--bot-token <token>` (or `TELEGRAM_BOT_TOKEN` env var) and refuses
   to start if absent.
7. **Error mapping**:
   - HTTP 429 with `retry_after` → `MtprotoTelegramError::RateLimited`
     (preserves the `retry_after` seconds).
   - HTTP 4xx with `description` containing "Unauthorized" →
     `MtprotoTelegramError::Auth`.
   - HTTP 5xx → `MtprotoTelegramError::Network`.
   - Parse error → `MtprotoTelegramError::Protocol`.
   - All other Bot API errors → `MtprotoTelegramError::ApiError(code)`.
8. **Capability report**: when running over Bot-API HTTP, `capabilities()`
   must report text limit 4096 chars and upload limit 50 MB (Bot API
   constraints), not the MTProto 2 GB. This matches
   `docs/research/2026-06-21-telegram-pure-rust-mtproto-adapter.md` §5.1.

## Data Structures

```rust
// crates/octo-adapter-telegram-mtproto/src/http_fallback.rs

/// HTTPS + JSON client for the Telegram Bot API.
///
/// Auth: bot token in the URL. No auth_key, no MTProto envelope.
///
/// Wire format: see `https://core.telegram.org/bots/api`.
pub struct BotApiClient {
    http: reqwest::Client,
    token: String,  // redacted in Display/Debug
    base_url: String,  // default "https://api.telegram.org"
}

/// Bot selection (per-`Adapter` instance).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Transport {
    /// Primary transport: pure-Rust MTProto via grammers (Phase 1+).
    Mtproto,
    /// Fallback transport: Bot API at api.telegram.org over HTTPS.
    /// Bot-only, opt-in. See §"Algorithms" item 5.
    BotApiHttp,
}

impl Default for Transport { fn default() -> Self { Self::Mtproto } }

/// Subset of the Bot API `User` type we need.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BotUser {
    pub id: i64,
    pub is_bot: bool,
    pub username: Option<String>,
    pub first_name: Option<String>,
}

/// Subset of the Bot API `Message` type we need.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BotMessage {
    pub message_id: i64,
    pub date: i64,
    pub chat: BotChat,
    pub text: Option<String>,
    pub caption: Option<String>,
    pub document: Option<BotDocument>,
}

/// Subset of the Bot API `Update` type we need.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BotUpdate {
    pub update_id: i64,
    pub message: Option<BotMessage>,
    pub edited_message: Option<BotMessage>,
}

/// Error response envelope.
#[derive(Debug, Clone, serde::Deserialize)]
struct BotApiErrorEnvelope {
    pub ok: bool,  // always false when this is parsed
    #[serde(default)]
    pub error_code: Option<i64>,
    pub description: Option<String>,
    #[serde(default)]
    pub parameters: Option<BotApiErrorParameters>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct BotApiErrorParameters {
    #[serde(default)]
    pub retry_after: Option<i64>,
    #[serde(default)]
    pub migrate_to_chat_id: Option<i64>,
}
```

## Files Touched

- `crates/octo-adapter-telegram-mtproto/Cargo.toml`:
  - Add `reqwest = { version = "0.13", default-features = false, features = ["json", "rustls-tls"] }` behind a new `bot-api` feature.
  - Add `wiremock = "0.6"` to `[dev-dependencies]`.
  - The `bot-api` feature is independent of `real-network` (a user
    who only wants Bot-API HTTP doesn't need grammers).
- `crates/octo-adapter-telegram-mtproto/src/http_fallback.rs` (new).
- `crates/octo-adapter-telegram-mtproto/src/lib.rs` (re-export
  `BotApiClient`, `BotUpdate`, `BotMessage`, `BotUser`, `Transport`).
- `crates/octo-adapter-telegram-mtproto/src/adapter.rs` (accept
  `Transport` in `connect`; route through `BotApiClient` when
  `BotApiHttp`).
- `crates/octo-adapter-telegram-mtproto/src/error.rs` (add
  `RateLimited { retry_after_secs: u64 }` variant if not present;
  reuse existing `ApiError` and `Auth` variants).
- `crates/octo-adapter-telegram-mtproto/examples/telegram_bot.rs` (new).
- `crates/octo-adapter-telegram-mtproto/CHANGELOG.md`: Phase 3 entry.
- `crates/octo-adapter-telegram-mtproto/Cargo.toml`: bump to `0.3.0`.

## Test Plan

1. **URL construction**: `BotApiClient::new(token)` builds the
   `sendMessage` URL as `https://api.telegram.org/bot<token>/sendMessage`
   (token is URL-safe, no special chars; if token has `:`, that's the
   bot_id separator and must not be percent-encoded).
2. **send_message happy path** (wiremock): POST to
   `/bot<token>/sendMessage` with form body
   `chat_id=123&text=hello` returns 200 with
   `{"ok": true, "result": {message_id: 1, chat: {id: 123, type: "private"},
   date: 1700000000, text: "hello"}}`. Assert the response is parsed
   into a `BotMessage` with `message_id == 1` and `text == Some("hello")`.
3. **send_document happy path** (wiremock): POST
   `/bot<token>/sendDocument` with multipart body containing
   `chat_id=123` and a `document` part with file content. Return
   canned `BotMessage` with a `document` field. Assert file is
   uploaded and response is parsed.
4. **get_updates happy path** (wiremock): GET
   `/bot<token>/getUpdates?offset=10&timeout=30` returns
   `{"ok": true, "result": [{update_id: 11, message: {...}}]}`. Assert
   `BotUpdate` parses with `update_id == 11`.
5. **get_updates long-poll behaviour** (wiremock): the mock server
   delays its response by 100 ms; the client times the call and
   asserts the call took ≥ 80 ms (long-poll is honoured: the client
   doesn't abort the request early).
6. **Error envelope: 401 Unauthorized** (wiremock): returns
   `{"ok": false, "error_code": 401, "description": "Unauthorized"}`.
   Assert `MtprotoTelegramError::Auth` is returned, and the original
   `description` is in the source chain.
7. **Error envelope: 429 Rate Limited** (wiremock): returns
   `{"ok": false, "error_code": 429, "description": "Too Many
   Requests", "parameters": {"retry_after": 5}}`. Assert
   `MtprotoTelegramError::RateLimited { retry_after_secs: 5 }`.
8. **Error envelope: 400 Bad Request** (wiremock): returns
   `{"ok": false, "error_code": 400, "description": "chat not
   found"}`. Assert `MtprotoTelegramError::ApiError(400)`.
9. **Error envelope: 500 Server Error** (wiremock): returns 500 with
   the error envelope. Assert `MtprotoTelegramError::Network`.
10. **Token redaction**: `BotApiClient` must not leak the bot token in
    its `Display` or `Debug` impls (test asserts the token substring
    is absent from the formatted output).
11. **Polling loop helper** (no real network, unit test): given a
    closure that returns canned `Vec<BotUpdate>` for three calls
    (one with one update, one empty, one with two updates), the loop
    helper drives the offset forward to 1+max(update_id) and yields
    each update in order. Assert no duplicate updates, no skipped
    updates, and the loop terminates after the third empty result.
12. **Transport routing** (unit test on the adapter): with
    `Transport::BotApiHttp` and a `bot_token`, `connect` returns an
    adapter that dispatches `send_envelope` to the `BotApiClient`
    path; with `Transport::Mtproto`, it dispatches to the existing
    grammers path. Assert no cross-contamination (a `BotApiHttp`
    adapter never calls into grammers types).
13. **Capability report**: with `Transport::BotApiHttp`,
    `capabilities()` reports text limit 4096 and upload limit
    50 MB; with `Transport::Mtproto`, it reports the MTProto limits
    (text 4096, upload 2 GB).

## Acceptance Criteria

- `cargo build -p octo-adapter-telegram-mtproto` succeeds with default
  features (no grammers, no reqwest; only the mock + Bot-API types are
  compiled and the `bot-api` module is gated on the feature flag, so
  the default build pulls nothing new).
- `cargo build -p octo-adapter-telegram-mtproto --features bot-api`
  succeeds and pulls reqwest.
- `cargo build -p octo-adapter-telegram-mtproto --features "real-network
  bot-api"` succeeds and pulls both grammers and reqwest.
- `cargo test -p octo-adapter-telegram-mtproto --features bot-api`
  passes all 13 new tests, plus all pre-existing tests.
- `cargo test -p octo-adapter-telegram-mtproto --features
  "real-network bot-api"` passes.
- `cargo test -p octo-adapter-telegram-mtproto` (default) passes
  (Bot-API module is feature-gated and the default build has no
  wiremock dependency).
- `cargo fmt --all -- --check` clean.
- `cargo clippy --workspace --all-targets --features
  "real-network bot-api" -- -D warnings` clean.

## Out of Scope (deferred to a later phase)

- Bot API webhook mode (`setWebhook` / `deleteWebhook`): the cipherocto
  DOT contract uses long-poll, not webhooks; deferred.
- Inline keyboards, callback queries, and other Bot API UI features:
  out of DOT scope; deferred.
- The `telegram_bot` Bot API surface beyond
  `sendMessage` / `sendDocument` / `getUpdates` / `getMe`: the
  cipherocto DOT contract uses a small, fixed set of methods; the
  rest can be added later if the contract grows.
- MTProto-over-HTTP (gap G4, mtproto_port.md §12): a different
  transport entirely; out of scope for this mission.
- The existing TDLib-based adapter's Bot API path: this mission
  implements the Bot API in the MTProto adapter from scratch per
  the canonical references; no code is shared with the TDLib
  adapter.
