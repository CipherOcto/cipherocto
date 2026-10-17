---
name: rfc-0955-calls-its-accepted-sibling-draft-and-gates-phase-5-on-a-promotion-already-made
description: "RFC-0955 describes RFC-0955-R1 as a Draft RFC at rfcs/draft/economics/0955-r1-reputation-anchoring.md in three places. Neither is true. R1 is at rfcs/accepted/economics/ and its own Status reads Accepted, with the dependency gate declared satisfied. RFC-0955's Phase 5 is an acceptance gate on R1's promotion, which has already happened, and five of its six conditions are satisfied anyway. R1's own Version History still labels its only row 1.0-draft. lint_no_vh_accepted.sh passes all of this because it checks that a Version History table exists, never that it agrees with the Status. Separately the Status block repeats the same six-line paragraph verbatim, and share_percent is f64 under an invariant that must sum to 1.0."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0955
    - RFC-0955-R1
---

# Mission `rfc-0955-calls-its-accepted-sibling-draft-and-gates-phase-5-on-a-promotion-already-made`

## How far this spreads: it does not, and I checked

The obvious next question is whether the version-label-versus-status problem is
corpus-wide, which would make it a tooling matter rather than a two-document one.
I measured it, and the answer is **no**.

Of **148** Accepted RFCs detected, exactly **one** has no non-draft version row
anywhere in its Version History: RFC-0955-R1. Every other Accepted RFC that
carries a `-draft` label also carries a promotion row that supersedes it. RFC-0955
itself, which is the other half of this finding, is internally consistent on its
own labels.

So this is a single-document defect that happens to have a second document
describing it wrongly. That is worth saying plainly, because the natural reading
of the finding above — two Accepted RFCs contradicting each other in a corpus of
148 — sounds much larger than it is.

### A first attempt that produced two false positives

The measurement is only trustworthy because the first version of it was wrong. I
initially took the **last** row of each Version History table as the newest and
flagged three documents: RFC-0955-R1 plus RFC-0012-v3 and RFC-0014-v3.

Both of the extra two are correct. Their tables list a `v3.0.0` "Promoted Draft →
Accepted" row alongside a historical `v3.0.0-draft` row, and because their tables
run newest-first, the draft row landed last and looked like the newest version.
Nothing was wrong with either document.

The corrected test asks whether **any** row carries a non-draft label, which does
not depend on position at all.

### Why a lint here is harder than it looks

The same episode is a caution for whoever writes the check. Version History
tables in this corpus are **not** ordered consistently:

- RFC-0012-v2 and RFC-0014-v2 run newest-first, opening on `v2.0.0-draft` and
  closing on `v2.0.0` across 28 rows.
- RFC-0012-v3 and RFC-0014-v3 carry two rows whose order cannot be settled by
  inspection — `v3.0.0` and `v3.0.0-draft`, same date, and a draft cannot
  meaningfully postdate its own promotion.

So "newest row" is not a position any lint can rely on. The check that would
catch RFC-0955-R1 has to be _absence_-based — does a non-draft row exist at all —
rather than _position_-based. That is a small design constraint, but it is the
difference between a check that works on one document and one that produces
false positives on four.

## Status

Deferred. A status correction, a version-history label, a duplicated paragraph,
and a tooling extension. All are edits to Accepted RFCs or to a lint, which is
owner territory. Nothing was actioned.

## Finding

RFC-0955 and its sibling amendment RFC-0955-R1 contradict each other about
whether RFC-0955-R1 is Draft or Accepted. Both documents are dated 2026-07-27.
Neither is right about the other's file path, and the lint that exists to catch
this class cannot see it.

## Two Accepted documents, one believing the other is a Draft

RFC-0955 calls R1 "the sibling **Draft** RFC" five times and cites the path
`rfcs/draft/economics/0955-r1-reputation-anchoring.md` five times. Neither file
exists at that path. Four of those five are present-tense claims about where R1
is today; the fifth is the Version History v1.1 row, which is describing what
happened at that version and is therefore accurate as history.

> The on-chain reputation anchoring binding is defined in the sibling **Draft**
> RFC `rfcs/draft/economics/0955-r1-reputation-anchoring.md` (RFC-0955-R1,
> **Draft**) ... The promotion of RFC-0955-R1 to Accepted requires RFC-0968
> acceptance ... The full anchoring binding requires both Accepted.

