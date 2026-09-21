# 0011-h-network-gossip — `octo network gossip` CLI surface (RFC-0011-t Phase 12 G15)

## Status

Completed (2026-09-20) — CLI dispatch slice for RFC-0011-t Phase 12 G15 gossip-stats amendment LANDED at `next b0cc47cc`. CLI mission YAML CREATED at CLI dispatch slice time per user decision. Pairs with substrate companion YAML `0011-h-s-a-gossip-stats` (substrate slice at `next c07ad375`, claimed transition at `next 2a12bef4`, completed transition paired). 1 NEW subcommand + 1 NEW output envelope + 3 NEW test vectors `tv_net12_1` through `tv_net12_3`. Layer C CLI dispatch only; substrate additive type extension lives at `crates/octo-network/src/mon/gossip.rs`.

## RFC

RFC-0011-t §Subcommand Taxonomy Phase 12 G15 — `gossip --stats`.

## Summary

EXPOSES the RFC-0011-t Phase 12 substrate additive type extension (`Gossip::stats()`) through the `octo network gossip --stats` CLI surface. Read-only subcommand; substrate-faithful projection of `Gossip` counters as ASCII or JSON. Layer C (octo-cli dispatch) only; substrate additive type extension lives in `crates/octo-network` Layer B.

### Subcommand surface

```
octo network gossip --stats [--format ascii|json] [--json]
```

### CLI substrate mapping

| Subcommand       | Substrate method  | Format / arg               | Output envelope            |
| ---------------- | ----------------- | -------------------------- | -------------------------- |
| `gossip --stats` | `Gossip::stats()` | `ascii` (default) / `json` | `NetworkGossipStatsOutput` |

### File changes

- `crates/octo-cli/src/commands/network.rs` — modified (1 file changed, 169 insertions, 0 deletions)

  - `NetworkAction::Gossip { action: NetworkGossipAction }` clap variant
  - `NetworkGossipAction` enum (Stats)
  - `GossipStatsArgs` (format + json)
  - `GossipStatsFormatKind` clap `ValueEnum` (Ascii + Json)
  - `NetworkGossipStatsOutput` envelope struct
  - `network_gossip_stats` handler
  - `gossip_stats_registry` runtime marker
  - Dispatch arm in `network_dispatch`
  - 3 NEW test vectors `tv_net12_1` through `tv_net12_3`

### Test vectors

| Vector       | Subcommand                     | Coverage                              |
| ------------ | ------------------------------ | ------------------------------------- |
| `tv_net12_1` | `gossip --stats`               | default format (ascii) parses cleanly |
| `tv_net12_2` | `gossip --stats --format json` | json format parses cleanly            |
| `tv_net12_3` | `gossip --stats --json`        | json flag parses cleanly              |

### Dependencies

- RFC-0011-t Phase 12 gossip-stats amendment Draft at `next 59bb39b4`
- Phase 12 G15 substrate stub fill-in at `next 8244e969`
- Phase 12 G15 substrate slice at `next c07ad375`
- Phase 12 G15 paired-YAML Claimed transition at `next 2a12bef4`
- Phase 12 G15 CLI dispatch slice at `next b0cc47cc`
- `octo_network` Layer B substrate (existing; Phase 12 EXTENDS the module at `crates/octo-network/src/mon/gossip.rs`)
- Layer C CLI dispatch envelope/handler pattern (Phase 5 RFC-0011-m at `next 346f10cc`)

## Acceptance Criteria

- [x] `NetworkAction::Gossip { action: NetworkGossipAction }` clap variant lands in commands/network.rs
- [x] `NetworkGossipAction` enum lands with `Stats(GossipStatsArgs)` variant
- [x] `GossipStatsArgs` lands with `format` (clap ValueEnum, default_value_t=Ascii) + `json: bool` fields
- [x] `GossipStatsFormatKind` clap ValueEnum lands with Ascii + Json variants
- [x] `NetworkGossipStatsOutput` envelope lands (mission_id_hex + 6 counter fields + format)
- [x] `network_gossip_stats` handler lands with gossip_stats_registry marker guard
- [x] `gossip_stats_registry` runtime marker lands (returns `false` for Phase 12 additive-type-only; per-extension impl crates Layer D OUT OF SCOPE)
- [x] Dispatch arm lands in `network_dispatch`
- [x] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean (NO regression of existing 430 tests)
- [x] `cargo test -p octo-cli --lib` green (433/433; +3 NEW test vectors)
- [x] Layer discipline preserved (Layer C only; zero Layer A or Layer B change in this commit)
- [x] 3 NEW test vectors cover default format + json format + json flag

## Out of Scope

- Substrate additive type extension (paired mission `0011-h-s-a-gossip-stats` covers that surface; landed at `next c07ad375`)
- Live gossip adapter (per-extension impl crates substrate-ext-gossip-source-* in Layer D, follow-on missions)
- Per-extension registry wiring (CLI dispatch uses marker; per-extension init fn OUT OF SCOPE for Phase 12)
- Wire format versioning (deferred to RFC-0011-h §Future Work items F8 + F9)
- Real anti-entropy counter (Phase 12 substrate stubs counter at 0; real counter in follow-on Layer D adapter mission)

## Notes

RFC-0011-t Phase 12 G15 CLI surface exposed through `octo network gossip --stats` subcommand. Substrate-faithful to `crates/octo-network/src/mon/gossip.rs` additive type extension. CLI dispatch slice paired with substrate companion YAML via the Phase 5 RFC-0011-m 5-commit pattern (stub fill-in → substrate slice → YAML Claimed → CLI dispatch → YAMLs Completed). Slot 89 `NetworkSubstrateUnavailable` REUSE per Phase 6 precedent (0 NEW OctoCliError variants). Per-extension crate pattern preserved (additive types in Layer B; concrete per-gossip-source adapter crates in separate Layer D follow-on missions). Layer A frozen preserved.
