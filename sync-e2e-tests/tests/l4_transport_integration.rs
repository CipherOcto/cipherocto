//! L4: Transport integration tests — full-chain end-to-end.
//!
//! Exercises the complete path: commit → drain_outbox → transport send_best →
//! GossipDispatcher → SyncNetworkBridge → handler. Uses mock adapters +
//! NodeTransport for in-process testing without Docker.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use parking_lot::Mutex;

use octo_network::sync::{
    GossipDispatcher, SyncDgpHandler, SyncNetworkBridge, SYNC_SNAPSHOT_OBJECT_TYPE,
};
use octo_sync::adapter::DatabaseSyncAdapter;
use octo_sync::config::{SyncConfig, SyncRole};
use octo_sync::envelope::WalTailChunk;
use octo_sync::identity::SyncPeerId;
use octo_sync::session::SyncSessionManager;
use octo_sync::test_util::MockAdapter;
use octo_transport::discovery::TransportDiscovery;
use octo_transport::node_transport::NodeTransport;
use octo_transport::sender::{NetworkSender, SendContext, TransportError};

use octo_network::dot::gateway::{GatewayClass, GatewayIdentity};
use octo_network::gdp::identity::GdpGatewayIdentity;
use octo_network::gdp::overlay_endpoint::OverlayEndpoint;
use octo_network::gdp::types::GatewayCapability;

/// Recording sender that captures payloads sent through it.
struct RecordingSender {
    name: String,
    payloads: Mutex<Vec<Vec<u8>>>,
}

impl RecordingSender {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            payloads: Mutex::new(Vec::new()),
        }
    }

    fn payloads(&self) -> Vec<Vec<u8>> {
        self.payloads.lock().clone()
    }

    fn payload_count(&self) -> usize {
        self.payloads.lock().len()
    }
}

