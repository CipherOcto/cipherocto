//! Deterministic Gossip Protocol (DGP) — RFC-0852
//!
//! Provides deterministic message propagation, deduplication,
//! replay protection, and multi-mode gossip across heterogeneous
//! transport fabrics.
//!
//! Key invariant: DGP separates transport nondeterminism from
//! consensus determinism. External networks may reorder, duplicate,
//! censor, or delay messages, but DGP ensures logical overlay state
//! converges deterministically.

pub mod dedup;
pub mod directed;
pub mod domain;
pub mod error;
pub mod flood;
pub mod fragment;
pub mod incremental;
pub mod object;
pub mod ordering;

pub use dedup::{DedupSet, GossipReplayCache};
pub use directed::DirectedMode;
pub use domain::{GossipDomainId, GossipScope};
pub use error::DgpError;
pub use flood::FloodMode;
pub use fragment::{FragmentAssembler, GossipFragment};
pub use incremental::IncrementalMode;
pub use object::{
    GossipObject, GossipObjectType, GossipPriority, FLAG_ANTI_ENTROPY, FLAG_COMPRESSED,
    FLAG_DIRECTED, FLAG_FLOOD, FLAG_INCREMENTAL, FLAG_RELIABLE,
};
pub use ordering::{first_valid_hash_wins, sort_canonical, CanonicalOrderKey};

/// DGP protocol version.
pub const DGP_PROTOCOL_VERSION: u16 = 1;

/// Default replay cache size.
pub const DEFAULT_REPLAY_CACHE_SIZE: u32 = 100_000;

/// Default replay window duration (logical time units).
pub const DEFAULT_REPLAY_WINDOW: u64 = 1_000_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dgp_constants() {
        assert_eq!(DGP_PROTOCOL_VERSION, 1);
        assert!(DEFAULT_REPLAY_CACHE_SIZE > 0);
        assert!(DEFAULT_REPLAY_WINDOW > 0);
    }

    #[test]
    fn test_full_pipeline_dedup_ordering() {
        // Simulate: 3 objects arrive out of order, dedup removes one duplicate
        let mut dedup = DedupSet::new();
        let obj_a = make_test_obj(1, 100, 0xAA);
        let obj_b = make_test_obj(1, 200, 0xBB);
        let obj_c = make_test_obj(1, 100, 0xAA); // duplicate

        let mut accepted = Vec::new();
        for obj in [&obj_a, &obj_b, &obj_c] {
            if dedup.insert_if_new(obj.object_hash) {
                accepted.push(obj.clone());
            }
        }
        assert_eq!(accepted.len(), 2);

        // Sort in canonical order
        sort_canonical(&mut accepted);
        assert!(accepted[0].ordering_key() < accepted[1].ordering_key());
    }

    #[test]
    fn test_first_valid_hash_wins_integration() {
        let obj_a = make_test_obj(1, 100, 0xAA);
        let obj_b = make_test_obj(1, 100, 0xBB);
        let candidates = [obj_a, obj_b];
        let winner = first_valid_hash_wins(&candidates).unwrap();
        assert_eq!(winner.object_hash[0], 0xAA);
    }

    fn make_test_obj(net: u32, ts: u64, hash_byte: u8) -> GossipObject {
        GossipObject {
            object_type: GossipObjectType::Envelope as u16,
            object_hash: [hash_byte; 32],
            object_size: 100,
            domain_id: GossipDomainId::new(net, [0u8; 32], GossipScope::GLOBAL),
            logical_timestamp: ts,
            origin_gateway: [1u8; 32],
            ttl_hops: 20,
            propagation_flags: FLAG_FLOOD | FLAG_INCREMENTAL,
            payload_root: [0u8; 32],
            signature: [0u8; 64],
        }
    }
}
