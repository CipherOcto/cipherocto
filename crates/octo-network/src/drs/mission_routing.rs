//! Mission-Aware and Onion-Compatible Routing (RFC-0856 §11, §13, §16, §17, §18)
//!
//! Extends DRS with:
//! - Onion-compatible route construction (per-hop key material for ORR)
//! - Mission-aware routing (geographic isolation, trust gating, bandwidth modes)
//! - Stealth routing (censorship resistance, metadata minimization)
//! - Partition resilience (automatic route recomputation)
//! - Token economics integration (route cost calculation)
//! - AI-native routing (adaptive weight optimization)

use serde::{Deserialize, Serialize};

// ── Onion-Compatible Routing (RFC-0856 §11) ──

/// Per-hop key material for onion route construction.
///
/// Each hop in an onion route needs an ephemeral X25519 key pair
/// for layered encryption (RFC-0858 §3.1).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct OnionHopKey {
    /// Hop index in the route (0 = entry, N-1 = exit)
    pub hop_index: u16,
    /// Ephemeral X25519 public key for this hop
    pub ephemeral_public: [u8; 32],
    /// BLAKE3-256 of the shared secret (X25519(ephemeral_secret, relay_public))
    pub shared_secret_hash: [u8; 32],
    /// Session key derived from shared secret (HKDF-BLAKE3)
    pub session_key: [u8; 32],
}

/// Onion-compatible route with per-hop key material.
///
/// Extends a deterministic route with the key derivation info
/// needed by ORR (RFC-0858) to construct layered encrypted onions.
///
/// **Security note:** This struct is for the sender's use only.
/// Each relay in the onion path only sees its own `OnionHopKey`.
/// The full route topology must never be shared with relays.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OnionRoute {
    /// Route ID (from DeterministicRoute)
    pub route_id: [u8; 32],
    /// Hop keys in order (entry → exit)
    pub hop_keys: Vec<OnionHopKey>,
    /// BLAKE3-256 Merkle root of all hop keys
    pub hop_keys_root: [u8; 32],
    /// Route version for key rotation
    pub version: u64,
}

impl OnionRoute {
    /// Compute the Merkle root of hop keys.
    pub fn compute_hop_keys_root(hop_keys: &[OnionHopKey]) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onion-hop-keys-root:");
        for hk in hop_keys {
            hasher.update(&hk.hop_index.to_be_bytes());
            hasher.update(&hk.ephemeral_public);
            hasher.update(&hk.shared_secret_hash);
            hasher.update(&hk.session_key);
        }
        *hasher.finalize().as_bytes()
    }

    /// Create a new onion route from hop keys.
    pub fn new(route_id: [u8; 32], hop_keys: Vec<OnionHopKey>, version: u64) -> Self {
        let hop_keys_root = Self::compute_hop_keys_root(&hop_keys);
        Self {
            route_id,
            hop_keys,
            hop_keys_root,
            version,
        }
    }

    /// Number of hops in this onion route.
    pub fn hop_count(&self) -> usize {
        self.hop_keys.len()
    }
}

/// Derive per-hop key material for an onion route.
///
/// Uses HKDF-BLAKE3 to derive session keys from shared secrets.
/// Each hop gets a unique session key even if the same relay appears
/// in multiple routes (per-route isolation).
pub fn derive_hop_key(
    hop_index: u16,
    ephemeral_public: [u8; 32],
    shared_secret: &[u8; 32],
    route_id: &[u8; 32],
) -> OnionHopKey {
    // Derive session key: HKDF-BLAKE3(salt="orr:hop_session:v1", ikm=shared_secret, info=route_id||hop_index)
    let mut info = Vec::with_capacity(34);
    info.extend_from_slice(route_id);
    info.extend_from_slice(&hop_index.to_be_bytes());
    let mut session_key = [0u8; 32];
    crate::ocrypt::hkdf_blake3(
        b"orr:hop_session:v1",
        shared_secret,
        &info,
        &mut session_key,
    );

    OnionHopKey {
        hop_index,
        ephemeral_public,
        shared_secret_hash: *blake3::hash(shared_secret).as_bytes(),
        session_key,
    }
}

// ── Mission-Aware Routing (RFC-0856 §13) ──

/// Geographic region identifiers for route isolation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u16)]
pub enum GeoRegion {
    NorthAmerica = 0x0001,
    Europe = 0x0002,
    Asia = 0x0003,
    SouthAmerica = 0x0004,
    Africa = 0x0005,
    Oceania = 0x0006,
    MiddleEast = 0x0007,
    Global = 0x0008,
}

