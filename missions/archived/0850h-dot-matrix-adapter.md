# Mission: DOT Matrix Adapter

## Status

Implemented (9 tests, retry/backoff, wire_bytes fix, health_check)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.1

## Summary

Implement a pure Rust Matrix Client-Server API adapter as a `cdylib` plugin. Matrix is the most aligned platform for CipherOcto — it is itself federated and decentralized.

## Design

See `docs/plans/2026-05-28-social-platform-transport-adapters-design.md` — Matrix Adapter.

## Acceptance Criteria

- [ ] `crates/octo-adapter-matrix/` crate compiles to `cdylib`
- [ ] Implements `PlatformAdapter` trait with all methods (6 required + 3 optional: replay_protection, health_check, shutdown)
- [ ] `send_envelope()` calls `PUT /rooms/{roomId}/send/m.room.message/{txnId}`
- [ ] `receive_messages()` uses `GET /sync` with `since` token for incremental sync
- [ ] `canonicalize()` extracts envelope from Matrix event content
- [ ] Fragmentation: rarely needed (65KB limit), media upload for larger payloads
- [ ] `CapabilityReport`: max_payload=65536, rate_limit=100/sec
- [ ] `domain_id()`: `BroadcastDomainId(0x0003, BLAKE3(room_id))`
- [ ] Config: `homeserver_url`, `access_token`, `rooms` (list of room IDs/aliases)
- [ ] Error handling: token expiry, homeserver unreachable, rate limiting
- [ ] Exponential backoff with jitter: initial=1s, max=120s, jitter=0-500ms
- [ ] Unit tests with mock HTTP responses
- [ ] Integration test against Synapse/Conduit test homeserver

## Location

`crates/octo-adapter-matrix/`

## Complexity

Medium

## Prerequisites

- Mission 0850e: DOT Adapter Registry & Plugin ABI

## Implementation Notes

- Matrix room IDs: `!abcdef:example.com` — include server name in hash for federation
- Sync endpoint: long-polling with `timeout=30000ms`
- Event type: use custom `m.room.message` with `msgtype: m.text` and base64 content
- Alternative: use custom event type `io.cipherocto.envelope` for cleaner separation
- Federation: messages propagate across homeservers automatically — no extra work needed
- Media upload: `POST /_matrix/media/v3/upload` for large envelopes, then send event with `m.file` content
- Access token: long-lived, obtained via login or registration API

## Additional Requirements (from Audit)

- [ ] Implement `self_handle()` for relay loop prevention (see Mission 0850s)
- [ ] Implement `shutdown()` for graceful cleanup (see Mission 0850t)
- [ ] Add tests to match ZeroClaw coverage (see Mission 0850u)
