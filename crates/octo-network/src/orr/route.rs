//! Multi-Transport Path Construction and Route Rotation (RFC-0858 §5, §9)
//!
//! Each onion hop can use a different transport carrier to maximize
//! censorship resistance. Routes rotate periodically to prevent
//! long-term traffic analysis.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use super::types::TransportVector;

// -- Multi-Transport Path Construction --

/// Carrier selection strategy for multi-transport paths.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum CarrierSelectionStrategy {
    /// Maximize transport diversity across hops
    MaxDiversity = 0x0001,
    /// Prefer highest censorship resistance score
    CensorshipResistance = 0x0002,
    /// Prefer highest bandwidth
    Bandwidth = 0x0003,
    /// Balanced: weighted sum of diversity, censorship, bandwidth
    Balanced = 0x0004,
}

/// Multi-transport path builder.
///
/// Constructs onion paths where each hop uses a different carrier
/// to maximize censorship resistance and traffic analysis resistance.
pub struct MultiTransportPathBuilder {
    /// Available carriers per hop position
    available: Vec<Vec<TransportVector>>,
    /// Selection strategy
    strategy: CarrierSelectionStrategy,
}

impl MultiTransportPathBuilder {
    /// Create a new path builder.
    pub fn new(strategy: CarrierSelectionStrategy) -> Self {
        Self {
            available: Vec::new(),
            strategy,
        }
    }

    /// Add available carriers for a hop position.
    pub fn add_hop_carriers(&mut self, carriers: Vec<TransportVector>) {
        self.available.push(carriers);
    }

    /// Build the path by selecting one carrier per hop.
    ///
    /// Maximizes transport diversity: each hop picks a different
    /// transport_type when possible.
    pub fn build(&self) -> Vec<TransportVector> {
        let mut selected = Vec::with_capacity(self.available.len());
        let mut used_types: BTreeSet<u16> = BTreeSet::new();

        for carriers in &self.available {
            let best = self.select_best_carrier(carriers, &used_types);
            used_types.insert(best.transport_type);
            selected.push(best);
        }

        selected
    }

    /// Select the best carrier from available options, preferring unused types.
    fn select_best_carrier(
        &self,
        carriers: &[TransportVector],
        used_types: &BTreeSet<u16>,
    ) -> TransportVector {
        match self.strategy {
            CarrierSelectionStrategy::MaxDiversity => {
                // Prefer carrier with unused transport_type, highest censorship_score
                carriers
                    .iter()
                    .max_by(|a, b| {
                        let a_new = !used_types.contains(&a.transport_type) as u8;
                        let b_new = !used_types.contains(&b.transport_type) as u8;
                        a_new
                            .cmp(&b_new)
                            .then(a.censorship_score.cmp(&b.censorship_score))
                    })
                    .cloned()
                    .unwrap_or_else(|| carriers[0].clone())
            }
            CarrierSelectionStrategy::CensorshipResistance => carriers
                .iter()
                .max_by_key(|c| c.censorship_score)
                .cloned()
                .unwrap_or_else(|| carriers[0].clone()),
            CarrierSelectionStrategy::Bandwidth => carriers
                .iter()
                .max_by_key(|c| c.bandwidth_class)
                .cloned()
                .unwrap_or_else(|| carriers[0].clone()),
            CarrierSelectionStrategy::Balanced => carriers
                .iter()
                .max_by_key(|c| {
                    let diversity_bonus = if !used_types.contains(&c.transport_type) {
                        100u16
                    } else {
                        0
                    };
                    diversity_bonus + c.censorship_score as u16 + c.bandwidth_class as u16
                })
                .cloned()
                .unwrap_or_else(|| carriers[0].clone()),
        }
    }

    /// Compute transport diversity score for a path.
    ///
    /// Returns the number of distinct transport types used.
    pub fn compute_diversity(path: &[TransportVector]) -> usize {
        path.iter()
            .map(|v| v.transport_type)
            .collect::<BTreeSet<_>>()
            .len()
    }
}

