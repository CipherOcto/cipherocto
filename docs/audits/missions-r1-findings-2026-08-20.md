# Mission R1 Aggregate Findings — 2026-08-20

## Scope

7 missions reviewed under multi-round adversarial protocol (per /goal "continue multi round adversarial review of the missions just created loop until dry"):

- `0205-001-stewards-meta-bootstrap` (Phase 0.1)
- `0205-002-phase1-deliverables` (Phase 1.3-1.11)
- `0205-003-r10-review` (RFC-0205+0206 v2.0 R10 reviewer pass)
- `0206-001-substrate-newtype` (Layer A substrate refactor)
- `0206-002-layer-b-type-renames` (29-site TYPE rename)
- `0206-003-trait-moves` (HolderRegistry + StoolapDidRegistry moves)
- `0206-004-adapter-crates` (5 adapter crates + facade)

7 reviewers dispatched in parallel; all returned.

## Severity Tally

| Mission   |   CRIT |   HIGH |    MED |    LOW |   Total |
| --------- | -----: | -----: | -----: | -----: | ------: |
| 0205-001  |      2 |      8 |      7 |      3 |      20 |
| 0205-002  |      2 |      5 |      6 |      5 |      18 |
| 0205-003  |      1 |      4 |      7 |      2 |      14 |
| 0206-001  |      2 |      4 |      6 |      7 |      19 |
| 0206-002  |      0 |      4 |      5 |      2 |      11 |
| 0206-003  |      2 |      3 |      4 |      2 |      11 |
| 0206-004  |      4 |      4 |      8 |      3 |      19 |
| **TOTAL** | **13** | **32** | **43** | **24** | **112** |

## Cross-Cutting Defect Classes

### Class 1 — Phantom Cross-References

Memory slug or path cited that does not exist:

- 0205-001 cites "§External Trust Root (cross-reference)" — fabricated section heading; term appears only in §Definitions table row.
- 0205-003 cites `docs/audits/rfc-0205-0206-r8-findings-status.md` — actual file is `rfc-0205-0206-r8-findings-2026-08-20.md` (dated suffix).
- 0205-003 cites `feedback_initiation_user_only` (BROKEN) — correct memory: `feedback_initiative_user_only`.
- 0206-003 cites `crates/octo-ident/src/did_registry_storage.rs` (fabricated) — RFC mandates `crates/octo-ident-storage/src/did_registry.rs:139`.
- 0206-004 cites `crates/octo-policy/src/lib.rs` (does not exist) — on-disk is `crates/cipherocto-policy/` (rename owned by non-existent `0206-cipherocto-policy-rename-alignment` mission).

**Density:** 5 of 7 missions affected. Root cause: missions invented paths/IDs without filesystem verification.

### Class 2 — Missing or Circular Dependencies

Per `no-phantom-mission-pointers` memory rule:

- 0205-001 YAML frontmatter lacks `depends_on:` field.
- 0205-002 lacks `0206-001-substrate-newtype` dep (edits file the mission owns).
- 0206-002 lacks `0206-003-trait-moves` dep (TV-0206-A7 rg scope includes `octo-cap-macaroon/src`).
- 0206-002 lacks `0206-004-adapter-crates` dep (`cargo build --workspace --all-targets` cannot pass without 5 adapter crates on disk).
- 0206-003 lacks `0206-002-layer-b-type-renames` dep (race: file moves before rename).
- 0206-003 cites `0206-004-adapter-crates` as target crate creator — wrong; 0206-004 adapter list has 5 crates, no `octo-ident-storage`.
- 0206-004 lacks `0206-cipherocto-policy-rename-alignment` dep (nonexistent mission required for `cipherocto-policy` → `octo-policy` rename).

**Density:** 6 of 7 missions affected. Root cause: missions drafted in batch without cross-checking DAG.

### Class 3 — Count Contradictions

Mission ACs cite counts the scope cannot validate:

- 0206-001 scope: "11-item re-export set (11 `pub use` + 1 `pub mod migrations`)" but cap: "≤ 8 `pub use` statements (TV-0206-A4)". 11 > 8 — direct numerical contradiction.
- 0206-002 description: "29 sites" but scope: "12 more sites TBD on per-file audit". 17+12=29 unverifiable.
- 0206-004 description: "4 trait declarations" but body lists 5 (4 NEW + 1 move).
- 0206-004 description: "5 adapter crates" but count = 4 NEW + 1 move (`VaultLookup` already declared).

**Density:** 3 of 7 missions. Root cause: copy-paste of RFC row counts without per-mission validation.

### Class 4 — Scope Conflicts Between Missions

Same file/line owned by ≥2 missions:

- `crates/octo-storage-core/Cargo.toml` `stoolap = { rev = "<sha-0>" }` pin: claimed by both 0205-002 (Phase 1.3) and 0206-001 (substrate skeleton). Two missions edit same line.
- `docs/runbooks/stoolap-steward.md`: claimed by both 0205-001 (bootstrap commit SHA documentation) and 0205-002 (procedures runbook).
- `crates/quota-router-storage/src/stoolap_did_registry.rs:139, :201`: TYPE-rename by 0206-002 AND impl-move by 0206-003. No ordering clause.
- `crates/quota-router-storage/src/holder_registry.rs:33`: TYPE-rename by 0206-002 (line 33 is `pub trait HolderRegistry`, not `stoolap::Database` — fabrication) AND trait move by 0206-003.

