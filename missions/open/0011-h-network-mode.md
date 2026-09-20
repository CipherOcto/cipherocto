# 0011-h-network-mode — `mode` subcommands per RFC-0011-h Phase 2

## Status

Completed (2026-09-20) — CLI dispatch LANDED at `next 8649ca4d`. RFC-0011-j Phase 2 `mode` subcommand surface wired to substrate.

## RFC

RFC-0011-h §Implementation Phases Phase 2 + RFC-0011-j §Subcommand Taxonomy

## Summary

CLI surface for `octo network mode show` + `octo network mode set` per RFC-0011-j Phase 2.

### Subcommands

- `mode show` — reads `BootstrapConfig::from_toml` and emits `octo.network.mode.show.v1` envelope
- `mode set --bootstrap-mode <direct|tor_only|tor_with_ip_fallback> --listen-addr <addr> --target-peers <n>` — writes via `BootstrapConfig::save_toml` after 3-flag confirmation

### Implementation

CLI dispatch landed 2026-09-20 at `next 8649ca4d`:

- `ModeShowArgs` + `ModeSetArgs` clap structs (Layer C)
- `ModeSetArgs::bootstrap_mode` flag (renamed from `mode` to avoid collision with global `--mode OperatorMode` flag)
- `mode_show` + `mode_set` handlers in `crates/octo-cli/src/commands/network.rs`
- `NetworkModeShowOutput` + `NetworkModeSetOutput` envelope payloads
- `NetworkConfigParseFailed` (slot 82) error mapping for `BootstrapConfigError`
- 4 test vectors: `tv_net2_1`, `tv_net2_2`, `tv_net2_3`, `tv_net2_14`

## Acceptance Criteria

- [x] CLI surface for the listed subcommands per RFC-0011-h §Subcommand Taxonomy
- [x] Test vectors per RFC-0011-h §Test Vectors for the listed subcommands (4 vectors added)
- [x] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-cli --lib` green (360/360)
- [x] Layer discipline preserved (CLI Layer C only; zero Layer A change)
- [x] Substrate-faithful boundary: every call crosses typed façade per [[cipherocto-design-principles]] §Stable Abstractions Principle

## Dependencies

- Hard sequencing: RFC-0011-h must be Accepted
- Hard sequencing: substrate companion `0011-h-s-a-bootstrap-orchestrator` (G1) must be LANDED — landed at `next c9121aa5`

## Out of Scope

- Substrate additions (covered by paired `0011-h-s-a-*` companion missions — G1 landed)
- `octo network bootstrap` orchestrator (Phase 6 mission `0011-h-s-a-bootstrap-orchestrator-v2` / G26 deferred)

## Notes

Phase 2 IMPLEMENTATION closed at `next 8649ca4d`. CLI dispatch wired substrate-faithfully per RFC-0011-j §Output Envelope. Phase 6 G26 (BootstrapOrchestrator lifecycle struct) deferred to RFC-0011-n; this Phase 2 mission does NOT depend on G26 — only on the G1 parser/saver which already landed.
