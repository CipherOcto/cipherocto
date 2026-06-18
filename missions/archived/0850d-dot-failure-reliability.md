# Mission: DOT Failure Domains and Reliability

## Status

Implemented (205 lines, 9 tests, exponential backoff, RetryConfig, jitter)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §11, §12, §13

## Summary

Implement Byzantine transport tolerance, platform partition handling, gateway failure recovery, and token economics integration for DOT.

## Acceptance Criteria

- [ ] Byzantine transport assumption: duplication, reordering, censorship, mutation tolerated
- [ ] Mutation detection via signature verification at every gateway
- [ ] Platform partition: automatic rerouting through remaining carriers
- [ ] Gateway failure: gateway replacement via GDP discovery
- [ ] Graceful shutdown: pending messages flushed before adapter teardown
- [ ] Exponential backoff utility: shared `compute_backoff_delay(base, attempt, max, jitter)` for all adapters
- [ ] Structured logging: all adapters log with module path, action, outcome, and JSON attributes
- [ ] Token economics: OCTO-B for relay bandwidth, OCTO-O for coordination, OCTO-N for uptime, OCTO-S for storage
- [ ] Gateway earnings: per validated relay, uptime, deterministic delivery, anti-censorship routing
- [ ] Carrier premium structure (base rate for NativeP2P, 1.2x for Matrix/Nostr, 1.5x for Telegram/Discord/Slack, 1.0x for IRC/Webhook/WebRTC, 2.0x for Signal/WhatsApp, 3.0x for LoRa/Bluetooth)
- [ ] Unit tests: 8+ tests covering Byzantine tolerance, partition recovery, token accounting
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/dot/mod.rs` (reliability extensions), `crates/octo-network/src/dot/economics.rs`

## Complexity

High

## Prerequisites

- Mission 0850: DOT Core Envelope and Native P2P
- Mission 0850b: DOT Gateway Federation
- Mission 0851: GDP Gateway Discovery

## Implementation Notes

- Byzantine tolerance is inherent in the design (verify everything, trust nothing)
- Platform partition = automatic failover, not manual intervention
- Token economics integration requires OCTO-B/N/S token contracts
- Carrier premiums reflect actual cost differences

## Reference

- RFC-0850 §11: Reliability Model
- RFC-0850 §12: Failure Domains
- RFC-0850 §13: Token Economics Integration
