//! In-memory events ring buffer. Phase 3 Part B.
//!
//! Bounded by `max_rows` (default 1_000_000) per design §InboundEvent
//! retention. The `db_writer` task is the sole writer (single-owner
//! pattern from design); the buffer's `parking_lot::Mutex` is held only
//! for the push/list/get operations, never across `.await`.
//!
//! Monotonic ids are assigned at insert time and **stored with the
//! event** so eviction does not corrupt the id-to-position mapping
//! (correctness review F8 — was position-based, broken after eviction).
//! The `since_id` filter on `list()` is what the design §Loss recovery
//! path uses to backfill after `RecvError::Lagged(n)`.
//!
//! Disk persistence is provided by the sibling `events_persister`
//! module. The buffer is the in-memory source of truth; the file is
//! the cold store; reload on boot hydrates from the file via
//! [`EventsBuffer::hydrate_from_entries`].

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;

use crate::events::InboundEvent;

/// One buffer entry — `(assigned_id, event)`. Stored together so the
/// id survives eviction (correctness review F8).
type Entry = (u64, InboundEvent);

#[derive(Debug)]
pub struct EventsBuffer {
    inner: Mutex<VecDeque<Entry>>,
    max_rows: usize,
    next_id: AtomicU64,
    total_evicted: AtomicU64,
    total_pushed: AtomicU64,
}

