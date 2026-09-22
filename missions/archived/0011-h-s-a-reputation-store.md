# 0011-h-s-a-reputation-store — Substrate additions for ReputationStore (RFC-0860)

## Status

Completed (2026-09-20) — Substrate additions for ReputationStore Phase 10 G13 FULL slice CLOSED. Substrate slice at `next c95fd8cb` + paired-YAML Claimed transition at `next 9159e613` + CLI dispatch slice at `next 026c2e5a` (Phase 10 FULL chain closed). CLI mission YAML `0011-h-network-reputation` CREATED at CLI dispatch slice time per user decision (`Completed` status). 2 NEW subcommands (`reputation list` + `reputation show`) + 2 NEW output envelopes + 6 NEW test vectors `tv_net10_1` through `tv_net10_6`. Layer B substrate EXTENDED `ReputationStore` trait at `crates/octo-reputation/src/store/mod.rs` §ReputationStore trait with `list` + `peer_reputation` async methods + `ReputationFilter` enum + `PeerReputation` struct (additive trait extension per Phase 4 G22 precedent zero central edit). Layer C CLI dispatch exposes surface via `NetworkAction::Reputation { action: NetworkReputationAction }` clap variant. 427/427 octo-cli tests pass (was 421, +6 new). 239/239 octo-reputation tests pass (was 233, +6 substrate). 1489/1489 octo-network tests pass (zero regression). Layer A frozen preserved zero change. Slot 89 `NetworkSubstrateUnavailable` REUSE (0 NEW OctoCliError variants per user decision). Per-extension crate pattern preserved (trait in Layer B; concrete per-store impl extensions OUT OF SCOPE for follow-on Layer D adapter missions).

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G13 + RFC-0011-r Phase 10 §Substrate-Additions Companion Missions + RFC-0860.

## Summary

EXTENDS existing `ReputationStore` trait at `crates/octo-reputation/src/store/mod.rs` §ReputationStore trait with 2 new async methods (`list(filter)` + `peer_reputation(did)`) + adds `ReputationFilter` enum (All + AboveScore(u32) + BelowScore(u32)) + adds `PeerReputation` struct (peer_did + score + attestations_count + last_updated_epoch). Required by `octo network reputation list` (read) + `octo network reputation show` (read) per RFC-0011-r Phase 10 §Subcommand Taxonomy.

### Substrate additions target

```rust
// crates/octo-reputation/src/store/mod.rs (EXTEND existing trait)
#[async_trait::async_trait]
pub trait ReputationStore: Send + Sync {
    // ... existing 14+ methods unchanged ...

    /// List peer reputations matching the given filter
    /// (Phase 10 G13 per RFC-0011-r §Substrate Mapping
    /// Table). Returns empty Vec if no peers match.
    /// Per-extension impl crates (Layer D) provide real
    /// implementations.
    async fn list(
        &self,
        filter: ReputationFilter,
    ) -> StoreResult<Vec<PeerReputation>>;

    /// Load reputation for a specific peer DID
    /// (Phase 10 G13 per RFC-0011-r §Substrate Mapping
    /// Table). Returns None if the peer has no
    /// recorded reputation.
    async fn peer_reputation(
        &self,
        did: &RecorderDid,
    ) -> StoreResult<Option<PeerReputation>>;
}

/// `ReputationFilter` — filter enum for
/// `ReputationStore::list` (Phase 10 G13 per
/// RFC-0011-r §Substrate Mapping Table).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReputationFilter {
    /// All peers (no filter).
    All,
    /// Peers with score >= threshold.
    AboveScore(u32),
    /// Peers with score <= threshold.
    BelowScore(u32),
}

/// `PeerReputation` — peer reputation summary
/// projection (Phase 10 G13 per RFC-0011-r
/// §Substrate Mapping Table).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerReputation {
    /// Peer DID (RecorderDid canonical).
    pub peer_did: RecorderDid,
    /// Aggregate reputation score.
    pub score: u32,
    /// Number of attestations on record.
    pub attestations_count: u32,
    /// Last update epoch (RFC-0855 §epoch).
    pub last_updated_epoch: u64,
}
```

