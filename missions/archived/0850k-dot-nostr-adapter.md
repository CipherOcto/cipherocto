# Mission: DOT Nostr Adapter

## Status

Implemented (13 tests)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.1

## Summary

Implement a Nostr relay adapter as a `cdylib` plugin. Nostr is censorship-resistant with relay federation — aligned with CipherOcto's decentralized model.

## Acceptance Criteria

- [x] `crates/octo-adapter-nostr/` crate compiles to `cdylib`
- [x] Implements `PlatformAdapter` trait with all methods (6 required + 3 optional: replay_protection, health_check, shutdown)
- [x] `send_envelope()` publishes Nostr event to configured relays
- [x] `receive_messages()` subscribes to relay filters (NIP-01)
- [x] `canonicalize()` extracts envelope from Nostr event content
- [x] Multi-relay propagation: same event published to N relays for redundancy
- [x] `CapabilityReport`: max_payload=65536 (NIP-01 event limit), rate_limit=10/sec
- [x] `domain_id()`: `BroadcastDomainId(0x0004, BLAKE3(relay_url:channel_tag))`
- [x] Config: `relays` (list of relay URLs), `private_key`, `channel_tags`
- [x] Unit tests with mock relay responses (13 tests)

## Location

`crates/octo-adapter-nostr/`

## Complexity

Medium

## Prerequisites

- Mission 0850e: DOT Adapter Registry & Plugin ABI

## Implementation Notes

- Nostr protocol: NIP-01 for basic relay communication (NOT NIP-04/NIP-17 DMs — DOT has its own encryption via RFC-0853)
- Events: custom kind (e.g., 30078) for CipherOcto envelopes, tagged with `["cipherocto", "<mission_id>"]`
- Relay federation: publish to multiple relays, subscribe to all — natural redundancy
- WebSocket: each relay connection is a persistent WS connection with exponential backoff reconnect
- Key management: Nostr keypair can be derived from gateway identity key

## Additional Requirements (from Audit)

- [ ] Implement `self_handle()` for relay loop prevention (see Mission 0850s)
- [ ] Implement `shutdown()` for graceful cleanup (see Mission 0850t)
- [ ] Add tests to match ZeroClaw coverage (see Mission 0850u)
