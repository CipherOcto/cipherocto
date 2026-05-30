# Mission: DOT Lark/Feishu Adapter (PlatformType 0x0013)

## Status

Open

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.1

## Summary

Implement a Lark/Feishu adapter for DOT transport. Lark is ByteDance's enterprise platform (Feishu in China), with a comprehensive API for messaging, bots, and group chats.

## Why

Lark/Feishu is a growing enterprise platform with excellent API support. Its bot framework supports rich messages, group chats, and events, making it a strong DOT transport carrier.

## External Dependencies

```toml
[dependencies]
# Lark/Feishu API
reqwest = { version = "0.12", features = ["json"] }
```

## Acceptance Criteria

- [ ] New crate: `crates/octo-adapter-lark/`
- [ ] `LarkConfig`: `app_id`, `app_secret`, `region` (CN or International), `groups` (chat IDs)
- [ ] Implements `PlatformAdapter` trait with all methods
- [ ] `send_envelope()` — sends DOT envelope via Lark bot message API (text, max 30000 chars)
- [ ] `receive_messages()` — receives via webhook callback (Lark pushes events)
- [ ] `canonicalize()` — extracts DOT envelope from message content
- [ ] `capabilities()`: max_payload=30000, supports_fragmentation=false
- [ ] `self_handle()` — returns bot's Open ID
- [ ] `shutdown()` — clears tenant access token cache
- [ ] Auth via tenant_access_token (auto-refresh, 2h expiry)
- [ ] Webhook verification: encrypt key + challenge
- [ ] Rate limiting: respect Lark rate limits (50 messages/sec)
- [ ] Region support: `open.larksuite.com` (International) vs `open.feishu.cn` (China)
- [ ] Domain hash: `BLAKE3-256("lark:{chat_id}")`
- [ ] PlatformType: `0x0013` (new allocation)
- [ ] Unit tests: 10+ tests

## Complexity

Medium-High (dual region, webhook encryption)

## Prerequisites

- Mission 0850e: DOT Adapter Registry & Plugin ABI

## Implementation Notes

- Lark API (International): `https://open.larksuite.com/open-apis/`
- Feishu API (China): `https://open.feishu.cn/open-apis/`
- Tenant access token: `POST /auth/v3/tenant_access_token/internal`
- Send message: `POST /im/v1/messages` with `receive_id_type=chat_id`
- Incoming events: Lark pushes to configured webhook URL
- Webhook verification: AES-CBC decrypt with encrypt key
- Rate limits: 50 messages per second per app
- Message types: text, interactive (card), post (rich text)
- ZeroClaw reference: `zeroclaw/crates/zeroclaw-channels/src/lark.rs` (5131 lines)