EXTENDS both `InMemoryReputationStore` (memory.rs) + `StoolapReputationStore` (stoolap.rs) with stub implementations returning empty Vec / None (no real aggregation logic in this trait-only phase; real aggregation in follow-on Layer D adapter mission).

Layer B substrate addition lands via EXTEND pattern (additive trait extension per Phase 4 G22 precedent). Both impls require stub implementations for new methods; downstream code unchanged.

## Acceptance Criteria

- [x] `ReputationStore` trait EXTENDED with 2 new async methods (`list` + `peer_reputation`) per RFC-0011-h §Substrate-Additions row G13 + RFC-0011-r Phase 10 §Substrate Mapping Table
- [x] `list(filter: ReputationFilter) -> StoreResult<Vec<PeerReputation>>` signature lands at end of trait
- [x] `peer_reputation(did: &RecorderDid) -> StoreResult<Option<PeerReputation>>` signature lands at end of trait
- [x] `ReputationFilter` enum (All + AboveScore(u32) + BelowScore(u32)) lands at same path with `#[serde(rename_all = "lowercase")]`
- [x] `PeerReputation` struct lands at same path with `peer_did: RecorderDid` + `score: u32` + `attestations_count: u32` + `last_updated_epoch: u64`
- [x] `InMemoryReputationStore` (memory.rs) EXTENDED with stub impls returning `Ok(Vec::new())` + `Ok(None)`
- [x] `StoolapReputationStore` (stoolap.rs) EXTENDED with stub impls returning `Ok(Vec::new())` + `Ok(None)`
- [x] `cargo clippy -p octo-reputation --all-targets -- -D warnings` clean (NO regression of existing 11+ modules)
- [x] `cargo test -p octo-reputation --lib` green (≥5 unit tests added; zero regression)
- [x] Layer discipline preserved (Layer B only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [x] ≥5 unit tests + ≥1 integration test (substrate-faithful boundary tests pin filter-mapping + empty-default + above-score + below-score + RecorderDid equality)

## Dependencies

- RFC-0011-h (must be Accepted before this mission lands per RFC-0011-h §Substrate-Additions Companion Missions)
- RFC-0011-r Phase 10 reputation-store amendment at `next 76998e03`
- Phase 10 G13 substrate stub fill-in at `next 6f32badb`
- Phase 10 G13 substrate slice at `next c95fd8cb`
- RFC-0860
- Existing `ReputationStore` trait at `crates/octo-reputation/src/store/mod.rs` §ReputationStore trait

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-reputation` covers that surface; CREATED at CLI dispatch slice time per user decision)
- Wire format versioning (deferred to RFC-0011-h §Future Work items F8 + F9)
- Per-extension transport impl (substrate-ext-reputation-store-* Layer D follow-on missions, OUT OF SCOPE for this trait-only phase)
- Real reputation aggregation logic (stub returns empty Vec / None; real aggregation in follow-on Layer D adapter mission per per-extension crate pattern)
- Pagination for `list` output (OUT OF SCOPE for this trait-only phase)

## Notes

Stub originally filed 2026-09-18 per [[no-phantom-mission-pointers]]. Full AC + scope land in Phase 10 stub fill-in commit at `next PENDING` per the Phase 4 paired-substrate completion pattern. Phase 10 follows the Phase 5 RFC-0011-m 5-commit pattern (stub fill-in → substrate slice → YAML Claimed → CLI dispatch → YAMLs Completed) verified at `next 8e7c5cec`, `24bfec96`, `fcb58331`, `346f10cc`, `97955c00`. Slot 89 `NetworkSubstrateUnavailable` REUSE per Phase 6 precedent (0 NEW OctoCliError variants). Additive trait extension pattern preserved per Phase 4 G22 precedent (no regression of existing 11+ modules; both impls get stub implementations for new methods). `#[non_exhaustive]` not needed on `ReputationFilter` (closed enum per `#[serde(rename_all = "lowercase")]`). `PeerReputation` is a struct (not enum) so no `#[non_exhaustive]` needed. CLI handler uses `tokio::runtime::Handle::current().block_on(...)` wrapper for async trait dispatch.