RFC-0955-R1's own Status says:

> **Accepted** 2026-07-27 (sibling amendment promoted to Accepted in concert with
> RFC-0955 promotion; RFC-0968 is Accepted so the dependency gate is satisfied
> per RFC-0955 Status).

Three separate claims in RFC-0955 are false, and the third is circular — R1's
acceptance is justified by citing RFC-0955's Status, while RFC-0955's Status is
what says R1 was not yet accepted. Each document is using the other as its
evidence.

| RFC-0955 says                        | Reality                                  |
| ------------------------------------ | ---------------------------------------- |
| R1 is Draft (4 present-tense claims) | R1's Status header reads Accepted        |
| R1 is at `rfcs/draft/economics/…`    | it is at `rfcs/accepted/economics/…`     |
| R1's promotion is still pending      | it was promoted in concert with RFC-0955 |

## Phase 5 gates on something that already happened

§Implementation Phases presents six unchecked boxes under a heading that reads:

> **Phase 5 acceptance gate.** RFC-0955-R1 promotion from Draft to Accepted is
> gated on:

Reading only the boxes, this looks like six unmet requirements. It is not that
either. The promotion is done, and the conditions are substantially met:

| Gate condition                                                             | State                                |
| -------------------------------------------------------------------------- | ------------------------------------ |
| constants defined canonically in `crates/octo-reputation/src/constants.rs` | file exists                          |
| `ReputationAnchorBatch` field order per R1 §Wire Contract                  | in 6 files                           |
| per-controller Merkle-root batching per RFC-0968 §28.1                     | `reputation_anchors` in 3 migrations |
| `ReputationError::AnchorTupleFanoutExceeded (0x2A)` per-controller text    | in 3 files                           |
| test vectors pinned in `tests/anchoring/canonical_blobs.rs`                | **file absent**                      |
| mission 0968a per-chain cost estimate                                      | mission exists                       |

Five of six hold. The gate is therefore not gating anything: it is a checklist
describing a decision that has already been made and recorded in the sibling's
own Status header.

**This is worth stating carefully, because the naive reading is wrong in both
directions.** The unchecked boxes are _not_ evidence that Phase 5 is unfinished —
they are an acceptance gate, and the acceptance happened. Nor are they evidence
that Phase 5 is fine — one named file does not exist. The correct statement is
that the gate is stale in the same way the Status is: it describes a pre-promotion
world that both documents have since left.

## Why no check catches it

`scripts/lint_no_vh_accepted.sh` is the suite for exactly this territory, and it
passes:

```
PASS: all Accepted RFCs have ## Version History.
```

Its documented behaviour is _"For each Accepted RFC, verify presence of `##
Version History` table. Reject RFC promotion to Accepted without VH."_ It checks
that the **table exists**. It never compares the table's newest row against the
`## Status` header.

So R1 sails through: it has a Version History table, the table is well-formed,
and the newest row says `1.0-draft`. The lint has no opinion about a version
label contradicting the status two hundred lines away.

The same blind spot explains RFC-0955's own table. RFC-0955 _is_ consistent with
itself on labels — its rows read 1.0 and 1.1 — but it carries the duplicated
paragraph described below, which the lint also has no opinion about.

Three recorded missions already work adjacent territory, and none covers this:

- `one-rfc-has-two-version-history-tables-claiming-the-same-version` — duplicate
  VH sections inside one RFC
- `six-accepted-rfcs-have-no-version-history-after-three-linter-fixes` — the
  presence lint's own detection bug, since fixed
- `the-version-history-heading-pattern-is-written-three-times-in-two-guards` —
  the heading alternation duplicated across two lint scripts

The gap they leave open is the one this finding sits in: **a Version History that
exists, parses, and disagrees with the Status.** Checking it means comparing the
newest row's version label against the status word, which is a small extension
rather than a new lint — and is the owner's call, since a false positive here
reads as a promotion error.

## R1's own Version History is stale in the same direction

R1's Version History has exactly one row:

| Version   | Date       | Changes                                                                                                  |
| --------- | ---------- | -------------------------------------------------------------------------------------------------------- |
| 1.0-draft | 2026-07-27 | Initial draft. Promoted from in-file amendment at RFC-0955 to a sibling Draft RFC per BLUEPRINT process. |

