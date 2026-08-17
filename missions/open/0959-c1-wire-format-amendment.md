# Mission: 0959-c1 — RFC-0959 wire-format amendment (S6e) — settlement DqaEncoding + VaultLookup reuse

## Status

**OPEN 2026-08-17 (@mmacedoeu).** Filed per audit verdict 2026-08-17
(storage restructure hard-recommendation #4). Closes audit Risk #5
(HIGH — vault-row lookup at 2 verify paths, only 1 LANDED) and Risk
#6 part (ChainId canonical mapping). S6e fifth sub-session per
`docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
§3 row 6 (Stream A.1 continuation; user-chosen S6 split-by-RFC
decision overrides §22 atomic-blocker bundle rule).

Pre-reqs verified landed: S3 (octo-vault crate LANDED), S4 (Dqa
codemod LANDED 2026-08-17 per
`S4-codemod-2026-08-17-LANDED.md`), S5 (verify-time invariant
LANDED 2026-08-17 commit `d007de54`), S6a (RFC-0870 v2.1
NodeEnvelope version_tag LANDED 2026-08-17), S6b (RFC-0957 v2.1
verify-time + Caveat DSL LANDED 2026-08-17 commits `c9149128` +
`4ec9779f` + `e5138420`), S6c (RFC-0862 v2.0 + c7 + c8 LANDED
2026-08-17).

## RFC

- Primary: RFC-0959 (Wire Format) — `SettlementEnvelope` DqaEncoding
  16-byte BE scale 12 per review §8.4.1 + §20.7 + §8.5.1
  precision-loss-tested round-trip fixtures
- Co-RFC: RFC-0960 (chain-aware bump) — `cost_vault_id` column
  reference + `chain_id` column in settlement_events schema per
  review §9.2 v004 sketch
- Co-RFC: RFC-0957 (verify-time bump) — `VaultLookup` trait reuse
  (audit hard-recommendation #4: settlement-time vault row lookup
  reuses same trait as capability verify-time per §20.7)

## Dependency edges

| From                                                                            | To                                        | Why                              | Layer direction     |
| ------------------------------------------------------------------------------- | ----------------------------------------- | -------------------------------- | ------------------- |
| `crates/octo-protocol/src/settlement_envelope.rs`                               | `octo_determin::DqaEncoding`              | Wire format                      | lib → lib           |
| `crates/quota-router-storage/src/settlement_event_repo.rs`                      | `octo_determin::Dqa`                      | In-memory field type             | lib → lib           |
| `crates/quota-router-storage/migrations/v004__create_settlement_events.sql`     | `cost DQA(12)` + `cost_vault_id BLOB(32)` | Storage column                   | n/a (schema only)   |
| `crates/octo-cap-macaroon/src/vault_lookup.rs` (re-use)                         | `OctoVaultLookup` adapter                 | Shared trait abstraction         | lib → lib (Layer B) |
| `crates/quota-router-storage/src/octo_vault_lookup.rs` (NEW per S5.1 follow-on) | `VaultLookup` trait                       | Settlement-time vault row lookup | lib → lib (Layer B) |

No new cyclic edges. New crate dep:
`quota-router-storage` gains `octo-vault` (already present per S3)

- `octo-cap-macaroon` (for `VaultLookup` trait re-export).

## Problem

Audit (2026-08-17) found two parallel-model risks at the
settlement wire-form layer:

**Risk A — storage column type divergence:**

- `crates/quota-router-storage/migrations/v004__create_settlement_events.sql:46`
  uses `cost_micro_octo_w BLOB NOT NULL` (16-byte BE u128)
- Per review §8.3.3 + §9.2 v004 sketch: should be `cost DQA(12) NOT
NULL` (scale 12 per §8.4.1 precision-loss analysis)
- Per review §20.7: settlement events gain `cost_vault_id BLOB(32)`
  column + `chain_id BLOB(32)` column for settlement-time
  cross-chain verification

**Risk B — vault row lookup at 2 paths, only 1 LANDED:**

- Capability verify-time path: `VaultLookup::lookup_vault(vault_id)`
  — LANDED via S5 commit `d007de54`
- Settlement-time path: spec §20.7 mandates "same vault-row
  UNIQUE INDEX lookup pattern"; no shared trait abstraction; risk
  of shadow impl that doesn't reuse `VaultLookup` trait

**Risk C — DFP codemod missed settlement_event_repo in-memory
field type:**

- `crates/quota-router-storage/src/settlement_event_repo.rs` still
  uses `cost_micro_octo_w: u128` field type (per audit Risk #4)
- This mission lands the field-type migration in concert with
  RFC-0959 wire-form amendment (single PR)

## Summary

RFC-0959 §Wire Format (accepted 2026-07-20) describes
`SettlementEnvelope` carrying `cost: u128` (16-byte BE). S4 DFP
codemod added `octo_determin::DqaEncoding` substrate for canonical
16-byte BE scale-12 wire form but did not wire it into
`SettlementEnvelope`. RFC-0959 v2.0 amendment (this mission)
extends the wire form to:

1. `cost: DqaEncoding` (16-byte BE scale 12) per §8.4.1
2. `cost_vault_id: [u8; 32]` field (NEW) per §20.7 — enables
   settlement-time vault row lookup for cross-chain settlement
   reject
3. `chain_id: [u8; 32]` field (NEW) — settlement envelope carries
   chain scope; settlement-time verify matches against
   `cost_vault_id` row's `chain_id` column

Per review §20.7 "settlement-time chain check uses the same
vault-row UNIQUE INDEX lookup pattern as §20.6.1 capability
verify-time, against `SettlementReceipt.cost_vault_id`".

**CRITICAL: settlement-time vault row lookup MUST reuse the
`VaultLookup` trait from `octo-cap-macaroon`.** No shadow impl.
Hard AC below.

Per RFC-0126 §backfill-precision pattern (referenced in §8.4.1),
25 byte-exact TV fixtures pin wire-form round-trip +
precision-loss-tested cases (per plan §3 S6 row 6 spec:
"RFC-0959: 25 wire-format").

## Acceptance Criteria

- AC-1: **RFC-0959 §Version History v2.0 row added** documenting:
  - `SettlementEnvelope.cost: DqaEncoding` (16-byte BE scale 12)
    replacing `u128` per §8.4.1
  - `SettlementEnvelope.cost_vault_id: [u8; 32]` NEW field per
    §20.7 cross-chain settlement reject
  - `SettlementEnvelope.chain_id: [u8; 32]` NEW field per §20.7
    chain scope carry
  - **VaultLookup trait reuse** for settlement-time vault row
    lookup — explicit cross-ref to `crates/octo-cap-macaroon/src/vault_lookup.rs`
  - Implementation mission: this file (`0959-c1-wire-format-amendment.md`)
  - Pre-req: S3 + S4 + S5 + S6a + S6b + S6c all LANDED 2026-08-17
- AC-2: **RFC-0959 §Wire Format subsection added** (new subsection
  under §Specification, after §Envelope Construction):
  - `SettlementEnvelope` v2.0 field list with byte offsets
  - `DqaEncoding` 16-byte BE encoding per
    `octo_determin::DqaEncoding::from_dqa` / `to_dqa`
  - `cost_vault_id` derivation cross-ref to RFC-0960 §vault_id
    canonical (`BLAKE3("cipherocto/vault/v1/" + chain_id +
owner_did + asset_id)`)
  - `chain_id` derivation cross-ref to RFC-0010 §ChainId 32-byte
    addendum (`BLAKE3("cipherocto/chain/v1/" + chain_string)`)
  - **Settlement-time vault row lookup algorithm** (3-step):
    1. `cost_vault_id` present? Else reject
    2. `vault_lookup.require_vault(cost_vault_id)` returns
       `VaultRowSnapshot { chain_id, is_active, ... }` — reuses
       `octo-cap-macaroon::vault_lookup::VaultLookup` trait
       directly (NO shadow impl)
    3. `vault_snapshot.chain_id == envelope.chain_id`? Else
       `SettlementError::ChainMismatch { vault_id, expected,
actual }`
  - Cross-ref to RFC-0957 §Verify-Time Extension (same trait
    shared)
- AC-3: **TV-0959-01..25 byte-exact fixtures** in
  `crates/octo-protocol/tests/tv_0959_settlement_wire.rs` (NEW):
  - **TV-0959-01..05**: `DqaEncoding` round-trip — `Dqa { value,
scale }` → 16-byte BE → `Dqa { value, scale }` byte-exact
  - **TV-0959-06..10**: precision-loss-tested — large values that
    would lose precision under scale-6 vs scale-12; round-trip
    preserves value (per RFC-0126 §backfill-precision)
  - **TV-0959-11..15**: `cost_vault_id` derivation cross-ref —
    verify `SettlementEnvelope.cost_vault_id` matches
    `octo_vault::vault_id(chain_id, owner_did, asset_id)` byte-exact
  - **TV-0959-16..20**: cross-chain settlement reject — envelope
    with `chain_id = "A"` + `cost_vault_id` whose row has
    `chain_id = "B"` → `SettlementError::ChainMismatch`
  - **TV-0959-21..25**: VaultLookup trait reuse — verify the
    settlement-time verifier constructs `OctoVaultLookup` and
    invokes `vault_lookup.require_vault(cost_vault_id)`; no
    separate `fn lookup_vault_row(&self, vault_id: &[u8; 32])`
    shadow impl in `quota-router-storage` (would defeat the
    abstraction)
- AC-4: **`v004__create_settlement_events.sql` migration v015 added**
  (per plan §9.2 v004 sketch):
  - `cost` column type: `BLOB NOT NULL` → `DQA(12) NOT NULL`
  - NEW column `cost_vault_id BLOB(32) NULL` per §20.7
  - NEW column `chain_id BLOB(32) NULL` per §20.7
  - NEW `CREATE UNIQUE INDEX idx_se_cost_vault_id ON
settlement_events(cost_vault_id)` per §20.7 audit query
  - Backfill per §9.4 RFC-0126 5-phase pattern (precision-loss
    tested)
- AC-5: **`settlement_event_repo.rs` in-memory field migration**:
  - `cost_micro_octo_w: u128` → `cost: Dqa`
  - `cost_vault_id: Option<[u8; 32]>` field NEW
  - `chain_id: Option<[u8; 32]>` field NEW
  - Wire codec: `DqaEncoding` 16-byte BE round-trip via
    `octo_determin::DqaEncoding::from_dqa` / `to_dqa`
  - Per `0105-x-s4-deferred-codemod-sites` AC-5: this is the
    in-memory field type migration (separate from x-mission because
    RFC-0959 wire-form change ships together)
- AC-6: **`OctoVaultLookup` adapter** in
  `crates/quota-router-storage/src/octo_vault_lookup.rs` (NEW per
  S5.1 follow-on `0957-g1-octo-vault-lookup-glue`):
  - Implements `octo_cap_macaroon::vault_lookup::VaultLookup` trait
  - Production impl uses Stoolap `vaults_vault_id_idx` UNIQUE INDEX
    lookup per review §20.6.1 option (b) + §20.7
  - Constructed with `Arc<dyn octo_vault::VaultStore>` (or similar
    Layer B substrate handle)
  - Settlement-time verifier holds `Arc<dyn VaultLookup>` and
    invokes `require_vault(cost_vault_id)` — trait abstraction
    shared with capability verify-time
- AC-7: Verification gate:
  ```bash
  cargo test -p octo-protocol --test tv_0959_settlement_wire  # 25/25 pass
  cargo test -p quota-router-storage --lib                  # no regressions (S6c 13/13 stay green)
  cargo test -p octo-cap-macaroon --lib                     # verify VaultLookup trait surface unchanged
  cargo test -p octo-vault --lib                            # no regressions (TV-V1 + TV-C1 stay green)
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
  commit `d007de54`)
