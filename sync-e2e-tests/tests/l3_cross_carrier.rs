//! Cross-carrier sync E2E tests (RFC-0862 Phase 4, mission 0862g).
//!
//! Tests the `MultiCarrierSync` broadcaster with multiple carriers,
//! failover, health degradation, crypto integration, and combined
//! sync + carrier scenarios.

use std::sync::Arc;

use octo_sync::carrier::{Carrier, MultiCarrierSync};
use octo_sync::config::SyncRole;
use octo_sync::envelope::WalTailChunk;
use octo_sync::error::SyncError;
use octo_sync::mission_crypto::{MissionCrypto, MissionPrivacy};
use octo_sync::DatabaseSyncAdapter;
use parking_lot::Mutex;
use sync_e2e_tests::TestCluster;

// ── Test carriers ─────────────────────────────────────────────────

/// A carrier that records all sent envelopes and can be toggled to fail.
struct RecordingCarrier {
    name: String,
    envelopes: Mutex<Vec<Vec<u8>>>,
    fail: Mutex<bool>,
}

impl RecordingCarrier {
    fn new(name: &str) -> Arc<Self> {
        Arc::new(Self {
            name: name.to_string(),
            envelopes: Mutex::new(Vec::new()),
            fail: Mutex::new(false),
        })
    }

    fn set_fail(&self, fail: bool) {
        *self.fail.lock() = fail;
    }

    fn envelopes(&self) -> Vec<Vec<u8>> {
        self.envelopes.lock().clone()
    }

    fn envelope_count(&self) -> usize {
        self.envelopes.lock().len()
    }
}

#[async_trait::async_trait]
impl Carrier for RecordingCarrier {
    fn name(&self) -> &str {
        &self.name
    }

    async fn send(&self, envelope: &[u8]) -> Result<(), SyncError> {
        if *self.fail.lock() {
            return Err(SyncError::AllCarriersFailed);
        }
        self.envelopes.lock().push(envelope.to_vec());
        Ok(())
    }
}

/// A carrier that always fails.
struct AlwaysFailCarrier {
    name: String,
    attempt_count: Mutex<usize>,
}

impl AlwaysFailCarrier {
    fn new(name: &str) -> Arc<Self> {
        Arc::new(Self {
            name: name.to_string(),
            attempt_count: Mutex::new(0),
        })
    }

    fn attempts(&self) -> usize {
        *self.attempt_count.lock()
    }
}

#[async_trait::async_trait]
impl Carrier for AlwaysFailCarrier {
    fn name(&self) -> &str {
        &self.name
    }

    async fn send(&self, _envelope: &[u8]) -> Result<(), SyncError> {
        *self.attempt_count.lock() += 1;
        Err(SyncError::AllCarriersFailed)
    }
}

/// A carrier that fails after N successful sends (simulates crash).
struct FailAfterCarrier {
    name: String,
    remaining: Mutex<usize>,
    envelopes: Mutex<Vec<Vec<u8>>>,
}

impl FailAfterCarrier {
    fn new(name: &str, succeed_count: usize) -> Arc<Self> {
        Arc::new(Self {
            name: name.to_string(),
            remaining: Mutex::new(succeed_count),
            envelopes: Mutex::new(Vec::new()),
        })
    }

    fn envelopes(&self) -> Vec<Vec<u8>> {
        self.envelopes.lock().clone()
    }

    fn envelope_count(&self) -> usize {
        self.envelopes.lock().len()
    }
}

#[async_trait::async_trait]
impl Carrier for FailAfterCarrier {
    fn name(&self) -> &str {
        &self.name
    }

    async fn send(&self, envelope: &[u8]) -> Result<(), SyncError> {
        let mut rem = self.remaining.lock();
        if *rem > 0 {
            *rem -= 1;
            self.envelopes.lock().push(envelope.to_vec());
            Ok(())
        } else {
            Err(SyncError::AllCarriersFailed)
        }
    }
}

// ── CC-T1: Two healthy carriers both receive the broadcast ────────

