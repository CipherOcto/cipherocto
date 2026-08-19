---
name: mission-0957-g1-octo-vault-lookup-glue-status
description: "0957-g1 LANDED 2026-08-18 — OctoVaultLookup glue crate + VaultSubstrate handle + 4 unit + 2 integration TV. Closes S5.1 follow-on per plan §3 row 6."
metadata:
  type: project
  modified: 2026-08-18T23:55:00.000Z
---

# Mission 0957-g1 — LANDED 2026-08-18

RFC-0957 verify-time bump S5.1 follow-on. Closes the production-wiring
gap left by `0957-g-verify-time-invariant` LANDED 2026-08-17: the
`VaultLookup` trait lives in `octo-cap-macaroon` but had no production
impl wired to the substrate's `vaults_vault_id_idx` UNIQUE INDEX lookup
primitive. Commit `5b698b72`.

## Scope as landed (7/8 ACs)

- **AC-1 NEW crate** ✅ — `crates/octo-cap-macaroon-vault/` (Cargo.toml
  - src/lib.rs + src/octo_vault_lookup.rs).
- **AC-2 Cargo deps** ✅ — `octo-cap-macaroon` + `octo-vault` only.
  NO `stoolap` direct dep (substrate exposes typed `VaultSubstrate`
  handle wrapping `Arc<stoolap::Database>`).
- **AC-3 `OctoVaultLookup` struct + `VaultLookup` impl** ✅ — maps
  `VaultState::Active → is_active: true`, `Frozen → false`. Returns
  `None` iff no row exists.
- **AC-4 re-exports** ✅ — `pub use octo_cap_macaroon::{VaultLookup,
VaultLookupExt, VaultRowSnapshot}` + `pub use
octo_vault_lookup::OctoVaultLookup`.
- **AC-5 unit tests** ✅ — `tests/unit_lookup.rs`: 4 tests (Active
  hit, Frozen hit, miss→None, Send+Sync sanity).
- **AC-6 integration TV-C1** ✅ — `tests/integration_tv_c1.rs`:
  TV-0957-g1-11 (happy path) + TV-0957-g1-12 (VaultRowMissing) re-run
  the canonical TV-0957-11/12 fixtures with production `OctoVaultLookup`
  glue. Both pass; pins the full `Macaroon::verify_for_vault_op`
  round-trip through the substrate's UNIQUE INDEX.
- **AC-7 verification gate** ✅ — `cargo build` + `cargo test
-p octo-cap-macaroon-vault` (7/7) + `cargo test --lib` on
  `octo-vault` (10/10) + `octo-cap-macaroon` (193/193) + `cargo
clippy --workspace --all-targets --features full -- -D warnings`
  clean + `cargo fmt --all -- --check` clean.
- **AC-8 memory card** ✅ — this file + MEMORY.md pointer.

## Substrate handle added (`VaultSubstrate`)

Mission risk B.1 confirmed: `octo-vault` had no row-read API before
this mission. Added minimal `pub struct VaultSubstrate { db:
Arc<stoolap::Database> }` + `pub fn new(db: Arc<Database>) -> Self` +
`pub fn lookup_by_vault_id(&self, vid: &VaultId) -> Result<Option<
(ChainId, VaultState)>, VaultError>`. `Send + Sync` (Arc-wrapped
Database); `Debug` redacts the underlying handle (defense-in-depth
mirror of `TransportDeliveryCatalog`).

This is NOT a `pub use stoolap::Database;` re-export — it's a new
typed struct that wraps the handle and exposes a read API. Layer
discipline preserved: external consumers funnel through
`lookup_by_vault_id`; only the substrate's internal migration runner

- tests access `db` directly.

## Layer direction diagram

```text
octo-cap-macaroon     (Layer B extension — consumer of trait)
       |
       v
octo-cap-macaroon-vault  (NEW — Layer B glue, owns VaultState→bool)
       |
       v
octo-vault            (Layer B substrate — typed VaultSubstrate handle)
       |
       v
stoolap fork          (Layer A — Database handle, NEVER re-exported)
```

## Out of scope (carried forward)

- Production deployment wiring (config-time injection of `Arc<
OctoVaultLookup>` into macaroon verify path) — `octo-vault-node`
  Layer C territory, S6+.
- Substrate migration to add `chain_state_idx` covering
  `(chain_id, is_active)` — RFC-0960 amendment territory, separate
  mission owed.
- `OctoVaultLookup` cache layer (moka / LRU wrap) — S6+ perf-data-
  driven.

## Why this matters

Closes the verify-time invariant's missing production impl: prior to
this mission, `VaultLookup` had only the `InMemoryLookup` unit-test
stand-in (in `octo-cap-macaroon/src/vault_lookup.rs::tests`) and the
`TestVaultLookup` TV stand-in (in `tv_0957_verify_time.rs`). Neither
exercised the substrate. Now `OctoVaultLookup` is the canonical
production impl wired to the Stoolap-fork substrate's UNIQUE INDEX,
and the integration tests prove the full round-trip works end-to-end.

**How to apply:** when wiring `VaultLookup` into a node crate,
depend on `octo-cap-macaroon-vault` (not `octo-vault` directly) so
the layer discipline stays enforced. Construct `OctoVaultLookup::new
(VaultSubstrate::new(Arc::new(db)))` once per substrate lifecycle.

Related: [[mission-0957-g-verify-time-invariant-status]] (parent
trait + TV-C1 substrate), [[octo-storage-split-status]] (S2 Layer A
substrate + Layer B facade).
