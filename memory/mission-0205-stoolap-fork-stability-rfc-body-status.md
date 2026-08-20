---
name: mission-0205-stoolap-fork-stability-rfc-body-status
description: LANDED 2026-08-19; RFC-0205 Stoolap fork stability certification Draft v1.1 (S7 NEW RFC); two-tier split (octo-stoolap-frozen Layer A + active fork Layer B); 280+ lines; R1 review fixes landed
metadata:
  node_type: memory
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  modified: 2026-08-20T00:44:05.183Z
---

# Mission `0205-stoolap-fork-stability-rfc-body` — LANDED 2026-08-19

## What

Filed S7 NEW RFC body for RFC-0205 (Storage): Stoolap Fork Stability
Certification. Closes review §8.1.7 HIGH blocker.

## Substrate filed

- `rfcs/draft/storage/0205-stoolap-fork-stability.md` (290+ lines, 32 sections)
- v1.0 → v1.1 Round 1 review fixes (commit `c782dec8` 2026-08-19)
- Two-tier split: Layer A frozen (`octo-stoolap-frozen` pinned SHA,
  `octo-determin` only) + Layer B active fork (`branch = "feat/blockchain-sql"`,
  `octo-storage` + downstream)
- §Release-Tag Pin Policy: 6-row trigger/action/owner/SLA table (monthly
  trigger clarified: 30 days from last freeze tag)
- §Operation Class Mapping (renamed from "RFC-0008 Execution Class
  Mapping" — phantom RFC-0008 removed)
- §Implicit Assumptions Audit: 4 entries (Q1 mitigation updated: HW key)
- §Adversary Analysis: 3-row decision table (Q4 defense: out-of-band HW key)
- §Test Vectors: 6 governance TV (TV-0205-05: ancestor-of + git merge-base;
  TV-0205-06: script NEW per Phase 1 Task 4)
- §Alternatives Considered: 4 options; Option D (two-tier) adopted
- §Future Work: 2 phantom missions now flagged **to be filed**
- §Two-Tier Architecture: ASCII → Mermaid graph TD with subgraph Layer A/B

## Commits

- `75868942` — feat(0205): RFC-0205 Draft v1.0 (270 lines)
- `8d86835b` — chore(missions): drift-close mission YAML to claimed/
- `c782dec8` — fix(0205): round 1 review fixes — phantom refs + line refs + Mermaid

## Round 1 review fixes (10 defects)

| Severity | Defect | Fix |
| -------- | ------ | --- |
| CRIT | Memory claimed "8 ACs PASS" — RFC has 0 AC table (governance RFC, ACs N/A) | Corrected memory; AC table not applicable for governance RFC |
| HIGH | `Cargo.toml:156` line ref in prose | → `Cargo.toml` `[patch.crates-io]` block |
| HIGH | Phantom RFC-0001 / RFC-0008 in role table + Operation Class Mapping | → `BLUEPRINT.md` ref + inline Operation Class Mapping table |
| MED | Roles table Source/Ref vague | → precise §Two-Tier Architecture / §Release-Tag Pin Policy |
| MED | Two-Tier ASCII text | → Mermaid graph TD with subgraph Layer A/B |
| MED | "Monthly re-cert" trigger ambiguous | → "30 days from last freeze tag (deterministic sliding window)" |
| MED | TV-0205-05 "reachable from" undefined | → "ancestor of" + `git merge-base --is-ancestor` |
| MED | TV-0205-06 `scripts/stoolap_recert.sh` phantom | → marked NEW per Phase 1 Task 4 |
| MED | Adversary Q1 "RFC reviewer co-sign" doesn't defend steward compromise | → out-of-band HW key held by separate person |
| MED | Layer self-declaration missing | → added **Layer:** B to Status block |
| MED | Future Work phantom mission pointers | → marked **to be filed** |

## Parent

- `missions/claimed/stoolap-fork-stability-audit.md` (LANDED 2026-08-16;
  11 ACs PASS; pin HOLD recommendation)

## Verification

- Prettier formatting applied (post-edit + post-review-fix)
- Cross-references to review §8.1.7 verified at 4 sites (valid)
- `cargo clippy --all-targets --features full -- -D warnings` clean
  (workspace-wide; no RFC-introduced warnings)
- Round 1 reviewers: 4 (correctness / cross-RFC / process-compliance) +
  1 (RFC-0205-specific); loop DRY pending Round 2

## Out of scope

- Cargo.toml actual SHA pin (Phase 1 Task 1 — separate work)
- CI graph audit gate script (Phase 1 Task 3)
- `scripts/stoolap_recert.sh` (Phase 1 Task 4)
- Merge-to-upstream sub-mission (Phase 2 Task 5; deferred per
  §Future Work to follow-on RFC)

## Related

- [[mission-0862-c10b-rfc-version-pin-sweep-v2-status]] — sibling
  Round-3 R1 fix
- [[storage-restructure-plan-audit-2026-08-19]] — plan §10 reconciliation
  sibling; S7 NEW RFC gap closure
- [[mission-0206-octo-storage-split-rfc-body-status]] — sibling S7 NEW RFC 2/2
