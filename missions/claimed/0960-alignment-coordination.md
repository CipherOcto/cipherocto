---
name: 0960-alignment-coordination
description: Coordination summary for RFC-0960 v3.5 mission alignment per audit 2026-08-24. Documents 2 inline retrofix categories surfaced by RFC-0960 v3.5 spec audit + scope of 1 new sibling mission for v3.5 substrate landing (0960-v3.5-landing). NO scope of its own — pure cross-RFC alignment documentation; existing 0960-* missions preserved untouched per historical-mission-preservation discipline except for inline retrofixes documented below per R19 scope discipline.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-24T00:00:00.000Z
  v: "1.0"
  depends_on:
    - 0960-a-grand-design-reference
    - 0960-vault-substrate-amendment
    - 0960-v3.5-landing
    - vault-chain-metadata
    - RFC-0960
status: OPEN
---

# Mission `0960-alignment-coordination` v1.0 — OPEN 2026-08-24

## Context

RFC-0960 v3.5 (canonical Accepted 2026-08-23) adds Vault Path Taxonomy amendment on top of RFC-0960 v3.0 substrate (landed via `0960-vault-substrate-amendment` 2026-08-18). Mission audit 2026-08-24 surfaced 2 retrofix categories for existing 0960-* missions + 1 new sibling mission to land v3.5 PENDING substrate (chain_metadata + ledger_chain_registry + policy_registry + policy_kind_authority tables + ValueTransfer trait + 5 policy-kind UUIDv5 fixtures).

This mission captures the audit findings + references the 1 new sibling mission that owns the v3.5 substrate landing work. **This mission is documentation-only** — it does not edit any existing 0960-* mission file beyond inline retrofixes documented below per historical-mission-preservation discipline (existing OPEN/LANDED mission state represents committed work at its filing time and is preserved where possible; only stale placeholders and clear contradictions receive inline retrofixes per R19 scope discipline).

## Inline retrofixes applied (2026-08-24 audit)

### Retrofix 1: `0960-a-grand-design-reference` v3.5 supersession + 25-variant drift

**Defect:** Mission Status block + §3 Constraint Set table row claim RFC-0960 v2.0 (umbrella stale) + "23 variants" (count drift; RFC-0960 grew during v3.1-v3.5 amendments).

**Evidence:** `grep -E "^(version|status):" rfcs/accepted/economics/0960-grand-design-vaults-capabilities-reservations.md` shows `version: 3.5`. Per audit, ground state of `docs/architecture/grand-design.md` §3 = "25 variants" (matches umbrella RFC post-v3.5).

**Fix:** Inline retro-supersession note added to Status block quote + §3 Constraint Set row in Type Coverage table bumped "23 variants" → "25 variants — post v3.5". AC text preserved per R19 (retro-supersession note in §Acceptance Criteria adds drift context; AC checkboxes left `[ ]` for future close-out pass).

### Retrofix 2: `0960-vault-substrate-amendment` v3.5 supersession note

**Defect:** Mission Status block + §Version History table reference RFC-0960 v3.0 LANDED state only; umbrella now at v3.5 with new PENDING substrate (chain_metadata + ledger_chain_registry + policy_registry + policy_kind_authority tables + ValueTransfer trait).

**Evidence:** `ls rfcs/accepted/economics/0960-v35-vault-path-taxonomy.md` exists; v3.5 Status header documents all PENDING substrate per research §16 mission `vault-chain-metadata` Phase 1.

**Fix:** Inline retro-supersession note added to Status block quote + new VH row added (2026-08-23 v3.5 supersession) preserving v3.0 LANDED state per historical-mission-preservation + RFC process amendment-stacking discipline. Mission AC text unchanged per R19 scope discipline; v3.5 substrate landing owned by separate mission `0960-v3.5-landing`.

## Gaps surfaced by RFC-0960 v3.5 audit

### Gap 1: 4 substrate tables PENDING landing

RFC-0960 v3.5 Status header documents 4 substrate tables PENDING landing per research §16 mission `vault-chain-metadata` Phase 1:

- `chain_metadata` (chain_namespace byte discrimination between Mesh Open Path vs Corporate Closed Path)
- `ledger_chain_registry` (RFC-0010 §2 substrate; canonical chain_id ↔ namespace registration)
- `policy_registry` (RFC-0967-A1 §2.5 substrate; policy_kind UUIDv5 namespace seeding)
- `policy_kind_authority` (RFC-0967-A1 §2.5 substrate; delegate authority chains)

**Coverage gap:** Vault substrate (v013 + v014) is LANDED but Mesh Open vs Corporate Closed path distinction (v3.5 §2.1 + §2.2) cannot be enforced without `chain_namespace` byte discrimination.

**Owned by mission:** `0960-v3.5-landing` (sibling; v015 + v016 migrations + DDL).

### Gap 2: `ValueTransfer` trait surface PENDING landing

RFC-0960 v3.5 §0 Status documents `ValueTransfer` trait surface PENDING landing (Layer B primitive; additive to RFC-0206 §4 additive-only migration rule; re-exports existing `transfer_events` v014 substrate through typed trait).

**Coverage gap:** Cross-chain settlement reject (RFC-0959 v2.0 + `0959-c1-wire-A-substrate-verify`) cannot be enforced without `ValueTransfer::apply` consuming `chain_id` byte via FK check on `chain_metadata`.

