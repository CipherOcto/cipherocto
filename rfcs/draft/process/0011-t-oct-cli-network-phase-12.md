# RFC-0011-t: `octo network` Phase 12 — Gossip Stats (G15)

## Status

Draft (2026-09-20) — RFC-0011-t lands RFC-0011-h §Implementation Phases Phase 12. One subcommand wires gossip stats via the existing `mon/gossip.rs` substrate (EXTEND the existing module per Phase 5 RFC-0011-m precedent). Companion stub mission G15 + 0 NEW OctoCliError variants (REUSES slot 89 `NetworkSubstrateUnavailable` per RFC-0011-h §Error Handling row 89) + 1 output envelope + 3 test vectors.

> **Amendment chain:** Twelfth amendment in the `0011-h-multiphase-rollout-plan` (see `docs/plans/2026-09-20-0011-h-multiphase-rollout-plan.md`, gitignored scratchpad per `.gitignore` line 46). Phase 1 = RFC-0011-i (DRY CLOSED). Phase 2 = RFC-0011-j (DRY CLOSED). Phase 3 = RFC-0011-k (DRY CLOSED). Phase 4 = RFC-0011-l (DRY CLOSED). Phase 5 = RFC-0011-m (DRY CLOSED). Phase 6 = RFC-0011-n (DRY CLOSED). Phase 7 = RFC-0011-o (DRY CLOSED + Accepted). Phase 8 = RFC-0011-p (IMPLEMENTATION CLOSED). Phase 9 = RFC-0011-q (IMPLEMENTATION CLOSED). Phase 10 = RFC-0011-r (IMPLEMENTATION CLOSED). Phase 11 = RFC-0011-s (IMPLEMENTATION CLOSED). Phase 12 = RFC-0011-t (this RFC).

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu

## Summary

RFC-0011-t lands the **gossip stats** slice of RFC-0011-h §Implementation Phases. One CLI subcommand reads gossip protocol stats (peers_reachable + messages_sent + messages_received + messages_dropped + anti_entropy_rounds + last_sync_epoch) via the existing `mon/gossip.rs` substrate (EXTEND the existing module per Phase 5 RFC-0011-m precedent; new additive types `Gossip` + `GossipStats` + `stats()` impl land on the existing module).

- `octo network gossip --stats [--format ascii|json] [--json]` — read-only projection of gossip stats

Substrate per RFC-0855 §8.2 (gossip protocol). EXTENDS existing `mon/gossip.rs` with `Gossip` + `GossipStats` + `stats()` per Phase 5 RFC-0011-m precedent (additive types on existing module; no NEW sibling module).

## Dependencies

- RFC-0011-h Accepted
- RFC-0855 (Governing RFC; §8.2 Gossip protocol)
- RFC-0011-i Phase 1 IMPLEMENTATION CLOSED
- RFC-0011-j Phase 2 IMPLEMENTATION CLOSED
- RFC-0011-k Phase 3 IMPLEMENTATION CLOSED
- RFC-0011-l Phase 4 IMPLEMENTATION CLOSED
- RFC-0011-m Phase 5 IMPLEMENTATION CLOSED
- RFC-0011-n Phase 6 IMPLEMENTATION CLOSED
- RFC-0011-o Phase 7 IMPLEMENTATION CLOSED + Accepted
- RFC-0011-p Phase 8 IMPLEMENTATION CLOSED
- RFC-0011-q Phase 9 IMPLEMENTATION CLOSED
- RFC-0011-r Phase 10 IMPLEMENTATION CLOSED
- RFC-0011-s Phase 11 IMPLEMENTATION CLOSED

## Design Goals

1. Wire `Gossip::stats()` extension (RFC-0855 §8.2) to the CLI for operator gossip-state inspection
2. Preserve per-extension crate pattern: existing gossip substrate in Layer B (`octo-network::mon::gossip`); concrete per-gossip-source adapter in Layer D, OUT OF SCOPE
3. Preserve Layer A frozen contract (zero Layer A change per RFC-0011-h §Layer Discipline)
4. Preserve additive type-extension pattern (Phase 5 RFC-0011-m precedent) — EXTEND existing module with new types; downstream code unchanged
5. REUSE existing `MissionGossipScope` + `MissionPropagationClass` + `MissionGossipMessage` types from `mon/gossip.rs` (zero regression)
6. Preserve BTreeMap determinism where substrate returns ordered data
7. Preserve slot 89 REUSE per Phase 6 precedent + user decision (0 NEW OctoCliError variants)
8. 3 test vectors — tv_net12_1 through tv_net12_3
9. Substrate-faithfulness audit: EXTEND existing `mon/gossip.rs` per Phase 5 RFC-0011-m precedent (additive types on existing module, NOT NEW sibling module)

## Motivation

