# RFC-0011-t: `octo network` Phase 12 — Gossip Stats (G15)

## Status

Accepted (2026-09-21) — RFC-0011-t promoted from Draft at `next f2664770` per the retroactive multi-round DRY CLOSURE gate that achieved R2 GATE GREEN (zero BLOCKER / MAJOR / MINOR findings; one residual NIT noted but non-gating). R1 verdict was 1 PASS + 4 FAIL with 11 BLOCKER/MAJOR + 4 MINOR + 1 NIT findings; R1.5 fix sweep at `next f2664770` closed all 16 findings. RFC-0011-t lands RFC-0011-h §Implementation Phases Phase 12. One subcommand wires gossip stats via the existing `mon/gossip.rs` substrate (EXTEND the existing module per Phase 5 RFC-0011-m precedent). Companion stub mission G15 + 0 NEW OctoCliError variants (REUSES slot 89 `NetworkSubstrateUnavailable` per RFC-0011-h §Error Handling row 89) + 1 output envelope + 9 test vectors (3 CLI dispatch + 4 substrate layer + 2 envelope / JSON dispatch expansion per R1.5 fix sweep).

> **Amendment chain:** Twelfth amendment in the `0011-h-multiphase-rollout-plan` (see `docs/plans/2026-09-20-0011-h-multiphase-rollout-plan.md`, gitignored scratchpad per [[docs-plans-scratchpad]]). Phase 1 = RFC-0011-i. Phase 2 = RFC-0011-j. Phase 3 = RFC-0011-k. Phase 4 = RFC-0011-l. Phase 5 = RFC-0011-m. Phase 6 = RFC-0011-n. Phase 7 = RFC-0011-o. Phase 8 = RFC-0011-p. Phase 9 = RFC-0011-q. Phase 10 = RFC-0011-r. Phase 11 = RFC-0011-s. Phase 12 = RFC-0011-t (this RFC).

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu

## Summary

RFC-0011-t lands the **gossip stats** slice of RFC-0011-h §Implementation Phases. One CLI subcommand reads gossip protocol stats (peers_reachable + messages_sent + messages_received + messages_dropped + anti_entropy_rounds + last_sync_epoch) via the existing `mon/gossip.rs` substrate (EXTEND the existing module per Phase 5 RFC-0011-m precedent; new additive types `Gossip` + `GossipStats` + `stats()` impl land on the existing module).

- `octo network gossip --stats [--format ascii|json] [--json]` — read-only projection of gossip stats

Substrate per RFC-0855 §8.2 (gossip protocol). EXTENDS existing `mon/gossip.rs` with `Gossip` + `GossipStats` + `stats()` per Phase 5 RFC-0011-m precedent (additive types on existing module; no NEW sibling module).

## Dependencies

- RFC-0011-h
- RFC-0855 §8.2
- RFC-0011-i
- RFC-0011-j
- RFC-0011-k
- RFC-0011-l
- RFC-0011-m
- RFC-0011-n
- RFC-0011-o
- RFC-0011-p
- RFC-0011-q
- RFC-0011-r
- RFC-0011-s

## Design Goals

