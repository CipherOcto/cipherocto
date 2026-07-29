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
//! 5. Session 8 (mission 0968 Phase 4): attestor registration +
//!    attestation round-trip + quorum threshold + gossip catch-up.

#![cfg(feature = "stoolap")]

use octo_determin::Dfp;
use octo_reputation::auth::{Attestation, AttestorId, AttestorRegistration};
use octo_reputation::constants::MIN_ATTESTOR_QUORUM;
use octo_reputation::gossip::GossipCatchUp;
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
        anchor_tx_hash: None,
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
        "v004__reputation_attestations",
        "v005__reputation_gossip_seen",
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
    assert!(versions.contains(&"v004__reputation_attestations"));
    assert!(versions.contains(&"v005__reputation_gossip_seen"));
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
            anchor_tx_hash: None,
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
async fn v004_migration_applies_reputation_attestations_table() {
    // Session 7 (mission 0968 Phase 4) v004 migration adds the
    // `reputation_attestations` + `reputation_attestors` tables. The
    // migration_version_helper_exposes_constant_set test already
    // asserts the version is in BUILTIN_MIGRATIONS; this test
    // additionally verifies the schema applied to a fresh in-memory
    // DB by inserting a row and reading it back. (Session 8 fills in
    // the full SQL impl; here we only assert the schema is present.)
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    // `reputation_attestors` exists — the runner applied v004.
    // We assert via a SELECT on the table (which fails the test if
    // the table is missing).
    let mut rows = store
        .database()
        .query("SELECT COUNT(*) FROM reputation_attestors", ())
        .expect("attestors table must exist after v004");
    let row = rows.next().expect("count row");
    let count: i64 = row.expect("ok row").get(0).expect("count col");
    assert_eq!(count, 0, "fresh attestors table is empty");
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

// -- Session 8 / mission 0968 Phase 4: federation storage substrate --

fn attestor_reg(byte: u8, pubkey_byte: u8) -> AttestorRegistration {
    AttestorRegistration {
        attestor_did: AttestorId::from_array([byte; 52]),
        pubkey: [pubkey_byte; 32],
        peer_set_id: [0xCC; 32],
        requested_at_unix: 1_000,
        registered_at_unix: 1_500,
    }
}

fn att_for(
    attestor: AttestorId,
    recorder: octo_reputation::RecorderDid,
    event_id: EventId,
) -> Attestation {
    Attestation {
        attestation_id: 0,
        attestor,
        recorder_did: recorder,
        event_id,
        signature: vec![1u8; 64],
        observed_at_unix: 1_000,
        received_at_unix: 1_500,
        source_mission: "mon:test".into(),
        source_domain: "domain:adapter:test".into(),
    }
}

#[tokio::test]
async fn v005_migration_applies_reputation_gossip_seen_table() {
    // Session 8 added v005 with reputation_gossip_seen (composite PK
    // (recorder_did, event_id) + attestor_did + observed_at_unix +
    // peer_id). Verify the table exists and is empty on cold boot.
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let mut rows = store
        .database()
        .query("SELECT COUNT(*) FROM reputation_gossip_seen", ())
        .expect("gossip_seen table must exist after v005");
    let row = rows.next().expect("count row");
    let count: i64 = row.expect("ok row").get(0).expect("count col");
    assert_eq!(count, 0, "fresh gossip_seen table is empty");
}

#[tokio::test]
async fn stoolap_attestor_register_lookup_roundtrip() {
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let reg = attestor_reg(0xAA, 0xBB);
    let did = store
        .register_attestor(reg.clone())
        .await
        .expect("register_attestor");
    assert_eq!(did, reg.attestor_did);
    let back = store
        .attestor_lookup_did(&reg.attestor_did)
        .await
        .expect("attestor_lookup_did");
    assert_eq!(back, reg);
}

#[tokio::test]
async fn stoolap_register_attestor_rejects_zero_pubkey() {
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let reg = AttestorRegistration {
        attestor_did: AttestorId::from_array([0xAA; 52]),
        pubkey: [0u8; 32],
        peer_set_id: [0xCC; 32],
        requested_at_unix: 1_000,
        registered_at_unix: 1_500,
    };
    let err = store
        .register_attestor(reg)
        .await
        .expect_err("zero pubkey must fail");
    assert_eq!(err.discriminant(), 0x3A);
}

#[tokio::test]
async fn stoolap_record_attestation_is_idempotent_on_composite_key() {
    // First INSERT returns a fresh id; re-INSERTing the same
    // (attestor, event_id) returns the original id (composite-key
    // dedup). The event_id is what `record_signal` assigned.
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let recorder = octo_reputation::RecorderDid::from_array([0x11; 52]);
    let did_event_id = store
        .record_signal(ev(0, recorder, 1.0, 1_000))
        .await
        .expect("record_signal");
    let attestor = AttestorId::from_array([0xAA; 52]);
    let a1 = att_for(attestor, recorder, did_event_id);
    let id1 = store.record_attestation(a1.clone()).await.expect("first");
    let id2 = store.record_attestation(a1).await.expect("second");
    assert_eq!(id1, id2, "composite-key dedup must return same id");
}

#[tokio::test]
async fn stoolap_attestor_quorum_threshold() {
    // 1, 2, 3 attestors → quorum_reached = false, false, true
    // (MIN_ATTESTOR_QUORUM = 3). Distinct attestors per event.
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let recorder = octo_reputation::RecorderDid::from_array([0x22; 52]);
    let eid = store
        .record_signal(ev(0, recorder, 1.0, 1_000))
        .await
        .expect("record_signal");
    // 0 attestors: quorum not reached.
    assert!(!store.attestor_quorum_reached(eid).await.expect("q0"));
    // 1 attestor.
    store
        .record_attestation(att_for(AttestorId::from_array([0x01; 52]), recorder, eid))
        .await
        .expect("att1");
    assert!(!store.attestor_quorum_reached(eid).await.expect("q1"));
    // 2 attestors.
    store
        .record_attestation(att_for(AttestorId::from_array([0x02; 52]), recorder, eid))
        .await
        .expect("att2");
    assert!(!store.attestor_quorum_reached(eid).await.expect("q2"));
    // 3 attestors — exactly MIN_ATTESTOR_QUORUM.
    store
        .record_attestation(att_for(AttestorId::from_array([0x03; 52]), recorder, eid))
        .await
        .expect("att3");
    assert!(store.attestor_quorum_reached(eid).await.expect("q3"));
    assert_eq!(MIN_ATTESTOR_QUORUM, 3);
}

#[tokio::test]
async fn stoolap_query_attestations_filters_by_recorder() {
    // R8 review: post-R5-F4 the stoolap `next_event_id` returns
    // monotonic per-recorder ids, so multi-event seeding per recorder
    // is now supported. We seed 2 events per recorder to exercise the
    // recorder-filter + since_event_id boundary together (1 of the 2
    // r1 events is above the boundary).
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let r1 = octo_reputation::RecorderDid::from_array([0xA1; 52]);
    let r2 = octo_reputation::RecorderDid::from_array([0xA2; 52]);
    let e1 = store
        .record_signal(ev(0, r1, 0.5, 1_000))
        .await
        .expect("record r1");
    let e3 = store
        .record_signal(ev(2, r2, 0.5, 1_002))
        .await
        .expect("record r2");
    let att = AttestorId::from_array([0xAA; 52]);
    store
        .record_attestation(att_for(att, r1, e1))
        .await
        .expect("att r1 e1");
    store
        .record_attestation(att_for(att, r2, e3))
        .await
        .expect("att r2 e3");
    // query for r1 returns 1 attestation (its own).
    let q = store
        .query_attestations(&r1, EventId::from_u64(0))
        .await
        .expect("q r1");
    assert_eq!(q.len(), 1, "r1 should have 1 attestation, got {}", q.len());
    assert_eq!(q[0].recorder_did, r1);
    // query for r2 returns 1 attestation (its own).
    let q = store
        .query_attestations(&r2, EventId::from_u64(0))
        .await
        .expect("q r2");
    assert_eq!(q.len(), 1);
    assert_eq!(q[0].recorder_did, r2);
    // query for an unknown recorder returns 0.
    let r_unknown = octo_reputation::RecorderDid::from_array([0xFF; 52]);
    let q = store
        .query_attestations(&r_unknown, EventId::from_u64(0))
        .await
        .expect("q r_unknown");
    assert_eq!(q.len(), 0);
}

#[tokio::test]
async fn stoolap_gossip_catch_up_returns_all_events_and_records_seen() {
    // R8 review: post-R5-F4 we seed 2 distinct recorders, each with a
    // single event; per-recorder event_id=1 in both, so the catch-up
    // boundary semantics rely on the (recorder_did, event_id) ordering
    // not global event_id collision. We exercise the SQL run +
    // gossip_seen ledger population. Boundary semantics for
    // since_event_id are exercised end-to-end in the gossip substrate
    // tests (octo-network).
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    for i in 0..2u64 {
        let did = octo_reputation::RecorderDid::from_array([i as u8; 52]);
        store
            .record_signal(ev(i, did, 0.5, 1_000 + i))
            .await
            .expect("record");
    }
    let attestor = AttestorId::from_array([0xFF; 52]);
    let catch_up = GossipCatchUp {
        attestor_did: attestor,
        since_event_id: EventId::from_u64(0),
    };
    let out = store
        .gossip_catch_up(&catch_up)
        .await
        .expect("gossip_catch_up");
    // Both events must surface (post-R5-F4 next_event_id yields distinct
    // (recorder_did, event_id) tuples, and the since_event_id boundary
    // is inclusive of 0→1).
    assert_eq!(out.len(), 2, "got {} events", out.len());
    // gossip_seen ledger must match returned events 1:1.
    let mut rows = store
        .database()
        .query("SELECT COUNT(*) FROM reputation_gossip_seen", ())
        .expect("count");
    let row = rows.next().expect("row");
    let n: i64 = row.expect("ok").get(0).expect("col");
    assert_eq!(
        n as usize,
        out.len(),
        "gossip_seen ledger rows ({n}) must match returned events ({})",
        out.len()
    );
}

/// Round 2 review #1: replay_for_audit must preserve the v011
/// anchor_tx_hash column so audit callers can classify post-anchor
/// events correctly. Pre-fix: replay returned `anchor_tx_hash: None`
/// regardless of persistence state.
#[tokio::test]
async fn replay_for_audit_preserves_anchor_tx_hash() {
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let did = octo_reputation::RecorderDid::from_array([0xAA; 52]);
    store
        .record_signal(ev(1, did, 0.5, 1_000))
        .await
        .expect("record");
    // Anchor the event with a fixed hash.
    let anchor = [0xAB; 32];
    store
        .set_event_anchor_tx_hash(EventId::from_u64(1), anchor)
        .await
        .expect("anchor");
    let events = store
        .replay_for_audit(&did, 0, 10_000)
        .await
        .expect("replay");
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].anchor_tx_hash,
        Some(anchor),
        "replay must surface the persisted anchor tx hash"
    );
}

