# Changelog — octo-adapter-telegram-mtproto

All notable changes to this crate are documented here. The crate adheres to
[Semantic Versioning](https://semver.org/) and the format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.4.1] — 2026-06-21

### Security

- **No new findings.**

### Fixes

- **R19-C1 (LOW): tighten `is_supergroup` heuristic.**
  The Phase A heuristic was `chat_id < 0`, which mis-
  classified legacy migrated basic groups (negative
  chat_ids without the `-1T` prefix, e.g., `-12345`) as
  supergroups. New heuristic: `chat_id <= -1_000_000_000_000`.
  This matches Telegram's canonical supergroup/channel
  chat_id construction
  (`-(1_000_000_000_000 + local_id)`) and the doc comment
  in `coordinator_admin.rs`. The fix is one line; the
  per-method `is_supergroup` callers
  (`promote_to_admin`, `demote_from_admin`,
  `transfer_ownership`, `add_member` supergroup branch)
  now correctly route legacy basic groups to
  `PlatformAdapterError::Unimplemented` instead of
  attempting a Telegram admin RPC. Test extended:
  `is_supergroup_detects_negative_ids` now also asserts
  the boundary (`-1_000_000_000_000` is a supergroup;
  `-1_000_000_000_000 + 1` is not) and the legacy basic
  group case (`-12345` is not a supergroup).
- **R19-C2 (LOW): add `tracing::debug!` at all 7
  connect-success notify sites.** The Phase A work
  added `connected_notify.notify_waiters()` at 7
  adapter connect-success paths
  (`connect_bot_token`, `connect_http`,
  `connect_user` code-only, `connect_user` 2FA,
  `connect_qr_login` already-authorized,
  `poll_qr_login`, `import_qr_login_token`) but no
  accompanying tracing. Operators debugging "why did
  the onboard CLI hang at wait_for_connected" had no
  log to inspect. Each site now emits
  `tracing::debug!(path, user_id, "connected_notify fired")`.

### Tests

- All 5 feature combinations still green:
  - default build: 152 tests pass
  - `--no-default-features`: 152 tests pass
  - `--features real-network`: 152 tests pass
  - `--features bot-api`: 176 tests pass
  - `--features "real-network bot-api"`: 176 tests pass
- `cargo fmt -p octo-adapter-telegram-mtproto -- --check`: clean
- `cargo clippy -p octo-adapter-telegram-mtproto --all-targets --features "real-network bot-api" -- -D warnings`: clean

## [0.4.0] — 2026-06-21

### Security

- **No new findings.** The new `GroupInfo` struct
  (returned by `MtprotoTelegramClient::get_chat`)
  carries only public group metadata (`chat_id`,
  `title`, `member_count`, `is_admin`); no token,
  session, password, or credential fields. The new
  `Connected { notify }` field is an
  `Arc<tokio::sync::Notify>`, not a session. The new
  `Runtime { groups }` field is a
  `RwLock<BTreeMap<i64, ()>>`, not a credential-bearing
  registry. No new `Display`/`Debug` paths were added
  for any credential-bearing type. **Result:** zero
  credential leaks introduced in this release.

### Breaking

- **`MtprotoTelegramClient` trait gained 11 new
  required methods.** Adapters that implement the
  trait by hand (none in this repo besides the
  built-in `MockTelegramMtprotoClient` and
  `RealTelegramMtprotoClient`) must add:
  `create_group`, `add_participant`, `kick_participant`,
  `promote_participant`, `demote_participant`,
  `set_chat_title`, `set_chat_about`, `delete_chat`,
  `leave_chat`, `get_chat`, `list_dialog_ids`. All
  return `Result<_, MtprotoTelegramError>`. New struct
  `GroupInfo` is part of the trait's API surface.

### Features

- **CoordinatorAdmin support (Mission
  0850p-a-coordinator-admin-telegram-mtproto).** New
  module `coordinator_admin.rs` implements
  `octo_network::dot::adapters::coordinator_admin::CoordinatorAdmin`
  for `MtprotoTelegramAdapter<C>`. Implemented methods:
  `admin_capabilities` (full Telegram-specific report —
  `can_create`, `can_leave`, `can_destroy`,
  `can_add_member`, `can_remove_member`, `can_promote`
  & `can_demote` both true but supergroup-only at the
  call site, `can_rename`, `can_describe`,
  `can_announce`, `can_transfer_ownership`),
  `platform_name() == "telegram"`, `create_group`,
  `leave_group`, `destroy_group`, `add_member`
  (auto-promotes the new member if the calling adapter
  is admin on a supergroup), `remove_member`,
  `promote_to_admin` (supergroup-only — basic groups
  return `Unimplemented`), `demote_from_admin`
  (supergroup-only), `rename_group`, `set_group_description`,
  `list_own_groups`, `get_group_metadata`,
  `resolve_invite` (Phase 1 stub — Unimplemented),
  `join_by_invite` (Phase 1 stub — Unimplemented),
  `transfer_ownership` (Phase 1 stub — Unimplemented
  for basic groups). The TDLib adapter
  (`octo-adapter-telegram`) is intentionally NOT
  touched — only the MTProto adapter opts in.
- **Connection notify (`Mission 0850p-a-notify-event-connected`).**
  New adapter-level field
  `connected_notify: Arc<tokio::sync::Notify>`. New
  method `connected() -> Arc<tokio::sync::Notify>`
  exposes a clone of the notify for callers that want
  to await connection completion. Wired to all 5
  successful connect paths: `connect_bot_token`,
  `connect_http`, `connect_user` (both code-only and
  2FA paths), `connect_qr_login` (already-authorized
  path), `poll_qr_login`, and `import_qr_login_token`.
- **`has_valid_session()`** (Mission
  0850p-a-has-valid-session). Adapter-level method
  returning `true` after any successful connect path
  completes and `false` before. Composes the
  `self_handle()` check with a runtime-state check.
- **`register_group_at_runtime(chat_id: i64)` and
  `is_runtime_group(chat_id: i64) -> bool`** (Mission
  0850p-a-register-group-at-runtime). Adapter-level
  registry (`runtime_groups: RwLock<BTreeMap<i64, ()>>`)
  for chat IDs created mid-session. Mirrors the
  WhatsApp `register_group_at_runtime(chat_jid: &str)`
  pattern from `octo-adapter-whatsapp`.
- **Telegram-specific capability report.** The MTProto
  adapter's `admin_capabilities` returns a faithful
  Telegram subset: create/leave/destroy/add/remove/
  promote/demote/rename/describe/announce are all
  supported; ban/lock/ephemeral/require-approval/
  join-by-id are NOT supported (Telegram has no
  equivalent primitives in the Bot API surface).
- **`as_coordinator_admin() -> Some(self)`** override on
  the `PlatformAdapter` impl. Returns the adapter as a
  `&dyn CoordinatorAdmin` so coordinator-level
  orchestration can discover the group-admin surface.

### Tests

- **22 new unit tests.**
  - **7 in `client.rs`** covering the mock client's
    new group-ops:
    `mock_create_group_returns_new_id`,
    `mock_add_and_kick_participant_round_trip`,
    `mock_set_chat_title_updates_title`,
    `mock_get_chat_unknown_returns_not_found`,
    `mock_delete_chat_removes_group`,
    `mock_list_dialog_ids_returns_sorted_ids`,
    `mock_set_mock_group_pre_seeds_state`.
  - **8 in `adapter.rs`** covering notify / session /
    runtime-registry / CoordinatorAdmin:
    `connected_notify_fires_on_bot_token_connect`
    (spawns a waiter, triggers connect, asserts notify
    fires within 1s),
    `connected_notify_does_not_fire_before_connect`
    (negative test: 100ms timeout),
    `connected_notify_clone_shares_underlying_notify`
    (two Arc<Notify> clones share underlying notify),
    `has_valid_session_false_before_connect`,
    `has_valid_session_true_after_bot_token_connect`,
    `register_group_at_runtime_idempotent_and_visible`,
    `as_coordinator_admin_returns_some`,
    `admin_capabilities_reports_telegram_subset`
    (16 capability booleans asserted).
  - **7 in `coordinator_admin.rs`** — 3 helper tests
    for `parse_chat_id` and `is_supergroup` plus 4
    end-to-end tests: `create_group_returns_handle`,
    `add_member_supergroup_promotes`,
    `promote_basic_group_returns_unimplemented`,
    `list_own_groups_returns_membership`.
- **All 5 feature combinations green:**
  - default build: 152 tests pass
  - `--no-default-features`: 152 tests pass
  - `--features real-network`: 152 tests pass
  - `--features bot-api`: 176 tests pass
  - `--features "real-network bot-api"`: 176 tests
    pass
- **`cargo fmt -p octo-adapter-telegram-mtproto -- --check`**: clean
- **`cargo clippy -p octo-adapter-telegram-mtproto --all-targets --features "real-network bot-api" -- -D warnings`**: clean

### Compatibility

- The MTProto adapter (`octo-adapter-telegram-mtproto`)
  is the **only** adapter affected. The TDLib adapter
  (`octo-adapter-telegram`) and the WhatsApp adapter
  are untouched. No public re-exports were renamed or
  removed; only additions.

## [0.3.3] — 2026-06-21

### Security

- **R17: hand-written `Debug` for `QrLoginHandle { token, url }`.**
  R15-C3 closed the bot-mode auth-leak
  (`MtprotoAuthAction`); R16-C1 closed the user-mode
  sister (`UserAuthAction`). R17 found the next sister
  leak: `QrLoginHandle` (a struct in `client.rs`) AND
  `MtprotoTelegramError::QrLoginHandle` (the matching
  error variant in `error.rs`) both derived `Debug` and
  would auto-format the raw QR login token bytes (the
  `auth.exportLoginToken` return — an authorization
  credential paired with the user scanning the QR) plus
  the `tg://login?token=<base64>` URL (same data,
  base64-encoded) on any `dbg!()`,
  `tracing::error!(?e, ...)`, or panic message. Fix:
  hand-written `Debug` on the struct prints
  `token: <redacted N bytes>` and `url: <redacted>`. The
  error enum's `Debug` is rewritten to mirror the
  auto-derive for every other variant (Auth / Network /
  Rpc / RateLimited / Session / Config / Capability /
  NotReady / Envelope / Internal) and only the
  `QrLoginHandle` variant is redacted. The `Display` path
  is unchanged: the QR variant still includes
  `url={url}` (caller needs the URL to render the QR
  code — it's the QR data, intentionally public) but the
  raw token never appears in any user-facing string.

### Tests

- **R17: 4 new unit tests.** 1 in `client.rs`
  (`qr_login_handle_struct_debug_does_not_leak_token_or_url`
  covers the struct's Debug redaction: no raw bytes, no
  base64 URL, redaction marker present, struct name
  present). 3 in `error.rs`
  (`qr_login_handle_error_variant_debug_does_not_leak_token_or_url`
  mirrors the struct test for the error variant;
  `qr_login_handle_error_variant_display_includes_url`
  locks in that the QR variant's `Display` still includes
  the URL — the caller needs it to render the QR code;
  `mtproto_telegram_error_debug_still_works_for_non_sensitive_variants`
  spot-checks that the hand-written Debug mirrors the
  auto-derive shape for the 10 non-credential variants so
  existing log lines / dbg!() calls on Auth / Network /
  Rpc / Session errors continue to show useful info).
- **R17: test totals** 130 default / 154 with
  `bot-api` (was 126 / 150 after R16).
- **R17: clippy / fmt clean.** `cargo fmt --check`,
  `cargo clippy --all-targets --features
  "real-network bot-api" -- -D warnings`, all four
  `cargo test --lib` feature combinations, and both
  example builds (`--features "real-network bot-api"`
  and no-features) are green.

## [0.3.2] — 2026-06-21

### Security

- **R16: hand-written `Debug` for `UserAuthAction`** (the
  user-mode sister of `MtprotoAuthAction`). R15-C3 closed
  the bot-mode auth-leak; the user-mode
  `RequestCode { phone }` / `SubmitCode { code }` /
  `SubmitPassword { password }` variants still derived
  `Debug`, so any `dbg!()` or `tracing::error!(?e)` on
  an `MtprotoAuthError::InvalidUserTransition` would
  leak the action payload. Fix: hand-written `Debug`
  prints variant name only, mirroring the R15-C3 fix
  on `MtprotoAuthAction`. The `Display` impl already
  redacted and is unchanged.

### Fixed

- **R16: `validate()` checks the new `bot_api_base_url`
  field.** R15-C11 added the field for tests but the
  `validate()` function never checked it, so empty
  strings and non-https URLs surfaced only at request
  time. Non-https was the worst failure mode — the
  bot token is the only auth credential on the Bot API
  path, and a typo (e.g. `http://attacker.example.com`)
  would silently send the token over plaintext. The
  new check rejects empty strings and any URL that
  doesn't start with `https://`. Tests using
  `MtprotoTelegramAdapter::new` directly (the wiremock
  happy-path) bypass `validate()` and are unaffected.
- **R16: `examples/telegram_bot.rs` uses `error!` for
  the "you built wrong" message** in the
  `not(bot-api)` branch. R15-C16's follow-up removed
  `warn` from the tracing import; the example now
  compiles cleanly without `--features bot-api` (it
  fails to compile before the fix). The `info` import
  is gated on `bot-api` so the `not(bot-api)` build
  doesn't warn about an unused import.

### Tests

- **R16: 7 new unit tests.** 2 in `auth.rs`
  (`user_auth_action_debug_does_not_leak_payload` covers
  all 3 sensitive variants + `Display` and `Debug`;
  `invalid_user_transition_error_does_not_leak_payload`
  closes the gap on the user-mode error path). 1 in
  `config.rs` (`bot_api_base_url_validation` covers
  `None` / empty / `http://` / `https://`). 4 in
  `adapter.rs` (`register_domain_accepts_user_chat_id`,
  `register_domain_accepts_basic_group_chat_id`,
  `register_domain_accepts_supergroup_chat_id`,
  `register_domain_rejects_empty_zero_non_i64`).
- **R16: test totals** 126 default / 150 with
  `bot-api` (was 119 / 143 after R15).
- **R16: clippy / fmt clean.** `cargo fmt --check`,
  `cargo clippy --all-targets --features
  "real-network bot-api" -- -D warnings`, all four
  `cargo test --lib` feature combinations, and both
  example builds (`--features "real-network bot-api"`
  and no-features) are green.

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

[0.3.3]: #033--2026-06-21
[0.3.2]: #032--2026-06-21
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
