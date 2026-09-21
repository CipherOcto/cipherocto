# RFC-0011-s: `octo network` Phase 11 — Topology Render (G14)

## Status

Draft (2026-09-20) — RFC-0011-s lands RFC-0011-h §Implementation Phases Phase 11. One subcommand wires topology render via the existing `TopologyCommitment` substrate (EXTEND `mon/topology.rs` with `render()` method per Phase 5 precedent). Companion stub mission G14 + 0 NEW OctoCliError variants (REUSES slot 89 `NetworkSubstrateUnavailable` per RFC-0011-h §Error Handling row 89) + 1 output envelope + 3 test vectors.

> **Amendment chain:** Eleventh amendment in the `0011-h-multiphase-rollout-plan` (see `docs/plans/2026-09-20-0011-h-multiphase-rollout-plan.md`, gitignored scratchpad per `.gitignore` line 46). Phase 1 = RFC-0011-i (DRY CLOSED). Phase 2 = RFC-0011-j (DRY CLOSED). Phase 3 = RFC-0011-k (DRY CLOSED). Phase 4 = RFC-0011-l (DRY CLOSED). Phase 5 = RFC-0011-m (DRY CLOSED). Phase 6 = RFC-0011-n (DRY CLOSED). Phase 7 = RFC-0011-o (DRY CLOSED + Accepted). Phase 8 = RFC-0011-p (IMPLEMENTATION CLOSED). Phase 9 = RFC-0011-q (IMPLEMENTATION CLOSED). Phase 10 = RFC-0011-r (IMPLEMENTATION CLOSED). Phase 11 = RFC-0011-s (this RFC).

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu

## Summary

RFC-0011-s lands the **topology render** slice of RFC-0011-h §Implementation Phases. One CLI subcommand renders the topology commitment as a graph (ASCII or DOT) via the existing `TopologyCommitment` substrate (EXTEND `mon/topology.rs` with `render()` method per Phase 5 RFC-0011-m precedent; existing `GraphFormat` enum from `mon/trust_graph.rs:41` is REUSED).

- `octo network topology render [--format ascii|dot] [--depth <N>]` — read-only projection of topology as graph

Substrate per RFC-0855 §5.1 (Topology models). EXTENDS existing `TopologyCommitment` at `crates/octo-network/src/mon/topology.rs:36` (additive method extension per Phase 5 RFC-0011-m precedent).

## Dependencies

- RFC-0011-h Accepted
- RFC-0855 (Governing RFC; §5.1 Topology models)
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

## Design Goals

1. Wire `TopologyCommitment::render()` extension (RFC-0855 §5.2) to the CLI for operator graph-output inspection
2. Preserve per-extension crate pattern: existing topology substrate in Layer B (`octo-network::mon::topology::TopologyCommitment`); concrete per-topology-source adapter in Layer D, OUT OF SCOPE
3. Preserve Layer A frozen contract (zero Layer A change per RFC-0011-h §Layer Discipline)
4. Preserve additive method extension pattern (Phase 5 RFC-0011-m precedent) — EXTEND existing struct with new method; downstream code unchanged
5. REUSE `GraphFormat` enum from `crates/octo-network/src/mon/trust_graph.rs:41` (existing import; Ascii + Dot variants)
6. Preserve BTreeMap determinism where substrate returns ordered data
7. Preserve slot 89 REUSE per Phase 6 precedent + user decision (0 NEW OctoCliError variants)
8. 3 test vectors — tv_net11_1 through tv_net11_3

## Motivation

RFC-0011-h §Implementation Phases Phase 11 (G14) calls for wiring topology render to the CLI. Operators need to inspect the topology commitment as a graph (ASCII for terminal output, DOT for further processing). The substrate is PARTIAL: `TopologyCommitment` struct exists with `compute(...)` constructor at `crates/octo-network/src/mon/topology.rs:36` per RFC-0855 §5.2 but lacks a `render(format)` method. This RFC's companion mission (G14 `0011-h-s-a-topology-render`) extends the struct with the `render()` method.

## Roles and Authorities

