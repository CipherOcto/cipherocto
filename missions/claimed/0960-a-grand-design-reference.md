# Mission 0960-a: Grand Design Reference Document

**RFC:** RFC-0960 (Economics): Grand Design — Vaults, Capabilities, Reservations
**Status:** Claimed (2026-07-23)

## Claimant

@mmacedoeu (agent-assisted)

## Pull Request

(none yet — docs deliverable, see Acceptance Criteria)

**Phase:** grand-design umbrella (W0 in plan `docs/plans/2026-07-23-economics-rfc-mission-order.md`)
**Master plan:** `docs/plans/2026-07-19-identity-master-plan.md` §4 row A (RFC-0960 reference)
**Session plan:** this mission (umbrella doc; no S0N-numbered session plan exists)

> **BLUEPRINT gate:** RFC-0960 reached Accepted v2.0 on 2026-07-23 (R1-R28 multi-round review closed; six companion RFCs promoted in lockstep). Per BLUEPRINT.md Mission Lifecycle, mission is CLAIMABLE. Claim filed 2026-07-23.

**Retro-supersession (2026-08-24 audit, RFC-0960 v3.5 supersession):** Umbrella RFC-0960 bumped to v3.5 (2026-08-23) by amendment `rfcs/accepted/economics/0960-v35-vault-path-taxonomy.md` (Mesh Open Path vs Corporate Closed Path taxonomy on same substrate; chain_metadata + ledger_chain_registry + policy_registry + policy_kind_authority + ValueTransfer trait surface documented as PENDING landing per research §16 mission `vault-chain-metadata`). Mission text + doc text preserved per historical-mission-preservation + R19 scope discipline. Follow-on substrate landing owned by mission `0960-v3.5-landing` (OPEN 2026-08-24). Constraint variant count drift: mission spec §3 says "23 variants"; doc §3 says "25 variants" (RFC-0960 grew during v3.1-v3.5 amendments); doc count = ground truth.

## Summary

Author the canonical navigation reference at `docs/architecture/grand-design.md` summarizing the 7-RFC economics stack (RFC-0960 umbrella + RFC-0961/0962/0963/0964/0965/0967 companions). The doc covers the WAL-primary architecture reframe (RFC-0960 v2.0), the four primitives (Vault, Capability, Reservation, Settlement), the 23-variant Constraint set, the audit-window state machine, the event-sourced ledger, the Economic VM, the Execution Envelope (RFC-0962), resource shard routing (RFC-0963), the Policy Object Graph (RFC-0967), the five new database-ergonomic primitives (Time Travel, Materialized Views, Event Store/CQRS, Git-Style Branches, Deterministic Cost Model), the multi-settlement + cross-chain surface, hierarchical vaults, and the central error code registry.

The doc is **navigation only** — every section points to the canonical RFC-0960 §1–§18 for normative spec. Re-specification is rejected per BLUEPRINT.md "Use Cases = intent, RFCs = design, Missions = execution" separation.

## Depends on (RFC + upstream missions)

