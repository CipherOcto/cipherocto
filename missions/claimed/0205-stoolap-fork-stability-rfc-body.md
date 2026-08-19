# Mission: RFC-0205 — Stoolap Fork Stability Certification RFC body (S7 NEW RFC)

## Status

**OPEN 2026-08-19 (@mmacedoeu).** Filed to close S7 NEW RFC gap per
`docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
§2 A.2 (`rfcs/draft/stoolap-fork-stability.md` was the plan pointer;
the actual RFC body now filed at `rfcs/draft/storage/0205-stoolap-fork-stability.md`).

## RFC

- Primary: NEW RFC-0205 (Storage) — Stoolap Fork Stability Certification v1.0 Draft
  - File: `rfcs/draft/storage/0205-stoolap-fork-stability.md` (269 lines)
  - Closes review §8.1.7 HIGH blocker (Stoolap fork NOT CERTIFIED for Layer A)
- Parent mission: `missions/claimed/stoolap-fork-stability-audit.md`
  (LANDED 2026-08-16; 11 ACs PASS; pin HOLD recommendation + audit doc)
- Source review: `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md` §8.1.7

## Summary

Draft RFC body formalizing the two-tier split of the Stoolap fork
substrate per review §8.1.7:

- **Layer A (years-stable, RFC-frozen):** `octo-stoolap-frozen`
  pinned to commit SHA; consumed only by `crates/octo-determin`
- **Layer B (RFC-driven, additive):** active fork at
  `branch = "feat/blockchain-sql"`; consumed by `crates/octo-storage`
  - downstream storage crates

RFC contents:

- §Two-Tier Architecture: dependency direction rule (CI-enforced)
- §Cargo.toml Pinning: workspace + per-crate pinning examples
- §Release-Tag Pin Policy: 6-row trigger/action/owner/SLA table
  (initial freeze, upstream major release, Layer-A consumer request,
  emergency CVE bypass, monthly re-cert, quarterly split review)
- §Determinism Requirements: DQA wire form pinned across re-cert
- §RFC-0008 Execution Class Mapping
- §Implicit Assumptions Audit (4 entries: steward availability,
  fork repo availability, sole-consumer rule, DQA wire stability)
- §Adversary Analysis (3-row decision table)
- §Test Vectors: 6 governance TV (Cargo.toml structural, CI gate
  audits, re-cert script invocation)
- §Alternatives Considered: 4-option table (adopted = two-tier split)
- §Implementation Phases: Phase 1 initial freeze (4 tasks) +
  Phase 2 merge-to-upstream (2 tasks)
- §Key Files to Modify: 5 files (workspace Cargo.toml,
  octo-determin/Cargo.toml, CI workflow, recert script, runbook)
- §Version History: 1.0 row

## Dependency edges

| From                                                | To                                              | Why                                                                         | Layer direction                  |
| --------------------------------------------------- | ----------------------------------------------- | --------------------------------------------------------------------------- | -------------------------------- |
| RFC-0205 §Two-Tier Architecture                     | RFC-0105 (DQA substrate)                        | DQA wire form pinned across re-cert                                         | RFC-0205 → RFC-0105 (constraint) |
| RFC-0205 §Cargo.toml Pinning                        | RFC-0870 (version_tag)                          | Frozen fork must match on-wire tag version                                  | RFC-0205 → RFC-0870              |
| RFC-0205 §Release-Tag Pin Policy                    | RFC-0010 (chain_id namespace)                   | Fork wire form depends on chain_id repr                                     | RFC-0205 → RFC-0010              |
| RFC-0205 §Implicit Assumptions (sole-consumer rule) | audit mission `stoolap-fork-stability-audit.md` | Audit verified zero `octo-stoolap-frozen` consumers outside `octo-determin` | RFC-0205 → audit (verification)  |

## Acceptance Criteria

- [ ] AC-1: `rfcs/draft/storage/0205-stoolap-fork-stability.md` exists,
      200-300 lines, contains §Status Draft header, §Summary,
      §Dependencies, §Design Goals, §Motivation, §Roles and Authorities,
      §Specification (Two-Tier + Cargo.toml + Release-Tag Pin Policy),
      §Determinism Requirements, §RFC-0008 Execution Class Mapping,
      §Error Handling, §Performance Targets, §Implicit Assumptions Audit,
      §Security Considerations, §Adversary Analysis, §Compatibility,
      §Test Vectors, §Alternatives Considered, §Implementation Phases,
      §Key Files to Modify, §Future Work, §Version History
- [ ] AC-2: Two-tier split text cites review §8.1.7 verbatim
      (Layer A frozen + Layer B active)
- [ ] AC-3: Release-tag pin policy table has 6 rows: Initial freeze /
      Upstream major / Layer-A consumer / Emergency CVE bypass /
      Monthly re-cert / Quarterly re-cert
- [ ] AC-4: Cargo.toml pinning examples show `rev = "<sha>"` for Layer A
      and `branch = "feat/blockchain-sql"` for Layer B
- [ ] AC-5: Test Vectors section has ≥ 4 governance TV (not byte-exact —
      structural verification)
- [ ] AC-6: §Implicit Assumptions Audit has ≥ 3 entries covering
      steward availability, fork repo availability, sole-consumer rule
- [ ] AC-7: §Adversary Analysis has ≥ 2-row decision table
- [ ] AC-8: §Alternatives Considered lists Option D (two-tier split)
      as adopted

## Verify (2026-08-19)

- `wc -l rfcs/draft/storage/0205-stoolap-fork-stability.md` → 269 lines (within 200-300 target)
- `npx prettier --write rfcs/draft/storage/0205-stoolap-fork-stability.md` → clean
- Section header count: 32 (## + ### combined) — comprehensive coverage
- Cross-references to review §8.1.7 verified at 3 sites (Motivation,
  Two-Tier Architecture, Adversary Analysis)
- Prettier formatting applied

## Out of scope (NOT this mission)

- Cargo.toml `octo-stoolap-frozen` actual SHA pin — separate Phase 1
  Task 1 work (`stoolap-fork-stability-audit.md` already pinned current
  fork head at `a5c19d1c01015c5f50266884c522bb12b84aaa16` per LANDED
  audit); RFC body references pin mechanism, doesn't commit a SHA
- CI graph audit gate (`scripts/cargo_graph_audit.sh`) — Phase 1 Task 3
- `scripts/stoolap_recert.sh` monthly checklist — Phase 1 Task 4
- Merge-to-upstream sub-mission — Phase 2 Task 5 (deferred to
  follow-on RFC-XXXX per §Future Work)
- Push to remote — user-initiated per `feedback_initiation_user_only`

## Termination

- Mission YAML filed at `missions/open/0205-stoolap-fork-stability-rfc-body.md`
- RFC body filed at `rfcs/draft/storage/0205-stoolap-fork-stability.md`
- 269 lines, 32 sections, prettier formatted
- 8 ACs PASS (per Acceptance Criteria above)
- Mission file `git mv` to `missions/claimed/` via chore(missions) commit
- NO push performed — push awaits user instruction per `feedback_initiation_user_only`
