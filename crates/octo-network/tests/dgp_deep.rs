//! Deep coverage tests for DGP — retention manager, compression payloads,
//! anti-entropy edge cases, first_valid_hash_wins, more scope/ordering coverage.

use octo_network::dgp::anti_entropy::{GossipStateSummary, ReconciliationConfig};
use octo_network::dgp::compression::{
    BitmapSummary, BloomSummary, RetentionClass, RetentionManager,
};
use octo_network::dgp::dedup::{DedupSet, GossipReplayCache};
use octo_network::dgp::domain::{GossipDomainId, GossipScope};
use octo_network::dgp::object::{GossipObject, GossipObjectType, FLAG_FLOOD};
use octo_network::dgp::ordering::{first_valid_hash_wins, sort_canonical};

fn make_obj(hash_byte: u8, domain_net: u32, scope: GossipScope, ts: u64, ttl: u16) -> GossipObject {
    GossipObject {
        object_type: GossipObjectType::Envelope as u16,
        object_hash: [hash_byte; 32],
        object_size: 100,
        domain_id: GossipDomainId::new(domain_net, [0u8; 32], scope),
        logical_timestamp: ts,
        origin_gateway: [0x01; 32],
        ttl_hops: ttl,
        propagation_flags: FLAG_FLOOD,
        payload_root: [0u8; 32],
        signature: [0u8; 64],
    }
}

fn make_domain_obj(hash_byte: u8, domain: &GossipDomainId, ts: u64) -> GossipObject {
    GossipObject {
        object_type: GossipObjectType::Envelope as u16,
        object_hash: [hash_byte; 32],
        object_size: 100,
        domain_id: *domain,
        logical_timestamp: ts,
        origin_gateway: [0x01; 32],
        ttl_hops: 20,
        propagation_flags: FLAG_FLOOD,
        payload_root: [0u8; 32],
        signature: [0u8; 64],
    }
}

// ── RetentionManager ──

#[test]
fn test_retention_manager_lifecycle() {
    let mut rm = RetentionManager::new();
    assert!(rm.is_empty());
    assert_eq!(rm.len(), 0);

    rm.admit([0x01; 32], RetentionClass::Ephemeral, 100);
    rm.admit([0x02; 32], RetentionClass::Consensus, 100);
    rm.admit([0x03; 32], RetentionClass::Archive, 100);

    assert_eq!(rm.len(), 3);
    assert_eq!(rm.count_by_class(RetentionClass::Ephemeral), 1);
    assert_eq!(rm.count_by_class(RetentionClass::Consensus), 1);

    // Check class
    assert_eq!(rm.get_class(&[0x01; 32]), Some(RetentionClass::Ephemeral));
    assert_eq!(rm.get_class(&[0xFF; 32]), None);

    // Ephemeral expires after 60 units
    assert!(!rm.is_expired(&[0x01; 32], 159));
    assert!(rm.is_expired(&[0x01; 32], 160));

    // Consensus expires after 86400 units
    assert!(!rm.is_expired(&[0x02; 32], 86499));

    // Archive never expires
    assert!(!rm.is_expired(&[0x03; 32], u64::MAX - 1));

    // Not tracked = expired
    assert!(rm.is_expired(&[0xFF; 32], 100));
}

#[test]
fn test_retention_manager_custom_duration() {
    let mut rm = RetentionManager::new();
    rm.set_duration(RetentionClass::Ephemeral, 10);

    rm.admit([0x01; 32], RetentionClass::Ephemeral, 100);
    assert!(!rm.is_expired(&[0x01; 32], 109));
    assert!(rm.is_expired(&[0x01; 32], 110));
}

#[test]
fn test_retention_manager_cleanup() {
    let mut rm = RetentionManager::new();

    rm.admit([0x01; 32], RetentionClass::Ephemeral, 100); // expires at 160
    rm.admit([0x02; 32], RetentionClass::Consensus, 100); // expires at 86500
    rm.admit([0x03; 32], RetentionClass::Archive, 100); // never expires

    let removed = rm.cleanup(200);
    assert_eq!(removed, 1); // only ephemeral expired
    assert_eq!(rm.len(), 2);

    let removed = rm.cleanup(86501);
    assert_eq!(removed, 1); // consensus expired
    assert_eq!(rm.len(), 1); // only archive remains
}

#[test]
fn test_retention_class_all_variants() {
    assert_eq!(
        RetentionClass::from_u16(0x0001),
        Some(RetentionClass::Ephemeral)
    );
    assert_eq!(
        RetentionClass::from_u16(0x0002),
        Some(RetentionClass::Mission)
    );
    assert_eq!(
        RetentionClass::from_u16(0x0003),
        Some(RetentionClass::Consensus)
    );
    assert_eq!(
        RetentionClass::from_u16(0x0004),
        Some(RetentionClass::Archive)
    );
    assert!(RetentionClass::from_u16(0x0000).is_none());
    assert!(RetentionClass::from_u16(0x0005).is_none());
}

