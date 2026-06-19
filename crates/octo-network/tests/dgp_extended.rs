//! Integration tests for DGP gaps — compression, object APIs, domain enums,
//! anti-entropy, directed mode, dedup helpers, ordering.

use octo_network::dgp::anti_entropy::{AntiEntropyReconciler, GossipStateSummary};
use octo_network::dgp::compression::{BitmapSummary, BloomSummary, RetentionClass};
use octo_network::dgp::dedup::{DedupSet, GossipReplayCache};
use octo_network::dgp::directed::DirectedMode;
use octo_network::dgp::domain::{GossipDomainId, GossipScope};
use octo_network::dgp::flood::FloodMode;
use octo_network::dgp::object::{
    validate_flags, GossipObject, GossipObjectType, GossipPriority, FLAG_ANTI_ENTROPY,
    FLAG_COMPRESSED, FLAG_DIRECTED, FLAG_FLOOD, FLAG_RELIABLE,
};
use octo_network::dgp::ordering::sort_canonical;

fn make_obj(hash_byte: u8, flags: u64, ts: u64, ttl: u16) -> GossipObject {
    GossipObject {
        object_type: GossipObjectType::Envelope as u16,
        object_hash: [hash_byte; 32],
        object_size: 100,
        domain_id: GossipDomainId::new(1, [0u8; 32], GossipScope::GLOBAL),
        logical_timestamp: ts,
        origin_gateway: [0x01; 32],
        ttl_hops: ttl,
        propagation_flags: flags,
        payload_root: [0u8; 32],
        signature: [0u8; 64],
    }
}

fn make_mission_obj(
    hash_byte: u8,
    flags: u64,
    mission: [u8; 32],
    ts: u64,
    ttl: u16,
) -> GossipObject {
    GossipObject {
        object_type: GossipObjectType::MissionState as u16,
        object_hash: [hash_byte; 32],
        object_size: 100,
        domain_id: GossipDomainId::new(1, mission, GossipScope::MISSION),
        logical_timestamp: ts,
        origin_gateway: [0x01; 32],
        ttl_hops: ttl,
        propagation_flags: flags,
        payload_root: [0u8; 32],
        signature: [0u8; 64],
    }
}

fn make_domain_obj(
    hash_byte: u8,
    flags: u64,
    domain: &GossipDomainId,
    ts: u64,
    ttl: u16,
) -> GossipObject {
    GossipObject {
        object_type: GossipObjectType::Envelope as u16,
        object_hash: [hash_byte; 32],
        object_size: 100,
        domain_id: *domain,
        logical_timestamp: ts,
        origin_gateway: [0x01; 32],
        ttl_hops: ttl,
        propagation_flags: flags,
        payload_root: [0u8; 32],
        signature: [0u8; 64],
    }
}

// ── Object type enum ──

#[test]
fn test_gossip_object_type_from_u16() {
    assert_eq!(
        GossipObjectType::from_u16(0x0001),
        Some(GossipObjectType::Envelope)
    );
    assert_eq!(
        GossipObjectType::from_u16(0x0008),
        Some(GossipObjectType::SnapshotFragment)
    );
    assert!(GossipObjectType::from_u16(0x00FF).is_none());
}

// ── Flag validation ──

#[test]
fn test_validate_flags_valid() {
    assert!(validate_flags(FLAG_FLOOD).is_ok());
    assert!(validate_flags(FLAG_FLOOD | FLAG_RELIABLE | FLAG_COMPRESSED).is_ok());
    assert!(validate_flags(FLAG_DIRECTED | FLAG_ANTI_ENTROPY).is_ok());
}

#[test]
fn test_validate_flags_reserved_bits() {
    assert!(validate_flags(0x10000).is_err()); // bit 16 is reserved
    assert!(validate_flags(u64::MAX).is_err());
}

// ── Object hash and verification ──

#[test]
fn test_derive_object_hash_deterministic() {
    let obj = make_obj(0xAA, FLAG_FLOOD, 1000, 10);
    let h1 = obj.derive_object_hash();
    let h2 = obj.derive_object_hash();
    assert_eq!(h1, h2);
}

#[test]
fn test_verify_hash_matches() {
    let mut obj = make_obj(0xAA, FLAG_FLOOD, 1000, 10);
    obj.object_hash = obj.derive_object_hash();
    assert!(obj.verify_hash());
}

#[test]
fn test_verify_hash_mismatch() {
    let obj = make_obj(0xAA, FLAG_FLOOD, 1000, 10);
    // object_hash is [0xAA; 32], but derived hash will be different
    assert!(!obj.verify_hash());
}

// ── Ordering key ──

#[test]
fn test_ordering_key_deterministic() {
    let obj = make_obj(0xAA, FLAG_FLOOD, 1000, 10);
    let k1 = obj.ordering_key();
    let k2 = obj.ordering_key();
    assert_eq!(k1, k2);
}

// ── TTL decrement ──