**Density:** 4 conflict pairs across 4 missions. Root cause: missions drafted in batch without per-file ownership matrix.

### Class 5 — AC Gates Use Wrong Primitive

AC verifications count or check the wrong thing:

- 0206-001 AC: `rg -c '^\s*pub use\b'` counts statements, not items — cannot verify "exactly 11 items".
- 0206-001 wildcard detector scoped to `lib.rs` only — migrations.rs becomes backdoor.
- 0206-002 AC: `cargo build --workspace --all-targets` requires adapter crates (0206-004) — not in `depends_on:`.
- 0206-004 AC: "5 directory existence check green" without reproducing `test -d` command per RFC TV-0206-A6.

**Density:** 4 of 7 missions. Root cause: ACs drafted without referencing RFC §Test Vectors exactly.

### Class 6 — Missing RFC-0205 v2.0 Acceptance Cross-Reference

Per BLUEPRINT.md §Dependency Validation Rules → 2-Cycle Atomic Promotion (rule 5, amendment filed in v2.0 batch), 0205/0206 RFCs are coupled pair; mission `depends_on:` should reflect:

- 0205-001: implicit (Phase 0.1 is acceptance precondition)
- 0205-002: implicit (Phase 1 is acceptance precondition)
- 0206-001: cites 0205-002 dep but not RFC-0205 v2.0 directly
- 0206-002: missing entirely
- 0206-004: missing entirely

**Density:** 3 of 7 missions omit direct RFC-0205 v2.0 citation.

### Class 7 — Undefined or Fabricated Types in Scope

Mission references types not defined in RFC or scope:

- 0206-001: `SubstrateError::AdapterIdNotRegistered { id }` variant not in RFC v2.0 §Substrate Newtype Refactor.
- 0206-001: `AdapterId` type referenced in `execute_checked` signature, not in 11-item re-export set, not in scope.
- 0206-001: `DdlOperation` and `Result` types referenced, not described in scope.
- 0205-002: `mode = "local-pin"` for `external-root-config.toml` — fabricated field; RFC §HW Key Custody §Quorum does not define `mode`.

**Density:** 3 of 7 missions. Root cause: types invented during mission drafting without RFC grounding.

### Class 8 — AC Forward-Requirement (Cannot Pass at Mission Close)

TV gates cited that require work after mission completion:

- 0205-001 AC #5: TV-0205-05 conditioned "after first freeze tag ceremony lands" — but freeze ceremony is Phase 1.3+ (after Phase 0.1).
- 0205-002 AC: TV-0205-05, -07, -21, -22, -23, -24 require freeze tag or fork SHA-256 object format — all forward-requirement.
- 0206-004: TV-0206-A8 (HolderRegistry declaration) owned by 0206-003 — mission cites but doesn't own.

**Density:** 3 of 7 missions. Root cause: missions cite ALL TV gates for completeness without filtering by ownership.

## R1 Decision: Wholesale v2.0 Rewrite Authorized

Per /goal "if you need a redesign 2.0 go for it" + RFC v2.0 wholesale-rewrite precedent:

- 13 CRIT across 7 missions (avg 1.9 CRIT/mission)
- 5 of 7 missions carry ≥2 CRITs
- Cross-cutting defect classes affect ALL 7 missions (no mission survives R1 clean except 0206-002 which has 0 CRITs)
- 4 scope conflicts between sibling missions require ownership-matrix resolution (not patch-fixable)
- 1 missing-mission dependency (`0206-cipherocto-policy-rename-alignment`) requires filing new mission
- Incremental fix would touch 112+ sites with high cascade risk; wholesale rewrite trades more upfront work for cleaner DAG

Pattern matches RFC v2.0 triggers (7+ structural defects → wholesale rewrite). **Decision: file mission v2.0 set, rewrite all 7 missions.**

## R1 Closure Conditions

- [x] All 7 reviewers returned
- [x] Severity tally tabulated
- [x] Cross-cutting classes extracted (8 classes)
- [x] Wholesale v2.0 rewrite decision documented
- [ ] v2.0 mission rewrite applied (next phase)
- [ ] R2 verification dispatched (after R1 fixes land)

## Cross-References

- `docs/audits/rfc-0205-0206-r9-findings-2026-08-20.md` — RFC v2.0 wholesale rewrite precedent (R9 methodology baseline)
- RFC-0205 v2.0 §Promotion Path Condition 3 (R10 reviewer pass — owned by `0205-003`)
- RFC-0206 v2.0 §Promotion Path Condition 3
- BLUEPRINT.md §Dependency Validation Rules → 2-Cycle Atomic Promotion (rule 5)

## Version History

| Version | Date       | Change                                                                       |
| ------- | ---------- | ---------------------------------------------------------------------------- |
| v1.0    | 2026-08-20 | R1 aggregate for 7 missions; 112 findings; wholesale v2.0 rewrite authorized |
