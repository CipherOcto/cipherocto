# Mission: DOT QQ Adapter (PlatformType 0x0014)

## Status

Open

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

- [ ] New crate: `crates/octo-adapter-qq/`
- [ ] `QQConfig`: `app_id`, `app_secret`, `groups` (group IDs), `sandbox` (bool, for testing)
- [ ] Implements `PlatformAdapter` trait with all methods
- [ ] `send_envelope()` — sends DOT envelope via QQ bot message API (text, max 2000 chars)
- [ ] `receive_messages()` — receives via webhook callback (QQ pushes events)
- [ ] `canonicalize()` — extracts DOT envelope from message content
- [ ] `capabilities()`: max_payload=2000, supports_fragmentation=true
- [ ] `self_handle()` — returns bot's Open ID
- [ ] `shutdown()` — clears access token cache
- [ ] Auth via access_token (auto-refresh, 2h expiry)
- [ ] Webhook verification: signature validation
- [ ] Rate limiting: respect QQ rate limits (5 messages/sec per group)
- [ ] Sandbox mode: `sandbox.api.sgroup.qq.com` for testing
- [ ] Domain hash: `BLAKE3-256("qq:{group_id}")`
- [ ] PlatformType: `0x0014` (new allocation)
- [ ] Unit tests: 10+ tests

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
