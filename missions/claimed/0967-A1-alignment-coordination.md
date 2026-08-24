---
name: 0967-A1-alignment-coordination
description: Coordination summary for RFC-0967-A1 v1.9.2 mission alignment per audit 2026-08-24. Documents 1 inline retrofix category surfaced by RFC-0967-A1 v1.9.2 spec audit (crate rename + umbrella supersession) + scope of 1 new sibling mission for v1.9.2 substrate landing (`0967-A1-v1.9.2-landing`). NO scope of its own — pure cross-RFC alignment documentation; existing 0967-* missions preserved untouched per historical-mission-preservation discipline except for inline retrofix documented below per R19 scope discipline.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-24T00:00:00.000Z
  v: "1.0"
  depends_on:
    - 0967-a-policy-object-graph
    - 0967-A1-v1.9.2-landing
    - 0960-v3.5-landing
    - 0206-001-substrate-newtype
    - 0206-009-adapter-crate-creation
    - RFC-0967-A1
    - RFC-0967
status: OPEN
---

# Mission `0967-A1-alignment-coordination` v1.0 — OPEN 2026-08-24

## Context

RFC-0967-A1 v1.9.2 (canonical Accepted 2026-08-24 per `rfcs/accepted/economics/0967-a1-policy-registry.md`) extends RFC-0967 v1.1-Resolved parent with 6 policy-kind traits + 30 per-policy-kind UUIDv5 fixtures + 8 canonical `octo/` prefix domain constants + `PolicyRegistry` trait extension. Mission audit 2026-08-24 surfaced 1 retrofix category for existing `0967-a-policy-object-graph` mission (crate rename + RFC-0967-A1 v1.9.2 supersession) + 1 new sibling mission to land v1.9.2 PENDING substrate.

This mission captures the audit findings + references the 1 new sibling mission that owns the v1.9.2 substrate landing work. **This mission is documentation-only** — it does not edit any existing 0967-* mission file beyond inline retrofix documented below per historical-mission-preservation discipline (existing OPEN/LANDED mission state represents committed work at its filing time and is preserved where possible; only stale placeholders and clear contradictions receive inline retrofix per R19 scope discipline).

## Inline retrofix applied (2026-08-24 audit)

### Retrofix: `0967-a-policy-object-graph` crate rename + RFC-0967-A1 v1.9.2 supersession

**Defect:** Two drift categories in single mission:

1. **Crate rename drift:** Mission title + 7 inline `cipherocto-policy` references are stale. Actual crate = `octo-policy` (per `crates/octo-policy/src/lib.rs`). Rename part of Wave 5/6 substrate index per MEMORY.md 2026-08-11.
2. **RFC-0967-A1 v1.9.2 supersession:** Mission cites umbrella RFC-0967 v1.1-Resolved only. Umbrella extended by amendment file `rfcs/accepted/economics/0967-a1-policy-registry.md` (v1.9.2 canonical Accepted 2026-08-24) + `rfcs/accepted/economics/0967-a1-a1-workflowkind-trait-sig-amendment.md` (v1.2 effective 2026-08-22).

**Evidence:**

1. `ls crates/octo-policy/` exists; `ls crates/cipherocto-policy/` does not. MEMORY.md 2026-08-11 entry documents Wave 5/6 substrate index.
2. `rg -A1 "version:" rfcs/accepted/economics/0967-a1-policy-registry.md` shows `version: 1.9.2` (canonical YAML frontmatter); VH row 1.9.2 dated 2026-08-24.

**Fix:** Inline retro-supersession note added to Status block quote (combined drift into single quote for readability) + new VH row 2026-08-24 added to §Version History documenting both drift categories + new `0967-A1-v1.9.2-landing` mission cross-link. Mission AC text + Cargo command refs preserved per historical-mission-preservation + R19 scope discipline. AC text references `cargo test -p cipherocto-policy --lib` retained as historical record (cargo command should be `cargo test -p octo-policy --lib` per actual crate name).

## Gaps surfaced by RFC-0967-A1 v1.9.2 audit

### Gap 1: 6 policy-kind traits PENDING landing

RFC-0967-A1 v1.9.2 VH row v1.7 row documents 6-trait surface (AuthorityPolicy, MembershipPolicy, InteropPolicy, BurnPolicy, WorkflowKind, AuditPolicy). Per v1.9.2 VH row 1.5: all 6 traits are "RFC-defined extension pending substrate landing via 0206-001 v3.0 + 0206-009".

