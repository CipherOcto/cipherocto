//! GDP Discovery Gossip Propagation (RFC-0851 §13)
//!
//! Bridges GDP discovery with DGP gossip infrastructure.
//! GDP advertisements propagate as DGP `GossipObject` with
//! `object_type = DiscoveryAdvertisement`.
//!
//! ## Gossip Modes by Lifecycle State
//!
//! | Lifecycle     | Gossip Mode    | When                              |
//! |---------------|----------------|-----------------------------------|
//! | Bootstrap     | Flood          | < 5 known gateways, broadcast all |
//! | Expansion     | Incremental    | Growing peer graph                |
//! | Stabilization | Incremental    | Steady state                      |
//! | Degraded      | Anti-Entropy   | > 33% unreachable                 |
//! | Recovering    | Flood          | Healing after partition           |
//!
//! ## Scope → TTL Mapping
//!
//! | DiscoveryScope | GossipScope  | TTL (hops) |
//! |----------------|--------------|------------|
//! | Local          | LOCAL        | 3          |
//! | Regional       | REGIONAL     | 10         |
//! | Mission        | MISSION      | 5          |
//! | Global         | GLOBAL       | 20         |
//! | Private        | PRIVATE      | 3          |
//! | Consensus      | CONSENSUS    | 10         |

use super::types::{DiscoveryLifecycle, DiscoveryScope};
use crate::dgp::domain::{GossipDomainId, GossipScope};
use crate::dgp::object::{
    GossipObject, GossipObjectType, FLAG_ANTI_ENTROPY, FLAG_DIRECTED, FLAG_FLOOD, FLAG_INCREMENTAL,
};

// ── Scope Mapping ──

/// Map GDP DiscoveryScope to DGP GossipScope (RFC-0851 §13).
pub fn scope_to_gossip_scope(scope: DiscoveryScope) -> GossipScope {
    match scope {
        DiscoveryScope::Local => GossipScope::LOCAL,
        DiscoveryScope::Regional => GossipScope::REGIONAL,
        DiscoveryScope::Mission => GossipScope::MISSION,
        DiscoveryScope::Global => GossipScope::GLOBAL,
        DiscoveryScope::Private => GossipScope::PRIVATE,
        DiscoveryScope::Consensus => GossipScope::CONSENSUS,
    }
}

/// Map GDP DiscoveryScope to a DGP GossipDomainId.
///
/// For Mission scope, the mission_id should be provided from the
/// GDP ScopeFilter. For other scopes, mission_id is zero.
pub fn scope_to_gossip_domain(
    scope: DiscoveryScope,
    network_id: u32,
    mission_id: [u8; 32],
) -> GossipDomainId {
    GossipDomainId::new(network_id, mission_id, scope_to_gossip_scope(scope))
}

/// Default TTL for a discovery scope (RFC-0851 §13 table).
pub fn scope_ttl(scope: DiscoveryScope) -> u16 {
    scope_to_gossip_scope(scope).default_ttl()
}

// ── Lifecycle → Gossip Mode ──

/// Gossip mode selected based on GDP lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryGossipMode {
    /// Broadcast aggressively to all peers (bootstrap, recovering)
    Flood,
    /// Propagate only unseen advertisements (expansion, stabilization)
    Incremental,
    /// Periodic Merkle summary reconciliation (degraded)
    AntiEntropy,
    /// Targeted propagation for mission overlays
    Directed,
}

/// Select gossip mode based on GDP lifecycle state (RFC-0851 §13).
pub fn lifecycle_to_gossip_mode(lifecycle: DiscoveryLifecycle) -> DiscoveryGossipMode {
    match lifecycle {
        DiscoveryLifecycle::Bootstrap => DiscoveryGossipMode::Flood,
        DiscoveryLifecycle::Expansion => DiscoveryGossipMode::Incremental,
        DiscoveryLifecycle::Stabilization => DiscoveryGossipMode::Incremental,
        DiscoveryLifecycle::Degraded => DiscoveryGossipMode::AntiEntropy,
        DiscoveryLifecycle::Recovering => DiscoveryGossipMode::Flood,
    }
}

