# 0011-h-s-a-discovery-advertisement-cache — Substrate additions for MissionAdvertisementCache::get + iter

## Status

Completed (2026-09-20) — Substrate additions + CLI dispatch CLOSED. Substrate-faithful `MissionAdvertisementCache` lookup + iteration landed in `crates/octo-network/src/mon/discovery.rs` at `next 24bfec96`. CLI dispatch wired at `octo network discovery advertisement show --advertisement-id <HEX> [--hops <U16>]` at `next 346f10cc`. Substrate-additions + CLI dispatch landed end-to-end per RFC-0011-m Phase 5 row G23.

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G23 + RFC-0011-m Phase 5 §Substrate-Additions Companion Missions + RFC-0855 §8.2 Mission Advertisement

## Summary

Adds lookup + iteration façade for cached mission advertisements. Required by `octo network discovery advertisement show` (Phase 5 G23 substrate companion).

### Substrate additions target

```rust
// crates/octo-network/src/mon/discovery.rs (existing module extended)
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MissionAdvertisementCache {
    entries: BTreeMap<[u8; 32], MissionAdvertisement>,
}

impl MissionAdvertisementCache {
    /// Substrate-faithful lookup helper for `octo network
    /// discovery advertisement show --advertisement-id <ID>`
    /// (RFC-0011-m Phase 5 G23).
    #[must_use]
    pub fn get(&self, advertisement_id: &[u8; 32]) -> Option<&MissionAdvertisement> {
        self.entries.get(advertisement_id)
    }

    /// Substrate-faithful iterator for `octo network
    /// discovery advertisement show` (RFC-0011-m Phase 5 G23).
    /// Returns gateway-id-keyed iteration per RFC-0855 §8.2.
    pub fn iter(&self) -> impl Iterator<Item = ([u8; 32], &MissionAdvertisement)> {
        self.entries.iter().map(|(k, v)| (*k, v))
    }

    /// Insert or replace an advertisement entry (substrate-faithful
    /// registry surface; Phase 5 closure path remains
    /// `AdapterUnwired` for write paths).
    pub fn insert(&mut self, advertisement: MissionAdvertisement) {
        let key = advertisement.advertisement_hash();
        self.entries.insert(key, advertisement);
    }

    /// Number of cached advertisements (operator-side
    /// observability helper).
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty (operator-side
    /// observability helper).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
```

Layer B substrate additions land in `crates/octo-network/src/mon/discovery.rs` (existing module extended). No new module: the cache types are surfaced in the existing discovery module because they share the same substrate anchors (`MissionAdvertisement`, `MissionId`, scope enum) per RFC-0855 §8.2.

`BTreeMap` chosen over `HashMap` for deterministic iteration order (RFC-0011-h §Output Envelope order determinism) and substrate-faithful ordering on `iter()` output (RFC-0855 §8.2 deterministic ordering contract).

## Acceptance Criteria

- [x] `MissionAdvertisementCache` struct lands in `crates/octo-network/src/mon/discovery.rs` per RFC-0011-h §Substrate-Additions row G23 (next 24bfec96)
- [x] `get(advertisement_id: &[u8; 32]) -> Option<&MissionAdvertisement>` method lands at same path
- [x] `iter() -> impl Iterator<Item = ([u8; 32], &MissionAdvertisement)>` method lands at same path
- [x] `insert(MissionAdvertisement)` registry helper lands (substrate-faithful surface; CLI dispatch does NOT call this — write paths remain `AdapterUnwired` per Phase 6 follow-on `0011-h-s-a-discovery-advertisement-persistence`)
- [x] `len()` + `is_empty()` observability helpers land
- [x] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-network --lib` green (1451/1451, +6 above Phase 4 baseline of 1438, 6 new MissionAdvertisementCache unit tests added)
- [x] Layer discipline preserved (Layer B only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [x] ≥3 unit tests + ≥1 integration test (6 MissionAdvertisementCache unit tests pin get-miss + get-hit + iter-empty + iter-non-empty + insert-idempotent + BTreeMap deterministic ordering + len/is_empty observability helpers; integration test deferred to Phase 6 persistence adapter follow-on)

## Dependencies

Hard sequencing: RFC-0011-h must be Accepted before this mission lands. Substrate-first ordering per [[no-phantom-mission-pointers]]: G23 substrate slice (this mission) lands BEFORE Phase 5 CLI dispatch slice.

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-discovery` covers that surface in Phase 5 CLI dispatch slice)
- Wire format versioning (deferred to substrate-additions companion)
- Per-extension transport impl (deferred to per-extension crate pattern)
- Persistence adapter (Phase 6 follow-on per `0011-h-s-a-discovery-advertisement-persistence`)
- TTL eviction policy (deferred to substrate-additions companion; substrate-faithful surface exposes `is_ttl_exceeded` on `MissionAdvertisement` already)

## Notes

Stub fill-in 2026-09-20 per RFC-0011-m closure card at `next 8e7c5cec`. Substrate slice landed 2026-09-20 at `next 24bfec96`. CLI dispatch slice landed 2026-09-20 at `next 346f10cc`. The substrate-faithful `Option<&MissionAdvertisement>` translation surfaces as typed exit 89 `NetworkSubstrateUnavailable { companion: "G23" }` in the CLI dispatch slice. The `BTreeMap` choice honors the deterministic ordering contract per RFC-0855 §8.2 + RFC-0011-h §Output Envelope order determinism. The `iter()` method returns owned `[u8; 32]` keys so the iterator lifetime is decoupled from the cache lifetime (substrate-faithful boundary per [[cipherocto-design-principles]] §No premature coupling).
