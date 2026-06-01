//! Mock Platform Adapter for integration testing.
//!
//! An in-memory implementation of `PlatformAdapter` that simulates
//! platform transport without network dependencies. Supports configurable
//! failure modes for testing adversarial scenarios.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use tokio::sync::Mutex;

use octo_network::dot::adapters::{
    CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::error::PlatformAdapterError;

/// Failure mode for the mock adapter.
#[derive(Clone, Debug)]
pub enum FailureMode {
    /// No failures — messages delivered normally
    None,
    /// Drop all outbound messages
    DropAll,
    /// Drop messages with a given probability (0-100)
    DropRandom(u8),
    /// Duplicate every message N times
    Duplicate(u8),
    /// Reorder messages (swap adjacent pairs)
    Reorder,
    /// Delay messages by N logical time units
    Delay(u64),
}

/// Mock platform adapter for integration testing.
///
/// Implements `PlatformAdapter` with in-memory message queues.
/// Supports message injection (for simulating inbound) and
/// observation (for asserting on outbound).
pub struct MockPlatformAdapter {
    /// Platform type this adapter simulates
    platform: PlatformType,
    /// Outbound messages (sent by the adapter)
    outbound: Arc<Mutex<Vec<Vec<u8>>>>,
    /// Inbound messages (to be received by the adapter)
    inbound: Arc<Mutex<VecDeque<RawPlatformMessage>>>,
    /// Failure mode
    failure_mode: FailureMode,
    /// Self handle for relay loop prevention
    self_id: Option<String>,
    /// Domain hash for this adapter
    domain_hash: [u8; 32],
    /// Message counter for unique IDs
    counter: Arc<Mutex<u64>>,
}

impl MockPlatformAdapter {
    /// Create a new mock adapter for the given platform type.
    pub fn new(platform: PlatformType) -> Self {
        let domain_hash = *blake3::hash(format!("mock:{:?}", platform).as_bytes()).as_bytes();
        Self {
            platform,
            outbound: Arc::new(Mutex::new(Vec::new())),
            inbound: Arc::new(Mutex::new(VecDeque::new())),
            failure_mode: FailureMode::None,
            self_id: None,
            domain_hash,
            counter: Arc::new(Mutex::new(0)),
        }
    }

    /// Set the failure mode.
    pub fn with_failure_mode(mut self, mode: FailureMode) -> Self {
        self.failure_mode = mode;
        self
    }

    /// Set the self handle for relay loop prevention.
    pub fn with_self_handle(mut self, handle: String) -> Self {
        self.self_id = Some(handle);
        self
    }

    /// Inject a message into the inbound queue (simulates receiving from platform).
    pub async fn inject_message(&self, payload: Vec<u8>) {
        let mut inbound = self.inbound.lock().await;
        let mut counter = self.counter.lock().await;
        *counter += 1;
        inbound.push_back(RawPlatformMessage {
            platform_id: format!("mock-{}", *counter),
            payload,
            metadata: BTreeMap::new(),
        });
    }

    /// Get all outbound messages (for assertions).
    pub async fn outbound_messages(&self) -> Vec<Vec<u8>> {
        self.outbound.lock().await.clone()
    }

    /// Get the count of outbound messages.
    pub async fn outbound_count(&self) -> usize {
        self.outbound.lock().await.len()
    }

    /// Clear all queues.
    pub async fn clear(&self) {
        self.outbound.lock().await.clear();
        self.inbound.lock().await.clear();
    }
}

#[async_trait::async_trait]
impl PlatformAdapter for MockPlatformAdapter {
    async fn send_envelope(
        &self,
        _domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let wire_bytes = envelope.to_wire_bytes();

        // Apply failure mode
        match &self.failure_mode {
            FailureMode::DropAll => {
                return Err(PlatformAdapterError::Unreachable {
                    platform: format!("{:?}", self.platform),
                    reason: "drop all mode".into(),
                });
            }
            FailureMode::DropRandom(pct) => {
                let hash = blake3::hash(&wire_bytes);
                let byte = hash.as_bytes()[0];
                if byte < (*pct * 255 / 100) as u8 {
                    return Err(PlatformAdapterError::Unreachable {
                        platform: format!("{:?}", self.platform),
                        reason: "random drop".into(),
                    });
                }
            }
            _ => {}
        }

        let mut outbound = self.outbound.lock().await;
        outbound.push(wire_bytes.clone());

        // Handle duplication
        if let FailureMode::Duplicate(n) = &self.failure_mode {
            for _ in 0..*n {
                outbound.push(wire_bytes.clone());
            }
        }

        let mut counter = self.counter.lock().await;
        *counter += 1;

        Ok(DeliveryReceipt {
            platform_message_id: format!("mock-{}", *counter),
            delivered_at: 0,
        })
    }

    async fn receive_messages(
        &self,
        _domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        let mut inbound = self.inbound.lock().await;
        let messages: Vec<RawPlatformMessage> = inbound.drain(..).collect();

        // Handle reordering
        if let FailureMode::Reorder = &self.failure_mode {
            let mut reordered = messages;
            if reordered.len() >= 2 {
                reordered.swap(0, 1);
            }
            return Ok(reordered);
        }

        Ok(messages)
    }

    fn canonicalize(
        &self,
        raw: &RawPlatformMessage,
    ) -> Result<DeterministicEnvelope, PlatformAdapterError> {
        if raw.payload.is_empty() {
            return Err(PlatformAdapterError::ApiError {
                code: 400,
                message: "empty payload".into(),
            });
        }
        DeterministicEnvelope::from_wire_bytes(&raw.payload).map_err(|e| {
            PlatformAdapterError::ApiError {
                code: 400,
                message: format!("canonicalize: {e}"),
            }
        })
    }

    fn capabilities(&self) -> CapabilityReport {
        CapabilityReport {
            max_payload_bytes: 65536,
            supports_fragmentation: true,
            supports_encryption: true,
            supports_raw_binary: true,
            rate_limit_per_second: 10000,
            media_capabilities: None,
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(self.platform, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        self.platform
    }

    fn self_handle(&self) -> Option<String> {
        self.self_id.clone()
    }
}
