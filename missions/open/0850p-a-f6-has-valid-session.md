# Mission: 0850p-a F6 — adapter-side has_valid_session() helper

## Status

Open (2026-06-16) — pre-public-launch follow-up

## RFC

RFC-0850p-a (Networking): WhatsApp Auth Onboarding — §"Future Work" F6

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

## Mitigates

Performance optimization; not a security issue. Replaces the R8-H1 polling-loop pattern.

## Deadline

Post-launch