/// Get the propagation flag for a gossip mode.
pub fn mode_to_flag(mode: DiscoveryGossipMode) -> u64 {
    match mode {
        DiscoveryGossipMode::Flood => FLAG_FLOOD,
        DiscoveryGossipMode::Incremental => FLAG_INCREMENTAL,
        DiscoveryGossipMode::AntiEntropy => FLAG_ANTI_ENTROPY,
        DiscoveryGossipMode::Directed => FLAG_DIRECTED,
    }
}

// ── Advertisement Wrapping ──

/// Create a DGP GossipObject for a GDP discovery advertisement.
///
/// Wraps the gateway identity into a `DiscoveryAdvertisement` object
/// with the appropriate scope, TTL, and propagation flags.
pub fn wrap_advertisement(
    gateway_id: [u8; 32],
    scope: DiscoveryScope,
    network_id: u32,
    mission_id: [u8; 32],
    logical_timestamp: u64,
    payload_root: [u8; 32],
    mode: DiscoveryGossipMode,
) -> GossipObject {
    let domain = scope_to_gossip_domain(scope, network_id, mission_id);
    let ttl = scope_ttl(scope);

    GossipObject {
        object_type: GossipObjectType::DiscoveryAdvertisement as u16,
        object_hash: [0u8; 32], // Will be computed by derive_object_hash
        object_size: 0,         // Set by caller after serialization
        domain_id: domain,
        logical_timestamp,
        origin_gateway: gateway_id,
        ttl_hops: ttl,
        propagation_flags: mode_to_flag(mode),
        payload_root,
        signature: [0u8; 64], // Set by caller after signing
    }
}

/// Check if a GossipObject is a DiscoveryAdvertisement.
pub fn is_discovery_advertisement(obj: &GossipObject) -> bool {
    obj.object_type == GossipObjectType::DiscoveryAdvertisement as u16
}

/// Select objects eligible for the current gossip mode.
///
/// Filters objects by the mode's propagation flag.
pub fn select_for_mode<'a>(
    objects: &'a [GossipObject],
    mode: DiscoveryGossipMode,
) -> Vec<&'a GossipObject> {
    match mode {
        DiscoveryGossipMode::Flood => objects
            .iter()
            .filter(|o| is_discovery_advertisement(o) && o.has_flag(FLAG_FLOOD) && o.ttl_hops > 0)
            .collect(),
        DiscoveryGossipMode::Incremental => objects
            .iter()
            .filter(|o| {
                is_discovery_advertisement(o) && o.has_flag(FLAG_INCREMENTAL) && o.ttl_hops > 0
            })
            .collect(),
        DiscoveryGossipMode::AntiEntropy => objects
            .iter()
            .filter(|o| {
                is_discovery_advertisement(o) && o.has_flag(FLAG_ANTI_ENTROPY) && o.ttl_hops > 0
            })
            .collect(),
        DiscoveryGossipMode::Directed => objects
            .iter()
            .filter(|o| {
                is_discovery_advertisement(o) && o.has_flag(FLAG_DIRECTED) && o.ttl_hops > 0
            })
            .collect(),
    }
}

