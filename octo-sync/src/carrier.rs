//! Cross-carrier sync (per RFC-0862 Phase 4, mission 0862g).
//!
//! v1 implementation: minimal stub. Multi-carrier propagation (NativeP2P +
//! Webhook + one social adapter) is a Phase 4 enhancement; the v1 single-leader
//! sync uses a single carrier (default: NativeP2P).
//!
//! This module provides the type definitions and a basic broadcast function
//! that fans out a sync envelope to all healthy carriers. The full
//! implementation (per-carrier health tracking, failover thresholds, etc.)
//! is in mission 0862g.

use std::time::Instant;

/// Per-carrier health tracking (per mission 0862g).
#[derive(Debug, Clone)]
pub struct CarrierHealth {
    /// The carrier name (e.g., "nativep2p", "webhook", "telegram").
    pub name: String,
    /// The last heartbeat timestamp.
    pub last_heartbeat: Instant,
    /// The last successful send timestamp.
    pub last_successful_send: Instant,
    /// The success rate over the last 100 attempts.
    pub success_rate: f64,
    /// The average latency in milliseconds over the last 100 attempts.
    pub avg_latency_ms: f64,
}

impl CarrierHealth {
    /// Create a new `CarrierHealth` with default values.
    pub fn new(name: impl Into<String>) -> Self {
        let now = Instant::now();
        Self {
            name: name.into(),
            last_heartbeat: now,
            last_successful_send: now,
            success_rate: 1.0,
            avg_latency_ms: 0.0,
        }
    }

    /// Return `true` if the carrier is healthy (success rate ≥ 0.5).
    pub fn is_healthy(&self) -> bool {
        self.success_rate >= 0.5
    }
}

/// A multi-carrier sync broadcaster (v1 stub).
///
/// The full implementation in mission 0862g has health-based failover,
/// per-carrier rate limiting, and DGP integration.
pub struct MultiCarrierSync {
    /// The list of healthy carriers.
    healthy_carriers: Vec<CarrierHealth>,
}

impl MultiCarrierSync {
    /// Create a new `MultiCarrierSync` with the default carriers
    /// (NativeP2P primary, Webhook secondary).
    pub fn default_carriers() -> Self {
        Self {
            healthy_carriers: vec![
                CarrierHealth::new("nativep2p"),
                CarrierHealth::new("webhook"),
            ],
        }
    }

    /// Broadcast an envelope to all healthy carriers.
    ///
    /// v1 stub: returns the count of healthy carriers that would receive the
    /// envelope. The full implementation in 0862g handles per-carrier
    /// send/ack/fail logic.
    pub fn broadcast(&self, _envelope: &[u8]) -> usize {
        self.healthy_carriers.iter().filter(|c| c.is_healthy()).count()
    }

    /// Return the list of healthy carrier names.
    pub fn healthy_carrier_names(&self) -> Vec<String> {
        self.healthy_carriers
            .iter()
            .filter(|c| c.is_healthy())
            .map(|c| c.name.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_carriers_are_healthy() {
        let m = MultiCarrierSync::default_carriers();
        let names = m.healthy_carrier_names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"nativep2p".to_string()));
        assert!(names.contains(&"webhook".to_string()));
    }

    #[test]
    fn broadcast_returns_carrier_count() {
        let m = MultiCarrierSync::default_carriers();
        let count = m.broadcast(b"some-envelope");
        assert_eq!(count, 2);
    }
}
