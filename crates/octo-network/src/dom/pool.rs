//! Mission-Scoped Mempool — RFC-0857 §2, §13
//!
//! Each mission gets an isolated mempool with capacity limits.
//! Default capacities: GLOBAL=100k, CONSENSUS=50k, MISSION=10k,
//! REGIONAL=5k, PRIVATE=1k, LOCAL=100.

use crate::dom::error::DomError;
use crate::dom::intent::OverlayIntent;
use crate::dom::ordering::canonical_sort;
use std::collections::BTreeMap;

/// Default capacity per scope (RFC-0857 §13).
pub fn default_capacity_for_scope(scope: u16) -> u32 {
    match scope {
        0x0001 => 100_000, // GLOBAL
        0x0002 => 5_000,   // REGIONAL
        0x0003 => 10_000,  // MISSION
        0x0004 => 1_000,   // PRIVATE
        0x0005 => 100,     // LOCAL
        0x0006 => 50_000,  // CONSENSUS
        _ => 10_000,
    }
}

/// Mission-scoped mempool.
pub struct MempoolPool {
    /// mission_id -> intents
    pools: BTreeMap<[u8; 32], Vec<OverlayIntent>>,
    /// mission_id -> scope
    scopes: BTreeMap<[u8; 32], u16>,
    /// global capacity
    max_global: u32,
    /// per-mission capacity
    max_per_mission: u32,
}