/// Deduplicate advertisements by gateway_id + sequence (logical_timestamp).
///
/// Returns only the newest advertisement per gateway.
pub fn deduplicate_by_gateway(objects: &[GossipObject]) -> Vec<&GossipObject> {
    use std::collections::HashMap;
    let mut newest: HashMap<[u8; 32], &GossipObject> = HashMap::new();

    for obj in objects {
        if !is_discovery_advertisement(obj) {
            continue;
        }
        let gw = obj.origin_gateway;
        newest
            .entry(gw)
            .and_modify(|existing| {
                if obj.logical_timestamp > existing.logical_timestamp {
                    *existing = obj;
                }
            })
            .or_insert(obj);
    }

    newest.into_values().collect()
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    fn make_gateway_id(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn make_adv(
        gateway_byte: u8,
        scope: DiscoveryScope,
        timestamp: u64,
        mode: DiscoveryGossipMode,
    ) -> GossipObject {
        wrap_advertisement(
            make_gateway_id(gateway_byte),
            scope,
            1,
            [0u8; 32],
            timestamp,
            [0xAA; 32],
            mode,
        )
    }

    #[test]
    fn test_scope_to_gossip_scope_mapping() {
        assert_eq!(
            scope_to_gossip_scope(DiscoveryScope::Local),
            GossipScope::LOCAL
        );
        assert_eq!(
            scope_to_gossip_scope(DiscoveryScope::Regional),
            GossipScope::REGIONAL
        );
        assert_eq!(
            scope_to_gossip_scope(DiscoveryScope::Mission),
            GossipScope::MISSION
        );
        assert_eq!(
            scope_to_gossip_scope(DiscoveryScope::Global),
            GossipScope::GLOBAL
        );
        assert_eq!(
            scope_to_gossip_scope(DiscoveryScope::Private),
            GossipScope::PRIVATE
        );
        assert_eq!(
            scope_to_gossip_scope(DiscoveryScope::Consensus),
            GossipScope::CONSENSUS
        );
    }

    #[test]
    fn test_scope_ttl_values() {
        assert_eq!(scope_ttl(DiscoveryScope::Local), 3);
        assert_eq!(scope_ttl(DiscoveryScope::Regional), 10);
        assert_eq!(scope_ttl(DiscoveryScope::Mission), 5);
        assert_eq!(scope_ttl(DiscoveryScope::Global), 20);
        assert_eq!(scope_ttl(DiscoveryScope::Private), 3);
        assert_eq!(scope_ttl(DiscoveryScope::Consensus), 10);
    }

    #[test]
    fn test_lifecycle_to_gossip_mode() {
        assert_eq!(
            lifecycle_to_gossip_mode(DiscoveryLifecycle::Bootstrap),
            DiscoveryGossipMode::Flood
        );
        assert_eq!(
            lifecycle_to_gossip_mode(DiscoveryLifecycle::Expansion),
            DiscoveryGossipMode::Incremental
        );
        assert_eq!(
            lifecycle_to_gossip_mode(DiscoveryLifecycle::Stabilization),
            DiscoveryGossipMode::Incremental
        );
        assert_eq!(
            lifecycle_to_gossip_mode(DiscoveryLifecycle::Degraded),
            DiscoveryGossipMode::AntiEntropy
        );
        assert_eq!(
            lifecycle_to_gossip_mode(DiscoveryLifecycle::Recovering),
            DiscoveryGossipMode::Flood
        );
    }

    #[test]
    fn test_wrap_advertisement_type() {
        let adv = make_adv(
            0x01,
            DiscoveryScope::Global,
            1000,
            DiscoveryGossipMode::Flood,
        );
        assert_eq!(
            adv.object_type,
            GossipObjectType::DiscoveryAdvertisement as u16
        );
        assert!(is_discovery_advertisement(&adv));
    }

    #[test]
    fn test_wrap_advertisement_ttl() {
        let adv = make_adv(
            0x01,
            DiscoveryScope::Global,
            1000,
            DiscoveryGossipMode::Flood,
        );
        assert_eq!(adv.ttl_hops, 20); // Global = 20

        let adv_local = make_adv(
            0x01,
            DiscoveryScope::Local,
            1000,
            DiscoveryGossipMode::Flood,
        );
        assert_eq!(adv_local.ttl_hops, 3); // Local = 3
    }

    #[test]
    fn test_wrap_advertisement_flag() {
        let flood = make_adv(
            0x01,
            DiscoveryScope::Global,
            1000,
            DiscoveryGossipMode::Flood,
        );
        assert!(flood.has_flag(FLAG_FLOOD));

        let incr = make_adv(
            0x01,
            DiscoveryScope::Global,
            1000,
            DiscoveryGossipMode::Incremental,
        );
        assert!(incr.has_flag(FLAG_INCREMENTAL));

        let ae = make_adv(
            0x01,
            DiscoveryScope::Global,
            1000,
            DiscoveryGossipMode::AntiEntropy,
        );
        assert!(ae.has_flag(FLAG_ANTI_ENTROPY));

        let dir = make_adv(
            0x01,
            DiscoveryScope::Global,
            1000,
            DiscoveryGossipMode::Directed,
        );
        assert!(dir.has_flag(FLAG_DIRECTED));
    }

    #[test]
    fn test_select_for_mode_flood() {
        let objects = vec![
            make_adv(
                0x01,
                DiscoveryScope::Global,
                1000,
                DiscoveryGossipMode::Flood,
            ),
            make_adv(
                0x02,
                DiscoveryScope::Global,
                1001,
                DiscoveryGossipMode::Incremental,
            ),
            make_adv(
                0x03,
                DiscoveryScope::Global,
                1002,
                DiscoveryGossipMode::Flood,
            ),
        ];
        let selected = select_for_mode(&objects, DiscoveryGossipMode::Flood);
        assert_eq!(selected.len(), 2);
        assert!(selected.iter().all(|o| o.has_flag(FLAG_FLOOD)));
    }

    #[test]
    fn test_select_for_mode_incremental() {
        let objects = vec![
            make_adv(
                0x01,
                DiscoveryScope::Global,
                1000,
                DiscoveryGossipMode::Flood,
            ),
            make_adv(
                0x02,
                DiscoveryScope::Global,
                1001,
                DiscoveryGossipMode::Incremental,
            ),
            make_adv(
                0x03,
                DiscoveryScope::Global,
                1002,
                DiscoveryGossipMode::Incremental,
            ),
        ];
        let selected = select_for_mode(&objects, DiscoveryGossipMode::Incremental);
        assert_eq!(selected.len(), 2);
        assert!(selected.iter().all(|o| o.has_flag(FLAG_INCREMENTAL)));
    }

    #[test]
    fn test_select_excludes_expired_ttl() {
        let mut adv = make_adv(
            0x01,
            DiscoveryScope::Global,
            1000,
            DiscoveryGossipMode::Flood,
        );
        adv.ttl_hops = 0; // Expired
        let objects = vec![adv];
        let selected = select_for_mode(&objects, DiscoveryGossipMode::Flood);
        assert!(selected.is_empty());
    }

    #[test]
    fn test_deduplicate_by_gateway() {
        let objects = vec![
            make_adv(
                0x01,
                DiscoveryScope::Global,
                1000,
                DiscoveryGossipMode::Flood,
            ),
            make_adv(
                0x01,
                DiscoveryScope::Global,
                2000,
                DiscoveryGossipMode::Flood,
            ), // Same gateway, newer
            make_adv(
                0x02,
                DiscoveryScope::Global,
                1500,
                DiscoveryGossipMode::Flood,
            ),
        ];
        let deduped = deduplicate_by_gateway(&objects);
        assert_eq!(deduped.len(), 2);

        // Gateway 0x01 should have timestamp 2000
        let gw1 = deduped
            .iter()
            .find(|o| o.origin_gateway == make_gateway_id(0x01))
            .unwrap();
        assert_eq!(gw1.logical_timestamp, 2000);
    }

    #[test]
    fn test_deduplicate_ignores_non_discovery() {
        let mut non_adv = make_adv(
            0x01,
            DiscoveryScope::Global,
            1000,
            DiscoveryGossipMode::Flood,
        );
        non_adv.object_type = GossipObjectType::Envelope as u16; // Not a discovery ad
        let objects = vec![non_adv];
        let deduped = deduplicate_by_gateway(&objects);
        assert!(deduped.is_empty());
    }

    #[test]
    fn test_scope_to_gossip_domain() {
        let domain = scope_to_gossip_domain(DiscoveryScope::Global, 1, [0u8; 32]);
        assert_eq!(domain.scope, GossipScope::GLOBAL);
        assert_eq!(domain.network_id, 1);

        let mission_domain = scope_to_gossip_domain(DiscoveryScope::Mission, 1, [0x42; 32]);
        assert_eq!(mission_domain.scope, GossipScope::MISSION);
        assert_eq!(mission_domain.mission_id, [0x42; 32]);
    }
}
