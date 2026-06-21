# Changelog — octo-adapter-telegram-mtproto

All notable changes to this crate are documented here. The crate adheres to
[Semantic Versioning](https://semver.org/) and the format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.3.1] — 2026-06-21

### Security

- **R15: hand-written `Debug` for all credential-bearing
  structs.** `AuthMode::BotToken` and `UserCredentials.phone`
  were previously leaked in full via the auto-derived
  `Debug`; same for `BotApiConfig.token` and the
  `RealTelegramMtprotoClient` QR-login state. Replaced with
  manual `Debug` impls that print `[REDACTED]`.
- **R15: hand-written `Display` for `MtprotoAuthAction`.**
  The `MtprotoAuthError::InvalidTransition` and the
  `From<MtprotoAuthError>` for `MtprotoTelegramError` mappings
  used `{action:?}`, which leaked the auth code / password
  in error messages. Switched to a variant-name-only
  `Display` and `{action}` in the format sites.
- **R15: `Zeroizing<String>` for `qr_api_hash`.** The
  `RealTelegramMtprotoClient` cached the QR-login `api_hash`
  in a plain `Mutex<Option<String>>`; now wrapped in
  `Zeroizing` so the secret is wiped from memory on drop.

### Fixed

- **R15: `register_domain` accepts all three chat-id
  conventions** (user id, basic group id, supergroup/channel
  id with the `-100…` prefix). The previous impl rejected
  non-positive ids, which broke supergroup / channel
  outbound. Documented the conventions in the doc-comment.
- **R15: `StoolapSession::Drop` zeroizes the cached
  `auth_key: Option<[u8; 256]>`** on drop. Previously the
  key was leaked into the heap on adapter shutdown.
- **R15: `from_file_or_env` distinguishes
  `io::ErrorKind::NotFound`** from other IO errors, instead
  of substring-matching `"No such file"` / `"not found"`
  (which is platform-fragile).
- **R15: `MtprotoTelegramAdapter::domain_id` no longer
  auto-populates** from a previously-used chat id. Callers
  must now call `register_domain` explicitly before sending
  an envelope. This closes a class of cross-chat send bugs
  where a stale `domain_id` would route a new message to
  the wrong conversation.
- **R15: `examples/telegram_bot.rs` uses `tracing` for
  runtime output.** Every `eprintln!` call has been replaced
  with `tracing::info!` / `error!` (the pre-init usage
  hint, which runs before the subscriber is installed,
  is the only `eprintln!` left). A new
  `init_tracing()` helper wires up
  `tracing_subscriber::fmt()` + `EnvFilter` with the
  `tracing-subscriber` dep gated on `bot-api`.

### Removed (or replaced)

- **R15: `MtprotoSelfIdentity::handle()` is gone.** It
  returned the wrong canonical form (`"user:12345"`
  instead of `"telegram:user:12345"`); callers that need
  the canonical form should use the
  `PlatformAdapter::self_handle` capability probe, which
  already returns the right form.
- **R15: `MtprotoSelfIdentity::set_username` is gone.**
  It has been deprecated since 0.1.0; the underlying
  field is set via `MtprotoTelegramAdapter::connect_*`
  flows and is not externally settable.
- **R15: env-var-driven `TELEGRAM_BOT_API_BASE_URL`
  override removed from `BotApiConfig::new`.** The
  public API to point the adapter at a non-default Bot
  API endpoint is now
  `MtprotoTelegramConfig::bot_api_base_url` (an
  `Option<String>` field, additive). This removes a
  racy `unsafe { std::env::set_var }` from the
  `connect_http` test and makes the override
  deterministic across parallel test runs.

### Tests

- **R15: 16 new unit tests.** `parse_flood_wait` (5),
  `read_all_peer_infos` chat / channel round-trip (2),
  `connect_http_tests` mod (4, gated on `bot-api`),
  `MtprotoSelfIdentity` canonical-form check (1), the
  `RateLimited` flood-wait integration test (1), plus
  internal coverage in `config::from_file_or_env` /
  `StoolapSession` zeroize on drop / `register_domain`
  three-convention parsing.
- **R15: bugfixes to existing tests.** Two
  `send_envelope` tests now call `register_domain`
  explicitly (the auto-population that used to paper
  over the missing call is gone). The `connect_http`
  happy-path test now points the adapter at a wiremock
  server via `config.bot_api_base_url` instead of a
  racy `unsafe { std::env::set_var }` /
  `env::remove_var` dance.
- **R15: clippy / fmt clean.** `cargo fmt --check`,
  `cargo clippy --all-targets --features "real-network
  bot-api" -- -D warnings`, and all four
  `cargo test --lib` feature combinations
  (default, `real-network`, `bot-api`, both) are green:
  119 / 119 / 143 / 143 tests, 0 failures.

## [0.3.0] — 2026-06-21

### Added

- **Phase 3: Bot-API HTTP fallback** (sub-mission `0850ab-c-http`).
  The Bot API at `https://api.telegram.org/bot<token>/<method>`
  is HTTPS + JSON, bot-only, and **not** part of MTProto. It
  is opt-in behind the new `bot-api` Cargo feature for
  cipherocto users in region-blocked networks where the
  Telegram DCs are unreachable but `api.telegram.org` is.
  Canonical reference: §4 of
  `docs/research/2026-06-21-telegram-pure-rust-mtproto-adapter.md`
  and the public Telegram Bot API at
  <https://core.telegram.org/bots/api>. The `mtproto_port.md`
  doc is **not** a reference for this module — it documents
  MTProto-over-HTTP (gap G4, not implemented; a different
  transport entirely).
- New `bot-api` Cargo feature (independent of `real-network`):
  pulls in `reqwest 0.13` + `rustls 0.23` +
  `rustls-native-certs 0.8`. The default build does **not**
  need an HTTP client. `wiremock 0.6` is added to
  `[dev-dependencies]` for the test suite.
- New `crate::transport::Transport` enum (unconditional):
  `Mtproto` (default) | `BotApiHttp`. Implements
  `Default`, `Display`, `FromStr`, `Serialize`, `Deserialize`
  (with kebab-case rename; canonical wire form `"http"`,
  alias `"bot-api-http"`). Used by the config to pick
  the transport and by the adapter to set `capabilities`.
- New `crate::http_fallback` module (gated on `bot-api`):
  - `BotApiClient` (reqwest + rustls). Methods:
    `send_message`, `send_document`, `get_updates` (long-poll
    via `timeout` query param, capped at 50 s),
    `get_me`, `method_url`. Debug impl **redacts the token**.
  - Typed response structs: `BotMessage`, `BotUpdate`,
    `BotUser`, `BotChat`, `BotDocument`.
  - Error envelope: `BotApiErrorParameters` (with
    `retry_after` and `migrate_to_chat_id`).
  - `run_long_poll` helper that drives the long-poll
    loop, advances `offset` to `max(update_id) + 1`, and
    calls a user-supplied handler.
  - Constants: `MAX_UPLOAD_BYTES = 50 MiB`,
    `MAX_MESSAGE_CHARS = 4096`, `MAX_LONG_POLL_SECS = 50`,
    `DEFAULT_BOT_API_BASE_URL = "https://api.telegram.org"`.
- New `MtprotoTelegramError::RateLimited { retry_after_secs }`
  variant. The `From<MtprotoTelegramError>` for
  `PlatformAdapterError` impl forwards the actual
  server-supplied backoff (in seconds) as `retry_after_ms`,
  not the conservative 1000 ms default used for
  `Rpc { code: 429 }`. The variant is `#[non_exhaustive]`-
  safe; the mapping is in the `adapter` module.
- `MtprotoTelegramConfig` gained a `transport: Transport`
  field (default `Mtproto`, env `TELEGRAM_TRANSPORT`).
  `validate()` rejects `BotApiHttp` for user mode (the Bot
  API is bot-only by design).
- `MtprotoTelegramAdapter::capabilities()` is now
  transport-aware: `Mtproto` reports 2 GB upload / 30 msg/s
  (1 msg/s in user mode); `BotApiHttp` reports 50 MB upload
  / 30 msg/s. Text limit is 4096 chars on both.
- `MtprotoTelegramAdapter::connect_http(bot_token)` (gated
  on `bot-api`): the Bot-API equivalent of
  `connect_bot_token`. Verifies the token via `getMe()` and
  populates the self-handle. Returns the `BotApiClient` so
  the caller can use it for `sendMessage` / `sendDocument` /
  `getUpdates`. Refuses to run if `config.transport` is not
  `BotApiHttp` or if `mode` is not `bot`.
- Example binary `examples/telegram_bot.rs` (gated on
  `bot-api`): smoke-test of the full Bot-API surface
  (`getMe` + `sendMessage` + `getUpdates` long-poll).
  Reads `TELEGRAM_BOT_TOKEN`, `TELEGRAM_DEST_CHAT`,
  `TELEGRAM_TEXT`, `TELEGRAM_LONG_POLL` env vars.

### Tests

- 18 new unit tests in `http_fallback` (URL construction,
  `Debug` redaction, empty token rejection,
  `sendMessage` happy path + form-encoding, `sendMessage`
  empty-text / oversize-text rejection, `sendDocument`
  happy path + oversize / empty-file rejection,
  `getUpdates` happy path + long-poll timing,
  `getMe`, 401 → `Auth`, 429 with `retry_after` →
  `RateLimited`, 400 → `Rpc`, 502 → `Network`,
  unparseable body → `Envelope`, reqwest error doesn't
  leak token, long-poll offset advancement).
- 4 new tests in `transport` (default, `from_str` aliases,
  `Display`, serde round-trip + unknown rejection).
- 5 new tests in `adapter` (capabilities for default /
  http / user mode, `RateLimited` mapping + clamp).
- 1 updated test in `error::is_retryable` covers the new
  `RateLimited` variant.
- **Test totals**: 109 default / 128 with `bot-api` (was
  99 / 99 before this phase). All `cargo fmt` and
  `cargo clippy --all-targets -- -D warnings` checks are
  clean across the default build, `--features bot-api`,
  `--features real-network`, and `--features "real-network
  bot-api"` combinations.

### Out of Scope

- Bot API webhook mode (`setWebhook` / `deleteWebhook`):
  the cipherocto DOT contract uses long-poll, not webhooks.
- Inline keyboards, callback queries, and other Bot API UI
  features: out of DOT scope.
- MTProto-over-HTTP (gap G4, `mtproto_port.md` §12): a
  different transport entirely; not this mission.

## [0.2.0] — 2026-06-21

### Added

- **Phase 2.5: QR login flow** (sub-mission `0850ab-c-user`).
- `MtprotoTelegramClient::qr_login` / `poll_qr_login` /
  `import_login_token` trait methods. Implementations:
  - `MockTelegramMtprotoClient` — deterministic mock that
    accepts a configurable number of pending polls before
    returning success (`set_qr_polls_to_success`).
  - `RealTelegramMtprotoClient` — wraps
    `tl::functions::auth::ExportLoginToken` and
    `tl::functions::auth::ImportLoginToken`. Drives the
    `UserAuthLifecycle` state machine through
    `NoCredentials → QrLoginPending → QrLoginConfirmed →
    SignedIn` on success.
- `MtprotoTelegramAdapter::connect_qr_login` /
  `poll_qr_login` / `import_qr_login_token` adapter
  methods that orchestrate the flow and drive the outer
  `AdapterLifecycle` to `Ready` on success.
- `QrLoginHandle` struct (re-export of the
  `MtprotoTelegramError::QrLoginHandle` variant payload;
  `QrLoginHandle::from_error` helper).
- Hand-rolled `build_qr_url` (standard base64 with padding)
  — no extra crate dependency for the
  `no-default-features` build.
- `MtprotoTelegramError::QrLoginHandle { token, url }`
  variant: a flow-state marker (not a real error). The
  `From<MtprotoTelegramError>` for `PlatformAdapterError`
  mapping translates it to `ApiError(425)` ("Too Early —
  the QR isn't scanned yet") for generic platform code
  that doesn't pattern-match on the variant directly.
- `UserAuthLifecycle::QrLoginPending` and
  `UserAuthLifecycle::QrLoginConfirmed` enum variants
  (repr `0x09` and `0x0A`) plus `UserAuthAction::QrLoginStart`
  / `UserAuthAction::QrLoginConfirm` client-side
  transitions; `SignInSucceeded` server-side transition
  drives `QrLoginConfirmed → SignedIn`.

### Changed

- `clippy::manual_div_ceil` fix in the new
  `build_qr_url` (`(n + 2) / 3` → `n.div_ceil(3)`).

### Deferred to sub-missions

- **Phase 3 — Bot-API HTTP fallback**: sub-mission
  `0850ab-c-http`.
- **Phase 4 — Transport wrappers** (SOCKS5, HTTP CONNECT,
  fake-TLS): sub-mission `0850ab-c-wrappers` (conditional
  on cipherocto use case).

[rfc]: ../../../rfcs/accepted/networking/0850ab-c-pure-rust-mtproto-telegram-adapter.md
[platform-adapter]: ../octo-network
[grammers]: https://crates.io/crates/grammers-client

[0.3.1]: #031--2026-06-21
[0.3.0]: #030--2026-06-21
[0.2.0]: #020--2026-06-21
[0.1.0]: #010--2026-06-21

## [0.1.0] — 2026-06-21

### Added

- Initial release: Phase 1 Core of the pure-Rust MTProto Telegram adapter
  ([RFC-0850ab-c][rfc], Mission 0850ab-c).
- `MtprotoTelegramAdapter<C>` implementing
  [`PlatformAdapter`][platform-adapter] from RFC-0850 §8.2.
- Pure-Rust MTProto transport via the
  [`grammers`][grammers] family of crates (no TDLib, no C/C++ toolchain).
- `StoolapSession` — a `grammers_session::Session` impl backed by CipherOcto's
  stoolap fork on `feat/blockchain-sql` (project-wide cipherocto persistence
  convention; closes the libsql transitive dep that
  `grammers_session::storages::SqliteSession` would otherwise pull in).
- `MockTelegramMtprotoClient` (default build, in-process) for adapter unit
  tests.
- `RealTelegramMtprotoClient` (gated behind `--features real-network`) wiring
  up `grammers_client::Client` + `SenderPool`.
- Bot-mode sign-in (`connect_bot_token`) with single-step state machine.
- Three lifecycles: `AdapterLifecycle`, `BotAuthLifecycle`, `UserAuthLifecycle`
  (the last is enum-skeleton only — full state machine deferred to Phase 2).
- DOT wire-format codec (`DOT/1/{b64}` for ≤ 4096-byte payloads;
  `DOT/2/{msg_id}` for larger via document upload).
- Self-handle filter (`MtprotoSelfHandle`) for self-loop prevention.
- Credential redaction (`redact_credentials`) for all log / Debug paths.
- `MtprotoTelegramConfig` schema mirrors `TelegramConfig` (TDLib adapter) plus
  additive MTProto-only fields.
- `AdapterKind` enum added to the TDLib adapter's `TelegramConfig`
  (`adapter_kind: Tdlib | Mtproto`, default `Tdlib`) — no breaking change for
  existing deployments.

### Deferred to sub-missions

- **Phase 2 — User mode + QR login**: sub-mission `0850ab-c-user`.
  `request_login_code` / `submit_code` / `submit_password` return
  `NotReady` in Phase 1.
- **Phase 3 — Bot-API HTTP fallback**: sub-mission `0850ab-c-http`.
- **Phase 4 — Transport wrappers** (SOCKS5, HTTP CONNECT, fake-TLS):
  sub-mission `0850ab-c-wrappers` (conditional on cipherocto use case).

[rfc]: ../../../rfcs/accepted/networking/0850ab-c-pure-rust-mtproto-telegram-adapter.md
[platform-adapter]: ../octo-network
[grammers]: https://crates.io/crates/grammers-client

[0.1.0]: #010--2026-06-21