impl MempoolPool {
    pub fn new(max_global: u32, max_per_mission: u32) -> Self {
        Self {
            pools: BTreeMap::new(),
            scopes: BTreeMap::new(),
            max_global,
            max_per_mission,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(100_000, 10_000)
    }

    /// Register a mission with a scope.
    pub fn register_mission(&mut self, mission_id: [u8; 32], scope: u16) {
        self.pools.entry(mission_id).or_default();
        self.scopes.insert(mission_id, scope);
    }

    /// Insert an intent into the appropriate mission pool.
    pub fn insert(&mut self, intent: OverlayIntent) -> Result<(), DomError> {
        // Enforce global capacity
        if self.total_count() as u32 >= self.max_global {
            return Err(DomError::CapacityExceeded {
                scope: 0x0001, // GLOBAL
                max_entries: self.max_global,
            });
        }

        let mission_id = intent.mission_id;
        let scope = self.scopes.get(&mission_id).copied().unwrap_or(0x0003);
        let pool = self.pools.entry(mission_id).or_default();

        if pool.len() as u32 >= self.max_per_mission {
            return Err(DomError::CapacityExceeded {
                scope,
                max_entries: self.max_per_mission,
            });
        }

        pool.push(intent);
        Ok(())
    }

    /// Get total intent count across all missions.
    pub fn total_count(&self) -> usize {
        self.pools.values().map(|v| v.len()).sum()
    }

    /// Get intent count for a specific mission.
    pub fn mission_count(&self, mission_id: &[u8; 32]) -> usize {
        self.pools.get(mission_id).map_or(0, |v| v.len())
    }

    /// Get all intents in canonical order for a specific mission.
    pub fn get_ordered(&self, mission_id: &[u8; 32]) -> Vec<OverlayIntent> {
        if let Some(pool) = self.pools.get(mission_id) {
            let mut intents = pool.clone();
            canonical_sort(&mut intents);
            intents
        } else {
            vec![]
        }
    }

    /// Evict expired intents (deterministic — all nodes remove same intents).
    pub fn evict_expired(&mut self, current_timestamp: u64) {
        for pool in self.pools.values_mut() {
            pool.retain(|i| i.expiration > current_timestamp);
        }
        // Remove empty pools and their scope entries
        self.pools.retain(|_, pool| !pool.is_empty());
        self.scopes
            .retain(|mission_id, _| self.pools.contains_key(mission_id));
    }

    /// Get mission IDs.
    pub fn mission_ids(&self) -> Vec<[u8; 32]> {
        self.pools.keys().copied().collect()
    }
}

/// Mempool state root — BLAKE3-256 Merkle root of all pending intents.
///
/// Provides a deterministic fingerprint of the mempool state for consensus.
/// Intents are sorted by canonical intent_id before hashing.
pub struct MempoolStateRoot;

impl MempoolStateRoot {
    /// Compute the Merkle root of all pending intents.
    ///
    /// Intents are first sorted by intent_id (ascending) for determinism,
    /// then hashed as a BLAKE3 Merkle tree over individual intent hashes.
    pub fn compute(intents: &[OverlayIntent]) -> [u8; 32] {
        if intents.is_empty() {
            return [0u8; 32];
        }

        // Sort by intent_id for deterministic ordering
        let mut sorted: Vec<&OverlayIntent> = intents.iter().collect();
        sorted.sort_by(|a, b| a.intent_id.cmp(&b.intent_id));

        // Compute leaf hashes
        let leaves: Vec<[u8; 32]> = sorted
            .iter()
            .map(|intent| {
                let mut hasher = blake3::Hasher::new();
                hasher.update(&intent.intent_id);
                hasher.update(&intent.sequence.to_be_bytes());
                hasher.update(&intent.mission_id);
                *hasher.finalize().as_bytes()
            })
            .collect();

        merkle_root(&leaves)
    }
}

/// Compute a BLAKE3-256 Merkle root from a slice of 32-byte leaves.
fn merkle_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        return [0u8; 32];
    }
    if leaves.len() == 1 {
        return leaves[0];
    }

    let mut level = leaves.to_vec();
    while level.len() > 1 {
        if !level.len().is_multiple_of(2) {
            let last = *level.last().expect("level is non-empty in merkle loop");
            level.push(last);
        }
        let mut next = Vec::with_capacity(level.len() / 2);
        for pair in level.chunks(2) {
            let mut hasher = blake3::Hasher::new();
            hasher.update(&pair[0]);
            hasher.update(&pair[1]);
            next.push(*hasher.finalize().as_bytes());
        }
        level = next;
    }
    level[0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::intent::ExecutionClass;

    fn make_intent(mission: u8, seq: u64, exp: u64) -> OverlayIntent {
        OverlayIntent {
            intent_id: {
                let mut id = [mission; 32];
                id[0] ^= seq as u8;
                id
            },
            intent_type: 0x0001,
            mission_id: [mission; 32],
            sender_id: [0xCC; 32],
            sequence: seq,
            logical_timestamp: 100,
            expiration: exp,
            payload_root: [0u8; 32],
            economic_weight: 100,
            execution_class: ExecutionClass::Economic as u16,
            signature: [0u8; 64],
        }
    }

    #[test]
    fn test_pool_insert_and_count() {
        let mut pool = MempoolPool::new(1000, 100);
        pool.register_mission([0x01; 32], 0x0003);
        pool.insert(make_intent(0x01, 1, 200)).unwrap();
        pool.insert(make_intent(0x01, 2, 200)).unwrap();
        assert_eq!(pool.mission_count(&[0x01; 32]), 2);
        assert_eq!(pool.total_count(), 2);
    }

    #[test]
    fn test_pool_capacity_exceeded() {
        let mut pool = MempoolPool::new(1000, 2);
        pool.register_mission([0x01; 32], 0x0003);
        pool.insert(make_intent(0x01, 1, 200)).unwrap();
        pool.insert(make_intent(0x01, 2, 200)).unwrap();
        let result = pool.insert(make_intent(0x01, 3, 200));
        assert!(matches!(result, Err(DomError::CapacityExceeded { .. })));
    }

    #[test]
    fn test_pool_isolation() {
        let mut pool = MempoolPool::new(1000, 100);
        pool.register_mission([0x01; 32], 0x0003);
        pool.register_mission([0x02; 32], 0x0003);
        pool.insert(make_intent(0x01, 1, 200)).unwrap();
        pool.insert(make_intent(0x02, 1, 200)).unwrap();
        assert_eq!(pool.mission_count(&[0x01; 32]), 1);
        assert_eq!(pool.mission_count(&[0x02; 32]), 1);
    }

    #[test]
    fn test_pool_evict_expired() {
        let mut pool = MempoolPool::new(1000, 100);
        pool.register_mission([0x01; 32], 0x0003);
        pool.insert(make_intent(0x01, 1, 50)).unwrap(); // expires at 50
        pool.insert(make_intent(0x01, 2, 200)).unwrap(); // expires at 200
        pool.evict_expired(100); // current=100
        assert_eq!(pool.mission_count(&[0x01; 32]), 1);
    }

    #[test]
    fn test_pool_get_ordered() {
        let mut pool = MempoolPool::new(1000, 100);
        pool.register_mission([0x01; 32], 0x0003);
        pool.insert(make_intent(0x01, 2, 200)).unwrap();
        pool.insert(make_intent(0x01, 1, 200)).unwrap();
        let ordered = pool.get_ordered(&[0x01; 32]);
        assert_eq!(ordered[0].sequence, 1); // lower sequence first
        assert_eq!(ordered[1].sequence, 2);
    }

    #[test]
    fn test_mempool_state_root_empty() {
        let intents: Vec<OverlayIntent> = vec![];
        let root = MempoolStateRoot::compute(&intents);
        assert_eq!(root, [0u8; 32]);
    }

    #[test]
    fn test_mempool_state_root_deterministic() {
        let intents = vec![
            make_intent(0x01, 2, 200),
            make_intent(0x01, 1, 200),
            make_intent(0x02, 1, 200),
        ];
        let r1 = MempoolStateRoot::compute(&intents);
        let r2 = MempoolStateRoot::compute(&intents);
        assert_eq!(r1, r2);
        assert_ne!(r1, [0u8; 32]);
    }

    #[test]
    fn test_mempool_state_root_order_independent() {
        // Same intents in different order should produce same root
        let intents_a = vec![make_intent(0x01, 1, 200), make_intent(0x01, 2, 200)];
        let intents_b = vec![make_intent(0x01, 2, 200), make_intent(0x01, 1, 200)];
        assert_eq!(
            MempoolStateRoot::compute(&intents_a),
            MempoolStateRoot::compute(&intents_b)
        );
    }

    #[test]
    fn test_mempool_state_root_different_intents() {
        let intents_a = vec![make_intent(0x01, 1, 200)];
        let intents_b = vec![make_intent(0x02, 1, 200)];
        assert_ne!(
            MempoolStateRoot::compute(&intents_a),
            MempoolStateRoot::compute(&intents_b)
        );
    }
}
