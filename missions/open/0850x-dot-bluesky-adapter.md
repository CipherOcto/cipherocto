# Mission: DOT Bluesky Adapter (PlatformType 0x000E)

## Status

Open

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.1

## Summary

Implement a Bluesky (AT Protocol) adapter for DOT transport. Bluesky is a decentralized social network growing rapidly, and its AT Protocol aligns with CipherOcto's decentralized philosophy.

## Why

Bluesky is the fastest-growing decentralized social platform. Its AT Protocol provides built-in federation, making it an ideal DOT transport carrier for censorship-resistant communication.

## External Dependencies

```toml
[dependencies]
# AT Protocol client (Rust)
atrium-api = "0.1"  # Official AT Protocol SDK
```

## Acceptance Criteria

- [ ] New crate: `crates/octo-adapter-bluesky/`
- [ ] `BlueskyConfig`: `handle` (e.g., `alice.bsky.social`), `app_password`, `pds_url` (optional, default: bsky.social)
- [ ] Implements `PlatformAdapter` trait with all methods
- [ ] `send_envelope()` — posts DOT envelope as a Bluesky post (text, max 300 chars)
- [ ] `receive_messages()` — polls `app.bsky.feed.getTimeline` or subscribes to firehose
- [ ] `canonicalize()` — extracts DOT envelope from post text
- [ ] `capabilities()`: max_payload=300 graphemes (~221 bytes base64), supports_fragmentation=true, media_capabilities=Some (images supported via `app.bsky.embed.images`)
- [ ] `media_capabilities`: max_upload_bytes=976563 (1MB image), supported_mime_types=["image/jpeg", "image/png", "image/webp"]
- [ ] `self_handle()` — returns bot's DID or handle
- [ ] `shutdown()` — clears session
- [ ] Auth via app password (OAuth2-like flow with session JWT)
- [ ] Rate limiting: respect Bluesky rate limits (300 requests/5min)
- [ ] Domain hash: `BLAKE3-256("bluesky:{did_or_handle}")`
- [ ] PlatformType: `0x000E` (new allocation)
- [ ] Unit tests: 10+ tests

## Complexity

Medium

## Prerequisites

- Mission 0850e: DOT Adapter Registry & Plugin ABI

## Implementation Notes

- Bluesky uses AT Protocol (XRPC), not REST API
- Auth: create session with `com.atproto.server.createSession`
- Posts: `com.atproto.repo.createRecord` with `app.bsky.feed.post` record type
- Polling: `app.bsky.feed.getTimeline` or `app.bsky.feed.getAuthorFeed`
- Max post length: 300 graphemes (may need fragmentation for larger envelopes)
- DID resolution: `com.atproto.identity.resolveHandle`
