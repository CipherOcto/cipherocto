# RFC-0011-r: `octo network` Phase 10 — Reputation Store (List + Show)

## Status

Accepted (2026-09-21) — RFC-0011-r promoted from Draft per the goal directive that all RFC-0011-h phases 7 to 14 plus retroactive Phases 1 to 6 + 8 to 10 + 12 must achieve 5-len DRY CLOSURE. Phase 10 retroactive multi-round DRY gate GREEN at R4 zero per the existing closure chain culminating in `next 6588aaf2` (4 rounds R1 fail → R1.5 fix → R2 fail → R2.5 fix → R3 fail → R3.5 fix → R4 zero = GATE GREEN).

> **Amendment chain:** Tenth amendment in the `0011-h-multiphase-rollout-plan` (see `docs/plans/2026-09-20-0011-h-multiphase-rollout-plan.md`, gitignored scratchpad per [[docs-plans-scratchpad]]). Phase 1 = RFC-0011-i. Phase 2 = RFC-0011-j. Phase 3 = RFC-0011-k. Phase 4 = RFC-0011-l. Phase 5 = RFC-0011-m. Phase 6 = RFC-0011-n. Phase 7 = RFC-0011-o. Phase 8 = RFC-0011-p. Phase 9 = RFC-0011-q. Phase 10 = RFC-0011-r (this RFC).

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu

## Summary

RFC-0011-r lands the **reputation store list + show** slice of RFC-0011-h §Implementation Phases. Two CLI subcommands wire to substrate (companion mission for 2 new trait methods on existing `ReputationStore` async trait):

- `octo network reputation list [--filter <filter>]` — read-only projection of peer reputations matching filter
- `octo network reputation show <peer_did>` — read-only projection of single peer reputation

Substrate per RFC-0860. EXTENDS existing `ReputationStore` trait at `crates/octo-reputation/src/store/mod.rs` §ReputationStore trait (additive trait extension per Phase 4 G22 precedent). Per-extension transport impl crates (substrate-ext-reputation-store-*) OUT OF SCOPE.

## Dependencies

- RFC-0011-h
- RFC-0860
- RFC-0011-i
- RFC-0011-j
- RFC-0011-k
- RFC-0011-l
- RFC-0011-m
- RFC-0011-n
- RFC-0011-o
- RFC-0011-p
- RFC-0011-q

## Design Goals

1. Wire `ReputationStore` substrate surface (RFC-0860) extension (list + peer_reputation methods) to the CLI for operator observability
2. Preserve per-extension crate pattern: trait in Layer B (`octo-reputation::store::ReputationStore`); concrete per-impl crates (memory + stoolap) in Layer B; per-transport adapter crates in Layer D, OUT OF SCOPE
3. Preserve Layer A frozen contract (zero Layer A change per RFC-0011-h §Layer Discipline)
4. Preserve additive trait extension pattern (Phase 4 G22 precedent) — extend trait with new methods; both existing impls must provide stub implementations; downstream code unchanged
5. Preserve BTreeMap determinism where substrate returns ordered data
6. Preserve slot 89 REUSE per Phase 6 precedent + user decision (0 NEW OctoCliError variants)
7. Preserve async-trait pattern: new methods on existing `#[async_trait]` `ReputationStore`; CLI handler uses `tokio::runtime::Handle::block_on` or async-aware dispatch path
8. 15 test vectors — tv_net10_1 through tv_net10_13 plus tv_net10_7b plus tv_net10_7c (reputation list: tv_net10_1, tv_net10_2, tv_net10_3, tv_net10_7, tv_net10_7b, tv_net10_7c, tv_net10_12, tv_net10_13; reputation show: tv_net10_4, tv_net10_5, tv_net10_6, tv_net10_8, tv_net10_9, tv_net10_10, tv_net10_11)

## Motivation

