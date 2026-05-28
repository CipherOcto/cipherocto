//! Overlay endpoint (RFC-0851 §6)

use serde::{Deserialize, Serialize};

/// Transport endpoint for overlay communication
#[derive(Debug, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct OverlayEndpoint {
    /// Transport type (per RFC-0850 platform types)
    pub transport_type: u16,
    /// BLAKE3-256 of platform endpoint ID
    pub endpoint_hash: [u8; 32],
    /// Lower = preferred
    pub priority: u16,
    /// 0-255
    pub bandwidth_class: u16,
    /// Endpoint flags
    pub flags: u64,
}

impl OverlayEndpoint {
    pub fn new(transport_type: u16, endpoint_hash: [u8; 32]) -> Self {
        Self {
            transport_type,
            endpoint_hash,
            priority: 100,
            bandwidth_class: 0,
            flags: 0,
        }
    }
}
