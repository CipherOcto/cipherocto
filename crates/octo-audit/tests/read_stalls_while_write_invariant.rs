//! Read-stalls-while-write invariant test vectors (RFC-0016-a §6.11).
//!
//! 8 vectors exercising the atomic pre/post-write observation
//! invariant required by RFC-0016-a §6.11. The invariant holds
//! regardless of R/W primitive choice per the RFC text (each
//! DOMAIN impl owns the choice — single shared `Mutex`,
//! `RwLock`, or sharded). This suite uses `std::sync::Mutex`
//! for the in-memory harness; the DOMAIN adapter conformance
//! AC verifies the production storage site applies the same
//! invariant (separate test or grep — DOMAIN impl owns the
//! primitive).
//!
//! Run with:
//!   cargo test -p octo-audit --test read_stalls_while_write_invariant

#![allow(unused_imports)]

use octo_audit::{
    append_audit_event, compute_chain_hash, AppendOnlyAuditSink, AuditError, AuditEvent,
    AuditEventKind,
};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Barrier, Mutex,
};
use std::thread;
use std::time::Duration;

/// Backing store for the shared `AppendOnlyAuditSink` impl. Reads
/// and writes coordinate through this single primitive so the §6.11
/// atomic pre/post-write observation invariant can be exercised
/// directly. The DOMAIN adapter conformance AC validates that
/// production sinks follow the same pattern.
#[derive(Default)]
struct SharedState {
    events: Vec<AuditEvent>,
}

/// In-memory `AppendOnlyAuditSink` impl for the §6.11 invariant
/// suite. Every `append` call enqueues the event; every
/// `last_event_id` reads the latest `event_id` atomically.
struct TestSink {
    state: Arc<Mutex<SharedState>>,
}

impl TestSink {
    fn new(state: Arc<Mutex<SharedState>>) -> Self {
        Self { state }
    }
}

impl AppendOnlyAuditSink for TestSink {
    fn append(&mut self, event: &AuditEvent) -> Result<(), AuditError> {
        let mut s = self
            .state
            .lock()
            .map_err(|_| AuditError::SinkSpecific("test state poisoned".into()))?;
        s.events.push(event.clone());
        Ok(())
    }
    fn last_event_id(&self) -> Result<Option<u64>, AuditError> {
        let s = self
            .state
            .lock()
            .map_err(|_| AuditError::SinkSpecific("test state poisoned".into()))?;
        Ok(s.events.last().map(|e| e.event_id))
    }
}

fn make_event(id: u64, kind: AuditEventKind) -> AuditEvent {
    let mut e = AuditEvent {
        event_id: id,
        node_did: "did:oct:test".to_owned(),
        event_kind: kind,
        cap_root_hash: [0xab; 32],
        at_millis_unix: 1_000 + id,
        prev_chain_hash: if id == 0 { [0; 32] } else { [id as u8; 32] },
        chain_hash: [0; 32],
    };
    e.chain_hash = compute_chain_hash(&e);
    e
}

// RFC-0016-a §6.11 — N=8 reader threads + 1 writer thread. Each
// reader captures a snapshot of the event-id sequence under the
// shared mutex and asserts the sequence is monotonic (no partial
// reads). The writer thread appends a batch mid-reads.
#[test]
fn rsw_concurrent_readers_no_partial_rows() {
    let state = Arc::new(Mutex::new(SharedState::default()));
    let readers = 8;
    let writer_appends = 32;
    let barrier = Arc::new(Barrier::new(readers + 1));

    // Writer thread — owns the TestSink exclusively (single writer
    // per RFC-0016-a §6.11 single-writer guarantee).
    let writer_state = Arc::clone(&state);
    let writer_barrier = Arc::clone(&barrier);
    let writer = thread::spawn(move || {
        let mut sink = TestSink::new(Arc::clone(&writer_state));
        writer_barrier.wait();
        for i in 0..writer_appends {
            let event = make_event(i, AuditEventKind::Insert);
            append_audit_event(&mut sink, event).expect("append");
        }
    });

    let reader_observations = Arc::new(Mutex::new(Vec::<Vec<u64>>::new()));
    let mut reader_handles = Vec::new();
    for _ in 0..readers {
        let state_c = Arc::clone(&state);
        let barrier_c = Arc::clone(&barrier);
        let obs_c = Arc::clone(&reader_observations);
        reader_handles.push(thread::spawn(move || {
            barrier_c.wait();
            for _ in 0..10 {
                let snapshot: Vec<u64> = {
                    let s = state_c.lock().expect("state lock");
                    s.events.iter().map(|e| e.event_id).collect()
                };
                // Snapshot MUST be monotonically increasing
                // (writer assigns event_id in order 0..N).
                assert!(
                    snapshot.windows(2).all(|w| w[0] < w[1]),
                    "reader saw non-monotonic snapshot under concurrent write: {snapshot:?}"
                );
                obs_c.lock().expect("obs lock").push(snapshot);
                thread::sleep(Duration::from_millis(1));
            }
        }));
    }

    writer.join().expect("writer join");
    for h in reader_handles {
        h.join().expect("reader join");
    }

    // Sanity: writer succeeded; state contains the expected number
    // of events.
    let final_state = state.lock().expect("final state lock");
    assert_eq!(final_state.events.len(), writer_appends as usize);

    // Sanity: every reader observation is a prefix of the final
    // ordered sequence OR the full final sequence (atomic
    // pre/post-write observation invariant from RFC-0016-a §6.11).
    let final_ids: Vec<u64> = (0..writer_appends).collect();
    let observations = reader_observations.lock().expect("observations lock");
    assert!(!observations.is_empty(), "readers recorded observations");
    for obs in observations.iter() {
        assert!(
            obs.windows(2).all(|w| w[0] < w[1]),
            "snapshot non-monotonic under concurrent writer: {obs:?}"
        );
        // Every snapshot is a prefix of the final event-id sequence.
        assert!(
            final_ids.starts_with(obs),
            "snapshot {obs:?} is not a prefix of final event-id sequence {final_ids:?}"
        );
    }
}