RFC-0011-h §Implementation Phases Phase 10 calls for wiring reputation store observability to the CLI. Operators need to list peer reputations (with optional AboveScore/BelowScore filter) and inspect a single peer's reputation. The substrate is PARTIAL: `ReputationStore` trait exists with 14+ async methods but lacks the `list(filter)` + `peer_reputation(did)` methods called for in RFC-0011-h G13. This RFC's companion mission (G13 `0011-h-s-a-reputation-store`) extends the trait with 2 new methods.

## Roles and Authorities

- **Operator**: invokes `octo network reputation list` + `octo network reputation show` for diagnostics
- **Peer DID**: the DID whose reputation is queried (subject of the read operation)
- **Filter**: optional AboveScore(u32) + BelowScore(u32) projection applied to the list output
- **Per-extension concrete impl crates** (Layer D): OUT OF SCOPE; trait in Layer B exposes the substrate surface for future follow-on Layer D adapter missions

## Specification

### Substrate additions

EXTEND existing `ReputationStore` trait at `crates/octo-reputation/src/store/mod.rs` §ReputationStore trait with 2 new async methods. EXTEND both `InMemoryReputationStore` (memory.rs) + `StoolapReputationStore` (stoolap.rs) with stub implementations returning empty Vec / None.

```rust
// In crates/octo-reputation/src/store/mod.rs (EXTEND existing trait)
#[async_trait::async_trait]
pub trait ReputationStore: Send + Sync {
    // ... existing 14+ methods unchanged ...

    /// List peer reputations matching the given filter (Phase 10 G13).
    /// Returns empty Vec if no peers match.
    async fn list(&self, filter: ReputationFilter) -> StoreResult<Vec<PeerReputation>>;

    /// Load reputation for a specific peer DID (Phase 10 G13).
    /// Returns None if the peer has no recorded reputation.
    async fn peer_reputation(&self, did: &RecorderDid) -> StoreResult<Option<PeerReputation>>;
}

/// `ReputationFilter` — filter enum for `ReputationStore::list`
/// (Phase 10 G13 per RFC-0011-r §Substrate Mapping Table).
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

/// `PeerReputation` — peer reputation summary projection
/// (Phase 10 G13 per RFC-0011-r §Substrate Mapping Table).
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

Layer B only. `#[non_exhaustive]` not needed on `ReputationFilter` (closed enum). `PeerReputation` is a struct (not enum) so no non_exhaustive needed.

### Subcommand Taxonomy (RFC-0011-h §Subcommand Taxonomy Phase 10)

| Subcommand                                         | Type | Layer | Companion |
| -------------------------------------------------- | ---- | ----- | --------- |
| `octo network reputation list [--filter <filter>]` | read | C     | G13       |
| `octo network reputation show <peer_did>`          | read | C     | G13       |

### Output Envelope (RFC-0011-h §Output Envelope Phase 10)

- `octo.network.reputation.list.v1` — wrapper envelope `NetworkReputationListOutput` + projection `Vec<PeerReputationProjection>`
- `octo.network.reputation.show.v1` — wrapper envelope `NetworkReputationShowOutput` + projection `Option<PeerReputationProjection>`

### Error Handling (RFC-0011-h §Error Handling Phase 10)

- Pre-companion G13 path: surfaces exit 89 `NetworkSubstrateUnavailable` (companion = "G13", detail = "")
- `list` + `peer_reputation` errors: per-variant mapping to exit 89 with distinct detail strings (per substrate StoreResult error mapping)

### Performance Targets (RFC-0011-h §Performance Targets Phase 10)

- `reputation list`: 100 ms (in-memory iteration)
- `reputation show`: 50 ms (in-memory lookup)

### Implicit Assumptions Audit (RFC-0011-h §Implicit Assumptions Audit Phase 10)

