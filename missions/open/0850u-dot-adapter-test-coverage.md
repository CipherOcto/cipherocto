# Mission: Add Tests to DOT Adapters (Match ZeroClaw Coverage)

## Status

Open

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

## Acceptance Criteria

### All Adapters

- [ ] Test domain_hash determinism and normalization
- [ ] Test encode_envelope / decode_envelope roundtrip
- [ ] Test decode_envelope with missing prefix (error case)
- [ ] Test decode_envelope with invalid base64 (error case)
- [ ] Test platform_type constant matches enum
- [ ] Test capabilities report values
- [ ] Test config deserialization from JSON
- [ ] Test health_check returns Ok for valid config
- [ ] Test self_handle returns expected value (where implemented)

### Platform-Specific Tests

#### Telegram
- [ ] Test message formatting (Markdown, HTML)
- [ ] Test chat_id normalization (negative IDs, supergroups)
- [ ] Test rate limit retry logic
- [ ] Test attachment parsing

#### Discord
- [ ] Test webhook URL validation
- [ ] Test embed formatting
- [ ] Test channel ID extraction

#### Matrix
- [ ] Test room ID formatting
- [ ] Test E2E encryption key handling
- [ ] Test sync token management

#### Slack
- [ ] Test channel ID validation
- [ ] Test thread_ts handling
- [ ] Test rate limit (Tier 3: 50 req/min)
- [ ] Test Socket Mode vs Web API mode

#### IRC
- [ ] Test channel name validation (#channel format)
- [ ] Test nickname normalization
- [ ] Test TLS connection handling

#### WhatsApp
- [ ] Test group JID formatting (@g.us)
- [ ] Test phone number normalization
- [ ] Test reconnect delay calculation
- [ ] Test session file paths (WAL/SHM sidecars)

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

Low (test-only changes)

## Prerequisites

None