- **Sibling missions:**
  - `missions/open/0862-c1-dqa-vault-bump-amendment.md` (S6c LANDED)
  - `missions/open/0870-c1-version-tag-amendment.md` (S6a LANDED)
  - `missions/open/0957-c1-verify-time-amendment.md` (S6b LANDED)
  - S6d RFC-0900 amendment (pending, 10 TV)
  - S6f RFC-0960 amendment (pending, 108 TV)
  - S6g RFC-0105 amendment (pending, 109 TV)
- **Pattern:** `crates/octo-cap-macaroon/src/vault_lookup.rs:62` —
  existing `VaultLookup` trait (S5 LANDED) — settlement-time
  verifier reuses this trait directly per AC-6
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

- `rfcs/accepted/messaging/0959-settlement-wire-format.md` (modify
  — §Version History v2.0 row + §Wire Format subsection +
  §Settlement-Time Vault Row Lookup subsection + §Cross-Chain
  Settlement Reject algorithm)
- `crates/octo-protocol/src/settlement_envelope.rs` (modify —
  v2.0 fields: `cost: DqaEncoding` + `cost_vault_id: [u8; 32]` +
  `chain_id: [u8; 32]`; wire codec update)
- `crates/octo-protocol/tests/tv_0959_settlement_wire.rs` (NEW —
  25 byte-exact fixtures per AC-3)