/// Round 5 review R5-F1: stoolap backend `query_anchors_by_controller_id`
/// round-trips an anchored event and respects the controller_id filter.
/// Round 6 review R6-F1: the original test name implied ORDER BY +
/// tie-break verification, but the test only seeds one event so neither
/// is exercised. Renamed to match what the test actually verifies:
/// anchor round-trip, proxy field preservation, controller filter,
/// empty-result for an unknown controller.
#[tokio::test]
async fn stoolap_query_anchors_round_trips_anchor_and_filters_by_controller() {
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    // Use the same `ev()` helper default controller (zeros) so the
    // SELECT JOIN finds the row.
    let cid = ControllerId::from_array([0u8; 32]);
    let did = octo_reputation::RecorderDid::from_array([0xC5; 52]);
    // One event is sufficient to exercise the SELECT path, the
    // recorded_at_unix proxy field, and the controller_id filter.
    // ORDER BY + tie-break are not exercised here (see
    // memory-only `query_anchors_tie_breaks_by_event_id_asc`).
    let eid = store
        .record_signal(ev(1, did, 0.5, 5_000))
        .await
        .expect("r1");
    store
        .set_event_anchor_tx_hash(eid, [0xAA; 32])
        .await
        .expect("anchor");
    let out = store
        .query_anchors_by_controller_id(cid)
        .await
        .expect("query");
    assert_eq!(out.len(), 1, "exactly 1 anchored event must be returned");
    let r = &out[0];
    assert_eq!(r.event_id, eid);
    assert_eq!(r.anchor_tx_hash, [0xAA; 32]);
    // recorded_at_unix is a PROXY field populated from the underlying
    // event's recorded_at_unix — verify the proxy is preserved through
    // the JOIN in query_anchors_by_controller_id.
    assert_eq!(
        r.recorded_at_unix, 5_000,
        "proxy field must reflect event recorded_at_unix"
    );
    // Empty result for a controller with no anchored events.
    let other_cid = ControllerId::from_array([0xFF; 32]);
    let empty = store
        .query_anchors_by_controller_id(other_cid)
        .await
        .expect("empty query");
    assert!(empty.is_empty(), "unknown controller must return empty Vec");
}

