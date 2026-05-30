# Mission: DOT WeChat Adapter (PlatformType 0x0011)

## Status

Open

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.1

## Summary

Implement a WeChat adapter for DOT transport. WeChat has 1.3B+ users and is the dominant messaging platform in China. Access requires WeChat Official Account or WeCom integration.

## Why

WeChat is essential for reaching Chinese users. The WeChat Official Account API provides messaging capabilities, though with significant restrictions (24h response window, template messages).

## External Dependencies

```toml
[dependencies]
# AES encryption for WeChat message encryption
aes = "0.8"
ecb = "0.1"
# WeChat API
reqwest = { version = "0.12", features = ["json"] }
```

## Acceptance Criteria

- [ ] New crate: `crates/octo-adapter-wechat/`
- [ ] `WeChatConfig`: `app_id`, `app_secret`, `token` (verification token), `encoding_aes_key`, `groups` (group chat IDs)
- [ ] Implements `PlatformAdapter` trait with all methods
- [ ] `send_envelope()` — sends DOT envelope via WeChat API (text message, max 2048 chars)
- [ ] `receive_messages()` — receives via webhook callback (WeChat pushes messages)
- [ ] `canonicalize()` — extracts DOT envelope from message content
- [ ] `capabilities()`: max_payload=2048 characters, supports_fragmentation=true, media_capabilities=Some (images via media upload API)
- [ ] `media_capabilities`: max_upload_bytes=10485760 (10MB image), supported_mime_types=["image/jpeg", "image/png"]
- [ ] `self_handle()` — returns bot's WeChat OpenID
- [ ] `shutdown()` — clears session
- [ ] Auth via access_token (2h expiry, auto-refresh)
- [ ] Message encryption: AES-256-CBC (WeChat requires encrypted messages)
- [ ] Webhook verification: echostr challenge-response
- [ ] Rate limiting: respect WeChat API limits
- [ ] Domain hash: `BLAKE3-256("wechat:{group_id_or_openid}")`
- [ ] PlatformType: `0x0011` (new allocation)
- [ ] Unit tests: 10+ tests

## Complexity

High (encryption, webhook, 24h response window)

## Prerequisites

- Mission 0850e: DOT Adapter Registry & Plugin ABI

## Implementation Notes

- WeChat API: `https://api.weixin.qq.com/cgi-bin/`
- Access token: `GET /token?grant_type=client_credential&appid=X&secret=X`
- Send message: `POST /message/custom/send`
- Message encryption: AES-256-CBC with PKCS7 padding
- Webhook: WeChat pushes messages to configured URL
- 24h response window: can only reply within 24h of last user message
- Group chat: requires WeChat Work (WeCom) for group messaging
- ZeroClaw reference: `zeroclaw/crates/zeroclaw-channels/src/wechat.rs` (2833 lines)
