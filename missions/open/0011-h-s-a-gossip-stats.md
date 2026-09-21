# 0011-h-s-a-gossip-stats — Substrate additions for Gossip::stats() impl (RFC-0855 §8.2)

## Status

Completed (2026-09-20) — Substrate slice landed at `next c07ad375` (Gossip + GossipStats additive types + stats() impl + 4 substrate tests tv_phase12_substrate_1..4 on existing mon/gossip.rs module per Phase 5 RFC-0011-m EXTEND-existing precedent). CLI dispatch slice at `next b0cc47cc`. RFC draft at `next 59bb39b4`. Stub fill-in at `next 8244e969`. Paired-YAML Claimed transition at `next 2a12bef4`. CLI mission YAML `0011-h-network-gossip` CREATED at CLI dispatch slice time per user decision (paired with this transition).

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G15 + RFC-0011-t Phase 12 G15 gossip-stats amendment Draft.

## Summary

Substrate-side anti-entropy counter + gossip stats projection. Required by `octo network gossip --stats` per RFC-0011-t Phase 12 §Subcommand Taxonomy.

### Substrate additions target

EXTENDS existing `crates/octo-network/src/mon/gossip.rs` (per Phase 5 RFC-0011-m precedent additive type extension; original stub noted `mon/gossip.rs (NEW)` but the substrate-faithful path is EXTEND the existing module which already houses `MissionGossipScope` + `MissionPropagationClass` + `MissionGossipMessage` — NOT create a new sibling module) with NEW additive types:

```rust
// crates/octo-network/src/mon/gossip.rs (EXTEND existing module)
use crate::mon::mission_id::MissionId;

/// In-memory snapshot of the gossip protocol state for a mission scope.
///
/// Phase 12 G15 per RFC-0011-t §Substrate Mapping Table. Operates
/// on the in-memory snapshot of the gossip state; live gossip
/// adapter OUT OF SCOPE for Phase 12. Per-extension impl crates
/// (Layer D) provide real gossip adapters in follow-on missions.
/// BTreeMap-based deterministic iteration ordering preserved
/// per RFC-0011-h §Output Envelope determinism.
#[derive(Clone, Debug)]
pub struct Gossip {
    mission_id: MissionId,
    peers_reachable: u64,
    messages_sent: u64,
    messages_received: u64,
    messages_dropped: u64,
    anti_entropy_rounds: u64,
    last_sync_epoch: u64,
}

/// Aggregate gossip stats output (read-only projection).
///
/// Phase 12 G15 per RFC-0011-t §Substrate Mapping Table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct GossipStats {
    pub mission_id_hex: String,
    pub peers_reachable: u64,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub messages_dropped: u64,
    pub anti_entropy_rounds: u64,
    pub last_sync_epoch: u64,
}

impl Gossip {
    /// Construct a new `Gossip` snapshot from explicit counters.
    pub fn new(
        mission_id: MissionId,
        peers_reachable: u64,
        messages_sent: u64,
        messages_received: u64,
        messages_dropped: u64,
        anti_entropy_rounds: u64,
        last_sync_epoch: u64,
    ) -> Self {
        Self {
            mission_id,
            peers_reachable,
            messages_sent,
            messages_received,
            messages_dropped,
            anti_entropy_rounds,
            last_sync_epoch,
        }
    }

    /// Read the gossip stats as a substrate-faithful projection.
    pub fn stats(&self) -> GossipStats {
        GossipStats {
            mission_id_hex: hex::encode(self.mission_id.to_canonical_bytes()),
            peers_reachable: self.peers_reachable,
            messages_sent: self.messages_sent,
            messages_received: self.messages_received,
            messages_dropped: self.messages_dropped,
            anti_entropy_rounds: self.anti_entropy_rounds,
            last_sync_epoch: self.last_sync_epoch,
        }
    }
}
```

EXTENDS the existing `mon/gossip.rs` module with NEW additive types. No new modules; no new files. REUSES existing `MissionId` from `mon/mission_id.rs` and `Serialize` + `JsonSchema` imports already in scope. Zero regression on existing `MissionGossipScope` + `MissionPropagationClass` + `MissionGossipMessage` types.

## Acceptance Criteria

- [ ] `Gossip` struct lands at `crates/octo-network/src/mon/gossip.rs` per RFC-0011-h §Substrate-Additions row G15 + RFC-0011-t Phase 12 §Substrate Mapping Table
- [ ] `GossipStats` struct lands with all 7 fields (mission_id_hex + peers_reachable + messages_sent + messages_received + messages_dropped + anti_entropy_rounds + last_sync_epoch)
- [ ] `Gossip::new(...)` constructor lands with 7 explicit counter params
- [ ] `Gossip::stats(&self) -> GossipStats` method lands
- [ ] Existing `MissionGossipScope` + `MissionPropagationClass` + `MissionGossipMessage` types preserved (zero regression)
- [ ] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [ ] `cargo test -p octo-network --lib` green (≥3 unit tests added; zero regression)
- [ ] Layer discipline preserved (Layer B only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [ ] ≥3 unit tests + ≥1 integration test (substrate-faithful boundary tests pin stats projection determinism + counter arithmetic + mission_id hex encoding)

## Dependencies

- RFC-0011-h Accepted (RFC-0011-h must be Accepted before this mission lands per RFC-0011-h §Substrate-Additions Companion Missions)
- RFC-0011-t Phase 12 gossip-stats amendment Draft
- Phase 12 G15 RFC draft at `next 59bb39b4`
- Existing `MissionGossipScope` + `MissionPropagationClass` + `MissionGossipMessage` types at `crates/octo-network/src/mon/gossip.rs` (EXTEND, not NEW)
- Existing `MissionId` at `crates/octo-network/src/mon/mission_id.rs` (REUSE, not NEW)

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-gossip` covers that surface; CREATED at CLI dispatch slice time per user decision)
- Wire format versioning (deferred to RFC-0011-h §Future Work items F8 + F9)
- Per-extension transport impl (substrate-ext-gossip-source-* Layer D follow-on missions, OUT OF SCOPE for this additive-type-only phase)
- Live gossip adapter (Phase 12 substrate operates on the in-memory snapshot only; real adapter in follow-on Layer D adapter mission)
- Real anti-entropy counter (Phase 12 returns stub 0; real counter in follow-on Layer D adapter mission)

## Notes

Stub originally filed 2026-09-18 per [[no-phantom-mission-pointers]]. Stub note pinned path `crates/octo-network/src/mon/gossip.rs (NEW)` — substrate-faithfulness audit per Phase 5 RFC-0011-m precedent overrides: EXTEND existing `mon/gossip.rs` instead of creating a new sibling module. Full AC + scope land in Phase 12 stub fill-in commit at `next PENDING` per the Phase 5 RFC-0011-m 5-commit pattern. Phase 12 follows the Phase 5 5-commit pattern (stub fill-in → substrate slice → YAML Claimed → CLI dispatch → YAMLs Completed) verified at `next 8e7c5cec`, `24bfec96`, `fcb58331`, `346f10cc`, `97955c00`. Slot 89 `NetworkSubstrateUnavailable` REUSE per Phase 6 precedent (0 NEW OctoCliError variants). BTreeMap-based deterministic iteration ordering preserved per RFC-0011-h §Output Envelope determinism. `Gossip::stats()` operates on in-memory snapshot only; live gossip adapter in follow-on Layer D adapter mission per per-extension crate pattern.
