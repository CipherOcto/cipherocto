# Changelog — octo-adapter-telegram-mtproto

All notable changes to this crate are documented here. The crate adheres to
[Semantic Versioning](https://semver.org/) and the format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

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