/// Bandwidth class for route selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u16)]
pub enum BandwidthClass {
    /// LoRa: < 1 kbps, duty-cycle aware
    VeryLow = 0x0001,
    /// Bluetooth: < 100 kbps
    Low = 0x0002,
    /// Standard internet
    Medium = 0x0003,
    /// High bandwidth (QUIC, fiber)
    High = 0x0004,
    /// Very high bandwidth (dedicated links)
    VeryHigh = 0x0005,
}

/// Mission routing constraints (RFC-0856 §13).
///
/// Defines constraints that route selection must satisfy
/// for mission-scoped routing.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MissionRouteConstraints {
    /// Geographic isolation: restrict relays to these regions
    pub allowed_regions: Vec<GeoRegion>,
    /// Minimum trust score for relay eligibility
    pub min_trust_score: u64,
    /// Minimum bandwidth class
    pub min_bandwidth: BandwidthClass,
    /// Stealth mode: prefer censorship-resistant carriers
    pub stealth_mode: bool,
    /// Maximum number of hops (used during route construction, not per-relay filtering)
    pub max_hops: u16,
    /// Mission ID for scoped routing
    pub mission_id: [u8; 32],
}

impl Default for MissionRouteConstraints {
    fn default() -> Self {
        Self {
            allowed_regions: vec![GeoRegion::Global],
            min_trust_score: 0,
            min_bandwidth: BandwidthClass::Medium,
            stealth_mode: false,
            max_hops: 5,
            mission_id: [0u8; 32],
        }
    }
}

/// Stealth routing configuration (RFC-0856 §13).
///
/// Minimizes metadata leakage by preferring high-censorship-resistance
/// carriers and randomizing hop selection within trust bounds.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StealthConfig {
    /// Minimum censorship resistance score (0-1000)
    pub min_censorship_resistance: u16,
    /// Prefer random hop selection over deterministic scoring
    pub randomize_hops: bool,
    /// Avoid known surveillance ASNs (list of ASN prefixes)
    pub blocked_asn_prefixes: Vec<u32>,
    /// Cover traffic ratio (0-100, percentage of dummy traffic)
    /// TODO: Not yet implemented — reserved for RFC-0856 §13 cover traffic generation
    pub cover_traffic_ratio: u8,
}

impl Default for StealthConfig {
    fn default() -> Self {
        Self {
            min_censorship_resistance: 500,
            randomize_hops: false,
            blocked_asn_prefixes: Vec::new(),
            cover_traffic_ratio: 0,
        }
    }
}

/// Check if a relay satisfies mission route constraints.
pub fn relay_satisfies_constraints(
    relay_trust: u64,
    relay_region: GeoRegion,
    relay_bandwidth: BandwidthClass,
    relay_censorship_resistance: u16,
    constraints: &MissionRouteConstraints,
    stealth: Option<&StealthConfig>,
) -> bool {
    // Trust gate
    if relay_trust < constraints.min_trust_score {
        return false;
    }

    // Geographic isolation
    if !constraints.allowed_regions.contains(&GeoRegion::Global)
        && !constraints.allowed_regions.contains(&relay_region)
    {
        return false;
    }

    // Bandwidth gate
    if relay_bandwidth < constraints.min_bandwidth {
        return false;
    }

    // Stealth mode: censorship resistance gate
    if constraints.stealth_mode {
        match stealth {
            Some(stealth) => {
                if relay_censorship_resistance < stealth.min_censorship_resistance {
                    return false;
                }
            }
            None => {
                // Stealth required but no config — reject relay
                return false;
            }
        }
    }

    true
}

// ── Partition Resilience (RFC-0856 §16) ──

/// Partition detection state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum PartitionState {
    /// Normal operation
    Healthy = 0x0001,
    /// Degraded: > 20% of known relays unreachable
    Degraded = 0x0002,
    /// Partitioned: > 50% of known relays unreachable
    Partitioned = 0x0003,
    /// Recovering: partition healed, recomputing routes
    Recovering = 0x0004,
}

/// Partition detection metrics.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PartitionMetrics {
    /// Total known relays
    pub total_relays: u32,
    /// Currently reachable relays
    pub reachable_relays: u32,
    /// Current partition state
    pub state: PartitionState,
    /// Epoch when partition was detected
    pub detected_at: u64,
    /// Number of route recomputations triggered
    pub recomputation_count: u32,
}

