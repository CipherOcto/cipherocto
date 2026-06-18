# Mission: DOT DingTalk Adapter (PlatformType 0x0012)

Implemented (402 lines, 11 tests)

## Claimant

@agent (Jcode)


## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.1

## Summary

Implement a DingTalk adapter for DOT transport. DingTalk is Alibaba's enterprise messaging platform with 600M+ users, dominant in Chinese enterprise communication.

## Why

DingTalk is the primary enterprise communication platform in China. Its robot webhook API is simple and well-documented, making it a good DOT transport carrier for enterprise use cases.

## External Dependencies

```toml
[dependencies]
# DingTalk API
reqwest = { version = "0.12", features = ["json"] }
```

## Acceptance Criteria

- [x] New crate: `crates/octo-adapter-dingtalk/`
- [x] `DingTalkConfig`: `webhook_url`, `secret` (optional, for signed webhooks), `groups` (group IDs)
- [x] Implements `PlatformAdapter` trait with all methods
- [x] `send_envelope()` — sends DOT envelope via DingTalk robot webhook (text, max 20000 chars)
- [x] `receive_messages()` — receives via webhook callback (DingTalk pushes messages)
- [x] `canonicalize()` — extracts DOT envelope from message content
- [x] `capabilities()`: max_payload=20000 characters, supports_fragmentation=false, media_capabilities=None (robot webhook only supports text/markdown)
- [x] `self_handle()` — returns robot's webhook ID
- [x] `shutdown()` — clears session webhooks
- [x] Auth via webhook URL (no OAuth needed for robot)
- [x] Signed webhook: HMAC-SHA256 with timestamp
- [x] Rate limiting: respect DingTalk rate limits (20 messages/min per group)
- [x] Per-message session webhooks (DingTalk provides unique webhook per incoming message)
- [x] Domain hash: `BLAKE3-256("dingtalk:{group_id}")`
- [x] PlatformType: `0x0012` (new allocation)
- [x] Unit tests: 10+ tests

## Complexity

Medium

## Prerequisites

- Mission 0850e: DOT Adapter Registry & Plugin ABI

## Implementation Notes

- DingTalk Robot Webhook: `POST https://oapi.dingtalk.com/robot/send?access_token=X`
- Message types: text, markdown, actionCard
- Signed webhook: `sign = Base64(HMAC-SHA256(timestamp + "\n" + secret))`
- Incoming messages: DingTalk pushes to configured callback URL
- Session webhooks: each incoming message carries a unique reply webhook URL
- Rate limits: 20 messages per minute per group
- ZeroClaw reference: `zeroclaw/crates/zeroclaw-channels/src/dingtalk.rs` (542 lines)