#[test]
fn test_ttl_decrement() {
    let mut obj = make_obj(0xAA, FLAG_FLOOD, 1000, 2);
    assert!(obj.decrement_ttl()); // ttl=1, still alive
    assert_eq!(obj.ttl_hops, 1);
    assert!(!obj.decrement_ttl()); // ttl=0, expired
    assert_eq!(obj.ttl_hops, 0);
}

// ── Domain scope enum ──

#[test]
fn test_gossip_scope_from_u16() {
    assert_eq!(GossipScope::from_u16(0x0001), Some(GossipScope::GLOBAL));
    assert_eq!(GossipScope::from_u16(0x0006), Some(GossipScope::CONSENSUS));
    assert!(GossipScope::from_u16(0x00FF).is_none());
}

#[test]
fn test_gossip_scope_default_ttl() {
    assert_eq!(GossipScope::GLOBAL.default_ttl(), 20);
    assert_eq!(GossipScope::LOCAL.default_ttl(), 3);
    assert_eq!(GossipScope::MISSION.default_ttl(), 5);
}

// ── Compression: Bloom filter ──

#[test]
fn test_bloom_filter_insert_and_check() {
    let mut bloom = BloomSummary::new();
    assert_eq!(bloom.bits.len(), 256);

    let hash_a = [0xAA; 32];
    let hash_b = [0xBB; 32];

    assert!(!bloom.might_contain(&hash_a)); // not inserted yet

    bloom.insert(&hash_a);
    assert!(bloom.might_contain(&hash_a)); // inserted
    assert_eq!(bloom.item_count, 1);

    bloom.insert(&hash_b);
    assert!(bloom.might_contain(&hash_b));
    assert_eq!(bloom.item_count, 2);
}

#[test]
fn test_bloom_filter_false_positive_rate() {
    let mut bloom = BloomSummary::new();
    // Insert 100 items
    for i in 0..100u8 {
        bloom.insert(&[i; 32]);
    }

    // Check a non-inserted item — may be false positive but should be rare
    let false_positives = (0u16..100)
        .filter(|&i| bloom.might_contain(&[(i + 200) as u8; 32]))
        .count();

    // With 2048 bits and 3 hashes, 100 items should have low FP rate
    assert!(false_positives < 50); // very generous bound
}

#[test]
fn test_bloom_filter_custom_params() {
    let bloom = BloomSummary::with_params(128, 5);
    assert_eq!(bloom.bits.len(), 128);
    assert_eq!(bloom.hash_count, 5);
}

// ── Compression: Bitmap ──

#[test]
fn test_bitmap_set_and_check() {
    let mut bmp = BitmapSummary::new(0, 64);
    assert_eq!(bmp.bits.len(), 8);

    bmp.set(0);
    bmp.set(63);
    assert!(bmp.is_set(0));
    assert!(bmp.is_set(63));
    assert!(!bmp.is_set(32));
}

#[test]
fn test_bitmap_out_of_range_ignored() {
    let mut bmp = BitmapSummary::new(0, 10);
    bmp.set(100); // out of range, should not panic
    assert_eq!(bmp.count_set(), 0);
}

#[test]
fn test_bitmap_count_set() {
    let mut bmp = BitmapSummary::new(0, 64);
    bmp.set(0);
    bmp.set(1);
    bmp.set(2);
    assert_eq!(bmp.count_set(), 3);
}

// ── Compression: Retention ──

#[test]
fn test_retention_class_enum() {
    assert_eq!(
        RetentionClass::from_u16(0x0001),
        Some(RetentionClass::Ephemeral)
    );
    assert_eq!(
        RetentionClass::from_u16(0x0004),
        Some(RetentionClass::Archive)
    );
    assert!(RetentionClass::from_u16(0x00FF).is_none());
}

#[test]
fn test_retention_class_default_duration() {
    assert_eq!(RetentionClass::Ephemeral.default_duration(), 60);
    assert_eq!(RetentionClass::Archive.default_duration(), u64::MAX);
}

// ── Anti-entropy ──

#[test]
fn test_anti_entropy_matching_summaries() {
    let domain = GossipDomainId::new(1, [0xAA; 32], GossipScope::GLOBAL);
    let obj = make_obj(0x01, FLAG_FLOOD, 1000, 10);

    let s1 = GossipStateSummary::compute(&domain, std::slice::from_ref(&obj));
    let s2 = GossipStateSummary::compute(&domain, &[obj]);

    assert!(s1.matches(&s2));
}

#[test]
fn test_anti_entropy_divergent_summaries() {
    let domain = GossipDomainId::new(1, [0xAA; 32], GossipScope::GLOBAL);
    let obj_a = make_domain_obj(0x01, FLAG_FLOOD, &domain, 1000, 10);
    let obj_b = make_domain_obj(0x02, FLAG_FLOOD, &domain, 1000, 10);

    let s1 = GossipStateSummary::compute(&domain, &[obj_a]);
    let s2 = GossipStateSummary::compute(&domain, &[obj_b]);

    assert!(!s1.matches(&s2));
}

