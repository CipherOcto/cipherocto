---
rfc: 0206-v3.0
title: Value Transfer Surface (VaultStore KV + ValueTransfer money-movement)
status: Accepted
version: 3.0
date: 2026-08-22
amends: RFC-0206 (semver-major)
supersedes: v2.4 (which was renumbered from v2.5)
builds_on:
  - rfcs/accepted/storage/0206-octo-storage-split.md
  - rfcs/accepted/economics/0967-a1-policy-registry.md
  - docs/research/2026-08-21-vault-monetary-representation-redesign.md
---

# RFC-0206 v3.0 — Value Transfer Surface

## 0. Status

**Accepted (v3.0, 2026-08-22).** SEMVER-MAJOR version bump from v2.3.

**Why SEMVER-MAJOR (not additive minor):** VaultStore trait UNCHANGED (3 KV methods). NEW ValueTransfer trait (11 money-movement methods) in `octo-vault` Layer B owner crate. Per RFC-0206 additive-only rule, additive trait additions = minor. **However**, ValueTransfer is a NEW trait surface that does not extend VaultStore — it is a sibling trait. Adding a new sibling trait to a Layer B substrate crate constitutes a non-additive trait-shape change (consumers must now call two distinct trait surfaces where they previously called one or none).

Per BLUEPRINT.md §Dependency Validation Rules rule 5 (RFC-0205 + RFC-0206 2-cycle atomic promotion), v3.0 lands in the same Cycle as RFC-0205 with the same reviewer board.