Its Status header was updated to Accepted. Its Version History was not. So R1
contradicts _itself_, not just its sibling — a reader who trusts the version
history over the status header concludes R1 is a draft.

## The Status block also repeats itself

The anchoring paragraph appears **three times** in RFC-0955. The first two are
the same six lines: once as a continuation of the `Accepted 2026-07-27 …` status
sentence, and again immediately below as a standalone paragraph. The third
occurrence is in §Reputation Anchoring (RFC-0955-R1), reworded and legitimately
restated.

A copy-paste artifact sitting in the Status block of an Accepted RFC is worth
removing on its own merits — it is the first thing a reader sees, and it is
there twice.

## What the document gets right

- **Phases 1 through 4 are honestly empty.** Every checkbox is unchecked, and
  that is accurate: `ModelAsset`, `DatasetAsset`, `ComputeAsset`, `ProofAsset`
  and `OwnershipShare` all return zero occurrences in `crates/`. The section
  makes no claim of progress, and there is none. This is the correct way to
  present an unimplemented phase, and it is worth saying so given how rarely the
  rest of the corpus manages it.
- **Phase 5's gate names concrete files**, which is why five of its six
  conditions could be checked at all. Naming the file rather than describing the
  work is what made this finding possible.
- **§Future Work records a removal honestly**: F4 "Dataset Reputation System" is
  marked REMOVED in RFC-0955-R1 rather than quietly dropped.
- **§Version History is at the end of the file**, after the appendices, which is
  unusual. Not a defect, but it is why this document's status story is easy to
  miss.

## Two smaller things, one of them already filed

- **`share_percent: f64` under "must sum to 1.0".** The invariant is stated on
  the field and the RFC's own worked example (30 / 25 / 45) happens to sum
  exactly. But across all 4851 three-way integer-percent compositions, **204
  (4.2%) do not sum to exactly 1.0 in `f64`** — 6 / 57 / 37 gives
  0.9999999999999999. Verified by exhaustive enumeration, cross-checked against
  the combinatorial total. This is the same float-versus-scaled-integer problem
  RFC-0105 exists to solve.
- **`TokenAmount`, 21 uses, defined nowhere.** Already recorded in
  `tokenamount-is-used-by-three-accepted-rfcs-and-defined-by-none-of-them`. Not
  re-filed here; RFC-0955 is its second-largest user after RFC-0918.

## What the owner should do

- **Correct RFC-0955's Status.** R1 is Accepted, at an `rfcs/accepted/` path, and
  its gate is satisfied. The circular citation between the two documents should
  go with it.
- **Update R1's Version History.** Its only row still reads `1.0-draft`. Adding
  the promotion row is the smallest fix here and removes a self-contradiction.
- **Re-label or retire the Phase 5 gate.** It currently gates on a decision that
  has been made. Either mark it satisfied with the one outstanding condition
  called out, or drop it and let R1 own its own checklist.
- **Delete the duplicated Status paragraph.**
- **Decide whether to extend the VH lint to check agreement.** Comparing the
  newest version row against the status word would have caught R1 immediately.
  It would also produce false positives on documents that use non-numeric version
  labels, so it needs a deliberate policy. **Proposing, not implementing.**
- **Settle `share_percent` before Phase 1 starts.** A scaled-integer basis-points
  type makes "must sum to 1.0" exactly true; `f64` makes it approximately true
  4.2% of the time for three owners. This is a Layer A decision.

## Reproduction

