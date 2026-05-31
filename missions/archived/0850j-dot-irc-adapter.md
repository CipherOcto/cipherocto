# Mission: DOT IRC Adapter

## Status

Implemented (24 tests)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.1

## Summary

Implement a pure Rust IRC client adapter as a `cdylib` plugin. IRC is the simplest transport — text-only, well-understood, widely deployed.

## Acceptance Criteria

- [x] `crates/octo-adapter-irc/` crate compiles to `cdylib`
- [x] Implements `PlatformAdapter` trait with all methods (6 required + 3 optional: replay_protection, health_check, shutdown)
- [x] `send_envelope()` sends PRIVMSG to IRC channel
- [x] `receive_messages()` reads from IRC channel via bot connection
- [x] `canonicalize()` extracts envelope from IRC message text
- [x] Fragmentation: multi-line PRIVMSG with sequence markers, UTF-8 safe boundary splitting
- [x] `CapabilityReport`: max_payload=480 (512 - ~32 PRIVMSG overhead), rate_limit=1/sec
- [x] `domain_id()`: `BroadcastDomainId(0x0006, BLAKE3(server:channel))`
- [x] Config: `server`, `port`, `nickname`, `channels`, `password` (optional)
- [x] Unit tests with mock IRC server responses (24 tests)

## Location

`crates/octo-adapter-irc/`

## Complexity

Low

## Prerequisites

- Mission 0850e: DOT Adapter Registry & Plugin ABI

## Implementation Notes

- IRC protocol is text-based, RFC 2812 compliant
- Max message length: 512 bytes including CRLF — envelopes must be base64 and may need fragmentation
- Use CTCP or DCC for binary data if available
- Reconnection: IRC servers can disconnect idle bots — implement keepalive PING/PONG
- SSL/TLS: support both plain and TLS connections

## Additional Requirements (from Audit)

- [ ] Implement `self_handle()` for relay loop prevention (see Mission 0850s)
- [ ] Implement `shutdown()` for graceful cleanup (see Mission 0850t)
- [ ] Add tests to match ZeroClaw coverage (see Mission 0850u)
