---
name: storage-restructuring-plan-active
description: Active plan as of 2026-08-16 — 14-RFC storage restructure, 4 streams / 8 sessions, S1+S2 LANDED, S3 next
metadata:
  type: project
---

# Active plan: 14-RFC storage layer restructure

**Source docs:**

- Review (2210 lines, R33 zero-new STOP): `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
- Execution plan (228 lines): `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`

**Status 2026-08-17 (drift corrected from 2026-08-16 status card):**

- S1 (Stream B.1, Stoolap audit) — LANDED (mission `stoolap-fork-stability-audit`)
- S2 (Stream B.2, octo-storage split) — LANDED (commits `fb98cf08`..`003f3a45` + R2-R10 = `e89acf4d`..`9990c026` + R11 = `643ba82d` zero new)
- S3 (Stream B.3, octo-vault crate + 14 TV) — LANDED
- S4 (Stream C.1, Dqa codemod) — LANDED (commits `19faf380` + `4ab400bd` + `18edbe0d`)
- S5 (Stream C.2, verify-time invariant) — LANDED (commit `d007de54`)
- S6a (RFC-0870 + 1 TV) — LANDED (commits `c7f99a47` + `ab2b57b4` + Round 2 fix)
- S6b (RFC-0957 + 22 TV) — LANDED (commits `c9149128` + `4ec9779f` + `e5138420` + `57676533` Round 6)
- S6c (RFC-0862 + 8 TV) — **next**
- S6d..S6g, S7, S8 — pending

**Session flow per plan §3:**

| Session | Stream | Pre-req  | Output                                                                                                                                    |
| ------- | ------ | -------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| S1      | B.1    | —        | Stoolap audit (DONE)                                                                                                                      |
| S2      | B.2    | —        | octo-storage split (DONE)                                                                                                                 |
| S3      | B.3    | S2       | octo-vault + 14 TV                                                                                                                        |
| S4      | C.1    | S2       | Dqa wire-format codemod                                                                                                                   |
| S5      | C.2    | S3,S4    | verify-time invariant + WrappedOnly                                                                                                       |
| S6      | A.1    | S3,S4,S5 | 7 B0 RFC amendments + 283 TV (atomic-blocker bundle per §22; S6a/S6b LANDED; user split-by-RFC overrides atomic-blocker for sub-sessions) |
| S7      | A.2    | S2,S6    | 5 B1/B2 amendments + 2 NEW RFCs + 56 TV                                                                                                   |
| S8      | D.1    | S6,S7    | 24 mission closures + PR bundle staged                                                                                                    |

**Critical rules from plan:**

- §22 B0 atomic-blocker: 7 B0 RFCs land together in S6
- §23 STAGE-1/STAGE-2 mission split; verify at claim time
- §8.5.1 fixture blast radius 301 sites across 12 test files
- §24 per-RFC TV count sum = 228, §1 cites 214 — R4-F1 doc-bug, reconcile at first session claim
- Push + remote writes user-only (per `feedback_initiative_user_only`)
- `docs/reviews/` + `docs/plans/` are local scratchpads, never committed (`docs-reviews-temporary` + `docs-plans-scratchpad`)

**Why:** User directive 2026-08-16 "stick with the plan, abandon tv fixture". Prior session drift to `0862-phase1-tv-fixture` was caused by compaction losing the plan context. Plan supersedes any individual mission's "next" appeal.

**How to apply:** Any `/compact`, drift, or "what's next?" — re-read `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md` §3 (session table) and verify which S# is currently active before claiming any mission. Do NOT pick missions like `0862-phase1-tv-fixture` as "next pick" without first checking plan §3 — they may be S6/S7/S8 scope, not standalone.

**Open missions tied to chain (plan §1):** `0862-phase1-tv-fixture`, `0105-dqa-literal-syntax`, `stoolap-fork-stability-audit` (latter filed and DONE 2026-08-16 per S1).