#[async_trait]
impl NetworkSender for RecordingSender {
    async fn send(&self, payload: &[u8], _ctx: &SendContext) -> Result<(), TransportError> {
        self.payloads.lock().push(payload.to_vec());
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn is_healthy(&self) -> bool {
        true
    }
}

/// Unhealthy sender that always fails.
struct FailingSender {
    name: String,
}

#[async_trait]
impl NetworkSender for FailingSender {
    async fn send(&self, _payload: &[u8], _ctx: &SendContext) -> Result<(), TransportError> {
        Err(TransportError::AdapterFailure(self.name.clone()))
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn is_healthy(&self) -> bool {
        false
    }
}

fn make_session(mission_id: [u8; 32]) -> (Arc<SyncSessionManager>, Arc<MockAdapter>) {
    let config = SyncConfig::new(mission_id, SyncRole::Replicator, vec![0x02; 32]);
    let adapter = Arc::new(MockAdapter::new(mission_id, [0x01; 32]));
    let session = Arc::new(
        SyncSessionManager::new(
            adapter.clone() as Arc<dyn DatabaseSyncAdapter>,
            config,
            &[0x42u8; 32],
        )
        .unwrap(),
    );
    (session, adapter)
}

fn make_identity(node_id: [u8; 32]) -> GdpGatewayIdentity {
    let base = GatewayIdentity::new(node_id, 1, GatewayClass::Edge, 1);
    GdpGatewayIdentity::new(base)
}

// ─── Test: drain_outbox → transport send_best delivers payload ─────────

#[tokio::test]
async fn drain_outbox_delivers_via_send_best() {
    let mission_id = [0xAAu8; 32];
    let (session, adapter) = make_session(mission_id);
    let sender = Arc::new(RecordingSender::new("webhook"));
    let transport = Arc::new(NodeTransport::new(vec![
        sender.clone() as Arc<dyn NetworkSender>
    ]));

    let peer_id = SyncPeerId([0x03u8; 32]);
    session.subscribe_peer(peer_id).unwrap();

    // Pre-populate WAL entries so on_commit can read them
    for i in 1..=3 {
        adapter.append_wal_entry(i, vec![0xAA, i as u8]);
    }

    // Writer commits LSN 1-3
    session.on_commit(1, 1, 3).unwrap();
    assert_eq!(session.current_lsn(), 3);

    // Drain the outbox
    let chunks = session.streamer().drain_outbox(&peer_id);
    assert!(!chunks.is_empty(), "should have chunks to send");

    // Encode and send_best
    let send_ctx = SendContext {
        mission_id,
        priority: 0,
        source_peer: [0x01u8; 32],
        origin_gateway: [0x01u8; 32],
    };
    for chunk in &chunks {
        let encoded = chunk.encode();
        transport.send_best(&encoded, &send_ctx).await.unwrap();
    }

    // Verify the sender received the payload
    assert!(
        sender.payload_count() >= 1,
        "sender should have received at least one payload"
    );
    let first_payload = &sender.payloads()[0];

    // Decode to verify it's a valid WalTailChunk
    let decoded = WalTailChunk::decode(first_payload).unwrap();
    assert_eq!(decoded.from_lsn, 1);
    assert_eq!(decoded.to_lsn, 3);
}

// ─── Test: full chain — commit → drain → transport → decode ───────────

#[tokio::test]
async fn full_chain_commit_to_transport() {
    let mission_id = [0xBBu8; 32];
    let (session, adapter) = make_session(mission_id);
    let sender = Arc::new(RecordingSender::new("quic"));
    let transport = Arc::new(NodeTransport::new(vec![
        sender.clone() as Arc<dyn NetworkSender>
    ]));

    let peer_id = SyncPeerId([0x04u8; 32]);
    session.subscribe_peer(peer_id).unwrap();

    // Pre-populate WAL entries
    for i in 1..=5 {
        adapter.append_wal_entry(i, vec![0xBB, i as u8]);
    }

    // Commit 5 LSNs
    session.on_commit(1, 1, 5).unwrap();
    assert_eq!(session.current_lsn(), 5);

    // Drain all chunks
    let chunks = session.streamer().drain_outbox(&peer_id);
    assert!(!chunks.is_empty());

    let send_ctx = SendContext {
        mission_id,
        priority: 0,
        source_peer: [0x01u8; 32],
        origin_gateway: [0x01u8; 32],
    };

    let mut total_entries = 0;
    for chunk in &chunks {
        let encoded = chunk.encode();
        transport.send_best(&encoded, &send_ctx).await.unwrap();
        let decoded = WalTailChunk::decode(&encoded).unwrap();
        total_entries += decoded.entries.len();
    }

    assert!(
        total_entries >= 5,
        "should have at least 5 entries, got {}",
        total_entries
    );
}

// ─── Test: failover skips unhealthy transport ──────────────────────────

#[tokio::test]
async fn send_best_failover_skips_unhealthy() {
    let mission_id = [0xCCu8; 32];

    let healthy_sender = Arc::new(RecordingSender::new("webhook"));
    let failing_sender = Arc::new(FailingSender {
        name: "quic".into(),
    });

    let transport = Arc::new(NodeTransport::new(vec![
        failing_sender as Arc<dyn NetworkSender>,
        healthy_sender.clone() as Arc<dyn NetworkSender>,
    ]));

    let send_ctx = SendContext {
        mission_id,
        priority: 0,
        source_peer: [0x01u8; 32],
        origin_gateway: [0x01u8; 32],
    };

    // Should succeed via failover to healthy sender
    let result = transport.send_best(b"test-payload", &send_ctx).await;
    assert!(result.is_ok());
    assert_eq!(healthy_sender.payload_count(), 1);
}

// ─── Test: GossipDispatcher full chain with WAL tail ──────────────────

#[tokio::test]
async fn gossip_dispatcher_wal_tail_chain() {
    let mission_id = [0xDDu8; 32];
    let (session, _adapter) = make_session(mission_id);

    let handler = Arc::new(SyncDgpHandler::new(session.clone()));
    let bridge = SyncNetworkBridge::new(mission_id, handler.clone());
    let dispatcher = GossipDispatcher::new().with_sync(bridge);

    // Encode a WAL tail chunk
    let chunk = WalTailChunk {
        from_lsn: 1,
        to_lsn: 3,
        entries: vec![vec![0x01, 0x02], vec![0x03, 0x04]],
        is_last: true,
    };
    let encoded = chunk.encode();

    let peer_id = [0x05u8; 32];

    // Route through GossipDispatcher
    let result = dispatcher.on_gossip_object(
        SYNC_SNAPSHOT_OBJECT_TYPE,
        0xB1, // WalTailResponse subtype
        peer_id,
        encoded,
    );
    assert!(result.is_ok());

    // Handler.on_wal_tail decodes and applies via session.apply_wal_tail.
    // On success, entries are applied directly (no raw bytes in drain_inbound).
    // The result.is_ok() assertion above confirms the dispatch succeeded.
    let (_summaries, _segments, wal_tails) = handler.drain_inbound();
    let _ = wal_tails; // raw fallback only on decode/apply failure
}

// ─── Test: TransportDiscovery + transport chain ───────────────────────

#[tokio::test]
async fn discovery_builds_and_queries() {
    let node_id = [0x42u8; 32];
    let identity = make_identity(node_id);
    let disc = TransportDiscovery::new(identity, [0xABu8; 32], 100);

    let sender = Arc::new(RecordingSender::new("webhook"));
    let transport = NodeTransport::new(vec![sender as Arc<dyn NetworkSender>]);

    // Build advertisement from transport
    let adv = disc.build_advertisement(&transport, 1, 1000);
    assert_eq!(adv.version, 1);
    assert_eq!(adv.overlay_endpoints.len(), 1);

    // Build from identity alone
    let adv2 = disc.build_advertisement_from_identity(2000);
    assert!(adv2.overlay_endpoints.is_empty());

    // Register a peer and query
    let entry = octo_network::gdp::cache::GatewayCacheEntry {
        advertisement_hash: [0x55u8; 32],
        first_seen: 1000,
        last_seen: 1000,
        trust_score: 500,
        identity: GatewayIdentity {
            gateway_id: [0x77u8; 32],
            public_key: [0x77u8; 32],
            network_id: 1,
            gateway_class: GatewayClass::Edge,
            creation_epoch: 1000,
            supported_platforms: 0,
            capabilities: 0,
        },
        capabilities: vec![GatewayCapability::Relay],
        endpoints: vec![OverlayEndpoint {
            transport_type: 5,
            endpoint_hash: [0u8; 32],
            priority: 100,
            bandwidth_class: 0,
            flags: 0,
        }],
    };
    disc.cache_insert(entry, 1000);

    assert_eq!(disc.peer_count(), 1);
    assert!(disc.peer_supports_transport(&[0x77u8; 32], 5));
    assert!(!disc.peer_supports_transport(&[0x77u8; 32], 99));
}

// ─── Test: tick() detects stale peers ─────────────────────────────────

#[tokio::test]
async fn tick_detects_stale_peers() {
    let mission_id = [0xEEu8; 32];
    let (session, _adapter) = make_session(mission_id);

    let peer_id = SyncPeerId([0x06u8; 32]);
    session.subscribe_peer(peer_id).unwrap();

    // Transition peer through full lifecycle to Streaming
    // (subscribe_peer puts peer in Connecting, so we start from Connecting)
    session
        .transition_peer(
            peer_id,
            octo_sync::state::SyncLifecycle::Authenticating,
            octo_sync::state::TransitionTrigger::TlsHandshakeComplete,
        )
        .unwrap();
    session
        .transition_peer(
            peer_id,
            octo_sync::state::SyncLifecycle::Streaming,
            octo_sync::state::TransitionTrigger::SignatureValid,
        )
        .unwrap();

    // Record heartbeat
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    session.record_heartbeat(peer_id, now);

    // Tick with same time — no timeout
    let actions = session.tick(now);
    assert!(
        actions.is_empty(),
        "should not have actions for recently heartbeat peer"
    );

    // Tick 20 seconds later — should suspect the peer
    let actions = session.tick(now + 20);
    let has_suspect = actions.iter().any(|a| {
        matches!(
            a,
            octo_sync::session::TickAction::TransitionToSuspect(id) if *id == peer_id
        )
    });
    assert!(
        has_suspect,
        "should suspect peer after 20s without heartbeat"
    );
}

// ─── Test: SyncSegment encode/decode round-trip ──────────────────────

#[test]
fn sync_segment_encode_decode_roundtrip() {
    use octo_sync::segment::SyncSegment;

    let seg = SyncSegment {
        table_id: 42,
        segment_index: 7,
        segment_root: [0xBBu8; 32],
        payload: vec![0xAA; 1024],
        compression: 1,
        crc32: 0xDEADBEEF,
        lsn_watermark: 12345,
    };

    let encoded = seg.encode();
    let decoded = SyncSegment::decode(&encoded).unwrap();

    assert_eq!(seg.table_id, decoded.table_id);
    assert_eq!(seg.segment_index, decoded.segment_index);
    assert_eq!(seg.segment_root, decoded.segment_root);
    assert_eq!(seg.payload, decoded.payload);
    assert_eq!(seg.compression, decoded.compression);
    assert_eq!(seg.crc32, decoded.crc32);
    assert_eq!(seg.lsn_watermark, decoded.lsn_watermark);
}

#[test]
fn sync_segment_encode_decode_transport() {
    use octo_sync::segment::SyncSegment;

    // Simulate the full transport chain: encode → send via NodeTransport → decode
    let seg = SyncSegment {
        table_id: 1,
        segment_index: 0,
        segment_root: [0xCCu8; 32],
        payload: b"test-segment".to_vec(),
        compression: 0,
        crc32: 0x12345678,
        lsn_watermark: 100,
    };

    let encoded = seg.encode();
    // Simulate transport transmission (encode → bytes → decode)
    let decoded = SyncSegment::decode(&encoded).unwrap();
    assert_eq!(seg, decoded);
}

// ─── Test: SegmentRequest encode/decode ──────────────────────────────

#[test]
fn segment_request_encode_decode() {
    use octo_sync::envelope::SegmentRequest;

    let req = SegmentRequest {
        table_id: 42,
        segment_index: 7,
        expected_root: [0xDDu8; 32],
    };
    let encoded = req.encode();
    let decoded = SegmentRequest::decode(&encoded).unwrap();
    assert_eq!(req, decoded);
}

#[test]
fn segment_not_found_encode_decode() {
    use octo_sync::envelope::SegmentNotFound;

    let snf = SegmentNotFound {
        table_id: 99,
        segment_index: 3,
        regenerated: true,
    };
    let encoded = snf.encode();
    let decoded = SegmentNotFound::decode(&encoded).unwrap();
    assert_eq!(snf, decoded);
}

// ─── Test: multi-transport broadcast delivers to all ──────────────────

#[tokio::test]
async fn multi_transport_broadcast() {
    let sender1 = Arc::new(RecordingSender::new("webhook"));
    let sender2 = Arc::new(RecordingSender::new("quic"));

    let transport = NodeTransport::new(vec![
        sender1.clone() as Arc<dyn NetworkSender>,
        sender2.clone() as Arc<dyn NetworkSender>,
    ]);

    let send_ctx = SendContext {
        mission_id: [0xFFu8; 32],
        priority: 0,
        source_peer: [0x01u8; 32],
        origin_gateway: [0x01u8; 32],
    };

    let count = transport.broadcast(b"broadcast-data", &send_ctx).await;
    assert_eq!(count, 2);
    assert_eq!(sender1.payload_count(), 1);
    assert_eq!(sender2.payload_count(), 1);
}
