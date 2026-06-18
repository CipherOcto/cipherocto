# Mission: DOT Telegram Adapter

## Status

**Superseded** — Replaced by [Mission 0850ab](./0850ab-dot-telegram-tdlib-adapter.md) (TDLib-backed implementation)

## Original Status

Implemented (9 tests, retry/backoff, self_handle, health_check)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.1

## Summary

Implement a pure Rust Telegram Bot API adapter as a `cdylib` plugin. Enables CipherOcto gateways to send and receive deterministic envelopes through Telegram groups.

## Design

See `docs/plans/2026-05-28-social-platform-transport-adapters-design.md` — Telegram Adapter.

## Acceptance Criteria

- [ ] `crates/octo-adapter-telegram/` crate compiles to `cdylib`
- [ ] Implements `PlatformAdapter` trait with all methods (6 required + 3 optional: replay_protection, health_check, shutdown)
- [ ] `send_envelope()` calls Telegram Bot API `sendMessage` / `sendDocument`
- [ ] `receive_messages()` uses long-polling via `getUpdates`
- [ ] `canonicalize()` extracts envelope from Telegram message text/document
- [ ] Fragmentation: large envelopes sent as document attachments
- [ ] `CapabilityReport`: max_payload=4096, rate_limit=30/sec per group
- [ ] `domain_id()`: `BroadcastDomainId(0x0001, BLAKE3(chat_id))`
- [ ] Config: `bot_token`, `webhook_port` (optional), `groups` (list of chat IDs)
- [ ] Error handling: rate limiting (429 retry), auth expiry, network timeout
- [ ] Exponential backoff with jitter: initial=1s, max=120s, jitter=0-500ms
- [ ] Self-loop prevention: `self_handle()` returns bot username to drop self-authored messages
- [ ] Unit tests with mock HTTP responses
- [ ] Integration test against Telegram Bot API sandbox

## Location

`crates/octo-adapter-telegram/`

## Complexity

Medium

## Prerequisites

- Mission 0850e: DOT Adapter Registry & Plugin ABI

## Implementation Notes

- Use `reqwest` for HTTP, `serde` for Telegram API JSON
- Telegram groups use negative chat IDs (e.g., `-1001234567890`)
- Bot must be added to group with message read permission
- Long-polling timeout: 30 seconds (Telegram recommendation)
- Webhook mode requires public TLS endpoint — document reverse proxy setup
- Message text encoding: envelope serialized as base64 in message body
- For envelopes > 4096 bytes after base64: use `sendDocument` with binary attachment

## Additional Requirements (from Audit)

- [ ] Implement `self_handle()` for relay loop prevention (see Mission 0850s)
- [ ] Implement `shutdown()` for graceful cleanup (see Mission 0850t)
- [ ] Add tests to match ZeroClaw coverage (see Mission 0850u)