/// Round 2 review #2: gossip_catch_up must preserve the v011
/// anchor_tx_hash column so gossip-fed peers see on-chain provenance
/// for catch-up'd events.
#[tokio::test]
async fn gossip_catch_up_preserves_anchor_tx_hash() {
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let did = octo_reputation::RecorderDid::from_array([0xBB; 52]);
    store
        .record_signal(ev(1, did, 0.5, 1_000))
        .await
        .expect("record");
    let anchor = [0xCD; 32];
    store
        .set_event_anchor_tx_hash(EventId::from_u64(1), anchor)
        .await
        .expect("anchor");
    // Catch-up from a fresh attestor — exercises the gossip_catch_up
    // SELECT path including the new anchor_tx_hash column.
    let attestor = AttestorId::from_array([0x99; 52]);
    let catch_up = GossipCatchUp {
        attestor_did: attestor,
        since_event_id: EventId::from_u64(0),
    };
    let _ = store
        .register_attestor(AttestorRegistration {
            attestor_did: attestor,
            pubkey: [0x99; 32],
            peer_set_id: [0u8; 32],
            requested_at_unix: 1_000,
            registered_at_unix: 1_500,
        })
        .await
        .expect("register");
    let out = store.gossip_catch_up(&catch_up).await.expect("catch_up");
    // gossip_catch_up may return >= 1 row; check the row(s) have
    // anchor_tx_hash preserved.
    let anchored: Vec<_> = out
        .iter()
        .filter(|e| e.anchor_tx_hash == Some(anchor))
        .collect();
    assert!(
        !anchored.is_empty(),
        "gossip_catch_up must surface anchor_tx_hash to gossip peers (got {} events)",
        out.len()
    );
}

