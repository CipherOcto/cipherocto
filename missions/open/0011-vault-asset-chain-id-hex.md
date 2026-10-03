---
name: 0011-vault-asset-chain-id-hex
description: "Land the Layer B wire-format migration for the three 32-byte id newtypes (`AssetId`, `ChainId`, `VaultId`) behind a `hex-ids` feature flag (off by default). Under `--features hex-ids`, each newtype serializes as a 64-char lowercase hex string via a newtype-specific adapter that delegates to `hex_id_32` from the paired Layer A mission `0011-caveat-form-amendment`. Each adapter accepts both the 64-hex form (canonical, preferred) and the 32-element byte-array form (legacy, preserved for migration). Under default features the derived `Serialize` form (32-element byte array) is unchanged. 4 new test vectors (`tv_cf_10..13`)."
metadata:
  node_type: mission
  type: layer-b-wire-format
  originSessionId: caveat-form-alignment-phase-0
  created: 2026-10-02
  v: "1.0"
  depends_on:
    - 0011-caveat-form-amendment
    - rfcs/draft/0011-caveat-form-amendment.md
status: OPEN
---

# Mission `0011-vault-asset-chain-id-hex` v1.0 — OPEN 2026-10-02

**Owner:** substrate (Layer B, `octo-cap-macaroon::substrate`).
**Phase:** Phase 2 of the Caveat Form Alignment plan.
**Companion:** `missions/open/0011-caveat-form-amendment.md` (Layer A, paired).

## Context

Per `rfcs/draft/0011-caveat-form-amendment.md` §Caveat Form Amendment clause 2,
`hex_id_32` is the substrate-owned serialization path for any 32-byte
id-bearing field. The drift audit
`docs/audits/2026-09-30-open-limitations-drift-audit.md` §3 records the
asymmetry on `AssetId`, `ChainId`, and `VaultId` as *unspecified* — neither
hex nor array is mandated on the newtype — so changing the default would be
a wire-format break for any consumer that already pins the 32-element form. A
feature flag is the correct shape: OFF by default, ON once the amendment lands
and consumers migrate.

The newtype adapters delegate the hex-decode work to `hex_id_32` from the
paired Layer A mission, so the 32-byte constraint is defined once and
re-exported three times.

## Scope

### Step 1: Add the `hex-ids` feature flag

Edit `crates/octo-cap-macaroon/Cargo.toml`. Add a `[features]` entry for
`hex-ids = []`. The flag is OFF by default. The comment block names the
paired Layer A mission and the off-until-acceptance migration posture.

### Step 2: Add 3 newtype-specific adapter modules

Edit `crates/octo-cap-macaroon/src/substrate.rs`. At the bottom of the file,
add three `pub mod` blocks gated by `#[cfg(feature = "hex-ids")]`:

- `pub mod asset_id_hex { ... }` — wraps `AssetId`'s 32-byte payload.
- `pub mod chain_id_hex { ... }` — wraps `ChainId`'s 32-byte payload.
- `pub mod vault_id_hex { ... }` — wraps `VaultId`'s 32-byte payload.

Each adapter exposes `pub fn serialize<S>` (emits `hex::encode` of the inner
32 bytes) and `pub fn deserialize<'de, D>` (delegates to
`crate::hex_id_32::deserialize` and wraps the result with the newtype's
`from_bytes` constructor).

### Step 3: Add the migration-paragraph doc comments

Edit `crates/octo-cap-macaroon/src/substrate.rs`. Add a paragraph to each of
`AssetId`, `ChainId`, and `VaultId`'s doc comment describing the
`hex-ids`-flag migration posture and pointing to the paired Layer A mission.

### Step 4: Add the feature-gated round-trip vectors

New file `crates/octo-cap-macaroon/tests/tv_cf_newtype_hex_round_trip.rs`,
gated with `#![cfg(feature = "hex-ids")]`:

| Vector | What it pins |
| ------ | ------------ |
| `tv_cf_10_asset_id_hex_round_trip` | `AssetId` under `--features hex-ids` round-trips through 64-hex. |
| `tv_cf_11_chain_id_hex_round_trip` | `ChainId` under `--features hex-ids` round-trips through 64-hex. |
| `tv_cf_12_vault_id_hex_round_trip` | `VaultId` under `--features hex-ids` round-trips through 64-hex. |

### Step 5: Add the default-feature regression vector

New file `crates/octo-cap-macaroon/tests/tv_cf_newtype_default_serde.rs`
(un-gated; this test must pass under default features to prove the
gate is honest):

| Vector | What it pins |
| ------ | ------------ |
| `tv_cf_13_default_newtype_serde_is_byte_array` | under default features, `AssetId` / `ChainId` / `VaultId` serialize as 32-element byte arrays. |

### Step 6: Confirm the gate is honest

