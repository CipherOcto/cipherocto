//! Gateway Heartbeat (RFC-0860 §3.2)

use serde::{Deserialize, Serialize};

/// Gateway Heartbeat — proves gateway is online and responsive.
///
/// 7 fields per RFC-0860 §3.2.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct GatewayHeartbeat {
    /// Gateway sending the heartbeat
    pub gateway_id: [u8; 32],
    /// Monotonically increasing sequence
    pub sequence: u64,
    /// Number of active relay routes
    pub active_routes: u32,
    /// Load class (0-255, where 0 = idle, 255 = saturated)
    pub load_class: u8,
    /// Uptime class (0-255, where 0 = just started, 255 = maximum uptime)
    pub uptime_class: u8,
    /// Logical timestamp
    pub logical_timestamp: u64,
    /// Ed25519 signature over all above fields
    pub signature: Vec<u8>,
}

impl GatewayHeartbeat {
    /// Compute canonical bytes for signing (all fields except signature)
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(32 + 8 + 4 + 1 + 1 + 8);
        buf.extend_from_slice(&self.gateway_id);
        buf.extend_from_slice(&self.sequence.to_be_bytes());
        buf.extend_from_slice(&self.active_routes.to_be_bytes());
        buf.push(self.load_class);
        buf.push(self.uptime_class);
        buf.extend_from_slice(&self.logical_timestamp.to_be_bytes());
        buf
    }
}

/// Load classification for heartbeats
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum LoadClass {
    Idle = 0x00,
    Light = 0x40,
    Moderate = 0x80,
    Heavy = 0xC0,
    Saturated = 0xFF,
}

/// Uptime classification for heartbeats
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum UptimeClass {
    JustStarted = 0x00,
    Minutes = 0x40,
    Hours = 0x80,
    Days = 0xC0,
    Maximum = 0xFF,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heartbeat_signing_bytes_size() {
        let hb = GatewayHeartbeat {
            gateway_id: [0u8; 32],
            sequence: 1,
            active_routes: 5,
            load_class: 0x80,
            uptime_class: 0xC0,
            logical_timestamp: 100,
            signature: vec![0u8; 64],
        };
        assert_eq!(hb.to_signing_bytes().len(), 54);
    }

    #[test]
    fn test_load_class_values() {
        assert_eq!(LoadClass::Idle as u8, 0x00);
        assert_eq!(LoadClass::Saturated as u8, 0xFF);
    }

    #[test]
    fn test_uptime_class_values() {
        assert_eq!(UptimeClass::JustStarted as u8, 0x00);
        assert_eq!(UptimeClass::Maximum as u8, 0xFF);
    }
}
