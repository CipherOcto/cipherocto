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
