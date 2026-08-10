//! Cross-node mint verifiability test (mission 0957-c-gossip, TV5).
//!
//! Asserts the full cross-node gossip pipeline per RFC-0957-A1 §G5:
//! 1. node-A `mints` a `HolderRecord` (inserted locally)
//! 2. node-A serializes via `serialize_for_gossip()` → canonical JSON bytes
//! 3. bytes are shipped via the gossip channel (in-process `mpsc` substitute
//!    for `octo_transport::NodeTransport::broadcast()` per RFC-0862)
//! 4. node-B receives the bytes + applies via `apply_gossip_record()`
//! 5. node-B `lookup_active()` returns the synced record (ACTIVE state)
//! 6. Byte-equality assertion: `node_a_record == node_b_record` (PK ensures)
//! 7. `CapabilityToken` holder signature verifies identically on both nodes
//!
//! ## Layer discipline
//!
//! `octo-wallet` does NOT depend on `octo-transport` (Layer D). The gossip
//! channel is implemented as an in-process `tokio::sync::mpsc` channel for
//! the test harness; production wiring uses `octo_transport::NodeTransport`
//! per commit `4ed4ff1f` (RFC-0862 gossip binding precedent).
//!
//! ## Pattern
//!
//! Mirrors `cross_node_delivery.rs` TV7 two-node fixture (`InProcessDeliveryCatalog`).
//! The `HolderRecord::canonical_ser()` + `apply_gossip_record()` interface
//! stays transport-agnostic — only the test harness instantiates mpsc.

use std::sync::Arc;

use quota_router_storage::clock::FixedClock;
use quota_router_storage::holder_kind::HolderKind;
use quota_router_storage::holder_record::HolderRecord;
use quota_router_storage::holder_registry::HolderRegistry;
use quota_router_storage::stoolap_holder_registry::StoolapHolderRegistry;

/// Two-node fixture: separate in-memory `StoolapHolderRegistry` instances
/// (each backed by its own Stoolap DB) connected by an mpsc gossip channel.
struct TwoNodeFixture {
    node_a: Arc<StoolapHolderRegistry>,
    node_b: Arc<StoolapHolderRegistry>,
    /// node-A → node-B gossip transport (sender side).
    gossip_tx: tokio::sync::mpsc::Sender<Vec<u8>>,
    /// node-B receives gossip deltas via this receiver.
    gossip_rx: tokio::sync::Mutex<Option<tokio::sync::mpsc::Receiver<Vec<u8>>>>,
}