- **Operator**: invokes `octo network topology render` for diagnostic graph output
- **Topology commitment**: the deterministic on-chain topology state (subject of the read operation)
- **Format**: optional ASCII / DOT projection of the graph rendering
- **Per-extension concrete impl crates** (Layer D): OUT OF SCOPE; existing topology substrate in Layer B exposes the render surface for future follow-on Layer D adapter missions

## Detailed Design

### CLI surface

```
octo network topology render [--format ascii|dot] [--depth <N>] [--json]
```

- `topology render` — read-only rendering of the committed topology
- `--format ascii` (default) / `--format dot` — ASCII (terminal-friendly) or DOT (graphviz pipe) format
- `--depth <N>` — optional depth cap for graph rendering (1-100; clap u16 overflow rejected pre-dispatch per Phase 5 RFC-0011-m precedent)
- `--json` — force JSON envelope output

### Substrate extension (Layer B)

EXTENDS `TopologyCommitment` at `crates/octo-network/src/mon/topology.rs:36` with:

```rust
impl TopologyCommitment {
    /// Render the topology commitment as an ASCII or DOT graph
    /// (Phase 11 G14 per RFC-0011-s §Substrate Mapping Table).
    /// Operates on the in-memory snapshot of the commitment;
    /// live topology-source adapter OUT OF SCOPE for Phase 11.
    pub fn render(&self, format: GraphFormat) -> String;
}
```

REUSES the existing `GraphFormat` enum at `crates/octo-network/src/mon/trust_graph.rs:41`:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum GraphFormat {
    Ascii,
    Dot,
}
```

The `render()` impl dispatches to a per-format string generator. ASCII format produces a BLAKE3 short-id + topology-model label per line; DOT format produces a `digraph G { ... }` block with deterministic BTreeMap iteration ordering per RFC-0011-h §Output Envelope determinism.

### Output envelope

```rust
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkTopologyRenderOutput {
    /// 32-byte mission_id as 64 lowercase hex chars (echo).
    pub mission_id_hex: String,
    /// Format that was applied (`ascii` / `dot`).
    pub format: String,
    /// Optional depth cap (1-100) that was applied.
    pub depth: Option<u16>,
    /// Rendered graph output (string body).
    pub render: String,
}
```

### Test vectors (3)

- `tv_net11_1`: topology render default format (ascii) parses cleanly
- `tv_net11_2`: topology render --format dot parses cleanly
- `tv_net11_3`: topology render --depth 200 rejected pre-dispatch (clap u16 overflow)

## Exit codes

Slot 89 `NetworkSubstrateUnavailable` REUSE per Phase 6 precedent + user decision. 0 NEW OctoCliError variants for Phase 11.

## Layer discipline

- **Layer A frozen preserved**: zero change to `octo-governance-core`, `octo-audit-core`, `octo-settlement-core`, `octo-vault-core`, `octo-wallet-core`.
- **Layer B substrate EXTENDED**: `TopologyCommitment` gains `render(format)` method. No new types.
- **Layer C CLI dispatch**: `commands/network.rs` extended with `NetworkAction::Topology { action: NetworkTopologyAction }` clap variant; `NetworkTopologyAction::Render(TopologyRenderArgs)`; output envelope; handler; 3 test vectors.

## Substrate-faithfulness

The `render()` method operates on the in-memory snapshot of the `TopologyCommitment` struct (no live topology source). Per-extension Layer D adapter crates (live topology source adapters) OUT OF SCOPE for Phase 11 per RFC-0011-h §Future Work items F8+F9. BTreeMap-based deterministic iteration ordering preserved per RFC-0011-h §Output Envelope determinism.

## Companion stub missions

G14 `0011-h-s-a-topology-render` (Open → Claimed → Completed paired with CLI dispatch slice).

## Out of Scope

- Live topology source adapter (Layer D; follow-on per-extension crate missions)
- Per-extension impl crates (substrate-ext-topology-source-*) OUT OF SCOPE for Phase 11
- Wire format versioning (RFC-0011-h §Future Work items F8+F9)
- Real anti-entropy counter or live topology fetching (stub returns in-memory snapshot only)

## History

- 2026-09-20 — Draft (this version)
