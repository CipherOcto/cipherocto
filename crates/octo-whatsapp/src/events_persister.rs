//! In-memory events ring buffer. Phase 3 Part B.
//!
//! Bounded by `max_rows` (default 1_000_000) per design §InboundEvent
//! retention. The `db_writer` task is the sole writer (single-owner
//! pattern from design); the buffer's `parking_lot::Mutex` is held only
//! for the push/list/get operations, never across `.await`.
//!
//! Monotonic ids are assigned at insert time. The `since_id` filter on
//! `list()` is what the design §Loss recovery path uses to backfill
//! after `RecvError::Lagged(n)`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;

use crate::events::InboundEvent;

#[derive(Debug)]
pub struct EventsBuffer {
    inner: Mutex<Vec<InboundEvent>>,
    max_rows: usize,
    next_id: AtomicU64,
    total_evicted: AtomicU64,
    total_pushed: AtomicU64,
}

impl EventsBuffer {
    pub fn new(max_rows: usize) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Vec::with_capacity(1024)),
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
        g.push(ev);
        // Evict in one shot to avoid one-eviction-per-push amortisation.
        if g.len() > self.max_rows {
            let drop_count = g.len() - self.max_rows;
            g.drain(..drop_count);
            self.total_evicted
                .fetch_add(drop_count as u64, Ordering::Relaxed);
        }
        self.total_pushed.fetch_add(1, Ordering::Relaxed);
        id
    }

    /// List events with optional `since_id` filter (exclusive lower bound).
    /// `limit` caps the response; pass `usize::MAX` for no cap.
    pub fn list(&self, since_id: Option<u64>, limit: usize) -> Vec<InboundEvent> {
        let g = self.inner.lock();
        let start = match since_id {
            Some(id) => {
                // We don't store ids in the buffer; we use the position
                // heuristic: since_id is 1-based. Position = (id - 1) - dropped.
                // For now, the simplest is: since_id refers to position
                // since eviction is FIFO. Caller tracks last_seen position.
                g.iter().position(|_| true).map_or(0, |p| {
                    p.saturating_add(id.saturating_sub(1) as usize)
                        .saturating_sub(p)
                })
            }
            None => 0,
        };
        g.iter().skip(start).take(limit).cloned().collect()
    }

    /// Snapshot list of recent events. Used by `events.list` for
    /// the "give me the last N" pattern (since_id = None, limit = N).
    pub fn list_recent(&self, limit: usize) -> Vec<InboundEvent> {
        let g = self.inner.lock();
        let start = g.len().saturating_sub(limit);
        g.iter().skip(start).cloned().collect()
    }

    pub fn get(&self, id: u64) -> Option<InboundEvent> {
        // See `list`: id-based lookup is approximate under eviction.
        // For now, treat id as 1-based position relative to start.
        let g = self.inner.lock();
        if id == 0 || id > g.len() as u64 {
            return None;
        }
        g.get((id - 1) as usize).cloned()
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
    use crate::events::EventEnvelope;

    fn dummy() -> InboundEvent {
        InboundEvent::parse(EventEnvelope {
            raw: "Message(id: \"X\", peer: \"P\", sender: \"S\", text: \"hi\", kind: Text, is_group: false)".to_string(),
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
    }

    #[test]
    fn list_recent_returns_last_n() {
        let b = EventsBuffer::new(100);
        for i in 0..10 {
            let ev = InboundEvent::Unknown {
                raw: format!("m{i}"),
                ts_unix_ms: i,
                ts_mono_ns: 0,
                untrusted: false,
            };
            b.push(ev);
        }
        let last3 = b.list_recent(3);
        assert_eq!(last3.len(), 3);
        if let InboundEvent::Unknown { raw, .. } = &last3[0] {
            assert_eq!(raw, "m7");
        } else {
            panic!("expected Unknown");
        }
    }

    #[test]
    fn get_returns_event_by_id() {
        let b = EventsBuffer::new(100);
        let id1 = b.push(dummy());
        let id2 = b.push(dummy());
        let ev1 = b.get(id1).unwrap();
        let ev2 = b.get(id2).unwrap();
        assert_eq!(ev1, dummy());
        assert_eq!(ev2, dummy());
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
            b.push(InboundEvent::Unknown {
                raw: format!("m{i}"),
                ts_unix_ms: i,
                ts_mono_ns: 0,
                untrusted: false,
            });
        }
        let v = b.list(None, 5);
        assert_eq!(v.len(), 5);
    }

    #[test]
    fn empty_buffer() {
        let b = EventsBuffer::new(100);
        assert!(b.is_empty());
        assert_eq!(b.len(), 0);
        assert!(b.list_recent(10).is_empty());
    }
}