#[test]
fn test_retention_class_all_durations() {
    assert_eq!(RetentionClass::Ephemeral.default_duration(), 60);
    assert_eq!(RetentionClass::Mission.default_duration(), 3600);
    assert_eq!(RetentionClass::Consensus.default_duration(), 86400);
    assert_eq!(RetentionClass::Archive.default_duration(), u64::MAX);
}

// ── Bitmap summary extended ──

#[test]
fn test_bitmap_large_range() {
    let mut bmp = BitmapSummary::new(0, 10000);
    assert_eq!(bmp.bits.len(), 1250);

    bmp.set(0);
    bmp.set(9999);
    bmp.set(5000);
    assert_eq!(bmp.count_set(), 3);
    assert!(bmp.is_set(0));
    assert!(bmp.is_set(9999));
    assert!(bmp.is_set(5000));
    assert!(!bmp.is_set(1));
}

#[test]
fn test_bitmap_nonzero_range_start() {
    let mut bmp = BitmapSummary::new(100, 50);
    assert_eq!(bmp.bits.len(), 7); // ceil(50/8)

    bmp.set(0);
    bmp.set(49);
    assert!(bmp.is_set(0));
    assert!(bmp.is_set(49));
}

// ── Bloom filter edge cases ──

#[test]
fn test_bloom_filter_empty() {
    let bloom = BloomSummary::new();
    assert_eq!(bloom.item_count, 0);
    assert!(!bloom.might_contain(&[0xAA; 32]));
}

#[test]
fn test_bloom_filter_default_trait() {
    let bloom = BloomSummary::default();
    assert_eq!(bloom.bits.len(), 256);
    assert_eq!(bloom.hash_count, 3);
}

// ── Anti-entropy extended ──

#[test]
fn test_anti_entropy_empty_domain() {
    let domain = GossipDomainId::new(1, [0xAA; 32], GossipScope::GLOBAL);
    let summary = GossipStateSummary::compute(&domain, &[]);

    assert_eq!(summary.object_count, 0);
    assert_eq!(summary.watermark, 0);
    assert_eq!(summary.state_root, [0u8; 32]);
}

#[test]
fn test_anti_entropy_multiple_objects_ordering() {
    let domain = GossipDomainId::new(1, [0xAA; 32], GossipScope::GLOBAL);

    // Insert in reverse order — summary should be deterministic
    let obj_a = make_domain_obj(0x01, &domain, 1000);
    let obj_b = make_domain_obj(0x02, &domain, 2000);
    let obj_c = make_domain_obj(0x03, &domain, 3000);

    let s1 = GossipStateSummary::compute(&domain, &[obj_c.clone(), obj_a.clone(), obj_b.clone()]);
    let s2 = GossipStateSummary::compute(&domain, &[obj_a, obj_b, obj_c]);

    assert!(s1.matches(&s2));
    assert_eq!(s1.object_count, 3);
    assert_eq!(s1.watermark, 3000);
}

#[test]
fn test_anti_entropy_watermark() {
    let domain = GossipDomainId::new(1, [0xAA; 32], GossipScope::GLOBAL);
    let obj = make_domain_obj(0x01, &domain, 5000);
    let summary = GossipStateSummary::compute(&domain, &[obj]);
    assert_eq!(summary.watermark, 5000);
}

#[test]
fn test_reconciliation_config_default() {
    let config = ReconciliationConfig::default();
    assert_eq!(config.interval_secs, 60);
}

// ── first_valid_hash_wins ──

#[test]
fn test_first_valid_hash_wins_basic() {
    let objects = vec![
        make_obj(0xBB, 1, GossipScope::GLOBAL, 100, 10),
        make_obj(0xAA, 1, GossipScope::GLOBAL, 100, 10),
        make_obj(0xCC, 1, GossipScope::GLOBAL, 100, 10),
    ];

    // All valid
    let winner = first_valid_hash_wins(&objects, |_| true).unwrap();
    assert_eq!(winner.object_hash[0], 0xAA);
}

#[test]
fn test_first_valid_hash_wins_filters_invalid() {
    let objects = vec![
        make_obj(0xAA, 1, GossipScope::GLOBAL, 100, 10),
        make_obj(0xBB, 1, GossipScope::GLOBAL, 100, 10),
    ];

    // Only 0xBB is valid
    let winner = first_valid_hash_wins(&objects, |o| o.object_hash[0] == 0xBB).unwrap();
    assert_eq!(winner.object_hash[0], 0xBB);
}