Run `cargo test -p octo-cap-macaroon --test tv_cf_newtype_hex_round_trip`
without the feature flag. The test binary is `#[cfg]`-gated, so it is empty
under default features — the default-feature `Serialize` is unchanged.

## Layer model

- `octo-cap-macaroon` (Layer B) — additive only (a new feature flag + 3 new
  feature-gated `pub mod` adapter modules + 4 new tests). No field removals,
  no variant additions, no public-API changes under default features.
- `octo-vault` (consumer of `AssetId` / `ChainId` / `VaultId`) — unchanged.
  Consumers that did not opt into `--features hex-ids` see no wire-format
  change.

## Acceptance Criterion

The mission is closed when the following are true:

- **AC-1:** `AssetId` / `ChainId` / `VaultId` serialize as 64-char lowercase
  hex strings under `--features hex-ids`. Pinned by `tv_cf_10`,
  `tv_cf_11`, `tv_cf_12`.
- **AC-2:** each newtype's `hex-ids` adapter accepts both the 64-hex form
  (canonical, preferred) and the 32-element byte-array form (legacy,
  preserved for migration). Pinned by `tv_cf_10..12` plus the
  `hex_id_32_accepts_legacy_array_form` test (`tv_cf_05`) from the paired
  Layer A mission (the Layer B adapter delegates to `hex_id_32::deserialize`
  which already accepts both forms).
- **AC-3:** under default features (no `hex-ids`), the derived `Serialize`
  form is unchanged — 32-element byte array. Pinned by
  `tv_cf_13_default_newtype_serde_is_byte_array`.
- **AC-4:** `cargo test -p octo-cap-macaroon --features hex-ids --tests`
  green (the 3 feature-gated vectors + the 6 Layer A integration vectors =
  9 total integration tests under `--features hex-ids`).
- **AC-5:** `cargo test -p octo-cap-macaroon --tests` green (the 1
  default-feature regression vector + the 6 Layer A integration vectors = 7
  total integration tests under default features).
- **AC-6:** `cargo clippy -p octo-cap-macaroon --all-targets -- -D warnings`
  clean.
- **AC-7:** `cargo clippy -p octo-cap-macaroon --all-features -- -D warnings`
  clean.
- **AC-8:** `cargo fmt --all -- --check` clean.
- **AC-9:** Layer B frozen check: `git diff crates/octo-cap-macaroon` shows
  only the `Cargo.toml` feature entry, the `substrate.rs` 3 new modules +
  3 doc-comment paragraphs, and the 2 new test files.

## Files / Artifacts

- Edit: `crates/octo-cap-macaroon/Cargo.toml` (add `hex-ids` feature flag)
- Edit: `crates/octo-cap-macaroon/src/substrate.rs` (3 new
  `pub mod` adapter modules + 3 doc-comment paragraphs)
- New: `crates/octo-cap-macaroon/tests/tv_cf_newtype_hex_round_trip.rs`
  (3 tests, feature-gated)
- New: `crates/octo-cap-macaroon/tests/tv_cf_newtype_default_serde.rs`
  (1 test, default-features regression)

## Cross-references

- RFC-0011 §Caveat Catalog — the canonical envelope the newtype hex form
  aligns with.
- RFC-0011 §Hex32 newtype — the operator-visible hex discipline this mission
  extends to substrate newtypes.
- `rfcs/draft/0011-caveat-form-amendment.md` — the amendment RFC. The
  acceptance of this mission is the `hex-ids` feature's `OFF by default`
  → `ON after migration` switch.
- `docs/plans/2026-10-02-caveat-form-alignment.md` Phase 2 — the plan.
- Paired Layer A mission `0011-caveat-form-amendment` — provides
  `hex_id_32` for the newtype adapters to delegate to.

## Out of scope

- The `Caveat::Vault` `hex_id_32` application (owned by the paired Layer A
  mission `0011-caveat-form-amendment`). This mission's `vault_id_hex` is
  for `VaultId` *outside* `Caveat::Vault`.
- The `CaveatSummaryView` field rename (owned by Phase 3 of the plan).
- Removing the 32-element array form from the adapters (deferred until the
  amendment is Accepted and consumers migrate).

## Dependencies

- `missions/open/0011-caveat-form-amendment.md` — paired Layer A mission
  whose `hex_id_32` adapter this mission's newtype adapters delegate to.
- `rfcs/draft/0011-caveat-form-amendment.md` — the amendment RFC.

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                          |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v1.0    | 2026-10-02 | Initial filing per Caveat Form Alignment plan Phase 0. `hex-ids` feature flag (off by default) + 3 newtype-specific adapter modules (`asset_id_hex`, `chain_id_hex`, `vault_id_hex`) + 4 new test vectors (`tv_cf_10..13`). Paired with Layer A. |