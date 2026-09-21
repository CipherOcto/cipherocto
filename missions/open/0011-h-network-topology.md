# 0011-h-network-topology — `octo network topology` CLI surface (RFC-0011-s Phase 11 G14)

## Status

Completed (2026-09-20) — CLI dispatch slice for RFC-0011-s Phase 11 G14 topology-render amendment LANDED at `next 7777610e`. CLI mission YAML CREATED at CLI dispatch slice time per user decision. Pairs with substrate companion YAML `0011-h-s-a-topology-render` (substrate slice at `next 293556e4`, claimed transition at `next 0a963705`, completed transition paired). 1 NEW subcommand + 1 NEW output envelope + 3 NEW test vectors `tv_net11_1` through `tv_net11_3`. Layer C CLI dispatch only; substrate method extension lives at `crates/octo-network/src/mon/topology.rs:81`.

## RFC

RFC-0011-s §Subcommand Taxonomy Phase 11 G14 — `topology render`.

## Summary

EXPOSES the RFC-0011-s Phase 11 substrate method extension (`TopologyCommitment::render(format)`) through the `octo network topology render` CLI surface. Read-only subcommand; substrate-faithful projection of `TopologyCommitment` as ASCII or DOT graph. Layer C (octo-cli dispatch) only; substrate method extension lives in `crates/octo-network` Layer B.

### Subcommand surface

```
octo network topology render [--format ascii|dot] [--depth <N>] [--json]
```

### CLI substrate mapping

| Subcommand | Substrate method | Format / arg | Output envelope |
| --- | --- | --- | --- |
| `topology render` | `TopologyCommitment::render(format)` | `ascii` (default) / `dot` | `NetworkTopologyRenderOutput` |

### File changes

- `crates/octo-cli/src/commands/network.rs` — modified (1 file changed, 171 insertions, 0 deletions)

  - `NetworkAction::Topology { action: NetworkTopologyAction }` clap variant
  - `NetworkTopologyAction` enum (Render)
  - `TopologyRenderArgs` (format + depth + json)
  - `TopologyFormatKind` clap `ValueEnum` (Ascii + Dot)
  - `NetworkTopologyRenderOutput` envelope struct
  - `network_topology_render` handler
  - `topology_render_registry` runtime marker
  - Dispatch arm in `network_dispatch`
  - 3 NEW test vectors `tv_net11_1` through `tv_net11_3`

### Test vectors

| Vector | Subcommand | Coverage |
| --- | --- | --- |
| `tv_net11_1` | `topology render` | default format (ascii) parses cleanly |
| `tv_net11_2` | `topology render --format dot` | dot format parses cleanly |
| `tv_net11_3` | `topology render --depth 65536` | clap u16 overflow pre-dispatch rejection |

### Dependencies

- RFC-0011-s Phase 11 topology-render amendment Draft at `next a2bfc2cb`
- Phase 11 G14 substrate stub fill-in at `next 5441fce4`
- Phase 11 G14 substrate slice at `next 293556e4`
- Phase 11 G14 paired-YAML Claimed transition at `next 0a963705`
- Phase 11 G14 CLI dispatch slice at `next 7777610e`
- `octo_network` Layer B substrate (existing; Phase 11 EXTENDS the struct at `crates/octo-network/src/mon/topology.rs:81`)
- Layer C CLI dispatch envelope/handler pattern (Phase 5 RFC-0011-m at `next 346f10cc`)

## Acceptance Criteria

- [x] `NetworkAction::Topology { action: NetworkTopologyAction }` clap variant lands in commands/network.rs
- [x] `NetworkTopologyAction` enum lands with `Render(TopologyRenderArgs)` variant
- [x] `TopologyRenderArgs` lands with `format` (clap ValueEnum, default_value_t=Ascii) + `depth: Option<u16>` + `json: bool` fields
- [x] `TopologyFormatKind` clap ValueEnum lands with Ascii + Dot variants
- [x] `NetworkTopologyRenderOutput` envelope lands (mission_id_hex + format + depth + render)
- [x] `network_topology_render` handler lands with topology_render_registry marker guard
- [x] `topology_render_registry` runtime marker lands (returns `false` for Phase 11 additive-method-only; per-extension impl crates Layer D OUT OF SCOPE)
- [x] Dispatch arm lands in `network_dispatch`
- [x] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean (NO regression of existing 427 tests)
- [x] `cargo test -p octo-cli --lib` green (430/430; +3 NEW test vectors)
- [x] Layer discipline preserved (Layer C only; zero Layer A or Layer B change in this commit)
- [x] 3 NEW test vectors cover format parsing + dot parsing + depth u16 overflow pre-dispatch

## Out of Scope

- Substrate method extension (paired mission `0011-h-s-a-topology-render` covers that surface; landed at `next 293556e4`)
- Live topology source adapter (per-extension impl crates substrate-ext-topology-source-* in Layer D, follow-on missions)
- Per-extension registry wiring (CLI dispatch uses marker; per-extension init fn OUT OF SCOPE for Phase 11)
- Wire format versioning (deferred to RFC-0011-h §Future Work items F8 + F9)
- Recursive graph rendering --depth > 1 (Phase 11 stubs depth flag; follow-on Layer D adapter missions in future)

## Notes

RFC-0011-s Phase 11 G14 CLI surface exposed through `octo network topology render` subcommand. Substrate-faithful to `crates/octo-network/src/mon/topology.rs:81` method extension. CLI dispatch slice paired with substrate companion YAML via the Phase 5 RFC-0011-m 5-commit pattern (stub fill-in → substrate slice → YAML Claimed → CLI dispatch → YAMLs Completed). Slot 89 `NetworkSubstrateUnavailable` REUSE per Phase 6 precedent (0 NEW OctoCliError variants). Per-extension crate pattern preserved (additive method in Layer B; concrete per-topology-source adapter crates in separate Layer D follow-on missions). clap u16 overflow pre-dispatch rejection per Phase 5 RFC-0011-m pastejacking defense pattern. Layer A frozen preserved.