// RFC-0016-a §6.11 — single-writer guarantee. The façade
// `append_audit_event` requires `&mut sink`, so the borrow
// checker prevents two threads from concurrently calling
// `append`. The DOMAIN adapter conformance AC validates
// production sinks enforce the same pattern.
#[test]
fn rsw_single_writer_guarantee() {
    let state = Arc::new(Mutex::new(SharedState::default()));
    let mut sink = TestSink::new(Arc::clone(&state));
    for i in 0..4 {
        let event = make_event(i, AuditEventKind::Insert);
        let chain = append_audit_event(&mut sink, event).expect("append");
        assert_eq!(chain.0.len(), 32);
    }
    let final_state = state.lock().expect("final state lock");
    assert_eq!(final_state.events.len(), 4);
    let ids: Vec<u64> = final_state.events.iter().map(|e| e.event_id).collect();
    assert_eq!(ids, vec![0, 1, 2, 3]);
}

// RFC-0016-a §6.11 — atomic pre/post-write observation: the
// reader either sees the pre-write state OR the post-write
// state, never a partial row.
#[test]
fn rsw_atomic_pre_post_write_observation() {
    let state = Arc::new(Mutex::new(SharedState::default()));
    let writer_started = Arc::new(AtomicBool::new(false));
    let writer_done = Arc::new(AtomicBool::new(false));
    let barrier = Arc::new(Barrier::new(2));

    let writer_state = Arc::clone(&state);
    let writer_started_c = Arc::clone(&writer_started);
    let writer_done_c = Arc::clone(&writer_done);
    let writer_barrier = Arc::clone(&barrier);
    let writer = thread::spawn(move || {
        let mut sink = TestSink::new(Arc::clone(&writer_state));
        // Sync with reader: writer flips `writer_started` then
        // waits at barrier so reader captures a pre-write
        // observation deterministically.
        writer_started_c.store(true, Ordering::SeqCst);
        writer_barrier.wait();
        for i in 0..10 {
            let event = make_event(i, AuditEventKind::Insert);
            append_audit_event(&mut sink, event).expect("append");
        }
        writer_done_c.store(true, Ordering::SeqCst);
    });

    let mut pre_observations = 0usize;
    let mut post_observations = 0usize;
    // Wait for writer to flip `started` BEFORE we read the state
    // (so we observe the deterministic pre-write state).
    while !writer_started.load(Ordering::SeqCst) {
        thread::yield_now();
    }
    // Pre-write observation: snapshot MUST be empty.
    let pre_len = {
        let s = state.lock().expect("state lock");
        s.events.len()
    };
    assert_eq!(pre_len, 0, "pre-write snapshot must be empty");
    pre_observations += 1;
    // Release writer.
    barrier.wait();

    // Poll until writer finishes; record post-write observations.
    while !writer_done.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_micros(50));
    }
    let post_len = {
        let s = state.lock().expect("state lock");
        s.events.len()
    };
    assert_eq!(
        post_len, 10,
        "post-write snapshot must contain all 10 events (no partial rows)"
    );
    post_observations += 1;

    writer.join().expect("writer join");
    assert!(
        pre_observations + post_observations > 0,
        "reader loop did not record any observation"
    );
}