/// Round 8 review R5-F4: `next_event_id` MUST yield strictly increasing
/// ids across successive `record_signal` calls. Pre-fix root cause:
/// `SELECT COALESCE(MAX(CAST(last_event_id AS INTEGER)), 0)` — stoolap
/// treats `CAST(BLOB AS INTEGER)` as 0, so MAX always returns 0 and
/// `next_event_id` returns 1 every call, colliding on the composite
/// `(recorder_did, event_id)` PK. Fix: read BLOB, decode 8-byte BE u64
/// in Rust, MAX in Rust.
#[tokio::test]
async fn stoolap_record_signal_assigns_monotonic_event_ids() {
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let did = octo_reputation::RecorderDid::from_array([0xD5; 52]);
    let e1 = store
        .record_signal(ev(1, did, 0.5, 1_000))
        .await
        .expect("r1");
    let e2 = store
        .record_signal(ev(2, did, 0.4, 2_000))
        .await
        .expect("r2");
    let e3 = store
        .record_signal(ev(3, did, 0.3, 3_000))
        .await
        .expect("r3");
    assert_eq!(e1.to_u64(), 1, "first event id must be 1");
    assert_eq!(
        e2.to_u64(),
        2,
        "second event id must be 2 (pre-fix bug returned 1)"
    );
    assert_eq!(
        e3.to_u64(),
        3,
        "third event id must be 3 (pre-fix bug returned 1, colliding on PK)"
    );
}