impl EventsBuffer {
    pub fn new(max_rows: usize) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(VecDeque::with_capacity(1024)),
            max_rows,
            next_id: AtomicU64::new(1),
            total_evicted: AtomicU64::new(0),
            total_pushed: AtomicU64::new(0),
        })
    }

    /// Assign the next id and push the event. Evicts oldest entries
    /// when `len() > max_rows`. Returns the assigned id.
    pub fn push(&self, ev: InboundEvent) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut g = self.inner.lock();
        g.push_back((id, ev));
        // Evict in one shot to avoid one-eviction-per-push amortisation.
        if g.len() > self.max_rows {
            let drop_count = g.len() - self.max_rows;
            for _ in 0..drop_count {
                g.pop_front();
            }
            self.total_evicted
                .fetch_add(drop_count as u64, Ordering::Relaxed);
        }
        self.total_pushed.fetch_add(1, Ordering::Relaxed);
        id
    }

    /// Hydrate from a pre-parsed sequence of `(id, event)` pairs (e.g.
    /// loaded from disk by `events_persister::load_initial_events`).
    /// Entries MUST be presented in ascending `id` order; the buffer
    /// does not sort. `next_id` is bumped to `max(id) + 1` so new
    /// events continue the sequence with no collisions.
    ///
    /// Existing in-memory state is REPLACED (the buffer is cleared
    /// first). This is intended for a fresh daemon-startup hydrate,
    /// not a merge.
    pub fn hydrate_from_entries(&self, entries: impl IntoIterator<Item = (u64, InboundEvent)>) {
        let mut g = self.inner.lock();
        g.clear();
        let mut max_id = 0_u64;
        for (id, ev) in entries {
            max_id = max_id.max(id);
            g.push_back((id, ev));
        }
        let next = max_id.saturating_add(1).max(1);
        self.next_id.store(next, Ordering::Relaxed);
        self.total_pushed.store(g.len() as u64, Ordering::Relaxed);
        // Reload does not restore eviction count (it was a runtime
        // stat). Start from zero.
        self.total_evicted.store(0, Ordering::Relaxed);
    }

    /// List events with optional `since_id` filter (exclusive lower
    /// bound). Returns events whose assigned id is strictly greater
    /// than `since_id`. If `since_id` is below the current buffer's
    /// smallest id (because of eviction), returns events from the
    /// earliest available id forward — the caller observes the gap.
    /// `limit` caps the response; pass `usize::MAX` for no cap.
    pub fn list(&self, since_id: Option<u64>, limit: usize) -> Vec<InboundEvent> {
        let g = self.inner.lock();
        let start_pos = match since_id {
            Some(id) => {
                // Skip until we find an entry with id > since_id.
                // If id is below the current watermark, we start at 0.
                g.iter().position(|entry| entry.0 > id).unwrap_or(g.len())
            }
            None => 0,
        };
        g.iter()
            .skip(start_pos)
            .take(limit)
            .map(|(_, ev)| ev.clone())
            .collect()
    }

    /// Snapshot list of recent events. Used by `events.list` for
    /// the "give me the last N" pattern (since_id = None, limit = N).
    pub fn list_recent(&self, limit: usize) -> Vec<InboundEvent> {
        let g = self.inner.lock();
        let start = g.len().saturating_sub(limit);
        g.iter().skip(start).map(|(_, ev)| ev.clone()).collect()
    }

    /// Lookup by id. Returns `None` if the id was evicted or never
    /// existed (correctness review F8 — was returning wrong events
    /// after eviction).
    pub fn get(&self, id: u64) -> Option<InboundEvent> {
        let g = self.inner.lock();
        // Linear scan is correct under FIFO ordering: the buffer is
        // sorted by id ascending. For typical buffer sizes (≤1M) and
        // `O(1)` access patterns this is acceptable; if needed, a
        // BTreeMap<u64, InboundEvent> could replace the VecDeque.
        g.iter()
            .find(|(assigned, _)| *assigned == id)
            .map(|(_, ev)| ev.clone())
    }

    /// Smallest id currently in the buffer (0 if empty). Callers can
    /// use this to detect eviction and warn the operator that prior
    /// events are no longer queryable.
    pub fn smallest_id(&self) -> u64 {
        let g = self.inner.lock();
        g.front().map(|(id, _)| *id).unwrap_or(0)
    }

    /// Largest id currently in the buffer (0 if empty).
    pub fn largest_id(&self) -> u64 {
        let g = self.inner.lock();
        g.back().map(|(id, _)| *id).unwrap_or(0)
    }

    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().is_empty()
    }

    pub fn max_rows(&self) -> usize {
        self.max_rows
    }

    pub fn total_evicted(&self) -> u64 {
        self.total_evicted.load(Ordering::Relaxed)
    }

    pub fn total_pushed(&self) -> u64 {
        self.total_pushed.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventEnvelope, InboundEvent};

    fn dummy() -> InboundEvent {
        InboundEvent::parse(EventEnvelope {
            raw:
                "Message(id: \"X\", peer: \"P\", sender: \"S\", text: \"hi\", kind: Text, is_group: false)"
                    .to_string(),
            ts_unix_ms: 1000,
            ts_mono_ns: 1,
        })
    }

    #[test]
    fn push_assigns_sequential_ids() {
        let b = EventsBuffer::new(100);
        let id1 = b.push(dummy());
        let id2 = b.push(dummy());
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(b.len(), 2);
        assert_eq!(b.total_pushed(), 2);
    }

    #[test]
    fn evicts_oldest_at_max_rows() {
        let b = EventsBuffer::new(3);
        for _ in 0..5 {
            b.push(dummy());
        }
        assert_eq!(b.len(), 3);
        assert_eq!(b.total_evicted(), 2);
        // After 5 pushes (ids 1..=5), the buffer holds ids 3, 4, 5.
        assert_eq!(b.smallest_id(), 3);
        assert_eq!(b.largest_id(), 5);
    }

    #[test]
    fn get_returns_event_by_id_no_eviction() {
        let b = EventsBuffer::new(100);
        let id1 = b.push(dummy());
        let id2 = b.push(dummy());
        let ev1 = b.get(id1).unwrap();
        let ev2 = b.get(id2).unwrap();
        assert_eq!(ev1, dummy());
        assert_eq!(ev2, dummy());
    }

    /// Correctness review F8: get(id) must return None for evicted
    /// ids, NOT a different (wrong) event.
    #[test]
    fn get_returns_none_for_evicted_id() {
        let b = EventsBuffer::new(3);
        let id1 = b.push(dummy());
        let id2 = b.push(dummy());
        let id3 = b.push(dummy());
        let id4 = b.push(dummy());
        let id5 = b.push(dummy());
        // id1 and id2 were evicted.
        assert!(b.get(id1).is_none(), "id1 should be evicted");
        assert!(b.get(id2).is_none(), "id2 should be evicted");
        // id3, id4, id5 are present.
        assert!(b.get(id3).is_some());
        assert!(b.get(id4).is_some());
        assert!(b.get(id5).is_some());
    }

    /// Correctness review F8: list(since_id=N) must include events
    /// whose id is strictly greater than N, even after eviction.
    #[test]
    fn list_since_id_survives_eviction() {
        let b = EventsBuffer::new(3);
        for i in 0..5 {
            b.push(InboundEvent::synthetic_unknown("test", format!("m{i}")));
        }
        // After 5 pushes, buffer holds ids 3, 4, 5 (raw: "m2", "m3", "m4").
        // since_id = 1 → should return all 3 (3, 4, 5).
        let v = b.list(Some(1), usize::MAX);
        assert_eq!(v.len(), 3);
        // since_id = 3 → should return only 4 and 5 (events strictly > id 3).
        let v = b.list(Some(3), usize::MAX);
        assert_eq!(v.len(), 2);
        // since_id = 100 → buffer's largest is 5 → empty list.
        let v = b.list(Some(100), usize::MAX);
        assert!(v.is_empty());
    }

    #[test]
    fn list_recent_returns_last_n() {
        let b = EventsBuffer::new(100);
        for i in 0..10 {
            b.push(InboundEvent::synthetic_unknown("test", format!("m{i}")));
        }
        let last3 = b.list_recent(3);
        assert_eq!(last3.len(), 3);
        if let InboundEvent::Unknown { wacore_event, .. } = &last3[0] {
            assert_eq!(wacore_event, &serde_json::Value::String("m7".to_string()));
        } else {
            panic!("expected Unknown");
        }
    }

    #[test]
    fn get_returns_none_for_out_of_range() {
        let b = EventsBuffer::new(100);
        assert!(b.get(0).is_none());
        assert!(b.get(999).is_none());
    }

    #[test]
    fn list_with_limit() {
        let b = EventsBuffer::new(100);
        for i in 0..20 {
            b.push(InboundEvent::synthetic_unknown("test", format!("m{i}")));
        }
        let v = b.list(None, 5);
        assert_eq!(v.len(), 5);
    }

    #[test]
    fn empty_buffer() {
        let b = EventsBuffer::new(100);
        assert!(b.is_empty());
        assert_eq!(b.len(), 0);
        assert_eq!(b.smallest_id(), 0);
        assert_eq!(b.largest_id(), 0);
        assert!(b.list_recent(10).is_empty());
    }

    #[test]
    fn hydrate_replaces_existing_state() {
        let b = EventsBuffer::new(100);
        for _ in 0..3 {
            b.push(dummy()); // ids 1..=3
        }
        assert_eq!(b.next_id.load(Ordering::Relaxed), 4);
        // Reload with persisted ids 10, 11, 12.
        let entries = vec![(10, dummy()), (11, dummy()), (12, dummy())];
        b.hydrate_from_entries(entries);
        assert_eq!(b.len(), 3);
        assert_eq!(b.smallest_id(), 10);
        assert_eq!(b.largest_id(), 12);
        // Next push should be id 13, not 4.
        let next = b.push(dummy());
        assert_eq!(next, 13, "next_id must continue post-reload");
    }

    #[test]
    fn hydrate_with_empty_iter_clears_buffer() {
        let b = EventsBuffer::new(100);
        b.push(dummy());
        b.hydrate_from_entries(std::iter::empty());
        assert!(b.is_empty());
        assert_eq!(b.next_id.load(Ordering::Relaxed), 1);
        // Next push starts at 1.
        assert_eq!(b.push(dummy()), 1);
    }
}
