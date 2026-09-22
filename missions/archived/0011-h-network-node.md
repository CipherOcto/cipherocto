# 0011-h-network-node — `node` subcommands per RFC-0011-q Phase 9

## Status

Completed (2026-09-20) — CLI dispatch slice LANDED at `next 54ac266d` + substrate companion mission G11 LANDED at `next 8c11d683` (stub fill-in at `next c0a33e5e`, paired-YAML Claimed at `next 7a3f0bd5`). Substrate-first ordering preserved per [[no-phantom-mission-pointers]]: G11 substrate (`SpecializedNodeRecord` struct + `NodeClass` enum + `SpecializedNodeRecordAccess` trait + `HolderDid` newtype + `SpecializedNodeError` enum + `load()` + `bind_to_did()` methods) landed BEFORE CLI dispatch. CLI dispatch slice atop the substrate adds: `NetworkAction::Node` clap variant + nested `NetworkNodeAction` enum + `NodeShowArgs` + `NodeBindArgs` (with `parse_32_byte_hex` shared helper per RFC-0011-h §Pastejacking Defense pattern; mixed-case hex rejected) + 2 output envelopes (`NetworkNodeShowOutput` + `NetworkNodeBindOutput`) + `SpecializedNodeRecordProjection` struct + dispatch arm + 2 handler functions (`network_node_show` + `network_node_bind`) + `specialized_node_registry` helper + `node_class_label` helper. 6 Phase 9 test vectors (`tv_net9_1` through `tv_net9_6`) added to `crates/octo-cli/src/commands/network.rs`. 421/421 octo-cli tests pass (was 415). Closes the DEFERRED gap from RFC-0011-h §Substrate-Additions row G11.

## RFC

RFC-0011-h §Implementation Phases Phase 9 + RFC-0011-q §Subcommand Taxonomy Phase 9 rows + RFC-0871 Specialized Node Protocol Envelope + RFC-0011-h §Output Envelope Phase 9 rows

## Summary

CLI surface for `octo network node show <node_id_hex>` (read) + `octo network node bind <node_id_hex> --holder-did <did>` (mutating; --dry-run + --apply + --confirm-acknowledge). Surfaces specialized node record state + controlled mutating bind to operators. Per-extension transport impl crates (substrate-ext-specialized-node-*) are OUT OF SCOPE per per-extension crate pattern per [[cipherocto-design-principles]] §User extensibility.

### Subcommands

#### `octo network node show <node_id_hex>`

Read-only projection of a specialized node record by node_id. Returns `NetworkNodeShowOutput` envelope: `node_id_hex` (input echo) + `record` (`Option<SpecializedNodeRecordProjection>`; None = node not in local registry). `record` projection includes: `node_id_hex` + `holder_did` (Option<String>; None pre-bind) + `node_class` (lowercase: builder | provider | storage | bandwidth | orchestrator) + `creation_epoch` + `metadata` (BTreeMap-backed; deterministic key order). Infallible; substrate-faithful per RFC-0011-q §Subcommand Taxonomy Phase 9.

```bash
octo network node show <node_id_hex> [--json]
```

`node_id` is 32-byte canonical identifier accepted as 64 hex chars (lowercase OR uppercase); mixed-case rejected by `parse_32_byte_hex` pastejacking defense (per RFC-0011-q §Pastejacking Defense pattern via `parse_specialized_node_id_hex` 1-arg wrapper).

#### `octo network node bind <node_id_hex> --holder-did <did>`

Mutating bind of a node to a holder DID (reversible per RFC-0011-h §Confirmation Flag). Returns `NetworkNodeBindOutput` envelope: `node_id_hex` + `holder_did` + `applied` (false = dry-run preview only). `bind` is mutating but reversible; `--dry-run` is default; `--apply + --confirm-acknowledge` required to lift dry-run (clap `conflicts_with` + `requires` per RFC-0011-h §Confirmation Flag Phase 9).

```bash
octo network node bind <node_id_hex> --holder-did <did> [--dry-run | --apply --confirm-acknowledge] [--json]
```

`holder_did` is canonical `did:octo:0x<hex>` format (no substrate-side validation; CLI parse layer accepts any non-empty string; per-extension impl crates validate at substrate layer). Errors mapped to exit 89 `NetworkSubstrateUnavailable` per RFC-0011-q §Error Handling: `NotFound` (transient) + `AlreadyBound` (permanent) + `InvalidDid(String)` + `Internal(String)` — each variant carries a distinct message so operators can distinguish from CLI output alone.

## Test vectors (RFC-0011-q §Test Vectors Phase 9)

- `tv_net9_1_node_show_parses_with_lowercase_hex` — node show subcommand parses cleanly with lowercase hex node_id
- `tv_net9_2_node_show_json_flag_parses` — node show --json flag parses cleanly
- `tv_net9_3_node_show_default_registry_returns_none` — default registry returns None; node_class_label + BTreeMap metadata + NodeClass::Provider projection substrate-faithful
- `tv_net9_4_node_bind_apply_parses_with_confirm_acknowledge` — node bind --apply --confirm-acknowledge parses cleanly with hex node_id + --holder-did
- `tv_net9_5_node_bind_apply_without_confirm_acknowledge_rejected` — parse-time rejection of --apply without --confirm-acknowledge (clap `requires` constraint per RFC-0011-q §Confirmation Flag)
- `tv_net9_6_node_bind_rejects_mixed_case_hex` — pastejacking defense rejects mixed-case hex via `parse_32_byte_hex` shared helper

## Dependencies

- RFC-0011-q Draft at `next c0287128`
- Substrate companion mission G11 at `missions/open/0011-h-s-a-specialized-node-record.md` (paired Open → Claimed → Completed)
- Substrate slice at `next 8c11d683`

## Out of Scope

- Per-extension concrete impls (BLE/USB/TCP/QUIC/HID specialized node adapters) — each lands in its own Layer D per-extension crate in follow-on missions
- G11 SpecializedNodeRecord binding persistence — in-memory mutation only; persistence adapter in follow-on Layer D adapter mission
- Real DID registry integration — uses local `HolderDid` newtype at substrate; `octo-did` cross-crate dep deferred per RFC-0011-h §Out of Scope
- Unbind flow — rebind via `AlreadyBound` error path only; unbind command OUT OF SCOPE for this trait-only phase

## Verification

- `cargo clippy -p octo-cli --all-targets -- -D warnings` clean (zero warnings)
- `cargo clippy -p octo-network --all-targets -- -D warnings` clean (zero warnings)
- `cargo test -p octo-cli --lib` passes 421/421 (was 415 before, plus 6 Phase 9 vectors tv_net9_1 through tv_net9_6)
- `cargo test -p octo-network --lib` passes 1489/1489 (was 1483 before, plus 6 Phase 9 substrate tests tv_phase9_substrate_1 through tv_phase9_substrate_6)
- `cargo fmt --all` clean

## Layer discipline

Zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle. Layer B substrate addition (`SpecializedNodeRecord` struct + `NodeClass` enum + `SpecializedNodeRecordAccess` trait + `HolderDid` newtype + `SpecializedNodeError` enum + methods) + Layer C CLI dispatch (subcommand + envelopes + handler). Per-extension crate pattern preserved. `bind_to_did` uses `&self` (not `&mut self`) for object-safe dispatch behind `Arc<dyn SpecializedNodeRecordAccess>` per RFC-0011-h §User extensibility registry pattern; concrete per-extension impl crates use interior mutability (`Mutex`/`RwLock`) internally.
