# Mission: DOT Discord Adapter

## Status

Implemented (11 tests, retry/backoff, wire_bytes fix, health_check)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.1

## Summary

Implement a pure Rust Discord adapter as a `cdylib` plugin using webhooks for sending and the Gateway WebSocket API for receiving.

## Design

See `docs/plans/2026-05-28-social-platform-transport-adapters-design.md` — Discord Adapter.

## Acceptance Criteria

- [ ] `crates/octo-adapter-discord/` crate compiles to `cdylib`
- [ ] Implements `PlatformAdapter` trait with all methods (6 required + 3 optional: replay_protection, health_check, shutdown)
- [ ] `send_envelope()` posts via Discord webhook URL
- [ ] `receive_messages()` connects to Discord Gateway WebSocket
- [ ] `canonicalize()` extracts envelope from Discord message content/embed
- [ ] Fragmentation: multi-message with sequence markers for large envelopes
- [ ] `CapabilityReport`: max_payload=2000, rate_limit=5/sec per channel
- [ ] `domain_id()`: `BroadcastDomainId(0x0002, BLAKE3(channel_id))`
- [ ] Config: `bot_token`, `webhook_url`, `guild_id`, `channels` (list of channel IDs)
- [ ] Error handling: rate limiting (429 + Retry-After), gateway reconnect
- [ ] Exponential backoff with jitter: initial=1s, max=120s, jitter=0-500ms
- [ ] Self-loop prevention: `self_handle()` returns bot user ID to drop self-authored messages
- [ ] Unit tests with mock HTTP responses
- [ ] Integration test against Discord test server

## Location

`crates/octo-adapter-discord/`

## Complexity

Medium

## Prerequisites

- Mission 0850e: DOT Adapter Registry & Plugin ABI

## Implementation Notes

- Webhook sending: `POST /webhooks/{id}/{token}` — no bot auth needed for send
- Gateway receiving: WebSocket with `GUILD_MESSAGES` intent
- Discord rate limits: 5 messages/second per channel, 50 per second globally
- Message content encoding: base64 envelope in message text
- For envelopes > 2000 bytes: split into multiple messages with `[fragment 1/N]` prefix
- Alternative: use file attachment (25MB limit) for single-message large envelopes
- Gateway heartbeat: Discord requires ACK heartbeat every 41.25 seconds

## Additional Requirements (from Audit)

- [ ] Implement `self_handle()` for relay loop prevention (see Mission 0850s)
- [ ] Implement `shutdown()` for graceful cleanup (see Mission 0850t)
- [ ] Add tests to match ZeroClaw coverage (see Mission 0850u)
