# Mission: 0850p-a — adapter-side has_valid_session() helper

## Status

Open (2026-06-16) — pre-public-launch follow-up

## RFC

RFC-0850p-a (Networking): WhatsApp Auth Onboarding — §"Future Work"

## Summary

Add a `pub fn has_valid_session(&self) -> bool` method to `WhatsAppWebAdapter` that returns `true` if a valid session exists (bot handle present and `self_handle().is_some()`). The `whoami` subcommand currently polls `self_handle()` at 250ms intervals (R8 hardcoded timeout 30s); this helper allows the polling loop to be replaced with a single check, reducing CPU usage and the wait latency for an already-paired bot.

## Design

In `crates/octo-adapter-whatsapp/src/adapter.rs`, add:

```rust
impl WhatsAppWebAdapter {
    pub fn has_valid_session(&self) -> bool {
        self.self_handle().is_some() && self.bot_handle.is_some()
    }
}
```

In `crates/octo-whatsapp-onboard/src/whoami.rs`, replace the polling loop with a single `adapter.has_valid_session()` check at startup, then proceed directly to verifying the WS connection (the existing 30s timeout is for the WS reconnect, not the session check).

## Acceptance Criteria

- [ ] `WhatsAppWebAdapter::has_valid_session()` returns `true` iff both `self_handle()` and `bot_handle` are present
- [ ] `whoami` uses `has_valid_session()` for the initial check; if `false`, exits 1 ("not paired; run `qr-link` or `pair-link`")
- [ ] Polling loop removed; `whoami` latency for an already-paired bot drops from 30s to <2s
- [ ] Unit test: `has_valid_session` returns `false` when only one of the two handles is set
- [ ] Integration test: `whoami` exits 0 in <2s for an already-paired bot

## Dependencies

Depends on the base 0850p-a RFC being Accepted. No prerequisite missions; this is a small adapter API addition.

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)

## Location

`crates/octo-adapter-whatsapp/src/lib.rs` (add `has_valid_session` method).

## Complexity

Trivial (~20 lines; one new method).

## Prerequisites

- RFC-0850p-a status: Accepted

## Notes

### Why a new method instead of polling?

The 250ms polling pattern in `whoami` is wasteful. `has_valid_session()` can use the adapter's internal state (which already knows the session validity) to return a synchronous boolean.

### Why purely additive?

This change is purely additive — no existing API is removed. The polling path is kept as a fallback for back-compat with operators that rely on it.

### Type Coverage

| RFC-0850p-a Type | Implemented By |
|-----------------|----------------|
| `WhatsAppWebAdapter::has_valid_session()` | This mission |
| `whoami` switches from polling to `has_valid_session` | This mission |

### Implementation Guide

Reference: `crates/octo-adapter-whatsapp/src/lib.rs` (existing `WhatsAppWebAdapter` struct).

## Mitigates

Performance optimization; not a security issue. Replaces the R8-H1 polling-loop pattern.

## Deadline

Post-launch
