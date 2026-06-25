//! DOM Transport Bridge — connects Deterministic Overlay Mempool (RFC-0857) to the transport stack.
//!
//! Propagates admitted `OverlayIntent`s to the network via `TransportBroadcaster`.

use std::sync::Arc;

use octo_network::dom::OverlayIntent;
use octo_network::sync::TransportBroadcaster;

/// Object type for mempool intents in DGP gossip (RFC-0857 §3).
pub const MEMPOOL_INTENT_OBJECT_TYPE: u16 = 0x0009;

/// Bridges DOM intent propagation to `TransportBroadcaster`.
///
/// Serializes `OverlayIntent`s and broadcasts them to the network.
pub struct DomTransportBridge {
    broadcaster: Arc<dyn TransportBroadcaster>,
}

impl DomTransportBridge {
    /// Create a new DOM transport bridge.
    pub fn new(broadcaster: Arc<dyn TransportBroadcaster>) -> Self {
        Self { broadcaster }
    }

    /// Serialize an intent to DGP-compatible bytes.
    ///
    /// Format: `[2-byte object_type LE][intent.to_signing_bytes()]`
    pub fn intent_object_bytes(intent: &OverlayIntent) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&MEMPOOL_INTENT_OBJECT_TYPE.to_le_bytes());
        buf.extend_from_slice(&intent.to_signing_bytes());
        buf
    }

    /// Broadcast an intent to the network.
    ///
    /// Serializes the intent, wraps with the DGP object type header,
    /// and calls `TransportBroadcaster::broadcast()`.
    pub async fn broadcast_intent(
        &self,
        intent: &OverlayIntent,
        mission_id: &[u8; 32],
    ) -> Result<(), std::io::Error> {
        let payload = Self::intent_object_bytes(intent);
        self.broadcaster.broadcast(&payload, mission_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_network::dom::OverlayIntent;

    struct MockBroadcaster;

    #[async_trait::async_trait]
    impl TransportBroadcaster for MockBroadcaster {
        async fn broadcast(
            &self,
            _payload: &[u8],
            _mission_id: &[u8; 32],
        ) -> Result<(), std::io::Error> {
            Ok(())
        }
    }

    struct FailingBroadcaster;

    #[async_trait::async_trait]
    impl TransportBroadcaster for FailingBroadcaster {
        async fn broadcast(
            &self,
            _payload: &[u8],
            _mission_id: &[u8; 32],
        ) -> Result<(), std::io::Error> {
            Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                "mock failure",
            ))
        }
    }

    fn make_intent() -> OverlayIntent {
        OverlayIntent {
            intent_id: [0x01; 32],
            intent_type: 0x0001, // Transaction
            mission_id: [0xAB; 32],
            sender_id: [0x02; 32],
            sequence: 1,
            logical_timestamp: 1000,
            expiration: 2000,
            payload_root: [0x03; 32],
            economic_weight: 5000,
            execution_class: 0x0002, // Standard
            signature: [0x04; 64],
        }
    }

    #[test]
    fn intent_object_bytes_has_correct_header() {
        let intent = make_intent();
        let bytes = DomTransportBridge::intent_object_bytes(&intent);
        // First 2 bytes should be object type 0x0009 in little-endian
        assert_eq!(bytes[0], 0x09);
        assert_eq!(bytes[1], 0x00);
        // Rest should be the intent's signing bytes
        assert!(bytes.len() > 2);
        assert_eq!(&bytes[2..], &intent.to_signing_bytes());
    }

    #[tokio::test]
    async fn broadcast_intent_succeeds() {
        let broadcaster = Arc::new(MockBroadcaster);
        let bridge = DomTransportBridge::new(broadcaster);
        let intent = make_intent();
        let mission_id = [0xAB; 32];
        let result = bridge.broadcast_intent(&intent, &mission_id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn broadcast_intent_propagation_failure() {
        let broadcaster = Arc::new(FailingBroadcaster);
        let bridge = DomTransportBridge::new(broadcaster);
        let intent = make_intent();
        let mission_id = [0xAB; 32];
        let result = bridge.broadcast_intent(&intent, &mission_id).await;
        assert!(result.is_err());
    }
}
