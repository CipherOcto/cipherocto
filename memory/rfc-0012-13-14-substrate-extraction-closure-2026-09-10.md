---
name: rfc-0012-13-14-substrate-extraction-closure-2026-09-10
description: 0012/0013/0014 substrate extraction — 6 new crates + DRY CLOSED 2026-09-10
metadata:
  type: project
---

3 substrate-extraction missions landed per /goal "address those 3 missions, upon finish start a multi round code review with loop until dry pattern":

## 6 new crates (3 Layer A frozen cores + 3 Layer B façades)

| RFC | Substrate (Layer A frozen) | Façade (Layer B) |
|---|---|---|
| RFC-0012 | octo-audit-core | octo-audit |
| RFC-0013 | octo-governance-core | octo-governance |
| RFC-0014 | octo-settlement-core | octo-settlement |

Deps per core: `blake3` + `serde` + `thiserror` ONLY (Layer A primitives). Façades depend only on the matching `-core` crate.

## Commits on `next`

- `426dff81` — feat(octo-audit) RFC-0012 substrate + façade (9 files, 464 insertions)
- `3aff437d` — feat(octo-governance) RFC-0013 substrate + façade (8 files, 472 insertions)
- `b8e0e454` — feat(octo-settlement) RFC-0014 substrate + façade (11 files, 584 insertions)
- `a2932181` — fix(octo-governance-core) R2 DRY review fix (drop dead Caller variant + semantic QuorumNotReached fix)

## 5-len DRY review — DRY CLOSED

R1 = 2 LOW findings (MINOR-1 dead `Caller` variant; MINOR-2 wrong error variant for total-vote-exceeds-100k). R2 fixes applied. R3 = 0 findings. R2 + R3 = 2 consecutive zero-finding rounds = DRY CLOSED.

## Build / test / lint summary

20/20 unit tests pass (6 audit + 8 governance + 6 settlement). Clippy 0 warnings under `--all-targets --all-features -- -D warnings`. Workspace build with `--all-features` fails on pre-existing octo-reputation `parity` module issue (unrelated to substrate extraction; known [[quota-router-core-feature-mutex]]).

## Cross-RFC invariants preserved

- `GovernanceModel` (5 variants) + `EmergencyAuthority` (3 variants) + `ProposalState` (6 variants) + `DecisionType` (7 variants) `repr(u16)` per RFC-0855 §11.1-11.3
- `AskState` (3 variants) `repr(u8)` per RFC-0959 §State Machine
- `ReservationState` (8 variants) `repr(u8)` per RFC-0960 §2.3
- Domain separator `cipherocto/reservation/v1/` byte-pinned via `domain_separator_pinned` unit test
- PQC migration blast radius confined to Layer A frozen cores

## User-owned follow-ons (NOT agent tasks per [[git-workflow]] + [[feedback_initiation_user_only]])

- `git push origin next` + PR `next → main`
- RFC VH row append for RFC-0012/0013/0014 per mission AC-8
- Mission YAML transition `Claimed → Completed`
- Cite-hygiene pass per mission VH requirement
- Follow-on mission kickoffs (storage adapter impls: `0012-audit-stoolap-sink`, `0013-governance-network-migration`, `0014-settlement-sm-engine-migration`)

## Why

Mission implementation per established layer model pattern; closure audit doc `docs/audits/2026-09-10-0012-0013-0014-substrate-extraction-r3-dry-closure.md` documents the full DRY trail.

## How to apply

Use this card as a reference for future substrate-extraction missions: same pattern (Layer A core + Layer B façade + curated re-export + storage adapter at DOMAIN layer + DRY 5-len review loop). Per [[memory-is-never-status-ground-truth]] this card is NOT ground truth; verify via git/cargo in the same turn.

Related: [[rfc-0012-13-14-0011a-0011g-dry-closure-2026-09-10]]
