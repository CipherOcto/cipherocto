//! GDP Discovery Scopes and Lifecycle (RFC-0851 §2, §8)
//!
//! Discovery plane handles visibility/topology; data plane handles envelope routing.
//! Discovery lifecycle: Bootstrap → Expansion → Stabilization.

use serde::{Deserialize, Serialize};

use super::types::{DiscoveryLifecycle, DiscoveryScope};
use super::GdpError;

/// TTL values per discovery scope (RFC-0851 §3)
pub const TTL_LOCAL: u16 = 3;
pub const TTL_REGIONAL: u16 = 10;
pub const TTL_MISSION: u16 = 5;
pub const TTL_GLOBAL: u16 = 20;
pub const TTL_PRIVATE: u16 = 3;
pub const TTL_CONSENSUS: u16 = 10;

/// Bootstrap method for initial peer discovery (RFC-0851 §8.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u16)]
pub enum BootstrapMethod {
    Static = 0x0001,        // Hardcoded seed list
    QrBlob = 0x0002,        // Human-transferable bootstrap blob
    LanBroadcast = 0x0003,  // LAN broadcast
    DotDomain = 0x0004,     // Existing DOT broadcast domain
    TrustedPeers = 0x0005,  // Trusted peer introductions
    MissionInvite = 0x0006, // Mission-scoped invitation
}

/// Scope filter for gateway visibility (RFC-0851 §2)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeFilter {
    /// Which scopes this gateway is visible in
    pub visible_scopes: Vec<DiscoveryScope>,
    /// Mission ID for mission-scoped discovery (None for non-mission scopes)
    pub mission_id: Option<[u8; 32]>,
}

impl ScopeFilter {
    pub fn global_only() -> Self {
        Self {
            visible_scopes: vec![DiscoveryScope::Global],
            mission_id: None,
        }
    }

    pub fn all_public() -> Self {
        Self {
            visible_scopes: vec![
                DiscoveryScope::Local,
                DiscoveryScope::Regional,
                DiscoveryScope::Global,
            ],
            mission_id: None,
        }
    }

    pub fn mission(mission_id: [u8; 32]) -> Self {
        Self {
            visible_scopes: vec![DiscoveryScope::Mission],
            mission_id: Some(mission_id),
        }
    }

    /// Check if a gateway with this filter is visible in the given scope.
    pub fn is_visible_in(&self, scope: &DiscoveryScope) -> bool {
        self.visible_scopes.contains(scope)
    }
}

/// Default TTL for a discovery scope.
pub fn default_ttl(scope: &DiscoveryScope) -> u16 {
    match scope {
        DiscoveryScope::Local => TTL_LOCAL,
        DiscoveryScope::Regional => TTL_REGIONAL,
        DiscoveryScope::Mission => TTL_MISSION,
        DiscoveryScope::Global => TTL_GLOBAL,
        DiscoveryScope::Private => TTL_PRIVATE,
        DiscoveryScope::Consensus => TTL_CONSENSUS,
    }
}

/// Minimum OCTO stake per scope (RFC-0851 §11.1)
pub fn min_octo_for_scope(scope: &DiscoveryScope) -> u64 {
    match scope {
        DiscoveryScope::Local => 0,
        DiscoveryScope::Regional => 500,
        DiscoveryScope::Mission => 1000,
        DiscoveryScope::Global => 1000,
        DiscoveryScope::Private => 0,
        DiscoveryScope::Consensus => 1000,
    }
}

/// Minimum OCTO-B role stake per scope (RFC-0851 §11.1)
pub fn min_octo_b_for_scope(scope: &DiscoveryScope) -> u64 {
    match scope {
        DiscoveryScope::Local => 0,
        DiscoveryScope::Regional => 50,
        DiscoveryScope::Mission => 100,
        DiscoveryScope::Global => 100,
        DiscoveryScope::Private => 0,
        DiscoveryScope::Consensus => 200,
    }
}

/// Discovery state machine tracking lifecycle phase transitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryState {
    /// Current lifecycle phase
    pub phase: DiscoveryLifecycle,
    /// Bootstrap method used
    pub bootstrap_method: Option<BootstrapMethod>,
    /// Number of peers discovered
    pub peer_count: u32,
    /// Number of expansion rounds completed
    pub expansion_rounds: u32,
    /// Epoch when stabilization was reached (0 if not yet)
    pub stabilized_at: u64,
}

impl DiscoveryState {
    pub fn new(method: BootstrapMethod) -> Self {
        Self {
            phase: DiscoveryLifecycle::Bootstrap,
            bootstrap_method: Some(method),
            peer_count: 0,
            expansion_rounds: 0,
            stabilized_at: 0,
        }
    }

    /// Transition to expansion phase (requires >= 1 peer discovered).
    pub fn start_expansion(&mut self) -> Result<(), GdpError> {
        if self.phase != DiscoveryLifecycle::Bootstrap {
            return Err(GdpError::InvalidAdvertisement {
                reason: format!(
                    "Cannot start expansion from phase {:?}, must be Bootstrap",
                    self.phase
                ),
            });
        }
        if self.peer_count < 5 {
            return Err(GdpError::InvalidAdvertisement {
                reason: format!(
                    "Cannot start expansion with {} peers, need >= 5 (RFC Section 13)",
                    self.peer_count
                ),
            });
        }
        self.phase = DiscoveryLifecycle::Expansion;
        Ok(())
    }

