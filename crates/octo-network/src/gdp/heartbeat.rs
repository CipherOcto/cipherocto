//! Gateway Heartbeat (RFC-0851 §12, references RFC-0860 §2.2)

/// Gateway Heartbeat — 7 fields per RFC-0860 §2.2
#[derive(Debug, Clone)]
pub struct GatewayHeartbeat {
    /// Gateway identifier
    pub gateway_id: [u8; 32],
    /// Strictly monotonic sequence
    pub sequence: u64,
    /// Number of active routes
    pub active_routes: u32,
    /// Load class (0-65535)
    pub load_class: u16,
    /// Uptime class (0-65535)
    pub uptime_class: u16,
    /// Logical timestamp
    pub logical_timestamp: u64,
    /// Ed25519 signature
    pub signature: [u8; 64],
}

impl GatewayHeartbeat {
    /// Compute signing bytes (excludes signature)
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.gateway_id);
        bytes.extend_from_slice(&self.sequence.to_be_bytes());
        bytes.extend_from_slice(&self.active_routes.to_be_bytes());
        bytes.extend_from_slice(&self.load_class.to_be_bytes());
        bytes.extend_from_slice(&self.uptime_class.to_be_bytes());
        bytes.extend_from_slice(&self.logical_timestamp.to_be_bytes());
        bytes
    }

    /// Check if heartbeat is expired given a timeout threshold
    pub fn is_expired(&self, current_epoch: u64, timeout_epochs: u64) -> bool {
        current_epoch > self.logical_timestamp.saturating_add(timeout_epochs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heartbeat_signing_bytes() {
        let hb = GatewayHeartbeat {
            gateway_id: [1u8; 32],
            sequence: 1,
            active_routes: 5,
            load_class: 100,
            uptime_class: 200,
            logical_timestamp: 1000,
            signature: [0u8; 64],
        };
        let b1 = hb.to_signing_bytes();
        let b2 = hb.to_signing_bytes();
        assert_eq!(b1, b2);
    }

    #[test]
    fn test_heartbeat_signing_excludes_signature() {
        let mut hb = GatewayHeartbeat {
            gateway_id: [1u8; 32],
            sequence: 1,
            active_routes: 5,
            load_class: 100,
            uptime_class: 200,
            logical_timestamp: 1000,
            signature: [0u8; 64],
        };
        let b1 = hb.to_signing_bytes();
        hb.signature = [0xFFu8; 64];
        let b2 = hb.to_signing_bytes();
        assert_eq!(b1, b2);
    }

    #[test]
    fn test_heartbeat_not_expired() {
        let hb = GatewayHeartbeat {
            gateway_id: [1u8; 32],
            sequence: 1,
            active_routes: 5,
            load_class: 100,
            uptime_class: 200,
            logical_timestamp: 1000,
            signature: [0u8; 64],
        };
        assert!(!hb.is_expired(1050, 100));
    }

    #[test]
    fn test_heartbeat_expired() {
        let hb = GatewayHeartbeat {
            gateway_id: [1u8; 32],
            sequence: 1,
            active_routes: 5,
            load_class: 100,
            uptime_class: 200,
            logical_timestamp: 1000,
            signature: [0u8; 64],
        };
        assert!(hb.is_expired(1200, 100));
    }

    #[test]
    fn test_heartbeat_exact_boundary() {
        let hb = GatewayHeartbeat {
            gateway_id: [1u8; 32],
            sequence: 1,
            active_routes: 5,
            load_class: 100,
            uptime_class: 200,
            logical_timestamp: 1000,
            signature: [0u8; 64],
        };
        // 1100 == 1000 + 100, should NOT be expired (strict >)
        assert!(!hb.is_expired(1100, 100));
    }
}
