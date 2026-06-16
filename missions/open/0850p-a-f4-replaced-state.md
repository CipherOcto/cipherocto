# Mission: 0850p-a F4 — explicit Replaced state in BotLifecycle

## Status

Open (2026-06-16) — pre-public-launch follow-up

## RFC

RFC-0850p-a (Networking): WhatsApp Auth Onboarding — §"Future Work" F4

## Summary

Add an explicit `Replaced` state to the `BotLifecycle` state machine, distinct from `LoggedOut`. Currently, when the operator pairs a competing device (e.g., a new tablet) from the same phone, the adapter receives `Event::LoggedOut` and the CLI exits 2 with the message "session logged out" — but the actual cause is "replaced by another device". Distinguishing the two allows recovery automation (re-pair) to react differently than a true logout (operator must investigate).

## Design

In `crates/octo-whatsapp-onboard-core/src/session.rs`, add a new state:

```rust
pub enum BotState {
    Disconnected,
    PairingQr,
    PairingCode,
    Connected,
    Replaced,  // NEW: distinct from LoggedOut
    LoggedOut,
    SessionExpired,
}
```

The `Event::LoggedOut` handler checks the `cause` field (whatsapp-rust exposes `LoggedOutCause::Replaced` or `LoggedOutCause::LoggedOut`); routes `Replaced` to the new state, leaves other causes mapped to `LoggedOut`. The `whoami --detect-replacement` subcommand (new) returns exit code 8 on `Replaced`, exit code 7 on `SessionExpired`, exit code 2 on other `LoggedOut`.

## Acceptance Criteria

- [ ] `BotState::Replaced` variant added
- [ ] `Event::LoggedOut { cause: Replaced }` → `BotState::Replaced`
- [ ] `Event::LoggedOut { cause: LoggedOut }` → `BotState::LoggedOut` (unchanged)
- [ ] `whoami --detect-replacement` returns exit code 8 on Replaced
- [ ] State machine unit tests updated; transition table updated
- [ ] Integration test: simulating `Event::LoggedOut { cause: Replaced }` exits with code 8

## Mitigates

D-WA-7 (multi-device pairing does not silently log out the adapter)

## Deadline

Pre-public-launch
