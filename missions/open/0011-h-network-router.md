# 0011-h-network-router — `router` subcommands per RFC-0011-p Phase 8

## Status

Completed (2026-09-20) — CLI dispatch slice LANDED at `next cee393d2` + substrate companion mission G10 LANDED at `next 52a99360` (stub fill-in at `next c0c3539f`, paired-YAML Claimed at `next 334deb71`). Substrate-first ordering preserved per [[no-phantom-mission-pointers]]: G10 substrate (`QuotaRouterNode` struct + `RouterStatus` enum + `QuotaRouterNodeAccess` trait + `status()` + `peer_capacity()` methods) landed BEFORE CLI dispatch. CLI dispatch slice atop the substrate adds: `NetworkAction::Router` clap variant + nested `NetworkRouterAction` enum + `RouterStatusArgs` + `RouterPeersArgs` (with `parse_32_byte_hex` shared helper per RFC-0011-h §Pastejacking Defense pattern; mixed-case hex rejected) + 2 output envelopes (`NetworkRouterStatusOutput` + `NetworkRouterPeersOutput`) + dispatch arm + 2 handler functions (`network_router_status` + `network_router_peers`) + `router_node_registry` helper + `status_label` helper. 6 Phase 8 test vectors (`tv_net8_1` through `tv_net8_6`) added to `crates/octo-cli/src/commands/network.rs`. 415/415 octo-cli tests pass (was 409). Closes the DEFERRED gap from RFC-0011-h §Substrate-Additions row G10.

## RFC

RFC-0011-h §Implementation Phases Phase 8 + RFC-0011-p §Subcommand Taxonomy Phase 8 rows + RFC-0870 Distributed Quota Router Network + RFC-0011-h §Output Envelope Phase 8 rows

## Summary

CLI surface for `octo network router status` + `octo network router peers <peer_node_id_hex>`. Surfaces quota router node operational state + per-peer capacity to operators. Per-extension transport impl crates (substrate-ext-quota-router-*) are OUT OF SCOPE per per-extension crate pattern per [[cipherocto-design-principles]] §User extensibility.

### Subcommands

#### `octo network router status`

Read-only projection of the local quota router node operational status. Returns `NetworkRouterStatusOutput` envelope: `node_id_hex` + `status` (lowercase: healthy | degraded | offline) + `reachable_peer_count` + `total_peer_count` + `last_sync_epoch`. Infallible; substrate-faithful per RFC-0011-p §Subcommand Taxonomy Phase 8.

```bash
octo network router status [--json]
```

#### `octo network router peers <peer_node_id_hex>`

Read-only projection of remaining quota capacity for a specific peer. Returns `NetworkRouterPeersOutput` envelope: `peer_node_id_hex` (input echo) + `capacity` (Option<u64>; None = peer not in local routing table) + `reachable` (bool). Infallible; substrate-faithful per RFC-0011-p §Subcommand Taxonomy Phase 8.

```bash
octo network router peers <peer_node_id_hex> [--json]
```

`peer_node_id` is 32-byte canonical identifier accepted as 64 hex chars (lowercase OR uppercase); mixed-case rejected by `parse_32_byte_hex` pastejacking defense (per RFC-0011-h §Pastejacking Defense pattern via `parse_router_peer_node_id_hex` 1-arg wrapper).

## Test vectors (RFC-0011-p §Test Vectors Phase 8)

- `tv_net8_1_router_status_parses_with_no_args` — router status subcommand parses cleanly
- `tv_net8_2_router_status_json_flag_parses` — router status --json flag parses cleanly
- `tv_net8_3_router_status_default_envelope_is_offline` — default node projection is Offline + empty peer counts
- `tv_net8_4_router_peers_parses_with_lowercase_hex` — router peers subcommand parses cleanly with lowercase hex peer_node_id
- `tv_net8_5_router_peers_rejects_mixed_case_hex` — pastejacking defense rejects mixed-case hex via `parse_32_byte_hex` shared helper
- `tv_net8_6_router_peers_accepts_uppercase_only_hex` — pastejacking defense accepts uppercase-only hex via `parse_32_byte_hex` shared helper

## Dependencies

- RFC-0011-p Draft at `next 92b69be3`
- Substrate companion mission G10 at `missions/open/0011-h-s-a-quota-router-node.md` (paired Open → Claimed → Completed)
- Substrate slice at `next 52a99360`

## Out of Scope

- Per-extension concrete impls (BLE/USB/TCP/QUIC/HID quota router adapters) — each lands in its own Layer D per-extension crate in follow-on missions
- G10 QuotaRouterNode persistence — in-memory struct only; persistence adapter in follow-on Layer D adapter mission
- Real quota routing logic — struct projection only; routing engine in follow-on Layer D adapter mission

## Verification

- `cargo clippy -p octo-cli --all-targets -- -D warnings` clean (zero warnings)
- `cargo clippy -p octo-network --all-targets -- -D warnings` clean (zero warnings)
- `cargo test -p octo-cli --lib` passes 415/415 (was 409 before, plus 6 Phase 8 vectors tv_net8_1 through tv_net8_6)
- `cargo test -p octo-network --lib quota::` passes 6/6 substrate unit tests
- `cargo fmt --all` clean

## Layer discipline

Zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle. Layer B substrate addition (`QuotaRouterNode` struct + `RouterStatus` enum + `QuotaRouterNodeAccess` trait + methods) + Layer C CLI dispatch (subcommand + envelopes + handler). Per-extension crate pattern preserved.
