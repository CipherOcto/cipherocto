//! Canonical processing order (RFC-0852 §4)

use super::object::GossipObject;

/// Canonical processing order key: (domain_id, logical_timestamp, object_hash).
///
/// DGP defines: objects MUST be processed in canonical order,
/// NOT arrival order, transport order, or platform sequence.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CanonicalOrderKey {
    /// Domain ID bytes (38 bytes for deterministic comparison)
    pub domain_bytes: Vec<u8>,
    /// Logical timestamp
    pub logical_timestamp: u64,
    /// Object hash
    pub object_hash: [u8; 32],
}

impl CanonicalOrderKey {
    /// Create from a gossip object.
    pub fn from_object(obj: &GossipObject) -> Self {
        Self {
            domain_bytes: obj.domain_id.to_canonical_bytes().to_vec(),
            logical_timestamp: obj.logical_timestamp,
            object_hash: obj.object_hash,
        }
    }
}

/// Sort gossip objects in canonical processing order.
pub fn sort_canonical(objects: &mut [GossipObject]) {
    objects.sort_by(|a, b| {
        let ka = CanonicalOrderKey::from_object(a);
        let kb = CanonicalOrderKey::from_object(b);
        ka.cmp(&kb)
    });
}

/// FIRST_VALID_HASH_WINS conflict resolution (RFC-0852 §5).
///
/// Given multiple objects with the same logical identity but different hashes,
/// selects the lexicographically lowest object_hash among those whose signature
/// is valid (verified by the caller-supplied predicate). If no candidate has a
/// valid signature, returns `None`.
///
/// Tie-breaking: lowest lexicographic origin_gateway.
///
/// TODO: verify signature natively once RFC-0852 §5 signature scheme is
/// implemented. Until then callers pass `verify` as a closure.
pub fn first_valid_hash_wins<F>(
    objects: &[GossipObject],
    verify: F,
) -> Option<&GossipObject>
where
    F: Fn(&GossipObject) -> bool,
{
    objects.iter().filter(|o| verify(o)).min_by(|a, b| {
        a.object_hash
            .cmp(&b.object_hash)
            .then_with(|| a.origin_gateway.cmp(&b.origin_gateway))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dgp::domain::{GossipDomainId, GossipScope};
    use crate::dgp::object::GossipObjectType;

    fn make_obj(domain_net: u32, ts: u64, hash_byte: u8, origin_byte: u8) -> GossipObject {
        GossipObject {
            object_type: GossipObjectType::Envelope as u16,
            object_hash: [hash_byte; 32],
            object_size: 100,
            domain_id: GossipDomainId::new(domain_net, [0u8; 32], GossipScope::GLOBAL),
            logical_timestamp: ts,
            origin_gateway: [origin_byte; 32],
            ttl_hops: 20,
            propagation_flags: 0,
            payload_root: [0u8; 32],
            signature: [0u8; 64],
        }
    }

    #[test]
    fn test_canonical_ordering_by_domain() {
        let a = make_obj(1, 100, 0xAA, 0x01);
        let b = make_obj(2, 100, 0xAA, 0x01);
        let ka = CanonicalOrderKey::from_object(&a);
        let kb = CanonicalOrderKey::from_object(&b);
        assert!(ka < kb);
    }

    #[test]
    fn test_canonical_ordering_by_timestamp() {
        let a = make_obj(1, 100, 0xAA, 0x01);
        let b = make_obj(1, 200, 0xAA, 0x01);
        let ka = CanonicalOrderKey::from_object(&a);
        let kb = CanonicalOrderKey::from_object(&b);
        assert!(ka < kb);
    }

    #[test]
    fn test_canonical_ordering_by_hash() {
        let a = make_obj(1, 100, 0x01, 0x01);
        let b = make_obj(1, 100, 0x02, 0x01);
        let ka = CanonicalOrderKey::from_object(&a);
        let kb = CanonicalOrderKey::from_object(&b);
        assert!(ka < kb);
    }

    #[test]
    fn test_sort_canonical() {
        let mut objects = vec![
            make_obj(1, 200, 0xBB, 0x01),
            make_obj(1, 100, 0xAA, 0x01),
            make_obj(2, 100, 0xAA, 0x01),
        ];
        sort_canonical(&mut objects);
        assert_eq!(objects[0].logical_timestamp, 100);
        assert_eq!(objects[0].object_hash[0], 0xAA);
        assert_eq!(objects[1].domain_id.network_id, 1);
        assert_eq!(objects[1].logical_timestamp, 200);
    }

    #[test]
    fn test_first_valid_hash_wins() {
        let objects = vec![
            make_obj(1, 100, 0xBB, 0x02),
            make_obj(1, 100, 0xAA, 0x01),
            make_obj(1, 100, 0xCC, 0x03),
        ];
        let winner = first_valid_hash_wins(&objects, |_| true).unwrap();
        assert_eq!(winner.object_hash[0], 0xAA);
    }

    #[test]
    fn test_first_valid_hash_wins_tiebreak_by_origin() {
        let objects = vec![make_obj(1, 100, 0xAA, 0x02), make_obj(1, 100, 0xAA, 0x01)];
        let winner = first_valid_hash_wins(&objects, |_| true).unwrap();
        assert_eq!(winner.origin_gateway[0], 0x01);
    }
}
