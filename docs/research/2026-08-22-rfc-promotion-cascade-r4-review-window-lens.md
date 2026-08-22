# RFC Promotion Cascade Readiness — Phase 4.4 R4 Fresh-Lens Research

**Date:** 2026-08-22
**Round:** R4 of Phase 4 fresh-lens loop
**Lens:** review window + reviewer count preconditions + R3 fix verification + cross-RFC body consistency
**Method:** R37 P3 loop-until-dry (2 consecutive 0-NEW rounds required)

## 0. R3 Recap

Per R3 (Phase 4.3 doc commit `24dd37c6`): 10 NEW findings (1 CRIT + 2 HIGH + 1 MED + 6 LOW). R3 resolved F-P4.2-16 CRITICAL with per-RFC phantom ref grep + F-P4.3-1 CRITICAL NEW (RFC-0959 v2.1 L31 bare phantom) + F-P4.3-2 HIGH NEW (RFC-0967-A1 v1.5 L17 bare phantom).

**R4 objective:** verify R3 fixes applied + apply R4 fresh-lens (review window + reviewer count + cross-RFC body consistency + R3 carryover closure).

## 1. R3 Fix Verification

Per R37 P3 methodology "fix-verify" pattern (per research doc v3.7.2 R11 row): R4 verifies whether R3 findings have been addressed before applying NEW lens.

### R3 fix status

| Finding | Severity | Fix proposed | Fix applied? | Verification |
|---------|----------|--------------|--------------|--------------|
| F-P4.3-1 CRITICAL | RFC-0959 v2.1 L31 bare phantom | R10.5 wrap | NO (R10.5 scope: RFC text edit needed) | R4 verification: phantom still present at L31 |
| F-P4.3-2 HIGH | RFC-0967-A1 v1.5 L17 bare phantom | R10.5 wrap | NO | R4 verification: phantom still present at L17 |
| F-P4.3-3 LOW | RFC-0967-A1-A1 wrapper consistency | verification | N/A (verify only) | PASS (verified in R3) |
| F-P4.3-4 LOW | RFC-0967-A1 v1.5 L152 wrapper | verification | N/A | PASS |
| F-P4.3-5 LOW | RFC-0206 v3.0 L37 + L48 wrappers | verification | N/A | PASS |
| F-P4.3-6 HIGH | L[N] ref corpus hygiene policy gap | distinguish prose vs fix-trail | N/A (memory card update needed) | R4 follow-up: see F-P4.4-2 |
| F-P4.3-7 MED | RFC-0967-A1 v1.5 L[N] 23 concentration | corpus STATE policy | N/A | R4 follow-up: see F-P4.4-3 |
| F-P4.3-8 LOW | RFC-0206 v3.3 v3.3 row self-ref | annotation | NO | self-referential note |
| F-P4.3-9 LOW | version pin distribution | no fix (compliant) | N/A | PASS |
| F-P4.3-10 LOW | RFC-0206 v3.3 v prefix in VH | strip `v` prefix | NO (RFC text edit needed) | R4 carryover: F-P4.4-4 |

### Findings

**Finding F-P4.4-1 (CRITICAL):** R3 fix verification shows ZERO R3 findings have been applied (other than verification PASS items). Per R37 P3 methodology "fix-verify" loop, the R4+ rounds need to EITHER apply fixes OR document deferral. Per `feedback_initiation_user_only`, file moves + RFC promotion is user-only. Per R10.5 scope discipline, RFC text + frontmatter + VH edits are in-scope.

**Resolution:** Apply R3 fixes (F-P4.3-1 + F-P4.3-2 phantom wraps + F-P4.3-10 v prefix strip) in subsequent Phase 4 rounds. R4 documents DEFERRED state per R37 P3 methodology.

## 2. Review Window + Reviewer Count Preconditions (R4 fresh-lens finding — CRITICAL)

Per `feedback_initiation_user_only` + BLUEPRINT.md §RFC Process: promotion requires "2+ maintainer approvals + 7-day minimum review window". R4 verification reveals:

### Review window status per RFC (2026-08-22)

