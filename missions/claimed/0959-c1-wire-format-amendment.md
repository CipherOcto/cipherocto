# Mission: 0959-c1 — RFC-0959 wire-format amendment (S6e) — settlement DqaEncoding + cost_vault_id + chain_id + VaultLookup reuse

## Status

**OPEN 2026-08-17 (@mmacedoeu); re-scoped 2026-08-19.** Filed per audit
verdict 2026-08-17 (storage restructure hard-recommendation #4).
Closes audit Risk #5 (HIGH — vault-row lookup at 2 verify paths, only
1 LANDED) and Risk #6 part (ChainId canonical mapping). S6e fifth
sub-session per `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
§3 row 6 (Stream A.1 continuation; user-chosen S6 split-by-RFC
decision overrides §22 atomic-blocker bundle rule).

Pre-reqs verified landed: S3 (octo-vault crate LANDED), S4 (Dqa
codemod LANDED 2026-08-17 per `S4-codemod-2026-08-17-LANDED.md`),
S5 (verify-time invariant LANDED 2026-08-17 commit `d007de54`),
S6a (RFC-0870 v2.1 NodeEnvelope version_tag LANDED 2026-08-17),
S6b (RFC-0957 v2.1 verify-time + Caveat DSL LANDED 2026-08-17
commits `c9149128` + `4ec9779f` + `e5138420`), S6c (RFC-0862 v2.0 +
c7 + c8 LANDED 2026-08-17).

## RFC

- Primary: RFC-0959 (Wire Format) — `SettlementEnvelope` v2.0 with
  `cost: DqaEncoding` (16-byte BE scale 12) + `cost_vault_id: Option<[u8;32]>`
  + `chain_id: Option<[u8;32]>` per review §8.4.1 + §20.7 + §8.5.1
  precision-loss-tested round-trip fixtures
- Co-RFC: RFC-0960 (chain-aware bump) — `cost_vault_id` column
  reference + `chain_id` column in settlement_events schema per
  review §9.2 v004 sketch
- Co-RFC: RFC-0957 (verify-time bump) — `VaultLookup` trait reuse
  (audit hard-recommendation #4: settlement-time vault row lookup
  reuses same trait as capability verify-time per §20.7)
- Co-RFC: RFC-0010 (32-byte addendum) — `chain_id` 32-byte BLAKE3
  derivation cross-ref (LANDED 2026-08-19 commit `9cf483db`)

## Recon 2026-08-19 — actual current state

Most substrate already landed (mission file path drift corrected):

| Mission AC | Original path (stale) | Actual current state |
|---|---|---|
| AC-5 | `settlement_event_repo.rs: cost_micro_octo_w: u128 → cost: Dqa` | ALREADY LANDED — field at line 277 typed `octo_determin::Dqa`; field name remains `cost_micro_octo_w` (cosmetic — wire form is already DqaEncoding via v004 column type) |
| AC-5 | `SettlementEnvelope.cost: DqaEncoding` | ALREADY LANDED — at `crates/quota-router-storage/src/ask.rs:1003` with `#[serde(with = "crate::dqa_serde::field")] pub cost: Dqa` |
| AC-6 | `crates/quota-router-storage/src/octo_vault_lookup.rs` (NEW per S5.1 follow-on) | ALREADY LANDED — actually at `crates/octo-cap-macaroon-vault/src/octo_vault_lookup.rs` (S5.1 follow-on landed; uses `octo_cap_macaroon::{VaultLookup, VaultRowSnapshot}` + `octo_vault::{ChainId, VaultId, VaultState, VaultSubstrate}`) |
| AC-2 step 2 | `crates/octo-protocol/src/settlement_envelope.rs` | NOT EXISTENT — `SettlementEnvelope` lives in `crates/quota-router-storage/src/ask.rs:984` |
| AC-2 step 2 | `crates/quota-router-storage/src/octo_vault_lookup.rs` | NOT EXISTENT — `OctoVaultLookup` lives in `crates/octo-cap-macaroon-vault/src/octo_vault_lookup.rs` |
| AC-4 | `migrations/v015__settlement_chain_vault.sql` | NOT EXISTENT — v015 slot taken by `v015__chain_aware_slash_ledger.sql`. New migration must be v016. |
| AC-3 | `crates/octo-protocol/tests/tv_0959_settlement_wire.rs` | NOT EXISTENT — new TV file must live in `crates/quota-router-storage/tests/` (where `SettlementEnvelope` lives) |
| AC-2 | `crates/quota-router-storage/src/settlement_verify.rs` (NEW) | NOT EXISTENT — needs creation |
| AC-2 | RFC-0959 §Wire Format subsection + §Settlement-Time Vault Row Lookup subsection + §Cross-Chain Settlement Reject algorithm | NOT EXISTENT — needs RFC-0959 v2.0 amendment |