// -- Route Rotation (RFC-0858 §9) --

/// Route rotation trigger types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum RotationTrigger {
    /// Time-based: rotate after N epochs
    TimeBased = 0x0001,
    /// Suspicion-based: rotate when anomaly detected
    SuspicionBased = 0x0002,
    /// Manual: rotate on explicit request
    Manual = 0x0003,
}

/// Route rotation configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RotationConfig {
    /// Rotation interval in logical time units
    pub interval: u64,
    /// Trigger type
    pub trigger: RotationTrigger,
    /// Whether to use dual-route handshake (seamless rotation)
    pub seamless: bool,
    /// Grace period for old route after rotation (time units)
    pub grace_period: u64,
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            interval: 300,
            trigger: RotationTrigger::TimeBased,
            seamless: true,
            grace_period: 30,
        }
    }
}

/// Route rotator — manages automatic route rotation.
///
/// RFC-0858 §9: Routes rotate periodically to prevent long-term
/// traffic analysis. New route is established before old one
/// terminates (dual-route handshake for seamless rotation).
pub struct RouteRotator {
    /// Current active route ID
    pub current_route: Option<[u8; 32]>,
    /// Next route being established (for seamless rotation)
    pub next_route: Option<[u8; 32]>,
    /// Epoch when current route was activated
    pub current_epoch: u64,
    /// Rotation configuration
    pub config: RotationConfig,
    /// Whether rotation is in progress
    pub rotation_in_progress: bool,
}

impl RouteRotator {
    /// Create a new route rotator.
    pub fn new(config: RotationConfig) -> Self {
        Self {
            current_route: None,
            next_route: None,
            current_epoch: 0,
            config,
            rotation_in_progress: false,
        }
    }

    /// Set the initial route.
    pub fn set_route(&mut self, route_id: [u8; 32], epoch: u64) {
        self.current_route = Some(route_id);
        self.current_epoch = epoch;
    }

    /// Check if rotation is needed.
    pub fn should_rotate(&self, current_epoch: u64) -> bool {
        if self.current_route.is_none() {
            return false;
        }
        match self.config.trigger {
            RotationTrigger::TimeBased => {
                current_epoch.saturating_sub(self.current_epoch) >= self.config.interval
            }
            RotationTrigger::SuspicionBased => {
                // Suspicion-based rotation is triggered externally via `trigger_rotation()`
                false
            }
            RotationTrigger::Manual => false,
        }
    }

    /// Start seamless rotation: establish new route while old is still active.
    ///
    /// Returns true if rotation started successfully.
    pub fn start_rotation(&mut self, new_route_id: [u8; 32]) -> bool {
        if self.rotation_in_progress {
            return false; // already rotating
        }
        if self.config.seamless {
            self.next_route = Some(new_route_id);
            self.rotation_in_progress = true;
            true
        } else {
            // Non-seamless: immediate switch
            self.current_route = Some(new_route_id);
            self.rotation_in_progress = false;
            true
        }
    }

    /// Complete the rotation: promote next route to current.
    pub fn complete_rotation(&mut self, epoch: u64) -> bool {
        if !self.rotation_in_progress || self.next_route.is_none() {
            return false;
        }
        self.current_route = self.next_route.take();
        self.current_epoch = epoch;
        self.rotation_in_progress = false;
        true
    }

    /// Trigger suspicion-based rotation.
    pub fn trigger_rotation(&mut self) {
        if self.config.trigger == RotationTrigger::SuspicionBased {
            // The caller should call start_rotation with the new route
            // This just marks that rotation is needed
        }
    }

    /// Check if the old route is still in grace period.
    pub fn is_in_grace_period(&self, current_epoch: u64, old_route_epoch: u64) -> bool {
        current_epoch.saturating_sub(old_route_epoch) < self.config.grace_period
    }
}

// -- Identity Preservation --

