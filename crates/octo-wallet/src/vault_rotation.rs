//! Provider key rotation event flow (mission 0957-b AC-7).
//!
//! Per mission 0957-b AC-7: provider key rotation event flow works; old
//! caps expire within 1h grace.
//!
//! Flow:
//! 1. Operator rotates provider key in vault (writes new slot, marks old
//!    slot as `revoked_at_unix`).
//! 2. `on_rotation(slot_id)` returns `RotationEvent` describing the
//!    rotation (old_slot_id, new_slot_id, rotated_at_unix, grace_until_unix).
//! 3. Marketplace subscribes; invalidates ASKs referencing old slot.
//! 4. Active capabilities bound to old slot: 1h grace; new mints rejected
//!    post-grace.
//!
//! The marketplace listens via `RotationListener` trait; the canonical
//! implementation invalidates ASKs whose `provider_slot_id` matches the
//! rotated-out slot.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Provider key rotation event (mission 0957-b AC-7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RotationEvent {
    /// Provider slot that was rotated out (now revoked).
    pub old_slot_id: String,
    /// New provider slot (active going forward).
    pub new_slot_id: String,
    /// Unix timestamp at which the rotation occurred.
    pub rotated_at_unix: u64,
    /// Unix timestamp at which the old slot becomes invalid for new mints.
    /// Per mission 0957-b AC-7: 1h after `rotated_at_unix`.
    pub grace_until_unix: u64,
}

impl RotationEvent {
    /// Construct a RotationEvent with the canonical 1h grace window.
    #[must_use]
    pub fn new(
        old_slot_id: impl Into<String>,
        new_slot_id: impl Into<String>,
        rotated_at_unix: u64,
    ) -> Self {
        const GRACE_SECS: u64 = 3600;
        Self {
            old_slot_id: old_slot_id.into(),
            new_slot_id: new_slot_id.into(),
            rotated_at_unix,
            grace_until_unix: rotated_at_unix.saturating_add(GRACE_SECS),
        }
    }

    /// Returns true iff the current time is past the grace window.
    /// Old caps bound to the rotated-out slot must be rejected for new
    /// mints after this point.
    #[must_use]
    pub fn grace_expired(&self, current_unix: u64) -> bool {
        current_unix >= self.grace_until_unix
    }

    /// Returns true iff a mint attempt with `now_unix` should be rejected
    /// for the old slot (post-grace).
    #[must_use]
    pub fn rejects_mint(&self, old_slot_id: &str, now_unix: u64) -> bool {
        old_slot_id == self.old_slot_id && self.grace_expired(now_unix)
    }
}

/// Rotation listener trait (marketplace implements).
///
/// Called by vault when a rotation event fires. Implementations MUST
/// invalidate state referencing the rotated-out slot (per AC-7: invalidates
/// ASKs referencing old slot).
pub trait RotationListener: Send + Sync + std::fmt::Debug {
    /// Handle a rotation event. Returns the number of invalidated entries.
    fn on_rotation(&self, event: &RotationEvent) -> usize;
}

/// Rotation tracker — collects rotation events + broadcasts to listeners.
///
/// Canonical usage: vault calls `tracker.publish(event)` after `on_rotation`.
/// Marketplace subscribes by adding itself as a listener.
#[derive(Debug, Default)]
pub struct RotationTracker {
    events: RwLock<Vec<RotationEvent>>,
    listeners: RwLock<Vec<Arc<dyn RotationListener>>>,
}

impl RotationTracker {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Publish a rotation event; invokes all listeners.
    /// Returns the total number of invalidated entries across listeners.
    pub fn publish(&self, event: RotationEvent) -> usize {
        let mut total_invalidated = 0;
        for listener in self.listeners.read().unwrap().iter() {
            total_invalidated += listener.on_rotation(&event);
        }
        self.events.write().unwrap().push(event);
        total_invalidated
    }

    /// Subscribe a listener.
    pub fn subscribe(&self, listener: Arc<dyn RotationListener>) {
        self.listeners.write().unwrap().push(listener);
    }

    /// Returns the rotation events recorded so far (most recent last).
    #[must_use]
    pub fn events(&self) -> Vec<RotationEvent> {
        self.events.read().unwrap().clone()
    }

    /// Returns the most recent rotation event affecting `slot_id`, if any.
    /// Returns None if no rotation affected this slot.
    #[must_use]
    pub fn rotation_for(&self, slot_id: &str) -> Option<RotationEvent> {
        self.events
            .read()
            .unwrap()
            .iter()
            .rev()
            .find(|e| e.old_slot_id == slot_id || e.new_slot_id == slot_id)
            .cloned()
    }

    /// Returns true iff a mint for `slot_id` should be rejected at `now_unix`.
    /// True if the slot was rotated-out AND the grace window has expired.
    #[must_use]
    pub fn rejects_mint(&self, slot_id: &str, now_unix: u64) -> bool {
        match self.rotation_for(slot_id) {
            Some(event) => event.rejects_mint(slot_id, now_unix),
            None => false,
        }
    }
}

