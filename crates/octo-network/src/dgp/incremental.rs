//! Incremental gossip mode (RFC-0852 §6)

use super::object::GossipObject;

/// Incremental mode: only propagate objects the peer hasn't seen.
///
/// Normal operation mode — efficient dissemination of new state.
pub struct IncrementalMode;

impl IncrementalMode {
    /// Filter objects eligible for incremental propagation.
    pub fn eligible(objects: &[GossipObject]) -> Vec<&GossipObject> {
        objects
            .iter()
            .filter(|o| o.has_flag(super::object::FLAG_INCREMENTAL) && o.ttl_hops > 0)
            .collect()
    }

    /// Select objects not yet seen by the peer.
    pub fn unseen_objects<'a>(
        objects: &'a [GossipObject],
        peer_seen: &[[u8; 32]],
    ) -> Vec<&'a GossipObject> {
        objects
            .iter()
            .filter(|o| !peer_seen.contains(&o.object_hash) && o.ttl_hops > 0)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dgp::domain::{GossipDomainId, GossipScope};
    use crate::dgp::object::{GossipObjectType, FLAG_FLOOD, FLAG_INCREMENTAL};

    fn make_incremental_obj(hash_byte: u8) -> GossipObject {
        GossipObject {
            object_type: GossipObjectType::Envelope as u16,
            object_hash: [hash_byte; 32],
            object_size: 100,
            domain_id: GossipDomainId::new(1, [0u8; 32], GossipScope::GLOBAL),
            logical_timestamp: 1000,
            origin_gateway: [1u8; 32],
            ttl_hops: 20,
            propagation_flags: FLAG_INCREMENTAL,
            payload_root: [0u8; 32],
            signature: [0u8; 64],
        }
    }

    #[test]
    fn test_eligible_filter() {
        let mut flood_obj = make_incremental_obj(0xAA);
        flood_obj.propagation_flags = FLAG_FLOOD;
        let objects = vec![make_incremental_obj(0xBB), flood_obj];
        let eligible = IncrementalMode::eligible(&objects);
        assert_eq!(eligible.len(), 1);
    }

    #[test]
    fn test_unseen_objects() {
        let objects = vec![
            make_incremental_obj(0xAA),
            make_incremental_obj(0xBB),
            make_incremental_obj(0xCC),
        ];
        let seen = vec![[0xAA; 32], [0xCC; 32]];
        let unseen = IncrementalMode::unseen_objects(&objects, &seen);
        assert_eq!(unseen.len(), 1);
        assert_eq!(unseen[0].object_hash[0], 0xBB);
    }

    #[test]
    fn test_unseen_excludes_expired() {
        let mut obj = make_incremental_obj(0xAA);
        obj.ttl_hops = 0;
        let objects = vec![obj];
        let unseen = IncrementalMode::unseen_objects(&objects, &[]);
        assert!(unseen.is_empty());
    }
}