**Owned by mission:** `0960-v3.5-landing` (sibling; new file `crates/octo-vault/src/transfer.rs`).

### Gap 3: 5 policy-kind UUIDv5 fixtures PENDING landing

RFC-0960 v3.5 §2.1 + §2.2 + RFC-0967-A1 §2.6 document 5 policy Kinds requiring UUIDv5 namespace seeding per `octo/policy/kind/v1/` domain separator:

- `octo/membership/capabilitygated/v1`
- `octo/interop/none/v1`
- `octo/burn/timelock/v1`
- `octo/audit/mainnet/v1`
- `octo/audit/ab/v1`

**Coverage gap:** No byte-exact fixture file exists; drift between RFC text and substrate impossible to detect without anti-drift helper.

**Owned by mission:** `0960-v3.5-landing` (sibling; `crates/octo-vault/tests/policy_kind_uuid_fixtures.rs` — 7 byte-exact fixtures + anti-drift helper).

## Sibling mission cross-references

- `0960-v3.5-landing` — primary substrate ownership for v3.5 PENDING landing (4 tables + ValueTransfer trait + 5 policy-kind UUIDv5 fixtures)

## Acceptance Criterion

- 2 inline retrofixes applied to `0960-a` + `0960-v` per audit findings
- 1 sibling mission filed (`0960-v3.5-landing`) + cross-references 2 retrofix missions via `depends_on` chain
- AC gate: `ls missions/claimed/0960-*.md` → 4 files (2 existing + 1 landing + 1 coordination)
- AC gate: `rg 'v3.5 supersession' missions/claimed/0960-a-grand-design-reference.md` → 1 hit (retrofix 1)
- AC gate: `rg 'v3.5 supersession' missions/claimed/0960-vault-substrate-amendment.md` → 1 hit (retrofix 2)
- AC gate: `rg 'pub trait ValueTransfer' missions/claimed/0960-v3.5-landing.md` → 1 hit (sibling mission trait spec)
- Cross-RFC cite validation: Guard 2 PASS for all 3 new + retrofixed mission files
- Prettier clean
- No new INVALID cites introduced

## Files / Artifacts

- Edit: `missions/claimed/0960-a-grand-design-reference.md` (v3.5 retro-supersession + 23→25 variants in §3 table + §Acceptance Criteria retro-supersession note)
- Edit: `missions/claimed/0960-vault-substrate-amendment.md` (v3.5 retro-supersession + VH row 2026-08-23 supersession)
- New: `missions/claimed/0960-v3.5-landing.md` (substrate landing ownership for 4 tables + ValueTransfer trait + 5 policy-kind UUIDv5 fixtures)
- New: `missions/claimed/0960-alignment-coordination.md` (this file)

## Cross-references

- RFC-0960 v3.5 (canonical Accepted 2026-08-23 — Vault Path Taxonomy amendment)
- RFC-0010 v1.7 (chain_namespace 0x01=Rfc/0x02=User per R5 fix-all; ledger_chain_registry DDL)
- RFC-0967-A1 v1.9.2 §2.6 (kind UUID registry — 5 policy kinds to seed)
- RFC-0206 §4 (Layer B additive-only migration rule for v015/v016 ownership)
- RFC-0105 v3.4 (asset_id derivation cross-ref)
- RFC-0959 v2.0 (ValueTransfer::apply chain_id consistency — cross-chain settlement reject)
- Mission `0960-vault-substrate-amendment` (LANDED 2026-08-18 — v013/v014 substrate)
- Mission `0960-a-grand-design-reference` (docs cross-link target — grand-design.md §Vault Path Taxonomy)
- Mission `vault-chain-metadata` (research §16 Phase 1 — substrate landing coordination)
- Mission `0959-c1-wire-A-substrate-verify` (cross-chain settlement reject substrate codependency)

## Out of scope

- Retroactive supersession of older 0960-* missions beyond the 2 inline retrofixes (per R19 scope discipline)
- `policy_kind_authority` LIVE delegate chain seeding (PENDING per RFC-0967-A1 §2.6; separate future mission)
- 108 byte-exact TV from `0960-vault-substrate-amendment` (already LANDED; out of scope)
- Updates to `docs/architecture/grand-design.md` beyond §Vault Path Taxonomy subsection (out of scope; owned by `0960-a-grand-design-reference` follow-on close-out pass)
- Cross-RFC harmonization edits (research doc + companion RFC cross-refs) per `vault-monetary-research-consequence` Phase 5 (separate phase)
- 0206-001 textual drift correction (per RFC-0960 v3.5 §0 Status: `0206-001` is Layer A storage-core, not vault DDL; corrected in v3.5 amendment file)

## Dependencies

- All 2 existing 0960-* missions (parent coverage)
- RFC-0960 v3.5 (canonical Accepted state)
- Mission `vault-chain-metadata` (research §16 Phase 1 substrate work — co-owned)

## Version History

| Version | Date       | Change                                                                                                                                                                                                    |
| ------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v1.0    | 2026-08-24 | Initial filing per RFC-0960 v3.5 mission audit 2026-08-24. 2 inline retrofix categories + 1 sibling mission for v3.5 PENDING substrate landing. Pure coordination; no new substrate code in this mission. |
