# 0011-h-network-envelope — `octo network envelope` CLI surface (RFC-0011-u Phase 13 G16a plus G16b)

## Status

Completed (2026-09-20) — CLI dispatch slice for RFC-0011-u Phase 13 G16a envelope-inspector plus G16b forward-envelope amendment LANDED at `next 344bd8b5`. CLI mission YAML CREATED at CLI dispatch slice time per user decision. Pairs with substrate companion YAMLs `0011-h-s-a-envelope-inspector` plus `0011-h-s-a-forward-envelope` (both substrate slice at `next 9c03f6dc`, both claimed transition at `next 59e8c43e`, both completed transition paired). 2 NEW subcommands + 2 NEW output envelopes + 6 NEW test vectors `tv_net13_1` through `tv_net13_6`. Layer C CLI dispatch only; substrate additive types live at `crates/octo-network/src/mon/envelope_inspector.rs` plus `crates/octo-network/src/mon/forward_envelope.rs`.

## RFC

RFC-0011-u Phase 13 G16a plus G16b — `envelope inspect` plus `envelope forward`.

## Summary

EXPOSES the RFC-0011-u Phase 13 substrate additive types (`EnvelopeInspector::inspect()` plus `EnvelopeInspector::from_fields()` plus `ForwardEnvelope::build()` plus `ForwardEnvelope::wire_bytes()`) through the `octo network envelope` CLI surface. Read-only `inspect` subcommand plus mutating `forward` subcommand with mutating gate. Layer C (octo-cli dispatch) only; substrate additive types live in `crates/octo-network` Layer B per Phase 7 RFC-0011-o SlashBridge NEW module precedent.

### Subcommand surface

```
octo network envelope inspect <envelope_id_hex> [--json]
octo network envelope forward <envelope_id_hex> --destination-peer-id-hex <peer_id_hex> --ttl-epochs <N> [--dry-run] [--confirm-acknowledge]
```

### CLI substrate mapping

| Subcommand       | Substrate method                                  | Format / arg             | Output envelope               |
| ---------------- | ------------------------------------------------- | ------------------------ | ----------------------------- |
| `envelope inspect` | `EnvelopeInspector::from_fields(...)` (projection) | `ascii` (default) / `json` | `NetworkEnvelopeInspectOutput` |
| `envelope forward` | `ForwardEnvelope::build(...)` + `wire_bytes()`    | mutating gate            | `NetworkEnvelopeForwardOutput` |

### File changes

- `crates/octo-cli/src/commands/network.rs` — modified (1 file changed, 394 insertions, 0 deletions)

  - `NetworkAction::Envelope { action: NetworkEnvelopeAction }` clap variant
  - `NetworkEnvelopeAction` `#[non_exhaustive]` enum (Inspect + Forward)
  - `EnvelopeInspectArgs` (envelope_id_hex + json)
  - `EnvelopeForwardArgs` (envelope_id_hex + destination_peer_id_hex + ttl_epochs + dry_run + confirm_acknowledge)
  - `NetworkEnvelopeInspectOutput` envelope struct (envelope_id_hex + envelope_kind + creator_did_hex + creation_epoch + ttl_epochs + format)
  - `NetworkEnvelopeForwardOutput` envelope struct (forward_envelope_hex + source_envelope_id_hex + destination_peer_id_hex + ttl_epochs + construction_epoch + broadcast_dispatched)
  - `network_envelope_inspect` handler with `envelope_inspect_registry` marker guard
  - `network_envelope_forward` handler with `envelope_forward_registry` marker guard + mutating gate
  - `envelope_inspect_registry` runtime marker
  - `envelope_forward_registry` runtime marker
  - Dispatch arm in `network_dispatch`
  - 6 NEW test vectors `tv_net13_1` through `tv_net13_6`

### Test vectors

| Vector       | Subcommand                                          | Coverage                                                          |
| ------------ | --------------------------------------------------- | ----------------------------------------------------------------- |
| `tv_net13_1` | `envelope inspect <envelope_id_hex>`                | default format (ascii) parses cleanly                             |
| `tv_net13_2` | `envelope inspect <envelope_id_hex> --json`         | json flag parses cleanly                                          |
| `tv_net13_3` | `envelope forward ... --destination-peer-id-hex ... --ttl-epochs 100` | destination plus ttl-epochs parses cleanly          |
| `tv_net13_4` | `envelope forward ... --dry-run`                    | dry-run flag parses cleanly (no broadcast)                        |
| `tv_net13_5` | `envelope forward ... --confirm-acknowledge`        | confirm-acknowledge flag parses cleanly (mutating gate)           |
| `tv_net13_6` | `envelope forward ...` (no confirm)                 | parses at clap level; handler enforces mutating gate at runtime   |

### Dependencies

