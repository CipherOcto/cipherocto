# 0011-h-network-slash-bridge — `slash-bridge` subcommands per RFC-0011-h Phase 7

## Status

Completed (2026-09-20) — CLI dispatch slice LANDED at `next f3b9f48e`. Paired substrate mission G9 LANDED at `next 8599f5c8`. Substrate-first ordering preserved per [[no-phantom-mission-pointers]]: G9 substrate (`SlashBridge` trait + `BridgedSlash` + `BridgeReceipt` + `BridgeError` types) landed BEFORE CLI dispatch. CLI dispatch slice atop the substrate adds: `NetworkAction::SlashBridge` clap variant + nested `NetworkSlashBridgeAction` enum + `SlashBridgeListArgs` + `SlashBridgePropagateArgs` (with `parse_gateway_id_hex` pastejacking defense per RFC-0011-h §Confirmation Flag) + 4 output envelopes (`NetworkSlashBridgeListOutput` + `BridgedSlashProjection` + `NetworkSlashBridgePropagateOutput` + `BridgeReceiptProjection`) + dispatch arm + `network_slash_bridge_list` + `network_slash_bridge_propagate` handlers + `slash_bridge_registry` helper. 6 Phase 7 test vectors (`tv_net7_1` through `tv_net7_6`) added to `crates/octo-cli/src/commands/network.rs`. 408/408 octo-cli tests pass (was 402). Closes the DEFERRED gap from RFC-0011-h §Substrate-Additions row G9.

## RFC

RFC-0011-h §Implementation Phases Phase 7 + RFC-0011-o §Subcommand Taxonomy Phase 7 rows + RFC-0855p-b §Slash Substrate + RFC-0855 §8.4 External Reputation Bridge

## Summary

CLI surface for `octo network slash-bridge list` + `octo network slash-bridge propagate <slash_envelope_id_hex>`. Bridges local `SlashStore` (RFC-0011-b Phase 2 substrate) to external reputation substrates via per-extension crate pattern per [[cipherocto-design-principles]] §User extensibility. Per-extension transport impl crates (substrate-ext-bridge-*) are OUT OF SCOPE.

### Subcommands

#### `octo network slash-bridge list`

Read-only projection of all bridged slashes currently held by the local `SlashBridge` trait implementation. Returns `NetworkSlashBridgeListOutput` envelope: `total` (count) + `slashes` (Vec of `BridgedSlashProjection`). Infallible; substrate-faithful per RFC-0011-o §Subcommand Taxonomy Phase 7.

```bash
octo network slash-bridge list [--json]
```

#### `octo network slash-bridge propagate <slash_envelope_id_hex>`

Mutating but reversible per RFC-0011-o §Confirmation Flag. Propagates a slash envelope to its external destination. Returns `NetworkSlashBridgePropagateOutput` envelope: `dry_run` flag + `receipt` (`BridgeReceiptProjection`) + `slash_envelope_id_hex` (input echo). Idempotent on `slash_envelope_id` per RFC-0855 §8.4 External Reputation Bridge — double-propagate yields same receipt.

```bash
octo network slash-bridge propagate <slash_envelope_id_hex> [--dry-run] [--confirm-acknowledge] [--json]
```

`--dry-run` defaults to `true` per RFC-0011-h §Confirmation Flag. Apply (reversible write) requires `--confirm-acknowledge`. `slash_envelope_id` is 32-byte canonical identifier (RFC-0855p-b §Wire Format) accepted as 64 lowercase hex chars; mixed-case rejected by `parse_gateway_id_hex` pastejacking defense.

## Test vectors (RFC-0011-o §Test Vectors Phase 7)

- `tv_net7_1_slash_bridge_list_parses_with_no_args` — list subcommand parses cleanly
- `tv_net7_2_slash_bridge_list_json_flag_parses` — list --json flag parses cleanly
- `tv_net7_3_slash_bridge_list_empty_envelope_total_zero` — list envelope projection is substrate-faithful (empty registry returns total=0)
- `tv_net7_4_slash_bridge_propagate_parses_with_dry_run_default` — propagate subcommand parses cleanly with dry_run default true
- `tv_net7_5_slash_bridge_propagate_confirm_acknowledge_parses` — propagate --confirm-acknowledge parses cleanly
- `tv_net7_6_slash_bridge_propagate_rejects_mixed_case_hex` — pastejacking defense rejects mixed-case hex

## Dependencies

- RFC-0011-o Draft at `next 27fe1a48`
- Substrate companion mission G9 at `missions/open/0011-h-s-a-slash-bridge-trait.md` (paired Open → Claimed → Completed)
- Substrate slice at `next 8599f5c8`

## Out of Scope

- Per-extension concrete impls (BLE/USB/TCP/QUIC/HID bridges) — each lands in its own Layer D per-extension crate in follow-on missions
- G9 SlashBridge propagation transaction semantics — `propagate_to` returns `BridgeReceipt` stub; real signature aggregation in follow-on Layer D adapter mission
- Wire format versioning — deferred per RFC-0011-h §Future Work items F8 + F9

## Verification

- `cargo clippy -p octo-cli --all-targets -- -D warnings` clean (zero warnings)
- `cargo clippy -p octo-network --all-targets -- -D warnings` clean (zero warnings)
- `cargo test -p octo-cli --lib` passes 408/408 (was 402 before, plus 6 Phase 7 vectors)
- `cargo test -p octo-network --lib mon::slash_bridge` passes 5/5 substrate unit tests
- `cargo fmt --all` clean

## Layer discipline

Zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle. Layer B substrate addition (trait + types) + Layer C CLI dispatch (subcommand + envelopes + handler). Per-extension crate pattern preserved.