```bash
# CONTROL — both files exist, and R1 is under accepted/, not draft/
find rfcs -iname '*0955*'

# R1 says Accepted; its version history says 1.0-draft
sed -n '1,6p' rfcs/accepted/economics/0955-r1-reputation-anchoring.md
grep -n -A4 '^## Version History' rfcs/accepted/economics/0955-r1-reputation-anchoring.md

# RFC-0955 says Draft five times, at a path that does not exist.
# Four are present-tense claims; the fifth is the Version History row,
# which is accurate history and should not be "fixed".
grep -n 'sibling Draft' rfcs/accepted/economics/0955-model-liquidity-layer.md
grep -c 'rfcs/draft/economics/0955-r1-reputation-anchoring.md' \
  rfcs/accepted/economics/0955-model-liquidity-layer.md
# 5
ls rfcs/draft/economics/0955-r1-reputation-anchoring.md
# no such file

# the circular citation
grep -n 'dependency gate is satisfied per RFC-0955 Status' \
  rfcs/accepted/economics/0955-r1-reputation-anchoring.md

# Phase 5 is a gate on a promotion already made, and five of six hold
grep -n 'Phase 5 acceptance gate' rfcs/accepted/economics/0955-model-liquidity-layer.md
[ -e crates/octo-reputation/src/constants.rs ] && echo "constants.rs EXISTS"
grep -rc --include=*.rs 'ReputationAnchorBatch' crates/ | grep -vE ':0$' | wc -l   # 6
grep -rl --include=*.sql 'reputation_anchors' crates/ | wc -l                      # 3
[ -e crates/octo-reputation/tests/anchoring/canonical_blobs.rs ] || echo "test vectors ABSENT"

# ...and the lint passes, because it checks presence and not agreement
bash scripts/lint_no_vh_accepted.sh 2>&1 | tail -2
grep -n 'verify presence of' scripts/lint_no_vh_accepted.sh

# the duplicated Status paragraph
grep -c 'The on-chain reputation anchoring binding is defined' \
  rfcs/accepted/economics/0955-model-liquidity-layer.md
# 3 — two identical in Status, one legitimately reworded in its own section

# Phases 1-4 are honestly empty
for s in ModelAsset DatasetAsset ComputeAsset ProofAsset OwnershipShare; do
  printf '%-18s %s\n' "$s" \
    "$(grep -rc --include=*.rs "\b$s\b" crates/ | grep -vE ':0$' | wc -l)"
done

# the f64 share invariant, exhaustively
python3 - <<'PY'
fails = total = 0
for a in range(1, 100):
    for b in range(1, 100):
        c = 100 - a - b
        if c < 1:
            continue
        total += 1
        if (a/100.0) + (b/100.0) + (c/100.0) != 1.0:
            fails += 1
print("compositions:", total, "expected", 99*98//2)
print("f64 sum != 1.0:", fails, f"({100*fails/total:.1f}%)")
print("RFC example 30/25/45 exact:", (30/100.0)+(25/100.0)+(45/100.0) == 1.0)
print("6/57/37 gives:", repr(6/100.0 + 57/100.0 + 37/100.0))
PY

# How far the version-label problem spreads.
# CAUTION: do NOT assume the last Version History row is the newest. Tables are
# ordered inconsistently corpus-wide, and assuming so yields false positives.
# The robust test is absence-based: does ANY row carry a non-draft label?
python3 - <<'ENUM'
import re, pathlib

bad = []
accepted = 0
for p in sorted(pathlib.Path('rfcs').rglob('*.md')):
    lines = p.read_text(encoding='utf-8', errors='replace').splitlines()
    head = "\n".join(lines[:40])
    if not (re.search(r'^## Status', head, re.M) and 'Accepted' in head[:2500]):
        continue
    accepted += 1
    rows, in_vh = [], False
    for ln in lines:
        if re.match(r'^#+\s*Version History', ln):
            in_vh = True
            continue
        if in_vh and re.match(r'^#+\s', ln):
            break
        if in_vh and ln.strip().startswith('|'):
            rows.append(ln)
    labels = [r.split('|')[1].strip() for r in rows[2:] if '|' in r]
    if labels and not any(re.search('draft', x, re.I) is None for x in labels):
        bad.append(p)

print("Accepted RFCs detected:", accepted)
print("with no non-draft version row at all:", len(bad))
for b in bad:
    print("   ", b)
ENUM
# 148 detected, 1 flagged: RFC-0955-R1
```

## Cross-references

- Mission `tokenamount-is-used-by-three-accepted-rfcs-and-defined-by-none-of-them`
  — RFC-0955's 21 uses, and the scaled-integer argument the `share_percent`
  finding also turns on
- Mission `six-accepted-rfcs-have-no-version-history-after-three-linter-fixes` —
  the same lint, and the detection bug it fixed; this finding is the gap that
  remained
- Mission `one-rfc-has-two-version-history-tables-claiming-the-same-version` —
  duplicate Version History structures, the nearest recorded neighbour
- Mission `the-marketplace-facade-has-no-production-caller` — the other half of
  the liquidity story: §Inference Marketplace here, `marketplace` there
- Mission `documentation-that-describes-a-mechanism-nobody-built` — Phases 1
  through 4 join it, though they at least say so
