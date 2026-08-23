---
rfc: 0959-v2.2
title: SettlementEnvelope burn_event wire form + DQA(12) cost_micro_octo_w migration
status: Accepted
version: 2.2
date: 2026-08-23
extends: RFC-0959 v2.0 (does not redefine — additive)
builds_on:
  - rfcs/accepted/economics/0959-ask-settlement-chain.md (v2.0)
  - rfcs/accepted/economics/0959-a1-market-delivery.md (A1)
  - rfcs/accepted/process/0206-v30-value-transfer-surface.md
  - docs/research/2026-08-21-vault-monetary-representation-redesign.md
---

# RFC-0959 v2.2 — SettlementEnvelope burn_event wire form

## 0. Status

**Accepted (v2.2, 2026-08-23).** EXTENDS RFC-0959 v2.0 (does not redefine). Additive to v2.0's `cost_vault_id: Option<[u8;32]>` + `chain_id: Option<[u8;32]>` fields.

**Promotion trail:** v2.1 initial draft 2026-08-22 → Accepted 2026-08-22 → v2.2 R5 fix-all 2026-08-23 per long-horizon plan v1.6 Phase 4 Tier 1 promotion sequence (RFC-0959 v2.2 third in Tier 1 order per research §20 decision #9). BurnEventRef wire form + DQA(12) cost migration + litellm_users_spend view all preserved.

## 1. Motivation

RFC-0959 v2.0 defines the SettlementEnvelope Rust wire form with `cost: MicroOCTO_W(pub u128)` (per `rfcs/accepted/economics/0959-ask-settlement-chain.md` line 225). The substrate SQL column `cost_micro_octo_w BLOB NOT NULL` (per `crates/octo-vault/migrations/v004__create_settlement_events.sql` line 56) stores the same u128 in 16-byte BE encoding. RFC-0959 v2.1 makes two additive changes:

1. **Add `burn_event: Option<BurnEventRef>`** — emitted when settlement references a finalized burn
2. **Migrate substrate SQL `cost_micro_octo_w BLOB(16)` to `cost_micro_octo_w DQA(12)`** — eliminates the i64 bridge (the Rust `MicroOCTO_W(pub u128)` newtype side is unchanged; the struct side already serializes through `dqa_serde::field` and stores the same value at scale=0)

## 2. BurnEventRef Specification

**Substrate status:** This struct is **RFC-defined only**; no on-disk implementation file exists (`crates/octo-vault/src/` currently contains only `lib.rs` and `migrations.rs`). Substrate-side impl location pending landing via mission 0206 v3.0 series.

```rust
// RFC-0959 v2.2 §2 wire form (no on-disk impl yet — pending 0206 series)
pub struct BurnEventRef {
    pub burn_id: [u8;16],
    pub chain_id: [u8;32],
    pub vault_id: [u8;32],
    pub amount_dqa_micros: i64,        // matches ValueTransfer::burn_pending amount_dqa_micros
                                       // (snapshot at finalize_burn time per research §7.2
                                       //  linearized state machine; finalize_burn itself
                                       //  takes only burn_id per RFC-0206 v3.0 §3)
    pub burn_policy_hash: [u8;32],     // Substrate-vs-RFC divergence header:
                                       //   - RFC-defined field in BurnEventRef (this struct).
                                       //   - Substrate `transfer_events` v014 has NO inline policy column
                                       //     (`attributes BLOB NOT NULL` per v014__create_transfer_events.sql:20).
                                       //   - Substrate `burn_pending` ALREADY carries
                                       //     `burn_policy_hash BLOB(32) NOT NULL` per research §7.3 (line 716,
                                       //     snapshot at insert time).
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

**Timing:** burn_event populated at `ValueTransfer::finalize_burn` time (NOT at `burn_pending` time). Balance snapshot reflects post-decrement state. The burn_event references the same `transfer_events` row that the burn inserted (`amount DQA(12)` column per v014__create_transfer_events.sql:26).

## 3. DQA(12) Cost Migration

**Substrate status:** Migration pending landing via subsequent mission. No `v019__migrate_cost_to_dqa.sql` exists on disk; current `crates/quota-router-storage/migrations/` max is v016 (`v016__settlement_chain_vault.sql`); `crates/octo-vault/migrations/` max is v014 (`v014__create_transfer_events.sql`). Per research doc §11 Phase 1 (substrate, line 1367) the v019 slot is reserved for additive rollback of `policy_registry`/`policy_kind_authority` DDL (RFC-0008 Accept-revert race) — mission implementer MUST coordinate v019 allocation with that scheduler before filing.

**Struct side already landed:** `SettlementEnvelope` (`crates/quota-router-storage/src/ask.rs` ~line 1024) already serializes `cost: Dqa` via `dqa_serde::field`. The struct side and the SQL side are decoupled through the serde adapter; this RFC's migration closes the SQL-side BLOB→DQA(12) gap. The struct side does NOT need to change.

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
    cost_micro_octo_w DQA(12) NOT NULL  -- native DQA type per RFC-0105 numeric parent §SQL Integration
                                         -- (DQA(12) = DECIMAL(12,0) per Stoolap = 12 integer digits, scale=0)
);
```

### 3.3 Migration path

- **Source value:** BLOB(16) holds a big-endian u128 (16-byte BE u128). For migration to DQA(12) (scale=0), the value must fit in `i64` because the canonical `Dqa` primitive (`Dqa::new(value: i64, scale: u8)` per `determin/src/dqa.rs:197`) carries an `i64` mantissa. Implementer MUST range-check the u128 against `i64::MAX` before constructing the `Dqa`; values that exceed i64 (which a u128 MicroOCTO_W can, since u128::MAX ≈ 3.4e38 > i64::MAX ≈ 9.2e18) MUST be rejected with a typed migration error (not silently truncated).
- **Conversion primitive (canonical):** For values that fit, the canonical BLOB(16) → DQA conversion goes through the substrate `DqaEncoding` wire form defined in `RFC-0126 §DQA Serialization (per RFC-0105)` (numeric parent `rfcs/accepted/numeric/0126-deterministic-serialization.md` line 476). The Rust substrate helper lives at `crates/quota-router-storage/src/dqa_serde.rs::dqa_from_bytes` (16-byte BE → `Dqa` via `DqaEncoding::to_dqa()`).
- **Migration step:** For each row: `dqa_from_bytes(&row.blob_16)` returns `Result<Dqa, DqaError>`; on `Ok(d)` with `d.scale == 0`, INSERT into new `cost_micro_octo_w DQA(12)` column. On `Err(_)` or non-zero scale, emit typed `MigrationError::CostOutOfRange` / `MigrationError::InvalidDqaScale`.
- **Pre-conditions:** `Dqa::new(value, 0)` is the constructor (per `determin/src/dqa.rs:197`); values > i64::MAX fail at `Dqa::new` and surface as `MigrationError::CostOutOfRange` per substrate migration runner convention. The prior R3 text cited a phantom `Dqa::from_be_bytes_scale0` method that does NOT exist in substrate (`determin/src/dqa.rs` pub fn list contains only `Dqa::new`, `Dqa::from_f64`, `DqaEncoding::from_dqa`, `DqaEncoding::to_dqa`); R5 corrects to the actual canonical primitives (`Dqa::new` + `DqaEncoding::to_dqa`).
- **RFC-0126 anchor, NOT RFC-0105:** The DQA 16-byte wire form lives in RFC-0126 §DQA Serialization (per RFC-0105) — RFC-0105 numeric parent has NO `§DQA Serialization` heading (sections there are `### Canonical Representation` at line 587 + `### SQL Integration` at line 220, but no serialization-named section). The R3 fix-all referenced a phantom `RFC-0105 §DQA Serialization` cite; R5 corrects the anchor to RFC-0126.
- **Scale semantics clarification:** DQA(12) is DECIMAL(12,0) — 12 integer digits, scale=0. The parenthetical "(scale=0)" in the prior text referred to the DQA mantissa scale, NOT the column scale; column-scale notation `DQA(N)` where N is the total digit count means `DECIMAL(N,0)`. A `cost_micro_octo_w` value of `1_000_000` (i.e., 1 OCTO-W expressed in MicroOCTO_W units) MUST encode as `Dqa { value: 1_000_000, scale: 0 }` so the column stores 1,000,000 (1 OCTO-W).
- **Verify:** `SELECT cost_micro_octo_w FROM settlement_events` returns non-zero DQA(12) values for rows with non-zero cost. (`settlement_events` has no `amount` column — that column belongs to `transfer_events` v014; the verify query is intentionally column-restricted to `cost_micro_octo_w`.)
- **Old BLOB column dropped post-migration verification**

## 4. litellm_users.spend — Derived VIEW per R2 Finding

The litellm_users_spend view (research doc §5.3) derives spend via JOIN over vault events:

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

| Source                                          | Claim                                                                                                                                | Status                      |
| ----------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ | --------------------------- |
| RFC-0960 v3.0 grand-design §2.5                 | `event_type TEXT NOT NULL` with valid values enumerated as a trailing SQL `--` comment, NOT enforced via CHECK clause                | RFC target spec (post-v015) |
| Research doc §8.6 (line 992-1008)               | `event_type TEXT NOT NULL CHECK (event_type IN ('Mint','TransferApplied','TransferCorrected','Burn'))` — i.e., WITH CHECK constraint | Research-doc claim          |
| Substrate `v014__create_transfer_events.sql:20` | `attributes BLOB NOT NULL` (no `event_type` column at all in v014)                                                                   | Current on-disk state       |

**R5 reconciliation decision:** The R3 RFC-0959 v2.1 framing claimed the research doc §8.6 description of `attributes BLOB` "reflects on-disk state" — this paraphrase is INCORRECT. Research doc §8.6 (line 992-1008) explicitly states the on-disk DDL uses `event_type TEXT NOT NULL CHECK (...)` — NOT `attributes BLOB`. The `attributes BLOB` is the actual v014 substrate column (line 20), which contradicts both the research doc §8.6 claim AND the RFC-0960 v3.0 grand-design §2.5 target spec.

For this RFC (RFC-0959 v2.2), the `litellm_users_spend` view MUST match RFC-0960 v3.0 grand-design §2.5 (the canonical target spec). Per R5: **`event_type TEXT NOT NULL`** with valid values documented as a trailing SQL `--` comment (no CHECK constraint) — diverging from research doc §8.6's CHECK-claim and aligning with RFC-0960 v3.0 grand-design. The research doc §8.6 CHECK assertion is a substrate-vs-RFC drift that MUST be flagged for research doc §8.6 amendment in a subsequent research-doc R-pass (out of scope for RFC-0959 v2.2 R5).

NOTE on substrate landing: the `event_type TEXT` column is NOT yet on disk at v014 (substrate currently exposes `attributes BLOB NOT NULL`); the TEXT shape is the post-v015+ migration target per RFC-0960 v3.0 grand-design §2.5. The v015+ migration is pending landing via mission 0206 v3.0 series (per research §10 Mission DAG Phase 1).

## 5. Execution Class Mapping (RFC-0008 §RFC-0008 Execution Class Mapping)

| Surface                                  | Class | Justification            |
| ---------------------------------------- | ----- | ------------------------ |
| SettlementEnvelope::burn_event           | A     | Deterministic reference  |
| cost_micro_octo_w BLOB→DQA(12) migration | A     | Deterministic conversion |
| litellm_users_spend view                 | A     | Deterministic sum        |

## 6. Cross-References

- RFC-0959 v2.0 (current wire form)
- RFC-0960 v3.0 grand-design §2.5 (transfer_events.event_type TEXT column — pending landing via v015+ migration)
- RFC-0105 numeric parent §SQL Integration (DQA(N) column-scale semantics)
- RFC-0126 §DQA Serialization (per RFC-0105) (16-byte BE DqaEncoding wire form)
- RFC-0126 Part 3 §Deterministic Canonical Serialization (DCS wire form)
- RFC-0206 v3.0 §3 ValueTransfer Trait (burn_event source: `burn_pending` sets amount, `finalize_burn` consumes burn_id per state machine)
- RFC-0206 v3.0 §4 Substrate Migration v015–v018 (policy_registry DDL + burn_pending→policy_registry binding)
- `docs/research/2026-08-21-vault-monetary-representation-redesign.md` §5.3 + §7.2 + §7.3 + §8.6 + §11 Phase 1 (v019 rollback slot) + §9 amendment table

## 7. Version History

| Version | Date | Change |
| ------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 2.1 | 2026-08-22 | Initial draft. Additive to v2.0. Adds burn_event wire form. Migrates cost_micro_octo_w BLOB→DQA(12). Resolves R2 finding on litellm_users_spend view filter (uses 'Burn' not 'BurnFinalized') + on-disk event_type column encoding (maintains TEXT per RFC-0960 v3.0 grand-design §2.5; pending landing via v015+ migration). R16 promotion: Draft → Accepted per long-horizon plan v1.6 Phase 4 Tier 1 promotion sequence. |
| 2.1 | 2026-08-23 | **R3 fix-all:** Substrate grounding + cite cleanup. Phantom `crates/octo-vault/src/burn_event_ref.rs` path stripped (RFC-defined only). Phantom `ValueTransfer::finalize_burn transfer_event amount` comment corrected to `burn_pending` amount per RFC-0206 v3.0 §3 + research §7.2 state machine. Phantom `burn_policy_hash` field marked RFC-defined pending substrate column. Phantom `v019__migrate_cost_to_dqa.sql` flagged pending landing + v019 slot reservation conflict noted. Phantom `dqa_from_u128` removed. **NOTE (R5 honest assessment):** R3's replacement DQA primitive cite `RFC-0105 §DQA Serialization` was itself a phantom anchor (no such section exists in RFC-0105 numeric parent or RFC-0105 v3.1 economics child). R5 corrects to `RFC-0126 §DQA Serialization (per RFC-0105)` + `RFC-0105 numeric parent §SQL Integration`. Wire format cite fixed (was RFC-0126 listed as CBOR → RFC-0126 Part 3 §DCS). Phantom `amount` column in verification SQL removed (settlement_events has no `amount` column; cost_micro_octo_w is the correct column). VIEW `te.amount_dqa_micros` → `te.amount` (matches v014 substrate). Fabricated CHECK constraint on `event_type` removed (RFC-0960 v3.0 §2.5 has TEXT with values in trailing `--` comment, NOT CHECK clause). Wrong §2.5 cite fixed to specify RFC-0960 v3.0 grand-design (v3.1 amendment has no §2.5). YAML `builds_on` path corrected `rfcs/draft/process/` → `rfcs/accepted/process/`. SettlementEnvelope struct-side serde adapter clarification added. |
| 2.2 | 2026-08-23 | **R5 fix-all:** Phantom `Dqa::from_be_bytes_scale0` removed; conversion routed through canonical substrate primitives `Dqa::new(value: i64, scale: u8)` + `DqaEncoding::to_dqa()` per `determin/src/dqa.rs:197/597` (free helper `dqa_from_bytes` in `crates/quota-router-storage/src/dqa_serde.rs`). DQA(N) column-scale semantics clarified: DQA(12) = DECIMAL(12,0) = 12 integer digits, scale=0 (RFC-0105 numeric parent §SQL Integration). u128 → i64 range-check requirement made explicit (u128::MAX > i64::MAX; implementer MUST surface typed `MigrationError::CostOutOfRange` for out-of-range rows, not silently truncate). Phantom `RFC-0105 §DQA Serialization` cite (lines 89 + 125) corrected to `RFC-0126 §DQA Serialization (per RFC-0105)`. Phantom research doc `§B.3` cite (line 63) corrected to `research doc §11 Phase 1 (line 1367)`. Wrong RFC-0206 v3.0 `§5` cite for policy_registry binding (line 47) corrected to `§4 Substrate Migration v015–v018`. `§1 Motivation` disambiguates Rust struct (`MicroOCTO_W(pub u128)`) from substrate SQL (`BLOB NOT NULL`) — RFC-0959 v2.0 defines the former, the migration target is the latter. `burn_policy_hash` framing augmented with substrate-vs-RFC divergence header noting `burn_pending` ALREADY carries the field per research §7.3 (line 716). §4 event_type divergence table added reconciling three sources (RFC-0960 §2.5 / research §8.6 / v014 substrate). YAML `date` + VH row aligned to 2026-08-23; version bumped 2.1 → 2.2 for R5 (resolves R4 VH two-row-same-version defect). |
