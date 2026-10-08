# Mission: octo-vault Layer-B Crate (S3)

## Status

**LANDED 2026-08-16 (drift closure).** Mission file written to
close the phantom pointer left by the storage restructure plan
(`docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
§3 S3 referenced `missions/open/octo-vault-s3-crate-substrate.md`
but the YAML was never authored).

## Commits

- `7bd0ac59` — feat(octo-vault): S3 — new Layer-B vault substrate crate
  (2026-08-16)
- `b211d00e` — fix(octo-vault): S3 round-2+3 review fixes
- `a34afc67` — fix(octo-vault): S3 round-4 review fixes (surface + ABI hygiene)
- `e59a5cb8` — fix(octo-vault): S3 round-1 review fixes
- `11e9efce` — feat(0105-v): asset_id_for + 9 role-token enum (adds
  `AssetId` derivation, canonical for vault PK Model B)
- `5b698b72` — feat(0957-g1): OctoVaultLookup glue crate + VaultSubstrate
  handle (S5.1 follow-on; sits on top of S3 substrate)

## RFC

RFC-0960 §20.3 (Vault PK Model B + composite PK); RFC-0105 §Asset ID
Derivation; §review §8.10 (canonical vault_id wire format).

## Summary

New Layer-B vault substrate crate. Model B per review §20.3
(composite PK `chain_id + owner_did + asset_id`; no VaultHierarchy;
state enum = `Active | Frozen` only).

## Acceptance Criteria

- [x] **AC-1**: `crates/octo-vault/Cargo.toml` declares the
      Layer-B vault substrate crate (depends on `octo-storage-core`
      + `octo-determin`; no high-level business schema)
- [x] **AC-2**: Vault types: `VaultId`, `ChainId`, `AssetId`,
      `VaultState`, `VaultPolicy`, `VaultMetadata`
- [x] **AC-3**: Canonical `vault_id` derivation per §8.10 wire
      format:
      `BLAKE3("cipherocto/vault/v1/" || chain_id || owner_did || asset_id)`
- [x] **AC-4**: `apply()` delegation to `octo_storage_core::apply_pending`
- [x] **AC-5**: Migration catalog v013 (vaults table + UNIQUE
      vault_id index) + v014 (append-only `transfer_events` per
      review §9.3)
- [x] **AC-6**: 10 byte-exact TV-V1 fixtures (test_vectors.rs)
      with pinned hex constants per `AssetId::derive(role_token)`
      × 9 role-token set
- [x] **AC-7**: 4 byte-exact TV-C1 (in `crates/octo-cap-macaroon/tests/
      tv_c1_verify_time.rs`) — verify-time invariant round-trip
- [x] **AC-8**: 3 TV-0862 vault_id cross-ref (added in 0862-c10
      sweep era; `crates/octo-vault/tests/tv_0862_vault_id_cross_ref.rs`)

## Test count (33/33 green at LANDED)

| Suite | Tests |
|---|---|
| `src/lib.rs` (unit) | 10 |
| `tests/apply_migrations.rs` | 5 |
| `tests/test_vectors.rs` | 15 (10 TV-V1 + 5 helpers) |
| `tests/tv_0862_vault_id_cross_ref.rs` | 3 |
| **TOTAL** | **33** |

Plus 4 TV-C1 in `crates/octo-cap-macaroon/tests/tv_c1_verify_time.rs`
(consumer-side; verify-time invariant against vault substrate) =
14 TV fixtures per plan §3 S3 spec.

## Files

```
crates/octo-vault/
  Cargo.toml
  src/
    lib.rs              (vault types + canonical derivation)
    migrations.rs       (v013 + v014 migrations)
  tests/
    apply_migrations.rs         (5 tests)
    test_vectors.rs             (15 tests, 10 TV-V1 hex pinned)
    tv_0862_vault_id_cross_ref.rs (3 tests)
  examples/
    capture_tv_v1.rs            (hex regen utility)
```

## Verification

```bash
cargo test -p octo-vault  # 33/33 green
cargo clippy -p octo-vault --all-targets -- -D warnings
```

## Reference

- Plan §3 row 5 (S3) + §4 (verification gate)
- Review §20.3 (Model B PK) + §8.10 (canonical vault_id wire form)
- RFC-0960 §20.3 (consumer spec)
- `no-phantom-mission-pointer` rule — this YAML closes the
  phantom pointer from the plan