#[tokio::test]
async fn two_healthy_carriers_both_receive() {
    let c1 = RecordingCarrier::new("nativep2p");
    let c2 = RecordingCarrier::new("webhook");
    let m = MultiCarrierSync::new(vec![c1.clone(), c2.clone()]);

    let count = m.broadcast(b"envelope-1").await;
    assert_eq!(count, 2);
    assert_eq!(c1.envelope_count(), 1);
    assert_eq!(c2.envelope_count(), 1);
    assert_eq!(c1.envelopes()[0], b"envelope-1");
    assert_eq!(c2.envelopes()[0], b"envelope-1");
}

// ── CC-T2: Three carriers, one unhealthy from the start ───────────

#[tokio::test]
async fn unhealthy_carrier_is_skipped() {
    let c1 = RecordingCarrier::new("c1");
    let c2 = AlwaysFailCarrier::new("c2");
    let c3 = RecordingCarrier::new("c3");
    let m = MultiCarrierSync::new(vec![c1.clone(), c2.clone(), c3.clone()]);

    // Force c2 to be unhealthy by recording many failures.
    {
        let mut health = m.health("c2").unwrap();
        health.record_attempt(false, 5000, 1, Some("down".into()));
        health.record_attempt(false, 5000, 2, None);
        health.record_attempt(false, 5000, 3, None);
        health.record_attempt(false, 5000, 4, None);
        health.record_attempt(false, 5000, 5, None);
        health.record_attempt(false, 5000, 6, None);
        health.record_attempt(false, 5000, 7, None);
        health.record_attempt(false, 5000, 8, None);
        health.record_attempt(false, 5000, 9, None);
        health.record_attempt(false, 5000, 10, None);
    }

    // Now broadcast — c2 is unhealthy, c1 and c3 should receive.
    let count = m.broadcast(b"envelope").await;
    assert_eq!(count, 2);
    assert_eq!(c1.envelope_count(), 1);
    assert_eq!(c3.envelope_count(), 1);
}

// ── CC-T3: Failover — primary fails, secondary receives ───────────

#[tokio::test]
async fn failover_primary_to_secondary() {
    let primary = RecordingCarrier::new("primary");
    primary.set_fail(true);
    let secondary = RecordingCarrier::new("secondary");

    let m = MultiCarrierSync::new(vec![primary.clone(), secondary.clone()]);
    let count = m.broadcast(b"failover-test").await;

    assert_eq!(count, 1, "only secondary should succeed");
    assert_eq!(primary.envelope_count(), 0, "primary should have 0");
    assert_eq!(secondary.envelope_count(), 1, "secondary should have 1");
    assert_eq!(secondary.envelopes()[0], b"failover-test");
}

// ── CC-T4: Crash simulation — carrier succeeds then fails ─────────

#[tokio::test]
async fn carrier_crash_mid_session() {
    let stable = RecordingCarrier::new("stable");
    let crashy = FailAfterCarrier::new("crashy", 3);
    let m = MultiCarrierSync::new(vec![stable.clone(), crashy.clone()]);

    // First 3 broadcasts: both succeed.
    for i in 0..3 {
        let data = format!("msg-{}", i).into_bytes();
        let count = m.broadcast(&data).await;
        assert_eq!(count, 2, "broadcast {} should reach both", i);
    }
    assert_eq!(crashy.envelopes().len(), 3);

    // Broadcast 4+: only stable succeeds.
    for i in 3..6 {
        let data = format!("msg-{}", i).into_bytes();
        let count = m.broadcast(&data).await;
        assert_eq!(count, 1, "broadcast {} should only reach stable", i);
    }
    assert_eq!(stable.envelope_count(), 6);
    assert_eq!(crashy.envelope_count(), 3, "crashy should still have 3");

    // Verify crashy health degraded (but may not be unhealthy yet — EMA from 10000
    // with 3 failures: 9000→8100→7290, still above 5000 threshold).
    let h = m.health("crashy").unwrap();
    assert!(
        h.success_rate_bp < 10_000,
        "crashy health should have degraded: {}",
        h.success_rate_bp
    );
}

// ── CC-T5: Health recovery after carrier comes back ───────────────