1. Wire `Gossip::stats()` extension (RFC-0855 §8.2) to the CLI for operator gossip-state inspection
2. Preserve per-extension crate pattern: existing gossip substrate in Layer B (`octo-network::mon::gossip`); concrete per-gossip-source adapter in Layer D, OUT OF SCOPE
3. Preserve Layer A frozen contract (zero Layer A change per RFC-0011-h §Layer Discipline)
4. Preserve additive type-extension pattern (Phase 5 RFC-0011-m precedent) — EXTEND existing module with new types; downstream code unchanged
5. REUSE existing `MissionGossipScope` + `MissionPropagationClass` + `MissionGossipMessage` types from `mon/gossip.rs` (zero regression)
6. Preserve scalar field-order determinism (counters are plain `u64`; no HashMap/BTreeMap iteration; zero collection dependencies) per RFC-0011-h §Output Envelope determinism
7. Preserve slot 89 REUSE per Phase 6 precedent + user decision (0 NEW OctoCliError variants)
8. 9 test vectors — 3 CLI dispatch (`tv_net12_1` through `tv_net12_3`) + 4 substrate layer (`tv_phase12_substrate_1` through `tv_phase12_substrate_4`) + 2 envelope / JSON dispatch expansion (`tv_net12_4` through `tv_net12_5`)
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
/// Scalar field-order determinism preserved per RFC-0011-h
/// §Output Envelope determinism (no HashMap/BTreeMap iteration;
/// zero collection dependencies).
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    ///
    /// `anti_entropy_rounds` is plumbed through the substrate
    /// API but caller-stubbed at 0 in this phase per RFC-0011-t
    /// §Substrate-faithfulness. The real anti-entropy counter
    /// (RFC-0855 §8.2) is OUT OF SCOPE for Phase 12 and lands
    /// in a follow-on Layer D adapter mission; until then CLI
    /// callers MUST pass `0` for this slot.
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
    /// Scalar field-order determinism preserved per RFC-0011-h
    /// §Output Envelope determinism.
    pub fn stats(&self) -> GossipStats;
}
```

The `stats()` impl returns the in-memory snapshot. Real anti-entropy counter is OUT OF SCOPE for Phase 12 (substrate-extension-only phase); the counter is plumbed through `Gossip::new` but caller-stubbed at 0 until Layer D adapter missions land.

### Output envelope

```rust
#[derive(Serialize, Deserialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkGossipStatsOutput {
    /// Format that was applied (`ascii` / `json`).
    pub format: String,
    /// Mission ID hex echoed (canonicalized via
    /// `MissionId::to_canonical_bytes`).
    pub mission_id_hex: String,
    /// Counter projections (peers_reachable +
    /// messages_sent + messages_received +
    /// messages_dropped + anti_entropy_rounds +
    /// last_sync_epoch).
    pub peers_reachable: u64,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub messages_dropped: u64,
    pub anti_entropy_rounds: u64,
    pub last_sync_epoch: u64,
}
```

### Test vectors (9)

- `tv_net12_1`: gossip --stats default format (ascii) parses cleanly
- `tv_net12_2`: gossip --stats --format json parses cleanly
- `tv_net12_3`: gossip --stats with --json flag parses cleanly
- `tv_net12_4`: handler dispatch with default format returns `Ok` and rendered envelope contains all 6 counter fields + mission_id_hex + format label (R1.5 fix: dispatch test inspects substrate body contract per Phase 10 + Phase 11 R3 MAJOR-1 lesson)
- `tv_net12_5`: `NetworkGossipStatsOutput` JSON envelope serde round-trip (R1.5 fix: envelope `Serialize + Deserialize` derives enable round-trip assertion)
- `tv_phase12_substrate_1`: `Gossip::new` constructs with explicit counters (mission_id + 6 `u64` fields)
- `tv_phase12_substrate_2`: `Gossip::stats()` projects all 6 counter fields verbatim from struct
- `tv_phase12_substrate_3`: `Gossip::stats()` deterministic across calls (R1.5 fix: same input → same output, no HashMap iteration)
- `tv_phase12_substrate_4`: `Gossip::stats()` distinct per mission (different `mission_id` → different `mission_id_hex`; counters identical for same input counters)

## Exit codes

Slot 89 `NetworkSubstrateUnavailable` REUSE per Phase 6 precedent + user decision. 0 NEW OctoCliError variants for Phase 12.

## Layer discipline

- **Layer A frozen preserved**: zero change to `octo-governance-core`, `octo-audit-core`, `octo-settlement-core`, `octo-vault-core`, `octo-wallet-core`.
- **Layer B substrate EXTENDED**: `mon/gossip.rs` gains `Gossip` + `GossipStats` + `stats()`. Additive types on existing module per Phase 5 RFC-0011-m precedent; downstream code unchanged.
- **Layer C CLI dispatch**: `commands/network.rs` extended with `NetworkAction::Gossip { action: NetworkGossipAction }` clap variant; `NetworkGossipAction::Stats(GossipStatsArgs)`; output envelope; handler; 5 test vectors (3 dispatch + 2 envelope / JSON dispatch expansion). 4 substrate layer test vectors land in `mon/gossip.rs` test module.

## Substrate-faithfulness

The `stats()` method operates on the in-memory snapshot of the `Gossip` struct (no live gossip adapter). Per-extension Layer D adapter crates (live gossip adapters) OUT OF SCOPE for Phase 12 per RFC-0011-h §Future Work items F8+F9. The stub YAML pinned path `crates/octo-network/src/mon/gossip.rs (NEW)` is overridden per Phase 5 RFC-0011-m precedent: EXTEND existing `mon/gossip.rs` (which already houses `MissionGossipScope` + `MissionPropagationClass` + `MissionGossipMessage`) instead of creating a new sibling module. Scalar field-order determinism preserved per RFC-0011-h §Output Envelope determinism (no HashMap/BTreeMap iteration; zero collection dependencies). `anti_entropy_rounds` is plumbed through `Gossip::new` but stubbed at 0 by callers (the real RFC-0855 §8.2 anti-entropy counter is OUT OF SCOPE for Phase 12 and lands in a follow-on Layer D adapter mission).

The CLI handler invokes the substrate method unconditionally — no registry gate (trait dispatch is the universal code path per Phase 10 RFC-0011-r R2.5 substrate-faithfulness precedent). The R1.5 fix removed the `gossip_stats_registry` always-false early-return (it was a residual additive-type-only phase artifact that violated the Phase 10/11 trait-dispatch precedent); the handler now exercises the substrate `Gossip::stats()` path end-to-end.

## Companion stub missions

G15 `0011-h-s-a-gossip-stats` (Open → Claimed → Completed paired with CLI dispatch slice).

## Out of Scope

- Live gossip adapter (Layer D; follow-on per-extension crate missions)
- Per-extension impl crates (substrate-ext-gossip-source-*) OUT OF SCOPE for Phase 12
- Wire format versioning (RFC-0011-h §Future Work items F8+F9)
- Real anti-entropy counter (substrate plumbs `anti_entropy_rounds` slot; caller stubs at 0; Layer D adapter missions in future)

## History

- 2026-09-20 — Draft (this RFC)
- 2026-09-21 — Promoted Draft → Accepted at `next f2664770` per retroactive multi-round DRY CLOSURE (R1 + R2 zero rounds gate pair). R1 verdict 1 PASS + 4 FAIL with 11 BLOCKER/MAJOR + 4 MINOR + 1 NIT; R1.5 fix sweep closed all 16 findings (registry gate removed, BTreeMap comment dropped, `Deserialize` derive added to `GossipStats` + `NetworkGossipStatsOutput`, `tv_net12_4` + `tv_net12_5` tests added, RFC cite hygiene sweep stripped status parentheticals + Governing RFC label + `.gitignore` line 46 ref); R2 achieved GATE GREEN with zero BLOCKER/MAJOR/MINOR.
- 2026-09-21 — R1.5 fix sweep: removed `gossip_stats_registry` always-false early-return gate + `gossip_stats_registry` fn (handler now invokes substrate unconditionally per Phase 10 RFC-0011-r R2.5 substrate-faithfulness precedent); dropped misleading "BTreeMap-based deterministic iteration" comment from `Gossip` struct doc + RFC code block (struct has zero collection fields; replaced with accurate "scalar field-order determinism" phrasing); `GossipStats` struct gained `Deserialize` derive for JSON envelope round-trip test; added doc comment to `Gossip::new` documenting the caller-stubbed-zero contract for `anti_entropy_rounds`; stripped status parentheticals from Dependencies + amendment chain; removed "Governing RFC:" label from Dependencies; removed `.gitignore` line 46 file:line ref from amendment chain (replaced with `[[docs-plans-scratchpad]]` memory cross-ref per `[[no-line-refs-anywhere]]`); expanded test vector count from 3 to 9 (added tv_phase12_substrate_1/2/3/4 substrate layer vectors + tv_net12_4 dispatch body-contract inspection + tv_net12_5 envelope serde round-trip); added explicit `anti_entropy_rounds` plumbed-but-stubbed-at-0 claim to §Substrate-faithfulness; fixed tv_net12_3 RFC description (was incoherent: described a non-existent "gossip without --stats flag" rejection scenario; corrected to match actual test "gossip --stats with --json flag parses cleanly"); replaced "this version" with "this RFC" in History line per Phase 11 R3 NIT-3 lesson; `GossipStats` RFC code block updated to drop `schemars::JsonSchema` (octo-network does not depend on schemars); output envelope RFC code block expanded to document inline counter field projection pattern (handler destructures stats.mission_id_hex etc. into envelope fields).