RFC-0011-h §Implementation Phases Phase 12 (G15) calls for wiring gossip stats to the CLI. Operators need to inspect gossip protocol state (peers reachable, messages sent/received/dropped, anti-entropy rounds). The substrate is PARTIAL: `mon/gossip.rs` exists with `MissionGossipScope` + `MissionPropagationClass` + `MissionGossipMessage` per RFC-0855 §8.2 but lacks a `Gossip` struct + `stats()` method. This RFC's companion mission (G15 `0011-h-s-a-gossip-stats`) extends the existing module with `Gossip` + `GossipStats` + `stats()`.

## Roles and Authorities

- **Operator**: invokes `octo network gossip --stats` for diagnostic gossip-state output
- **Gossip protocol state**: the in-memory snapshot of gossip counters (subject of the read operation)
- **Anti-entropy counter**: stub returns 0 (real counter in follow-on Layer D adapter per per-extension crate pattern)
- **Per-extension concrete impl crates** (Layer D): OUT OF SCOPE; existing gossip substrate in Layer B exposes the stats surface for future follow-on Layer D adapter missions

## Detailed Design

### CLI surface

```
octo network gossip --stats [--format ascii|json] [--json]
```

- `gossip --stats` — read-only rendering of gossip protocol counters
- `--format ascii` (default) / `--format json` — human-readable ASCII or machine-readable JSON
- `--json` — force JSON envelope output

### Substrate extension (Layer B)

EXTENDS `mon/gossip.rs` (existing module) with:

```rust
/// In-memory snapshot of the gossip protocol state for a mission scope.
///
/// Phase 12 G15 per RFC-0011-t §Substrate Mapping Table. Operates
/// on the in-memory snapshot of the gossip state; live gossip
/// adapter OUT OF SCOPE for Phase 12. Per-extension impl crates
/// (Layer D) provide real gossip adapters in follow-on missions.
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
    ) -> Self;

    /// Read the gossip stats as a substrate-faithful projection.
    /// BTreeMap-based deterministic iteration ordering preserved
    /// per RFC-0011-h §Output Envelope determinism.
    pub fn stats(&self) -> GossipStats;
}
```

The `stats()` impl returns the in-memory snapshot. Real anti-entropy counter is OUT OF SCOPE for Phase 12 (substrate-extension-only phase); the counter is plumbed but stubbed at 0 until Layer D adapter missions land.

### Output envelope

```rust
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkGossipStatsOutput {
    /// Format that was applied (`ascii` / `json`).
    pub format: String,
    /// Rendered gossip stats body (ASCII table or JSON object).
    pub stats: GossipStats,
}
```

### Test vectors (3)

- `tv_net12_1`: gossip --stats default format (ascii) parses cleanly
- `tv_net12_2`: gossip --stats --format json parses cleanly
- `tv_net12_3`: gossip (without --stats flag) is rejected pre-dispatch (clap arg requirement)

## Exit codes

Slot 89 `NetworkSubstrateUnavailable` REUSE per Phase 6 precedent + user decision. 0 NEW OctoCliError variants for Phase 12.

## Layer discipline

- **Layer A frozen preserved**: zero change to `octo-governance-core`, `octo-audit-core`, `octo-settlement-core`, `octo-vault-core`, `octo-wallet-core`.
- **Layer B substrate EXTENDED**: `mon/gossip.rs` gains `Gossip` + `GossipStats` + `stats()`. Additive types on existing module per Phase 5 RFC-0011-m precedent; downstream code unchanged.
- **Layer C CLI dispatch**: `commands/network.rs` extended with `NetworkAction::Gossip { action: NetworkGossipAction }` clap variant; `NetworkGossipAction::Stats(GossipStatsArgs)`; output envelope; handler; 3 test vectors.

## Substrate-faithfulness

The `stats()` method operates on the in-memory snapshot of the `Gossip` struct (no live gossip adapter). Per-extension Layer D adapter crates (live gossip adapters) OUT OF SCOPE for Phase 12 per RFC-0011-h §Future Work items F8+F9. The stub YAML pinned path `crates/octo-network/src/mon/gossip.rs (NEW)` is overridden per Phase 5 RFC-0011-m precedent: EXTEND existing `mon/gossip.rs` (which already houses `MissionGossipScope` + `MissionPropagationClass` + `MissionGossipMessage`) instead of creating a new sibling module. BTreeMap-based deterministic iteration ordering preserved per RFC-0011-h §Output Envelope determinism.

## Companion stub missions

G15 `0011-h-s-a-gossip-stats` (Open → Claimed → Completed paired with CLI dispatch slice).

## Out of Scope

- Live gossip adapter (Layer D; follow-on per-extension crate missions)
- Per-extension impl crates (substrate-ext-gossip-source-*) OUT OF SCOPE for Phase 12
- Wire format versioning (RFC-0011-h §Future Work items F8+F9)
- Real anti-entropy counter (stub returns 0; Layer D adapter missions in future)

## History

- 2026-09-20 — Draft (this version)
