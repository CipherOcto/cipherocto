---
rfc: 0959-v2.3
title: SettlementEnvelope burn_event wire form + DQA(12) cost_micro_octo_w migration
status: Accepted
version: 2.4
date: 2026-08-23
extends: RFC-0959 v2.0 (does not redefine — additive)
builds_on:
  - rfcs/accepted/economics/0959-ask-settlement-chain.md (v2.0)
  - rfcs/accepted/economics/0959-a1-market-delivery.md (A1)
  - rfcs/accepted/process/0206-v30-value-transfer-surface.md
  - docs/research/2026-08-21-vault-monetary-representation-redesign.md
---

# RFC-0959 v2.3 — SettlementEnvelope burn_event wire form

## 0. Status

**Accepted (v2.4, 2026-08-23).** EXTENDS RFC-0959 v2.0 (does not redefine). Additive to v2.0's `cost_vault_id: Option<[u8;32]>` + `chain_id: Option<[u8;32]>` fields.

**Promotion trail:** v2.1 initial draft 2026-08-22 → Accepted 2026-08-22 → v2.2 R5 fix-all 2026-08-23 → v2.3 R7 fix-all 2026-08-23 → v2.4 R9 fix-all 2026-08-23 per long-horizon plan v1.6 Phase 4 Tier 1 promotion sequence (RFC-0959 third in Tier 1 order per research doc §20 decision #9). BurnEventRef wire form + DQA(12) cost migration + litellm_users_spend view all preserved.

## 1. Motivation

RFC-0959 v2.0 defines `SettlementEvent.cost: MicroOCTO_W` where `MicroOCTO_W(pub u128)` is the newtype (per `rfcs/accepted/economics/0959-ask-settlement-chain.md` §SettlementEvent struct). The substrate SQL column `cost_micro_octo_w BLOB NOT NULL` (per `crates/quota-router-storage/migrations/v004__create_settlement_events.sql` §settlement_events table; column comment at v004 states "16-byte big-endian (`BLOB`)") stores the same u128 in 16-byte BE encoding. RFC-0959 v2.1 makes two additive changes:

1. **Add `burn_event: Option<BurnEventRef>`** — emitted when settlement references a finalized burn
2. **Migrate substrate SQL `cost_micro_octo_w` from `BLOB NOT NULL` (carrying 16-byte BE u128 per column comment) to `cost_micro_octo_w DQA(12)`** — eliminates the i64 bridge

**Substrate-vs-RFC column-type note (R9 surfaced):** The on-disk column type is `BLOB NOT NULL` (no length specifier); the "16-byte" payload size is a logical-payload description in the v004 column comment, not a column-shape constraint. Migrating to `cost_micro_octo_w DQA(12)` replaces the unconstrained BLOB with a typed DQA column.

**Substrate-vs-RFC divergence (struct side, R7 clarification):** Per S4 DFP codemod (memory card "S4 DFP codemod 2026-08-17"), substrate `SettlementEnvelope.cost` (per `crates/quota-router-storage/src/ask.rs` §SettlementEnvelope) and `SettlementEvent.cost` (per same file §SettlementEvent) have ALREADY migrated from `MicroOCTO_W(pub u128)` to `Dqa` with `#[serde(with = "crate::dqa_serde::field")]`. The `MicroOCTO_W(pub u128)` newtype is now RFC-defined only with no on-disk substrate consumer. The SQL-side BLOB→DQA(12) migration described below closes the on-disk format gap; the Rust side is already aligned.

## 2. BurnEventRef Specification

**Substrate status:** This struct is **RFC-defined only**; no on-disk implementation file exists (`crates/octo-vault/src/` currently contains only `lib.rs` and `migrations.rs`). Substrate-side impl location pending landing via mission 0206 v3.0 series.

```rust
// RFC-0959 §2 wire form (no on-disk impl yet — pending 0206 series)
pub struct BurnEventRef {
    pub burn_id: [u8;16],
    pub chain_id: [u8;32],   // chain_id[0] = NAMESPACE-BYTE OVERWRITE post-BLAKE3 per RFC-0206 v3.3 §2.3
                             // (e.g., 0x01 = Mainnet); the 31 bytes [1..32] are the BLAKE3 output
                             // per RFC-0010 §3 derive_chain_id. Consumers MUST NOT treat chain_id
                             // as a generic 32-byte BLAKE3 hash; the namespace byte at [0] is
                             // context-specific per the §2.5 disambiguation table.
    pub vault_id: [u8;32],   // vault_id[0] is also a BLAKE3 output byte (NOT a namespace-byte
                             // overwrite) per RFC-0206 v3.0 §3 ValueTransfer Trait vault_id
                             // parameter — distinct from chain_id[0] semantics; do not conflate.
    pub amount_dqa_micros: i64,        // matches ValueTransfer::burn_pending amount_dqa_micros
                                       // (snapshot at finalize_burn time per research §7.2
                                       //  linearized state machine; finalize_burn itself
                                       //  takes only burn_id per RFC-0206 v3.0 §3)
    pub burn_policy_hash: [u8;32],     // Substrate-vs-RFC divergence header:
                                       //   - RFC-defined field in BurnEventRef (this struct).
                                       //   - Substrate `transfer_events` v014 has NO inline policy column
                                       //     (`attributes BLOB NOT NULL` per §transfer_events table).
                                       //   - Substrate `burn_pending` ALREADY carries
                                       //     `burn_policy_hash BLOB(32) NOT NULL` per research §7.3
                                       //     (snapshot at insert time).
                                       //   - The burn_pending→policy_registry binding is declared in
                                       //     RFC-0206 v3.0 §4 (Substrate Migration v015–v018), not §5
                                       //     (§5 is the Execution Class Mapping table).
                                       //   - BurnEventRef's own burn_policy_hash field is RFC-defined
                                       //     only — no substrate column pending landing on
                                       //     transfer_events; the value SHOULD be copied from the
                                       //     referenced burn_pending row at finalize_burn time per
                                       //     research §7.3 substrate-snapshot semantics.
    pub finalized_at_unix: i64,        // matches transfer_events.occurred_at_unix (v014)
}
```

Wire form (DCS canonical per RFC-0126 Part 3):

```
{ "burn_id": [u8;16], "chain_id": [u8;32], "vault_id": [u8;32],
  "amount_dqa_micros": i64, "burn_policy_hash": [u8;32], "finalized_at_unix": i64 }
```

**Timing:** burn_event populated at `ValueTransfer::finalize_burn` time (NOT at `burn_pending` time). Balance snapshot reflects post-decrement state. The burn_event references the same `transfer_events` row that the burn inserted (`amount DQA(12)` column per §transfer_events table on-disk column shape).

## 3. DQA(12) Cost Migration

**Substrate status:** Migration pending landing via subsequent mission. No `v019__migrate_cost_to_dqa.sql` exists on disk; current `crates/quota-router-storage/migrations/` max is v016 (`v016__settlement_chain_vault.sql`); `crates/octo-vault/migrations/` max is v014 (`v014__create_transfer_events.sql`). Per research doc §11 Phase 1 (substrate) the v019 slot is reserved for additive rollback of `policy_registry`/`policy_kind_authority` DDL (RFC-0008 Accept-revert race) — mission implementer MUST coordinate v019 allocation with that scheduler before filing.

**Substrate-vs-RFC divergence (v019 slot allocation, R7 surfaced):** RFC-0206 v3.0 §4 (Substrate Migration v015–v018) lists v015=ValueTransfer trait / v016=burn_pending / v017=chain_metadata+policy_registry / v018=litellm_users+view — none of which match the actual on-disk migration numbering (v015=chain_aware_slash_ledger at quota-router-storage; v016=settlement_chain_vault at quota-router-storage). The v019 reservation is sensible only relative to RFC-0206 v3.0 §4 planned numbering, not actual on-disk numbering. Mission implementer MUST verify the actual on-disk max version (v016 at quota-router-storage, v014 at octo-vault) before allocating a migration number; renumbering vs substrate reality may be required if v015–v018 land with different numbering than RFC-0206 v3.0 §4 planned.

**Struct side already landed:** `SettlementEnvelope` (`crates/quota-router-storage/src/ask.rs` §SettlementEnvelope struct) already serializes `cost: Dqa` via `dqa_serde::field`. The struct side and the SQL side are decoupled through the serde adapter; this RFC's migration closes the SQL-side BLOB→DQA(12) gap. The struct side does NOT need to change.

### 3.1 Before (substrate, v004__create_settlement_events.sql)

```sql
-- v004__create_settlement_events.sql (current on-disk)
CREATE TABLE settlement_events (
    ...
    cost_micro_octo_w BLOB NOT NULL  -- 16-byte BE u128 (legacy wire form)
);
```

### 3.2 After (target shape, migration pending landing)

```sql
-- v0XX__migrate_cost_to_dqa.sql (migration number TBD per coordination)
CREATE TABLE settlement_events (
    ...
    cost_micro_octo_w DQA(12) NOT NULL  -- native DQA type per RFC-0105 numeric parent §SQL Column Scale Semantics
                                         -- (DQA(N) declares column scale = N fractional digits per §SQL Column Scale Semantics
                                         --  canonical interpretation; see substrate-vs-RFC divergence header below)
);
```

**Substrate-vs-RFC divergence (column-scale semantics, R7 surfaced + R9 marked inline):** RFC-0105 numeric parent §SQL Column Scale Semantics defines `DQA(N)` as a column with `N` fractional digits (e.g., `price DQA(6)` → stored with scale 6 → `123456` → `0.123456`). §3.2 of this RFC previously asserted `DQA(12) = DECIMAL(12,0) = 12 integer digits, scale=0` — this RFC-defined-extension interpretation is pending Stoolap substrate confirmation of DQA(N) column-scale semantics for integer-scale (scale=0) columns. §3.2 SQL block is **RFC-defined extension pending substrate landing** until Stoolap substrate confirms integer-scale DQA column support; the SQL block above is illustrative of the RFC interpretation, NOT a recommendation to apply RFC-0105 §SQL Column Scale Semantics canonical interpretation.

### 3.3 Migration path

**Substrate-vs-RFC divergence (format mismatch, R7 surfaced):** On-disk substrate `cost_micro_octo_w` (column type `BLOB NOT NULL` per `crates/quota-router-storage/migrations/v004__create_settlement_events.sql` §settlement_events table; column comment "16-byte big-endian u128" describes logical payload size) holds raw 16-byte big-endian u128. This is NOT the `DqaEncoding` wire form (8-byte i64 BE mantissa + 1-byte scale + 7-byte reserved per `crates/quota-router-storage/src/dqa_serde.rs` §DqaEncoding). Therefore `dqa_from_bytes` (which parses `DqaEncoding`) rejects every raw u128 BE row with `DqaError::InvalidEncoding`. The canonical migration path is format-conversion: read u128 BE → range-check → `Dqa::new(value_as_i64, 0)` — NOT `dqa_from_bytes`.

- **Source value:** `BLOB NOT NULL` carrying a 16-byte big-endian u128 (per v004 column comment). For migration to DQA column, the value must fit in `i64` because the canonical `Dqa` primitive (`Dqa::new(value: i64, scale: u8)` per `determin/src/dqa.rs` §Dqa struct) carries an `i64` mantissa. Implementer MUST range-check the u128 against `i64::MAX` before constructing the `Dqa`; values that exceed i64 (which a u128 MicroOCTO_W can, since u128::MAX ≈ 3.4e38 > i64::MAX ≈ 9.2e18) MUST be rejected with a typed migration error (not silently truncated).
- **Conversion primitive (canonical, R7 corrected):** For values that fit, the canonical BLOB (16-byte BE u128) → DQA conversion is direct u128 BE → i64 range-check → `Dqa::new(value_as_i64, 0)` per `determin/src/dqa.rs` §Dqa struct. The prior R5/R3 path via `dqa_from_bytes` was wrong because it parses `DqaEncoding` (RFC-0126 §DQA Serialization) not raw u128 BE; that helper is reserved for DqaEncoding wire-form decoding elsewhere.
- **Migration step:** For each row: read u128 BE → `if value > i64::MAX as u128 { emit MigrationError::CostOutOfRange } else { let d = Dqa::new(value as i64, 0); INSERT into cost_micro_octo_w column }`. On i64::MAX overflow, emit typed `MigrationError::CostOutOfRange`.

**Substrate-vs-RFC divergence (typed error variants, R7 surfaced):** §3.3 of this RFC prescribes typed `MigrationError::CostOutOfRange` and `MigrationError::InvalidDqaScale` variants. The substrate `MigrationError` enum in `crates/quota-router-storage/src/migrations.rs` currently has only `Storage(octo_storage_core::_legacy_StorageError)` and `UnknownMigration { version, catalog_max }` variants. Both RFC-defined variants are **RFC-defined extension pending substrate landing** via subsequent mission (out of scope for this RFC; mission implementer MUST add these variants to the substrate `MigrationError` enum as part of the migration landing).

- **Pre-conditions:** `Dqa::new(value, 0)` is the constructor (per `determin/src/dqa.rs` §Dqa struct); values > i64::MAX fail at `Dqa::new` and surface as `MigrationError::CostOutOfRange` per substrate migration runner convention. The substrate `determin/src/dqa.rs` pub fn list includes (non-exhaustive): `Dqa::new`, `Dqa::from_f64`, `Dqa::to_f64`, `Dqa::add`, `Dqa::subtract`, `Dqa::multiply`, `Dqa::divide`, `Dqa::negate`, `Dqa::absolute`, `Dqa::compare`; free fns `dqa_add`, `dqa_sub`, `dqa_mul`, `dqa_div`, `dqa_cmp`, `dqa_negate`, `dqa_abs`, `dqa_assign_to_column`; plus `DqaEncoding::from_dqa` + `DqaEncoding::to_dqa` (per `crates/quota-router-storage/src/dqa_serde.rs`). Implementer SHOULD use `dqa_assign_to_column` for the column-scale assignment (per RFC-0105 §SQL Column Scale Semantics §Expression-to-Column Assignment Coercion) rather than direct `Dqa::new` + manual scale alignment.
- **RFC-0126 anchor, NOT RFC-0105:** The DQA 16-byte wire form lives in RFC-0126 §DQA Serialization (per RFC-0105) — RFC-0105 numeric parent has NO `§DQA Serialization` heading (sections there include `### Canonical Representation` + `### SQL Integration` + `### SQL Column Scale Semantics`, but no serialization-named section). The R3 fix-all referenced a phantom `RFC-0105 §DQA Serialization` cite; R5 corrected the anchor to RFC-0126. R7 reiterates this anchor is correct: DqaEncoding wire form = RFC-0126 §DQA Serialization, NOT RFC-0105. The migration target column semantics = RFC-0105 §SQL Column Scale Semantics.
- **Scale semantics clarification:** Per RFC-0105 §SQL Column Scale Semantics, `DQA(N)` declares a column with `N` fractional digits (e.g., `price DQA(6)` → scale=6 → value 123456 stored as `0.123456`). The exact integer-scale (`scale=0`) DQA column support in Stoolap is RFC-defined extension pending substrate confirmation (see §3.2 substrate-vs-RFC header). A `cost_micro_octo_w` value of `1_000_000` (i.e., 1 OCTO-W expressed in MicroOCTO_W units) MUST encode as `Dqa { value: 1_000_000, scale: 0 }`. Per RFC-0105 §SQL Column Scale Semantics §Expression-to-Column Assignment Coercion algorithm, inserting `Dqa { value: 1_000_000, scale: 0 }` into a column declared `DQA(12)` (canonical interpretation = scale 12) coerces by padding: `intermediate = (1_000_000 as i128) * POW10[12] = 10^18`; the column would then store `10^18` with scale 12 (= 0.000000001 OCTO-W), NOT `1_000_000` with scale 0 (= 1 OCTO-W). Mission implementer MUST either (a) use `DQA(0)` (integer-scale DQA column, pending Stoolap substrate confirmation per §3.2 header) so `Dqa { value: 1_000_000, scale: 0 }` stores as `1_000_000`, OR (b) accept RFC-0105 canonical column-scale semantics and pre-scale values to fit `DQA(12)` (i.e., store `1_000_000` as `Dqa { value: 0, scale: 12 }` representing `0.000000001_000_000` OCTO-W units in fractional form, which does NOT match the RFC-0959 intent of integer MicroOCTO_W units). The §3.3 "stores 1,000,000" claim assumes interpretation (a); interpretation (b) contradicts the §3.2 + §3.3 RFC intent and is silently wrong for any reader who follows §3.2 DDL alone.
- **Verify:** `SELECT cost_micro_octo_w FROM settlement_events` returns non-zero DQA values for rows with non-zero cost. (`settlement_events` has no `amount` column — that column belongs to `transfer_events` v014; the verify query is intentionally column-restricted to `cost_micro_octo_w`.)
- **Old BLOB column dropped post-migration verification**

## 4. litellm_users.spend — Derived VIEW per R2 Finding

The litellm_users_spend view derives spend via JOIN over vault events. Per R5 substrate alignment (VH v2.2 row), the SQL uses `te.amount` matching the v14 substrate `transfer_events.amount DQA(12) NOT NULL` column (canonical source: `crates/octo-vault/migrations/v014__create_transfer_events.sql` §transfer_events table); research doc §5.3 historically described this view with `te.amount_dqa_micros` (a stale column name from a pre-v14 substrate draft). The v14 substrate column name `amount` is authoritative; research doc §5.3 is queued for amendment in a subsequent research-doc R-pass (out of scope for RFC-0959 v2.4 R9).

```sql
CREATE VIEW litellm_users_spend AS
SELECT
    lu.user_id,
    COALESCE(SUM(te.amount), 0) AS spend_dqa_micros
FROM litellm_users lu
LEFT JOIN vaults v ON v.owner_did = lu.user_id
LEFT JOIN transfer_events te ON te.from_vault_id = v.vault_id
                              AND te.event_type IN ('TransferApplied', 'Burn')
GROUP BY lu.user_id;
```

**R2 finding fix:** filter uses `'Burn'` (per RFC-0206 v3.0 + state machine linearization in research doc §7.2), NOT `'BurnFinalized'`. View references the SAME on-disk column shape as RFC-0960 v3.0 grand-design §2.5.

**Substrate-vs-RFC divergence header on event_type encoding:** Three sources currently disagree on the event_type column shape — they MUST be reconciled before Phase 1 ships.

| Source                                                              | Claim                                                                                                                                | Status                      |
| ------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ | --------------------------- |
| RFC-0960 v3.0 grand-design §2.5                                     | `event_type TEXT NOT NULL` with valid values enumerated as a trailing SQL `--` comment, NOT enforced via CHECK clause                | RFC target spec (post-v015) |
| Research doc §8.6                                                   | `event_type TEXT NOT NULL CHECK (event_type IN ('Mint','TransferApplied','TransferCorrected','Burn'))` — i.e., WITH CHECK constraint | Research-doc claim          |
| Substrate `v014__create_transfer_events.sql` §transfer_events table | `attributes BLOB NOT NULL` (no `event_type` column at all in v014)                                                                   | Current on-disk state       |

**R5 reconciliation decision:** The R3 RFC-0959 v2.1 framing claimed the research doc §8.6 description of `attributes BLOB` "reflects on-disk state" — this paraphrase is INCORRECT. Research doc §8.6 explicitly states the on-disk DDL uses `event_type TEXT NOT NULL CHECK (...)` — NOT `attributes BLOB`. The `attributes BLOB` is the actual v014 substrate column (§transfer_events table), which contradicts both the research doc §8.6 claim AND the RFC-0960 v3.0 grand-design §2.5 target spec.

For this RFC (RFC-0959 v2.3), the `litellm_users_spend` view MUST match RFC-0960 v3.0 grand-design §2.5 (the canonical target spec). Per R5: **`event_type TEXT NOT NULL`** with valid values documented as a trailing SQL `--` comment (no CHECK constraint) — diverging from research doc §8.6's CHECK-claim and aligning with RFC-0960 v3.0 grand-design. The research doc §8.6 CHECK assertion is a substrate-vs-RFC drift that MUST be flagged for research doc §8.6 amendment in a subsequent research-doc R-pass (out of scope for RFC-0959 v2.3 R5).

NOTE on substrate landing: the `event_type TEXT` column is NOT yet on disk at v014 (substrate currently exposes `attributes BLOB NOT NULL`); the TEXT shape is the post-v015+ migration target per RFC-0960 v3.0 grand-design §2.5. The v015+ migration is pending landing via mission 0206 v3.0 series (per research §10 Mission DAG Phase 1).

## 5. Execution Class Mapping (RFC-0008 §RFC-0008 Execution Class Mapping)

| Surface                                  | Class | Justification            |
| ---------------------------------------- | ----- | ------------------------ |
| SettlementEnvelope::burn_event           | A     | Deterministic reference  |
| cost_micro_octo_w BLOB→DQA(12) migration | A     | Deterministic conversion |
| litellm_users_spend view                 | A     | Deterministic sum        |

## 6. Cross-References

- RFC-0959 v2.0 (current wire form; `SettlementEvent.cost: MicroOCTO_W` newtype defined at RFC-0959 v2.0 §Data Structures; substrate has since migrated to Dqa per S4 DFP codemod)
- RFC-0960 v3.0 grand-design §2.5 (transfer_events.event_type TEXT column — pending landing via v015+ migration)
- RFC-0105 numeric parent §SQL Column Scale Semantics (DQA(N) column-scale semantics — canonical interpretation: N fractional digits)
- RFC-0105 numeric parent §SQL Integration (DQA arithmetic examples; NOT column-scale semantics anchor)
- RFC-0126 §DQA Serialization (per RFC-0105) (16-byte BE DqaEncoding wire form — distinct from raw u128 BE substrate format)
- RFC-0126 Part 3 §Deterministic Canonical Serialization (DCS wire form)
- RFC-0206 v3.0 §3 ValueTransfer Trait (burn_event source: `burn_pending` sets amount, `finalize_burn` consumes burn_id per state machine)
- RFC-0206 v3.0 §4 Substrate Migration v015–v018 (policy_registry DDL + burn_pending→policy_registry binding; actual on-disk numbering diverges per §3 substrate-vs-RFC divergence header)
- RFC-0206 v3.3 §2.3 chain_id[0] NAMESPACE-BYTE OVERWRITE semantics (overwrite post-BLAKE3 per RFC-0010 §3 derive_chain_id; e.g., 0x01 = Mainnet) — applies to BurnEventRef.chain_id per §2 struct comment
- RFC-0206 v3.3 §2.5 `0x01` namespace byte disambiguation table (chain_id namespace-byte vs asset_id namespace-byte vs ExecutionClass enum discriminant)
- `docs/research/2026-08-21-vault-monetary-representation-redesign.md` §5.3 + §7.2 + §7.3 + §8.6 + §11 Phase 1 (v019 rollback slot) + §9 amendment table + §20 decision #9 (RFC promotion priority order)

## 7. Version History

| Version | Date | Change |
| ------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 2.1 | 2026-08-22 | Initial draft. Additive to v2.0. Adds burn_event wire form. Migrates cost_micro_octo_w BLOB→DQA(12). Resolves R2 finding on litellm_users_spend view filter (uses 'Burn' not 'BurnFinalized') + on-disk event_type column encoding (maintains TEXT per RFC-0960 v3.0 grand-design §2.5; pending landing via v015+ migration). R16 promotion: Draft → Accepted per long-horizon plan v1.6 Phase 4 Tier 1 promotion sequence. |
| 2.2 | 2026-08-23 | **R5 fix-all (consolidates R3 fix-all content):** Substrate grounding + cite cleanup. Phantom `crates/octo-vault/src/burn_event_ref.rs` path stripped (RFC-defined only). Phantom `ValueTransfer::finalize_burn transfer_event amount` comment corrected to `burn_pending` amount per RFC-0206 v3.0 §3 + research §7.2 state machine. Phantom `burn_policy_hash` field marked RFC-defined pending substrate column. Phantom `v019__migrate_cost_to_dqa.sql` flagged pending landing + v019 slot reservation conflict noted. Phantom `dqa_from_u128` removed. R3's phantom `RFC-0105 §DQA Serialization` cite corrected to `RFC-0126 §DQA Serialization (per RFC-0105)`. Wire format cite fixed (was RFC-0126 listed as CBOR → RFC-0126 Part 3 §DCS). Phantom `amount` column in verification SQL removed (settlement_events has no `amount` column; cost_micro_octo_w is the correct column). VIEW `te.amount_dqa_micros` → `te.amount` (matches v014 substrate). Fabricated CHECK constraint on `event_type` removed (RFC-0960 v3.0 §2.5 has TEXT with values in trailing `--` comment, NOT CHECK clause). Wrong §2.5 cite fixed to specify RFC-0960 v3.0 grand-design (v3.1 amendment has no §2.5). YAML `builds_on` path corrected `rfcs/draft/process/` → `rfcs/accepted/process/`. SettlementEnvelope struct-side serde adapter clarification added. Phantom `Dqa::from_be_bytes_scale0` removed; conversion routed through canonical substrate primitives `Dqa::new(value: i64, scale: u8)` + `DqaEncoding::to_dqa()` per `determin/src/dqa.rs` §Dqa struct (free helper `dqa_from_bytes` in `crates/quota-router-storage/src/dqa_serde.rs`). DQA(N) column-scale semantics clarified: DQA(12) = DECIMAL(12,0) = 12 integer digits, scale=0 (RFC-0105 numeric parent §SQL Integration). u128 → i64 range-check requirement made explicit (u128::MAX > i64::MAX; implementer MUST surface typed `MigrationError::CostOutOfRange` for out-of-range rows, not silently truncate). Phantom research doc `§B.3` cite corrected to `research doc §11 Phase 1`. Wrong RFC-0206 v3.0 `§5` cite for policy_registry binding corrected to `§4 Substrate Migration v015–v018`. `§1 Motivation` disambiguates Rust struct (`MicroOCTO_W(pub u128)`) from substrate SQL (`BLOB NOT NULL`). `burn_policy_hash` framing augmented with substrate-vs-RFC divergence header noting `burn_pending` ALREADY carries the field per research §7.3. §4 event_type divergence table added reconciling three sources (RFC-0960 §2.5 / research §8.6 / v014 substrate). YAML `date` + VH row aligned to 2026-08-23; version bumped 2.1 → 2.2 (resolves R4 VH two-row-same-version defect by consolidating R3 into v2.2 single row). |
| 2.3 | 2026-08-23 | **R7 fix-all (R6 findings):** 23 R6 findings applied. (a) §1 Motivation struct attribution corrected: `SettlementEvent.cost: MicroOCTO_W` (per RFC-0959 v2.0 §Data Structures), NOT `SettlementEnvelope.cost`; substrate path corrected `crates/octo-vault/migrations/v004__...` → `crates/quota-router-storage/migrations/v004__...`; line-225 anchor replaced with bare §Data Structures reference (no-line-refs-anywhere). (b) §1 Motivation substrate-vs-RFC divergence header added: substrate `SettlementEnvelope.cost` + `SettlementEvent.cost` already migrated to `Dqa` per S4 DFP codemod (memory card "S4 DFP codemod 2026-08-17"); `MicroOCTO_W(pub u128)` newtype now RFC-defined only with no on-disk substrate consumer. (c) §3.2 substrate-vs-RFC divergence header added: RFC-0105 numeric parent §SQL Column Scale Semantics defines `DQA(N)` = N fractional digits (e.g., `price DQA(6)` → scale=6 → `0.123456`); RFC-0959 v2.2's `DQA(12) = 12 integer digits, scale=0` interpretation is RFC-defined extension pending Stoolap substrate confirmation of integer-scale DQA column support. (d) §3.3 migration primitive corrected: `dqa_from_bytes` parses `DqaEncoding` (8-byte i64 BE + 1-byte scale + 7-byte reserved) which is NOT the on-disk u128 BE format; canonical path is direct u128 BE → range-check → `Dqa::new(value_as_i64, 0)`. Substrate-vs-RFC divergence header added for `MigrationError::CostOutOfRange` + `MigrationError::InvalidDqaScale` typed error variants (substrate `MigrationError` enum in `crates/quota-router-storage/src/migrations.rs` currently has only `Storage` + `UnknownMigration` variants). Substrate-vs-RFC divergence added for v019 slot allocation conflict (RFC-0206 v3.0 §4 planned numbering v015-v018 vs actual on-disk v015=chain_aware_slash_ledger / v016=settlement_chain_vault). (e) §3.3 substrate pub fn list corrected from overstated "only 4 fns" to non-exhaustive enumeration. (f) §6 Cross-References reorganized: RFC-0105 §SQL Column Scale Semantics cited as canonical DQA(N) anchor; §SQL Integration cited only as arithmetic examples; §Data Structures reference added; §20 decision #9 cite verified (research doc §20). (g) §7 VH table deduplicated: R3 row consolidated into R5/v2.2 single row (eliminates two-v2.1-rows defect). R7/v2.3 row added. YAML version: 2.2 → 2.3; date: 2026-08-23 (unchanged). |
| 2.4 | 2026-08-23 | **R9 fix-all (R8 findings):** 22 R8 findings applied. (a) §0 Status header v2.2 → v2.4 (R8 surfaced stale-version defect despite YAML v2.3 + VH v2.3 row); YAML `version: 2.3 → 2.4`; `date:` 2026-08-23 (unchanged). (b) §0 Promotion trail extended with v2.4 R9 fix-all entry. (c) §1 Motivation substrate-vs-RFC column-type note: on-disk `cost_micro_octo_w` column type is `BLOB NOT NULL` (no length specifier), NOT `BLOB(16)`; the "16-byte" payload size is a logical-payload description in the v004 column comment, not a column-shape constraint; migrated DDL retargeted from `BLOB(16)` (RFC claim) to `BLOB NOT NULL` carrying 16-byte BE u128 (substrate truth). (d) Line-ref anchors stripped throughout per `no-line-refs-anywhere.md`: L51 `(line 716)` research §7.3 → bare §section ref; L80 `~line 1024` ask.rs → §SettlementEnvelope struct; L104 `(line 807)` RFC-0105 §SQL Column Scale Semantics → bare §section ref; L145 table cell `(line 992-1008)` research §8.6 → bare §section ref; L146 `v014__create_transfer_events.sql:20` → §transfer_events table on-disk column shape; L148 `(line 992-1008)` research §8.6 + `(line 20)` v014 column → §section ref + §transfer_events table; L72 `:26` v014 amount column → §transfer_events table. (e) §2 BurnEventRef struct updated: `chain_id: [u8;32]` annotated with RFC-0206 v3.3 §2.3 NAMESPACE-BYTE OVERWRITE semantics (chain_id[0] = 0x01 Mainnet namespace byte overwrite post-BLAKE3 per RFC-0010 §3 derive_chain_id; 31 bytes [1..32] are the BLAKE3 output). (f) `vault_id: [u8;32]` annotated as BLAKE3 output (no namespace-byte overwrite) to contrast with chain_id. (g) §3.2 inline marker added clarifying SQL block is RFC-defined extension pending substrate landing (not RFC-0105 §SQL Column Scale Semantics canonical). (h) §3.3 scale-semantics claim reconciled: per RFC-0105 §Expression-to-Column Assignment Coercion, `Dqa { value: 1_000_000, scale: 0 }` in `DQA(12)` (canonical scale 12) coerces to `10^18` with scale 12 (= 0.000000001 OCTO-W), NOT `1_000_000` with scale 0; mission implementer MUST either (a) use `DQA(0)` integer-scale (pending Stoolap substrate confirmation per §3.2) or (b) accept RFC-0105 canonical and pre-scale to fractional form. (i) §4 litellm_users_spend view cite acknowledged: research doc §5.3 references `te.amount_dqa_micros` (stale pre-v14 column name); SQL uses `te.amount` matching on-disk v14 substrate `transfer_events.amount DQA(12) NOT NULL`. (j) §6 Cross-References expanded: added RFC-0206 v3.3 §2.3 chain_id namespace-byte overwrite cite; added RFC-0206 v3.3 §2.5 disambiguation table cite. (k) VH v2.3 row cleaned: `(research doc §20 exists at line 1772)` anchor stripped per no-line-refs-anywhere rule. YAML version: 2.3 → 2.4; date: 2026-08-23 (unchanged); §0 Status block updated to v2.4 + R9 fix-all entry. |