| Dependency                                                           | Status                     | Required?                                                              |
| -------------------------------------------------------------------- | -------------------------- | ---------------------------------------------------------------------- |
| RFC-0960 (Grand Design)                                              | ACCEPTED v2.0 (2026-07-23) | YES — umbrella                                                         |
| RFC-0957 (Capability Token Format)                                   | Accepted (2026-07-20)      | YES — Capability primitive substrate                                   |
| RFC-0958 (ZK Capability Subclass)                                    | Accepted (2026-07-21)      | YES — ZK subclass binding                                              |
| RFC-0959 (Ask Settlement Chain)                                      |                            | YES — SettlementReceipt primitive                                      |
| RFC-0126 (Deterministic Serialization)                               | Accepted                   | YES — canonical_ser for all primitives                                 |
| RFC-0102 (Wallet Cryptography)                                       | Accepted                   | YES — Transfer primitive sketch                                        |
| RFC-0862 (Stoolap Sync Layer)                                        | Accepted v1.2.0            | YES — event log replication                                            |
| RFC-0909 (Deterministic Quota Accounting)                            | Accepted v69               | NO (coexistence only per RFC-0959 v1.0 Option A)                       |
| RFC-0961 (CIPHERO_SQL Deterministic SQL)                             |                            | YES — Deterministic SQL spec; DEFERRED for implementation (see §Notes) |
| RFC-0962 (ExecutionEnvelope Object Protocol)                         |                            | YES — ExecutionEnvelope spec                                           |
| RFC-0963 (Resource Shard Routing)                                    |                            | YES — shard routing spec                                               |
| RFC-0964 (Constraint Encoding Standard)                              |                            | YES — 23-variant encoding                                              |
| RFC-0965 (Capability Extension Format)                               |                            | YES — caveat type set                                                  |
| RFC-0967 (Policy Object Graph)                                       |                            | YES — PolicyReference caveat + PolicyGraph                             |
| Mission `missions/claimed/0957-a-capability-token-macaroon.md`       | Claimed (2026-07-20)       | NO (substrate, not blocking)                                           |
| Mission `missions/claimed/0957-b-provider-boundary-exercise-path.md` | Claimed (2026-07-20)       | NO (substrate, not blocking)                                           |
| Mission `missions/claimed/0959-a-ask-pricing-stoolap.md`             | Claimed (2026-07-20)       | NO (substrate, not blocking)                                           |

## Type Coverage

This mission is a documentation mission. Coverage is per **section** of the umbrella doc, not per code type.

| RFC-0960 Section                            | Doc Section Pointer | For                                                                               |
| ------------------------------------------- | ------------------- | --------------------------------------------------------------------------------- |
| §1 Architecture (WAL primary)               | Doc §1              | v2.0 reframe; capability-as-WAL-write-authorization                               |
| §1.1 Deterministic WAL Protocol             | Doc §1.1            | `WALSegment`, `WALEntry`                                                          |
| §1.2 ExecutionEnvelope as WAL Projection    | Doc §1.1            | envelope → SQL ops → WAL entries                                                  |
| §1.3 Capability as WAL-Write Authorization  | Doc §1.3            | capability ↔ policy_id ↔ WAL entry                                                |
| §1.4 Strategic Positioning                  | Doc §1.4            | enterprise migration pitch                                                        |
| §2 Primitives (4)                           | Doc §2              | Vault, Capability, Reservation, Settlement                                        |
| §3 Constraint Set (25 variants — post v3.5) | Doc §3              | Categorized table; encoded per RFC-0964; mission spec said "23 variants" pre-v3.5 |
| §4 Audit Window                             | Doc §4              | Reservation state machine                                                         |
| §5 Event-Sourced Ledger                     | Doc §5              | `transfer_events` schema                                                          |
| §6 Economic VM                              | Doc §6              | declarative policy language                                                       |
| §7 Atomic Swaps + Cross-Chain               | Doc §7              | MultiSettlement                                                                   |
| §8 Hierarchical Vaults                      | Doc §8              | capability-security lattice                                                       |
| §9 Horizontal Scalability                   | Doc §9              | shard routing; → RFC-0963                                                         |
| §10 Execution Envelope                      | Doc §10             | detailed; → RFC-0962                                                              |
| §14 Time Travel                             | Doc §14             | ASOF queries                                                                      |
| §15 Materialized Views                      | Doc §15             | chained hash projection                                                           |
| §16 Event Store/CQRS                        | Doc §16             | `event_log` + projection views                                                    |
| §17 Git-Style Branches                      | Doc §17             | Branch + Merge first-class                                                        |
| §18 Deterministic Cost Model                | Doc §18             | gas enum (DB-cost units)                                                          |
| Central Error Code Registry                 | Doc §20             | 33 codes from RFC-0960/0961/0962/0963/0964/0965/0967                              |
| Companion RFC map                           | Doc §21             | W0–W7 wave assignment                                                             |

## In Scope

