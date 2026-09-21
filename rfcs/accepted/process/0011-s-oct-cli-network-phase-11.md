# RFC-0011-s: `octo network` Phase 11 — Topology Render (G14)

## Status

Accepted (2026-09-21) — RFC-0011-s lands RFC-0011-h §Implementation Phases Phase 11. One subcommand wires topology render via the existing `TopologyCommitment` substrate (EXTEND `mon/topology.rs` with `render()` method per Phase 5 precedent). Companion stub mission G14 + 0 NEW OctoCliError variants (REUSES slot 89 `NetworkSubstrateUnavailable` per RFC-0011-h §Error Handling row 89) + 1 output envelope + 13 test vectors.

Promoted Draft → Accepted at `next 8d00e0ff` per RFC-0011-h promotion precedent (R1+R2 zero rounds gate pair at R4).

> **Amendment chain:** Eleventh amendment in the `0011-h-multiphase-rollout-plan` (see `docs/plans/2026-09-20-0011-h-multiphase-rollout-plan.md`, gitignored scratchpad per [[docs-plans-scratchpad]]). Phase 1 = RFC-0011-i. Phase 2 = RFC-0011-j. Phase 3 = RFC-0011-k. Phase 4 = RFC-0011-l. Phase 5 = RFC-0011-m. Phase 6 = RFC-0011-n. Phase 7 = RFC-0011-o. Phase 8 = RFC-0011-p. Phase 9 = RFC-0011-q. Phase 10 = RFC-0011-r. Phase 11 = RFC-0011-s (this RFC).

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu

## Summary

RFC-0011-s lands the **topology render** slice of RFC-0011-h §Implementation Phases. One CLI subcommand renders the topology commitment as a graph (ASCII or DOT) via the existing `TopologyCommitment` substrate (EXTEND `mon/topology.rs` with `render()` method per Phase 5 RFC-0011-m precedent; existing `§GraphFormat` enum from Phase 1 `TrustGraph::render` at `§mon/trust_graph.rs` is REUSED).

- `octo network topology render [--format ascii|dot] [--depth <N>]` — read-only projection of topology as graph

Substrate per RFC-0855 §5.1 Topology models. EXTENDS existing `§TopologyCommitment` at `§mon/topology.rs` (additive method extension per Phase 5 RFC-0011-m precedent).

## Dependencies

- RFC-0011-h
- RFC-0855 §5.1 Topology models
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

## Design Goals

1. Wire `TopologyCommitment::render()` extension (RFC-0855 §5.2) to the CLI for operator graph-output inspection
2. Preserve per-extension crate pattern: existing topology substrate in Layer B (`octo-network::mon::topology::TopologyCommitment`); concrete per-topology-source adapter in Layer D, OUT OF SCOPE
3. Preserve Layer A frozen contract (zero Layer A change per RFC-0011-h §Layer Discipline)
4. Preserve additive method extension pattern (Phase 5 RFC-0011-m precedent) — EXTEND existing struct with new method; downstream code unchanged
5. REUSE `§GraphFormat` enum from Phase 1 `TrustGraph::render` at `§mon/trust_graph.rs` (existing import; Ascii + Dot variants)
6. Preserve BTreeMap determinism where substrate returns ordered data
7. Preserve slot 89 REUSE per Phase 6 precedent + user decision (0 NEW OctoCliError variants)
8. 13 test vectors — tv_net11_1 through tv_net11_13

## Motivation

RFC-0011-h §Implementation Phases Phase 11 (G14) calls for wiring topology render to the CLI. Operators need to inspect the topology commitment as a graph (ASCII for terminal output, DOT for further processing). The substrate is PARTIAL: `TopologyCommitment` struct exists with `compute(...)` constructor at `§mon/topology.rs` per RFC-0855 §5.2 but lacks a `render(format)` method. This RFC's companion mission (G14 `0011-h-s-a-topology-render`) extends the struct with the `render()` method.

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
- `--depth <N>` — optional depth cap for graph rendering (1-100; `parse_graph_depth` clamp rejected pre-dispatch per Phase 5 RFC-0011-m precedent shared with `trust-graph render`)
- `--json` — force JSON envelope output

