# Mission: DOT Twitter/X Adapter (PlatformType 0x000F)

Implemented (441 lines, 10 tests)

## Claimant

@agent (Jcode)


## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.1

## Summary

Implement a Twitter/X adapter for DOT transport using the Twitter API v2. Twitter has massive reach (500M+ users) and is a key platform for public DOT message dissemination.

## Why

Twitter/X is the largest public social platform. While API access has restrictions, it remains critical for reaching users who don't install specialized software.

## External Dependencies

```toml
[dependencies]
# Twitter API v2 (bearer token auth)
reqwest = { version = "0.12", features = ["json"] }
```

## Acceptance Criteria

- [x] New crate: `crates/octo-adapter-twitter/`
- [x] `TwitterConfig`: `bearer_token`, `account_id` (optional), `poll_interval_secs`
- [x] Implements `PlatformAdapter` trait with all methods
- [x] `send_envelope()` — posts DOT envelope as a tweet (max 280 chars)
- [x] `receive_messages()` — polls `GET /2/users/:id/mentions` or search
- [x] `canonicalize()` — extracts DOT envelope from tweet text
- [x] `capabilities()`: max_payload=280 characters (~206 bytes base64), supports_fragmentation=true, media_capabilities=Some (images/media via `media/upload`)
- [x] `media_capabilities`: max_upload_bytes=5242880 (5MB image), supported_mime_types=["image/jpeg", "image/png", "image/gif", "image/webp"]
- [x] `self_handle()` — returns bot's Twitter user ID
- [x] `shutdown()` — clears cached state
- [x] Auth via Bearer token (OAuth2)
- [x] Rate limiting: respect Twitter rate limits (300 tweets/3h)
- [x] Domain hash: `BLAKE3-256("twitter:{user_id}")`
- [x] PlatformType: `0x000F` (new allocation)
- [x] Unit tests: 10+ tests

## Complexity

Medium

## Prerequisites

- Mission 0850e: DOT Adapter Registry & Plugin ABI

## Implementation Notes

- Twitter API v2: `https://api.x.com/2/`
- Auth: Bearer token in Authorization header
- Post tweet: `POST /2/tweets` with `{ "text": "DOT/1/..." }`
- Poll mentions: `GET /2/users/:id/mentions`
- Rate limits: 300 tweets per 3 hours (app-level)
- Max tweet length: 280 characters (may need fragmentation)
- ZeroClaw reference: `zeroclaw/crates/zeroclaw-channels/src/twitter.rs` (541 lines)
