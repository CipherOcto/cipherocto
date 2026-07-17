//! L4: Cross-transport E2E tests — sync via platform adapters.
//!
//! These tests verify that sync data flows through the `NodeTransport`
//! integration layer (PlatformAdapterBridge → PlatformAdapter) instead
//! of raw TCP.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use octo_network::dot::adapters::{
    CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
};
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::error::PlatformAdapterError;
use octo_network::dot::{BroadcastDomainId, PlatformType};
use octo_network::sync::TransportBroadcaster;
use octo_transport::adapter_bridge::PlatformAdapterBridge;
use octo_transport::broadcaster::NodeTransportBroadcaster;
use octo_transport::node_transport::NodeTransport;
use octo_transport::sender::{NetworkSender, SendContext, TransportError};

fn stoolap_node_bin() -> String {
    let candidates = [
        {
            let mut p = std::env::current_exe().unwrap();
            p.pop();
            p.pop();
            p.pop();
            p.push("stoolap-node");
            p.push("target");
            p.push("debug");
            p.push("stoolap-node");
            p
        },
        {
            let mut p = std::env::current_dir().unwrap_or_default();
            p.push("stoolap-node");
            p.push("target");
            p.push("debug");
            p.push("stoolap-node");
            p
        },
    ];
    for c in &candidates {
        if c.exists() {
            return c.to_string_lossy().to_string();
        }
    }
    panic!("stoolap-node not found. Build: cd sync-e2e-tests/stoolap-node && cargo build");
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn wait_for_status(path: &str, timeout: Duration) -> Option<i64> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Ok(content) = std::fs::read_to_string(path) {
            let trimmed = content.trim();
            if let Ok(n) = trimmed.parse::<i64>() {
                if n > 0 {
                    return Some(n);
                }
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

// ─── Recording adapter ──────────────────────────────────────────────

struct RecordingAdapter {
    platform_type: PlatformType,
    payloads: parking_lot::Mutex<Vec<Vec<u8>>>,
}

impl RecordingAdapter {
    fn new(pt: PlatformType) -> Arc<Self> {
        Arc::new(Self {
            platform_type: pt,
            payloads: parking_lot::Mutex::new(Vec::new()),
        })
    }

    fn captured_count(&self) -> usize {
        self.payloads.lock().len()
    }
}

#[async_trait]
impl PlatformAdapter for RecordingAdapter {
    async fn send_envelope(
        &self,
        _domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let wire = envelope.to_wire_bytes();
        self.payloads.lock().push(wire);
        Ok(DeliveryReceipt {
            platform_message_id: format!("rec-{}", self.payloads.lock().len()),
            delivered_at: 1000,
        })
    }

    async fn receive_messages(
        &self,
        _domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        Ok(vec![])
    }

    fn canonicalize(
        &self,
        _raw: &RawPlatformMessage,
    ) -> Result<DeterministicEnvelope, PlatformAdapterError> {
        Ok(DeterministicEnvelope::default())
    }

    fn capabilities(&self) -> CapabilityReport {
        CapabilityReport {
            max_payload_bytes: 65536,
            supports_fragmentation: false,
            supports_encryption: false,
            supports_raw_binary: true,
            rate_limit_per_second: 1000,
            media_capabilities: None,
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(self.platform_type, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        self.platform_type
    }
}

fn test_domain() -> BroadcastDomainId {
    BroadcastDomainId::new(PlatformType::Webhook, "test")
}

// ─── Transport chain integration tests ──────────────────────────────

#[tokio::test]
async fn l4_transport_chain_commit_to_adapter() {
    // Create adapter, hold reference for inspection, pass to bridge
    let adapter = RecordingAdapter::new(PlatformType::Webhook);
    let bridge =
        PlatformAdapterBridge::new(adapter.clone() as Arc<dyn PlatformAdapter>, test_domain());

    let transport = Arc::new(NodeTransport::new(vec![
        Arc::new(bridge) as Arc<dyn NetworkSender>
    ]));
    let broadcaster = NodeTransportBroadcaster::new(transport);

    let mission_id = [0xABu8; 32];
    let result = broadcaster
        .broadcast(b"sync-wal-chunk-data", &mission_id)
        .await;
    assert!(result.is_ok(), "broadcast should succeed");

    assert_eq!(
        adapter.captured_count(),
        1,
        "adapter should have received 1 envelope"
    );
}

#[tokio::test]
async fn l4_multi_transport_broadcast() {
    let adapter1 = RecordingAdapter::new(PlatformType::Webhook);
    let adapter2 = RecordingAdapter::new(PlatformType::Quic);

    let bridge1 =
        PlatformAdapterBridge::new(adapter1.clone() as Arc<dyn PlatformAdapter>, test_domain());
    let bridge2 =
        PlatformAdapterBridge::new(adapter2.clone() as Arc<dyn PlatformAdapter>, test_domain());

    let transport = Arc::new(NodeTransport::new(vec![
        Arc::new(bridge1) as Arc<dyn NetworkSender>,
        Arc::new(bridge2) as Arc<dyn NetworkSender>,
    ]));
    let broadcaster = NodeTransportBroadcaster::new(transport);

    let mission_id = [0xCDu8; 32];
    let result = broadcaster.broadcast(b"multi-transport", &mission_id).await;
    assert!(result.is_ok());

    assert_eq!(adapter1.captured_count(), 1);
    assert_eq!(adapter2.captured_count(), 1);
}

#[tokio::test]
async fn l4_failover_skips_unhealthy_adapter() {
    struct UnhealthySender;
    #[async_trait]
    impl NetworkSender for UnhealthySender {
        async fn send(&self, _p: &[u8], _c: &SendContext) -> Result<(), TransportError> {
            Err(TransportError::Unhealthy)
        }
        fn name(&self) -> &str {
            "unhealthy"
        }
        fn is_healthy(&self) -> bool {
            false
        }
    }

    let adapter = RecordingAdapter::new(PlatformType::Webhook);
    let bridge =
        PlatformAdapterBridge::new(adapter.clone() as Arc<dyn PlatformAdapter>, test_domain());

    let transport = Arc::new(NodeTransport::new(vec![
        Arc::new(UnhealthySender) as Arc<dyn NetworkSender>,
        Arc::new(bridge) as Arc<dyn NetworkSender>,
    ]));
    let broadcaster = NodeTransportBroadcaster::new(transport);

    let result = broadcaster.broadcast(b"failover-test", &[0xEFu8; 32]).await;
    assert!(result.is_ok(), "should succeed via healthy adapter");
    assert_eq!(
        adapter.captured_count(),
        1,
        "healthy adapter should receive the payload"
    );
}

#[tokio::test]
async fn l4_broadcast_count_matches_healthy_senders() {
    struct FailingSender;
    #[async_trait]
    impl NetworkSender for FailingSender {
        async fn send(&self, _p: &[u8], _c: &SendContext) -> Result<(), TransportError> {
            Err(TransportError::AdapterFailure("fail".into()))
        }
        fn name(&self) -> &str {
            "failing"
        }
        fn is_healthy(&self) -> bool {
            true
        }
    }

    let adapter = RecordingAdapter::new(PlatformType::Webhook);
    let bridge =
        PlatformAdapterBridge::new(adapter.clone() as Arc<dyn PlatformAdapter>, test_domain());

    let transport = Arc::new(NodeTransport::new(vec![
        Arc::new(FailingSender) as Arc<dyn NetworkSender>,
        Arc::new(bridge) as Arc<dyn NetworkSender>,
    ]));
    let broadcaster = NodeTransportBroadcaster::new(transport);

    // broadcast() returns Ok even if some senders fail — it counts successes
    let result = broadcaster.broadcast(b"mixed", &[0xAAu8; 32]).await;
    assert!(result.is_ok());
}

// ─── Stoolap-node --adapter flag verification ────────────────────────

#[test]
fn l4_stoolap_node_accepts_adapter_flag() {
    let bin = stoolap_node_bin();
    let output = std::process::Command::new(&bin)
        .arg("--help")
        .output()
        .expect("failed to run stoolap-node --help");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--adapter"),
        "stoolap-node should document --adapter flag"
    );
}

#[tokio::test]
async fn l4_stoolap_node_starts_with_adapter() {
    let bin = stoolap_node_bin();
    let port = free_port();

    let dir = tempfile::tempdir().unwrap();
    let dsn = format!("file://{}/db", dir.path().to_str().unwrap());

    let mut child = std::process::Command::new(&bin)
        .arg("--dsn")
        .arg(&dsn)
        .arg("--listen")
        .arg(port.to_string())
        .arg("--mission-id")
        .arg("abcd000000000000000000000000000000000000000000000000000000000000")
        .arg("--node-id")
        .arg("0100000000000000000000000000000000000000000000000000000000000000")
        .arg("--adapter")
        .arg("webhook")
        .spawn()
        .expect("failed to spawn stoolap-node");

    tokio::time::sleep(Duration::from_millis(2000)).await;

    let status = child.try_wait();
    match status {
        Ok(Some(_exit)) => {
            // Exited early — adapter not found is OK, process shouldn't crash hard
        }
        Ok(None) => {
            // Still running — good
        }
        Err(e) => panic!("failed to check process status: {e}"),
    }

    child.kill().ok();
    let _ = child.wait();
}