- RFC-0011-u Phase 13 G16a envelope-inspector plus G16b forward-envelope amendment Draft at `next 296d9a1e`
- Phase 13 G16a plus G16b substrate stub fill-in at `next dbaa3e82`
- Phase 13 G16a plus G16b substrate slice at `next 9c03f6dc`
- Phase 13 G16a plus G16b paired-YAML Claimed transition at `next 59e8c43e`
- Phase 13 G16a plus G16b CLI dispatch slice at `next 344bd8b5`
- `octo_network` Layer B substrate (TWO NEW MODULES per Phase 7 RFC-0011-o SlashBridge precedent: `crates/octo-network/src/mon/envelope_inspector.rs` + `crates/octo-network/src/mon/forward_envelope.rs`)
- Layer C CLI dispatch envelope/handler pattern (Phase 5 RFC-0011-m at `next 346f10cc`)

## Acceptance Criteria

- [x] `NetworkAction::Envelope { action: NetworkEnvelopeAction }` clap variant lands in commands/network.rs
- [x] `NetworkEnvelopeAction` `#[non_exhaustive]` enum lands with `Inspect(EnvelopeInspectArgs)` + `Forward(EnvelopeForwardArgs)` variants
- [x] `EnvelopeInspectArgs` lands with `envelope_id_hex` (positional) + `json: bool` fields
- [x] `EnvelopeForwardArgs` lands with `envelope_id_hex` (positional) + `destination_peer_id_hex` + `ttl_epochs: u64` + `dry_run: bool` + `confirm_acknowledge: bool` fields
- [x] `NetworkEnvelopeInspectOutput` envelope lands (envelope_id_hex + envelope_kind + creator_did_hex + creation_epoch + ttl_epochs + format)
- [x] `NetworkEnvelopeForwardOutput` envelope lands (forward_envelope_hex + source_envelope_id_hex + destination_peer_id_hex + ttl_epochs + construction_epoch + broadcast_dispatched)
- [x] `network_envelope_inspect` handler lands with `envelope_inspect_registry` marker guard
- [x] `network_envelope_forward` handler lands with `envelope_forward_registry` marker guard + mutating gate (per Phase 4 RFC-0011-l precedent)
- [x] `envelope_inspect_registry` runtime marker lands (returns `false` for Phase 13 additive-type-only; per-extension impl crates Layer D OUT OF SCOPE)
- [x] `envelope_forward_registry` runtime marker lands (returns `false` for Phase 13 additive-type-only; per-extension impl crates Layer D OUT OF SCOPE)
- [x] Both handlers use `parse_32_byte_hex` shared helper with field-name arg (pastejacking defense per Phase 5 RFC-0011-m precedent)
- [x] `ForwardEnvelopeError` non-exhaustive match in `network_envelope_forward` carries wildcard arm mapping to `OctoCliError::ConfirmationRequired`
- [x] All 6 test vectors use `NetworkAction` + `NetworkEnvelopeAction` match-with-wildcard pattern (Rust 2024 non-exhaustive match requirement)
- [x] Dispatch arm lands in `network_dispatch`
- [x] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean (NO regression of existing 433 tests)
- [x] `cargo test -p octo-cli --lib` green (439/439; +6 NEW test vectors)
- [x] Layer discipline preserved (Layer C only; zero Layer A or Layer B change in this commit)
- [x] 6 NEW test vectors cover inspect default + inspect json + forward destination + ttl + forward dry-run + forward confirm-acknowledge + forward no-confirm-parses-but-handler-gates

## Out of Scope

- Substrate additive types (paired missions `0011-h-s-a-envelope-inspector` + `0011-h-s-a-forward-envelope` cover that surface; both landed at `next 9c03f6dc`)
- Live envelope store adapter (per-extension impl crates `substrate-ext-envelope-store-*` in Layer D, follow-on missions)
- Per-extension forward envelope adapter (per-extension impl crates `substrate-ext-envelope-forwarder-*` in Layer D, follow-on missions)
- Per-extension registry wiring (CLI dispatch uses markers; per-extension init fn OUT OF SCOPE for Phase 13)
- Wire format versioning (deferred to RFC-0011-h §Future Work items F8 + F9)
- Live network propagation (Phase 13 substrate operates on the in-memory snapshot only; real propagation in follow-on Layer D adapter mission)

## Notes

RFC-0011-u Phase 13 G16a plus G16b CLI surface exposed through `octo network envelope inspect` + `octo network envelope forward` subcommands. Substrate-faithful to TWO NEW modules at `crates/octo-network/src/mon/envelope_inspector.rs` + `crates/octo-network/src/mon/forward_envelope.rs` per Phase 7 RFC-0011-o SlashBridge NEW module precedent. CLI dispatch slice paired with TWO substrate companion YAMLs via the Phase 5 RFC-0011-m 5-commit pattern (stub fill-in → substrate slice → YAML Claimed → CLI dispatch → YAMLs Completed). Slot 89 `NetworkSubstrateUnavailable` REUSE per Phase 6 precedent (0 NEW OctoCliError variants). Per-extension crate pattern preserved (additive types in Layer B; concrete per-envelope-source + per-forward-target adapter crates in separate Layer D follow-on missions). Layer A frozen preserved.