| Assumption                                         | Where Relied Upon             | Blast Radius if False                                    | Mitigation                                                  |
| -------------------------------------------------- | ----------------------------- | -------------------------------------------------------- | ----------------------------------------------------------- |
| `ReputationStore` is per-store canonical interface | existing trait + new methods  | extension breaks existing impls                          | Phase 4 G22 additive extension pattern; both impls updated  |
| `RecorderDid` is canonical DID form                | new methods + CLI parse layer | malformed peer_did rejected                              | CLI parse layer canonical validation                        |
| Async runtime available                            | new methods are async         | CLI cannot block_on async fn                             | `tokio::runtime::Handle::current().block_on(...)` wrapper   |
| Filter AboveScore + BelowScore is closed enum      | RFC-0011-r + this RFC         | unknown filter surfaces as `serde` deserialization error | `#[serde(rename_all = "lowercase")]` + filter arg as string |

### Security Considerations (RFC-0011-h §Security Considerations Phase 10)

- Read-only subcommands; no mutating surface; no confirmation flag required
- Holder DID redacted on error paths per RFC-0011-h §Security Considerations redaction invariant
- Filter threshold values are bounded u32 (no overflow risk)

### Adversarial Review (RFC-0011-h §Adversarial Review Phase 10)

- Threat 1: Operator queries malformed peer_did — mitigated by CLI parse layer canonical validation
- Threat 2: AboveScore/BelowScore filter injection via clap — mitigated by enum-based value_parser
- Threat 3: Async runtime unavailable — mitigated by `tokio::runtime::Handle::current()` guard
- Threat 4: Store returns unbounded list — bounded by substrate impl; CLI projection mirrors substrate count

### Compatibility (RFC-0011-h §Compatibility Phase 10)

- Backward compatible: NEW subcommand + NEW envelope; trait EXTENDS with 2 new methods (additive per RFC-0011-h §Substrate Discipline)
- Existing `InMemoryReputationStore` + `StoolapReputationStore` impls require stub implementations for new methods; downstream callers unchanged
- Forward compatible: new methods are part of existing `#[async_trait]`; future per-extension impl crates provide real implementations

### Test Vectors (RFC-0011-h §Test Vectors Phase 10)

| Vector      | Surface          | Coverage                                                                                                                                                                                                                                                                                   |
| ----------- | ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| tv_net10_1  | CLI parse        | `reputation list --filter all` parses cleanly with `All` filter + `None` threshold                                                                                                                                                                                                         |
| tv_net10_2  | CLI parse        | `reputation list --filter above-score --threshold 100` parses with composed args                                                                                                                                                                                                           |
| tv_net10_3  | handler rule     | `reputation list --filter above-score` WITHOUT `--threshold` rejected by handler validation (`ConfirmationRequired`) — args-construction contract test (parse-time enforcement lives in handler per Phase 7 RFC-0011-o precedent)                                                          |
| tv_net10_4  | CLI parse        | `reputation show <did:octo:0x<104-lowercase-hex>>` parses cleanly                                                                                                                                                                                                                          |
| tv_net10_5  | pastejacking     | `reputation show <mixed-case hex peer_did>` rejected by `parse_reputation_peer_did`                                                                                                                                                                                                        |
| tv_net10_6  | pastejacking     | `reputation show <uppercase-only hex peer_did>` accepted by `parse_reputation_peer_did`                                                                                                                                                                                                    |
| tv_net10_7  | handler rule     | `reputation list --filter all` dispatches to `ReputationStore::list` substrate via `InMemoryReputationStore::list` returning `Ok(empty Vec)` which projects to envelope (substrate-faithful wiring per RFC-0011-h §Substrate Discipline)                                                   |
| tv_net10_7b | handler rule     | `reputation list --filter above-score --threshold 100` dispatches to substrate via the AboveScore match arm (symmetric with tv_net10_7 to catch handler-side match arm bugs since stub returns empty regardless of filter)                                                                 |
| tv_net10_7c | handler rule     | `reputation list --filter below-score --threshold 100` dispatches to substrate via the BelowScore match arm (symmetric with tv_net10_7b for the other threshold bound direction)                                                                                                           |
| tv_net10_8  | handler rule     | `reputation show <peer_did>` dispatches to `ReputationStore::peer_reputation` substrate via `InMemoryReputationStore::peer_reputation` returning `Ok(None)` which projects to envelope (substrate-faithful wiring)                                                                         |
| tv_net10_9  | input validation | `reputation show <peer_did-without-0x-prefix>` rejected by `parse_reputation_peer_did` as `pastejacking-defense-prefix-missing`                                                                                                                                                            |
| tv_net10_10 | input validation | `reputation show <peer_did-wrong-byte-length>` rejected by `parse_reputation_peer_did` as `pastejacking-defense-length-mismatch`                                                                                                                                                           |
| tv_net10_11 | input validation | `reputation show <peer_did-with-non-hex-char>` rejected by `parse_reputation_peer_did` as `pastejacking-defense-invalid-hex`                                                                                                                                                               |
| tv_net10_12 | filter rule      | `reputation list --filter below-score` WITHOUT `--threshold` rejected by handler validation (`ConfirmationRequired`) — args-construction contract test symmetric with tv_net10_3 (above-score without threshold); parse-time enforcement lives in handler per Phase 7 RFC-0011-o precedent |
| tv_net10_13 | filter rule      | `reputation list --filter below-score --threshold 100` parses cleanly with composed args (`BelowScore(100)` filter shape — symmetric with tv_net10_2 covering above-score direction per RFC-0011-h §Subcommand Taxonomy Phase 10)                                                          |

