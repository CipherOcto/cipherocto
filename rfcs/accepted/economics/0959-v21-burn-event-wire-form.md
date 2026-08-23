---
rfc: 0959-v2.1
title: SettlementEnvelope burn_event wire form + DQA(12) cost_micro_octo_w migration
status: Accepted
version: 2.1
date: 2026-08-22
extends: RFC-0959 v2.0 (does not redefine — additive)
builds_on:
  - rfcs/accepted/economics/0959-ask-settlement-chain.md (v2.0)
  - rfcs/accepted/economics/0959-a1-market-delivery.md (A1)
  - rfcs/accepted/process/0206-v30-value-transfer-surface.md
  - docs/research/2026-08-21-vault-monetary-representation-redesign.md
---

# RFC-0959 v2.1 — SettlementEnvelope burn_event wire form

## 0. Status

**Accepted (v2.1, 2026-08-22).** EXTENDS RFC-0959 v2.0 (does not redefine). Additive to v2.0's `cost_vault_id: Option<[u8;32]>` + `chain_id: Option<[u8;32]>` fields.

**Promotion trail:** v2.1 initial draft 2026-08-22 → Accepted 2026-08-22 per long-horizon plan v1.6 Phase 4 Tier 1 promotion sequence (RFC-0959 v2.1 third in Tier 1 order per research §20 decision #9). BurnEventRef wire form + DQA(12) cost migration + litellm_users_spend view all preserved.

## 1. Motivation

RFC-0959 v2.0 defines the SettlementEnvelope wire form with cost_micro_octo_w as `BLOB(16)` (16-byte BE u128). RFC-0959 v2.1 makes two additive changes:

1. **Add `burn_event: Option<BurnEventRef>`** — emitted when settlement references a finalized burn
2. **Migrate `cost_micro_octo_w BLOB(16)` to `cost_micro_octo_w DQA(12)`** — eliminates the i64 bridge

## 2. BurnEventRef Specification

**Substrate status:** This struct is **RFC-defined only**; no on-disk implementation file exists (`crates/octo-vault/src/` currently contains only `lib.rs` and `migrations.rs`). Substrate-side impl location pending landing via mission 0206 v3.0 series.

```rust
// RFC-0959 v2.1 §2 wire form (no on-disk impl yet — pending 0206 series)
pub struct BurnEventRef {
    pub burn_id: [u8;16],
    pub chain_id: [u8;32],
    pub vault_id: [u8;32],
    pub amount_dqa_micros: i64,        // matches ValueTransfer::burn_pending amount_dqa_micros
                                       // (snapshot at finalize_burn time per research §7.2
                                       //  linearized state machine; finalize_burn itself
                                       //  takes only burn_id per RFC-0206 v3.0 §3)
    pub burn_policy_hash: [u8;32],     // RFC-defined; substrate column pending landing —
                                       // no inline policy column in transfer_events v014;
                                       // burn_pending policy binding lives in policy_registry
                                       // per RFC-0206 v3.0 §5
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

**Substrate status:** Migration pending landing via subsequent mission. No `v019__migrate_cost_to_dqa.sql` exists on disk; current `crates/quota-router-storage/migrations/` max is v016 (`v016__settlement_chain_vault.sql`); `crates/octo-vault/migrations/` max is v014 (`v014__create_transfer_events.sql`). Per research doc §B.3 the v019 slot is reserved for additive rollback of policy_registry/policy_kind_authority DDL (RFC-0008 Accept-revert race) — mission implementer MUST coordinate v019 allocation with that scheduler before filing.

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
    cost_micro_octo_w DQA(12) NOT NULL  -- native DQA type per RFC-0105 §SQL Integration
);
```

### 3.3 Migration path

- Migration: read BLOB(16), parse as u128 BE, encode as DQA(12) per RFC-0105 §DQA Serialization (scale=0), INSERT into new column. No `dqa_from_u128` helper exists; conversion uses the canonical RFC-0105 primitive directly (`Dqa::from_be_bytes_scale0` or equivalent per RFC-0105 §DQA Serialization).
- Verify: `SELECT cost_micro_octo_w FROM settlement_events` returns non-zero DQA(12) values for rows with non-zero cost. (`settlement_events` has no `amount` column — that column belongs to `transfer_events` v014; the verify query is intentionally column-restricted to `cost_micro_octo_w`.)
- Old BLOB column dropped post-migration verification

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

**R2 finding fix:** filter uses `'Burn'` (per RFC-0206 v3.0 + state machine linearization in research doc §7.2), NOT `'BurnFinalized'`. View references the SAME on-disk column shape as RFC-0960 v3.0 grand-design §2.5 (`event_type TEXT NOT NULL` — TEXT column with valid values enumerated as an SQL `--` comment, NOT a CHECK clause).

**On-disk event_type encoding choice:** Maintain RFC-0960 v3.0 grand-design §2.5 spec shape (`event_type TEXT NOT NULL`; valid values `'Mint' | 'TransferApplied' | 'TransferCorrected' | 'Burn'` documented in trailing SQL comment, NOT enforced via CHECK clause). NOTE: substrate migration `v014__create_transfer_events.sql` currently exposes `attributes BLOB NOT NULL` (line 20), NOT `event_type TEXT` — the TEXT discriminator is pending landing via v015+ migration in the 0206 series. Research doc §8.6's note about `attributes BLOB` reflects on-disk state; the §2.5 grand-design TEXT shape is the post-v015 target spec, not yet substrate.

## 5. Execution Class Mapping (RFC-0008 §RFC-0008 Execution Class Mapping)

| Surface                                  | Class | Justification            |
| ---------------------------------------- | ----- | ------------------------ |
| SettlementEnvelope::burn_event           | A     | Deterministic reference  |
| cost_micro_octo_w BLOB→DQA(12) migration | A     | Deterministic conversion |
| litellm_users_spend view                 | A     | Deterministic sum        |

## 6. Cross-References

- RFC-0959 v2.0 (current wire form)
- RFC-0960 v3.0 grand-design §2.5 (transfer_events.event_type TEXT column — pending landing via v015+ migration)
- RFC-0105 §DQA Serialization (DQA(12) primitive + SQL Integration)
- RFC-0126 Part 3 §Deterministic Canonical Serialization (DCS wire form)
- RFC-0206 v3.0 §3 ValueTransfer Trait (burn_event source: `burn_pending` sets amount, `finalize_burn` consumes burn_id per state machine)
- `docs/research/2026-08-21-vault-monetary-representation-redesign.md` §5.3 + §7.2 + §8.6 + §9 amendment table

## 7. Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| ------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2.1     | 2026-08-22 | Initial draft. Additive to v2.0. Adds burn_event wire form. Migrates cost_micro_octo_w BLOB→DQA(12). Resolves R2 finding on litellm_users_spend view filter (uses 'Burn' not 'BurnFinalized') + on-disk event_type column encoding (maintains TEXT per RFC-0960 v3.0 grand-design §2.5; pending landing via v015+ migration). R16 promotion: Draft → Accepted per long-horizon plan v1.6 Phase 4 Tier 1 promotion sequence.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| 2.1     | 2026-08-23 | **R3 fix-all:** Substrate grounding + cite cleanup. Phantom `crates/octo-vault/src/burn_event_ref.rs` path stripped (RFC-defined only). Phantom `ValueTransfer::finalize_burn transfer_event amount` comment corrected to `burn_pending` amount per RFC-0206 v3.0 §3 + research §7.2 state machine. Phantom `burn_policy_hash` field marked RFC-defined pending substrate column. Phantom `v019__migrate_cost_to_dqa.sql` flagged pending landing + v019 slot reservation conflict noted. Phantom `dqa_from_u128` removed; DQA(12) primitive cite fixed (RFC-0104 §DFP → RFC-0105 §DQA Serialization). Wire format cite fixed (was RFC-0126 listed as CBOR → RFC-0126 Part 3 §DCS). Phantom `amount` column in verification SQL removed (settlement_events has no `amount` column; cost_micro_octo_w is the correct column). VIEW `te.amount_dqa_micros` → `te.amount` (matches v014 substrate). Fabricated CHECK constraint on `event_type` removed (RFC-0960 v3.0 §2.5 has TEXT with values in trailing `--` comment, NOT CHECK clause). Wrong §2.5 cite fixed to specify RFC-0960 v3.0 grand-design (v3.1 amendment has no §2.5). YAML `builds_on` path corrected `rfcs/draft/process/` → `rfcs/accepted/process/`. SettlementEnvelope struct-side serde adapter clarification added. |
