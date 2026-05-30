# Mission: DOT Reddit Adapter (PlatformType 0x0010)

## Status

Open

## RFC

RFC-0850: Deterministic Transport (DOT) — §8.1

## Summary

Implement a Reddit adapter for DOT transport. Reddit's subreddit structure provides natural broadcast domains for DOT message routing.

## Why

Reddit has 1.5B+ monthly visitors and subreddit-based organization that maps well to DOT broadcast domains. Posts/comments provide persistent, threaded transport.

## External Dependencies

```toml
[dependencies]
# Reddit API (OAuth2)
reqwest = { version = "0.12", features = ["json"] }
```

## Acceptance Criteria

- [ ] New crate: `crates/octo-adapter-reddit/`
- [ ] `RedditConfig`: `client_id`, `client_secret`, `refresh_token`, `subreddits` (list of subreddit names)
- [ ] Implements `PlatformAdapter` trait with all methods
- [ ] `send_envelope()` — posts DOT envelope as a Reddit post or comment (max 10000 chars)
- [ ] `receive_messages()` — polls `GET /r/{subreddit}/new` for new posts/comments
- [ ] `canonicalize()` — extracts DOT envelope from post body
- [ ] `capabilities()`: max_payload=10000, supports_fragmentation=false
- [ ] `self_handle()` — returns bot's Reddit username
- [ ] `shutdown()` — clears cached access token
- [ ] Auth via OAuth2 (client credentials + refresh token)
- [ ] Rate limiting: respect Reddit rate limits (60 requests/min)
- [ ] Domain hash: `BLAKE3-256("reddit:{subreddit}")`
- [ ] PlatformType: `0x0010` (new allocation)
- [ ] Unit tests: 10+ tests

## Complexity

Medium

## Prerequisites

- Mission 0850e: DOT Adapter Registry & Plugin ABI

## Implementation Notes

- Reddit API: `https://oauth.reddit.com/`
- Auth: OAuth2 with `client_id`, `client_secret`, `refresh_token`
- Access token: `POST https://www.reddit.com/api/v1/access_token`
- Post: `POST /api/submit` with `{ "sr": "subreddit", "title": "...", "text": "DOT/1/..." }`
- Poll: `GET /r/{subreddit}/new?limit=25`
- Rate limits: 60 requests per minute (OAuth)
- Max post body: 10,000 characters (rarely needs fragmentation)
- ZeroClaw reference: `zeroclaw/crates/zeroclaw-channels/src/reddit.rs` (560 lines)