#[test]
fn test_first_valid_hash_wins_none_valid() {
    let objects = vec![make_obj(0xAA, 1, GossipScope::GLOBAL, 100, 10)];

    let winner = first_valid_hash_wins(&objects, |_| false);
    assert!(winner.is_none());
}

#[test]
fn test_first_valid_hash_wins_tiebreak_origin() {
    let mut obj_a = make_obj(0xAA, 1, GossipScope::GLOBAL, 100, 10);
    obj_a.origin_gateway = [0x02; 32];
    let mut obj_b = make_obj(0xAA, 1, GossipScope::GLOBAL, 100, 10);
    obj_b.origin_gateway = [0x01; 32];

    let objects = vec![obj_a, obj_b];
    let winner = first_valid_hash_wins(&objects, |_| true).unwrap();
    assert_eq!(winner.origin_gateway[0], 0x01); // lower origin wins
}

// ── Sort canonical extended ──

#[test]
fn test_sort_canonical_deterministic() {
    let mut objects = vec![
        make_obj(0xCC, 2, GossipScope::GLOBAL, 100, 10),
        make_obj(0xAA, 1, GossipScope::GLOBAL, 200, 10),
        make_obj(0xBB, 1, GossipScope::GLOBAL, 100, 10),
    ];

    sort_canonical(&mut objects);
    let first_order: Vec<u8> = objects.iter().map(|o| o.object_hash[0]).collect();

    // Sort again — same order
    sort_canonical(&mut objects);
    let second_order: Vec<u8> = objects.iter().map(|o| o.object_hash[0]).collect();

    assert_eq!(first_order, second_order);
}

#[test]
fn test_sort_canonical_by_domain_first() {
    let mut objects = vec![
        make_obj(0xAA, 2, GossipScope::GLOBAL, 100, 10),
        make_obj(0xAA, 1, GossipScope::GLOBAL, 100, 10),
    ];

    sort_canonical(&mut objects);
    assert_eq!(objects[0].domain_id.network_id, 1);
    assert_eq!(objects[1].domain_id.network_id, 2);
}

// ── GossipScope comprehensive ──

#[test]
fn test_gossip_scope_all_variants() {
    assert_eq!(GossipScope::from_u16(0x0001), Some(GossipScope::GLOBAL));
    assert_eq!(GossipScope::from_u16(0x0002), Some(GossipScope::REGIONAL));
    assert_eq!(GossipScope::from_u16(0x0003), Some(GossipScope::MISSION));
    assert_eq!(GossipScope::from_u16(0x0004), Some(GossipScope::PRIVATE));
    assert_eq!(GossipScope::from_u16(0x0005), Some(GossipScope::LOCAL));
    assert_eq!(GossipScope::from_u16(0x0006), Some(GossipScope::CONSENSUS));
    assert!(GossipScope::from_u16(0x0000).is_none());
    assert!(GossipScope::from_u16(0x0007).is_none());
}

// ── Dedup edge cases ──

#[test]
fn test_dedup_set_eviction() {
    let mut set = DedupSet::new(3);
    set.insert_if_new([0x01; 32]);
    set.insert_if_new([0x02; 32]);
    set.insert_if_new([0x03; 32]);
    assert_eq!(set.len(), 3);

    // Insert 4th — should evict oldest
    set.insert_if_new([0x04; 32]);
    assert_eq!(set.len(), 3);
}

#[test]
fn test_replay_cache_is_empty() {
    let mut cache = GossipReplayCache::new(100, 1000);
    assert!(cache.is_empty());
    cache.check_and_insert([0x01; 32], 100).unwrap();
    assert!(!cache.is_empty());
}

// ── Object hash comprehensive ──

#[test]
fn test_object_hash_different_for_different_objects() {
    let mut obj_a = make_obj(0xAA, 1, GossipScope::GLOBAL, 1000, 10);
    obj_a.payload_root = [0x01; 32];
    let mut obj_b = make_obj(0xBB, 1, GossipScope::GLOBAL, 1000, 10);
    obj_b.payload_root = [0x02; 32];

    let h_a = obj_a.derive_object_hash();
    let h_b = obj_b.derive_object_hash();
    assert_ne!(h_a, h_b);
}

#[test]
fn test_ordering_key_components() {
    let obj = make_obj(0xAA, 1, GossipScope::GLOBAL, 1000, 10);
    let (domain_bytes, ts, hash) = obj.ordering_key();

    assert!(!domain_bytes.is_empty());
    assert_eq!(ts, 1000);
    assert_eq!(hash, obj.object_hash);
}
