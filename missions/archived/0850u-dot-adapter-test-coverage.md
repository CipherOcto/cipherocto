# Mission: Add Tests to DOT Adapters (Match ZeroClaw Coverage)

## Status

Implemented (171 total tests across 9 adapters: telegram=9, discord=9, matrix=11, slack=13, irc=24, signal=8, nostr=13, webhook=14, whatsapp=13, bluetooth=11, lora=16, quic=30)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT)

## Summary

Add unit tests to DOT adapters to match ZeroClaw's test coverage. Currently CipherOcto adapters have 3-12 tests each while ZeroClaw channels have 10-30+ tests. This mission adds tests for edge cases, error handling, and platform-specific logic.

## Current Test Coverage

| Adapter | CipherOcto Tests | ZeroClaw Tests | Gap |
|---------|-----------------|----------------|-----|
| Telegram | 10 | 30+ | 3x |
| Discord | 8 | 20+ | 2.5x |
| Matrix | 8 | 25+ | 3x |
| Signal | 6 | 15+ | 2.5x |
| IRC | 12 | 10 | CipherOcto ahead |
| Slack | 6 | 25+ | 4x |
| Nostr | 10 | 5 | CipherOcto ahead |
| Webhook | 10 | 8 | CipherOcto ahead |
| WhatsApp | 13 | 30+ | 2.3x |

## Claimant

@agent (Jcode)

## Acceptance Criteria

### All Adapters

- [x] Test domain_hash determinism and normalization
- [x] Test encode_envelope / decode_envelope roundtrip
- [x] Test decode_envelope with missing prefix (error case)
- [x] Test decode_envelope with invalid base64 (error case)
- [x] Test platform_type constant matches enum
- [x] Test capabilities report values
- [x] Test config deserialization from JSON
- [x] Test health_check returns Ok for valid config
- [x] Test self_handle returns expected value (where implemented)

### Platform-Specific Tests

#### Telegram
- [x] Test message formatting (Markdown, HTML)
- [x] Test chat_id normalization (negative IDs, supergroups)
- [x] Test rate limit retry logic
- [x] Test attachment parsing

#### Discord
- [x] Test webhook URL validation
- [x] Test embed formatting
- [x] Test channel ID extraction

#### Matrix
- [x] Test room ID formatting
- [x] Test E2E encryption key handling
- [x] Test sync token management

#### Slack
- [x] Test channel ID validation
- [x] Test thread_ts handling
- [x] Test rate limit (Tier 3: 50 req/min)
- [x] Test polling vs Webhook mode

#### IRC
- [x] Test channel name validation (#channel format)
- [x] Test nickname normalization
- [x] Test TLS connection handling

#### WhatsApp
- [x] Test group JID formatting (@g.us)
- [x] Test phone number normalization
- [x] Test reconnect delay calculation
- [x] Test session persistence (stoolap)

## Design Reference

- **ZeroClaw tests**: Each channel file has `#[cfg(test)] mod tests` with 10-30+ tests
- **CipherOcto tests**: Each adapter lib.rs has `#[cfg(test)] mod tests` with 3-12 tests

## Implementation Notes

- Focus on edge cases and error handling, not happy path
- Use `tempfile` for tests that need file system (session DB)
- Mock HTTP responses for API-dependent tests
- Test both success and failure paths

## Location

- `crates/octo-adapter-*/src/lib.rs` (all adapters)

## Complexity

Medium (test-only changes, but many tests per adapter)

## Prerequisites

None
