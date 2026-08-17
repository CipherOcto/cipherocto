---
name: audit-2026-08-17-storage-restructure-parallel-model-risks
description: 2026-08-17 audit verdict on storage restructure plan + review — 7 parallel-model risks surfaced; 3 new unification missions filed (c9 + x-mission + S6e). Spend_ledger + vault balance are structural dual substrates (per RFC-0862 §Future Work F12). Push user-only per [[feedback_initiative_user_only]] + [[git-workflow]].
metadata:
  type: project
---

# 2026-08-17 storage restructure audit — parallel-model risks

Hard ground-check of
`docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md` +
`docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
under lens: **"spending and cost based on DQA type unified into
the capabilities-based vault model"**.

## Verdict

**PARTIAL UNIFICATION.** Vault substrate (octo-vault v013/v014)

- verify-time invariant (VaultLookup trait per §20.6.1 option b
  LANDED via S5) + WrappedOnly intra-chain rule LANDED. **Substrate
  ahead of RFC text + field-type coherence.** 7 distinct
  parallel-model risks surfaced.

## Risk inventory

| #   | Risk                                                                     | Severity   | Closure                                                              |
| --- | ------------------------------------------------------------------------ | ---------- | -------------------------------------------------------------------- |
| 1   | `MicroOctoW` type alias split (3 sites, 2 underlying types)              | CRITICAL   | `0862-c9` (FILED 2026-08-17)                                         |
| 2   | 4 column types for "amount" (INTEGER / DQA(12) / BLOB u128 / BIGINT)     | CRITICAL   | RFC-0900 (S6d) + RFC-0959 (S6e) + v008 spend_ledger (separate)       |
| 3   | Spend_ledger NOT vault-bound by design (RFC-0862 §Future Work F12)       | STRUCTURAL | Not unified by design; document in plan §20.3                        |
| 4   | Field type drift in marketplace/task_market/slash_store/CLI (still u128) | HIGH       | `0105-x` (FILED 2026-08-17)                                          |
| 5   | Vault row lookup at 2 verify paths, only 1 LANDED                        | HIGH       | `0959-c1` S6e (FILED 2026-08-17) — VaultLookup trait reuse AC        |
| 6   | ChainId 4 representations (32B / 17B / String / u32)                     | MED        | 17B+32B coexistence per R15-F9; `octo-reputation::u32` audit pending |
| 7   | `octo-reputation::Auth.chain_id: u32` separate semantic                  | LOW        | Audit pending; may be internal numeric ref                           |

## What's UNIFIED

- **Vault substrate**: `crates/octo-vault/migrations/v013__create_vaults.sql` PK = `(chain_id, owner_did, asset_id)` with `vaults_vault_id_idx` UNIQUE INDEX per §20.3 Model B. **LANDED S3.**
- **Transfer events**: `v014__create_transfer_events.sql` PK = `(chain_id, event_id)` + `amount DQA(12)` per §20.3. **LANDED S3.**
- **vault_id derivation**: `octo-vault::vault_id_unchecked` uses `b"cipherocto/vault/v1/" + chain_id + owner_did + asset_id` BLAKE3 prefix per §20.3. **LANDED S3.**
- **Verify-time invariant**: `octo-cap-macaroon::vault_lookup::VaultLookup` trait + `Macaroon::verify_for_vault_op(vault_lookup: &dyn VaultLookup)` per §20.6.1 option (b). **LANDED S5 + S6b.**
- **WrappedOnly intra-chain**: `Macaroon::attenuate` + `verify_for_vault_op` chain guard + max depth 16 + cycle detection + chainless-parent reject per §20.7. **LANDED S5 + S6b.**
- **NodeEnvelope version_tag**: `version_tag: u8 = 0xA1` V2 wire-form discriminator per §14.1 + S6a. **LANDED S6a.**

## What's DIVERGED

- **Spend_ledger substrate (INTEGER + scale=0)**: NOT vault-bound. PK = `(holder_did, macaroon_id)`. Capability redemption drains spend_ledger, NOT vault balance. **Per RFC-0862 §Future Work F12 by design.** Vault verify + spend_ledger deduct are parallel paths. Not convergent without redesigning spend_ledger substrate.
- **`MicroOctoW` type alias split**: caveat::mod + caveat::payment = `u128`; stoolap_spend_ledger = `Dqa`. Same name, 2 types. **c9 closes.**
- **Field type drift**: 7 files still use `u128` for amount-bearing fields. **x-mission closes.**
- **Settlement wire form**: `cost_micro_octo_w BLOB` (16-byte BE u128) in v004. NOT `DQA(12)`. **S6e RFC-0959 closes.**
- **Slash ledger schema**: `BIGINT` micro_octo_w + no chain_id column. **S6d RFC-0900 closes.**

## Hard recommendations (acted on 2026-08-17)

1. **File `0862-c9-micro-octow-type-unification`** — canonical alias in `determin/src/lib.rs`. **DONE 2026-08-17.**
2. **File `0105-x-s4-deferred-codemod-sites`** — extend S4 codemod to 7 deferred files. **DONE 2026-08-17.**
3. **Verify S6e RFC-0959 reuses `VaultLookup` trait** — `0959-c1-wire-format-amendment` filed with hard AC-3 + AC-6. **DONE 2026-08-17.**
4. **`octo-reputation::u32 chain_id` semantic audit** — pending. May be internal numeric ID; document or canonical-map.
5. **Doc-bug owed**: §8.3.3 row for v007 specifies DQA(12) but LANDED substrate uses INTEGER. Update review §8.3.3 + §9.1 to acknowledge INTEGER+scale=0 substrate decision OR file migration v008. **Local scratchpad; cannot commit per [[docs-reviews-temporary]] + [[docs-plans-scratchpad]].**

## Will parallel models persist?

YES, by design (3):

- spend_ledger = prepaid budget substrate, separate from vault balance (per RFC-0862 §Future Work F12). Two parallel balance substrates will remain after all B0 RFCs land.
- 17-byte ChainNamespace + 32-byte chain_id coexistence (per R15-F9 additive).

YES, by omission (5) — closure missions filed 2026-08-17:

- c9 (MicroOctoW alias)
- x-mission (u128 field type drift)
- S6e (settlement wire DqaEncoding + VaultLookup reuse)
- S6d (slash ledger schema)
- `octo-reputation::u32` audit (pending)

## Source

- Plan: `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 S6 row 6 (Stream A.1) + §5 risk register + §8 termination
- Review:
  `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
  §8.1.2 + §8.3.3 + §8.4.1 + §9.1 + §9.2 + §9.3 + §18 + §20.3 +
  §20.5.1 + §20.6 + §20.6.1 + §20.7 + §22 + §24
- Memory card: `S4-codemod-2026-08-17-LANDED.md` (existing S4
  codemod receipt)

## Push authorization

3 mission files + MEMORY.md update queued on `next`. Push
user-only per [[feedback_initiative_user_only]] + [[git-workflow]].
