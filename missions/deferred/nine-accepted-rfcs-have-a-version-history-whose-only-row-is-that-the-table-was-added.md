---
name: nine-accepted-rfcs-have-a-version-history-whose-only-row-is-that-the-table-was-added
description: "lint_no_vh_accepted.sh requires every Accepted RFC to carry a Version History table, and checks only that the table exists. M37, a corpus-wide sync referenced by 23 files, satisfied it by adding tables to RFCs that lacked them. Nine Accepted RFCs now have a Version History whose only row reads 'Retroactive VH table addition', and six more carry that note as a second row. The tables record their own creation rather than the document's versions. The note is honest about being retroactive, which is better than inventing history, but the guard it satisfies cannot tell the difference. This is the mechanism behind the corpus converging on full VH coverage, and it explains why the RFC-0955-R1 label drift survived the lint."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.0"
  depends_on:
    - lint_no_vh_accepted.sh
---

# Mission `nine-accepted-rfcs-have-a-version-history-whose-only-row-is-that-the-table-was-added`

## Status

Deferred. Content decisions about nine Accepted RFCs' history tables, and a
tooling question about what the guard should actually assert. Nothing was
actioned.

## Finding

`scripts/lint_no_vh_accepted.sh` requires every Accepted RFC to carry a
`## Version History` table. Its own documented behaviour is narrow: _"verify
presence of `## Version History` table. Reject RFC promotion to Accepted without
VH."_

Sixteen Accepted RFCs now carry a Version History row that describes the table's
own creation rather than any version the document went through.

| Shape                                            | Count        |
| ------------------------------------------------ | ------------ |
| Table whose **only** row is the retroactive note | **9**        |
| Two rows: a real `1.0` plus the retroactive note | 6            |
| Full history plus a trailing retroactive note    | 1 (RFC-0114) |

The nine, all of which have a Version History section containing exactly one
data row:

- `economics/0932-gateway-auth-api-key-management`
- `economics/0933-rate-limiting-integration`
- `economics/0934-budget-management-spend-tracking`
- `economics/0935-secret-manager-integration`
- `economics/0936-pre-call-checks`
- `economics/0937-prometheus-metrics-endpoint`
- `economics/0938-yaml-interpolation-universal-key`
- `numeric/0102-wallet-cryptography`
- `numeric/0104-deterministic-floating-point`

A representative table in full:

```
| Version | Date       | Change                                                        |
| ------- | ---------- | ------------------------------------------------------------- |
| 1.0     | 2026-08-22 | Retroactive VH table addition (per long-horizon plan v1.3 ...  |
```

**That table tells a reader nothing about RFC-0937's versions.** It records that
on 2026-08-22 someone added a table. `lint_no_vh_accepted.sh` passes it,
because the table exists, has a header, and has a row.

## M37 is the mechanism, and it is larger than previously recorded

The retroactive rows all name their provenance:

> Retroactive VH table addition (per long-horizon plan v1.3 Phase 1 + Option C
> per M37).

**M37 is referenced by 23 files** across `rfcs/`, and it does two distinct
things:

| Role                                               | Files | Example                                                                                |
| -------------------------------------------------- | ----- | -------------------------------------------------------------------------------------- |
| Retroactive Version History table addition         | 16    | `                                                                                      | 1.0 | 2026-08-22 | Retroactive VH table addition ...` |
| Status header synced against a Version History row | 7     | `Accepted v2.1 — Status header synced with §Version History v2.1-Resolved ... per M37` |

The second role is the one already recorded, and it is how the six companion
RFCs of RFC-0960 ended up citing M37 while RFC-0960 itself did not. This mission
adds the first role, which is the larger of the two and the one that produced the
corpus's full VH coverage.

So the picture is: a guard required Version History tables, and the repair was to
**add tables** rather than to **recover histories**. For sixteen documents that
is now visible in the corpus.

## Why the lint cannot see it

The guard asserts a structural property — _this file has a Version History
section_ — and stops there. Everything that would distinguish a real history from
a placeholder lives inside the table, where nothing looks:

- whether a row's Version cell names a version rather than an event
- whether the row's date is the document's date or the table's creation date
- whether the Change column describes a change to the _document_

A guard that wanted to catch this would have to assert something about table
**content**, which is a different and more opinionated check than the one written.
That is a tooling decision, and it is the owner's.

**This is the same blind spot as the RFC-0955-R1 label drift, one level up.**
That finding is a Version History that exists, parses, and disagrees with its
Status header. This is a Version History that exists, parses, and says nothing.
The guard passes both for the same reason.

## What the corpus did right, and should keep doing

**The rows are honest.** They say "Retroactive VH table addition". They do not
invent a version history, they do not claim a promotion that never happened, and
they cite the plan that authorised them. A fabricated row — a plausible v1.0 with
a tidy change description — would have been far more damaging and much harder to
distinguish from a real one.

The recorded repair of the six documents in
`six-accepted-rfcs-have-no-version-history-after-three-linter-fixes` took the
same shape and is worth comparing against: that mission states explicitly that
its two reconstructed tables declare no version and no promotion, and that the
dates come from `git log` rather than invention. **Sixteen more documents carry
the same disclosure, unremarked.**

That is the credit line. The gap is not dishonesty, it is that the honest
placeholder and a genuine history are indistinguishable to every tool in the
tree.

