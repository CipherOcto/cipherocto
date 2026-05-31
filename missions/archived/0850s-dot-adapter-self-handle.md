# Mission: Add self_handle() to DOT Adapters (Matrix, IRC, Slack, Signal, Nostr)

## Status

Implemented (5 adapters: matrix=11 tests, irc=24, slack=13, signal=8, nostr=13)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.2

## Summary

Add `self_handle()` override to 5 DOT adapters that currently return `None` (default). This enables relay loop prevention — the gateway drops inbound messages whose sender matches the bot's own identity.

## Why

Without `self_handle()`, the gateway cannot prevent the bot from responding to its own outbound messages that are echoed back by the platform. This can cause infinite relay loops on platforms that echo bot messages (e.g., Slack, Matrix, IRC).

Telegram and Discord already implement `self_handle()` — this mission brings the remaining adapters to parity.

## Claimant

@agent (Jcode)

## Acceptance Criteria

### Matrix (`crates/octo-adapter-matrix/`)

- [x] Add `self_handle()` returning the bot's Matrix user ID (e.g., `@bot:server`)
- [x] Resolve from `matrix-sdk` client's `user_id()` on first call
- [x] Cache in `Arc<Mutex<Option<String>>>`
- [x] Test: verify self_handle returns Some after client init

### IRC (`crates/octo-adapter-irc/`)

- [x] Add `self_handle()` returning the configured IRC nickname
- [x] Source from `config.nickname` (already available in IRC adapter config)
- [x] No caching needed (static config)
- [x] Test: verify self_handle returns configured nickname

### Slack (`crates/octo-adapter-slack/`)

- [x] Add `self_handle()` returning the bot's Slack user ID
- [x] Resolve via `auth.test` API call (same endpoint used in health_check)
- [x] Cache in `Arc<Mutex<Option<String>>>` to avoid repeated API calls
- [x] Test: verify self_handle returns Some after auth.test

### Signal (`crates/octo-adapter-signal/`)

- [x] Add `self_handle()` returning the bot's Signal phone number
- [x] Source from `config.phone_number` (already available in Signal adapter config)
- [x] No caching needed (static config)
- [x] Test: verify self_handle returns configured phone number

### Nostr (`crates/octo-adapter-nostr/`)

- [x] Add `self_handle()` returning the bot's Nostr public key (hex)
- [x] Source from `config.nsec` or derived public key
- [x] Cache in `Arc<Mutex<Option<String>>>`
- [x] Test: verify self_handle returns Some after key derivation

## Design Reference

- **ZeroClaw pattern**: `zeroclaw/crates/zeroclaw-channels/src/telegram.rs` line 3764 — `fn self_handle(&self) -> Option<String>`
- **ZeroClaw pattern**: `zeroclaw/crates/zeroclaw-channels/src/discord.rs` — decodes bot ID from token
- **CipherOcto Telegram**: `crates/octo-adapter-telegram/src/lib.rs` — already implements self_handle
- **CipherOcto Discord**: `crates/octo-adapter-discord/src/lib.rs` — already implements self_handle

## Implementation Notes

- Use `Arc<Mutex<Option<String>>>` for lazy initialization (same pattern as Telegram/Discord adapters)
- For IRC and Signal, the identity is static (from config) — no API call needed
- For Matrix and Slack, the identity is resolved at runtime via API
- For Nostr, the identity is derived from the secret key

## Location

- `crates/octo-adapter-matrix/src/lib.rs`
- `crates/octo-adapter-irc/src/lib.rs`
- `crates/octo-adapter-slack/src/lib.rs`
- `crates/octo-adapter-signal/src/lib.rs`
- `crates/octo-adapter-nostr/src/lib.rs`

## Complexity

Low-Medium

## Prerequisites

None