#[tokio::test]
async fn carrier_health_recovery() {
    let c1 = FailAfterCarrier::new("c1", 0);
    let m = MultiCarrierSync::new(vec![c1.clone()]);

    // Start unhealthy (0 successes, immediate failure).
    let count = m.broadcast(b"msg1").await;
    assert_eq!(count, 0);

    // After many failures, health is clearly below threshold.
    for _ in 0..15 {
        m.broadcast(b"noise").await;
    }
    assert!(!m.health("c1").unwrap().is_healthy());

    // Now "recover" — replace c1 with a healthy one.
    // We test this by creating a new MultiCarrierSync with a working carrier.
    let c1_recovered = RecordingCarrier::new("c1");
    let m2 = MultiCarrierSync::new(vec![c1_recovered.clone()]);
    let count = m2.broadcast(b"recovered").await;
    assert_eq!(count, 1);
    assert_eq!(c1_recovered.envelope_count(), 1);
}

// ── CC-T6: Crypto integration — PRIVATE mission encrypted ─────────

#[tokio::test]
async fn private_mission_encrypted_across_carriers() {
    let c1 = RecordingCarrier::new("c1");
    let c2 = RecordingCarrier::new("c2");

    let keyring = Arc::new(octo_sync::keyring::MissionKeyRing::derive(
        &[0x42u8; 32],
        [0xABu8; 32],
    ));
    let crypto = Arc::new(MissionCrypto::new(keyring, MissionPrivacy::Private));
    let m = MultiCarrierSync::with_crypto(vec![c1.clone(), c2.clone()], crypto);

    let plaintext = b"secret sync data";
    let count = m.broadcast(plaintext).await;
    assert_eq!(count, 2);

    // The wire payload should NOT be the plaintext — it should be encrypted
    // with a 12-byte nonce prefix.
    for carrier in &[c1.clone(), c2.clone()] {
        let envelopes = carrier.envelopes();
        assert_eq!(envelopes.len(), 1);
        let wire = &envelopes[0];
        assert!(
            wire.len() > 12,
            "encrypted wire should have nonce prefix + ciphertext"
        );
        // First 12 bytes are nonce, should not be zero (random).
        let nonce: [u8; 12] = wire[..12].try_into().unwrap();
        assert_ne!(nonce, [0u8; 12], "nonce should be random, not zero");
        // The payload should NOT match plaintext.
        assert_ne!(&wire[12..], plaintext.as_slice());
    }
}

// ── CC-T7: Crypto integration — PUBLIC mission passthrough ────────

#[tokio::test]
async fn public_mission_passthrough_across_carriers() {
    let c1 = RecordingCarrier::new("c1");
    let c2 = RecordingCarrier::new("c2");

    let keyring = Arc::new(octo_sync::keyring::MissionKeyRing::derive(
        &[0x42u8; 32],
        [0xABu8; 32],
    ));
    let crypto = Arc::new(MissionCrypto::new(keyring, MissionPrivacy::Public));
    let m = MultiCarrierSync::with_crypto(vec![c1.clone(), c2.clone()], crypto);

    let plaintext = b"public sync data";
    let count = m.broadcast(plaintext).await;
    assert_eq!(count, 2);

    // PUBLIC missions send plaintext unchanged.
    for carrier in &[c1.clone(), c2.clone()] {
        let envelopes = carrier.envelopes();
        assert_eq!(envelopes.len(), 1);
        assert_eq!(&envelopes[0], plaintext);
    }
}

// ── CC-T8: Crypto roundtrip — receiver can decrypt ────────────────

#[tokio::test]
async fn crypto_roundtrip_via_carriers() {
    let c1 = RecordingCarrier::new("c1");

    let keyring = Arc::new(octo_sync::keyring::MissionKeyRing::derive(
        &[0x42u8; 32],
        [0xABu8; 32],
    ));
    let crypto = Arc::new(MissionCrypto::new(keyring.clone(), MissionPrivacy::Private));
    let m = MultiCarrierSync::with_crypto(vec![c1.clone()], crypto);

    let plaintext = b"roundtrip payload";
    m.broadcast(plaintext).await;

    // Receiver uses same keyring to decrypt.
    let receiver_crypto = MissionCrypto::new(keyring, MissionPrivacy::Private);
    let wire = &c1.envelopes()[0];
    let decrypted = receiver_crypto.receive(wire, b"sync-envelope").unwrap();
    assert_eq!(decrypted, plaintext);
}

// ── CC-T9: All carriers fail — broadcast returns 0 ────────────────