/// Simple listener that maintains a count of invalidated entries per slot
/// (for testing + simple use cases).
#[derive(Debug, Default)]
pub struct CountingListener {
    invalidated: RwLock<HashMap<String, usize>>,
}

impl CountingListener {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the count of invalidated entries for `slot_id`.
    #[must_use]
    pub fn invalidated_for(&self, slot_id: &str) -> usize {
        self.invalidated
            .read()
            .unwrap()
            .get(slot_id)
            .copied()
            .unwrap_or(0)
    }
}

impl RotationListener for CountingListener {
    fn on_rotation(&self, event: &RotationEvent) -> usize {
        let mut counts = self.invalidated.write().unwrap();
        *counts.entry(event.old_slot_id.clone()).or_insert(0) += 1;
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grace_window_is_one_hour() {
        let event = RotationEvent::new("old-slot", "new-slot", 1_700_000_000);
        assert_eq!(event.grace_until_unix, 1_700_000_000 + 3600);
    }

    #[test]
    fn grace_expired_returns_false_within_window() {
        let event = RotationEvent::new("old", "new", 1_700_000_000);
        assert!(!event.grace_expired(1_700_000_000 + 100));
        assert!(!event.grace_expired(1_700_000_000 + 3599));
    }

    #[test]
    fn grace_expired_returns_true_at_or_after_window() {
        let event = RotationEvent::new("old", "new", 1_700_000_000);
        assert!(event.grace_expired(1_700_000_000 + 3600));
        assert!(event.grace_expired(1_700_000_000 + 3601));
    }

    #[test]
    fn rejects_mint_only_for_old_slot_post_grace() {
        let event = RotationEvent::new("rotated-out", "new-active", 1_700_000_000);
        // Before grace: mints allowed.
        assert!(!event.rejects_mint("rotated-out", 1_700_000_000 + 100));
        // After grace: mints rejected for OLD slot.
        assert!(event.rejects_mint("rotated-out", 1_700_000_000 + 3600));
        // New slot always allowed.
        assert!(!event.rejects_mint("new-active", 1_700_000_000 + 3600));
        // Different slot (not the rotated one) allowed.
        assert!(!event.rejects_mint("unrelated-slot", 1_700_000_000 + 3600));
    }

    #[test]
    fn tracker_publishes_and_invents_listeners() {
        let tracker = RotationTracker::new();
        let listener = Arc::new(CountingListener::new());
        tracker.subscribe(listener.clone());

        let event = RotationEvent::new("slot-a", "slot-b", 1_700_000_000);
        let invalidated = tracker.publish(event.clone());
        assert_eq!(invalidated, 1);
        assert_eq!(listener.invalidated_for("slot-a"), 1);
        assert_eq!(tracker.events().len(), 1);
    }

    #[test]
    fn tracker_rejects_mint_after_grace() {
        let tracker = RotationTracker::new();
        let event = RotationEvent::new("slot-a", "slot-b", 1_700_000_000);
        tracker.publish(event);

        // Within grace: mints allowed.
        assert!(!tracker.rejects_mint("slot-a", 1_700_000_000 + 100));
        // After grace: mints rejected for old slot.
        assert!(tracker.rejects_mint("slot-a", 1_700_000_000 + 3600));
        // New slot: always allowed.
        assert!(!tracker.rejects_mint("slot-b", 1_700_000_000 + 3600));
    }

    #[test]
    fn tracker_multiple_listeners_all_invoke() {
        let tracker = RotationTracker::new();
        let l1 = Arc::new(CountingListener::new());
        let l2 = Arc::new(CountingListener::new());
        tracker.subscribe(l1.clone());
        tracker.subscribe(l2.clone());

        let event = RotationEvent::new("slot-x", "slot-y", 1_700_000_000);
        let invalidated = tracker.publish(event);
        assert_eq!(invalidated, 2); // both listeners
        assert_eq!(l1.invalidated_for("slot-x"), 1);
        assert_eq!(l2.invalidated_for("slot-x"), 1);
    }

    #[test]
    fn tracker_rotation_for_returns_most_recent() {
        let tracker = RotationTracker::new();
        tracker.publish(RotationEvent::new("slot-a", "slot-b", 1_700_000_000));
        tracker.publish(RotationEvent::new("slot-b", "slot-c", 1_700_003_600));
        let event = tracker.rotation_for("slot-b").unwrap();
        assert_eq!(event.old_slot_id, "slot-b");
        assert_eq!(event.new_slot_id, "slot-c");
    }

    #[test]
    fn tracker_rotation_for_unknown_returns_none() {
        let tracker = RotationTracker::new();
        assert!(tracker.rotation_for("unknown-slot").is_none());
    }
}