**Coverage gap:** `rg 'AuthorityPolicy|MembershipPolicy|InteropPolicy|BurnPolicy|WorkflowKind|AuditPolicy' crates/octo-policy/src/ crates/octo-policy-storage/src/` returns 0 hits. Mission `0967-a-policy-object-graph` (LANDED 2026-08-07) implements PolicyObject + intersect + is_subgraph only — 6 new traits not yet landed.

**Owned by mission:** `0967-A1-v1.9.2-landing` (sibling; 6 traits + 3 Selector types in `policy_kinds.rs`).

### Gap 2: `kind_uuid_registry` (30 UUIDv5 fixtures) PENDING landing

RFC-0967-A1 v1.9.2 §2.6 table documents 30 per-policy-kind UUIDv5 fixtures (6 Auth + 7 Membership + 4 Interop + 3 Burn + 4 Workflow + 3 Audit + 3 Selector = 30, per §v1.7 row F-R6-009 reconciliation).

**Coverage gap:** No `kind_uuid_registry.rs` module exists on disk. RFC documents the 30 UUIDv5 fixtures in prose but no Rust implementation.

**Owned by mission:** `0967-A1-v1.9.2-landing` (sibling; 30 UUIDv5 constants + `kind_uuid_fixt` anti-drift helper).

### Gap 3: `domain_separators` (canonical `octo/` prefix) PENDING landing

RFC-0967-A1 v1.9.2 VH row v1.7 row + F-R8-DOMSEP-PREFIX-DRIFT: `AUDIT_VARIANT_HASH_DOMAIN` migrated from `cipherocto/audit/v1/` → canonical `octo/audit/v1/`. Per F-R8 fix-all cascade.

**Coverage gap:** No `domain_separators.rs` module exists on disk. AuditPolicy `AUDIT_VARIANT_HASH_DOMAIN` constant stale or absent.

**Owned by mission:** `0967-A1-v1.9.2-landing` (sibling; 8 canonical `octo/` prefix domain constants).

### Gap 4: `PolicyRegistry` trait extension PENDING landing

RFC-0967-A1 v1.9.2 PolicyRegistry trait documents 3-method trait (lookup_policy, register_policy, delegate_authority) consuming v016 `policy_registry` + `policy_kind_authority` tables from `0960-v3.5-landing`.

**Coverage gap:** No `policy_registry.rs` module in `crates/octo-policy-storage/src/`. Adapter crate substrate exists per `0206-009` LANDED per MEMORY; trait extension PENDING.

**Owned by mission:** `0967-A1-v1.9.2-landing` (sibling; PolicyRegistry trait + 3 methods in `octo-policy-storage/src/policy_registry.rs`).

### Gap 5: `WorkflowKind` trait signature amendments PENDING landing

RFC-0967-A1-A1 v1.2 effective (2026-08-22) per `rfcs/accepted/economics/0967-a1-a1-workflowkind-trait-sig-amendment.md` documents F-R8-WFCOMPOSITE-NO-PROOF-PARAM: WorkflowKind 4 methods take `proof: &[u8]` parameter replacing phantom `ctx: &WorkflowContext`.

**Coverage gap:** WorkflowKind trait does not exist on disk (per Gap 1); F-R8 amendment cannot apply until substrate trait lands.

**Owned by mission:** `0967-A1-v1.9.2-landing` (sibling; WorkflowKind 4-method signature includes `proof: &[u8]` parameter from initial landing).

## Sibling mission cross-references

- `0967-A1-v1.9.2-landing` — primary substrate ownership for RFC-0967-A1 v1.9.2 PENDING landing (6 traits + 30 UUIDv5 fixtures + 8 domain constants + PolicyRegistry trait + 36 byte-exact fixtures)

## Acceptance Criterion

- 1 inline retrofix applied to `0967-a` per audit findings (combined crate rename + RFC-0967-A1 v1.9.2 supersession in single Status block quote)
- 1 sibling mission filed (`0967-A1-v1.9.2-landing`) + cross-references 1 retrofix mission + 3 dependency missions via `depends_on` chain
- AC gate: `ls missions/claimed/0967-*.md` → 3 files (1 existing + 1 landing + 1 coordination)
- AC gate: `rg 'crate rename drift|RFC-0967-A1 v1.9.2 supersession' missions/claimed/0967-a-policy-object-graph.md` → 1 hit (retro-supersession note)
- AC gate: `rg 'pub trait AuthorityPolicy|pub trait WorkflowKind' missions/claimed/0967-A1-v1.9.2-landing.md` → 2 hits (sibling mission trait spec)
- Cross-RFC cite validation: Guard 2 PASS for all 1 retrofix + 2 new mission files
- Prettier clean
- No new INVALID cites introduced