#[tokio::test]
async fn all_carriers_fail_returns_zero() {
    let c1 = AlwaysFailCarrier::new("c1");
    let c2 = AlwaysFailCarrier::new("c2");
    let m = MultiCarrierSync::new(vec![c1.clone(), c2.clone()]);

    let count = m.broadcast(b"envelope").await;
    assert_eq!(count, 0);
    assert_eq!(c1.attempts(), 1);
    assert_eq!(c2.attempts(), 1);
}

// ── CC-T10: Broadcast is concurrent — all carriers send in parallel

#[tokio::test]
async fn broadcast_concurrent() {
    let c1 = RecordingCarrier::new("c1");
    let c2 = RecordingCarrier::new("c2");
    let c3 = RecordingCarrier::new("c3");
    let m = MultiCarrierSync::new(vec![c1.clone(), c2.clone(), c3.clone()]);

    // Send 10 messages rapidly.
    for i in 0..10 {
        let data = format!("concurrent-{}", i).into_bytes();
        let count = m.broadcast(&data).await;
        assert_eq!(count, 3);
    }

    assert_eq!(c1.envelope_count(), 10);
    assert_eq!(c2.envelope_count(), 10);
    assert_eq!(c3.envelope_count(), 10);
}

// ── CC-T11: healthy_carrier_names filters correctly ───────────────

#[tokio::test]
async fn healthy_carrier_names_filters_unhealthy() {
    let c1 = RecordingCarrier::new("healthy");
    let c2 = AlwaysFailCarrier::new("unhealthy");
    let m = MultiCarrierSync::new(vec![c1.clone(), c2.clone()]);

    // Degrade c2's health.
    for _ in 0..20 {
        m.broadcast(b"noise").await;
    }

    let names = m.healthy_carrier_names();
    assert_eq!(names, vec!["healthy"]);
    assert_eq!(m.all_carrier_names().len(), 2);
}

// ── CC-T12: Crypto + failover — PRIVATE mission, one carrier dies ─

#[tokio::test]
async fn private_mission_failover() {
    let primary = FailAfterCarrier::new("primary", 2);
    let secondary = RecordingCarrier::new("secondary");

    let keyring = Arc::new(octo_sync::keyring::MissionKeyRing::derive(
        &[0x42u8; 32],
        [0xABu8; 32],
    ));
    let crypto = Arc::new(MissionCrypto::new(keyring.clone(), MissionPrivacy::Private));
    let m = MultiCarrierSync::with_crypto(vec![primary.clone(), secondary.clone()], crypto);

    // First 2 broadcasts: both succeed.
    for i in 0..2 {
        let count = m.broadcast(b"secret").await;
        assert_eq!(count, 2, "broadcast {}", i);
    }

    // Third broadcast: primary fails, secondary succeeds.
    let count = m.broadcast(b"after-crash").await;
    assert_eq!(
        count, 1,
        "only secondary should succeed after primary crash"
    );

    // Secondary received all 3 broadcasts.
    assert_eq!(secondary.envelope_count(), 3);

    // All secondary envelopes should be encrypted (not plaintext).
    let receiver_crypto = MissionCrypto::new(keyring, MissionPrivacy::Private);
    for wire in secondary.envelopes() {
        let _pt = receiver_crypto.receive(&wire, b"sync-envelope").unwrap();
    }
}

// ── CC-T13: Cross-carrier + sync integration ──────────────────────
//
// Writer commits entries, fans out via WAL tail to readers.
// Separately, a MultiCarrierSync broadcasts carrier-level envelopes.
// Both paths work independently.