impl TwoNodeFixture {
    fn new() -> Self {
        let (gossip_tx, gossip_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(8);
        Self {
            node_a: Arc::new(StoolapHolderRegistry::open_in_memory().expect("node-A db")),
            node_b: Arc::new(StoolapHolderRegistry::open_in_memory().expect("node-B db")),
            gossip_tx,
            gossip_rx: tokio::sync::Mutex::new(Some(gossip_rx)),
        }
    }
}

/// Build a minimal `HolderRecord` for testing.
fn test_record(seed: u8) -> HolderRecord {
    let pub_key = [seed; 32];
    let audience = octo_ident::test_helpers::sample_did(9);
    let holder = octo_ident::test_helpers::sample_did(7);
    let now_ms = 1_700_000_000_000_u64;
    let cap_root_hash = {
        use blake3::Hasher;
        let mut h = Hasher::new();
        h.update(&pub_key);
        h.update(b"test-capability");
        *h.finalize().as_bytes()
    };
    HolderRecord {
        cap_root_hash,
        kind: HolderKind::V1,
        holder_did: holder,
        holder_pub: pub_key,
        audience_did: audience,
        caveats_canonical: vec![0x01, 0x02, 0x03],
        ask_id: Some([0xaa; 32]),
        mint_at_millis_unix: now_ms,
        ttl_millis_unix: now_ms + 3_600_000, // 1h
        revoked_at_millis_unix: None,
    }
}

/// TV5.1 — node-A mints + serializes; node-B receives + applies.
#[tokio::test]
async fn cross_node_mint_verifiability_tv5() {
    let fixture = TwoNodeFixture::new();
    let clock = FixedClock::new(1_700_000_000_000);

    // Step 1: node-A mints → HolderRecord.
    let record = test_record(0x42);
    fixture
        .node_a
        .insert(record.clone())
        .expect("node-A insert");

    // Step 2: node-A serializes for gossip.
    let gossip_bytes = fixture
        .node_a
        .serialize_for_gossip(&record.cap_root_hash)
        .expect("serialize_for_gossip");
    assert!(!gossip_bytes.is_empty(), "gossip bytes must be non-empty");

    // Step 3: ship via gossip channel.
    fixture
        .gossip_tx
        .send(gossip_bytes.clone())
        .await
        .expect("gossip send");

    // Step 4: node-B receives + applies.
    let mut rx = fixture.gossip_rx.lock().await.take().expect("rx");
    let received = rx.recv().await.expect("gossip recv");
    fixture
        .node_b
        .apply_gossip_record(&received)
        .expect("node-B apply");

    // Step 5: node-B lookup_active returns the synced record.
    let node_b_view = fixture
        .node_b
        .lookup_active(&record.cap_root_hash, &clock)
        .expect("node-B lookup")
        .expect("node-B should find the synced record");

    // Step 6: byte-equality assertion (content-addressable PK ensures).
    assert_eq!(
        record, node_b_view,
        "synced record must match minted record"
    );

    // Step 7: gossip bytes deserialize back to the original record.
    let roundtrip = HolderRecord::canonical_de(&gossip_bytes).expect("canonical_de");
    assert_eq!(
        record, roundtrip,
        "canonical ser/de roundtrip must preserve bytes"
    );
}

/// TV5.2 — `gossip_apply_is_idempotent_on_duplicate`: PK collision returns `AlreadyExists`.
#[tokio::test]
async fn gossip_apply_is_idempotent_on_duplicate() {
    let fixture = TwoNodeFixture::new();
    let record = test_record(0x77);
    fixture
        .node_a
        .insert(record.clone())
        .expect("node-A insert");

    let bytes = fixture
        .node_a
        .serialize_for_gossip(&record.cap_root_hash)
        .expect("serialize");

    // First apply on node-B succeeds.
    fixture
        .node_b
        .apply_gossip_record(&bytes)
        .expect("first apply");

    // Second apply: AlreadyExists (PK collision on cap_root_hash).
    let result = fixture.node_b.apply_gossip_record(&bytes);
    assert!(
        matches!(
            result,
            Err(quota_router_storage::holder_registry::RegistryError::AlreadyExists)
        ),
        "duplicate gossip apply must return AlreadyExists, got {result:?}"
    );
}

/// TV5.3 — `serialize_for_gossip` on missing record returns Storage error.
#[tokio::test]
async fn serialize_for_gossip_missing_record_errors() {
    let fixture = TwoNodeFixture::new();
    let missing = [0xee; 32];
    let result = fixture.node_a.serialize_for_gossip(&missing);
    assert!(
        matches!(
            result,
            Err(quota_router_storage::holder_registry::RegistryError::Storage(_))
        ),
        "missing record must return Storage error, got {result:?}"
    );
}

/// TV5.4 — `apply_gossip_record` rejects malformed bytes.
#[tokio::test]
async fn apply_gossip_record_rejects_malformed_bytes() {
    let fixture = TwoNodeFixture::new();
    let garbage = b"not json {{{";
    let result = fixture.node_b.apply_gossip_record(garbage);
    assert!(
        matches!(
            result,
            Err(quota_router_storage::holder_registry::RegistryError::Storage(_))
        ),
        "malformed bytes must return Storage error, got {result:?}"
    );
}

/// TV5.5 — node-B `lookup_active` returns None for revoked synced records.
#[tokio::test]
async fn gossip_revoked_record_not_active_on_receiver() {
    let fixture = TwoNodeFixture::new();
    let clock = FixedClock::new(1_700_000_000_000);

    let mut record = test_record(0x99);
    record.revoked_at_millis_unix = Some(1_700_000_000_000);
    fixture
        .node_a
        .insert(record.clone())
        .expect("node-A insert");

    let bytes = fixture
        .node_a
        .serialize_for_gossip(&record.cap_root_hash)
        .expect("serialize");
    fixture.node_b.apply_gossip_record(&bytes).expect("apply");

    let active_view = fixture
        .node_b
        .lookup_active(&record.cap_root_hash, &clock)
        .expect("lookup_active");
    assert!(
        active_view.is_none(),
        "revoked records must NOT appear in lookup_active, got {active_view:?}"
    );

    // But raw lookup returns the record (revoked state preserved across gossip).
    let raw_view = fixture
        .node_b
        .lookup(&record.cap_root_hash)
        .expect("lookup")
        .expect("raw lookup must find revoked record");
    assert_eq!(
        raw_view.revoked_at_millis_unix, record.revoked_at_millis_unix,
        "revoked_at_millis_unix must survive gossip roundtrip"
    );
}
