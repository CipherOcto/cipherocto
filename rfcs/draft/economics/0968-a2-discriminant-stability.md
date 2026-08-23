# Discriminant Stability Sub-amendment (file: 0968-a2) — Rescoped Amendment Table

## Status

- **Version:** 0.2.0
- **Status:** Draft v0.2.0 (2026-08-22)
- **Sub-amendment of:** RFC-0968 (Economics): Reputation Registry (Accepted at `rfcs/accepted/economics/0968-reputation-registry.md`)
- **Rescope authority:** research doc §20 decision #1 (filing gate authorized) + multi-round adversarial review recommendation (R6 option c) v0.1.0 §1-§4 retired as fabricated

> This is a 1-page amendment-table-only sub-amendment. Prior v0.1.0 §1-§4 content was retired per R1-R6 adversarial review (loop DRY at R7+R8). v0.2.0 replaces §1-§4 with a controller-level codepoint amendment table mirroring parent RFC-0968 §28 amendment table convention, plus a governance-quorum carryover note (closed by parent RFC-0968 §28 amendments 26 + 27 in RFC-0968-A1).

## Authors

- Author: @cipherocto

## Maintainers

- Maintainer: @mmacedoeu

## Summary

This sub-amendment carries the controller-level codepoint reservations that parent RFC-0968 §28 line 3160 deferred to RFC-0968-A2 (next amendment round) or to mission 0968a on-chain anchoring. The amendment table below mirrors the parent §28 subsection convention (§28.1 Economic and Sybil Defenses) while remaining a separate sub-amendment file for the filing gate authority established by research doc §20 decision #1.

Two deferred amendments are carried:

- Amendment 40 — `ControllerIdMissing` codepoint reservation (closed-by-IMPL per `crates/octo-reputation/src/error.rs:217`; canonical codepoint `0x34`)
- Amendment 44 — `controller_id = blake3(governance_pubkey)` derivation (deferred; canonical derivation documented in mission `octo-reputation-controller-id-missing-variant.md`)

Governance quorum was closed in parent RFC-0968 §28 amendments 26 + 27 (RFC-0968-A1, I-5 + I-6) — informational carryover in §2 below.

## Dependencies

- **RFC-0968** (parent RFC; authoritative source for §28 amendment table convention)
- **Mission `octo-reputation-controller-id-missing-variant.md`** (Closed 2026-08-13; `ControllerIdMissing = 0x34` landed in substrate via `crates/octo-reputation/src/error.rs:217`; compat switched in `crates/quota-router-core/src/marketplace/reputation_compat.rs`)
- **Parent RFC-0968 §28 amendments 26 + 27** (governance quorum + trusted-clock, both closed by RFC-0968-A1)
- **Research doc `docs/research/2026-08-21-vault-monetary-representation-redesign.md` §20 decision #1** (filing gate authorization)

## §1 Controller-level Codepoint Amendments

| #   | Amendment                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             | Codepoint                       | Source / Verification                                                                                                                                                                                                         |
| --- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 40  | `ControllerIdMissing` variant reserved in `crates/octo-reputation/src/error.rs:217` with discriminant `0x34`. Original `0x2E` codepoint retired-in-error (already held by `RotationProvenanceMissingTombstoned = 0x2E` per `error.rs:193`, tombstone-did slash gate added in Round 7 substrate follow-on). Canonical codepoint `0x34` sits in `0x30..=0x3F` reserved range per parent §13 line 2641. Carries follow-on TODO in `crates/quota-router-core/src/marketplace/reputation_compat.rs` for compat writer to switch to the canonical variant. Existing consumers branching on `RecorderDidMalformed("controller_id must be non-zero...")` retain compat-layer behaviour for one release cycle. | `0x34`                          | Mission `octo-reputation-controller-id-missing-variant.md` (Closed 2026-08-13, 211 octo-reputation lib tests + 4 marketplace_reputation_async tests pass) + research doc v3.17 (R27 apply 2 codepoint correction 0x2E → 0x34) |
| 44  | `controller_id = blake3(governance_pubkey)` canonical derivation for all governance-anchored controller identity. Replaces legacy ad-hoc construction sites in substrate. Carries forward into quorum verification (parent §28 amendment 26: `GovernanceProof.governance_set_hash: [u8; 32]` binds officer key into suspension digest).                                                                                                                                                                                                                                                                                                                                                               | (derivation rule, no codepoint) | Mission `octo-reputation-controller-id-missing-variant.md` line 87 (cross-reference) + research doc v3.13 (R24 apply 2 DEFER-TO-A2 annotations corpus-wide)                                                                   |

