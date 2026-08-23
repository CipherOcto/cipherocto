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
  - rfcs/draft/process/0206-v30-value-transfer-surface.md
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

```rust
// crates/octo-vault/src/burn_event_ref.rs
pub struct BurnEventRef {
    pub burn_id: [u8;16],
    pub chain_id: [u8;32],
    pub vault_id: [u8;32],
    pub amount_dqa_micros: i64,        // matches ValueTransfer::finalize_burn transfer_event amount
    pub burn_policy_hash: [u8;32],     // snapshot at finalize_burn time
    pub finalized_at_unix: i64,        // matches transfer_event timestamp
}
```

Wire form (CBOR canonical per RFC-0126):

```
{ "burn_id": [u8;16], "chain_id": [u8;32], "vault_id": [u8;32],
  "amount_dqa_micros": i64, "burn_policy_hash": [u8;32], "finalized_at_unix": i64 }
```

**Timing:** burn_event populated at `ValueTransfer::finalize_burn` time (NOT at `burn_pending` time). Balance snapshot reflects post-decrement state. The burn_event references the same `transfer_events` row that the burn inserted.

## 3. DQA(12) Cost Migration

### 3.1 Before

```sql
-- v004__create_settlement_events.sql
CREATE TABLE settlement_events (
    ...
    cost_micro_octo_w BLOB NOT NULL  -- 16-byte BE u128
);
```

### 3.2 After

```sql
-- v019__migrate_cost_to_dqa.sql
CREATE TABLE settlement_events (
    ...
    cost_micro_octo_w DQA(12) NOT NULL  -- native DFP type, no bridge
);
```

### 3.3 Migration path

- v019 migration: read BLOB(16), parse as u128 BE, convert to DQA(12) via `dqa_from_u128(amount, scale=0)`, INSERT into new column
- Verify: `SELECT cost_micro_octo_w FROM settlement_events WHERE amount != 0` returns non-zero DQA(12) values
- Old BLOB column dropped post-migration verification

## 4. litellm_users.spend — Derived VIEW per R2 Finding

The litellm_users_spend view (research doc §5.3) derives spend via JOIN over vault events:

```sql
CREATE VIEW litellm_users_spend AS
SELECT
    lu.user_id,
    COALESCE(SUM(te.amount_dqa_micros), 0) AS spend_dqa_micros
FROM litellm_users lu
LEFT JOIN vaults v ON v.owner_did = lu.user_id
LEFT JOIN transfer_events te ON te.from_vault_id = v.vault_id
                              AND te.event_type IN ('TransferApplied', 'Burn')
GROUP BY lu.user_id;
```

**R2 finding fix:** filter uses `'Burn'` (per RFC-0206 v3.0 + state machine linearization in research doc §7.2), NOT `'BurnFinalized'`. View references the SAME on-disk column shape as RFC-0960 §2.5 (`event_type TEXT NOT NULL CHECK (...)`).

**On-disk event_type encoding choice:** Maintain RFC-0960 §2.5 spec shape (`event_type TEXT NOT NULL CHECK (event_type IN ('Mint','TransferApplied','TransferCorrected','Burn'))`). Research doc §8.6's note about `attributes BLOB` is a research observation, not adopted. Substrate migration v014 already uses TEXT column per spec.

## 5. Execution Class Mapping (RFC-0008 §RFC-0008 Execution Class Mapping)

| Surface                                  | Class | Justification            |
| ---------------------------------------- | ----- | ------------------------ |
| SettlementEnvelope::burn_event           | A     | Deterministic reference  |
| cost_micro_octo_w BLOB→DQA(12) migration | A     | Deterministic conversion |
| litellm_users_spend view                 | A     | Deterministic sum        |

## 6. Cross-References

- RFC-0959 v2.0 (current wire form)
- RFC-0960 §2.5 (transfer_events.event_type TEXT column)
- RFC-0104 §DFP (DQA(12) primitive)
- RFC-0126 §Canonical Serialization (CBOR)
- RFC-0206 v3.0 §3 ValueTransfer Trait (burn_event source)
- `docs/research/2026-08-21-vault-monetary-representation-redesign.md` v2.0 §5.3 + §8.6 + §9 amendment table

## 7. Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                 |
| ------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2.1     | 2026-08-22 | Initial draft. Additive to v2.0. Adds burn_event wire form. Migrates cost_micro_octo_w BLOB→DQA(12). Resolves R2 finding on litellm_users_spend view filter (uses 'Burn' not 'BurnFinalized') + on-disk event_type column encoding (maintains TEXT per RFC-0960 §2.5). |
| 2.1     | 2026-08-22 | **R16 promotion:** Draft → Accepted per long-horizon plan v1.6 Phase 4 Tier 1 promotion sequence. Status bumper. BurnEventRef wire form + DQA(12) migration + litellm_users_spend view preserved.                                                                      |