1. **Author `docs/architecture/grand-design.md`** — navigation reference covering the sections above. Each section opens with a one-paragraph summary + a precise pointer to RFC-0960 §1–§18 for normative spec. Companion RFCs (RFC-0961/0962/0963/0964/0965/0967) cross-referenced at the point where they extend RFC-0960 primitives.
2. **Section completeness** — every numbered section in RFC-0960 §1–§18 + Central Error Code Registry is either covered in the doc or explicitly flagged as deferred (only RFC-0961 CIPHERO_SQL — see §Notes).
3. **Cross-RFC register** — doc §21 maps each companion RFC to the wave in `docs/plans/2026-07-23-economics-rfc-mission-order.md` (W1–W7) that picks it up.
4. **RFC-0961 deferral rationale** — doc §22 documents the explicit rationale for keeping RFC-0961 (CIPHERO_SQL) as Accepted-but-not-scheduled in the current wave plan (deferral-by-priority, not deferral-by-status).
5. **Formatting** — markdown conforms to repo conventions (RFC referencing rule: RFC-0957 not RFC-0957 (Accepted); mermaid diagrams over ASCII; ends with newline).

## Out of Scope (this mission only)

- Code implementation of any primitive → defer to W1–W7 missions
- New RFC authorship → RFC-0960 already Accepted v2.0
- Mission authorship for W1–W7 → separate missions
- Stoolap schema migration for `transfer_events` or `consensus_sessions` → sm-engine scope (W2/W6)

## Implementation Guide

**Target file path:** `docs/architecture/grand-design.md`
**Length target:** 200–400 lines. Concise navigation, not 1700-line re-spec.

**Section structure (per RFC-0960 §-numbering):**

```markdown
# Grand Design — Vaults, Capabilities, Reservations (RFC-0960)

## Status

**Spec authority:** RFC-0960 (Accepted v2.0, 2026-07-23). This document is a navigation reference. Normative spec lives in RFC-0960 + 6 companion RFCs.

## 1. Architecture (WAL primary)

## 1.1 Deterministic WAL Protocol

## 1.2 ExecutionEnvelope as WAL Projection

## 1.3 Capability as WAL-Write Authorization

## 1.4 Strategic Positioning

## 2. Primitives

### 2.1 Vault

### 2.2 Capability

### 2.3 Reservation

### 2.4 Settlement (alias to RFC-0959 SettlementReceipt)

### 2.5 Transfer (consequence, not primitive)

## 3. Constraint Set (23 variants)

## 4. Audit Window — Reservation state machine

## 5. Event-Sourced Ledger

## 6. Economic VM

## 7. Atomic Swaps + Cross-Chain

## 8. Hierarchical Vaults

## 9. Horizontal Scalability (→ RFC-0963)

## 10. ExecutionEnvelope (→ RFC-0962)

## 14. Time Travel — ASOF Queries

## 15. Materialized Views

## 16. Event Store/CQRS

## 17. Git-Style Branches

## 18. Deterministic Cost Model

## 20. Central Error Code Registry

## 21. Companion RFC Map (W0–W7)

## 22. RFC-0961 Deferral Rationale

## 23. References
```

**Cross-reference discipline:** RFC-0965 §Specification for caveat type spec; RFC-0964 §Specification for constraint encoding; RFC-0962 §Specification for envelope wire protocol; RFC-0963 §Specification for shard routing; RFC-0967 §3 The `PolicyGraph` DAG for policy graph; RFC-0959 §2 BurnEventRef Specification for SettlementReceipt primitive. Never inline normative content from RFC-0960 §1–§18 — always pointer.

## Acceptance Criteria

- [ ] **AC-1:** `docs/architecture/grand-design.md` exists; 200–400 line count; ends with newline
- [ ] **AC-2:** Doc covers RFC-0960 §1–§18 sections (with mermaid diagrams per repo conventions for state machines + flow)
- [ ] **AC-3:** Doc includes Central Error Code Registry table (33 codes from RFC-0960 §Central Error Code Registry)
- [ ] **AC-4:** Doc §21 maps each companion RFC to its wave (W1–W7) in `docs/plans/2026-07-23-economics-rfc-mission-order.md`
- [ ] **AC-5:** Doc §22 documents RFC-0961 deferral rationale (see §Notes)
- [ ] **AC-6:** Every RFC reference uses bare-number form (RFC-0960, not RFC-0960 (Accepted v2.0))
- [ ] **AC-7:** Prettier formatting clean (`npx prettier --write docs/architecture/grand-design.md`)
- [ ] **AC-8:** Mission advances from `claimed/` to `with-pr/` once PR opened; to `archived/` once merged

