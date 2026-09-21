# RFC-0011-v: `octo network` Phase 14 — Heartbeat Probe (G17) + Stale-Stub Sweep

## Status

Draft (2026-09-20) — RFC-0011-v lands RFC-0011-h §Implementation Phases Phase 14. ONE CLI subcommand (`heartbeat probe`) wires heartbeat probing via ONE NEW substrate module (`mon/heartbeat.rs`). Companion stub mission G17 + 0 NEW OctoCliError variants (REUSES slot 89 `NetworkSubstrateUnavailable` per RFC-0011-h §Error Handling row 89) + 1 output envelope + 3 test vectors. The final RFC-0011-h §Substrate-Additions Companion Missions G-row G17 closes the 9-row deferred stub sweep opened at Phase 7 (RFC-0011-o).

> **Amendment chain:** Fourteenth + final amendment in the `0011-h-multiphase-rollout-plan` (see `docs/plans/2026-09-20-0011-h-multiphase-rollout-plan.md`, gitignored scratchpad per `.gitignore` line 46). Phase 1 = RFC-0011-i (DRY CLOSED). Phase 2 = RFC-0011-j (DRY CLOSED). Phase 3 = RFC-0011-k (DRY CLOSED). Phase 4 = RFC-0011-l (DRY CLOSED). Phase 5 = RFC-0011-m (DRY CLOSED). Phase 6 = RFC-0011-n (DRY CLOSED). Phase 7 = RFC-0011-o (DRY CLOSED + Accepted). Phase 8 = RFC-0011-p (IMPLEMENTATION CLOSED). Phase 9 = RFC-0011-q (IMPLEMENTATION CLOSED). Phase 10 = RFC-0011-r (IMPLEMENTATION CLOSED). Phase 11 = RFC-0011-s (IMPLEMENTATION CLOSED). Phase 12 = RFC-0011-t (IMPLEMENTATION CLOSED). Phase 13 = RFC-0011-u (IMPLEMENTATION CLOSED). Phase 14 = RFC-0011-v (this RFC).

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu

## Summary

RFC-0011-v lands the **heartbeat probe** slice of RFC-0011-h §Implementation Phases. ONE CLI subcommand wires heartbeat probing (read-only peer reachability diagnostic) via ONE NEW substrate module in `mon/`:

- `octo network heartbeat probe <peer_did> [--timeout-ms <N>]` — read-only heartbeat probe returning `Reachable` with `rtt_ms` or `Unreachable` with reason or `Timeout` after `--timeout-ms` (default 5000ms)

Substrate per RFC-0855 §Wire Format heartbeat probing. ONE NEW substrate module (`mon/heartbeat.rs`) per Phase 7 RFC-0011-o NEW module precedent for SlashBridge trait substrate.

## Dependencies

- RFC-0011-h Accepted
- RFC-0855 (Governing RFC; §Wire Format)
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
- RFC-0011-t Phase 12 IMPLEMENTATION CLOSED
- RFC-0011-u Phase 13 IMPLEMENTATION CLOSED

## Design Goals

1. Wire `Heartbeat::probe()` to the CLI for operator peer reachability diagnostic (read-only)
2. Preserve per-extension crate pattern: trait in Layer B (`octo-network::mon::heartbeat`); concrete per-transport heartbeat adapter in Layer D, OUT OF SCOPE
3. Preserve Layer A frozen contract (zero Layer A change per RFC-0011-h §Layer Discipline)
4. THREE NEW types (`Heartbeat` + `HeartbeatProbeResult` enum + `UnreachableReason` enum); additive on NEW module, NOT existing sibling
5. Preserve BTreeMap determinism where substrate returns ordered data
6. Preserve slot 89 REUSE per Phase 6 precedent + user decision (0 NEW OctoCliError variants)
7. 3 test vectors — tv_net14_1 through tv_net14_3
8. `parse_32_byte_hex` pastejacking defense: every hex arg uses the shared helper per Phase 5 RFC-0011-m precedent
9. clap u16 overflow pre-dispatch rejection (Phase 5 precedent for `--timeout-ms`)

## Motivation

RFC-0011-h §Implementation Phases Phase 14 (G17) calls for wiring heartbeat probing to the CLI. Operators need a lightweight peer reachability diagnostic that does NOT require a full envelope round-trip. The substrate is MISSING: no `heartbeat` module exists in `mon/` (only `liveness.rs` for a different concern — election-window liveness vs. transport-level heartbeat probe). This RFC's companion mission (`G17` `0011-h-s-a-heartbeat-probe`) creates ONE NEW module in `mon/` with substrate types.

## Roles and Authorities

- **Operator**: invokes `octo network heartbeat probe <peer_did>` for reachability diagnostic
- **Peer DID**: the target peer's decentralized identifier (`did:octo:...` string)
- **Per-extension concrete impl crates** (Layer D): OUT OF SCOPE; substrate type in Layer B exposes the probe surface for future follow-on Layer D adapter missions

## Detailed Design

### CLI surface

```
octo network heartbeat probe <peer_did> [--timeout-ms <N: u16>] [--format ascii|json] [--json]
```

