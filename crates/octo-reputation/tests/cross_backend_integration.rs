//! Cross-backend determinism property-test.
//!
//! Verifies the contract in
//! `docs/plans/2026-07-27-mission-0968-stoolap-impl.md` Session 2:
//!
//! > Same 1_000-event sequence against `InMemoryReputationStore` and
//! > `StoolapReputationStore` (memory DSN) yields byte-identical
//! > canonical_bytes for every event.
//!
//! Tolerance on aggregate EWMA scores is `1e-12` (one-time `f64 → Dfp`
//! rounding at the canonical-bytes boundary). Other paths are
//! byte-identical.
//!
//! Suite is gated on `--features stoolap`; with the default build the
//! file is skipped entirely (`#![cfg(feature = "stoolap")]` at the top).

#![cfg(feature = "stoolap")]

use octo_determin::Dfp;
use octo_reputation::store::ReputationStore;
use octo_reputation::types::{ControllerId, EventId, SignalEvent};
use octo_reputation::{
    InMemoryReputationStore, ReputationLayer, SignalKind, StoolapReputationStore,
};

const N_EVENTS: u64 = 1_000;
const CROSS_BACKEND_TOL: f64 = 1e-12;

/// Seed the same `score_delta` sequence across both backends.
fn mk_event(seed: u64, did: octo_reputation::RecorderDid) -> SignalEvent {
    // Deterministic score: 0.5 + (seed % 100) * 0.005 → range [0.5, 0.995].
    let score = 0.5 + (seed % 100) as f64 * 0.005;
    // Timestamps strictly monotonic from 1_000 onward.
    let ts = 1_000 + seed * 60;
    SignalEvent {
        event_id: EventId::from_u64(seed), // reassigned by store
        recorder_did: did,
        controller_id: ControllerId::from_array([0u8; 32]),
        signal_kind: SignalKind::Outcome,
        layer: ReputationLayer::Market,
        score_delta: Dfp::from_f64(score),
        recorded_at_unix: ts,
        rotation_provenance: None,
        audit_ref: None,
    }
}

#[tokio::test]
async fn cross_backend_1k_events_byte_identical_canonical_bytes_and_aggregate() {
    let did = octo_reputation::RecorderDid::from_array([0xAB; 52]);

    // Two independent stores, seeded with identical input.
    let mem = InMemoryReputationStore::new();
    let stoolap = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");

    for i in 0..N_EVENTS {
        let ev = mk_event(i + 1, did);
        mem.record_signal(ev.clone()).await.expect("mem.record");
        stoolap.record_signal(ev).await.expect("stoolap.record");
    }

    // Aggregate score must agree within `CROSS_BACKEND_TOL`.
    let mem_agg = mem
        .read_aggregate(&did, SignalKind::Outcome, ReputationLayer::Market)
        .await
        .expect("mem.read");
    let stoolap_agg = stoolap
        .read_aggregate(&did, SignalKind::Outcome, ReputationLayer::Market)
        .await
        .expect("stoolap.read");

    assert_eq!(
        mem_agg.samples, stoolap_agg.samples,
        "sample counts diverged: mem={} stoolap={}",
        mem_agg.samples, stoolap_agg.samples,
    );
    assert_eq!(mem_agg.samples, N_EVENTS);
    let diff = (mem_agg.score_ewma.to_f64() - stoolap_agg.score_ewma.to_f64()).abs();
    assert!(
        diff < CROSS_BACKEND_TOL,
        "score_ewma diverged: mem={} stoolap={} diff={diff}",
        mem_agg.score_ewma.to_f64(),
        stoolap_agg.score_ewma.to_f64(),
    );
    // Canonical bytes for the aggregate must match bit-for-bit: the BLOB
    // round-trip preserves all 24 bytes identically.
    assert_eq!(
        octo_reputation::types::dfp_to_blob(&mem_agg.score_ewma),
        octo_reputation::types::dfp_to_blob(&stoolap_agg.score_ewma),
        "score_ewma canonical bytes diverged"
    );

    // Replay output must be equal (event count + identical ts ordering).
    let mem_events = mem
        .replay_for_audit(&did, 0, u64::MAX)
        .await
        .expect("mem.replay");
    let stoolap_events = stoolap
        .replay_for_audit(&did, 0, u64::MAX)
        .await
        .expect("stoolap.replay");
    assert_eq!(
        mem_events.len(),
        stoolap_events.len(),
        "event counts diverged: mem={} stoolap={}",
        mem_events.len(),
        stoolap_events.len()
    );
    assert_eq!(mem_events.len() as u64, N_EVENTS);
    // Each event's score_delta round-trips to byte-identical 24-byte
    // canonical form across both backends.
    for (m, s) in mem_events.iter().zip(stoolap_events.iter()) {
        assert_eq!(
            octo_reputation::types::dfp_to_blob(&m.score_delta),
            octo_reputation::types::dfp_to_blob(&s.score_delta),
            "event {} score_delta canonical bytes diverged",
            m.event_id.to_u64(),
        );
        assert_eq!(m.recorded_at_unix, s.recorded_at_unix);
    }
}

#[tokio::test]
async fn cross_backend_canonical_bytes_identical_for_empty_run() {
    // A store with no events at all still satisfies the determinism
    // contract — both backends must report the same AggregateNotFound
    // variant for the same (did, kind, layer) tuple.
    let did = octo_reputation::RecorderDid::from_array([0xCD; 52]);

    let mem = InMemoryReputationStore::new();
    let stoolap = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");

    let mem_err = mem
        .read_aggregate(&did, SignalKind::Outcome, ReputationLayer::Market)
        .await
        .unwrap_err();
    let stoolap_err = stoolap
        .read_aggregate(&did, SignalKind::Outcome, ReputationLayer::Market)
        .await
        .unwrap_err();
    assert_eq!(
        std::mem::discriminant(&mem_err),
        std::mem::discriminant(&stoolap_err),
        "AggregateNotFound discriminant diverged between backends"
    );
}