### Substrate extension (Layer B)

EXTENDS `TopologyCommitment` at `§mon/topology.rs` with:

```rust
impl TopologyCommitment {
    /// Render the topology commitment as an ASCII or DOT graph
    /// (Phase 11 G14 per RFC-0011-s §Substrate Mapping Table).
    /// Operates on the in-memory snapshot of the commitment;
    /// live topology-source adapter OUT OF SCOPE for Phase 11.
    pub fn render(&self, format: GraphFormat) -> String;
}
```

REUSES the existing `§GraphFormat` enum from Phase 1 `TrustGraph::render` at `§mon/trust_graph.rs`:

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
    pub depth: Option<u32>,
    /// Rendered graph output (string body).
    pub render: String,
}
```

### Test vectors (13)

- `tv_net11_1`: topology render default format (ascii) parses cleanly
- `tv_net11_2`: topology render --format dot parses cleanly
- `tv_net11_3`: topology render --depth 200 rejected pre-dispatch (`parse_graph_depth` 1..=100 clamp)
- `tv_net11_4`: topology render --depth 1 accepted pre-dispatch (lower boundary of `parse_graph_depth` 1..=100)
- `tv_net11_5`: topology render --depth 100 accepted pre-dispatch (upper boundary of `parse_graph_depth` 1..=100)
- `tv_net11_6`: topology render default depth (no `--depth`) parses with `None` (depth cap is optional)
- `tv_net11_7`: topology render --depth 0 rejected pre-dispatch (`parse_graph_depth` 1..=100 clamp below-range)
- `tv_net11_8`: handler dispatch with `--depth 5` returns `Ok` and substrate body contract contains `depth=5` footer + empty-topology marker (R3.5 substrate body inspection per Phase 10 R3 MAJOR-1 lesson; dispatch handler returns `Result<(), OctoCliError>` so body contract verified via direct substrate call)
- `tv_net11_9`: handler dispatch with no `--depth` returns `Ok` and substrate body contains `depth=full` footer (default dispatch surfaces omitted depth as `depth=full`)
- `tv_net11_10`: topology render --json flag parses cleanly
- `tv_net11_11`: topology render --format dot (R3.5 NIT-1 close; explicit `TopologyFormatKind::Dot` variant assertion)
- `tv_net11_12`: topology render --depth non-numeric rejected pre-dispatch (`parse_graph_depth` integer parse; R3.5 NIT-2 close)
- `tv_net11_13`: `NetworkTopologyRenderOutput` JSON envelope serde round-trip (R3.5 MINOR-3 close; `Serialize + Deserialize` contract verifiable)

## Exit codes

Slot 89 `NetworkSubstrateUnavailable` REUSE per Phase 6 precedent + user decision. 0 NEW OctoCliError variants for Phase 11.

## Layer discipline

- **Layer A frozen preserved**: zero change to `octo-governance-core`, `octo-audit-core`, `octo-settlement-core`, `octo-vault-core`, `octo-wallet-core`.
- **Layer B substrate EXTENDED**: `TopologyCommitment` gains `render(format)` method. No new types.
- **Layer C CLI dispatch**: `commands/network.rs` extended with `NetworkAction::Topology { action: NetworkTopologyAction }` clap variant; `NetworkTopologyAction::Render(TopologyRenderArgs)`; output envelope; handler; 13 test vectors.

## Substrate-faithfulness

The `render(format, depth: Option<u32>)` method (R3.5 substrate signature change) operates on the in-memory snapshot of the `TopologyCommitment` struct (no live topology source). Per-extension Layer D adapter crates (live topology source adapters) OUT OF SCOPE for Phase 11 per RFC-0011-h §Future Work items F8+F9. Determinism preserved across calls (no HashMap iteration, no randomness, no collection iteration of any kind in `render_ascii` + `render_dot`) per RFC-0011-h §Output Envelope determinism.

The CLI handler invokes the substrate method unconditionally — no registry gate (trait dispatch is the universal code path per Phase 10 RFC-0011-r R2.5 substrate-faithfulness precedent). The depth footer and empty-state sentinel are embedded INSIDE the rendered body at the substrate layer, NOT appended by the handler (R3.5 fix; closes R3 bugs MINOR-1 DOT contamination regression where the depth footer was appended after the closing `}` of the digraph, breaking valid DOT output for `dot -Tpng` / `dot -Tsvg` processing). For ASCII output, the `depth={N}` (or `depth=full` when omitted) footer line and the empty-state sentinel `(empty topology — live source adapter required for Phase 11)` appear as trailing lines. For DOT output, both the depth footer and the empty-state sentinel appear as DOT comments INSIDE the digraph block so the output remains valid DOT (bypass `dot -Tpng` / `dot -Tsvg` to verify). The depth footer surfaces the `--depth` cap value in operator-visible output so the cap is never silently dropped (Phase 11 trait-only dispatch does not depth-filter an in-memory snapshot; the value is forwarded for future Layer D live-topology adapters). The empty-state sentinel mirrors the Phase 1 `TrustGraph::render_ascii` precedent at `mon::trust_graph::render_ascii` (returns `(empty trust graph)` for empty input) and signals to operators that the dispatched Mesh commitment is a stub, not a real topology.

## Companion stub missions

G14 `0011-h-s-a-topology-render` (Open → Claimed → Completed paired with CLI dispatch slice).

## Out of Scope

- Live topology source adapter (Layer D; follow-on per-extension crate missions)
- Per-extension impl crates (substrate-ext-topology-source-*) OUT OF SCOPE for Phase 11
- Wire format versioning (RFC-0011-h §Future Work items F8+F9)
- Real anti-entropy counter or live topology fetching (stub returns in-memory snapshot only)

## History

- 2026-09-20 — Draft (this RFC)
- 2026-09-21 — R1.5 fix sweep: depth 1..=100 clamp via `parse_graph_depth`; cite hygiene strip status parentheticals on Dependencies + amendment chain; `§GraphFormat` REUSE ref to Phase 1 `TrustGraph::render`; removed file:line refs from prose; depth test vectors expanded to 7
- 2026-09-21 — R2.5 fix sweep: `--depth` value surfaces in rendered output footer (`depth={N}` or `depth=full`); empty-topology stub marker appended to rendered body (Phase 1 `TrustGraph::render_ascii` empty-state sentinel precedent); `escape_dot` helper added to `mon::topology` (Phase 1 `mon::trust_graph::escape_dot` precedent); `#[non_exhaustive]` attribute added to `GraphFormat` enum in `mon::trust_graph` (RFC §Substrate Extension claim); tv_net11_8 dispatch test with `--depth 5`; tv_net11_9 dispatch test with default depth; depth test vectors expanded to 9
- 2026-09-21 — R3.5 fix sweep: substrate `render(format, depth: Option<u32>)` signature extended; depth footer + empty-topology marker embedded INSIDE substrate body (ASCII: trailing lines; DOT: comments inside digraph block — keeps DOT output valid for `dot -Tpng` / `dot -Tsvg`); handler no longer appends depth footer or marker; tv_phase11_substrate_5/6/7/8 added at substrate layer to verify body contract (closes R3 bugs MINOR-1 DOT contamination regression); tv_net11_8 + tv_net11_9 dispatch tests now inspect substrate body contract per Phase 10 R3 MAJOR-1 lesson; tv_net11_10 (--json flag parse) + tv_net11_11 (--format dot explicit variant) + tv_net11_12 (--depth non-numeric rejected) + tv_net11_13 (NetworkTopologyRenderOutput JSON serde round-trip) added to close R3 MINOR + NIT test coverage gaps; `NetworkTopologyRenderOutput` envelope gained `Deserialize` derive; depth test vectors expanded to 13