    /// Transition to stabilization phase.
    pub fn stabilize(&mut self, current_epoch: u64) -> Result<(), GdpError> {
        if self.phase != DiscoveryLifecycle::Expansion {
            return Err(GdpError::InvalidAdvertisement {
                reason: format!(
                    "Cannot stabilize from phase {:?}, must be Expansion",
                    self.phase
                ),
            });
        }
        self.phase = DiscoveryLifecycle::Stabilization;
        self.stabilized_at = current_epoch;
        Ok(())
    }

    /// Add discovered peers during expansion.
    pub fn add_discovered_peers(&mut self, count: u32) {
        self.peer_count = self.peer_count.saturating_add(count);
    }

    /// Record an expansion round.
    pub fn record_expansion_round(&mut self) {
        self.expansion_rounds = self.expansion_rounds.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_filter_global_only() {
        let filter = ScopeFilter::global_only();
        assert!(filter.is_visible_in(&DiscoveryScope::Global));
        assert!(!filter.is_visible_in(&DiscoveryScope::Local));
        assert!(!filter.is_visible_in(&DiscoveryScope::Regional));
    }

    #[test]
    fn test_scope_filter_all_public() {
        let filter = ScopeFilter::all_public();
        assert!(filter.is_visible_in(&DiscoveryScope::Local));
        assert!(filter.is_visible_in(&DiscoveryScope::Regional));
        assert!(filter.is_visible_in(&DiscoveryScope::Global));
        assert!(!filter.is_visible_in(&DiscoveryScope::Private));
    }

    #[test]
    fn test_scope_filter_mission() {
        let mission_id = [0xABu8; 32];
        let filter = ScopeFilter::mission(mission_id);
        assert!(filter.is_visible_in(&DiscoveryScope::Mission));
        assert_eq!(filter.mission_id, Some(mission_id));
        assert!(!filter.is_visible_in(&DiscoveryScope::Global));
    }

    #[test]
    fn test_default_ttl_values() {
        assert_eq!(default_ttl(&DiscoveryScope::Local), 3);
        assert_eq!(default_ttl(&DiscoveryScope::Regional), 10);
        assert_eq!(default_ttl(&DiscoveryScope::Mission), 5);
        assert_eq!(default_ttl(&DiscoveryScope::Global), 20);
        assert_eq!(default_ttl(&DiscoveryScope::Private), 3);
        assert_eq!(default_ttl(&DiscoveryScope::Consensus), 10);
    }

    #[test]
    fn test_stake_requirements() {
        assert_eq!(min_octo_for_scope(&DiscoveryScope::Local), 0);
        assert_eq!(min_octo_for_scope(&DiscoveryScope::Regional), 500);
        assert_eq!(min_octo_for_scope(&DiscoveryScope::Global), 1000);
        assert_eq!(min_octo_for_scope(&DiscoveryScope::Consensus), 1000);
    }

    #[test]
    fn test_octo_b_stake_requirements() {
        assert_eq!(min_octo_b_for_scope(&DiscoveryScope::Local), 0);
        assert_eq!(min_octo_b_for_scope(&DiscoveryScope::Regional), 50);
        assert_eq!(min_octo_b_for_scope(&DiscoveryScope::Global), 100);
        assert_eq!(min_octo_b_for_scope(&DiscoveryScope::Consensus), 200);
    }

    #[test]
    fn test_discovery_lifecycle_transitions() {
        let mut state = DiscoveryState::new(BootstrapMethod::Static);
        assert_eq!(state.phase, DiscoveryLifecycle::Bootstrap);

        // Cannot expand with fewer than 5 peers (RFC Section 13)
        assert!(state.start_expansion().is_err());

        state.add_discovered_peers(5);
        assert!(state.start_expansion().is_ok());
        assert_eq!(state.phase, DiscoveryLifecycle::Expansion);

        state.record_expansion_round();
        assert_eq!(state.expansion_rounds, 1);

        assert!(state.stabilize(1000).is_ok());
        assert_eq!(state.phase, DiscoveryLifecycle::Stabilization);
        assert_eq!(state.stabilized_at, 1000);
    }

    #[test]
    fn test_discovery_bad_transition() {
        let mut state = DiscoveryState::new(BootstrapMethod::QrBlob);
        state.add_discovered_peers(5);
        state.start_expansion().unwrap();

        // Cannot go back to bootstrap
        assert!(state.start_expansion().is_err());
    }

    #[test]
    fn test_scope_repr_values() {
        assert_eq!(DiscoveryScope::Local as u16, 0x0001);
        assert_eq!(DiscoveryScope::Regional as u16, 0x0002);
        assert_eq!(DiscoveryScope::Mission as u16, 0x0003);
        assert_eq!(DiscoveryScope::Global as u16, 0x0004);
        assert_eq!(DiscoveryScope::Private as u16, 0x0005);
        assert_eq!(DiscoveryScope::Consensus as u16, 0x0006);
    }

    #[test]
    fn test_scope_filter_serialization_roundtrip() {
        let filter = ScopeFilter::mission([0xCDu8; 32]);
        let json = serde_json::to_string(&filter).unwrap();
        let deserialized: ScopeFilter = serde_json::from_str(&json).unwrap();
        assert_eq!(filter.mission_id, deserialized.mission_id);
        assert_eq!(
            filter.visible_scopes.len(),
            deserialized.visible_scopes.len()
        );
    }
}
