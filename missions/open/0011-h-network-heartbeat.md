# 0011-h-network-heartbeat — `octo network heartbeat` CLI surface (RFC-0011-v Phase 14 G17)

## Status

Completed (2026-09-20) — CLI dispatch slice for RFC-0011-v Phase 14 G17 heartbeat-probe amendment LANDED at `next f622434c`. CLI mission YAML CREATED at CLI dispatch slice time per user decision. Pairs with substrate companion YAML `0011-h-s-a-heartbeat-probe` (substrate slice at `next ffd3b9bf`, claimed transition at `next 64e8dd4c`, completed transition paired). 1 NEW subcommand + 1 NEW output envelope + 3 NEW test vectors `tv_net14_1` through `tv_net14_3`. Layer C CLI dispatch only; substrate additive type lives at `crates/octo-network/src/mon/heartbeat.rs`.

## RFC

RFC-0011-v §Subcommand Taxonomy Phase 14 G17 — `heartbeat probe`.

## Summary

EXPOSES the RFC-0011-v Phase 14 substrate additive type (`Heartbeat::probe()`) through the `octo network heartbeat probe` CLI surface. Read-only subcommand; substrate-faithful projection of `HeartbeatProbeResult` (Reachable + Unreachable + Timeout). Layer C (octo-cli dispatch) only; substrate additive type lives in `crates/octo-network` Layer B per Phase 7 RFC-0011-o SlashBridge NEW module precedent.

### Subcommand surface

```
octo network heartbeat probe <peer_did> [--timeout-ms <N: u16>] [--json]
```

### CLI substrate mapping

| Subcommand          | Substrate method           | Format / arg                       | Output envelope                |
| ------------------- | -------------------------- | ---------------------------------- | ------------------------------ |
| `heartbeat probe`   | `Heartbeat::probe()`       | `ascii` (default) / `json`         | `NetworkHeartbeatProbeOutput`  |

### File changes

- `crates/octo-cli/src/commands/network.rs` — modified (1 file changed, 189 insertions, 0 deletions)

  - `NetworkAction::Heartbeat { action: NetworkHeartbeatAction }` clap variant
  - `NetworkHeartbeatAction` enum (Probe)
  - `HeartbeatProbeArgs` (peer_did + timeout_ms + json)
  - `NetworkHeartbeatProbeOutput` envelope struct (peer_did + result_label + rtt_ms + unreachable_reason_label + unreachable_reason_detail + timeout_ms)
  - `network_heartbeat_probe` handler with `heartbeat_probe_registry` marker guard
  - `heartbeat_probe_registry` runtime marker
  - Dispatch arm in `network_dispatch`
  - 3 NEW test vectors `tv_net14_1` through `tv_net14_3`

### Test vectors

| Vector       | Subcommand                                         | Coverage                                                       |
| ------------ | -------------------------------------------------- | -------------------------------------------------------------- |
| `tv_net14_1` | `heartbeat probe <peer_did>`                       | default 5000ms timeout parses cleanly                          |
| `tv_net14_2` | `heartbeat probe <peer_did> --timeout-ms 1000`     | explicit 1000ms timeout parses cleanly                         |
| `tv_net14_3` | `heartbeat probe <peer_did> --timeout-ms 65535`    | clap u16 max parses cleanly (Phase 5 RFC-0011-m precedent)     |

### Dependencies

- RFC-0011-v Phase 14 heartbeat-probe amendment Draft at `next 2bb4bfb2`
- Phase 14 G17 substrate stub fill-in at `next d7be389d`
- Phase 14 G17 substrate slice at `next ffd3b9bf`
- Phase 14 G17 paired-YAML Claimed transition at `next 64e8dd4c`
- Phase 14 G17 CLI dispatch slice at `next f622434c`
- `octo_network` Layer B substrate (ONE NEW MODULE per Phase 7 RFC-0011-o SlashBridge precedent: `crates/octo-network/src/mon/heartbeat.rs`)
- Layer C CLI dispatch envelope/handler pattern (Phase 5 RFC-0011-m at `next 346f10cc`)

## Acceptance Criteria

- [x] `NetworkAction::Heartbeat { action: NetworkHeartbeatAction }` clap variant lands in commands/network.rs
- [x] `NetworkHeartbeatAction` enum lands with `Probe(HeartbeatProbeArgs)` variant
- [x] `HeartbeatProbeArgs` lands with `peer_did` (positional) + `timeout_ms: u16` (default 5000) + `json: bool` fields
- [x] clap u16 overflow pre-dispatch rejection per Phase 5 RFC-0011-m precedent (verified via tv_net14_3 max value test)
- [x] `NetworkHeartbeatProbeOutput` envelope lands (peer_did + result_label + rtt_ms + unreachable_reason_label + unreachable_reason_detail + timeout_ms)
- [x] `network_heartbeat_probe` handler lands with `heartbeat_probe_registry` marker guard
- [x] `heartbeat_probe_registry` runtime marker lands (returns `false` for Phase 14 additive-type-only; per-extension impl crates Layer D OUT OF SCOPE)
- [x] Handler invokes `Heartbeat::probe()` which returns `Timeout` unconditionally for Phase 14 per RFC-0011-v §Heartbeat Probe semantics
- [x] All 3 test vectors use `NetworkAction` + `NetworkHeartbeatAction` match pattern without unreachable wildcards (single-variant enum)
- [x] Dispatch arm lands in `network_dispatch`
- [x] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean (NO regression of existing 439 tests)
- [x] `cargo test -p octo-cli --lib` green (442/442; +3 NEW test vectors)
- [x] Layer discipline preserved (Layer C only; zero Layer A or Layer B change in this commit)
- [x] 3 NEW test vectors cover default timeout + explicit 1000ms timeout + clap u16 max timeout

## Out of Scope

- Substrate additive type (paired mission `0011-h-s-a-heartbeat-probe` covers that surface; landed at `next ffd3b9bf`)
- Live transport-level probe adapter (per-extension impl crates `substrate-ext-heartbeat-transport-*` in Layer D, follow-on missions)
- Per-extension registry wiring (CLI dispatch uses marker; per-extension init fn OUT OF SCOPE for Phase 14)
- Wire format versioning (deferred to RFC-0011-h §Future Work items F8 + F9)
- Real probe logic (Phase 14 substrate returns Timeout unconditionally; real probe in follow-on Layer D adapter mission)

## Notes

RFC-0011-v Phase 14 G17 CLI surface exposed through `octo network heartbeat probe` subcommand. Substrate-faithful to ONE NEW module at `crates/octo-network/src/mon/heartbeat.rs` per Phase 7 RFC-0011-o SlashBridge NEW module precedent. CLI dispatch slice paired with substrate companion YAML via the Phase 5 RFC-0011-m 5-commit pattern (stub fill-in → substrate slice → YAML Claimed → CLI dispatch → YAMLs Completed). Slot 89 `NetworkSubstrateUnavailable` REUSE per Phase 6 precedent (0 NEW OctoCliError variants). Per-extension crate pattern preserved (additive types in Layer B; concrete per-transport heartbeat adapter crates in separate Layer D follow-on missions). Layer A frozen preserved. Phase 14 closes the final G-row G17 in RFC-0011-h §Substrate-Additions Companion Missions table; paired with stale-stub sweep commit per Phase 6 `9a903993` precedent.
