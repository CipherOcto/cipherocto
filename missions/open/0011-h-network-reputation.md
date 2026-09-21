# 0011-h-network-reputation — `octo network reputation` CLI surface (RFC-0011-r Phase 10 G13)

## Status

Completed (2026-09-20) — CLI dispatch slice for RFC-0011-r Phase 10 G13 reputation-store amendment LANDED at `next 026c2e5a`. CLI mission YAML CREATED at CLI dispatch slice time per user decision. Pairs with substrate companion YAML `0011-h-s-a-reputation-store` (substrate slice at `next c95fd8cb`, claimed transition at `next 9159e613`, completed transition paired). 2 NEW subcommands + 2 NEW output envelopes + 6 NEW test vectors `tv_net10_1` through `tv_net10_6`. Layer C CLI dispatch only; substrate trait lives at `crates/octo-reputation/src/store/mod.rs` §ReputationStore trait.

## RFC

RFC-0011-r §Subcommand Taxonomy Phase 10 G13 — `reputation list` + `reputation show`.

## Summary

EXPOSES the RFC-0011-r Phase 10 substrate additions (REPUTATION list/peer queries) through the `octo network reputation` CLI surface. Read-only subcommands; substrate-faithful projection of `ReputationFilter` + `PeerReputation`. Layer C (octo-cli dispatch) only; substrate trait extension lives in `crates/octo-reputation` Layer B.

### Subcommand surface

```
octo network reputation list [--filter all|above-score|below-score] [--threshold <N>] [--json]
octo network reputation show <peer_did> [--json]
```

### CLI substrate mapping

| Subcommand | Substrate method | Filter / arg | Output envelope |
| --- | --- | --- | --- |
| `reputation list` | `ReputationStore::list(filter)` | `all` (default) / `above-score <N>` / `below-score <N>` | `NetworkReputationListOutput` |
| `reputation show` | `ReputationStore::peer_reputation(did)` | `did:octo:0x<104-hex>` | `NetworkReputationShowOutput` |

### File changes

- `crates/octo-cli/src/commands/network.rs` — modified (1 file changed, 394 insertions, 0 deletions)

  - `NetworkAction::Reputation { action: NetworkReputationAction }` clap variant
  - `NetworkReputationAction` enum (List + Show)
  - `ReputationListArgs` (filter + threshold + json)
  - `ReputationShowArgs` (peer_did + json)
  - `ReputationFilterKind` clap `ValueEnum` (All + AboveScore + BelowScore)
  - `NetworkReputationListOutput` + `NetworkReputationShowOutput` envelope structs
  - `PeerReputationProjection` substrate-faithful projection struct
  - `network_reputation_list` + `network_reputation_show` handlers
  - `reputation_store_registry` runtime registry marker
  - `parse_reputation_peer_did` did-decoding helper (pastejacking defense per Phase 5 RFC-0011-m precedent)
  - Dispatch arm in `network_dispatch`
  - 6 NEW test vectors `tv_net10_1` through `tv_net10_6`

### Test vectors

| Vector | Subcommand | Coverage |
| --- | --- | --- |
| `tv_net10_1` | `reputation list --filter all` | bare-word filter parsing |
| `tv_net10_2` | `reputation list --filter above-score --threshold 100` | filter + threshold composed arg parsing |
| `tv_net10_3` | `reputation list --filter above-score` (no threshold) | handler-validation rule documented (threshold required for above-score/below-score) |
| `tv_net10_4` | `reputation show <canonical did>` | canonical DID parsing |
| `tv_net10_5` | `reputation show <mixed-case did>` | pastejacking defense rejects mixed-case hex |
| `tv_net10_6` | `reputation show <uppercase-only did>` | uppercase-only hex accepted |

### Dependencies

- RFC-0011-r Phase 10 reputation-store amendment Draft at `next 76998e03`
- Phase 10 G13 substrate stub fill-in at `next 6f32badb`
- Phase 10 G13 substrate slice at `next c95fd8cb`
- Phase 10 G13 paired-YAML Claimed transition at `next 9159e613`
- Phase 10 G13 CLI dispatch slice at `next 026c2e5a`
- `octo_network` Layer B substrate (existing; Phase 10 EXTENDS the trait at `crates/octo-reputation/src/store/mod.rs` §ReputationStore trait)
- Layer C CLI dispatch envelope/handler pattern (Phase 5 RFC-0011-m at `next 346f10cc`)

## Acceptance Criteria

- [x] `NetworkAction::Reputation { action: NetworkReputationAction }` clap variant lands in commands/network.rs
- [x] `NetworkReputationAction` enum lands with `List(ReputationListArgs)` + `Show(ReputationShowArgs)` variants
- [x] `ReputationListArgs` lands with `filter` (clap ValueEnum) + `threshold: Option<u32>` + `json: bool` fields
- [x] `ReputationShowArgs` lands with `peer_did: String` + `json: bool` fields
- [x] `ReputationFilterKind` clap ValueEnum lands with All + AboveScore + BelowScore variants
- [x] `NetworkReputationListOutput` envelope lands (filter + threshold + peers)
- [x] `NetworkReputationShowOutput` envelope lands (peer_did + record)
- [x] `PeerReputationProjection` projection lands (peer_did_hex + score + attestations_count + last_updated_epoch)
- [x] `network_reputation_list` handler lands with above-score/below-score threshold validation + handler validation prior to substrate gate
- [x] `network_reputation_show` handler lands with parse_reputation_peer_did pastejacking defense
- [x] `reputation_store_registry` runtime marker lands (returns `false` for Phase 10 trait-only; per-extension impl crates Layer D OUT OF SCOPE)
- [x] `parse_reputation_peer_did` shared helper lands with mixed-case hex rejection (pastejacking defense per Phase 5 RFC-0011-m precedent)
- [x] Dispatch arm lands in `network_dispatch`
- [x] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean (NO regression of existing 421 tests)
- [x] `cargo test -p octo-cli --lib` green (427/427; +6 NEW test vectors)
- [x] Layer discipline preserved (Layer C only; zero Layer A or Layer B change in this commit)
- [x] 6 NEW test vectors cover filter parsing + threshold composition + show DID parsing + pastejacking defense

## Out of Scope

- Substrate trait extension (paired mission `0011-h-s-a-reputation-store` covers that surface; landed at `next c95fd8cb`)
- Real reputation aggregation (per-extension impl crates substrate-ext-reputation-store-* in Layer D, follow-on missions)
- Per-extension registry wiring (CLI dispatch uses marker; per-extension init fn OUT OF SCOPE for Phase 10)
- Wire format versioning (deferred to RFC-0011-h §Future Work items F8 + F9)
- Pagination for `reputation list` output (OUT OF SCOPE for Phase 10)

## Notes

RFC-0011-r Phase 10 G13 CLI surface exposed through `octo network reputation list` + `reputation show` subcommands. Substrate-faithful to `crates/octo-reputation/src/store/mod.rs` §ReputationStore trait extension. CLI dispatch slice paired with substrate companion YAML via the Phase 5 RFC-0011-m 5-commit pattern (stub fill-in → substrate slice → YAML Claimed → CLI dispatch → YAMLs Completed). Slot 89 `NetworkSubstrateUnavailable` REUSE per Phase 6 precedent (0 NEW OctoCliError variants). Per-extension crate pattern preserved (trait in Layer B; concrete per-extension impl crates in separate Layer D follow-on missions). pastejacking defense per Phase 5 RFC-0011-m precedent (mixed-case hex rejected). Layer A frozen preserved.