## Files / Artifacts

- Edit: `missions/claimed/0967-a-policy-object-graph.md` (retro-supersession note in Status block + VH row 2026-08-24)
- New: `missions/claimed/0967-A1-v1.9.2-landing.md` (substrate landing ownership for 6 traits + 30 UUIDv5 fixtures + 8 domain constants + PolicyRegistry trait + 36 byte-exact fixtures)
- New: `missions/claimed/0967-A1-alignment-coordination.md` (this file)

## Cross-references

- RFC-0967-A1 v1.9.2 (canonical Accepted 2026-08-24 — 6 traits + 30 UUIDv5 fixtures + 8 domain constants + PolicyRegistry trait)
- RFC-0967-A1-A1 v1.2 effective 2026-08-22 (WorkflowKind trait signature F-R8-WFCOMPOSITE-NO-PROOF-PARAM amendment)
- RFC-0967 v1.1-Resolved (parent RFC — PolicyObject + intersect + is_subgraph LANDED via `0967-a-policy-object-graph`)
- RFC-0960 v3.5 (umbrella — provides vault substrate + `0960-v3.5-landing` v016 policy_registry + policy_kind_authority tables)
- RFC-0206 v3.0 (Database newtype + AdapterAllowlist — substrate adapter pattern; Layer B additive-only rule)
- Mission `0967-a-policy-object-graph` (LANDED 2026-08-07 — retrofix target)
- Mission `0967-A1-v1.9.2-landing` (sibling — substrate landing ownership)
- Mission `0960-v3.5-landing` (co-dependency — v016 policy_registry + policy_kind_authority tables)
- Mission `0206-001-substrate-newtype` v3.0 (co-dependency — Database newtype + AdapterAllowlist substrate)
- Mission `0206-009-adapter-crate-creation` v1.0 (co-dependency — `octo-policy-storage` adapter crate)
- Sibling coordination: `0959-alignment-coordination` + `0960-alignment-coordination` (cross-RFC harmonization pattern)

## Out of scope

- Retroactive supersession of older 0967-* missions beyond the 1 inline retrofix (per R19 scope discipline)
- LIVE policy_kind_authority delegate chains (PENDING per RFC-0967-A1 §2.6; separate future mission)
- 108 byte-exact vault_id TV (LANDED via `0960-vault-substrate-amendment`; out of scope)
- 5 policy-kind UUIDv5 fixtures for RFC-0960 v3.5 (5 Kinds per `0960-v3.5-landing` Step 5 — SEPARATE fixtures per RFC-0967-A1 §2.6 30-entry table; this mission owns 30, not 5)
- ZK subclass `PolicyGraph` proof (RFC-0958 subclass; deferred per `0967-a` mission Notes)
- Cross-RFC harmonization edits (research doc + companion RFC cross-refs) per `vault-monetary-research-consequence` Phase 5 (separate phase)
- Cargo command text rewrites in `0967-a-policy-object-graph` (e.g., `cargo test -p cipherocto-policy` → `cargo test -p octo-policy`) — historical mission text preserved verbatim; only retro-supersession note added per R19

## Dependencies

- `0967-a-policy-object-graph` (LANDED 2026-08-07 — retrofix target)
- `0967-A1-v1.9.2-landing` (sibling — substrate landing ownership)
- `0960-v3.5-landing` (OPEN 2026-08-24 — co-dependency for PolicyRegistry trait substrate)
- `0206-001-substrate-newtype` v3.0 (LANDED per MEMORY — co-dependency)
- `0206-009-adapter-crate-creation` v1.0 (LANDED per MEMORY — co-dependency)
- RFC-0967-A1 v1.9.2 (canonical Accepted state)

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                   |
| ------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| v1.0    | 2026-08-24 | Initial filing per RFC-0967-A1 v1.9.2 mission audit 2026-08-24. 1 inline retrofix category (combined crate rename + RFC-0967-A1 v1.9.2 supersession) + 1 sibling mission for v1.9.2 PENDING substrate landing. Pure coordination; no new substrate code in this mission. |