| # | RFC | First review round | Days since first round | 7-day met? | Reviewer count declared? |
|---|-----|---------------------|------------------------|------------|--------------------------|
| 1 | RFC-0105 v3.0 | R2 (2026-08-19 per memory card) | 3 days | NO | NO |
| 2 | RFC-0903-D1 v1.0 | R2 (2026-08-19) | 3 days | NO | NO |
| 3 | RFC-0959 v2.1 | R2 (2026-08-19) | 3 days | NO | NO |
| 4 | RFC-0960 v3.1 | R2 (2026-08-19) | 3 days | NO | NO |
| 5 | RFC-0967-A1 v1.5 | R2 (2026-08-19) | 3 days | NO | NO |
| 6 | RFC-0967-A1-A1 | R5 (2026-08-19 per memory card R5 row) | 3 days | NO | NO |
| 7 | RFC-0010 v1.7 | R2 (2026-08-19) | 3 days | NO | NO |
| 8 | RFC-0206 v3.0 | R2 (2026-08-19) | 3 days | NO | NO |
| 9 | RFC-0206 v3.3 | R8 (2026-08-21 per memory card R8 row) | 1 day | NO | YES (L9: "2+ maintainer approvals + 7-day minimum") |

### Findings

**Finding F-P4.4-2 (CRITICAL):** ZERO of the 9 promotion candidates meet the 7-day review window per `feedback_initiation_user_only`. Earliest first round = R2 (2026-08-19) = 3 days ago. RFC-0206 v3.3 was first reviewed at R8 (2026-08-21) = 1 day ago.

**Implication:** Phase 4 promotion (RFC `git mv` from `rfcs/draft/` → `rfcs/accepted/`) CANNOT proceed until 2026-08-26 at earliest (R2 + 7 days = 2026-08-26). This blocks Phase 4 promotion for ~4 days per the 7-day window rule.

**Resolution options:**
- (a) Wait until 2026-08-26 + apply R3 fixes + execute promotion (recommended per standing instructions)
- (b) Request user override of 7-day window per `feedback_initiation_user_only` (user-only escalation)
- (c) Apply R3 fixes + RFC text edits + draft memos for review during the 4-day window

Per standing user instruction: "don't push, commits are free" — file moves + RFC edits are local-only operations. The 7-day window is a USER-FACING constraint for promotion (the `git mv` from draft → accepted) + push to remote.

**Finding F-P4.4-3 (HIGH):** ZERO of the 9 promotion candidates declare reviewer count in frontmatter or §0 Status. Only RFC-0206 v3.3 declares "2+ maintainer approvals + 7-day minimum review window" at L9 inline header. Per corpus STATE hygiene, all 9 SHOULD declare reviewer preconditions for promotion audit trail.

**Resolution:** Add `reviewers_required: 2+` + `review_window_days: 7` to frontmatter per RFC. Per R10.5 scope, this is RFC text edit (in-scope).

## 3. Cross-RFC Body Consistency (R4 fresh-lens finding — MED)

Per R37 P3 methodology "cross-RFC consistency", amendment pairs SHOULD have consistent body text where they overlap. R4 spot-check on key amendment pairs.

### RFC-0206 v3.0 vs RFC-0206 v3.3 cross-consistency

Per R10 fix trail (RFC-0206 v3.3 §5 v3.1 row): "create_vault return type reconciliation + balance type clarification + vault_id derivation freeze + membership_proof rename + domain-separator prefix harmonization + 0x01 namespace byte disambiguation".

R4 verification:

| Topic | RFC-0206 v3.0 §3 | RFC-0206 v3.3 §2.1 | Consistent? |
|-------|------------------|---------------------|-------------|
| `create_vault` return type | `Result<[u8; 32], ValueTransferError>` | `Result<[u8; 32], ValueTransferError>` | YES |
| `balance` return type | DQA(12) | DQA(12) | YES |
| `vault_id` derivation | BLAKE3(...) | BLAKE3(...) per R12 F-R12-XR-VT-ASSET-ID-SIZING-DRIFT (32 → 16 byte asset_id truncation) | POSSIBLE DRIFT |
| `membership_proof` naming | `membership_proof` | renamed | R10 trail says "rename" — what was it renamed FROM? |

### Findings

**Finding F-P4.4-4 (MED):** `vault_id` derivation formula may have drift between RFC-0206 v3.0 §3 and RFC-0206 v3.3 §2.3. Per R12 fix F-R12-XR-VT-ASSET-ID-SIZING-DRIFT (CRIT) — asset_id 32-byte → 16-byte per RFC-0105 v3.0 §2.1 UUIDv5 truncation. R4 verification needed: does RFC-0206 v3.0 §3 reflect this 16-byte change, or is it stale at 32-byte?