**Retro-supersession note (2026-08-24 audit, RFC-0960 v3.5 supersession):** Per audit findings 2026-08-24: (a) doc line count = 578 (over 200-400 target by 45%); (b) §3 Constraint Set count = 25 variants (mission spec said 23 pre-v3.5); (c) RFC-0960 umbrella now at v3.5 (per amendment `rfcs/accepted/economics/0960-v35-vault-path-taxonomy.md` 2026-08-23). Doc substantively complete; ACs left unchecked for future close-out pass + v3.5 cross-link update. Per R19 scope discipline + historical-mission-preservation: AC text preserved, retro-supersession note documents drift. v3.5 substrate landing owned by mission `0960-v3.5-landing`.

## Risks (this mission)

| Risk                                                                   | Mitigation                                                                                                    |
| ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| Doc bloats beyond 400 lines                                            | Disallow inline re-spec; only navigation + pointers                                                           |
| RFC-0962 execution-envelope ambiguity (replaces/redefined by RFC-0960) | Cite RFC-0960 §1.2 + RFC-0962 §Open Questions consistently                                                    |
| Cross-RFC section drift between doc and RFC-0960                       | Doc mirrors RFC-0960 §-numbering verbatim; doc PR review must diff against RFC-0960 §-numbering for any drift |
| Companion RFC promotes to v2.1+ breaking Doc cross-references          | Companion RFC status headers; re-check on each RFC bump                                                       |
| RFC-0961 deferral rationale absent                                     | Doc §22 explicit; master plan §7 deferral note + mission Notes row keeps rationale visible                    |

## Notes

### RFC-0961 deferral rationale

RFC-0961 (CIPHERO_SQL Deterministic SQL) is **Accepted v2.0** (2026-07-23, promoted in lockstep with RFC-0960). It is **deferred** in the 2026-07-23 wave plan's priority order, not because of accept-status. Rationale:

1. **Coupling.** RFC-0961 is the canonical SQL dialect for `ExecutionEnvelope`. It is only exercised when the envelope surface lands (W6). Building a full SQL parser before the envelope + KV substrate (W6) exists is premature.
2. **Specification completeness.** RFC-0961 §Open Questions resolved at RFC-0960 R28+; however, the parser implementation must consume RFC-0964 (constraint encoding) + RFC-0965 (caveat payloads) + RFC-0962 (envelope envelope shape). None of those crate consumers exist yet (W3 first).
3. **Reference impl dependency.** The minimum viable CIPHERO_SQL parser needs a deterministic SQL AST + a parser + a deterministic executor. The executor slots into the ExecutionEnvelope wire (W6). Without W6, the parser has no consumer.
4. **Re-evaluation trigger.** RFC-0961 will be re-evaluated after W6 (ExecutionEnvelope) lands. Likely placement: W8 (post-W7) or in v2.0 wave alongside W7 shard routing.

This deferral is **deferral-by-priority** (per user direction 2026-07-23: "RFC deferred for future in the priority order does not related with being accepted or not"). The RFC's Accepted status is unchanged; the wave plan just doesn't schedule it.

**Deferral action:** plan `docs/plans/2026-07-23-economics-rfc-mission-order.md` §3 + §10 already documents the deferral. Doc §22 makes it visible to future readers.

### Wave naptime note

Doc §21 must reflect the wave plan as written. Per memory `rfc-version-history` and `referencing-convention` rules, do NOT pin RFC version numbers in cross-references — only the doc text quoting RFC section numbers (`RFC-0960 §1.1`) is acceptable.

### CLAUDE.md referencing rule

Per CLAUDE.md "RFC Referencing rule": use RFC-0960, not RFC-0960 (Accepted v2.0 v23). Same for all 7 companion RFCs. Doc must follow.

---

**Submission Date:** 2026-07-23
**Last Updated:** 2026-07-23
**Version:** 1.0 (Open → Claimed 2026-07-23)
