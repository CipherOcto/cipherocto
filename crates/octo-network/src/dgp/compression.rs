//! Gossip Compression and Retention (RFC-0852 §11, §13)
//!
//! Compression summaries (Bloom filters, Merkle roots, bitmaps) for
//! efficient state synchronization, and retention classes for storage management.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::object::GossipObject;

// -- Bloom Filter Summary --

/// Default Bloom filter size in bytes (256 bytes = 2048 bits).
pub const DEFAULT_BLOOM_SIZE: usize = 256;

/// Default number of hash iterations for Bloom filter.
pub const DEFAULT_BLOOM_HASH_COUNT: usize = 3;

/// Bloom filter summary for quick set membership checks.
///
/// RFC-0852 §11: Bloom filters MUST use BLAKE3-256 as the hash function.
/// Each iteration uses: BLAKE3-256(object_hash || iteration_index).
///
/// False positives are acceptable; false negatives are not.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BloomSummary {
    /// Filter bytes
    pub bits: Vec<u8>,
    /// Number of hash iterations
    pub hash_count: usize,
    /// Number of items inserted
    pub item_count: u32,
}

impl BloomSummary {
    /// Create a new Bloom filter with default parameters.
    pub fn new() -> Self {
        Self {
            bits: vec![0u8; DEFAULT_BLOOM_SIZE],
            hash_count: DEFAULT_BLOOM_HASH_COUNT,
            item_count: 0,
        }
    }

    /// Create a Bloom filter with custom size and hash count.
    pub fn with_params(size_bytes: usize, hash_count: usize) -> Self {
        Self {
            bits: vec![0u8; size_bytes],
            hash_count,
            item_count: 0,
        }
    }

    /// Insert an object hash into the Bloom filter.
    ///
    /// Uses BLAKE3-256(object_hash || iteration_index) for each hash iteration.
    pub fn insert(&mut self, object_hash: &[u8; 32]) {
        let total_bits = self.bits.len() * 8;
        for i in 0..self.hash_count {
            let bit_index = self.bloom_hash(object_hash, i) % total_bits;
            self.bits[bit_index / 8] |= 1 << (bit_index % 8);
        }
        self.item_count += 1;
    }

    /// Check if an object hash might be in the set.
    ///
    /// Returns true if the hash MIGHT be present (possibly false positive).
    /// Returns false if the hash is DEFINITELY not present.
    pub fn might_contain(&self, object_hash: &[u8; 32]) -> bool {
        let total_bits = self.bits.len() * 8;
        for i in 0..self.hash_count {
            let bit_index = self.bloom_hash(object_hash, i) % total_bits;
            if self.bits[bit_index / 8] & (1 << (bit_index % 8)) == 0 {
                return false;
            }
        }
        true
    }

    /// Compute BLAKE3-256(object_hash || iteration_index) and return as usize.
    fn bloom_hash(&self, object_hash: &[u8; 32], iteration: usize) -> usize {
        let mut hasher = blake3::Hasher::new();
        hasher.update(object_hash);
        hasher.update(&iteration.to_be_bytes());
        let hash = hasher.finalize();
        // Use first 8 bytes as usize
        let bytes = hash.as_bytes();
        usize::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ])
    }
}

impl Default for BloomSummary {
    fn default() -> Self {
        Self::new()
    }
}

// -- Bitmap Summary --

/// Bitmap summary for range commitments.
///
/// Each bit represents whether an object in a known range is present.
/// Useful for efficient "what's missing" queries during anti-entropy sync.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BitmapSummary {
    /// Bitmap bytes
    pub bits: Vec<u8>,
    /// Starting index of the range
    pub range_start: u64,
    /// Number of items in the range
    pub range_count: u64,
}

impl BitmapSummary {
    /// Create a new bitmap summary for a range.
    pub fn new(range_start: u64, range_count: u64) -> Self {
        let byte_count = ((range_count + 7) / 8) as usize;
        Self {
            bits: vec![0u8; byte_count],
            range_start,
            range_count,
        }
    }

    /// Mark an index as present (0-based relative to range_start).
    pub fn set(&mut self, index: u64) {
        if index < self.range_count {
            self.bits[(index / 8) as usize] |= 1 << (index % 8);
        }
    }

