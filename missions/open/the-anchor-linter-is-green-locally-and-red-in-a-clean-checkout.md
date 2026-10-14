---
name: the-anchor-linter-is-green-locally-and-red-in-a-clean-checkout
description: "lint-rfc-anchors.py exits 0 on a developer's machine and exits 1 in a clean checkout of the same commit. It indexes headings from rfcs, docs, missions and CLAUDE.md, and the index is built by walking the filesystem, so gitignored scratchpads supply 13454 of the 34406 headings. Locally 588 findings are all baselined; in a clean clone there are 619, of which 14 distinct anchors over 31 name-site pairs are new. RFC-0008 cites a Coverage Gap section that exists as a heading only in a gitignored review document. The linter is new and unpushed, so CI is green today, but the first push of the branch turns the Lint workflow red at that step."
metadata:
  node_type: mission
  type: tooling
  created: 2026-10-14T00:00:00.000Z
  v: "1.0"
  depends_on:
    - the-cite-validators-finding-counts-are-specific-to-one-machine
    - the-only-ci-wired-anchor-linter-checks-rfcs-but-indexes-headings-from-the-whole-tree
---

# Mission `the-anchor-linter-is-green-locally-and-red-in-a-clean-checkout`

## Status

Open, and it is predictive rather than historical: the failure has not reached
CI yet because the linter has not been pushed. Extending the baseline is a
maintainer decision and has not been done here.

## The measurement

Same commit, two environments:

|                                       | This machine | Clean checkout                               |
| ------------------------------------- | ------------ | -------------------------------------------- |
| `python3 scripts/lint-rfc-anchors.py` | **0**        | **1**                                        |
| headings indexed                      | 34406        | 20952                                        |
| dangling findings                     | 588          | 619                                          |
| baselined                             | 588 of 588   | 588 of 619                                   |
| would turn CI red                     | 0            | 31 name-site pairs, over 14 distinct anchors |

The clean-checkout run lists the new anchors. A representative one is
`§coverage gap`, cited twice by RFC-0008. The listing carries a file and line
for each site, which is the very shape this repository's own line-reference rule
bans; the anchor name and the citing RFC are the parts that matter here, so the
sites are named rather than reproduced.

RFC-0008 cites `§Coverage Gap`. In the repository there is no heading by that
exact name — there is `Coverage Gaps` and `Coverage Gap (68 RFCs requiring ...)`.
The exact match exists only in a gitignored review document: 39 headings in
`docs/reviews`, `docs/audits` and `docs/plans` against 3 in the tracked tree.
`§Cross-RFC Harmonization` is starker still: **0 tracked headings, 8 gitignored**.

## Why the two environments differ

The linter builds its index by walking the filesystem and checks citations in a
separate, narrower tree. Both facts are deliberate and documented — the index
must span the whole repository because a section may be defined in `docs/` or
`missions/` and cited from `rfcs/`. What was not accounted for is that the
repository's `docs/` contains 711 markdown files git does not track, in three
directories gitignored on purpose as local scratchpads.

So the local index is 1.64 times the size a clean checkout builds, and a citation
that resolves against a scratchpad heading resolves nowhere else. The baseline
was recorded against the larger index, which means it encodes 31 resolutions
that cannot happen in the environment the gate actually runs in.

This is the same defect as the one recorded for `validate_cites.sh`, and the
mechanism is shared: a corpus assembled by `find` from a working tree that
contains untracked local documents. That mission measures the cite validator's
version of it. This is the anchor linter's.

## Why CI is green right now

The linter is new and unpushed. It does not exist at the tip of `origin/next`,
so the workflow step that invokes it has never run. The branch is 540 commits
ahead of the remote and nothing has been pushed.

That is the only reason this is a prediction and not an outage. The first push
that includes `scripts/lint-rfc-anchors.py` runs the step in a clean checkout,
where it exits 1.

## What is not affected

`lint-rfc-type-refs.py`, the other baseline-backed linter in the same workflow,
exits 0 in the clean checkout with 652 of 652 baselined. It indexes symbols out
of RFC code blocks rather than headings across the tree, so it does not read
`docs/` and is not exposed to this. The exposure is specific to a linter whose
index is built outside the tree it gates.

## Decisions for maintainers

1. **Extend the baseline by the 31 name-site pairs** the clean checkout reports,
   and re-record it from a clean checkout rather than from a working tree, so
   the baseline describes the environment CI runs in. This changes a CI gate.
2. **Or make the index read only tracked files**, which removes the divergence
   at its source and makes a local run and a CI run the same corpus. It would
   also stop citations from resolving against headings that exist only on one
   developer's machine — which is the more correct outcome, since a section a
   reader cannot reach is a section that does not exist. Same trade-off as
   `validate_cites.sh`, and the same decision.
3. **The 14 anchors themselves name sections that do not exist in the
   repository.** Three of them read like genuine defects in the RFCs rather than
   index artifacts — `§coverage gap`, `§cross-rfc harmonization`,
   `§trust level canonical uuids); must` — where the citation carries a
   qualifier the linter's terminator cuts at the wrong place. Whether those are
   citation defects or a terminator defect is the open question, and it overlaps
   the terminator decision already recorded in
   `the-only-ci-wired-anchor-linter-checks-rfcs-but-indexes-headings-from-the-whole-tree`.

## Note on what was not done

The baseline was not extended and the index was not changed to read tracked
files only. The first alters a CI gate; the second alters what the linter
accepts. What was done is the measurement, in both environments, and the
finding that they disagree.