## §2 Governance Quorum Carryover

Closed by parent RFC-0968 §28 amendment 26 (RFC-0968-A1, I-5): `GOVERNANCE_QUORUM = 3` constant + `GovernanceProof.governance_set_hash: [u8; 32]` field. Implementation MUST verify the active-set digest at the snapshot where `governance_pubkey` is active. Single-key-compromise attack closed.

Closed by parent RFC-0968 §28 amendment 27 (RFC-0968-A1, I-6): trusted-clock wrapper at API boundary. Caller-supplied `now_unix` is rejected at public RPC entrypoints; the receiving service supplies its trusted-clock value. Suspension/resume/rotation proofs carry the timestamp inside the signature, not as a separate parameter. Closes stale-proof replay.

Both carryovers are informational; no A2 amendment table rows are added (parent RFC-0968 §28 amendments 26 + 27 already closed them in RFC-0968-A1).

## §3 Reserved Codepoint Range

Per `crates/octo-reputation/src/error.rs` source order + parent RFC-0968 §13 table, the error discriminant codepoints currently occupy `0x01..=0x3A` monotonically (with document gaps for `#[allow(non_canonical)]`-tombstoned interim-session slots). `0x3B..=0xFF` reserved for future amendments per parent §13 line 2641.

| Codepoint | Variant                                               | Source line    |
| --------- | ----------------------------------------------------- | -------------- |
| `0x33`    | `AnchorSubmitterRejected(String)`                     | `error.rs:181` |
| `0x34`    | `ControllerIdMissing` (reserved, amendment 40)        | `error.rs:217` |
| `0x3A`    | `GossipEnvelopeInvalid(&'static str)` (table ceiling) | `error.rs:223` |

## §4 Sub-Amendment Procedure Note

This sub-amendment file exists for the filing gate authority established by research doc §20 decision #1. Parent RFC-0968 §28 amendment procedure holds that amendments are folded in place (§16, §20, §21, §22) without separate amendment documents. Future amendments to this codepoint table should consider folding into parent RFC-0968 §28.1 (Economic and Sybil Defenses) per that convention once RFC-0968-A2 promotion lands.

## Cross-References

- RFC-0968 (Economics): Reputation Registry (parent)
- RFC-0968 §28 (Amendments; amendment table convention precedent)
- RFC-0968 §13 (Error discriminant table; canonical anchor for §3 reserved range)
- Research doc `docs/research/2026-08-21-vault-monetary-representation-redesign.md` §20 (Phase 0 User Decision Matrix, decision #1)
- Mission `missions/claimed/octo-reputation-controller-id-missing-variant.md` (Closed 2026-08-13)
- Substrate source `crates/octo-reputation/src/error.rs` (codepoint assignments)

## Lifecycle Requirements

- **Status:** Draft
- **Acceptance target:** user-initiated Accept per BLUEPRINT.md §RFC Process
- **VH row addition:** required upon acceptance

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| ------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0.2.0   | 2026-08-22 | **Rescope per option (c) from R6 adversarial review recommendation.** Prior v0.1.0 §1-§4 content retired (verified fabricated per R1-R6 loop DRY at R7+R8). v0.2.0 = 1-page amendment-table-only sub-amendment mirroring parent §28 convention. §1 carries controller-level codepoint amendments 40 + 44 (with substrate-verified `0x34` for amendment 40). §2 carries governance-quorum OQ-V4 informational closure (parent §28 amendments 26 + 27 already closed). §3 documents reserved codepoint range per parent §13 + `error.rs` source order. |
| 0.1.0   | 2026-08-22 | Initial draft (filing gated per research doc §20 decision #1). Retired; body content §1-§4 fabricated per R1-R6 adversarial review (loop DRY at R7+R8). Commit history: created `c3e9889f`; initial draft superseded by v0.2.0 amendment-table-only rescope.                                                                                                                                                                                                                                                                                         |