**Promotion trail:** v3.0 initial draft 2026-08-22 → Accepted 2026-08-22 per long-horizon plan v1.6 Phase 4 Tier 3 promotion sequence (RFC-0206 v3.0 first in Tier 3 pair per research §20 decision #9, 2-Cycle Atomic with RFC-0205). VaultStore UNCHANGED + ValueTransfer NEW trait + v015–v018 migration split + execution class mapping + Layer B additive-only rule justification all preserved. Cite pins stripped to bare RFC numbers per CLAUDE.md §RFC Reference Conventions.

## 1. Motivation

The vault substrate (RFC-0960 §2.6 + RFC-0206 VaultStore trait) currently exposes only **3 KV-shape methods**: `get`, `put`, `delete`. Ruff migration of `quota-router-core`, `quota-router-storage`, `quota-router-sm-engine` to vault substrate (per `docs/audits/cost-storage-vault-abstraction-2026-08-21.md`) requires **money-movement primitives** (mint, burn_pending, finalize_burn, cancel_burn, immediate_burn, debit, credit, transfer, balance, history, create_vault) that VaultStore's KV surface cannot express.

Splitting VaultStore (KV) from ValueTransfer (money) preserves:
- RFC-0206 KV-shape discipline (no method-count drift on the substrate's primary KV surface)
- Layer B additive-only rule (no method-shape change to existing traits)
- Type-system no-enum principle (per `cipherocto-design-principles.md` §Extension over enumeration — primitive types in trait signatures, no Layer B context types)

## 2. VaultStore Trait — UNCHANGED

```rust
// VaultStore trait shape — illustrative; substrate-side impl location pending landing via Phase 1 mission 0206-001 v3.0 (R12 fix F-R12-XR-PHANTOM-0206-V30-MISSED: pre-revert reference site `crates/octo-vault/src/vault_store.rs` was REVERTED per R10.5 scope correction).
pub trait VaultStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, SubstrateError>;
    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), SubstrateError>;
    fn delete(&self, key: &[u8]) -> Result<(), SubstrateError>;
}
```

## 3. ValueTransfer Trait — NEW

```rust
// ValueTransfer trait shape — illustrative; substrate-side impl location pending landing via Phase 1 mission 0206-001 v3.0 (R12 fix F-R12-XR-PHANTOM-0206-V30-MISSED: pre-revert reference site `crates/octo-vault/src/value_transfer.rs` was REVERTED per R10.5 scope correction).
pub trait ValueTransfer {
    fn create_vault(
        &self,
        chain_id: &[u8;32],
        vault_id: &[u8;32],
        owner_did: &[u8;32],
        asset_id: &[u8;32],
        membership_proof: &[u8],   // raw bytes; kind_uuid at offset [0..16]
    ) -> Result<[u8;32], ValueTransferError>;

    fn mint(
        &self,
        chain_id: &[u8;32],
        vault_id: &[u8;32],
        amount_dqa_micros: i64,
        proof: &[u8],               // raw bytes; kind_uuid at offset [0..16]
    ) -> Result<(), ValueTransferError>;

    fn burn_pending(
        &self,
        chain_id: &[u8;32],
        vault_id: &[u8;32],
        amount_dqa_micros: i64,
        proof: &[u8],
        unlock_at_unix: i64,
    ) -> Result<[u8;16], ValueTransferError>;

    fn finalize_burn(
        &self,
        burn_id: &[u8;16],
    ) -> Result<(), ValueTransferError>;   // now_unix from ChainClock

    fn cancel_burn(
        &self,
        burn_id: &[u8;16],
        proof: &[u8],
    ) -> Result<(), ValueTransferError>;

    fn immediate_burn(
        &self,
        chain_id: &[u8;32],
        vault_id: &[u8;32],
        amount_dqa_micros: i64,
        proof: &[u8],
    ) -> Result<(), ValueTransferError>;

    fn debit(
        &self,
        chain_id: &[u8;32],
        vault_id: &[u8;32],
        amount_dqa_micros: i64,
        reason: &str,
        settlement_ref: &[u8;32],
    ) -> Result<(), ValueTransferError>;

    fn credit(
        &self,
        chain_id: &[u8;32],
        vault_id: &[u8;32],
        amount_dqa_micros: i64,
        reason: &str,
        settlement_ref: &[u8;32],
    ) -> Result<(), ValueTransferError>;

    fn transfer(
        &self,
        from_chain: &[u8;32],
        from_vault: &[u8;32],
        to_chain: &[u8;32],
        to_vault: &[u8;32],
        amount_dqa_micros: i64,
        reason: &str,
        settlement_ref: &[u8;32],
    ) -> Result<(), ValueTransferError>;

    fn balance(
        &self,
        chain_id: &[u8;32],
        vault_id: &[u8;32],
    ) -> Result<DqaMicros, ValueTransferError>;  // R12 fix F-R12-XR-PHANTOM-0206-V30-MISSED — corrected to DqaMicros (i64) per RFC-0206 v3.1 §2.2 reconciliation; pre-R10 narrative incorrectly cited `DQA<12>` per R7 fix F-R6-003

    fn history(
        &self,
        chain_id: &[u8;32],
        vault_id: &[u8;32],
        cursor: Option<&[u8;32]>,
        limit: usize,
    ) -> Result<(Vec<TransferEvent>, Option<[u8;32]> /* next_cursor */), ValueTransferError>;  // R7 fix F-R6-004 reconciliation
}
```

**Primitive types only** — no Layer B context types (AuthorityProof/BurnContext/etc.) in trait signatures. Per R2 finding X7: substrate trait signatures take primitives + raw byte proofs. The `proof: &[u8]` carries the kind_uuid at offset [0..16] (UUIDv5 derivation per RFC-0967-A1 §Kind UUID Registry).

## 4. Substrate Migration v015–v018 (R6 fix F-R6-001 — scope reconciliation with research doc §11 line 1260)

Per R5 fix F-R5-007 (pinned ordering) + R6 fix F-R6-001 (cross-doc consistency): substrate migrations split across 4 versions. **v015 = ValueTransfer trait surface only** (no schema); **v016 = burn_pending table**; **v017 = chain_metadata + ledger_chain_registry + policy_registry + policy_kind_authority**; **v018 = litellm_users + litellm_keys + scim_* + litellm_users_spend view**. v018 BLOCKS on v017 (litellm_users_spend view JOINs transfer_events; policy_registry must exist before policy_kind_authority validates any INSERT).

- `crates/octo-vault/migrations/v015__add_value_transfer.sql`: Adds `ValueTransfer` trait impl + AdapterAllowlist registration of `transfer_events` write-target. No new tables. [R12 fix F-R12-XR-PHANTOM-0206-V30-MISSED: migration file path is illustrative; substrate-side migration pending landing via Phase 1 mission 0206-001 v3.0 — pre-revert reference path REVERTED per R10.5 scope correction.]
- `crates/octo-vault/migrations/v016__add_burn_pending.sql`: Adds `burn_pending` table per research doc §7.3. [R12 fix: pending landing via 0206-001 v3.0.]
- `crates/octo-vault/migrations/v017__add_chain_metadata_and_policy_registry.sql`: Adds `chain_metadata` columns per research doc §8.1 + `ledger_chain_registry` + `policy_registry` + `policy_kind_authority` per RFC-0967-A1 §2.4 + §2.5. [R12 fix: pending landing via 0206-001 v3.0.]
- `crates/octo-vault/migrations/v018__add_litellm_persistence.sql`: Adds `litellm_users` + `litellm_keys` + `scim_users` + `scim_groups` + `scim_group_members` + `litellm_users_spend` view per research doc §5.3. [R12 fix: pending landing via 0206-001 v3.0.]

Does NOT alter `vaults` or `transfer_events` tables (substrate migration v013/014 already on disk).

## 5. Execution Class Mapping (RFC-0008 §RFC-0008 Execution Class Mapping)

| Method | Class | Justification |
|---|---|---|
| `create_vault` | A or B-with-ZK-proof | Consensus-path vault creation; MembershipPolicy gating per RFC-0967-A1 |
| `mint` | A or B-with-ZK-proof | Consensus-path mint gate; AuthorityPolicy gating |
| `burn_pending` | A or B-with-ZK-proof | Consensus-path burn commitment; BurnPolicy gating; **R7 fix F-XCLASS-001** substrate-level ZK guard applies for Class B AuthorityPolicy bound to burn queue; BurnPolicy::validate_unlock_window is policy-internal timestamp check (Class A) — substrate guard is on policy binding, NOT on validate_unlock_window method |
| `finalize_burn` | A | Deterministic; ChainClock-derived `now_unix` |
| `cancel_burn` | A or B-with-ZK-proof | Capability-gated reversal |
| `immediate_burn` | A or B-with-ZK-proof | BurnPolicy::allowed_chain_namespaces gate (no Mainnet) |
| `debit` | A | Deterministic balance decrement |
| `credit` | A | Deterministic balance increment |
| `transfer` | A | Atomic cross-vault decrement+increment; **R7 fix F-XCLASS-004** if `from_chain != to_chain`, substrate MUST invoke `InteropPolicy::validate_transfer` (Class A/B-with-ZK-proof per RFC-0967-A1 §3) BEFORE applying the transfer; same-chain transfer skips InteropPolicy |
| `balance` | A | Read-only, deterministic |
| `history` | A | Read-only, deterministic; **R7 fix F-XCLASS-005** raw transfer_event log; not policy-gated (no ZK) — distinguishes from `WorkflowKind::read_user_info` which IS policy-gated |

**R7 fix F-XCLASS-002 — AuditPolicy substrate call sites:** AuditPolicy::emit_fields and variant_assignment are Class A (deterministic), invoked at substrate on every `append_transfer_event` call. Substrate loads chain_metadata.audit_policy_hash → `audit_registry.by_hash` → `policy.emit_fields()` selects which fields to populate; `policy.variant_assignment(chain_id)` selects A/B audit variant. Call sites: `mint`, `burn_pending`, `finalize_burn`, `debit`, `credit`, `transfer`. AuditPolicy is NOT dead code — wire-up documented in §5 above (cross-reference research doc §8.5 for DDL columns).

**R9 fix F-R9-PHANTOM-TV-AUDIT-REF — phantom `TV-AUDIT-EMIT-INVARIANT` reference removed:** Earlier §5 narrative referenced "research doc §11 Phase 2 acceptance gate TV-AUDIT-EMIT-INVARIANT"; research §11 does NOT define that TV. Phantom cross-reference. The §11 acceptance gate that audits AuditPolicy emit-field coverage is `TV-AUDIT-EMIT-COVERAGE` (Phase 2 substrate acceptance gate) — corrected.

**R7 fix F-XCLASS-003 — WorkflowKind::provision_subject composite dispatch:** substrate invokes provision_subject via CompositeWorkflow dispatch (post-consensus side effect). Specifically: `create_vault` path → after `MembershipPolicy::validate` returns Ok AND vault row INSERTed → substrate iterates `chain_metadata.workflow_kind_hashes` and invokes `provision_subject` on each. For CompositeWorkflow with AlwaysBoth: all workflows invoked in same tx. Direct-call invocation (not via create_vault) is NOT a substrate path — application code may call WorkflowKind::provision_subject directly but that is OUTSIDE the consensus path.

**R7 fix F-XCLASS-006 — v017 Class C rejection enforcement:** migration v017 includes `CHECK (execution_class IN ('A', 'B-with-ZK-proof'))` constraint on policy_registry; substrate rejects INSERT/UPDATE setting execution_class='C' at DDL level. Defense-in-depth: even if application code mis-classifies, migration-time DDL enforces.

## 6. Layer B Additive-Only Rule Justification

Per RFC-0206 additive-only rule: "Layer B = years-stable, RFC-driven, additive only. New RFC adds feature." The semantic difference between minor (additive) and major (non-additive) for trait-shape:

- **Additive (minor):** New methods on existing trait, default-implemented for backward compat
- **Non-additive (major):** New trait surface (consumers must call two distinct surfaces) OR method signature change OR removal

ValueTransfer is a NEW trait surface — not additive. SEMVER-MAJOR required.

## 7. Cross-References

- RFC-0206 (current substrate) — `rfcs/accepted/storage/0206-octo-storage-split.md`
- RFC-0205 (2-cycle sibling — atomic promotion)
- RFC-0960 §Substrate (consumption context)
- RFC-0967-A1 §0 (Policy Registry Trait Extension — policy gating)
- RFC-0008 §RFC-0008 Execution Class Mapping
- `docs/research/2026-08-21-vault-monetary-representation-redesign.md` v2.0 §7.4 + §8.4

## 8. Version History

| Version | Date | Change |
|---|---|---|
| 3.0 | 2026-08-22 | Initial draft. VaultStore UNCHANGED (3 KV methods). NEW ValueTransfer trait (11 money-movement methods) in `octo-vault` Layer B owner crate. Primitive types only in trait signatures. Substrate migration v015 adds policy_registry + policy_kind_authority + chain_metadata columns + burn_pending. Renumbered from v2.4 (which was renumbered from v2.5) per R2 finding: non-additive trait-shape change = SEMVER-MAJOR. |
| 3.0 | 2026-08-22 | **R16 promotion:** Draft → Accepted per long-horizon plan v1.6 Phase 4 Tier 3 promotion sequence (RFC-0206 v3.0 first in Tier 3 pair, 2-Cycle Atomic with RFC-0205). Status bumper + citation cleanup (4 STALE RFC-0206 v2.x pins + 1 STALE RFC-0960 pin + 2 INVALID non-heading anchors all stripped/fixed per CLAUDE.md §RFC Reference Conventions). VaultStore UNCHANGED + ValueTransfer NEW + v015–v018 migration split + execution class mapping preserved. |
| 2.4 | — | (Never landed) Renumbered to v3.0 due to SEMVER-MAJOR classification. |
| 2.3 | 2026-08-19 | Accepted |