- `crates/quota-router-storage/src/settlement_event_repo.rs`
  (modify — in-memory field types per AC-5)
- `crates/quota-router-storage/migrations/v015__settlement_chain_vault.sql`
  (NEW — `cost DQA(12)` + `cost_vault_id BLOB(32)` + `chain_id
BLOB(32)` + UNIQUE INDEX per AC-4)
- `crates/quota-router-storage/src/octo_vault_lookup.rs` (NEW per
  S5.1 follow-on — implements `VaultLookup` trait for
  settlement-time verifier per AC-6)
- `crates/quota-router-storage/src/settlement_verify.rs` (NEW —
  settlement-time verifier invokes `OctoVaultLookup`; rejects on
  chain mismatch per AC-2 3-step algorithm)
- `crates/octo-vault/src/vault_id.rs` (cross-ref verify — `vault_id`
  derivation unchanged; settlement reuses production impl)
- `crates/octo-cap-macaroon/src/vault_lookup.rs` (NO modify — trait
  surface stays Layer B, no settlement-specific extensions added)

## Existing patterns reused

- `crates/octo-cap-macaroon/src/vault_lookup.rs::VaultLookup` trait
  (S5 LANDED) — settlement-time verifier reuses via `Arc<dyn
VaultLookup>` directly per AC-6 (no shadow impl)
- `crates/octo-determin::DqaEncoding::from_dqa` / `to_dqa` (per
  review §8.1.2) — wire codec round-trip
