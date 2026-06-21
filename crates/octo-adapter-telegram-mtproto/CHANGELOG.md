# Changelog — octo-adapter-telegram-mtproto

All notable changes to this crate are documented here. The crate adheres to
[Semantic Versioning](https://semver.org/) and the format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

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
