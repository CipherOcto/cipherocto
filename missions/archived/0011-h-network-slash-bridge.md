# 0011-h-network-slash-bridge — `slash-bridge` subcommands per RFC-0011-h Phase 7

## Status

Completed (2026-09-20) — CLI dispatch slice LANDED at `next f3b9f48e` + R1.5 fix sweep LANDED at `next 172957d7` + R2.5 fix sweep LANDED at `next 25a625c9`. Paired substrate mission G9 LANDED at `next 8599f5c8` (stub fill-in at `next 3d81ecad`, YAMLs Completed paired at `next 288be12c`). Substrate-first ordering preserved per [[no-phantom-mission-pointers]]: G9 substrate (`SlashBridge` trait + `BridgedSlash` + `BridgeReceipt` + `BridgeError` types) landed BEFORE CLI dispatch. CLI dispatch slice atop the substrate adds: `NetworkAction::SlashBridge` clap variant + nested `NetworkSlashBridgeAction` enum + `SlashBridgeListArgs` + `SlashBridgePropagateArgs` (with `parse_32_byte_hex` shared helper per RFC-0011-h §Pastejacking Defense + R2.5 parser migration; mutually-exclusive `--dry-run` / `--apply` flag group per R1.5 clap `conflicts_with`; `--apply` requires `--confirm-acknowledge` via clap `requires` attribute) + 4 output envelopes (`NetworkSlashBridgeListOutput` + `BridgedSlashProjection` + `NetworkSlashBridgePropagateOutput` + `BridgeReceiptProjection`) + dispatch arm + `network_slash_bridge_list` + `network_slash_bridge_propagate` handlers + `slash_bridge_registry` helper. 7 Phase 7 test vectors (`tv_net7_1` through `tv_net7_6b`) added to `crates/octo-cli/src/commands/network.rs`. 409/409 octo-cli tests pass (was 402). Closes the DEFERRED gap from RFC-0011-h §Substrate-Additions row G9.

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

`--dry-run` defaults to `false` per RFC-0011-h §Confirmation Flag (R1.5 mutually-exclusive flag group). `--dry-run` and `--apply` are mutually exclusive (clap `conflicts_with`); `--apply` requires `--confirm-acknowledge` (clap `requires`, parse-time rejection). Both flags opt-in; operator MUST choose one. `slash_envelope_id` is 32-byte canonical identifier (RFC-0855p-b §Wire Format) accepted as 64 hex chars (lowercase OR uppercase); mixed-case rejected by `parse_32_byte_hex` pastejacking defense (R2.5 migration).

## Test vectors (RFC-0011-o §Test Vectors Phase 7)

- `tv_net7_1_slash_bridge_list_parses_with_no_args` — list subcommand parses cleanly
- `tv_net7_2_slash_bridge_list_json_flag_parses` — list --json flag parses cleanly
- `tv_net7_3_slash_bridge_list_empty_envelope_total_zero` — list envelope projection is substrate-faithful (empty registry returns total=0)
- `tv_net7_4_slash_bridge_propagate_parses_with_apply_and_confirm` — propagate subcommand parses cleanly with --apply --confirm-acknowledge (R1.5)
- `tv_net7_5_slash_bridge_propagate_apply_rejects_missing_confirm_at_parse_time` — propagate --apply WITHOUT --confirm-acknowledge rejected at parse-time by clap (R1.5)
- `tv_net7_6_slash_bridge_propagate_rejects_mixed_case_hex` — pastejacking defense rejects mixed-case hex via `parse_32_byte_hex` shared helper (R2.5)
- `tv_net7_6b_slash_bridge_propagate_accepts_uppercase_only_hex` — pastejacking defense accepts uppercase-only hex via `parse_32_byte_hex` shared helper (R2.5)

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
- `cargo test -p octo-cli --lib` passes 409/409 (was 402 before, plus 7 Phase 7 vectors tv_net7_1 through tv_net7_6b)
- `cargo test -p octo-network --lib mon::slash_bridge` passes 5/5 substrate unit tests
- `cargo fmt --all` clean

## Layer discipline

Zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle. Layer B substrate addition (trait + types) + Layer C CLI dispatch (subcommand + envelopes + handler). Per-extension crate pattern preserved.