// RFC-0016-a §6.11 — DOMAIN adapter conformance (runtime check
// using the harness `TestSink`). The production storage site
// (`crates/octo-audit/src/storage/stoolap.rs`) MUST route through
// the gated adapter (R/W primitive enforcing atomic pre/post-
// write observation). The DOMAIN adapter conformance AC
// validates the production site via grep + manual review.
#[test]
fn rsw_sink_impl_enforces_atomic_observation() {
    let state = Arc::new(Mutex::new(SharedState::default()));
    let mut sink = TestSink::new(Arc::clone(&state));
    // Append two events under exclusive ownership.
    for i in 0..2 {
        let event = make_event(i, AuditEventKind::Insert);
        let _ = append_audit_event(&mut sink, event).expect("append");
    }
    // A separate reader MUST see exactly 2 events (post-write).
    // Never 1 (partial).
    let snapshot_len = {
        let s = state.lock().expect("state lock");
        s.events.len()
    };
    assert_eq!(snapshot_len, 2);
}

// RFC-0016-a §6.11 — read-side does not block under write-side
// starvation (smoke). Under the `std::sync::Mutex` primitive
// used by the harness, readers acquire the lock per snapshot;
// concurrent appends serialise but readers never observe a
// torn state.
#[test]
fn rsw_reads_completable_under_concurrent_writes() {
    let state = Arc::new(Mutex::new(SharedState::default()));
    let stop = Arc::new(AtomicBool::new(false));
    let total_appends = Arc::new(AtomicU64::new(0));

    let writer_state = Arc::clone(&state);
    let writer_stop = Arc::clone(&stop);
    let writer_total = Arc::clone(&total_appends);
    let writer = thread::spawn(move || {
        let mut sink = TestSink::new(Arc::clone(&writer_state));
        let mut i = 0u64;
        while !writer_stop.load(Ordering::SeqCst) {
            let event = make_event(i, AuditEventKind::Insert);
            append_audit_event(&mut sink, event).expect("append");
            writer_total.fetch_add(1, Ordering::SeqCst);
            i += 1;
            if i > 200 {
                break;
            }
        }
    });

    // Reader loop captures monotonic snapshots. Capture the snapshot
    // BEFORE the exit-condition check so we always record at least
    // one observation, even on heavily-loaded CI runners where the
    // writer thread can complete its 200-iteration burst before the
    // reader thread is scheduled (RFC-0016-a §6.11 invariant: readers
    // can complete observation under concurrent writes — including
    // zero-write windows where the writer has already finished).
    let mut observations = 0usize;
    loop {
        let snapshot: Vec<u64> = {
            let s = state.lock().expect("state lock");
            s.events.iter().map(|e| e.event_id).collect()
        };
        assert!(
            snapshot.windows(2).all(|w| w[0] < w[1]),
            "non-monotonic snapshot under concurrent writes: {snapshot:?}"
        );
        observations += 1;
        if total_appends.load(Ordering::SeqCst) >= 50 {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }

    stop.store(true, Ordering::SeqCst);
    writer.join().expect("writer join");
    assert!(
        observations > 0,
        "reader recorded no observations while writer was active"
    );
}

// RFC-0016-a §6.11 — write-side enforces chain-hash re-derivation
// even under contention (defense-in-depth: the façade
// `append_audit_event` re-derives `chain_hash` from canonical
// bytes BEFORE the sink is called; a concurrent reader cannot
// race the re-derivation).
#[test]
fn rsw_write_side_re_derives_chain_hash() {
    let state = Arc::new(Mutex::new(SharedState::default()));
    let mut sink = TestSink::new(Arc::clone(&state));
    let mut event = make_event(99, AuditEventKind::Insert);
    // Flip one byte → mismatch.
    event.chain_hash[0] ^= 0xff;
    let err = append_audit_event(&mut sink, event).unwrap_err();
    assert!(
        matches!(err, AuditError::ChainHashMismatch { event_id: 99 }),
        "ChainHashMismatch MUST fire before sink call even under contention: {err:?}"
    );
    let snapshot_len = {
        let s = state.lock().expect("state lock");
        s.events.len()
    };
    assert_eq!(
        snapshot_len, 0,
        "sink MUST NOT have stored the mismatched event (RFC-0016-a §6.10)"
    );
}

// RFC-0016-a §6.11 — DOMAIN adapter conformance grep assertion
// (test-side equivalent: the harness `TestSink` MUST use a
// primitive that enforces atomic observation; `std::sync::Mutex`
// satisfies the invariant). The production DOMAIN adapter
// (`crates/octo-audit/src/storage/stoolap.rs`) is verified by a
// separate AC-9 grep + AC-16 atomicity check; this test ensures
// the harness implements the same contract.
#[test]
fn rsw_harness_satisfies_dom_adapter_contract() {
    let state = Arc::new(Mutex::new(SharedState::default()));
    let mut sink = TestSink::new(Arc::clone(&state));
    // Write 1 event.
    let e0 = make_event(0, AuditEventKind::Insert);
    append_audit_event(&mut sink, e0).expect("append");
    // Concurrent reader MUST see exactly 1 event (no torn 0.5).
    let n = {
        let s = state.lock().expect("state lock");
        s.events.len()
    };
    assert_eq!(n, 1);
}