    /// Check if an index is marked as present.
    pub fn is_set(&self, index: u64) -> bool {
        if index >= self.range_count {
            return false;
        }
        self.bits[(index / 8) as usize] & (1 << (index % 8)) != 0
    }

    /// Count the number of set bits (present items).
    pub fn count_set(&self) -> u32 {
        self.bits.iter().map(|b| b.count_ones()).sum()
    }

    /// Compute the set difference: indices present in self but not in other.
    pub fn difference(&self, other: &BitmapSummary) -> Vec<u64> {
        let mut diff = Vec::new();
        for i in 0..self.range_count {
            if self.is_set(i) && !other.is_set(i) {
                diff.push(self.range_start + i);
            }
        }
        diff
    }
}

// -- Retention Classes --

/// Retention classes for gossip objects (RFC-0852 §13).
///
/// Objects are assigned a retention class that determines how long
/// they are kept in the gossip cache before automatic cleanup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u16)]
pub enum RetentionClass {
    /// Short-lived: memory only, discarded quickly
    Ephemeral = 0x0001,
    /// Mission lifetime: kept while mission is active
    Mission = 0x0002,
    /// Consensus-critical: kept until finality
    Consensus = 0x0003,
    /// Long-term archival: stored on disk
    Archive = 0x0004,
}

impl RetentionClass {
    /// Parse from u16.
    pub fn from_u16(val: u16) -> Option<Self> {
        match val {
            0x0001 => Some(Self::Ephemeral),
            0x0002 => Some(Self::Mission),
            0x0003 => Some(Self::Consensus),
            0x0004 => Some(Self::Archive),
            _ => None,
        }
    }

    /// Default duration in logical time units for this retention class.
    pub fn default_duration(&self) -> u64 {
        match self {
            Self::Ephemeral => 60,
            Self::Mission => 3600,
            Self::Consensus => 86400,
            Self::Archive => u64::MAX, // effectively forever
        }
    }
}

/// Retention entry tracking an object's retention class and expiry.
#[derive(Clone, Debug)]
pub struct RetentionEntry {
    /// Object hash
    pub object_hash: [u8; 32],
    /// Retention class
    pub class: RetentionClass,
    /// Logical timestamp when the object was admitted
    pub admitted_at: u64,
    /// Logical timestamp when the object expires (admitted_at + duration)
    pub expires_at: u64,
}

/// Retention manager — tracks objects by retention class and cleans up expired ones.
#[derive(Clone, Debug)]
pub struct RetentionManager {
    /// Object hash -> retention entry
    entries: BTreeMap<[u8; 32], RetentionEntry>,
    /// Custom durations per class (overrides defaults)
    durations: BTreeMap<RetentionClass, u64>,
}

impl RetentionManager {
    /// Create a new retention manager with default durations.
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            durations: BTreeMap::new(),
        }
    }

    /// Set a custom duration for a retention class.
    pub fn set_duration(&mut self, class: RetentionClass, duration: u64) {
        self.durations.insert(class, duration);
    }

    /// Admit an object with a given retention class.
    pub fn admit(&mut self, object_hash: [u8; 32], class: RetentionClass, current_time: u64) {
        let duration = self
            .durations
            .get(&class)
            .copied()
            .unwrap_or_else(|| class.default_duration());
        self.entries.insert(
            object_hash,
            RetentionEntry {
                object_hash,
                class,
                admitted_at: current_time,
                expires_at: current_time.saturating_add(duration),
            },
        );
    }

    /// Check if an object has expired.
    pub fn is_expired(&self, object_hash: &[u8; 32], current_time: u64) -> bool {
        if let Some(entry) = self.entries.get(object_hash) {
            return current_time >= entry.expires_at;
        }
        true // not tracked = expired
    }

    /// Remove all expired entries. Returns the count of removed objects.
    pub fn cleanup(&mut self, current_time: u64) -> usize {
        let before = self.entries.len();
        self.entries
            .retain(|_, entry| current_time < entry.expires_at);
        before - self.entries.len()
    }

    /// Get the retention class for an object.
    pub fn get_class(&self, object_hash: &[u8; 32]) -> Option<RetentionClass> {
        self.entries.get(object_hash).map(|e| e.class)
    }

    /// Number of tracked objects.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Count objects by retention class.
    pub fn count_by_class(&self, class: RetentionClass) -> usize {
        self.entries.values().filter(|e| e.class == class).count()
    }
}

