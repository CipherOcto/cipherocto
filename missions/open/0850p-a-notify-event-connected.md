# Mission: 0850p-a — Notify-based Event::Connected observation

## Status

Open (2026-06-16) — pre-public-launch follow-up

## RFC

RFC-0850p-a (Networking): WhatsApp Auth Onboarding — §"Future Work"

## Summary

Replace the 250ms polling loop in `wait_for_connected` with a `tokio::sync::Notify`-based signal from the adapter. Currently the adapter's `self_phone` field is a `parking_lot::Mutex<Option<String>>` with no signal exposed; the CLI polls until it sees a value. Adding a `Notify` removes the polling cost (negligible at 250ms but unnecessary).

## Design

In `crates/octo-adapter-whatsapp/src/adapter.rs`:

```rust
pub struct WhatsAppWebAdapter {
    // existing fields...
    pub(crate) connected_notify: Arc<tokio::sync::Notify>,
}

impl WhatsAppWebAdapter {
    pub fn connected(&self) -> tokio::sync::Notify {  // returns a clonable handle
        (*self.connected_notify).clone()
    }
}

// In the Event::Connected handler:
self.self_phone.lock().replace(device.pn);
self.connected_notify.notify_waiters();
```

In `crates/octo-whatsapp-onboard-core/src/session.rs`:

```rust
pub async fn wait_for_connected(adapter: &WhatsAppWebAdapter, timeout: Duration) -> Result<String, CoreError> {
    let notify = adapter.connected();
    let check = async {
        notify.notified().await;
        adapter.self_handle().ok_or(CoreError::NoSession)
    };
    match tokio::time::timeout(timeout, check).await {
        Ok(Ok(phone)) => Ok(phone),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(CoreError::Timeout),
    }
}
```

## Acceptance Criteria

- [ ] `WhatsAppWebAdapter` exposes a `connected()` method returning a clonable `Notify`
- [ ] `Event::Connected` handler calls `notify_waiters()`
- [ ] `wait_for_connected` uses `Notify` instead of polling
- [ ] Cross-crate refactor: `octo-adapter-whatsapp` gains a new public API; `octo-whatsapp-onboard-core` switches to it
- [ ] Unit test: `wait_for_connected` returns immediately on a pre-set `Notify`
- [ ] Integration test: full `qr-link` flow completes in <2s after `Event::Connected` (vs. current 250ms-2s lag)

## Dependencies

Depends on the base 0850p-a RFC being Accepted. No prerequisite missions; this is a cross-crate refactor of the adapter's `Connected` signaling.

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)

## Location

`crates/octo-adapter-whatsapp/src/lib.rs` (add `tokio::sync::watch`); `crates/octo-whatsapp-onboard-core/src/wait.rs` (add `wait_for_connected`).

## Complexity

Medium (~150 lines; cross-crate refactor of the `Connected` signal).

## Prerequisites

- RFC-0850p-a status: Accepted
- All callers of the old polling API must be updated in the same PR

## Notes

### Why `watch` and not `Notify`?

`watch` allows multiple consumers to see the latest value and wait for the next change. `Notify` is one-shot. The adapter needs `watch` because `whoami` and the QR-handler both want to know the current state.

### Why a cross-crate refactor?

The polling API is used by both `octo-adapter-whatsapp` (long-running) and `octo-whatsapp-onboard-core` (one-shot). Changing the signal type requires updating both crates in lockstep.

### Type Coverage

| RFC-0850p-a Type | Implemented By |
|-----------------|----------------|
| `tokio::sync::watch` (or `Notify`) channel in `WhatsAppWebAdapter` | This mission |
| `wait_for_connected()` async function | This mission |

### Implementation Guide

Reference: `crates/octo-adapter-whatsapp/src/lib.rs` (existing `Event` stream); `tokio::sync::watch` documentation.

## Mitigates

Performance optimization; replaces 250ms polling.

## Deadline

Post-launch (cross-crate refactor)