Substrate-side coverage (Phase 10 substrate trait extension) lives in `crates/octo-reputation/src/store/memory.rs` + `stoolap.rs` + `compat/mod.rs` test modules as `tv_phase10_substrate_*` (e.g., `tv_phase10_substrate_1_reputation_filter_variants`, `tv_phase10_substrate_3_in_memory_list_returns_empty_by_default`, `tv_phase10_substrate_10_compat_list_passthrough_returns_empty`, `tv_phase10_substrate_11_compat_peer_reputation_passthrough_returns_none`). Substrate tests verify trait behavior; CLI tests verify clap parsing + handler dispatch to the trait boundary (per Phase 5 RFC-0011-m precedent).

Coverage split per Phase 5 RFC-0011-m precedent: CLI tests cover clap parsing + handler dispatch to the trait boundary; substrate tests cover trait behavior (stub impls return empty).

### Alternatives Considered

- **Single struct + JSON payload**: rejected — substrate-faithful typed projection preserves Layer B stability
- **Mutating commands**: rejected — RFC-0011-r is read-only; no confirmation flag needed
- **Direct substrate call without trait**: rejected — per-extension crate pattern requires trait for registry dispatch

### Substrate-Additions Companion Missions (RFC-0011-h §Substrate-Additions Companion Missions row G13)

- `0011-h-s-a-reputation-store` — substrate-additions prerequisite (Open → Claimed → Completed paired with CLI dispatch)

### Implementation Phases

1. Stub fill-in: substrate companion YAML `0011-h-s-a-reputation-store` (Open → Claimed) with full type signatures + AC
2. Substrate slice: EXTEND `ReputationStore` trait + add `ReputationFilter` enum + add `PeerReputation` struct + EXTEND both `InMemoryReputationStore` + `StoolapReputationStore` with stub implementations
3. Paired-YAML Claimed transition for substrate companion
4. CLI dispatch slice atop substrate: `NetworkAction::Reputation` + nested `NetworkReputationAction` + `ReputationListArgs` + `ReputationShowArgs` + 2 envelopes + 2 handlers + `reputation_store_registry` helper + 6 test vectors
5. CLI mission YAML CREATED Completed + paired substrate companion YAML Completed paired
6. Closure artifacts: audit doc + memory card + MEMORY.md index entry

