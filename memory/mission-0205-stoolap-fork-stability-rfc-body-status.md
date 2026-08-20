---
name: mission-0205-stoolap-fork-stability-rfc-body-status
description: LANDED 2026-08-19; RFC-0205 Stoolap fork stability certification Draft v1.3 (S7 NEW RFC 1/2); two-tier split (octo-stoolap-frozen Layer A + active fork Layer B); R1 + R2 + R3 review fixes landed
metadata:
  node_type: memory
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  modified: 2026-08-19T23:30:00.000Z
---

# Mission `0205-stoolap-fork-stability-rfc-body` — LANDED 2026-08-19

## What

Filed S7 NEW RFC body for RFC-0205 (Storage): Stoolap Fork Stability
Certification. Closes review §8.1.7 HIGH blocker.

## Substrate filed

- `rfcs/draft/storage/0205-stoolap-fork-stability.md` (290+ lines, 32 sections)
- v1.0 → v1.1 (Round 1) → v1.2 (Round 2) → v1.3 (Round 3) review fixes
- Two-tier split: Layer A frozen (workspace `[patch.crates-io]` redirect
  `stoolap = { git = "...", rev = "<sha>" }`, `octo-storage-core` sole consumer
  per `crates/octo-storage-core/Cargo.toml`; R3 corrected prior v1.2 claim
  that `octo-determin` was the sole consumer — verified `determin/Cargo.toml`
  has zero stoolap deps) + Layer B active fork (`branch = "feat/blockchain-sql"`,
  `octo-storage` + downstream)
- §Release-Tag Pin Policy: 6-row trigger/action/owner/SLA table; monthly
  trigger = 30 days from last freeze tag (deterministic sliding window);
  pre-v0 bootstrap = daily-watch on `feat/blockchain-sql` with 24 h RFC
  reviewer escalation (R2 added)
- §Operation Class Mapping (renamed from "RFC-0008 Execution Class Mapping")
- §Implicit Assumptions Audit: 4 entries (Q1 mitigation: HW key)
- §Adversary Analysis: 3-row decision table (Q4: out-of-band HW key)
- §Test Vectors: 6 governance TV (TV-0205-05: ancestor-of + tag-match
  `git rev-parse <tag> == <rev>` byte-equal — R2 added tag-match check
  to defend against force-push retargeting)
- §Alternatives Considered: 4 options; Option D (two-tier) adopted
- §Future Work: 2 phantom missions flagged `to be filed` (backticked R2)
- §Two-Tier Architecture: Mermaid graph TD with subgraph Layer A/B
- §Key Files to Modify: runbook content checklist enumerated R2
  (pre-v0 daily-watch / freeze signing / CVE bypass audit-log format /
  merge-base + tag-match CI gate / quarterly review template)

## Commits

- `75868942` — feat(0205): RFC-0205 Draft v1.0
- `8d86835b` — chore(missions): drift-close mission YAML
- `c782dec8` — fix(0205): R1 fixes (phantom refs + line refs + Mermaid)
- `ba78fefe` — fix(0205): R2 fixes (bootstrap clause + tag-match check +
  runbook checklist + backticks)

## Round 1 review fixes (10 defects)

| Severity | Defect                                                       | Fix                                                   |
| -------- | ------------------------------------------------------------ | ----------------------------------------------------- |
| CRIT     | Memory claimed "8 ACs PASS" — governance RFC has no AC table | Corrected memory                                      |
| HIGH     | `Cargo.toml:156` line ref in prose                           | → `Cargo.toml` `[patch.crates-io]` block              |
| HIGH     | Phantom `RFC-0001`/`RFC-0008`                                | → `BLUEPRINT.md` ref + inline Operation Class Mapping |
| MED      | Roles Source/Ref vague                                       | → precise §names                                      |
| MED      | Two-Tier ASCII                                               | → Mermaid graph TD                                    |
| MED      | Monthly re-cert trigger ambiguous                            | → "30 days from last freeze tag"                      |
| MED      | TV-0205-05 "reachable from" undefined                        | → "ancestor of" + `git merge-base --is-ancestor`      |
| MED      | TV-0205-06 script phantom                                    | → marked NEW per Phase 1 Task 4                       |
| MED      | Adversary Q1 mitigation weak                                 | → out-of-band HW key held by separate person          |
| MED      | Layer self-declaration missing                               | → added to Status                                     |
| MED      | Future Work phantom pointers                                 | → marked `to be filed`                                |

## Round 2 review fixes (4 defects)

| Severity | Defect                                               | Fix                                                                                                                                              |
| -------- | ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| MED      | Pre-v0 re-cert schedule undefined                    | → bootstrap clause: daily-watch on `feat/blockchain-sql` with 24 h RFC-reviewer escalation; SLA column split                                     |
| MED      | TV-0205-05 force-push retargeting bypass             | → tag-match check `git rev-parse <tag> == <rev>` byte-equal added alongside `merge-base --is-ancestor`                                           |
| MED      | `docs/runbooks/stoolap-steward.md` content undefined | → enumerated checklist (pre-v0 daily-watch / freeze signing / CVE bypass audit-log format / merge-base + tag-match CI gate / quarterly template) |
| MED      | Future Work `**to be filed**` bolded                 | → backticked `\`to be filed\``                                                                                                                   |