#[test]
fn test_anti_entropy_reconcile_matching() {
    let domain = GossipDomainId::new(1, [0xAA; 32], GossipScope::GLOBAL);
    let obj = make_obj(0x01, FLAG_FLOOD, 1000, 10);

    let summary = GossipStateSummary::compute(&domain, std::slice::from_ref(&obj));

    let result =
        AntiEntropyReconciler::reconcile(&summary, &summary, &[obj], &[[0x01; 32]]).unwrap();

    assert!(result.missing_from_peer.is_empty());
    assert!(result.missing_from_us.is_empty());
}

#[test]
fn test_anti_entropy_reconcile_divergent() {
    let domain = GossipDomainId::new(1, [0xAA; 32], GossipScope::GLOBAL);
    let obj_local = make_domain_obj(0x01, FLAG_FLOOD, &domain, 1000, 10);
    let obj_remote = make_domain_obj(0x02, FLAG_FLOOD, &domain, 1000, 10);

    let local_summary = GossipStateSummary::compute(&domain, std::slice::from_ref(&obj_local));
    let remote_summary = GossipStateSummary::compute(&domain, &[obj_remote]);

    let result = AntiEntropyReconciler::reconcile(
        &local_summary,
        &remote_summary,
        &[obj_local],
        &[[0x02; 32]],
    )
    .unwrap();

    assert_eq!(result.missing_from_peer.len(), 1);
    assert_eq!(result.missing_from_peer[0], [0x01; 32]);
    assert_eq!(result.missing_from_us.len(), 1);
    assert_eq!(result.missing_from_us[0], [0x02; 32]);
}

// ── Directed mode ──

#[test]
fn test_directed_mode_eligible_filter() {
    let directed = make_mission_obj(0xAA, FLAG_DIRECTED, [0x01; 32], 1000, 10);
    let flood = make_mission_obj(0xBB, FLAG_FLOOD, [0x01; 32], 1000, 10);

    let objects = vec![directed, flood];
    let eligible = DirectedMode::eligible(&objects);
    assert_eq!(eligible.len(), 1);
}

#[test]
fn test_directed_mode_target_validation() {
    let obj = make_mission_obj(0xAA, FLAG_DIRECTED, [0x01; 32], 1000, 10);

    assert!(DirectedMode::is_valid_target(&obj, &[[0x01; 32]]));
    assert!(!DirectedMode::is_valid_target(&obj, &[[0x02; 32]]));
}

// ── Dedup helpers ──

#[test]
fn test_dedup_set_contains() {
    let mut set = DedupSet::new(100);
    set.insert_if_new([0xAA; 32]);

    assert!(set.contains(&[0xAA; 32]));
    assert!(!set.contains(&[0xBB; 32]));
}

#[test]
fn test_dedup_set_is_empty() {
    let mut set = DedupSet::new(100);
    assert!(set.is_empty());
    set.insert_if_new([0xAA; 32]);
    assert!(!set.is_empty());
}

#[test]
fn test_replay_cache_purge_expired() {
    let mut cache = GossipReplayCache::new(100, 1000);

    cache.check_and_insert([0x01; 32], 100).unwrap();
    cache.check_and_insert([0x02; 32], 200).unwrap();
    cache.check_and_insert([0x03; 32], 300).unwrap();

    assert_eq!(cache.len(), 3);

    // Purge with timestamp 1500 — entries with ts < 500 should be purged
    let purged = cache.purge_expired(1500);
    assert!(purged >= 2);
}

// ── Ordering by priority ──

#[test]
fn test_sort_canonical() {
    let mut objects = vec![
        make_obj(0xCC, FLAG_FLOOD, 3000, 10),
        make_obj(0xAA, FLAG_FLOOD, 1000, 10),
        make_obj(0xBB, FLAG_FLOOD, 2000, 10),
    ];

    sort_canonical(&mut objects);

    // Should be sorted by canonical ordering
    assert_eq!(objects[0].object_hash[0], 0xAA);
    assert_eq!(objects[1].object_hash[0], 0xBB);
    assert_eq!(objects[2].object_hash[0], 0xCC);
}

// ── Flood mode ──

#[test]
fn test_flood_mode_eligible_excludes_expired() {
    let alive = make_obj(0xAA, FLAG_FLOOD, 1000, 10);
    let expired = make_obj(0xBB, FLAG_FLOOD, 2000, 0);

    let objects = vec![alive, expired];
    let eligible = FloodMode::eligible(&objects);
    assert_eq!(eligible.len(), 1);
    assert_eq!(eligible[0].object_hash[0], 0xAA);
}

// ── Priority enum ──

#[test]
fn test_gossip_priority_ordering() {
    assert!(GossipPriority::Critical < GossipPriority::Consensus);
    assert!(GossipPriority::Consensus < GossipPriority::Mission);
    assert!(GossipPriority::Mission < GossipPriority::Standard);
    assert!(GossipPriority::Standard < GossipPriority::Bulk);
    assert!(GossipPriority::Bulk < GossipPriority::Archive);
}