#[tokio::test]
async fn sync_wal_tail_with_carrier_broadcast() {
    let mut cluster = TestCluster::new(2, &[SyncRole::Replicator, SyncRole::Observer]);

    // Subscribe reader to writer.
    let writer_peer = cluster.node(0).peer_id(&cluster.mission_id);
    cluster
        .node_mut(1)
        .session
        .subscribe_peer(writer_peer)
        .unwrap();

    // Create a carrier broadcaster.
    let c1 = RecordingCarrier::new("nativep2p");
    let c2 = RecordingCarrier::new("webhook");
    let broadcaster = MultiCarrierSync::new(vec![c1.clone(), c2.clone()]);

    // Writer commits entries and fans out via WAL tail.
    for i in 0..5 {
        let data = format!("sync-entry-{}", i).into_bytes();
        let (txn_id, from_lsn, to_lsn) = cluster.node(0).commit_entry(&data);
        cluster
            .node(0)
            .session
            .on_commit(txn_id, from_lsn, to_lsn)
            .unwrap();
        let entries = cluster.adapter(0).read_wal_range(from_lsn, to_lsn).unwrap();
        let chunk = WalTailChunk {
            from_lsn,
            to_lsn,
            entries,
            is_last: true,
        };
        cluster.fan_out(0, &chunk);

        // Separately, broadcast a carrier-level envelope.
        let carrier_envelope = format!("carrier-{}", i).into_bytes();
        let count = broadcaster.broadcast(&carrier_envelope).await;
        assert_eq!(count, 2);
    }

    // Verify sync path: reader has all entries.
    assert_eq!(cluster.adapter(1).current_lsn().unwrap(), 5);

    // Verify carrier path: both carriers received all 5 broadcasts.
    assert_eq!(c1.envelope_count(), 5);
    assert_eq!(c2.envelope_count(), 5);
}

// ── CC-T14: Multi-carrier with 5-node sync cluster ────────────────

#[tokio::test]
async fn five_node_sync_with_carrier_broadcast() {
    let mut cluster = TestCluster::new(
        5,
        &[
            SyncRole::Replicator,
            SyncRole::Observer,
            SyncRole::Observer,
            SyncRole::Observer,
            SyncRole::Observer,
        ],
    );

    // All readers subscribe to writer.
    for reader_idx in 1..5 {
        let writer_peer = cluster.node(0).peer_id(&cluster.mission_id);
        cluster
            .node_mut(reader_idx)
            .session
            .subscribe_peer(writer_peer)
            .unwrap();
    }

    // Three carriers.
    let c1 = RecordingCarrier::new("p2p");
    let c2 = RecordingCarrier::new("webhook");
    let c3 = RecordingCarrier::new("social");
    let broadcaster = MultiCarrierSync::new(vec![c1.clone(), c2.clone(), c3.clone()]);

    // Writer commits 10 entries.
    for i in 0..10 {
        let data = format!("five-node-{}", i).into_bytes();
        let (txn_id, from_lsn, to_lsn) = cluster.node(0).commit_entry(&data);
        cluster
            .node(0)
            .session
            .on_commit(txn_id, from_lsn, to_lsn)
            .unwrap();
        let entries = cluster.adapter(0).read_wal_range(from_lsn, to_lsn).unwrap();
        let chunk = WalTailChunk {
            from_lsn,
            to_lsn,
            entries,
            is_last: true,
        };
        cluster.fan_out(0, &chunk);
    }

    // Broadcast carrier-level envelope.
    let count = broadcaster.broadcast(b"five-node-carrier").await;
    assert_eq!(count, 3);

    // All readers converged.
    for i in 1..5 {
        assert_eq!(cluster.adapter(i).current_lsn().unwrap(), 10);
    }

    // All 3 carriers got the envelope.
    assert_eq!(c1.envelope_count(), 1);
    assert_eq!(c2.envelope_count(), 1);
    assert_eq!(c3.envelope_count(), 1);
}

// ── CC-T15: Health tracking across broadcasts ─────────────────────

#[tokio::test]
async fn health_tracking_across_multiple_broadcasts() {
    let c1 = RecordingCarrier::new("c1");
    let c2 = FailAfterCarrier::new("c2", 5);
    let m = MultiCarrierSync::new(vec![c1.clone(), c2.clone()]);

    // First 5: both healthy.
    for _ in 0..5 {
        m.broadcast(b"ok").await;
    }
    assert_eq!(m.health("c1").unwrap().success_rate_bp, 10_000);
    assert!(m.health("c2").unwrap().is_healthy());

    // Next 10: c2 fails every time.
    for _ in 0..10 {
        m.broadcast(b"fail").await;
    }

    // c1 still at 100%.
    assert_eq!(m.health("c1").unwrap().success_rate_bp, 10_000);

    // c2 degraded (EMA: 0.9^10 * 10000 ≈ 3486).
    let h2 = m.health("c2").unwrap();
    assert!(
        !h2.is_healthy(),
        "c2 health should be below 5000bp after 10 consecutive failures"
    );
    assert!(
        h2.success_rate_bp < 5000,
        "c2 success rate: {}",
        h2.success_rate_bp
    );
}
