//! Directed gossip mode (RFC-0852 §6)

use super::object::GossipObject;

/// Directed mode: targeted propagation to specific peers.
///
/// Use cases: mission overlays, validator coordination.
/// Objects with FLAG_DIRECTED are propagated only to peers matching
/// the mission scope.
pub struct DirectedMode;

impl DirectedMode {
    /// Filter objects eligible for directed propagation.
    pub fn eligible(objects: &[GossipObject]) -> Vec<&GossipObject> {
        objects
            .iter()
            .filter(|o| o.has_flag(super::object::FLAG_DIRECTED) && o.ttl_hops > 0)
            .collect()
    }

    /// Check if a peer is a valid target for directed gossip.
    pub fn is_valid_target(object: &GossipObject, peer_missions: &[[u8; 32]]) -> bool {
        // Directed gossip targets peers in the same mission scope
        peer_missions.contains(&object.domain_id.mission_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dgp::domain::{GossipDomainId, GossipScope};
    use crate::dgp::object::{GossipObjectType, FLAG_DIRECTED, FLAG_FLOOD};

    fn make_directed_obj(hash_byte: u8, mission: [u8; 32]) -> GossipObject {
        GossipObject {
            object_type: GossipObjectType::MissionState as u16,
            object_hash: [hash_byte; 32],
            object_size: 100,
            domain_id: GossipDomainId::new(1, mission, GossipScope::MISSION),
            logical_timestamp: 1000,
            origin_gateway: [1u8; 32],
            ttl_hops: 5,
            propagation_flags: FLAG_DIRECTED,
            payload_root: [0u8; 32],
            signature: [0u8; 64],
        }
    }

    #[test]
    fn test_eligible_filter() {
        let mut flood_obj = make_directed_obj(0xAA, [0x01; 32]);
        flood_obj.propagation_flags = FLAG_FLOOD;
        let objects = vec![make_directed_obj(0xBB, [0x01; 32]), flood_obj];
        let eligible = DirectedMode::eligible(&objects);
        assert_eq!(eligible.len(), 1);
    }

    #[test]
    fn test_is_valid_target_same_mission() {
        let obj = make_directed_obj(0xAA, [0x01; 32]);
        assert!(DirectedMode::is_valid_target(&obj, &[[0x01; 32]]));
    }

    #[test]
    fn test_is_valid_target_different_mission() {
        let obj = make_directed_obj(0xAA, [0x01; 32]);
        assert!(!DirectedMode::is_valid_target(&obj, &[[0x02; 32]]));
    }
}