## Relationship to the recorded lint-fix work

`six-accepted-rfcs-have-no-version-history-after-three-linter-fixes` is
**Resolved** and covers six specific documents: four `## History` renames
(`0011-s/t/u/v`) and two additions (`0001-mission-lifecycle`,
`0002-agent-manifest`). **No overlap with the sixteen here.** Different
documents, different mechanism, different campaign. That mission fixed the six
it found and the guard now reports `PASS=155 FAIL=0 MISSING_VH=0`.

So the corpus genuinely converged. The sixteen are the residue of a second,
earlier campaign that added the tables rather than renaming them.

## What the owner should do

- **Decide whether a retroactive placeholder satisfies the guard.** If it does,
  the guard's name overstates what it checks. If it does not, sixteen Accepted
  RFCs need their histories written — which is real work and should be scheduled
  rather than assumed.
- **Rewrite the nine one-row Change cells to say what happened at v1.0**, using
  the document's own Submission Date. Most of these RFCs have one; a row reading
  "1.0, 2026-03-07, initial draft" alongside the existing retroactive note would
  cost one line each and make the table answer the question it exists to answer.
- **Do not delete the retroactive rows.** They are the accurate record of an
  editorial event, and they are what lets a reader distinguish a reconstructed
  history from a native one.
- **Consider whether the guard should assert anything about content.** The
  cheapest version: a Version History with zero rows that describe a document
  version is suspicious. That is heuristic and will produce false positives on
  legitimately unusual documents. **Proposing, not implementing.**
- **Leave the two-role M37 citation as-is**, but note that the plan it names —
  "long-horizon plan v1.3 Phase 1 + Option C" — is itself a gitignored
  `docs/plans/` artifact, so the justification for sixteen editorial changes is
  invisible to a fresh checkout. That is already recorded for the plan class.

## Reproduction

```bash
# CONTROL — the guard passes, and says what it checks
bash scripts/lint_no_vh_accepted.sh 2>&1 | tail -2
grep -n 'verify presence of' scripts/lint_no_vh_accepted.sh

# how far M37 actually reaches, in both of its roles.
# The two sets are DISJOINT: 16 + 7 = 23.
python3 - <<'ENUM'
import pathlib
retro, sync = [], []
for p in sorted(pathlib.Path('rfcs').rglob('*.md')):
    t = p.read_text(encoding='utf-8', errors='replace')
    if 'M37' not in t:
        continue
    if 'Retroactive VH table addition' in t:
        retro.append(p.name)
    elif 'Status header synced' in t:
        sync.append(p.name)
print("M37 files:", len(retro) + len(sync))
print("  retroactive VH row only:", len(retro))
print("  status-header sync only:", len(sync))
for x in sync:
    print("      ", x)
ENUM
# 23 = 16 + 7. The 7 are the five RFC-0960 companions (0961, 0962, 0964,
# 0965, 0967) plus 0862-writer-election-bootstrap-v130 and 0008. RFC-0960
# itself is absent from both sets, which is the finding recorded in
# rfc-0960-pins-six-companion-versions-and-every-one-is-behind.

# the nine one-row tables
python3 - <<'ENUM'
import re, pathlib

one, two, many = [], [], []
for p in sorted(pathlib.Path('rfcs/accepted').rglob('*.md')):
    lines = p.read_text(encoding='utf-8', errors='replace').splitlines()
    if 'Retroactive VH table addition' not in "\n".join(lines):
        continue
    rows, in_vh = [], False
    for ln in lines:
        if re.match(r'^#+\s*Version History', ln):
            in_vh = True
            continue
        if in_vh and re.match(r'^#+\s', ln):
            break
        if in_vh and ln.strip().startswith('|'):
            rows.append(ln)
    n = len([r for r in rows[2:] if '|' in r])
    (one if n == 1 else two if n == 2 else many).append(p.name)

print("only row is the retroactive note:", len(one))
for x in one:
    print("   ", x)
print("two rows:", len(two))
print("full history plus note:", len(many))
ENUM

# a representative table, in full
grep -n -A6 '^## Version History' \
  rfcs/accepted/economics/0937-prometheus-metrics-endpoint.md

# ...and the guard is happy with it
bash scripts/lint_no_vh_accepted.sh 2>&1 | grep -c FAIL   # 0

# no overlap with the six the resolved mission repaired
grep -rliE 'Retroactive VH|M37' missions/ | head
# only coordination docs and the RFC-0960 companion-pin mission
```

## Cross-references

- Mission `six-accepted-rfcs-have-no-version-history-after-three-linter-fixes` —
  the same guard, six different documents, and the resolved repair this one sits
  beside
- Mission `rfc-0955-calls-its-accepted-sibling-draft-and-gates-phase-5-on-a-promotion-already-made`
  — the same guard passing a Version History that contradicts its Status header;
  this is the same blindness one level up
- Mission `rfc-0960-pins-six-companion-versions-and-every-one-is-behind` — the
  M37 **status-sync** role, and the earlier, narrower reading of M37's reach that
  this mission corrects
- Mission `fifteen-accepted-rfcs-chain-their-amendments-to-a-gitignored-plan` —
  the gitignored `docs/plans/` justification named in the retroactive rows
- Mission `documentation-that-describes-a-mechanism-nobody-built` — the adjacent
  class; this one describes a table nobody checks the contents of