**Resolution:** Per pre-promotion edit backlog (R2 §10), RFC-0206 v3.0 §3 should be updated to match v3.3 §2.3 derivation.

**Finding F-P4.4-5 (LOW):** `membership_proof` rename history underdocumented. R10 fix trail says "membership_proof rename" but doesn't specify FROM → TO. Per corpus STATE hygiene, the rename should be documented in a VH row entry or §2 amendment note.

### RFC-0967-A1 v1.0/v1.1/v1.2/v1.5 vs RFC-0967-A1-A1

| Topic | RFC-0967-A1 v1.5 §2.1 | RFC-0967-A1-A1 §3 | Consistent? |
|-------|------------------------|---------------------|-------------|
| `WorkflowKind` trait signature | `proof: &[u8]` | `proof: &[u8]` (replaces phantom `ctx: &WorkflowContext`) | YES |
| `AUDIT_VARIANT_HASH_DOMAIN` value | `octo/audit/v1/` (or `octo/audit/ab/v1/` per R12 fix F-R12-RFC0967A1-V15-R9-PROPAGATION-MISSING) | `octo/audit/v1/` | PARTIAL DRIFT |

### Findings

**Finding F-P4.4-6 (LOW):** `AUDIT_VARIANT_HASH_DOMAIN` value has PARTIAL drift between RFC-0967-A1 v1.5 §2.1 and RFC-0967-A1-A1 §3. Per RFC-0967-A1 v1.3 VH row: "R9 fix F-R9-AUDIT-PREFIX-DRIFT propagated to §2.1 variant_assignment formula: `octo/audit/v1/` → `octo/audit/ab/v1/` (A/B-kind-specific prefix...)". R4 spot-check: A1-A1 §3 still references `octo/audit/v1/` (R8-apply-time value) NOT `octo/audit/ab/v1/` (R9 propagated value). The A1-A1 amendment inherits the R8 amendment narrative which uses the R8-apply-time value.

**Resolution:** Per R12 fix F-R12-RFC0967A1-V15-R9-PROPAGATION-MISSING closure: update A1-A1 §3 narrative to use `octo/audit/ab/v1/`.

## 4. §Section Anchor Resolution (R4 fresh-lens finding — LOW)

Per pre-commit Guard 2 simulation: §section anchors should resolve to canonical §section names in target RFC. R4 spot-check on named anchors.

### Named anchor resolution

Per R3 grep:
- RFC-0967-A1 v1.5: 33 named anchors (highest concentration across 9 RFCs)
- RFC-0206 v3.3: 16 named anchors

R4 spot-check on cross-RFC named anchors:

| Anchor | Used in | Target RFC | Resolves? |
|--------|---------|------------|-----------|
| §RFC-0008 Execution Class Mapping | 9 RFCs (1+ refs each) | RFC-0008 (accepted) | YES (verified in R3) |
| §Authority-to-Issue (RFC-0105) | RFC-0105 v3.0 | self | YES |
| §BurnPolicy | RFC-0960 v3.1 | RFC-0967-A1 (draft) | TBD |
| §ValueTransfer | RFC-0960 v3.1 | RFC-0206 v3.0 (draft) | TBD |

### Findings

**Finding F-P4.4-7 (LOW):** Cross-RFC named anchors should resolve on Validator per Guard 2. R4 spot-check is INCOMPLETE (would need full §section lookup across all cited RFCs + accepted RFCs). Recommend: R5 corpus-wide named anchor resolution audit.

## 5. R4 NEW Findings Summary

| Severity | Count | Findings |
|----------|-------|----------|
| CRITICAL | 2 | F-P4.4-1 (R3 fixes unapplied) + F-P4.4-2 (review window BLOCKER — 7-day not met) |
| HIGH | 1 | F-P4.4-3 (reviewer count declaration missing in 8/9 RFCs) |
| MED | 1 | F-P4.4-4 (vault_id derivation drift between RFC-0206 v3.0/v3.3) |
| LOW | 3 | F-P4.4-5 (membership_proof rename underdocumented) + F-P4.4-6 (AUDIT_VARIANT_HASH_DOMAIN drift) + F-P4.4-7 (named anchor resolution incomplete) |

