---
rfc: 0968-A2
title: Discriminant Stability Sub-amendment — Rescoped Amendment Table
status: Draft
version: 0.3.0
date: 2026-08-23
amends: RFC-0968-A1
builds_on:
  - rfcs/accepted/economics/0968-reputation-registry.md
  - rfcs/accepted/economics/0968-a1-reputation-registry-amendments.md
  - docs/research/2026-08-21-vault-monetary-representation-redesign.md
---

# Discriminant Stability Sub-amendment — Rescoped Amendment Table

## Status

**Draft (v0.3.0, 2026-08-23).**

> v0.3.0 = R9 fix-all pass on the v0.2.0 rescope. v0.2.0 was a 1-page amendment-table-only sub-amendment (R6 option c from R1-R6 adversarial review; v0.1.0 §1-§4 retired as fabricated per loop DRY at R7+R8). v0.3.0 corrects cite-fabrication findings surfaced in R9 fresh-eyes review: (H1) dropped fabricated `0x30..=0x3F` reserved-range attribution to parent §13; (H2) `0x3B..=0xFF` reserved-range now attributed to substrate `is_reserved()` test, not parent §13 (parent says `0x2A..=0xFF`); (H3) §3 now notes substrate doc-comment admits `0x3A` is in reserved band; (H4-H7) YAML frontmatter added per sibling RFC convention; (H8) partial-unblocker scope documented (amendments 9-82 realignment deferred). v0.3.0 is **Draft**; YAML `status: Draft` per BLUEPRINT.md §RFC Process enum.

## Authors

- Author: @cipherocto

## Maintainers

- Maintainer: @mmacedoeu

## Summary

This sub-amendment carries the controller-level codepoint reservations that parent RFC-0968 §28 line 3160 deferred to RFC-0968-A2 (next amendment round) or to mission `missions/claimed/octo-reputation-controller-id-missing-variant.md`. The amendment table below mirrors the parent §28 subsection convention (§28.1 Economic and Sybil Defenses) while remaining a separate sub-amendment file for the filing gate authority established by research doc §20 decision #1.

Two amendments deferred by parent §28 line 3160 are carried:

- Amendment 40 — `ControllerIdMissing` codepoint reservation (now **closed-by-IMPL** per `crates/octo-reputation/src/error.rs:217`; canonical codepoint `0x34`)
- Amendment 44 — `controller_id = blake3(governance_pubkey)` derivation (deferred; canonical derivation documented in mission `missions/claimed/octo-reputation-controller-id-missing-variant.md`)

Governance quorum open-question `OQ-V4` (defined in §2 below) was closed in parent RFC-0968 §28 amendments 26 + 27 (RFC-0968-A1, I-5 + I-6) — informational carryover in §2 below.

