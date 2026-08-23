---
rfc: 0960-v3.1
title: Vault Path Taxonomy (Corporate vs Mesh)
status: Accepted
version: 3.1
date: 2026-08-22
amends: RFC-0960
builds_on:
  - rfcs/accepted/economics/0960-grand-design-vaults-capabilities-reservations.md
  - rfcs/accepted/process/0010-v17-chain-id-registration-authority.md
  - docs/research/2026-08-21-vault-monetary-representation-redesign.md
---

# RFC-0960 v3.1 — Vault Path Taxonomy

## 0. Status

**Accepted (v3.1, 2026-08-22).** Additive to RFC-0960 (2026-08-17).

**Promotion trail:** v3.1 initial draft 2026-08-22 → Accepted 2026-08-22 per long-horizon plan Phase 4 Tier 1 promotion sequence (RFC-0960 v3.1 fourth in Tier 1 = final Tier 1, per research §20 decision #9). Mesh open path vs corporate closed path taxonomy on same substrate; `chain_metadata` table + `ledger_chain_registry` + `policy_registry` + `policy_kind_authority` substrate tables + `ValueTransfer` trait surface documented here are pending landing via Phase 1 mission 0206-001 v3.0 + 0206-009 per RFC-0206 v3.0 §3 + §4 explicit annotations — only v013 + v014 migrations (`vaults` + `transfer_events`) exist on disk at the time of this RFC's acceptance. Pre-existing cite pins stripped to bare RFC numbers per CLAUDE.md §RFC Reference Conventions.

## 1. Motivation

RFC-0960 defines the vault substrate with chain-aware PK `(chain_id, owner_did, asset_id)`. v3.1 adds explicit **path taxonomy**: corporate closed-set vs mesh open-set, sharing the same substrate but distinguishing by chain_id namespace.

## 2. Path Taxonomy

### 2.1 Mesh Open Path

- `chain_namespace = 0x01 (Mainnet)` per RFC-0010 §2 ledger_chain_registry Table comment (v1.7 amendment; bare parent canonical has no §2/§3 section — see note in §5 Cross-References)
- Membership: capability-gated (RFC-0957)
- Mint authority: governance-vote or single-key
- Interop default: no-bridge (`octo/interop/none/v1` per RFC-0967-A1 §2.6 kind UUID registry; no substrate-side `octo-interop-no-bridge` crate exists at the time of this RFC's acceptance — the kind UUID is a registry-only identifier)
- Burn default: `octo-burn-time-locked` 24h (FATF Travel Rule + MiCA Article 23 guidance)
- Audit: `octo-audit-mainnet-slim` for v1; A/B for v2

### 2.2 Corporate Closed Path

- `chain_namespace = 0x02 (User)` per RFC-0010 §2 ledger_chain_registry Table comment (v1.7 amendment; bare parent canonical has no §2/§3 section — see note in §5 Cross-References)
- Membership: corp-members-table or capability-gated
- Mint authority: corp-admin (single-key or multisig)
- Interop: opt-in (atomic-swap / wrapped-representation per policy)
- Burn: opt-in to any of `octo-burn-{time-locked,immediate,multisig}`
- Audit: `octo-audit-mainnet-slim` or `octo-audit-testnet-verbose` per corp choice

### 2.3 Same Substrate, Different Policy Bindings

Both paths share:

- `vaults` table (PK `(chain_id, owner_did, asset_id)` per RFC-0960)
- `transfer_events` table (event-sourced per RFC-0960 §2.5; on-disk PK `(chain_id, event_id)` per `crates/octo-vault/migrations/v014__create_transfer_events.sql` line 31; `attributes BLOB` carries the event-type discriminator — there is no `event_type TEXT` column in v014, the §2.5 grand-design prose pre-dates the v014 substrate shape)
- `ValueTransfer` trait (RFC-0206 §3 — substrate trait pending landing via Phase 1 mission 0206-001 v3.0 per RFC-0206 v3.0 §3 explicit annotation; `crates/octo-vault/src/value_transfer.rs` does not exist at the time of this RFC's acceptance)

The path differs only in policy bindings (`chain_metadata` columns — pending substrate landing via Phase 1 mission 0206-001 v3.0 per RFC-0206 v3.0 §4 explicit annotation; only v013 + v014 exist on disk).

## 3. Substrate Migration v015 — additive to v3.0 substrate

`chain_metadata` table augmented per RFC-0010 §3 Chain-id Derivation + §4 Authority Registration Flow + research doc §8.1 — substrate migration pending landing via Phase 1 mission 0206-001 v3.0 per RFC-0206 v3.0 §4 explicit annotation (v017 = chain_metadata + ledger_chain_registry + policy_registry + policy_kind_authority). No change to existing `vaults` or `transfer_events` tables (v013 + v014 already landed on disk).

## 4. Execution Class Mapping (RFC-0008 §RFC-0008 Execution Class Mapping)

| Surface                                                                        | Class                | Justification                                                                                                                                                                        |
| ------------------------------------------------------------------------------ | -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `chain_metadata.workflow_kind_hashes` resolution                               | A                    | Deterministic Vec traversal (column pending substrate landing via 0206-001 v3.0 per RFC-0206 v3.0 §4; no `workflow_kind_hashes` column on disk at the time of this RFC's acceptance) |
| Path taxonomy registration (no-bridge, Class A)                                | A                    | Deterministic `INSERT INTO chain_metadata` + Ed25519 sig verify (per RFC-0010 §4 flow) — substrate target table pending landing via 0206-001 v3.0 per RFC-0206 v3.0 §4               |
| Path taxonomy registration (atomic-swap / wrapped, Class A or B-with-ZK-proof) | A or B-with-ZK-proof | Per RFC-0967-A1 §2.1 (InteropPolicy trait declaration) + ZK capability proof per RFC-0958 + RFC-0965                                                                                 |
| Burn policy resolution                                                         | A                    | Per RFC-0967-A1 §2.1 (BurnPolicy trait declaration)                                                                                                                                  |

## 5. Cross-References

- RFC-0960 (current substrate)
- RFC-0010 §3 Chain-id Derivation (chain_namespace byte — in RFC-0010 v1.7 amendment, since bare parent canonical `0010-canonical-did-codec.md` has no §3; bare-RFC-0010 resolver per `scripts/validate_cites.sh:78-84` excludes amendment files)
- RFC-0010 §3 Chain-id Derivation + §4 Authority Registration Flow (chain_id registration authority — in RFC-0010 v1.7 amendment; same amendment-only caveat as above)
- RFC-0206 §3 ValueTransfer Trait (substrate surface — pending landing via 0206-001 v3.0 per RFC-0206 v3.0 §3 explicit annotation; trait module path `crates/octo-vault/src/value_transfer.rs` does not exist at the time of this RFC's acceptance)
- RFC-0967-A1 §0 Status (Policy Registry Trait Extension)
- `docs/research/2026-08-21-vault-monetary-representation-redesign.md` §3 D1 + §4 + §6 (version pin stripped per CLAUDE.md §RFC Reference Conventions)

## 6. Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| ------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 3.1     | 2026-08-22 | Initial draft. Additive to v3.0. Mesh open path vs corporate closed path taxonomy on same substrate. Same `vaults` + `transfer_events` tables; different `chain_metadata` policy bindings. Resolves R2 finding on path taxonomy ambiguity.                                                                                                                                                                                                                                                                                                                                                                      |
| 3.1     | 2026-08-22 | [R13 fix F-R12-XR-CROSS-RFC-INTEROP-DRIFT closure: aligned interop default with RFC-0967-A1 v1.5 §2.6 UUID registry.]                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| 3.1     | 2026-08-22 | [R15 fix F-R15-FD-1 cascade + F-R15-FD-5: RFC-0967-A1 v1.4 → v1.5.]                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| 3.1     | 2026-08-22 | [R15 fix F-R15-FD-5b: replaced phantom v3.2 amendment parenthetical with section anchor §2.1 Mesh Open Path per CLAUDE.md §No line refs anywhere.]                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| 3.1     | 2026-08-22 | **R16 promotion:** Draft → Accepted per long-horizon plan (v1.0 at `/home/mmacedoeu/.claude/plans/vault-monetary-research-consequence-2026-08-22.md`) Phase 4 Tier 1 promotion sequence (Tier 1 final). Status bumper + citation cleanup (3 pre-existing STALE v3.0 pins + 1 STALE v3.3 pin + 2 INVALID non-heading §InteropPolicy/§BurnPolicy anchors all stripped/fixed). Mesh open vs corporate closed path taxonomy + same-substrate binding preserved; `chain_metadata` table + `ValueTransfer` trait + `policy_registry` substrate tables all documented as pending landing via 0206-001 v3.0 + 0206-009. |