> Phase 10 follows the Phase 5 RFC-0011-m 5-commit pattern (stub fill-in → substrate slice → YAML Claimed → CLI dispatch → YAMLs Completed) verified at `next 8e7c5cec`, `24bfec96`, `fcb58331`, `346f10cc`, `97955c00`.

### Key Files to Modify

- `rfcs/draft/process/0011-r-oct-cli-network-phase-10.md` — NEW RFC Draft (this file)
- `missions/open/0011-h-s-a-reputation-store.md` — substrate companion YAML stub fill-in
- `crates/octo-reputation/src/store/mod.rs` — EXTEND existing trait with `list` + `peer_reputation` methods + add `ReputationFilter` enum + `PeerReputation` struct
- `crates/octo-reputation/src/store/memory.rs` — EXTEND `InMemoryReputationStore` with stub implementations
- `crates/octo-reputation/src/store/stoolap.rs` — EXTEND `StoolapReputationStore` with stub implementations
- `crates/octo-cli/src/commands/network.rs` — CLI dispatch slice (Args + envelopes + handlers + test vectors)
- `missions/open/0011-h-network-reputation.md` — NEW CLI mission YAML (Created Completed paired)

### Future Work

- Per-extension concrete impls (per-transport reputation store adapters) — follow-on Layer D missions, OUT OF SCOPE for this trait-only phase
- Real reputation aggregation logic — stub returns empty; real aggregation in follow-on Layer D adapter mission
- Pagination for `list` output — OUT OF SCOPE for this trait-only phase

## Rationale

RFC-0011-r closes the G13 deferred stub per "everything included, no deferral" directive. The substrate extension (2 new async trait methods on existing `ReputationStore`) lands FIRST per [[no-phantom-mission-pointers]] pairing invariant, then CLI dispatch atop it. The additive trait extension pattern (Phase 4 G22 precedent) preserves backward compatibility: existing callers unchanged; new callers can use new methods.

## Version History

| Version | Date       | Notes                                                                                                                                                                                                                                                                                                                                                            |
| ------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1.0  | 2026-09-20 | Initial draft; pending R1 of 5-len DRY CLOSURE cycle                                                                                                                                                                                                                                                                                                             |
| v0.2.0  | 2026-09-21 | Promoted Draft to Accepted. RFC-0011-r year-stable per Layer B substrate convention. Retroactive multi-round DRY CLOSED at `next 6588aaf2` after 4 rounds (R1 fail → R1.5 fix → R2 fail → R2.5 fix → R3 fail → R3.5 fix → R4 zero = GATE GREEN). File moved from `rfcs/draft/process/` to `rfcs/accepted/process/` per accepted RFC-0011-v directory convention. |

## Cross-references

- **RFC-0011-h** §Substrate-Additions Companion Missions row G13 — canonical substrate spec for `ReputationStore` extension
- **RFC-0011-h** §Error Handling row 89 — slot 89 `NetworkSubstrateUnavailable` REUSE
- **RFC-0011-h** §Exit Codes slot 89 — REUSE per Phase 10
- **RFC-0011-h** §Performance Targets Phase 10 rows — 100 ms list + 50 ms show
- **RFC-0011-h** §Subcommand Taxonomy Phase 10 rows — `reputation list` + `reputation show`
- **RFC-0011-h** §Output Envelope Phase 10 rows — `NetworkReputationListOutput` + `NetworkReputationShowOutput`
- **RFC-0011-h** §Implicit Assumptions Audit Phase 10 rows
- **RFC-0011-h** §Security Considerations Phase 10 rows
- **RFC-0011-h** §Adversarial Review Phase 10 rows — 4 threats
- **RFC-0860**
- **RFC-0011-o** Phase 7 — prior phase (slash-bridge)
- **RFC-0011-p** Phase 8 — prior phase (router)
- **RFC-0011-q** Phase 9 — prior phase (specialized-node)
