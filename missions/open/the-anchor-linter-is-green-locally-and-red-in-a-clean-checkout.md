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

## Classifying the fourteen

The 31 pairs are 14 distinct anchors. Classifying them decides how much of the
work is fixing citations and how much is fixing the linter.

The first attempt at this was done by hand, with `git grep` over tracked files,
and it was wrong in detail: it sorted headings into categories they do not belong
to, because it guessed at a matching rule the linter does not use. That rule is
threefold — a citation resolves when its name matches a heading exactly, when one
of its progressive forms does (which drops a trailing parenthetical), or when a
heading begins with the name followed by a space. Reimplementing it from memory
gave a different answer than the linter gives.

Asked directly, through the linter's own `normalize`, `candidates` and `resolves`
against this working tree's index, the fourteen fall into exactly two groups.

**Twelve resolve here, and every heading that satisfies them is in a gitignored
document.** Not one resolves against a tracked heading:

| Anchor                                            | Satisfied by                                  | Lives in                                   |
| ------------------------------------------------- | --------------------------------------------- | ------------------------------------------ |
| `§20.3.1`, `§20.3.2`, `§20.6.1`, `§20.7`          | a heading of that number                      | `docs/reviews/` storage-layer analysis     |
| `§appendix c`                                     | `Appendix C — backward compatibility`         | `docs/plans/` whatsapp phase 2.5           |
| `§appendix d`                                     | `Appendix D — daemon.api.version progression` | `docs/plans/` whatsapp phase 2             |
| `§coverage gap`                                   | `Coverage Gap — octo-market crate`            | `docs/plans/` storage-layer execution plan |
| `§layer c`                                        | `Layer C — http proxy + python sdk`           | `docs/audits/` developer guidelines        |
| `§replay defense`                                 | `Replay Defense`                              | `docs/audits/` developer guidelines        |
| `§error envelope (canonical`                      | `Error Envelope`, via the stripped form       | `docs/audits/` RFC-0011 spec cycle         |
| `§cross-rfc harmonization`, and its dated variant | `Cross-RFC Harmonization plan`                | gitignored                                 |

**Two resolve nowhere at all**, here or in a checkout — `§trust level canonical
uuids); must` and `§trust level canonical uuids); not`. These dangle on this
machine too; the local index never supplied them.

So the local index is not merely inflating a count. Every one of the twelve is a
citation pointing at a section that exists **only in review notes and execution
plans nobody else holds**, and the sections it finds are working documents — a
WhatsApp CLI plan, a storage-layer execution plan — rather than the RFCs the
citations name. A reader following `RFC-0960 §20.3.1` would be sent to a heading
present in exactly one person's uncommitted notes.

Three of the fourteen are additionally malformed in the citation itself, the
linter having read a sentence as a section name: `§error envelope (canonical`,
`§cross-rfc harmonization (2026-08-24`, and the two `§trust level` forms. Those
are terminator defects whichever way the corpus question is settled.

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
3. **Extending the baseline with all 31 banks a working document as a spec.**
   Every one of the twelve resolvable anchors resolves to a heading in
   `docs/reviews`, `docs/audits` or `docs/plans` — a WhatsApp CLI plan, a
   storage-layer execution plan, a developer-guidelines audit — and to nothing
   tracked. Recording those as accepted would enshrine, as the standard, that an
   RFC's section is a heading in one person's uncommitted plan. Two of the
   fourteen (`§trust level canonical uuids); must` and `; not`) dangle here too,
   so they are not a corpus artifact at all and should not be baselined as one.
   If the baseline is extended, the terminator artifacts — the three whose
   citation text is a sentence — are worth separating first, since they are a
   linter defect rather than a citation one.
4. **The cheapest real work is the twelve.** Each names a section that does not
   exist in the repository, and each citation points at an RFC while resolving to
   an unrelated working document. That is a genuine defect in the RFC text, and
   it is the part a reader would actually hit.

## Note on what was not done

The baseline was not extended and the index was not changed to read tracked
files only. The first alters a CI gate; the second alters what the linter
accepts. What was done is the measurement, in both environments, and the
finding that they disagree.
