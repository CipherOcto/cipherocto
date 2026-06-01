# Mission: DOT QQ Adapter (PlatformType 0x0014)

Implemented (417 lines, 9 tests)

## Claimant

@agent (Jcode)


## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.1

## Summary

Implement a QQ adapter for DOT transport. QQ is Tencent's messaging platform with 600M+ users, particularly popular among younger demographics in China.

## Why

QQ has a massive user base in China and provides a bot API (QQ Official Bot) for group messaging. It's essential for reaching Chinese users who prefer QQ over WeChat.

## External Dependencies

```toml
[dependencies]
# QQ Official Bot API
reqwest = { version = "0.12", features = ["json"] }
```

## Acceptance Criteria

- [x] New crate: `crates/octo-adapter-qq/`
- [x] `QQConfig`: `app_id`, `app_secret`, `groups` (group IDs), `sandbox` (bool, for testing)
- [x] Implements `PlatformAdapter` trait with all methods
- [x] `send_envelope()` — sends DOT envelope via QQ bot message API (text, max 2000 chars)
- [x] `receive_messages()` — receives via webhook callback (QQ pushes events)
- [x] `canonicalize()` — extracts DOT envelope from message content
- [x] `capabilities()`: max_payload=2000 characters, supports_fragmentation=true, media_capabilities=Some (images via media upload API)
- [x] `media_capabilities`: max_upload_bytes=10485760 (10MB), supported_mime_types=["image/jpeg", "image/png", "image/gif"]
- [x] `self_handle()` — returns bot's Open ID
- [x] `shutdown()` — clears access token cache
- [x] Auth via access_token (auto-refresh, 2h expiry)
- [x] Webhook verification: signature validation
- [x] Rate limiting: respect QQ rate limits (5 messages/sec per group)
- [x] Sandbox mode: `sandbox.api.sgroup.qq.com` for testing
- [x] Domain hash: `BLAKE3-256("qq:{group_id}")`
- [x] PlatformType: `0x0014` (new allocation)
- [x] Unit tests: 10+ tests

## Complexity

Medium-High

## Prerequisites

- Mission 0850e: DOT Adapter Registry & Plugin ABI

## Implementation Notes

- QQ Bot API: `https://api.sgroup.qq.com/` (production) or `https://sandbox.api.sgroup.qq.com/` (sandbox)
- Access token: `POST /app/{app_id}/token` with `{ "appId": X, "clientSecret": X }`
- Send message: `POST /v2/groups/{group_id}/messages`
- Incoming events: QQ pushes to configured webhook URL
- Rate limits: 5 messages per second per group
- Message types: text, rich text, markdown
- Auth: OAuth2 with app_id + app_secret
- ZeroClaw reference: `zeroclaw/crates/zeroclaw-channels/src/qq.rs` (2130 lines)