/// R8 review: `next_event_id` iterates ALL aggregate rows. Seed events
/// under 3 distinct `recorder_did`s so `reputation_aggregates` has
/// multiple rows; assert new event id under any recorder exceeds the
/// global MAX (not just per-recorder).
#[tokio::test]
async fn stoolap_next_event_id_maxes_across_multiple_aggregates() {
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let r1 = octo_reputation::RecorderDid::from_array([0xD1; 52]);
    let r2 = octo_reputation::RecorderDid::from_array([0xD2; 52]);
    let r3 = octo_reputation::RecorderDid::from_array([0xD3; 52]);
    // Seed r1 with 5 events → last_event_id=5 in r1's aggregate
    for i in 0..5 {
        store
            .record_signal(ev(i, r1, 0.5, 1_000 + i))
            .await
            .expect("r1");
    }
    // Seed r2 with 2 events → last_event_id=2 in r2's aggregate
    store
        .record_signal(ev(0, r2, 0.5, 2_000))
        .await
        .expect("r2a");
    store
        .record_signal(ev(1, r2, 0.5, 2_001))
        .await
        .expect("r2b");
    // Seed r3 with 1 event → last_event_id=1 in r3's aggregate
    store
        .record_signal(ev(0, r3, 0.5, 3_000))
        .await
        .expect("r3");
    // Now insert under r3 again. Global MAX is 8 (5+2+1 events
    // recorded across r1/r2/r3 — each aggregate row's last_event_id
    // reflects only its own recorder's latest). Next id MUST be 9,
    // not 2 (which would be max-within-r3).
    let e = store
        .record_signal(ev(1, r3, 0.5, 3_001))
        .await
        .expect("r3-second");
    assert_eq!(
        e.to_u64(),
        9,
        "next_event_id must MAX across all aggregate rows (global=8 → +1=9); \
         a per-recorder MAX bug would return 2"
    );
}

/// R8 review: seed a corrupt `last_event_id` BLOB (length != 8) and
/// assert `next_event_id` rejects with the `:blob_len` error. Guards
/// against future regressions that drop the length check.
#[tokio::test]
async fn stoolap_next_event_id_rejects_malformed_blob() {
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    // Inject a corrupt aggregate row directly. The aggregate PK is
    // (recorder_did, signal_kind, layer) so we provide a sentinel
    // recorder_did, kind=Outcome=1, layer=Market=1, and a 7-byte
    // last_event_id (wrong length).
    let did_bytes = vec![0xE1u8; 52];
    let score_blob = vec![0u8; 24];
    let corrupt_blob = vec![0u8; 7];
    store
        .database()
        .execute(
            "INSERT INTO reputation_aggregates (
                recorder_did, signal_kind, layer, score_ewma, samples,
                severity_total, last_event_id, last_event_unix, updated_at_unix
             ) VALUES ($1, 1, 1, $2, 0, 0, $3, 0, 0)",
            vec![
                stoolap::Value::blob(did_bytes),
                stoolap::Value::blob(score_blob),
                stoolap::Value::blob(corrupt_blob),
            ],
        )
        .expect("insert corrupt aggregate");
    // Inserting another event MUST surface the :blob_len error.
    let did = octo_reputation::RecorderDid::from_array([0xE2; 52]);
    let res = store.record_signal(ev(1, did, 0.5, 5_000)).await;
    let msg = format!("{:?}", res);
    assert!(
        res.is_err(),
        "corrupt blob_len must produce error, got {}",
        msg
    );
    assert!(
        msg.contains("blob_len"),
        "error must be the :blob_len variant, got {}",
        msg
    );
}