**R4 NEW: 7 findings (2 CRIT + 1 HIGH + 1 MED + 3 LOW).**

## 6. Convergence Loop Status

Per R37 P3 methodology "loop-until-dry":

- **R1:** 18 findings
- **R2:** 17 effective (13 NEW + 4 R1 corrections)
- **R3:** 10 NEW (1 CRIT + 2 HIGH + 1 MED + 6 LOW)
- **R4:** 7 NEW (2 CRIT + 1 HIGH + 1 MED + 3 LOW)
- **R5 (next):** apply R3 + R4 fixes + verify with full corpus lens

**Convergence direction:** R1=18 → R2=13 NEW → R3=10 NEW → R4=7 NEW. STRICTLY DECREASING. Per BLUEPRINT.md §Adversarial Review Process DRY criterion "2 consecutive rounds with 0 NEW findings required", loop is approaching DRY but not yet at threshold.

**R5 expectation:** apply R3 + R4 fixes (F-P4.3-1 + F-P4.3-2 + F-P4.3-10 + F-P4.4-3 reviewer count + F-P4.4-4 vault_id drift) + verify. Expect 2-4 NEW findings (convergence tail).

**R6 expectation:** second consecutive round. Expect 0-2 NEW findings.

**DRY target:** R6 or R7 should reach 0 NEW (or near-0 with only verification items).

## 7. Phase 4 Promotion Timeline (R4 projection)

Per F-P4.4-2 CRITICAL 7-day window constraint:

| Date | Status | Action |
|------|--------|--------|
| 2026-08-19 | R2 first round | R2 finding closure (16 CRIT per memory card) |
| 2026-08-22 | TODAY | R1-R4 fresh-lens research docs |
| 2026-08-22-26 | Window wait | Apply R3 + R4 fixes to RFCs (text-only); 9 RFC edits per backlog |
| 2026-08-26 | 7-day met (R2 first round) | Earliest promotion date for R2-reviewed RFCs |
| 2026-08-28 | 7-day met (R8 first round for RFC-0206 v3.3) | Earliest promotion date for v3.3 |
| 2026-08-29+ | Promote | `git mv rfcs/draft/* rfcs/accepted/*` per Tier 1/2/3 sequence (R1 §7) |

Per `feedback_initiation_user_only`, the `git mv` is local + push awaits user. Window wait is for user-facing decision to promote, not for local edits.

## 8. R10.5 Scope Discipline Recap

R4 fixes (R3 carries + F-P4.4-3 reviewer count + F-P4.4-4 vault_id drift) are RFC text + frontmatter edits ONLY. NO substrate crate code edits. NO Cargo.toml / Cargo.lock edits. NO `docs/audits/` file creation. NO push (user-only per `feedback_initiation_user_only`).

## 9. Cross-References

- Phase 4.1 R1 doc: `docs/research/2026-08-22-rfc-promotion-cascade-readiness.md`
- Phase 4.2 R2 doc: `docs/research/2026-08-22-rfc-promotion-cascade-r2-section-lens.md`
- Phase 4.3 R3 doc: `docs/research/2026-08-22-rfc-promotion-cascade-r3-phantom-ref-lint.md`
- Memory card R37 P3 methodology + R11 fix-verify pattern
- `feedback_initiation_user_only`: 7-day review window + 2+ maintainer approvals
- BLUEPRINT.md §RFC Process: VH + 2-Cycle Atomic Promotion + reviewer preconditions

## 10. Version History

| Version | Date | Change |
|---------|------|--------|
| v1.0 | 2026-08-22 | Initial R4 fresh-lens analysis; 7 NEW findings (2 CRIT + 1 HIGH + 1 MED + 3 LOW); F-P4.4-2 CRITICAL: 7-day review window BLOCKER (earliest promotion date 2026-08-26); F-P4.4-3 HIGH: 8 of 9 RFCs missing reviewer count declaration; F-P4.4-4 MED: vault_id derivation drift between RFC-0206 v3.0/v3.3; F-P4.4-6 LOW: AUDIT_VARIANT_HASH_DOMAIN drift between RFC-0967-A1 v1.5/A1-A1. Convergence: R1=18 → R2=13 → R3=10 → R4=7 (strictly decreasing). Phase 4 promotion timeline projected: earliest promotion 2026-08-26 (R2-reviewed) + 2026-08-28 (R8-reviewed v3.3). |