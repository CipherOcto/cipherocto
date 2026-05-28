//! DGP Integration — RFC-0857 §6
//!
//! DOM intents wrap into DGP GossipObject as:
//! - object_type: MempoolIntent (0x0009)
//! - payload: DCS-serialized OverlayIntent
//! - domain_id: derived from mission_id + mempool scope
//! - object_hash: BLAKE3-256(dcs_serialize(GossipObject))

/// DGP object type for mempool intents.
pub const MEMPOOL_INTENT_OBJECT_TYPE: u16 = 0x0009;

/// Rate limit: max intents per sender per logical_timestamp window.
pub const MAX_INTENTS_PER_SENDER_PER_WINDOW: u32 = 100;

/// Compute the domain_id for a mempool intent.
/// domain_id = BLAKE3-256(mission_id || scope_bytes)
pub fn compute_domain_id(mission_id: &[u8; 32], scope: u16) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(mission_id);
    hasher.update(&scope.to_be_bytes());
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mempool_intent_type() {
        assert_eq!(MEMPOOL_INTENT_OBJECT_TYPE, 0x0009);
    }

    #[test]
    fn test_domain_id_deterministic() {
        let mission = [0xAA; 32];
        let id1 = compute_domain_id(&mission, 0x0003);
        let id2 = compute_domain_id(&mission, 0x0003);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_domain_id_different_scopes() {
        let mission = [0xAA; 32];
        let id1 = compute_domain_id(&mission, 0x0001);
        let id2 = compute_domain_id(&mission, 0x0003);
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_rate_limit_constant() {
        assert_eq!(MAX_INTENTS_PER_SENDER_PER_WINDOW, 100);
    }
}