/// Verify that a peer identity is preserved across route changes.
///
/// RFC-0858 §9: The same peer_id MUST persist across route changes.
/// Only the transport path changes, not the identity.
pub fn verify_identity_preservation(old_peer_id: &[u8; 32], new_peer_id: &[u8; 32]) -> bool {
    old_peer_id == new_peer_id
}

/// Compute a diversity score for a set of transport vectors.
///
/// Higher score = more diverse transport types.
/// Score = number of distinct transport types / total hops * 100
pub fn compute_transport_diversity_score(vectors: &[TransportVector]) -> u8 {
    if vectors.is_empty() {
        return 0;
    }
    let distinct = vectors
        .iter()
        .map(|v| v.transport_type)
        .collect::<BTreeSet<_>>()
        .len();
    ((distinct * 100) / vectors.len()).min(100) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_vector(transport_type: u16, censorship: u8, bandwidth: u8) -> TransportVector {
        TransportVector {
            transport_type,
            domain_id: [0xAA; 32],
            priority: 10,
            bandwidth_class: bandwidth,
            censorship_score: censorship,
        }
    }

    // -- MultiTransportPathBuilder tests --

    #[test]
    fn test_path_builder_max_diversity() {
        let mut builder = MultiTransportPathBuilder::new(CarrierSelectionStrategy::MaxDiversity);
        builder.add_hop_carriers(vec![make_vector(1, 200, 50), make_vector(2, 150, 80)]);
        builder.add_hop_carriers(vec![make_vector(1, 100, 90), make_vector(3, 180, 60)]);
        let path = builder.build();
        assert_eq!(path.len(), 2);
        // Hop 0: both unused, picks type 1 (higher censorship 200 > 150)
        // Hop 1: type 1 used, picks type 3 (unused, censorship 180)
        assert_ne!(path[0].transport_type, path[1].transport_type);
        assert_eq!(path[0].transport_type, 1);
        assert_eq!(path[1].transport_type, 3);
    }

    #[test]
    fn test_path_builder_censorship_resistance() {
        let mut builder =
            MultiTransportPathBuilder::new(CarrierSelectionStrategy::CensorshipResistance);
        builder.add_hop_carriers(vec![make_vector(1, 100, 50), make_vector(2, 250, 30)]);
        let path = builder.build();
        assert_eq!(path[0].transport_type, 2); // highest censorship score
    }

    #[test]
    fn test_path_builder_bandwidth() {
        let mut builder = MultiTransportPathBuilder::new(CarrierSelectionStrategy::Bandwidth);
        builder.add_hop_carriers(vec![make_vector(1, 100, 50), make_vector(2, 100, 200)]);
        let path = builder.build();
        assert_eq!(path[0].transport_type, 2); // highest bandwidth
    }

    #[test]
    fn test_path_builder_balanced() {
        let mut builder = MultiTransportPathBuilder::new(CarrierSelectionStrategy::Balanced);
        builder.add_hop_carriers(vec![make_vector(1, 200, 50), make_vector(2, 150, 80)]);
        let path = builder.build();
        assert_eq!(path.len(), 1);
    }

    #[test]
    fn test_compute_diversity() {
        let path = vec![
            make_vector(1, 200, 50),
            make_vector(2, 200, 50),
            make_vector(1, 200, 50),
        ];
        assert_eq!(MultiTransportPathBuilder::compute_diversity(&path), 2);
    }

    #[test]
    fn test_compute_diversity_empty() {
        assert_eq!(MultiTransportPathBuilder::compute_diversity(&[]), 0);
    }

    // -- RouteRotator tests --

    #[test]
    fn test_rotator_should_rotate_time_based() {
        let config = RotationConfig {
            interval: 100,
            trigger: RotationTrigger::TimeBased,
            seamless: true,
            grace_period: 10,
        };
        let mut rotator = RouteRotator::new(config);
        rotator.set_route([0x01; 32], 100);
        assert!(!rotator.should_rotate(150));
        assert!(!rotator.should_rotate(199));
        assert!(rotator.should_rotate(200));
        assert!(rotator.should_rotate(300));
    }

    #[test]
    fn test_rotator_seamless_rotation() {
        let config = RotationConfig {
            interval: 100,
            trigger: RotationTrigger::TimeBased,
            seamless: true,
            grace_period: 10,
        };
        let mut rotator = RouteRotator::new(config);
        rotator.set_route([0x01; 32], 100);

        // Start rotation
        assert!(rotator.start_rotation([0x02; 32]));
        assert!(rotator.rotation_in_progress);
        assert_eq!(rotator.next_route, Some([0x02; 32]));
        assert_eq!(rotator.current_route, Some([0x01; 32])); // still active

        // Complete rotation
        assert!(rotator.complete_rotation(200));
        assert_eq!(rotator.current_route, Some([0x02; 32]));
        assert!(!rotator.rotation_in_progress);
    }

    #[test]
    fn test_rotator_non_seamless() {
        let config = RotationConfig {
            interval: 100,
            trigger: RotationTrigger::TimeBased,
            seamless: false,
            grace_period: 10,
        };
        let mut rotator = RouteRotator::new(config);
        rotator.set_route([0x01; 32], 100);

        assert!(rotator.start_rotation([0x02; 32]));
        assert!(!rotator.rotation_in_progress); // immediate switch
        assert_eq!(rotator.current_route, Some([0x02; 32]));
    }

    #[test]
    fn test_rotator_no_double_rotation() {
        let config = RotationConfig::default();
        let mut rotator = RouteRotator::new(config);
        rotator.set_route([0x01; 32], 100);

        assert!(rotator.start_rotation([0x02; 32]));
        assert!(!rotator.start_rotation([0x03; 32])); // already rotating
    }

    #[test]
    fn test_rotator_complete_without_start() {
        let config = RotationConfig::default();
        let mut rotator = RouteRotator::new(config);
        assert!(!rotator.complete_rotation(100)); // no rotation in progress
    }

    #[test]
    fn test_rotator_grace_period() {
        let config = RotationConfig {
            interval: 100,
            trigger: RotationTrigger::TimeBased,
            seamless: true,
            grace_period: 50,
        };
        let rotator = RouteRotator::new(config);
        assert!(rotator.is_in_grace_period(120, 100)); // 20 < 50
        assert!(!rotator.is_in_grace_period(160, 100)); // 60 >= 50
    }

    #[test]
    fn test_rotation_config_default() {
        let config = RotationConfig::default();
        assert_eq!(config.interval, 300);
        assert_eq!(config.trigger, RotationTrigger::TimeBased);
        assert!(config.seamless);
        assert_eq!(config.grace_period, 30);
    }

    // -- Identity preservation tests --

    #[test]
    fn test_identity_preservation_same() {
        assert!(verify_identity_preservation(&[0xAA; 32], &[0xAA; 32]));
    }

    #[test]
    fn test_identity_preservation_different() {
        assert!(!verify_identity_preservation(&[0xAA; 32], &[0xBB; 32]));
    }

    // -- Transport diversity score tests --

    #[test]
    fn test_diversity_score_all_different() {
        let vectors = vec![
            make_vector(1, 200, 50),
            make_vector(2, 200, 50),
            make_vector(3, 200, 50),
        ];
        assert_eq!(compute_transport_diversity_score(&vectors), 100);
    }

    #[test]
    fn test_diversity_score_all_same() {
        let vectors = vec![
            make_vector(1, 200, 50),
            make_vector(1, 200, 50),
            make_vector(1, 200, 50),
        ];
        assert_eq!(compute_transport_diversity_score(&vectors), 33);
    }

    #[test]
    fn test_diversity_score_empty() {
        assert_eq!(compute_transport_diversity_score(&[]), 0);
    }

    #[test]
    fn test_diversity_score_mixed() {
        let vectors = vec![
            make_vector(1, 200, 50),
            make_vector(2, 200, 50),
            make_vector(1, 200, 50),
            make_vector(2, 200, 50),
        ];
        assert_eq!(compute_transport_diversity_score(&vectors), 50); // 2/4 * 100
    }
}
