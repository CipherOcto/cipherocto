# 0011-h-s-a-reputation-store — Substrate additions for ReputationStore (RFC-0860)

## Status

Open (2026-09-20) — Substrate-additions prerequisite per RFC-0011-h §Substrate-Additions Companion Missions row G13 + RFC-0011-r Phase 10 §Substrate-Additions Companion Missions. Substrate slice pending per the Phase 4 paired-substrate completion pattern (companion YAML filled in → substrate lands → YAML Claimed → CLI dispatch lands → YAML Completed paired). RFC-0011-r Phase 10 reputation-store amendment Draft landed at `next 76998e03`.

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G13 + RFC-0011-r Phase 10 §Substrate-Additions Companion Missions + RFC-0860 Reputation Store.

## Summary

EXTENDS existing `ReputationStore` trait at `crates/octo-reputation/src/store/mod.rs:51` with 2 new async methods (`list(filter)` + `peer_reputation(did)`) + adds `ReputationFilter` enum (All + AboveScore(u32) + BelowScore(u32)) + adds `PeerReputation` struct (peer_did + score + attestations_count + last_updated_epoch). Required by `octo network reputation list` (read) + `octo network reputation show` (read) per RFC-0011-r Phase 10 §Subcommand Taxonomy.

### Substrate additions target

```rust
// crates/octo-reputation/src/store/mod.rs (EXTEND existing trait at L51)
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

- [ ] `ReputationStore` trait EXTENDED with 2 new async methods (`list` + `peer_reputation`) per RFC-0011-h §Substrate-Additions row G13 + RFC-0011-r Phase 10 §Substrate Mapping Table
- [ ] `list(filter: ReputationFilter) -> StoreResult<Vec<PeerReputation>>` signature lands at end of trait
- [ ] `peer_reputation(did: &RecorderDid) -> StoreResult<Option<PeerReputation>>` signature lands at end of trait
- [ ] `ReputationFilter` enum (All + AboveScore(u32) + BelowScore(u32)) lands at same path with `#[serde(rename_all = "lowercase")]`
- [ ] `PeerReputation` struct lands at same path with `peer_did: RecorderDid` + `score: u32` + `attestations_count: u32` + `last_updated_epoch: u64`
- [ ] `InMemoryReputationStore` (memory.rs) EXTENDED with stub impls returning `Ok(Vec::new())` + `Ok(None)`
- [ ] `StoolapReputationStore` (stoolap.rs) EXTENDED with stub impls returning `Ok(Vec::new())` + `Ok(None)`
- [ ] `cargo clippy -p octo-reputation --all-targets -- -D warnings` clean (NO regression of existing 11+ modules)
- [ ] `cargo test -p octo-reputation --lib` green (≥5 unit tests added; zero regression)
- [ ] Layer discipline preserved (Layer B only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [ ] ≥5 unit tests + ≥1 integration test (substrate-faithful boundary tests pin filter-mapping + empty-default + above-score + below-score + RecorderDid equality)

## Dependencies

- RFC-0011-h Accepted (RFC-0011-h must be Accepted before this mission lands per RFC-0011-h §Substrate-Additions Companion Missions)
- RFC-0011-r Phase 10 reputation-store amendment Draft at `next 76998e03`
- RFC-0860 Reputation Store (governing RFC)
- Existing `ReputationStore` trait at `crates/octo-reputation/src/store/mod.rs:51`

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-reputation` covers that surface; CREATED at CLI dispatch slice time per user decision)
- Wire format versioning (deferred to RFC-0011-h §Future Work items F8 + F9)
- Per-extension transport impl (substrate-ext-reputation-store-* Layer D follow-on missions, OUT OF SCOPE for this trait-only phase)
- Real reputation aggregation logic (stub returns empty Vec / None; real aggregation in follow-on Layer D adapter mission per per-extension crate pattern)
- Pagination for `list` output (OUT OF SCOPE for this trait-only phase)

## Notes

Stub originally filed 2026-09-18 per [[no-phantom-mission-pointers]]. Full AC + scope land in Phase 10 stub fill-in commit at `next PENDING` per the Phase 4 paired-substrate completion pattern. Phase 10 follows the Phase 5 RFC-0011-m 5-commit pattern (stub fill-in → substrate slice → YAML Claimed → CLI dispatch → YAMLs Completed) verified at `next 8e7c5cec`, `24bfec96`, `fcb58331`, `346f10cc`, `97955c00`. Slot 89 `NetworkSubstrateUnavailable` REUSE per Phase 6 precedent (0 NEW OctoCliError variants). Additive trait extension pattern preserved per Phase 4 G22 precedent (no regression of existing 11+ modules; both impls get stub implementations for new methods). `#[non_exhaustive]` not needed on `ReputationFilter` (closed enum per `#[serde(rename_all = "lowercase")]`). `PeerReputation` is a struct (not enum) so no `#[non_exhaustive]` needed. CLI handler uses `tokio::runtime::Handle::current().block_on(...)` wrapper for async trait dispatch.