- `heartbeat probe` — read-only rendering of heartbeat probe result (Reachable { rtt_ms } + Unreachable { reason } + Timeout)
- `--timeout-ms` — probe timeout in milliseconds (clap u16 overflow pre-dispatch rejection; default 5000)
- `--format ascii|json` — output format (default ascii)
- `--json` — alias for `--format json`

### Substrate extensions (Layer B — ONE NEW MODULE)

NEW `crates/octo-network/src/mon/heartbeat.rs`:

```rust
/// Heartbeat probe (RFC-0855 §Wire Format heartbeat probing).
///
/// Phase 14 G17 per RFC-0011-v §Substrate Mapping Table. Operates
/// on the in-memory snapshot; real transport-level probe OUT OF
/// SCOPE. Per-extension impl crates (Layer D) provide real
/// transport-level probes in follow-on missions.
#[derive(Clone, Debug, Default)]
pub struct Heartbeat;

/// Heartbeat probe result.
///
/// Phase 14 G17 per RFC-0011-v §Substrate Mapping Table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HeartbeatProbeResult {
    /// Peer is reachable; `rtt_ms` is round-trip-time in milliseconds.
    Reachable { rtt_ms: u32 },
    /// Peer is unreachable; `reason` describes why.
    Unreachable { reason: UnreachableReason },
    /// Probe timed out.
    Timeout,
}

/// Reason for an unreachable probe result.
///
/// Phase 14 G17 per RFC-0011-v §Substrate Mapping Table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnreachableReason {
    /// Peer DID is malformed.
    InvalidPeerDid,
    /// No transport adapter registered for this peer.
    NoTransportAdapter,
    /// Transport adapter refused the probe (e.g. protocol mismatch).
    AdapterRefused,
    /// Reserved for follow-on Layer D adapter detail.
    Other(String),
}

impl Heartbeat {
    /// Probe a peer by DID; returns `HeartbeatProbeResult` indicating
    /// reachability + RTT or unreachable reason. Phase 14 returns
    /// `Timeout` unconditionally (real probe OUT OF SCOPE).
    pub fn probe(&self, peer_did: &str, timeout_ms: u16) -> HeartbeatProbeResult;
}
```

### Output envelope

```rust
/// `octo network heartbeat probe` output envelope.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkHeartbeatProbeOutput {
    /// Peer DID (echo).
    pub peer_did: String,
    /// Probe result (Reachable { rtt_ms } + Unreachable { reason } + Timeout).
    pub result: HeartbeatProbeResult,
    /// Timeout in milliseconds (echo).
    pub timeout_ms: u16,
    /// Format flag (ascii + json).
    pub format: HeartbeatFormatKind,
}
```

### Test vectors (3)

- `tv_net14_1`: heartbeat probe default (no --timeout-ms) parses cleanly with default 5000ms timeout
- `tv_net14_2`: heartbeat probe --timeout-ms 1000 parses cleanly
- `tv_net14_3`: heartbeat probe --timeout-ms 65535 parses cleanly (clap u16 max)

## Exit codes

Slot 89 `NetworkSubstrateUnavailable` REUSE per Phase 6 precedent + user decision. 0 NEW OctoCliError variants for Phase 14.

## Layer discipline

- **Layer A frozen preserved**: zero change to `octo-governance-core`, `octo-audit-core`, `octo-settlement-core`, `octo-vault-core`, `octo-wallet-core`.
- **Layer B substrate NEW MODULE**: `mon/heartbeat.rs` (per Phase 7 RFC-0011-o NEW module precedent for SlashBridge).
- **Layer C CLI dispatch**: `commands/network.rs` extended with `NetworkAction::Heartbeat { action: NetworkHeartbeatAction }` clap variant; `NetworkHeartbeatAction::Probe(HeartbeatProbeArgs)`; 1 output envelope; 1 handler; 3 test vectors.

## Substrate-faithfulness

The `Heartbeat::probe()` operates on the in-memory snapshot only (returns `Timeout` unconditionally). Per-extension Layer D adapter crates (live transport-level probe) OUT OF SCOPE for Phase 14 per RFC-0011-h §Future Work items F8+F9. BTreeMap-based deterministic iteration ordering preserved per RFC-0011-h §Output Envelope determinism (no Map fields introduced). `parse_32_byte_hex` pastejacking defense preserved per Phase 5 RFC-0011-m precedent.

## Companion stub missions

G17 `0011-h-s-a-heartbeat-probe` (Open → Claimed → Completed paired with CLI dispatch slice).

## Stale-stub sweep

Per Phase 6 `9a903993` precedent, the final commit for RFC-0011-v sweep any remaining Open stubs related to RFC-0011-h §Substrate-Additions after Phase 14 closure. Per "everything included, no deferral" user directive 2026-09-20, no G-row may remain Open after Phase 14.

## Out of Scope

- Live transport-level probe adapter (Layer D; follow-on per-extension crate missions)
- Per-extension impl crates (substrate-ext-heartbeat-transport-*) OUT OF SCOPE for Phase 14
- Wire format versioning (RFC-0011-h §Future Work items F8+F9)
- Real probe logic (Phase 14 substrate returns Timeout unconditionally; real probe in follow-on Layer D adapter mission)

## History

- 2026-09-20 — Draft (this version)
