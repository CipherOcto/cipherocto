//! Flood gossip mode (RFC-0852 §6)

use super::object::GossipObject;

/// Flood mode: broadcast aggressively to all peers.
///
/// Use cases: bootstrap, emergency, partition recovery.
/// All objects with FLAG_FLOOD set are propagated to every known peer.
pub struct FloodMode;

impl FloodMode {
    /// Filter objects eligible for flood propagation.
    pub fn eligible(objects: &[GossipObject]) -> Vec<&GossipObject> {
        objects
            .iter()
            .filter(|o| o.has_flag(super::object::FLAG_FLOOD) && o.ttl_hops > 0)
            .collect()
    }

    /// Check if an object should be flooded (not yet seen by peer).
    pub fn should_flood(object: &GossipObject, peer_seen: &[[u8; 32]]) -> bool {
        !peer_seen.contains(&object.object_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dgp::domain::{GossipDomainId, GossipScope};
    use crate::dgp::object::{GossipObjectType, FLAG_FLOOD, FLAG_INCREMENTAL};

    fn make_flood_obj(hash_byte: u8) -> GossipObject {
        GossipObject {
            object_type: GossipObjectType::Envelope as u16,
            object_hash: [hash_byte; 32],
            object_size: 100,
            domain_id: GossipDomainId::new(1, [0u8; 32], GossipScope::GLOBAL),
            logical_timestamp: 1000,
            origin_gateway: [1u8; 32],
            ttl_hops: 20,
            propagation_flags: FLAG_FLOOD,
            payload_root: [0u8; 32],
            signature: [0u8; 64],
        }
    }

    fn make_incremental_obj(hash_byte: u8) -> GossipObject {
        let mut obj = make_flood_obj(hash_byte);
        obj.propagation_flags = FLAG_INCREMENTAL;
        obj
    }

    #[test]
    fn test_eligible_filter() {
        let objects = vec![
            make_flood_obj(0xAA),
            make_incremental_obj(0xBB),
            make_flood_obj(0xCC),
        ];
        let eligible = FloodMode::eligible(&objects);
        assert_eq!(eligible.len(), 2);
    }

    #[test]
    fn test_eligible_excludes_expired() {
        let mut obj = make_flood_obj(0xAA);
        obj.ttl_hops = 0;
        let objects = vec![obj];
        let eligible = FloodMode::eligible(&objects);
        assert!(eligible.is_empty());
    }

    #[test]
    fn test_should_flood() {
        let obj = make_flood_obj(0xAA);
        assert!(FloodMode::should_flood(&obj, &[]));
        assert!(!FloodMode::should_flood(&obj, &[[0xAA; 32]]));
    }
}