## Round 3 review fixes (deep-dive CRITICAL+HIGH+MED)

| Severity | Defect                                                                                                              | Fix                                                                                                                                                                                                                                                                                                                                                                                         |
| -------- | ------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| CRIT     | Consumer misidentification: RFC-0205 v1.2 named `octo-determin` as sole Layer A consumer                            | Verified via `crates/octo-storage-core/Cargo.toml` (sole `stoolap` dep in tree) + `determin/Cargo.toml` (zero stoolap deps). Wholesale rewrite: §Summary, §Motivation, §Roles, §Two-Tier Architecture Mermaid, §Implicit Assumptions row 3, §Security Considerations Layer-A-drift row, §Operation Class Mapping row 1, §Compatibility Backward — all `octo-determin` → `octo-storage-core` |
| CRIT     | §Cargo.toml Pinning Layer A template used nonexistent `[dependencies.octo-stoolap-frozen]`                          | Verified fork repo `Cargo.toml` declares `name = "stoolap"` (upstream name); no `octo-stoolap-frozen` crate exists. Template rewritten to actual mechanism: workspace `[patch.crates-io] stoolap = { git = "...", rev = "<sha>" }` redirect                                                                                                                                                 |
| HIGH     | §Release-Tag Pin Policy missing CRITICAL CVE mid-cycle trigger                                                      | New row: ice-until-patch semantics; v{N+1} bump RFC + TV-0205-05 gate; `octo-storage-core` auto-picks up via `[patch.crates-io]` redirect; SLA 7 days                                                                                                                                                                                                                                       |
| HIGH     | §Adversary Row 1 Q4 didn't cite TV-0205-05 explicitly                                                               | Q4 Defense now cites TV-0205-05 (`git merge-base --is-ancestor` + `git rev-parse <tag> == <rev>` byte-equal)                                                                                                                                                                                                                                                                                |
| MED      | §Operation Class Mapping didn't restate the sole-consumer invariant                                                 | Row 1 updated: "Layer A substrate; years-stable; sole consumer is `octo-storage-core`"; new row 5 added for CRITICAL CVE mid-cycle bump (Class A)                                                                                                                                                                                                                                           |
| MED      | §Test Vectors §General rule + all 6 entries still referenced `octo-stoolap-frozen` crate + `octo-determin` consumer | Rewrote §General rule + TV-0205-01..05 to reference `[patch.crates-io]` redirect + `octo-storage-core` + backward slug path; TV-0205-02 references `crates/octo-storage-core/Cargo.toml`                                                                                                                                                                                                    |
| MED      | §Implementation Phases Phase 1 Task 2 said `octo-determin/Cargo.toml`                                               | Updated to "workspace `Cargo.toml` `[patch.crates-io]` block edit"                                                                                                                                                                                                                                                                                                                          |
| MED      | §Key Files to Modify referenced `crates/octo-determin/Cargo.toml` (nonexistent path)                                | Replaced with `crates/octo-storage-core/Cargo.toml` row (no change — documents why)                                                                                                                                                                                                                                                                                                         |
| MED      | §Compatibility Backward had post-RFC-0205 `octo-stoolap-frozen` (stale ref)                                         | Rewrote to "post-RFC-0205 workspace `[patch.crates-io]` frozen redirect (the upstream `stoolap` crate is patched to the fork rev at build time)"                                                                                                                                                                                                                                            |
| MED      | §Compatibility Forward didn't address drop-fork scenario                                                            | Added clause: Layer B branch pin → crates-io semver migration per §Future Work                                                                                                                                                                                                                                                                                                              |

## S7 NEW RFC closure

Both S7 NEW RFC bodies filed (2/2):

- `rfcs/draft/storage/0205-stoolap-fork-stability.md` — LANDED
- `rfcs/draft/storage/0206-octo-storage-split.md` — LANDED

S7 NEW RFC gap = **CLOSED**.

## Verification

- Prettier + cargo fmt --all applied
- Cross-references to review §8.1.7 verified at 4 sites
- `cargo clippy --all-targets --features full -- -D warnings` clean
- 4 R1 reviewers + 4 R2 reviewers; loop DRY pending Round 3

## Out of scope

- Cargo.toml actual SHA pin (Phase 1 Task 1)
- CI graph audit gate script (Phase 1 Task 3)
- `scripts/stoolap_recert.sh` (Phase 1 Task 4)
- Merge-to-upstream sub-mission (Phase 2 Task 5; deferred to follow-on RFC)

## Related

- [[mission-0862-c10b-rfc-version-pin-sweep-v2-status]] — sibling R3 fix
- [[storage-restructure-plan-audit-2026-08-19]] — plan §10 reconciliation
- [[mission-0206-octo-storage-split-rfc-body-status]] — sibling S7 NEW RFC 2/2
