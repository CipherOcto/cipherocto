//! Mock Network for multi-node integration testing.
//!
//! Simulates N interconnected gateways with configurable topology.
//! Each gateway has a MockPlatformAdapter and can exchange envelopes
//! through a shared message bus.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use tokio::sync::Mutex;

use octo_network::dot::adapters::PlatformAdapter;
use octo_network::dot::domain::PlatformType;
use octo_network::dot::envelope::{DeterministicEnvelope, MessageType};

use super::mock_adapter::{FailureMode, MockPlatformAdapter};

/// A simulated gateway node in the mock network.
pub struct MockGateway {
    /// Gateway identifier
    pub id: [u8; 32],
    /// Mock adapter for this gateway
    pub adapter: MockPlatformAdapter,
    /// Received envelopes (after canonicalization)
    pub received: Arc<Mutex<Vec<DeterministicEnvelope>>>,
}

/// Mock network simulating N interconnected gateways.
///
/// Gateways can send envelopes to each other through the shared bus.
/// The network supports configurable topology and failure injection.
pub struct MockNetwork {
    /// Gateways in the network
    pub gateways: Vec<MockGateway>,
    /// Message bus: gateway_id -> messages destined for it
    bus: Arc<Mutex<BTreeMap<[u8; 32], Vec<Vec<u8>>>>>,
}

impl MockNetwork {
    /// Create a new mock network with N gateways.
    pub fn new(gateway_count: usize) -> Self {
        let mut gateways = Vec::new();
        for i in 0..gateway_count {
            let id = {
                let mut bytes = [0u8; 32];
                bytes[0] = (i + 1) as u8;
                bytes
            };
            let adapter = MockPlatformAdapter::new(PlatformType::NativeP2P);
            gateways.push(MockGateway {
                id,
                adapter,
                received: Arc::new(Mutex::new(Vec::new())),
            });
        }
        Self {
            gateways,
            bus: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    /// Create a mock network with failure modes per gateway.
    pub fn with_failures(gateway_count: usize, failures: Vec<FailureMode>) -> Self {
        let mut net = Self::new(gateway_count);
        for (i, mode) in failures.into_iter().enumerate() {
            if i < net.gateways.len() {
                net.gateways[i].adapter =
                    MockPlatformAdapter::new(PlatformType::NativeP2P).with_failure_mode(mode);
            }
        }
        net
    }

    /// Send an envelope from one gateway to all others (broadcast).
    pub async fn broadcast(&self, sender_idx: usize, envelope: &DeterministicEnvelope) {
        let wire_bytes = envelope.to_wire_bytes();
        let mut bus = self.bus.lock().await;
        for (i, gw) in self.gateways.iter().enumerate() {
            if i != sender_idx {
                bus.entry(gw.id)
                    .or_insert_with(Vec::new)
                    .push(wire_bytes.clone());
            }
        }
    }

    /// Deliver all pending messages to their destination gateways.
    ///
    /// This simulates the network delivering messages.
    pub async fn deliver_all(&self) {
        let mut bus = self.bus.lock().await;
        for gw in &self.gateways {
            if let Some(messages) = bus.remove(&gw.id) {
                for msg in messages {
                    gw.adapter.inject_message(msg).await;
                }
            }
        }
    }

    /// Get the number of pending messages in the bus.
    pub async fn pending_count(&self) -> usize {
        let bus = self.bus.lock().await;
        bus.values().map(|v| v.len()).sum()
    }

    /// Simulate a network partition between two gateways.
    ///
    /// Messages between gateway_a and gateway_b will be dropped.
    pub async fn partition(&self, _gateway_a_idx: usize, _gateway_b_idx: usize) {
        // In a real implementation, this would filter the bus.
        // For now, we mark it conceptually — the test controls delivery.
    }

    /// Create a test envelope with the given parameters.
    pub fn make_envelope(
        envelope_id: [u8; 32],
        network_id: u32,
        source_peer: [u8; 32],
        timestamp: u64,
    ) -> DeterministicEnvelope {
        DeterministicEnvelope {
            version: 1,
            network_id,
            message_type: MessageType::Message as u16,
            envelope_id,
            mission_id: [0u8; 32],
            source_peer,
            origin_gateway: source_peer,
            logical_timestamp: timestamp,
            ttl_hops: 10,
            payload_hash: blake3::hash(b"test payload").into(),
            route_trace_root: [0u8; 32],
            flags: 0,
            signature: [0u8; 64],
        }
    }
}