**Real remaining work** (corrected scope):

1. Add `cost_vault_id: Option<[u8; 32]>` + `chain_id: Option<[u8; 32]>`
   fields to `SettlementEnvelope` (extend existing struct at
   `crates/quota-router-storage/src/ask.rs:984`).
2. Update `SettlementEnvelope::compute_settlement_hash()` to include
   the new fields in canonical preimage.
3. Add `SettlementError::ChainMismatch { vault_id, vault_chain_id,
   envelope_chain_id }` variant + `SettlementError::CostVaultIdMissing`
   variant.
4. Create `crates/quota-router-storage/src/settlement_verify.rs` with
   `verify_settlement_chain_match(envelope, vault_lookup)` function —
   holds `&dyn octo_cap_macaroon::VaultLookup` + verifies cost_vault_id
   present, vault row exists, vault.chain_id == envelope.chain_id.
   No shadow impl. Reuses `VaultLookup` trait from `octo-cap-macaroon`.
5. Add `octo-cap-macaroon` dep to
   `crates/quota-router-storage/Cargo.toml` (intra-Layer B dep —
   allowed by layer model per RFC-0957-A1 §Layer Discipline).
6. Create `crates/quota-router-storage/migrations/v016__settlement_chain_vault.sql`:
   - `cost_micro_octo_w BLOB` column type unchanged (already 16-byte
     BE DqaEncoding-compatible via scale=0 i64-bridge; per RFC-0960
     §Vault Substrate + 0900-d AC-2 sub-clause)
   - NEW column `cost_vault_id BLOB(32) NULL` per §20.7
   - NEW column `chain_id BLOB(32) NULL` per §20.7
   - NEW `CREATE UNIQUE INDEX idx_se_cost_vault_id ON
     settlement_events(cost_vault_id)` per §20.7 audit query
   - Backfill legacy v004 rows with `NULL` (no DEFAULT — fork parser
     limitation per v015 recon).
7. Create `crates/quota-router-storage/tests/tv_0959_settlement_wire.rs` —
   25 byte-exact TV per AC-3 split below.
8. Update `rfcs/accepted/economics/0959-ask-settlement-chain.md`:
   - Status header v1.0 → v2.0
   - §Version History v2.0 row
   - §Wire Format v2.0 subsection (replaces v1.0 §Wire Format) —
     field list with byte offsets + `DqaEncoding` 16-byte BE encoding
     + `cost_vault_id` derivation cross-ref to RFC-0960 + `chain_id`
     derivation cross-ref to RFC-0010 v1.6
   - §Settlement-Time Vault Row Lookup subsection (NEW) — 3-step
     algorithm: `cost_vault_id` present → `vault_lookup.lookup_vault(cost_vault_id)`
     → `vault.chain_id == envelope.chain_id` else
     `SettlementError::ChainMismatch`
   - §Cross-Chain Settlement Reject subsection (NEW) — explicit
     reject invariant per §20.7
   - Cross-ref to RFC-0957 §Verify-Time Extension (same `VaultLookup`
     trait shared between capability + settlement paths)
9. Update `PersistedSettlementEvent` in
   `crates/quota-router-storage/src/settlement_event_repo.rs` to
   carry `cost_vault_id: Option<[u8; 32]>` + `chain_id: Option<[u8; 32]>`
   fields + UPDATE/INSERT SQL bindings.
10. Update `SettlementEventInsert` (the DAO input struct) to carry
    the new fields + UPDATE `INSERT INTO settlement_events` SQL.
11. Update `lib.rs` `pub use` to expose `settlement_verify` module.

## Dependency edges

