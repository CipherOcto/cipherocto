# Mission: DOT Signal Adapter

## Status

Implemented (6 tests, signal-cli bridge, retry/backoff)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.1

## Summary

Implement a Signal adapter using `signal-cli` as a bridge process. Signal provides end-to-end encryption with no backdoor access — the strongest privacy guarantee of any Tier 2 platform.

## Acceptance Criteria

- [ ] `crates/octo-adapter-signal/` crate manages `signal-cli` subprocess
- [ ] Implements `PlatformAdapter` trait with all methods (6 required + 3 optional: replay_protection, health_check, shutdown)
- [ ] `send_envelope()` sends via `signal-cli send` command
- [ ] `receive_messages()` reads from `signal-cli receive` stream
- [ ] `canonicalize()` extracts envelope from Signal message body
- [ ] `CapabilityReport`: max_payload=65536, rate_limit=5/sec
- [ ] `domain_id()`: `BroadcastDomainId(0x0005, BLAKE3(group_id))`
- [ ] Config: `signal_cli_path`, `phone_number`, `groups`
- [ ] Error handling: signal-cli crash recovery, registration expiry
- [ ] Unit tests with mock signal-cli output

## Location

`crates/octo-adapter-signal/`

## Complexity

High

## Prerequisites

- Mission 0850e: DOT Adapter Registry & Plugin ABI

## Implementation Notes

- `signal-cli` is a Java application — must be installed separately
- Communication: stdin/stdout JSON-RPC or daemon mode (`signal-cli daemon`)
- Registration: requires phone number for initial setup, then can operate headless
- Groups: Signal groups have UUID-based identifiers
- Attachment support: `signal-cli send -a <file>` for large envelopes
- Bridge architecture: the adapter spawns `signal-cli daemon` and communicates via Unix socket

## Additional Requirements (from Audit)

- [ ] Implement `self_handle()` for relay loop prevention (see Mission 0850s)
- [ ] Implement `shutdown()` for graceful cleanup (see Mission 0850t)
- [ ] Add tests to match ZeroClaw coverage (see Mission 0850u)