impl PartitionMetrics {
    /// Compute partition state from relay reachability.
    pub fn compute_state(total: u32, reachable: u32) -> PartitionState {
        if total == 0 {
            return PartitionState::Healthy;
        }
        let reachable = reachable.min(total); // clamp to avoid underflow
        let unreachable_pct = ((total - reachable) * 100) / total;
        if unreachable_pct >= 50 {
            PartitionState::Partitioned
        } else if unreachable_pct >= 20 {
            PartitionState::Degraded
        } else {
            PartitionState::Healthy
        }
    }

    /// Check if route recomputation is needed.
    pub fn needs_recomputation(&self) -> bool {
        matches!(
            self.state,
            PartitionState::Partitioned | PartitionState::Recovering
        )
    }
}

// ── Token Economics (RFC-0856 §17) ──

/// Route cost calculation (RFC-0856 §17).
///
/// Route cost = sum of per-relay costs, where each relay cost
/// is based on bandwidth class, geographic distance, and trust tier.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RouteCostBreakdown {
    /// Base cost per relay hop (OCTO units)
    pub base_cost_per_hop: u64,
    /// Bandwidth multiplier (1.0 = 1000)
    pub bandwidth_multiplier: u32,
    /// Geographic distance penalty (0 = same region, 1000 = cross-continent)
    pub geo_distance_penalty: u32,
    /// Trust tier discount (higher trust = lower cost)
    pub trust_discount_bps: u16,
    /// Total computed cost
    pub total_cost: u64,
}

impl RouteCostBreakdown {
    /// Compute route cost and return full breakdown.
    pub fn compute(
        hop_count: u16,
        bandwidth_class: BandwidthClass,
        geo_distance_penalty: u32,
        trust_discount_bps: u16,
    ) -> Self {
        let base_cost_per_hop: u64 = 100;
        let bw_mult: u64 = match bandwidth_class {
            BandwidthClass::VeryLow => 5000,
            BandwidthClass::Low => 2000,
            BandwidthClass::Medium => 1000,
            BandwidthClass::High => 500,
            BandwidthClass::VeryHigh => 200,
        };
        let bandwidth_multiplier = bw_mult as u32;
        let hop_cost = base_cost_per_hop * bw_mult / 1000;
        let geo_penalty = hop_cost * geo_distance_penalty as u64 / 1000;
        let subtotal = (hop_cost + geo_penalty) * hop_count as u64;
        let discount = subtotal * trust_discount_bps as u64 / 10000;
        let total_cost = subtotal.saturating_sub(discount);
        Self {
            base_cost_per_hop,
            bandwidth_multiplier,
            geo_distance_penalty,
            trust_discount_bps,
            total_cost,
        }
    }
}

/// Compute route cost per RFC-0856 §17.
///
/// Convenience wrapper around `RouteCostBreakdown::compute()`.
pub fn compute_route_cost(
    hop_count: u16,
    bandwidth_class: BandwidthClass,
    geo_distance_penalty: u32,
    trust_discount_bps: u16,
) -> u64 {
    RouteCostBreakdown::compute(
        hop_count,
        bandwidth_class,
        geo_distance_penalty,
        trust_discount_bps,
    )
    .total_cost
}

// ── AI-Native Routing (RFC-0856 §18) ──

/// Adaptive weight optimization for AI-native routing.
///
/// Weights are adjusted based on observed performance metrics.
/// This enables self-tuning route selection that adapts to
/// changing network conditions.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdaptiveWeights {
    /// Trust weight (adjusted by relay failure rate)
    pub trust: u32,
    /// Bandwidth weight (adjusted by throughput observations)
    pub bandwidth: u32,
    /// Latency weight (adjusted by RTT measurements)
    pub latency: u32,
    /// Cost weight (adjusted by budget constraints)
    pub cost: u32,
    /// Censorship resistance weight (adjusted by blocking events)
    pub censorship_resistance: u32,
    /// Total must sum to 10000 (basis points)
    pub total_bps: u32,
}

impl AdaptiveWeights {
    /// Create balanced default weights.
    pub fn balanced() -> Self {
        Self {
            trust: 3000,
            bandwidth: 2000,
            latency: 2000,
            cost: 1500,
            censorship_resistance: 1500,
            total_bps: 10000,
        }
    }