| From | To | Why | Layer direction |
|---|---|---|---|
| `crates/quota-router-storage/src/settlement_verify.rs` (NEW) | `octo_cap_macaroon::VaultLookup` | Trait abstraction | lib → lib (Layer B intra) |
| `crates/quota-router-storage/src/settlement_event_repo.rs` | `octo_determin::Dqa` (already) | In-memory cost type | lib → lib (Layer A) |
| `crates/quota-router-storage/migrations/v016__settlement_chain_vault.sql` (NEW) | `cost_vault_id BLOB(32)` + `chain_id BLOB(32)` columns | Storage schema | n/a (schema only) |
| `crates/quota-router-storage/tests/tv_0959_settlement_wire.rs` (NEW) | `octo_vault::vault_id_unchecked` (cross-ref) | TV cross-RFC pinning | test → test |

No new cyclic edges. New crate dep:
- `quota-router-storage` gains `octo-cap-macaroon` (intra-Layer B
  per RFC-0957-A1 §Layer Discipline; `VaultLookup` trait source)

## Problem

Audit (2026-08-17) found two parallel-model risks at the settlement
wire-form layer:

**Risk A — settlement envelope carries no chain scope:**

- `SettlementEnvelope` lacks `chain_id` field; no way to
  deterministically assert the settlement belongs to a specific chain
- Per review §20.7: settlement-time verifier needs
  `envelope.chain_id == cost_vault_row.chain_id` for cross-chain
  settlement reject
- Per RFC-0010 v1.6 (LANDED 2026-08-19): chain_id canonical form is
  `BLAKE3("cipherocto/chain/v1/" || chain_string)` 32-byte

**Risk B — vault row lookup at 2 paths, only 1 LANDED:**

- Capability verify-time path: `VaultLookup::lookup_vault(vault_id)`
  — LANDED via S5 commit `d007de54` + S5.1 follow-on
  `OctoVaultLookup` at `crates/octo-cap-macaroon-vault/src/octo_vault_lookup.rs`