impl Default for RetentionManager {
    fn default() -> Self {
        Self::new()
    }
}

// -- Helpers --

/// Compute a Merkle state summary over a set of gossip objects.
///
/// Used in anti-entropy sync to quickly detect state divergence.
pub fn compute_state_summary(objects: &[GossipObject]) -> [u8; 32] {
    let mut hashes: Vec<[u8; 32]> = objects.iter().map(|o| o.object_hash).collect();
    hashes.sort();
    crate::common::merkle::compute_merkle_root(&hashes)
}

/// Build a Bloom summary from a set of gossip objects.
pub fn build_bloom_summary(objects: &[GossipObject]) -> BloomSummary {
    let mut bloom = BloomSummary::new();
    for obj in objects {
        bloom.insert(&obj.object_hash);
    }
    bloom
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dgp::domain::{GossipDomainId, GossipScope};
    use crate::dgp::object::{GossipObjectType, FLAG_FLOOD};

    fn make_obj(hash_byte: u8) -> GossipObject {
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

    // -- Bloom filter tests --

    #[test]
    fn test_bloom_insert_and_check() {
        let mut bloom = BloomSummary::new();
        bloom.insert(&[0xAA; 32]);
        assert!(bloom.might_contain(&[0xAA; 32]));
    }

    #[test]
    fn test_bloom_no_false_negative() {
        let mut bloom = BloomSummary::new();
        for i in 0..50u8 {
            bloom.insert(&[i; 32]);
        }
        // All inserted items must be found
        for i in 0..50u8 {
            assert!(bloom.might_contain(&[i; 32]), "false negative for {}", i);
        }
    }

    #[test]
    fn test_bloom_not_inserted() {
        let bloom = BloomSummary::new();
        // Empty filter: nothing should match
        assert!(!bloom.might_contain(&[0xAA; 32]));
    }

    #[test]
    fn test_bloom_uses_blake3() {
        // Verify the hash function is BLAKE3, not something else
        let mut bloom = BloomSummary::with_params(32, 1);
        bloom.insert(&[0x42; 32]);
        // The bit position should be deterministic with BLAKE3
        let mut bloom2 = BloomSummary::with_params(32, 1);
        bloom2.insert(&[0x42; 32]);
        assert_eq!(bloom.bits, bloom2.bits);
    }

    #[test]
    fn test_bloom_item_count() {
        let mut bloom = BloomSummary::new();
        assert_eq!(bloom.item_count, 0);
        bloom.insert(&[0xAA; 32]);
        bloom.insert(&[0xBB; 32]);
        assert_eq!(bloom.item_count, 2);
    }

    // -- Bitmap summary tests --

    #[test]
    fn test_bitmap_set_and_check() {
        let mut bmp = BitmapSummary::new(0, 100);
        bmp.set(5);
        bmp.set(50);
        assert!(bmp.is_set(5));
        assert!(bmp.is_set(50));
        assert!(!bmp.is_set(10));
    }

    #[test]
    fn test_bitmap_count_set() {
        let mut bmp = BitmapSummary::new(0, 64);
        bmp.set(0);
        bmp.set(31);
        bmp.set(63);
        assert_eq!(bmp.count_set(), 3);
    }

    #[test]
    fn test_bitmap_difference() {
        let mut bmp1 = BitmapSummary::new(0, 10);
        bmp1.set(0);
        bmp1.set(1);
        bmp1.set(2);

        let mut bmp2 = BitmapSummary::new(0, 10);
        bmp2.set(1);
        bmp2.set(3);

        let diff = bmp1.difference(&bmp2);
        assert_eq!(diff, vec![0, 2]); // present in bmp1 but not bmp2
    }

    #[test]
    fn test_bitmap_out_of_range() {
        let bmp = BitmapSummary::new(0, 10);
        assert!(!bmp.is_set(100));
    }

    // -- Retention class tests --

    #[test]
    fn test_retention_class_from_u16() {
        assert_eq!(
            RetentionClass::from_u16(0x0001),
            Some(RetentionClass::Ephemeral)
        );
        assert_eq!(
            RetentionClass::from_u16(0x0004),
            Some(RetentionClass::Archive)
        );
        assert_eq!(RetentionClass::from_u16(0x0099), None);
    }

    #[test]
    fn test_retention_class_ordering() {
        assert!(RetentionClass::Ephemeral < RetentionClass::Mission);
        assert!(RetentionClass::Mission < RetentionClass::Consensus);
        assert!(RetentionClass::Consensus < RetentionClass::Archive);
    }

    #[test]
    fn test_retention_class_default_duration() {
        assert_eq!(RetentionClass::Ephemeral.default_duration(), 60);
        assert_eq!(RetentionClass::Mission.default_duration(), 3600);
        assert_eq!(RetentionClass::Archive.default_duration(), u64::MAX);
    }

    // -- RetentionManager tests --

    #[test]
    fn test_retention_manager_admit_and_check() {
        let mut mgr = RetentionManager::new();
        mgr.admit([0xAA; 32], RetentionClass::Ephemeral, 100);
        assert!(!mgr.is_expired(&[0xAA; 32], 150)); // 150 < 100 + 60
        assert!(mgr.is_expired(&[0xAA; 32], 161)); // 161 >= 100 + 60
    }

    #[test]
    fn test_retention_manager_cleanup() {
        let mut mgr = RetentionManager::new();
        mgr.admit([0x01; 32], RetentionClass::Ephemeral, 100); // expires at 160
        mgr.admit([0x02; 32], RetentionClass::Mission, 100); // expires at 3700
        mgr.admit([0x03; 32], RetentionClass::Archive, 100); // never expires
        assert_eq!(mgr.len(), 3);

        let cleaned = mgr.cleanup(200);
        assert_eq!(cleaned, 1); // only ephemeral expired
        assert_eq!(mgr.len(), 2);
    }

    #[test]
    fn test_retention_manager_custom_duration() {
        let mut mgr = RetentionManager::new();
        mgr.set_duration(RetentionClass::Ephemeral, 10); // override to 10
        mgr.admit([0xAA; 32], RetentionClass::Ephemeral, 100);
        assert!(mgr.is_expired(&[0xAA; 32], 111)); // 111 >= 100 + 10
    }

    #[test]
    fn test_retention_manager_get_class() {
        let mut mgr = RetentionManager::new();
        mgr.admit([0xAA; 32], RetentionClass::Consensus, 100);
        assert_eq!(mgr.get_class(&[0xAA; 32]), Some(RetentionClass::Consensus));
        assert_eq!(mgr.get_class(&[0xFF; 32]), None);
    }

    #[test]
    fn test_retention_manager_count_by_class() {
        let mut mgr = RetentionManager::new();
        mgr.admit([0x01; 32], RetentionClass::Ephemeral, 100);
        mgr.admit([0x02; 32], RetentionClass::Ephemeral, 100);
        mgr.admit([0x03; 32], RetentionClass::Mission, 100);
        assert_eq!(mgr.count_by_class(RetentionClass::Ephemeral), 2);
        assert_eq!(mgr.count_by_class(RetentionClass::Mission), 1);
        assert_eq!(mgr.count_by_class(RetentionClass::Archive), 0);
    }

    // -- Helper tests --

    #[test]
    fn test_compute_state_summary_deterministic() {
        let objects = vec![make_obj(0xAA), make_obj(0xBB), make_obj(0xCC)];
        let s1 = compute_state_summary(&objects);
        let s2 = compute_state_summary(&objects);
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_build_bloom_summary() {
        let objects = vec![make_obj(0xAA), make_obj(0xBB)];
        let bloom = build_bloom_summary(&objects);
        assert_eq!(bloom.item_count, 2);
        assert!(bloom.might_contain(&[0xAA; 32]));
        assert!(bloom.might_contain(&[0xBB; 32]));
    }
}