    /// Adjust weights based on observed relay failure.
    /// Increases trust weight, decreases bandwidth weight.
    pub fn adjust_for_failure(&mut self) {
        self.trust = (self.trust + 200).min(5000);
        self.bandwidth = self.bandwidth.saturating_sub(100);
        self.cost = self.cost.saturating_sub(50);
        self.censorship_resistance = (self.censorship_resistance + 100).min(3000);
        self.renormalize();
    }

    /// Adjust weights based on successful high-throughput relay.
    pub fn adjust_for_success(&mut self) {
        self.bandwidth = (self.bandwidth + 100).min(4000);
        self.latency = (self.latency + 50).min(3000);
        self.renormalize();
    }

    /// Renormalize weights to sum to 10000 bps.
    fn renormalize(&mut self) {
        let sum =
            self.trust + self.bandwidth + self.latency + self.cost + self.censorship_resistance;
        if sum == 0 {
            *self = Self::balanced();
            return;
        }
        self.trust = (self.trust as u64 * 10000 / sum as u64) as u32;
        self.bandwidth = (self.bandwidth as u64 * 10000 / sum as u64) as u32;
        self.latency = (self.latency as u64 * 10000 / sum as u64) as u32;
        self.cost = (self.cost as u64 * 10000 / sum as u64) as u32;
        self.censorship_resistance = 10000 - self.trust - self.bandwidth - self.latency - self.cost;
        self.total_bps = 10000;
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    // ── Onion Routing Tests ──

    #[test]
    fn test_onion_hop_key_derivation() {
        let shared_secret = [0x42u8; 32];
        let route_id = [0x01u8; 32];
        let hk = derive_hop_key(0, [0xAA; 32], &shared_secret, &route_id);
        assert_eq!(hk.hop_index, 0);
        assert_eq!(hk.ephemeral_public, [0xAA; 32]);
        // Session key should be deterministic
        let hk2 = derive_hop_key(0, [0xAA; 32], &shared_secret, &route_id);
        assert_eq!(hk.session_key, hk2.session_key);
    }

    #[test]
    fn test_onion_hop_keys_different_indices() {
        let shared_secret = [0x42u8; 32];
        let route_id = [0x01u8; 32];
        let hk0 = derive_hop_key(0, [0xAA; 32], &shared_secret, &route_id);
        let hk1 = derive_hop_key(1, [0xBB; 32], &shared_secret, &route_id);
        assert_ne!(hk0.session_key, hk1.session_key);
    }

    #[test]
    fn test_onion_route_root_deterministic() {
        let keys = vec![
            derive_hop_key(0, [0xAA; 32], &[0x01; 32], &[0x02; 32]),
            derive_hop_key(1, [0xBB; 32], &[0x03; 32], &[0x02; 32]),
        ];
        let r1 = OnionRoute::compute_hop_keys_root(&keys);
        let r2 = OnionRoute::compute_hop_keys_root(&keys);
        assert_eq!(r1, r2);
    }

    #[test]
    fn test_onion_route_hop_count() {
        let keys = vec![
            derive_hop_key(0, [0xAA; 32], &[0x01; 32], &[0x02; 32]),
            derive_hop_key(1, [0xBB; 32], &[0x03; 32], &[0x02; 32]),
            derive_hop_key(2, [0xCC; 32], &[0x04; 32], &[0x02; 32]),
        ];
        let route = OnionRoute::new([0x01; 32], keys, 1);
        assert_eq!(route.hop_count(), 3);
    }

    // ── Mission-Aware Routing Tests ──

    #[test]
    fn test_relay_satisfies_constraints_global() {
        let constraints = MissionRouteConstraints::default();
        assert!(relay_satisfies_constraints(
            100,
            GeoRegion::NorthAmerica,
            BandwidthClass::High,
            500,
            &constraints,
            None,
        ));
    }

    #[test]
    fn test_relay_fails_trust_gate() {
        let constraints = MissionRouteConstraints {
            min_trust_score: 500,
            ..Default::default()
        };
        assert!(!relay_satisfies_constraints(
            100,
            GeoRegion::Global,
            BandwidthClass::High,
            500,
            &constraints,
            None,
        ));
    }

    #[test]
    fn test_relay_fails_geographic_isolation() {
        let constraints = MissionRouteConstraints {
            allowed_regions: vec![GeoRegion::Europe],
            ..Default::default()
        };
        assert!(!relay_satisfies_constraints(
            100,
            GeoRegion::NorthAmerica,
            BandwidthClass::High,
            500,
            &constraints,
            None,
        ));
    }

    #[test]
    fn test_relay_fails_bandwidth_gate() {
        let constraints = MissionRouteConstraints {
            min_bandwidth: BandwidthClass::High,
            ..Default::default()
        };
        assert!(!relay_satisfies_constraints(
            100,
            GeoRegion::Global,
            BandwidthClass::Low,
            500,
            &constraints,
            None,
        ));
    }

    #[test]
    fn test_stealth_mode_censorship_gate() {
        let constraints = MissionRouteConstraints {
            stealth_mode: true,
            ..Default::default()
        };
        let stealth = StealthConfig {
            min_censorship_resistance: 800,
            ..Default::default()
        };
        assert!(!relay_satisfies_constraints(
            100,
            GeoRegion::Global,
            BandwidthClass::High,
            500,
            &constraints,
            Some(&stealth),
        ));
        assert!(relay_satisfies_constraints(
            100,
            GeoRegion::Global,
            BandwidthClass::High,
            900,
            &constraints,
            Some(&stealth),
        ));
    }

    #[test]
    fn test_stealth_mode_no_config_rejects() {
        let constraints = MissionRouteConstraints {
            stealth_mode: true,
            ..Default::default()
        };
        // stealth_mode=true but stealth=None → reject
        assert!(!relay_satisfies_constraints(
            100,
            GeoRegion::Global,
            BandwidthClass::High,
            900,
            &constraints,
            None,
        ));
    }

    // ── Partition Resilience Tests ──

    #[test]
    fn test_partition_state_healthy() {
        assert_eq!(
            PartitionMetrics::compute_state(10, 9),
            PartitionState::Healthy
        );
    }

    #[test]
    fn test_partition_state_degraded() {
        assert_eq!(
            PartitionMetrics::compute_state(10, 7),
            PartitionState::Degraded
        );
    }

    #[test]
    fn test_partition_state_partitioned() {
        assert_eq!(
            PartitionMetrics::compute_state(10, 4),
            PartitionState::Partitioned
        );
    }

    #[test]
    fn test_partition_state_reachable_exceeds_total() {
        // Should clamp and not underflow
        assert_eq!(
            PartitionMetrics::compute_state(10, 15),
            PartitionState::Healthy
        );
    }

    #[test]
    fn test_partition_needs_recomputation() {
        let metrics = PartitionMetrics {
            total_relays: 10,
            reachable_relays: 4,
            state: PartitionState::Partitioned,
            detected_at: 1000,
            recomputation_count: 0,
        };
        assert!(metrics.needs_recomputation());

        let healthy = PartitionMetrics {
            total_relays: 10,
            reachable_relays: 9,
            state: PartitionState::Healthy,
            detected_at: 1000,
            recomputation_count: 0,
        };
        assert!(!healthy.needs_recomputation());
    }

    // ── Token Economics Tests ──

    #[test]
    fn test_route_cost_basic() {
        let cost = compute_route_cost(3, BandwidthClass::Medium, 0, 0);
        assert_eq!(cost, 300); // 3 hops × 100 base × 1.0 bw_mult
    }

    #[test]
    fn test_route_cost_high_bandwidth_cheaper() {
        let cost_high = compute_route_cost(3, BandwidthClass::High, 0, 0);
        let cost_low = compute_route_cost(3, BandwidthClass::Low, 0, 0);
        assert!(cost_high < cost_low);
    }

    #[test]
    fn test_route_cost_trust_discount() {
        let cost_no_discount = compute_route_cost(3, BandwidthClass::Medium, 0, 0);
        let cost_with_discount = compute_route_cost(3, BandwidthClass::Medium, 0, 1000);
        assert!(cost_with_discount < cost_no_discount);
    }

    // ── AI-Native Routing Tests ──

    #[test]
    fn test_adaptive_weights_balanced() {
        let w = AdaptiveWeights::balanced();
        assert_eq!(w.total_bps, 10000);
        assert_eq!(
            w.trust + w.bandwidth + w.latency + w.cost + w.censorship_resistance,
            10000
        );
    }

    #[test]
    fn test_adaptive_weights_failure_adjustment() {
        let mut w = AdaptiveWeights::balanced();
        let trust_before = w.trust;
        w.adjust_for_failure();
        assert!(w.trust > trust_before);
        assert_eq!(w.total_bps, 10000);
    }

    #[test]
    fn test_adaptive_weights_success_adjustment() {
        let mut w = AdaptiveWeights::balanced();
        let bw_before = w.bandwidth;
        w.adjust_for_success();
        assert!(w.bandwidth > bw_before);
        assert_eq!(w.total_bps, 10000);
    }
}
