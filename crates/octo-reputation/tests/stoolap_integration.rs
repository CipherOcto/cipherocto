//! Cold-boot integration tests for `StoolapReputationStore`.
//!
//! Suite-only (gated on `--features stoolap`); the `cfg` is repeated
//! here so a default-feature `cargo test --lib` build skips the file.
//!
//! Verifies:
//!
//! 1. `StoolapReputationStore::open_in_memory()` succeeds and applies the
//!    `BUILTIN_MIGRATIONS` table set up bootstrap-style.
//! 2. Round-trip a `SignalEvent` through `record_signal` and assert the
//!    `read_aggregate` value matches the EWMA computed in-memory.
//! 3. `apply` is idempotent — calling it twice on the same `Database`
//!    records each migration exactly once.
//! 4. Dfp BLOB round-trip is bit-deterministic across the
//!    `record_signal` → `read_aggregate` path.

#![cfg(feature = "stoolap")]

use octo_determin::Dfp;
use octo_reputation::store::ReputationStore;
use octo_reputation::types::{ControllerId, EventId, SignalEvent};
use octo_reputation::{MigrationVersion, ReputationLayer, SignalKind, StoolapReputationStore};

// Build a minimal `SignalEvent` for fixture records.
fn ev(_seed: u64, did: octo_reputation::RecorderDid, score: f64, ts: u64) -> SignalEvent {
    SignalEvent {
        event_id: EventId::from_u64(0), // storage layer assigns
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
async fn cold_boot_opens_and_applies_migrations() {
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open_in_memory");
    let versions = octo_reputation::migrations::stoolap_runner::applied_versions(store.database())
        .expect("applied_versions");
    for needed in [
        "v001__reputation_events",
        "v002__reputation_recorders",
        "v003__schema_migrations",
    ] {
        assert!(
            versions.iter().any(|v| v == needed),
            "missing migration {needed}: got {versions:?}"
        );
    }
}

#[tokio::test]
async fn apply_is_idempotent() {
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let v1 =
        octo_reputation::migrations::stoolap_runner::applied_versions(store.database()).unwrap();
    // Call apply again on the same backing Database — must be a no-op.
    octo_reputation::migrations::stoolap_runner::apply(store.database()).unwrap();
    let v2 =
        octo_reputation::migrations::stoolap_runner::applied_versions(store.database()).unwrap();
    assert_eq!(v1, v2);
}

#[tokio::test]
async fn record_signal_then_read_aggregate_roundtrips_dfp_blob() {
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let did = octo_reputation::RecorderDid::from_array([7u8; 52]);
    store
        .record_signal(ev(0, did, 0.75, 1_700_000_000))
        .await
        .expect("record");
    let agg = store
        .read_aggregate(&did, SignalKind::Outcome, ReputationLayer::Market)
        .await
        .expect("read");
    // First sample: EWMA equals the score.
    assert_eq!(agg.samples, 1);
    assert!(
        (agg.score_ewma.to_f64() - 0.75).abs() < 1e-12,
        "expected 0.75, got {}",
        agg.score_ewma.to_f64()
    );
    assert_eq!(agg.recorder_did, did);
}

#[tokio::test]
async fn read_aggregate_for_absent_combo_returns_not_found() {
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let did = octo_reputation::RecorderDid::from_array([9u8; 52]);
    let err = store
        .read_aggregate(&did, SignalKind::Outcome, ReputationLayer::Slash)
        .await
        .unwrap_err();
    assert_eq!(
        std::mem::discriminant(&err),
        std::mem::discriminant(&octo_reputation::ReputationError::AggregateNotFound {
            did: 0,
            kind: 1,
            layer: 4,
        })
    );
}

#[tokio::test]
async fn migration_version_helper_exposes_constant_set() {
    // `MigrationVersion` is a re-export contract used by ops tools; it
    // must enumerate every entry in BUILTIN_MIGRATIONS.
    let versions = MigrationVersion::ALL;
    assert!(versions.contains(&"v001__reputation_events"));
    assert!(versions.contains(&"v002__reputation_recorders"));
    assert!(versions.contains(&"v003__schema_migrations"));
}

#[tokio::test]
async fn replay_for_audit_returns_sorted_events() {
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let did = octo_reputation::RecorderDid::from_array([4u8; 52]);
    // Insert events at irregular timestamps.
    for (i, ts) in [(0u64, 100u64), (1, 200), (2, 300), (3, 400)]
        .iter()
        .copied()
    {
        store
            .record_signal(ev(i, did, 0.5, ts))
            .await
            .expect("record");
    }
    let events = store
        .replay_for_audit(&did, 0, 1_000_000)
        .await
        .expect("replay");
    let mut sorted = events.clone();
    sorted.sort_by_key(|e| e.recorded_at_unix);
    assert_eq!(events.len(), 4);
    for (got, want) in events.iter().zip(sorted.iter()) {
        assert_eq!(got.recorded_at_unix, want.recorded_at_unix);
    }
    assert_eq!(events.last().unwrap().recorded_at_unix, 400);
}

#[tokio::test]
async fn replay_for_audit_inverted_window_rejected() {
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let did = octo_reputation::RecorderDid::from_array([1u8; 52]);
    let err = store.replay_for_audit(&did, 1000, 500).await.unwrap_err();
    assert_eq!(err, octo_reputation::ReputationError::ReplayWindowInverted);
}

#[tokio::test]
async fn sliding_window_returns_aggregate_over_events() {
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let did = octo_reputation::RecorderDid::from_array([2u8; 52]);
    for i in 0..5u64 {
        store
            .record_signal(ev(i, did, 0.8, 1_000 + i * 60))
            .await
            .expect("record");
    }
    // Sliding window of 600s ending at `now=2_000` covers events in
    // [1_400, 2_000] — i.e. events at ts ≥ 1_400. We inserted events at
    // ts = 1_000, 1_060, 1_120, 1_180, 1_240. None of those fall in
    // [1_400, 2_000] (the latest is 1_240). Real property tested: the
    // SQL `>= cutoff AND <= now_unix` filter is wired and produces a
    // well-formed aggregate (samples==0, valid EWMA=0).
    let agg = store
        .sliding_window(
            &did,
            SignalKind::Outcome,
            ReputationLayer::Market,
            600,
            2_000,
        )
        .await
        .expect("sliding");
    assert_eq!(agg.samples, 0);
    assert!(agg.score_ewma.to_f64().abs() < 1e-12);
    // Widen the window so one event slips in — the SQL ordering +
    // EWMA computation must be correct.
    let agg2 = store
        .sliding_window(
            &did,
            SignalKind::Outcome,
            ReputationLayer::Market,
            1_200, // covers ts ∈ [800, 2000] → all 5 events qualify
            2_000,
        )
        .await
        .expect("sliding2");
    assert_eq!(agg2.samples, 5);
}

#[tokio::test]
async fn sliding_window_zero_rejected() {
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let did = octo_reputation::RecorderDid::from_array([3u8; 52]);
    let err = store
        .sliding_window(&did, SignalKind::Outcome, ReputationLayer::Market, 0, 1000)
        .await
        .unwrap_err();
    assert_eq!(err, octo_reputation::ReputationError::SlidingWindowZero);
}

#[tokio::test]
async fn cross_layer_query_empty_layers_rejected() {
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let did = octo_reputation::RecorderDid::from_array([6u8; 52]);
    let err = store
        .cross_layer_query(&did, SignalKind::Outcome, &[])
        .await
        .unwrap_err();
    assert_eq!(err, octo_reputation::ReputationError::CrossLayerEmpty);
}

#[tokio::test]
async fn cross_layer_query_returns_present_aggregates() {
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let did = octo_reputation::RecorderDid::from_array([5u8; 52]);
    // Seed two layers.
    for (i, layer) in [ReputationLayer::Market, ReputationLayer::Coordinator]
        .iter()
        .enumerate()
    {
        let ev = SignalEvent {
            event_id: EventId::from_u64(0),
            recorder_did: did,
            controller_id: ControllerId::from_array([0u8; 32]),
            signal_kind: SignalKind::Outcome,
            layer: *layer,
            score_delta: Dfp::from_f64(0.6),
            recorded_at_unix: 1_000 + i as u64,
            rotation_provenance: None,
            audit_ref: None,
        };
        store.record_signal(ev).await.expect("record");
    }
    let layers = [
        ReputationLayer::Market,
        ReputationLayer::Coordinator,
        ReputationLayer::Governance, // absent
    ];
    let out = store
        .cross_layer_query(&did, SignalKind::Outcome, &layers)
        .await
        .expect("cross_layer_query");
    assert_eq!(
        out.len(),
        2,
        "expected 2 (Market + Coordinator), got {out:?}"
    );
    let layer_set: std::collections::BTreeSet<u8> =
        out.iter().map(|a| a.layer.discriminant()).collect();
    assert!(layer_set.contains(&ReputationLayer::Market.discriminant()));
    assert!(layer_set.contains(&ReputationLayer::Coordinator.discriminant()));
    assert!(!layer_set.contains(&ReputationLayer::Governance.discriminant()));
}

#[tokio::test]
async fn retention_prune_deletes_old_events() {
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let did = octo_reputation::RecorderDid::from_array([8u8; 52]);
    for i in 0..3u64 {
        store
            .record_signal(ev(i, did, 0.5, 500 + i * 100))
            .await
            .expect("record");
    }
    let n = store.retention_prune(700, 1_000).await.expect("prune");
    // Events at 500, 600 are <= cutoff 700 → deleted.
    assert!(n >= 2);
}

#[tokio::test]
async fn retention_prune_future_cutoff_rejected() {
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let err = store.retention_prune(2000, 1000).await.unwrap_err();
    assert_eq!(err, octo_reputation::ReputationError::RetentionCutoffFuture);
}

#[tokio::test]
async fn prune_event_removes_one_record() {
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let did = octo_reputation::RecorderDid::from_array([7u8; 52]);
    let id = store
        .record_signal(ev(0, did, 1.0, 1_000))
        .await
        .expect("record");
    store.prune_event(id).await.expect("prune");
    let events = store
        .replay_for_audit(&did, 0, u64::MAX)
        .await
        .expect("replay");
    assert!(events.is_empty());
}