**Partial-unblocker scope:** Substrate `crates/octo-reputation/src/error.rs` header doc-comment states "Do NOT change discriminants until RFC-0968-A2 lands." That header doc-comment refers to the amendments-9-82 realignment (parent RFC §13 + substrate gap closure). A2 v0.3.0 carries only amendments 40 + 44 + governance-quorum informational carryover; amendments 9-82 realignment is **out of scope** and deferred to a future sub-amendment (filed-gate convention requires research doc authorization per §20 decision #1; not yet authorized). Substrate header doc-comment stays accurate as a forward-looking note.

## Dependencies

- **RFC-0968** (grandparent RFC; authoritative source for §13 error discriminant table + §28 amendment table convention)
- **RFC-0968-A1** (immediate parent amendment; this sub-amendment's `amends:` addressee)
- **Mission `missions/claimed/octo-reputation-controller-id-missing-variant.md`** (Closed 2026-08-13; `ControllerIdMissing = 0x34` landed in substrate via `crates/octo-reputation/src/error.rs:217`; compat switched in `crates/quota-router-core/src/marketplace/reputation_compat.rs`)
- **Parent RFC-0968 §28 amendments 26 + 27** (governance quorum + trusted-clock, both closed by RFC-0968-A1)
- **Research doc `docs/research/2026-08-21-vault-monetary-representation-redesign.md` §20 decision #1** (filing gate authorization)

## §1 Controller-level Codepoint Amendments

| #   | Amendment                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            | Codepoint                       | Source / Verification                                                                                                                                                                                                                          |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 40  | `ControllerIdMissing` variant reserved in `crates/octo-reputation/src/error.rs:217` with discriminant `0x34`. Original `0x2E` codepoint retired-in-error (already held by `RotationProvenanceMissingTombstoned = 0x2E` per `error.rs:193`, tombstone-did slash gate added in Round 7 substrate follow-on). `0x34` sits in substrate-assigned codepoint range (`0x01..=0x39` per substrate header doc-comment; substrate reserves `0x3B..=0xFF` per `is_reserved()` test at `error.rs` test-module). Carries follow-on TODO in `crates/quota-router-core/src/marketplace/reputation_compat.rs` for compat writer to switch to the canonical variant. Existing consumers branching on `RecorderDidMalformed("controller_id must be non-zero...")` retain compat-layer behaviour for one release cycle. | `0x34`                          | Mission `missions/claimed/octo-reputation-controller-id-missing-variant.md` (Closed 2026-08-13, 211 octo-reputation lib tests + 4 marketplace_reputation_async tests pass) + research doc v3.17 (R27 apply 2 codepoint correction 0x2E → 0x34) |
| 44  | `controller_id = blake3(governance_pubkey)` canonical derivation for all governance-anchored controller identity. Replaces legacy ad-hoc construction sites in substrate. Carries forward into quorum verification (parent §28 amendment 26: `GovernanceProof.governance_set_hash: [u8; 32]` binds officer key into suspension digest).                                                                                                                                                                                                                                                                                                                                                                                                                                                              | (derivation rule, no codepoint) | `crates/quota-router-core/src/marketplace/reputation_compat.rs:73` (`controller_id_from_governance_pubkey`) + research doc v3.13 (R24 apply 2 DEFER-TO-A2 annotations corpus-wide)                                                             |

## §2 Governance Quorum Carryover (OQ-V4)

**OQ-V4** = "Governance quorum + trusted-clock wrapper" — the open-question closed by parent RFC-0968 §28 amendments 26 + 27 (RFC-0968-A1, I-5 + I-6).

Closed by parent RFC-0968 §28 amendment 26 (RFC-0968-A1, I-5): `GOVERNANCE_QUORUM = 3` constant + `GovernanceProof.governance_set_hash: [u8; 32]` field. Implementation MUST verify the active-set digest at the snapshot where `governance_pubkey` is active. Single-key-compromise attack closed.

Closed by parent RFC-0968 §28 amendment 27 (RFC-0968-A1, I-6): trusted-clock wrapper at API boundary. Caller-supplied `now_unix` is rejected at public RPC entrypoints; the receiving service supplies its trusted-clock value. Suspension/resume/rotation proofs carry the timestamp inside the signature, not as a separate parameter. Closes stale-proof replay.

Both carryovers are informational; no A2 amendment table rows are added (parent RFC-0968 §28 amendments 26 + 27 already closed OQ-V4 in RFC-0968-A1).

## §3 Reserved Codepoint Range (substrate authority + parent §13)

**Parent RFC-0968 §13 table** documents discriminants `0x01..=0x29` monotonically (line 2641); parent explicitly states `0x2A..=0xFF are reserved for future variants` per Round 10 M1.

**Substrate `crates/octo-reputation/src/error.rs`** extends the assigned range to `0x01..=0x39` in source order (per Round 2 review C2 header doc-comment); substrate reserves `0x3B..=0xFF` per `is_reserved()` test (line referenced in substrate test-module). The `0x2A..=0x3A` gap is **substrate-local drift** (17 codepoints) not yet reflected in parent §13 — parent §13 amendment to record substrate variants is part of amendments-9-82 realignment (deferred per §1 partial-unblocker scope).

**Known substrate-vs-parent disagreement:** substrate `GossipEnvelopeInvalid = 0x3A` sits in the parent-reserved band (parent §13 reserves `0x2A..=0xFF`; `0x3A` is in that band). Substrate header doc-comment acknowledges this drift ("implementation puts `GossipEnvelopeInvalid = 0x3A` in the reserved band"). A2 does NOT ratify this as a feature; A2 §3 records the state for the amendments-9-82 realignment sub-amendment to resolve.

| Codepoint     | Variant                                                  | Source line            | Authority                                  |
| ------------- | -------------------------------------------------------- | ---------------------- | ------------------------------------------ |
| `0x33`        | `AnchorSubmitterRejected(String)`                        | `error.rs:181`         | substrate (parent §13 gap: `0x2A..=0x3A`)  |
| `0x34`        | `ControllerIdMissing` (assigned, amendment 40)           | `error.rs:217`         | substrate (parent §13 gap: `0x2A..=0x3A`)  |
| `0x39`        | Substrate table ceiling (per Round 2 review C2 doc)      | `error.rs`             | substrate header doc-comment               |
| `0x3A`        | `GossipEnvelopeInvalid(&'static str)` (in reserved band) | `error.rs:223`         | substrate drift (parent §13 says reserved) |
| `0x3B..=0xFF` | **Substrate reserved** (per `is_reserved()` test)        | `error.rs` test-module | substrate test (NOT parent §13)            |

## §4 Sub-Amendment Procedure Note + Partial-Unblocker Scope

This sub-amendment file exists for the filing gate authority established by research doc §20 decision #1. Parent RFC-0968 §28 amendment procedure holds that amendments are folded in place (§16, §20, §21, §22) without separate amendment documents. Future amendments to this codepoint table should consider folding into parent RFC-0968 §28.1 (Economic and Sybil Defenses) per that convention once RFC-0968-A2 promotion lands.

**Partial-unblocker scope (re-stated from Summary):** A2 v0.3.0 carries amendments 40 + 44 + governance-quorum OQ-V4 informational carryover. Substrate header doc-comment "Do NOT change discriminants until RFC-0968-A2 lands" refers to amendments 9-82 realignment (parent §13 + substrate gap closure for `0x2A..=0x3A`); that realignment is **deferred** and not authorized by research doc §20. Substrate header doc-comment remains accurate as a forward-looking note pointing at A2 (and its successors, when authorized). The cross-RFC cite chain required for amendments 9-82 (parent §13 amendment rows + substrate gap closure + tombstone semantics reconciliation) exceeds the 1-page amendment-table scope A2 v0.3.0 inherits.

## Cross-References

- RFC-0968 (Economics): Reputation Registry (grandparent RFC)
- RFC-0968-A1 (Economics): Reputation Registry Amendments (immediate parent amendment; `amends:` addressee)
- RFC-0968 §28 (Amendments; amendment table convention precedent; amendments 26 + 27 closed OQ-V4)
- RFC-0968 §13 (Error discriminant table; canonical anchor for §3 reserved range)
- Research doc `docs/research/2026-08-21-vault-monetary-representation-redesign.md` §20 (Phase 0 User Decision Matrix, decision #1)
- Mission `missions/claimed/octo-reputation-controller-id-missing-variant.md` (Closed 2026-08-13; canonical `0x34` codepoint + blake3 derivation)
- Substrate source `crates/octo-reputation/src/error.rs` (codepoint assignments; header doc-comment "Do NOT change discriminants until RFC-0968-A2 lands"; `is_reserved()` test)
- Substrate source `crates/quota-router-core/src/marketplace/reputation_compat.rs` (compat writer + blake3 derivation function `controller_id_from_governance_pubkey`)

## Lifecycle Requirements

- **Status:** Draft (per YAML frontmatter)
- **Acceptance target:** user-initiated Accept per BLUEPRINT.md §RFC Process
- **VH row addition on Accept:** append a row to parent RFC-0968 §28 amendment table at amendment rows 40 + 44; remove A2 §1 amendment table per parent folding convention
- **Future sub-amendment precedent:** A2 v0.3.0 establishes the sub-amendment YAML convention (`rfc: XXXX-A2`, `amends: XXXX-A1`, `builds_on: [XXXX, XXXX-A1]`) for future sub-amendments authorized by research doc §20 decision pattern

## Version History

| Version | Date       | Changes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| ------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0.3.0   | 2026-08-23 | **R9 fix-all pass.** YAML frontmatter added (H4-H7); §1 amendment 40 dropped fabricated `0x30..=0x3F` parent-§13 cite, now cites substrate `is_reserved()` + substrate header doc (H1); §3 now distinguishes parent §13 reserved range `0x2A..=0xFF` from substrate reserved range `0x3B..=0xFF` (H2) + acknowledges `0x3A = GossipEnvelopeInvalid` as substrate drift in parent-reserved band (H3); §1 + §4 document partial-unblocker scope (H8); §2 names OQ-V4 in body (MED); Cross-Refs adds RFC-0968-A1 (MED); VH column renamed Change→Changes; v0.1.0 row tagged `State: Superseded`; mission cited by full `missions/claimed/` path uniformly; `0x34` relabeled "(assigned)" not "(reserved)"; "monotonically" → "in source order". |
| 0.2.0   | 2026-08-22 | **Rescope per option (c) from R6 adversarial review recommendation.** Prior v0.1.0 §1-§4 content retired (verified fabricated per R1-R6 loop DRY at R7+R8). v0.2.0 = 1-page amendment-table-only sub-amendment mirroring parent §28 convention. §1 carries controller-level codepoint amendments 40 + 44 (with substrate-verified `0x34` for amendment 40). §2 carries governance-quorum OQ-V4 informational closure (parent §28 amendments 26 + 27 already closed). §3 documents reserved codepoint range per parent §13 + `error.rs` source order.                                                                                                                                                                                         |
| 0.1.0   | 2026-08-22 | Initial draft (filing gated per research doc §20 decision #1). **State: Superseded by v0.2.0.** Retired; body content §1-§4 fabricated per R1-R6 adversarial review (loop DRY at R7+R8). Commit history: created `c3e9889f`; initial draft superseded by v0.2.0 amendment-table-only rescope, then by v0.3.0 fix-all pass.                                                                                                                                                                                                                                                                                                                                                                                                                   |
