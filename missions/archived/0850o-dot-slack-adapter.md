# Mission: DOT Slack Adapter

## Status

Implemented (8 tests, Slack Web API, retry/backoff, health_check)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.1

## Summary

Implement a pure Rust Slack Bot API adapter as a `cdylib` plugin. Enables CipherOcto gateways to send and receive deterministic envelopes through Slack channels.

## Acceptance Criteria

- [ ] `crates/octo-adapter-slack/` crate compiles to `cdylib`
- [ ] Implements `PlatformAdapter` trait with all methods (including `replay_protection`, `health_check`, `shutdown`)
- [ ] `send_envelope()` calls Slack Web API `chat.postMessage`
- [ ] `receive_messages()` uses Slack Events API or Socket Mode
- [ ] `canonicalize()` extracts envelope from Slack message text
- [ ] `CapabilityReport`: max_payload=40000 (Slack message limit), rate_limit=1/sec per channel
- [ ] `domain_id()`: `BroadcastDomainId(0x0007, BLAKE3(channel_id))`
- [ ] Config: `bot_token`, `app_token` (for Socket Mode), `channels`
- [ ] Error handling: rate limiting (Tier 3: 50+/sec), token expiry, Retry-After header parsing
- [ ] Socket Mode reconnect: exponential backoff with jitter (initial=3s, max=120s)
- [ ] Unit tests with mock HTTP responses

## Location

`crates/octo-adapter-slack/`

## Complexity

Medium

## Prerequisites

- Mission 0850e: DOT Adapter Registry & Plugin ABI

## Implementation Notes

- Slack Web API: `https://slack.com/api/chat.postMessage`
- Socket Mode for receiving (requires `app_token` with `connections:write` scope)
- Alternative: Events API with webhook endpoint
- Rate limits: Tier 3 (50+ requests/sec) — generous
- Message encoding: base64 envelope in `text` field, or use Slack's file upload API for large payloads

## Additional Requirements (from Audit)

- [ ] Implement `self_handle()` for relay loop prevention (see Mission 0850s)
- [ ] Implement `shutdown()` for graceful cleanup (see Mission 0850t)
- [ ] Add tests to match ZeroClaw coverage (see Mission 0850u)