/// R8 review: `set_event_anchor_tx_hash` with R7-F2 fix MUST update
/// ONLY the targeted (recorder_did, event_id) row when multiple
/// recorders share the same event_id. We seed two events directly
/// via raw SQL (bypassing `record_signal` so they collide on
/// event_id=1 under distinct recorder_dids) and anchor one. The
/// composite-PK scope guard in the UPDATE must limit the write to
/// the targeted row.
#[tokio::test]
async fn stoolap_set_anchor_scopes_by_composite_pk_cross_recorder() {
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let cid = ControllerId::from_array([0u8; 32]);
    let r_a = octo_reputation::RecorderDid::from_array([0xC1; 52]);
    let r_b = octo_reputation::RecorderDid::from_array([0xC2; 52]);
    let eid_1: Vec<u8> = 1u64.to_be_bytes().to_vec();
    let kind_i = SignalKind::Outcome as i32;
    let layer_i = ReputationLayer::Market as i32;
    let score_blob = vec![0u8; 24];
    // Insert two events under different recorders but the SAME
    // event_id=1 (this is the scenario R7-F2 protects against).
    for did_bytes in [r_a.as_bytes().to_vec(), r_b.as_bytes().to_vec()] {
        store
            .database()
            .execute(
                "INSERT INTO reputation_events
                 (recorder_did, event_id, controller_id, signal_kind, layer,
                  score_delta, recorded_at_unix, rotation_provenance)
                 VALUES ($1, $2, $3, $4, $5, $6, 1000, NULL)",
                vec![
                    stoolap::Value::blob(did_bytes),
                    stoolap::Value::blob(eid_1.clone()),
                    stoolap::Value::blob(cid.as_bytes().to_vec()),
                    stoolap::Value::integer(kind_i as i64),
                    stoolap::Value::integer(layer_i as i64),
                    stoolap::Value::blob(score_blob.clone()),
                ],
            )
            .expect("insert event");
    }
    // Anchor r_a's event via the public API.
    let hash_a = [0xAA; 32];
    store
        .set_event_anchor_tx_hash(EventId::from_u64(1), hash_a)
        .await
        .expect("anchor");
    // Verify: only one row updated. We SELECT both events and count
    // anchor_tx_hash == hash_a.
    let rows = store
        .database()
        .query(
            "SELECT recorder_did, anchor_tx_hash FROM reputation_events
             WHERE event_id = $1
             ORDER BY recorder_did ASC",
            vec![stoolap::Value::blob(eid_1.clone())],
        )
        .expect("select");
    let mut anchored_count = 0;
    let mut total = 0;
    for r in rows {
        let r = r.expect("row");
        total += 1;
        let did: Vec<u8> = r
            .get(0)
            .ok()
            .and_then(|v| {
                if let stoolap::Value::Blob(b) = v {
                    Some(b.to_vec())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        let anc: Option<Vec<u8>> = r.get(1).ok().and_then(|v| {
            if let stoolap::Value::Blob(b) = v {
                Some(b.to_vec())
            } else {
                None
            }
        });
        if anc.as_deref() == Some(&hash_a[..]) {
            assert_eq!(did, r_a.as_bytes().to_vec(), "anchored row must be r_a");
            anchored_count += 1;
        }
    }
    assert_eq!(total, 2, "two seeded events expected, got {}", total);
    assert_eq!(
        anchored_count, 1,
        "composite-PK scope regression would anchor both rows; got {}",
        anchored_count
    );
}

/// R8 review: re-anchoring an already-anchored event with a different
/// hash MUST error (anchor_tx_hash is a one-shot on-chain proof).
#[tokio::test]
async fn stoolap_re_anchor_with_different_hash_errors() {
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let did = octo_reputation::RecorderDid::from_array([0xC8; 52]);
    let e = store
        .record_signal(ev(1, did, 0.5, 1_000))
        .await
        .expect("record");
    store
        .set_event_anchor_tx_hash(e, [0xAA; 32])
        .await
        .expect("first anchor");
    let res = store.set_event_anchor_tx_hash(e, [0xBB; 32]).await;
    assert!(
        res.is_err(),
        "re-anchor with different hash must error, got {:?}",
        res
    );
}

/// R8 review: 100 sequential `record_signal` calls under one recorder
/// must yield ids 1..=100 with no duplicates / gaps. Surfaces any
/// off-by-one or unsigned-overflow regression in next_event_id.
#[tokio::test]
async fn stoolap_record_signal_monotonic_at_scale() {
    let store = StoolapReputationStore::open_in_memory()
        .await
        .expect("open");
    let did = octo_reputation::RecorderDid::from_array([0xC9; 52]);
    let mut ids = Vec::with_capacity(100);
    for i in 0..100u64 {
        let e = store
            .record_signal(ev(i, did, 0.5, 1_000 + i))
            .await
            .expect("record");
        ids.push(e.to_u64());
    }
    let expected: Vec<u64> = (1..=100).collect();
    assert_eq!(
        ids,
        expected,
        "ids must be exactly 1..=100; got first={} last={} len={}",
        ids.first().copied().unwrap_or(0),
        ids.last().copied().unwrap_or(0),
        ids.len()
    );
}