- `crates/octo-determin::Dqa::Display` (per review §8.1.2) — used
  for log + error message human-readable form
- `crates/quota-router-storage/migrations/v013__create_vaults.sql`
  PK shape + `vaults_vault_id_idx` UNIQUE INDEX — settlement-time
  verifier queries this index via `OctoVaultLookup`
- `crates/quota-router-storage/src/stoolap_spend_ledger.rs::dqa_to_i64`
  (S6c LANDED) — boundary helper pattern for I64 column conversion
- TV fixture byte-pin pattern from
  `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs`
  (S6c LANDED) — 25 fixtures follow same byte-pinned layout

## Risks

- **VaultLookup trait shadow impl** (HIGH per audit #5): the most
  important risk. If settlement-time verifier defines a private
  `fn lookup_vault_row(&self, ...)` instead of using
  `Arc<dyn VaultLookup>`, the abstraction is defeated and the
  verify-time invariant may diverge. Mitigation: AC-3 TV-0959-21..25
  pin the trait surface; AC-6 makes `OctoVaultLookup` adapter a
  CRITICAL pre-req for AC-7 verification gate.
- **Wire-form drift** (HIGH): `cost_micro_octo_w` storage column
  changes from `BLOB` to `DQA(12)` per AC-4. Migration v015 must
  backfill per RFC-0126 §backfill-precision pattern (precision-loss
  tested). All existing settlement_events rows need migration.
  Mitigation: 5-phase backfill pattern; precision-loss-tested TV
  fixtures (TV-0959-06..10).
- **CLI surface regression** (HIGH per audit #4): CLI surface
  displays `cost_micro_octo_w` in JSON output. With field type
  becoming `Dqa`, CLI JSON serialization must produce
  back-compatible output OR be a documented breaking change.
  Mitigation: pin CLI JSON output in test fixtures; document
  breaking change in changelog.
- **Schema migration rollback** (MED): v015 promotes `cost` column
  type. Rollback requires the backfill to be reversible. Mitigation:
  v015 includes a `cost_micro_octo_w BLOB` shadow column for
  rollback window (deprecate after 1 release).
- **Cargo workspace dep churn** (LOW): `quota-router-storage` gains
  `octo-cap-macaroon` dep (for `VaultLookup` trait); already has
  `octo-vault`. Per layer model, both are Layer B; intra-Layer B
  dep is allowed.
- **Co-mission dependency** (LOW): depends on
  `0862-c9-micro-octow-type-unification.md` (filed 2026-08-17)
  - `0105-x-s4-deferred-codemod-sites.md` (filed 2026-08-17) for
    in-memory field type migration. AC-5 in this mission ships the
    settlement_event_repo migration; x-mission covers the other 6
    file sites.

## Version history

| Date       | Author     | Change                                                                                                                                                                                                                                             |
| ---------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-08-17 | @mmacedoeu | Initial filing per audit verdict 2026-08-17 (storage restructure hard-recommendation #4, parallel-model Risks #5 HIGH + #6 MED). S6e sub-session. Co-filed with `0862-c9-micro-octow-type-unification.md` + `0105-x-s4-deferred-codemod-sites.md`. |

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