- Settlement-time path: spec §20.7 mandates "same vault-row
  UNIQUE INDEX lookup pattern"; needs the same `VaultLookup` trait
  abstraction (NOT a shadow impl per audit #5)

**Risk C — settlement envelope lacks cost_vault_id:**

- `SettlementEnvelope` lacks `cost_vault_id` field; no way to
  bind a settlement to a specific vault row for cross-chain reject
- Per review §20.7: `cost_vault_id` is the settlement-time vault row
  lookup key — UNIQUE INDEX `idx_se_cost_vault_id` enables audit
  queries ("show all settlements against vault X")

## Summary

RFC-0959 §Wire Format (v1.0, accepted 2026-07-20) describes
`SettlementEnvelope` carrying `cost: Dqa` (16-byte BE scale 0 via
`dqa_serde::field`; LANDED 2026-08-17 per S4 codemod). RFC-0959 v2.0
amendment (this mission) extends the wire form to:

1. `cost: DqaEncoding` (16-byte BE scale 12) per §8.4.1 —
   **UNCHANGED** from v1.0 substrate (already Dqa; codemod LANDED)
2. `cost_vault_id: Option<[u8; 32]>` field (NEW) per §20.7 — enables
   settlement-time vault row lookup for cross-chain settlement reject
3. `chain_id: Option<[u8; 32]>` field (NEW) per §20.7 — settlement
   envelope carries chain scope; settlement-time verify matches
   against `cost_vault_id` row's `chain_id` column

Per review §20.7 "settlement-time chain check uses the same
vault-row UNIQUE INDEX lookup pattern as §20.6.1 capability
verify-time, against `SettlementEnvelope.cost_vault_id`".

**CRITICAL: settlement-time vault row lookup MUST reuse the
`VaultLookup` trait from `octo-cap-macaroon`.** No shadow impl.
Hard AC below. Per audit #5 HIGH.

Per RFC-0126 §backfill-precision pattern (referenced in §8.4.1),
25 byte-exact TV fixtures pin wire-form round-trip +
precision-loss-tested cases (per plan §3 S6 row 6 spec:
"RFC-0959: 25 wire-format").

## Acceptance Criteria

- AC-1: **RFC-0959 §Version History v2.0 row added** documenting:
  - `SettlementEnvelope.cost: Dqa` (16-byte BE scale 0) — **unchanged
    from v1.0** (already shipped via S4 codemod; canonical byte form
    matches §8.4.1 scale-12 for the scale=0 subset)
  - `SettlementEnvelope.cost_vault_id: Option<[u8; 32]>` NEW field per
    §20.7 cross-chain settlement reject
  - `SettlementEnvelope.chain_id: Option<[u8; 32]>` NEW field per
    §20.7 chain scope carry
  - **VaultLookup trait reuse** for settlement-time vault row
    lookup — explicit cross-ref to
    `crates/octo-cap-macaroon/src/vault_lookup.rs`
  - Implementation mission: this file (`0959-c1-wire-format-amendment.md`)
  - Pre-req: S3 + S4 + S5 + S6a + S6b + S6c all LANDED 2026-08-17
- AC-2: **RFC-0959 §Wire Format v2.0 subsection added** (replaces
  v1.0 §Wire Format):
  - `SettlementEnvelope` v2.0 field list with byte offsets
  - `Dqa` 16-byte BE encoding per `dqa_serde::field` (already
    shipped via S4)
  - `cost_vault_id` derivation cross-ref to RFC-0960 §vault_id
    canonical (`BLAKE3("cipherocto/vault/v1/" + chain_id +
owner_did + asset_id)`)
  - `chain_id` derivation cross-ref to RFC-0010 v1.6 §ChainId
    32-byte addendum (`BLAKE3("cipherocto/chain/v1/" || chain_string)`)
  - **Settlement-time vault row lookup algorithm** (3-step):
    1. `cost_vault_id` present? Else reject with
       `SettlementError::CostVaultIdMissing`
    2. `vault_lookup.lookup_vault(cost_vault_id)` returns
       `Option<VaultRowSnapshot { chain_id, is_active, ... }>`
       — reuses `octo-cap-macaroon::vault_lookup::VaultLookup` trait
       directly (NO shadow impl)
    3. `vault_snapshot.chain_id == envelope.chain_id`? Else
       `SettlementError::ChainMismatch { vault_id, expected,
actual }`
  - Cross-ref to RFC-0957 §Verify-Time Extension (same trait shared)
- AC-3: **TV-0959-01..25 byte-exact fixtures** in
  `crates/quota-router-storage/tests/tv_0959_settlement_wire.rs` (NEW):
  - **TV-0959-01..05**: `Dqa` 16-byte BE round-trip — `Dqa { value,
    scale }` → 16-byte BE → `Dqa { value, scale }` byte-exact
    (via `dqa_serde::field` codec path)
  - **TV-0959-06..10**: precision-loss-tested — large values that
    would lose precision under scale-6 vs scale-12; round-trip
    preserves value (per RFC-0126 §backfill-precision)
  - **TV-0959-11..15**: `cost_vault_id` derivation cross-ref —
    verify `SettlementEnvelope.cost_vault_id` matches
    `octo_vault::vault_id_unchecked(chain_id, owner_did, asset_id)`
    byte-exact
  - **TV-0959-16..20**: cross-chain settlement reject — envelope
    with `chain_id = "A"` + `cost_vault_id` whose row has
    `chain_id = "B"` → `SettlementError::ChainMismatch`
  - **TV-0959-21..25**: VaultLookup trait reuse — verify the
    settlement-time verifier constructs `&dyn VaultLookup` (from
    `octo-cap-macaroon`) and invokes
    `vault_lookup.lookup_vault(cost_vault_id)`; no separate
    `fn lookup_vault_row(&self, vault_id: &[u8; 32])` shadow
    impl in `quota-router-storage` (would defeat the abstraction)
- AC-4: **`v016__settlement_chain_vault.sql` migration added** (per
  plan §9.2 v004 sketch + recon v015 slot correction):
  - `cost_micro_octo_w BLOB NOT NULL` column UNCHANGED (already
    16-byte BE DqaEncoding-compatible via scale=0)
  - NEW column `cost_vault_id BLOB(32) NULL` per §20.7
  - NEW column `chain_id BLOB(32) NULL` per §20.7
  - NEW `CREATE UNIQUE INDEX idx_se_cost_vault_id ON
settlement_events(cost_vault_id)` per §20.7 audit query
  - Backfill legacy v004 rows with `NULL` (no DEFAULT — fork parser
    limitation per v015 recon documented)
- AC-5: **`settlement_event_repo.rs` field additions**:
  - `PersistedSettlementEvent` gains `cost_vault_id: Option<[u8; 32]>`
    + `chain_id: Option<[u8; 32]>` fields
  - `SettlementEventInsert` DAO input struct gains the same fields
  - INSERT SQL bindings updated for the 2 new columns
  - `cost_micro_octo_w: octo_determin::Dqa` field ALREADY present
    (line 277) — no change to cost field
- AC-6: **`OctoVaultLookup` adapter** ALREADY LANDED at
  `crates/octo-cap-macaroon-vault/src/octo_vault_lookup.rs` (S5.1
  follow-on). This mission REUSES the adapter via the `VaultLookup`
  trait — no new adapter creation.
- AC-7: **`settlement_verify.rs` settlement-time verifier created**
  at `crates/quota-router-storage/src/settlement_verify.rs` (NEW):
  - Function: `pub fn verify_settlement_chain_match(
      envelope: &SettlementEnvelope,
      vault_lookup: &dyn VaultLookup,
    ) -> Result<(), SettlementError>`
  - 3-step algorithm per AC-2 (cost_vault_id present → vault row
    exists → chain match)
  - `SettlementError::CostVaultIdMissing` + `SettlementError::ChainMismatch`
    variants added to `SettlementError` enum
  - HOLD `&dyn octo_cap_macaroon::VaultLookup` directly — no shadow
    impl, no `Arc<dyn ...>` wrapping required for the verifier
    function signature
- AC-8: **Cargo.toml dep added**:
  - `crates/quota-router-storage/Cargo.toml` gains
    `[dependencies.octo-cap-macaroon]` (intra-Layer B dep —
    `VaultLookup` trait source)
- AC-9: **lib.rs module declaration**:
  - `crates/quota-router-storage/src/lib.rs` declares
    `pub mod settlement_verify;` + adds to `pub use` block if
    applicable
- AC-10: Verification gate:
  ```bash
  cargo test -p quota-router-storage --test tv_0959_settlement_wire  # 25/25 pass
  cargo test -p quota-router-storage --lib                            # no regressions (S6c 13/13 stay green)
  cargo test -p octo-cap-macaroon --lib                              # verify VaultLookup trait surface unchanged
  cargo test -p octo-vault --lib                                     # no regressions (TV-V1 + TV-C1 stay green)
  cargo clippy --workspace --all-targets --features full -- -D warnings
  cargo fmt --all -- --check
  npx prettier --write missions/open/0959-c1-wire-format-amendment.md
  ```

## Cross-reference

- **Pre-req:** `missions/open/0862-c1-dqa-vault-bump-amendment.md`
  (S6c LANDED 2026-08-17), `missions/open/0957-c1-verify-time-amendment.md`
  (S6b LANDED 2026-08-17), `missions/open/0870-c1-version-tag-amendment.md`
  (S6a LANDED 2026-08-17),
  `missions/open/0957-g-verify-time-invariant.md` (S5 LANDED 2026-08-17
  commit `d007de54`), `missions/open/0010-c-32-byte-addendum.md`
  (LANDED 2026-08-19 commit `9cf483db`)
- **Sibling missions:**
  - `missions/open/0862-c1-dqa-vault-bump-amendment.md` (S6c LANDED)
  - `missions/open/0870-c1-version-tag-amendment.md` (S6a LANDED)
  - `missions/open/0957-c1-verify-time-amendment.md` (S6b LANDED)
  - S6d RFC-0900 amendment (pending, 10 TV)
  - S6f RFC-0960 amendment (pending, 108 TV)
  - S6g RFC-0105 amendment (pending, 109 TV)
- **Pattern:** `crates/octo-cap-macaroon/src/vault_lookup.rs:62` —
  existing `VaultLookup` trait (S5 LANDED) — settlement-time
  verifier reuses this trait directly per AC-7 (no shadow impl)
- **Pattern:** `crates/octo-cap-macaroon-vault/src/octo_vault_lookup.rs` —
  production `OctoVaultLookup` adapter (S5.1 follow-on LANDED) —
  settlement-time verifier is called by the marketplace with
  `OctoVaultLookup` as the `&dyn VaultLookup` arg
- **Plan:** `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 row 6 (Stream A.1 S6e sub-session)
- **Review source:**
  `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
  §8.4.1 (wire-format blast + precision loss) + §9.2 (v004 sketch)
  - §20.7 (RFC impact update for RFC-0959) + §24 (per-RFC TV
    acceptance table: RFC-0959 = 25 TV)
- **Audit source:** 2026-08-17 audit verdict, Risks #5 (HIGH) +
  #6 (MED partial)

## Critical files

- `rfcs/accepted/economics/0959-ask-settlement-chain.md` (modify
  — Status v1.0 → v2.0 + §Version History v2.0 row + §Wire Format
  v2.0 subsection + §Settlement-Time Vault Row Lookup subsection +
  §Cross-Chain Settlement Reject subsection)
- `crates/quota-router-storage/src/ask.rs` (modify —
  `SettlementEnvelope` v2.0 fields: `cost_vault_id: Option<[u8; 32]>`
  + `chain_id: Option<[u8; 32]>`; `compute_settlement_hash` canonical
  preimage extension; `SettlementError::CostVaultIdMissing` +
  `SettlementError::ChainMismatch` enum variants)
- `crates/quota-router-storage/src/settlement_verify.rs` (NEW —
  settlement-time verifier per AC-7)
- `crates/quota-router-storage/src/settlement_event_repo.rs`
  (modify — `PersistedSettlementEvent` + `SettlementEventInsert`
  field additions + INSERT SQL bindings per AC-5)
- `crates/quota-router-storage/migrations/v016__settlement_chain_vault.sql`
  (NEW — `cost_vault_id BLOB(32) NULL` + `chain_id BLOB(32) NULL` +
  UNIQUE INDEX per AC-4)
- `crates/quota-router-storage/tests/tv_0959_settlement_wire.rs`
  (NEW — 25 byte-exact fixtures per AC-3)
- `crates/quota-router-storage/Cargo.toml` (modify — add
  `octo-cap-macaroon` dep per AC-8)
- `crates/quota-router-storage/src/lib.rs` (modify — declare
  `pub mod settlement_verify;` per AC-9)
- `crates/octo-cap-macaroon/src/vault_lookup.rs` (NO modify — trait
  surface stays Layer B, no settlement-specific extensions added)
- `crates/octo-cap-macaroon-vault/src/octo_vault_lookup.rs` (NO
  modify — production adapter already LANDED via S5.1)

## Existing patterns reused

- `crates/octo-cap-macaroon/src/vault_lookup.rs::VaultLookup` trait
  (S5 LANDED) — settlement-time verifier reuses via `&dyn VaultLookup`
  per AC-7 (no shadow impl, no `Arc<dyn ...>` wrapping)
- `crates/octo-cap-macaroon-vault/src/octo_vault_lookup.rs::OctoVaultLookup`
  (S5.1 follow-on LANDED) — production adapter invoked by marketplace
  caller with `OctoVaultLookup` as the `&dyn VaultLookup` arg
- `crates/octo-determin::Dqa` 16-byte BE wire form via
  `dqa_serde::field` (S4 LANDED) — wire codec round-trip
- `crates/octo-vault::vault_id_unchecked(chain_id, owner_did,
  asset_id)` — canonical vault_id helper for TV cross-ref
  (`SettlementEnvelope.cost_vault_id` MUST match this byte-exact)
- `crates/octo-ident::chain::ChainId::as_bytes()` — RFC-0010 v1.6
  32-byte BLAKE3 derivation (LANDED 2026-08-19)
- `crates/quota-router-storage/migrations/v015__chain_aware_slash_ledger.sql`
  — v015 backfill pattern (UPDATE ... WHERE col IS NULL) for
  v016 migration
- TV fixture byte-pin pattern from
  `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs`
  (S6c LANDED) — 25 fixtures follow same byte-pinned layout

## Risks

- **VaultLookup trait shadow impl** (HIGH per audit #5): the most
  important risk. If settlement-time verifier defines a private
  `fn lookup_vault_row(&self, ...)` instead of using
  `&dyn VaultLookup`, the abstraction is defeated and the
  verify-time invariant may diverge. Mitigation: AC-3 TV-0959-21..25
  pin the trait surface; AC-7 makes `&dyn VaultLookup` the explicit
  verifier arg; AC-10 verification gate exercises the trait
  reuse path.
- **Wire-form drift** (MED): `cost_micro_octo_w` storage column
  unchanged (BLOB; already 16-byte BE DqaEncoding-compatible).
  v016 adds 2 new columns + UNIQUE INDEX — additive schema change,
  no migration of existing column data required.
  Mitigation: 25 TV fixtures pin byte form; INSERT bindings
  updated atomically with column additions.
- **CLI surface regression** (LOW per audit #4): CLI surface
  displays `cost_micro_octo_w` in JSON output. With v2.0 fields
  added (both `Option<[u8; 32]>`), CLI JSON serialization produces
  either `null` or `[u8, u8, ...]` byte array for the new fields.
  Mitigation: pin CLI JSON output in test fixtures; document
  additive schema in changelog.
- **Schema migration rollback** (LOW): v016 is purely additive
  (new NULL columns + new UNIQUE INDEX). Rollback = drop the
  2 columns + drop the UNIQUE INDEX. No data migration required.
- **Cargo workspace dep churn** (LOW): `quota-router-storage`
  gains `octo-cap-macaroon` dep (intra-Layer B per RFC-0957-A1
  §Layer Discipline). Per layer model, intra-Layer B dep is
  allowed.
- **Layer discipline preservation** (LOW): `settlement_verify.rs`
  takes `&dyn VaultLookup` (trait), not a concrete
  `&OctoVaultLookup` (struct). The trait lives in
  `octo-cap-macaroon` (Layer B extension); the impl lives in
  `octo-cap-macaroon-vault` (Layer B substrate adapter). The
  verifier never imports the substrate struct — Layer discipline
  preserved.

## Version history

| Date       | Author     | Change |
| ---------- | ---------- | ------ |
| 2026-08-17 | @mmacedoeu | Initial filing per audit verdict 2026-08-17 (storage restructure hard-recommendation #4, parallel-model Risks #5 HIGH + #6 MED). S6e sub-session. Co-filed with `0862-c9-micro-octow-type-unification.md` + `0105-x-s4-deferred-codemod-sites.md`. |
| 2026-08-19 | @mmacedoeu | Re-scoped per 2026-08-19 recon: corrected file paths (`SettlementEnvelope` lives in `crates/quota-router-storage/src/ask.rs:984`, NOT `crates/octo-protocol/src/settlement_envelope.rs`); corrected migration slot (v016, NOT v015 — taken by `chain_aware_slash_ledger`); corrected OctoVaultLookup location (already LANDED at `crates/octo-cap-macaroon-vault/src/octo_vault_lookup.rs`, NOT new file); expanded AC list from 7 → 10 to reflect recon-delimited surface; cross-ref RFC-0010 v1.6 (LANDED 2026-08-19). |

## Out of scope

- Slash ledger schema DQA(12) + chain_id column promotion
  (RFC-0900 amendment — S6d mission)
- Task market vault-id addendum (RFC-0918 amendment — S7 mission)
- Caveat payload codec DqaEncoding conversion for amount-bearing
  variants (RFC-0965 amendment — S7 mission)
- ZK circuit Dqa witness (RFC-0958 amendment — S7 mission)
- Vault substrate Model B spec text amendment (RFC-0960 amendment —
  S6f mission, 108 TV; substrate already LANDED per S3)
- Asset_id canonical derivation addendum (RFC-0105 amendment — S6g
  mission, 109 TV)
- Capability attenuation subsumption check (RFC-0957 §Verify-Time
  Extension step 4 — already LANDED via S5; no S6 work)
- Settlement envelope version_tag (RFC-0870 §NodeEnvelope Version Tag
  — already LANDED via S6a; no separate field needed on
  SettlementEnvelope since settlement is envelope-payload, not
  envelope-header)
- DFP / DECIMAL substrate (mission 0111) — off-limits per user
  constraint