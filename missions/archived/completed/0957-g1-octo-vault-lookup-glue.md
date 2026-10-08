# Mission: OctoVaultLookup Glue Crate (S5.1 Follow-on)

## Status

**LANDED 2026-08-19 (drift closure).** Mission file written to
remedy the **phantom pointer** left by 0957-g's deferral memo
(`missions/open/0957-g1-octo-vault-lookup-glue.md` referenced but
never authored).

## Commits

- `5b698b72` — feat(0957-g1): OctoVaultLookup glue crate +
  VaultSubstrate handle (LANDED 2026-08-18)
- `160ffd4b` — fix(0105-v2-followon): canonicalize role-token form
  in 0957-g1 NEW test files (drift fix, 3 sites in 2 files)

## RFC

RFC-0960 §20.3 (vaults_vault_id_idx UNIQUE INDEX substrate);
RFC-0957 verify-time bump (consumer trait).

## Summary

Wire the `octo_cap_macaroon::VaultLookup` trait (Layer B extension,
LANDED 2026-08-17 in 0957-g) to the Stoolap-fork substrate's
`vaults_vault_id_idx` UNIQUE INDEX lookup primitive. Topology
mirrors `TransportDeliveryCatalog` (crates/octo-cap-macaroon-transport):
consumer trait stays primitive-typed; glue crate sits between
consumer + substrate owner; substrate exports a typed
`VaultSubstrate` handle.

## Acceptance Criteria

- [x] **AC-1**: New crate `octo-cap-macaroon-vault` (Layer B
      extension) with `OctoVaultLookup` struct implementing
      `VaultLookup::lookup_vault` via the substrate handle
- [x] **AC-2**: `VaultSubstrate` typed handle wrapping
      `Arc<stoolap::Database>` so `VaultLookup` consumers don't see
      raw DB
- [x] **AC-3**: `VaultRowSnapshot { is_active: bool, chain_id: [u8;
      32] }` returned to verify path
- [x] **AC-4**: `is_active` flag computed from substrate state enum
      (Active=true; Frozen/Liquidating/Closed=false — fail-closed
      semantics)
- [x] **AC-5**: Unit tests for Active + Frozen + Missing row paths
      (4/4 green at 0957-g1 LANDED)
- [x] **AC-6**: Integration test re-runs canonical TV-0957-11 +
      TV-0957-12 with production `OctoVaultLookup` instead of
      `TestVaultLookup` stand-in (2/2 green at 0957-g1 LANDED)
- [x] **AC-7**: `OctoVaultLookup` is `Send + Sync` (required for
      `&dyn VaultLookup` injection on multi-threaded node stacks)
- [x] **AC-8**: Consumer (`octo-cap-macaroon`) stays free of any
      vault-substrate dep (Layer B → Layer B via the glue crate)

## Files

| File | Purpose |
|---|---|
| `crates/octo-cap-macaroon-vault/Cargo.toml` | crate manifest |
| `crates/octo-cap-macaroon-vault/src/lib.rs` | crate root |
| `crates/octo-cap-macaroon-vault/src/octo_vault_lookup.rs` | glue impl |
| `crates/octo-cap-macaroon-vault/tests/unit_lookup.rs` | unit tests (4) |
| `crates/octo-cap-macaroon-vault/tests/integration_tv_c1.rs` | integration (2) |

## Verification

```bash
cargo test -p octo-cap-macaroon-vault --tests  # 4/4 unit + 2/2 integration
cargo clippy -p octo-cap-macaroon-vault --all-targets -- -D warnings
```

## Reference

- 0957-g memory card (parent mission; DEFERRED §S5.1 → this mission)
- `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan`
  §3 row 6 (Stream C.2 continuation)
- RFC-0960 §20.3 (substrate UNIQUE INDEX)
- `no-phantom-mission-pointer` rule — this YAML is the remedy for
  the phantom pointer that 0957-g's deferral memo left
