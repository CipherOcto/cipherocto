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

**Fixed.** The corpus divergence no longer exists: the index is built from
git-tracked files, so this machine and a clean checkout compute the same
number. The baseline was regenerated from 588 to 619, and both environments now
report 619 of 619 baselined and exit 0.

The measurement sections below are kept as the record of what the divergence
was, and the "correct pattern already exists in a sibling script" section is
what the fix was built from.

The 31 findings that only a clean checkout could see were **recorded** in the
baseline, not resolved. That remains open work on the RFCs and is recorded
below as such; what is fixed is that CI and the working tree now agree about
which findings exist.

## Resolution

One enumeration site caused the whole divergence, and it was the index rather
than the scan corpus: `rfcs/` is fully tracked, so the citations it scans were
never machine-dependent, while the headings they resolve against were built by
walking `docs/` and `missions/` and so included 711 gitignored local
scratchpads.

The index now prefers git-tracked files and falls back to the filesystem walk
only for a source outside the checkout, which is what the mutation harness's
fixture corpora need.

| Check                                                   | Result                         |
| ------------------------------------------------------- | ------------------------------ |
| local, before                                           | 588 findings, exit 0           |
| tracked-only archive, before                            | 619 findings, exit 1           |
| local, after                                            | 619 of 619 baselined, exit 0   |
| tracked-only archive, after                             | 619 of 619 baselined, exit 0   |
| mutation harness (fixture corpora outside the checkout) | 26 passed, 0 failed, 3 skipped |
| a newly invented section planted in a tracked RFC       | 620 findings, 1 NEW, exit 1    |
| same, restored                                          | exit 0                         |

The baseline change is **purely additive**: 588 entries to 619, with zero
stale and zero removed.

`--strict` and `--corpus` are unchanged. The corpus report still counts what
the filesystem contains, which remains a valid census of the working tree even
though the lint no longer depends on it.

## The finding

This mission records the corpus divergence and the fourteen anchors. Why the
linter resolves them the way it does — including two gaps that make some of them
linter defects rather than citation defects — is consolidated in
`the-anchor-linter-checks-that-a-section-exists-somewhere-not-in-the-rfc-it-cites`.

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

## Reproducing this without cloning

The original measurement used a full clone, which costs 1.6 GB. On a working
volume at 100% that is the difference between a check being possible and not.
The same comparison is available from a tracked-only archive of the four paths
the linter actually reads:

```bash
mkdir -p /tmp/cleanco
git archive HEAD rfcs docs missions CLAUDE.md | tar -x -C /tmp/cleanco
python3 scripts/lint-rfc-anchors.py --root /tmp/cleanco
```

That is 27 MB, and it needs no clone, no fetch and no object database. Re-measured
on 2026-10-14 after a day of new missions and documents were added:

| Corpus                  | Headings indexed | Findings | Result                       |
| ----------------------- | ---------------- | -------- | ---------------------------- |
| tracked-only archive    | 21,033           | 619      | exit 1, 0 of 619 baselined   |
| full local working tree | 34,493           | 588      | exit 0, 588 of 588 baselined |

The divergence is **stable**: eight new tracked missions added thousands of
headings to both corpora equally and moved neither number. The gap is still 31
findings and 13,460 headings, and the ratio is still 1.64.

The three gitignored directories are `docs/reviews/` (409 files),
`docs/audits/` (197) and `docs/plans/` (105) — 711 in total, matching the count
recorded above.

## The correct pattern already exists in a sibling script

This was found after the mission was written, and it changes the shape of the
decision: the alternative is not hypothetical. Another guard in this same
repository already does it.

How each document guard enumerates its corpus:

| Guard                    | Enumeration        | Machine-independent                              |
| ------------------------ | ------------------ | ------------------------------------------------ |
| `lint_section_cites.sh`  | `git ls-files`     | **yes**                                          |
| `lint-rfc-anchors.py`    | filesystem `rglob` | **no**                                           |
| `lint-rfc-type-refs.py`  | filesystem `rglob` | yes — reads `rfcs/` only, which is fully tracked |
| `lint_frontmatter.py`    | filesystem `rglob` | no — reads `docs/` and `missions/` too           |
| `lint_no_vh_accepted.sh` | `RFC_ROOT` only    | yes                                              |

`lint_section_cites.sh` draws its denominator from the repository rather than
from the tree it is scanning, and says why in its own comments: a run against a
fixture corpus otherwise reports `COVERAGE=0%` for content it examined in full,
which reads as "nothing was examined". It enumerates with `git ls-files` and
makes every path CWD-relative so the figure and the comparison cannot disagree
about which tree is meant.

`lint-rfc-anchors.py` walks the filesystem in five places. Its **only** use of
`git ls-files` is inside the reporting mode added for this investigation — it
knows the difference and does not apply it to the lint path.

Measured today, running each guard over a tracked-only archive and over the
full working tree:

| Guard                    | Full tree    | Tracked-only             | Diverges                            |
| ------------------------ | ------------ | ------------------------ | ----------------------------------- |
| `lint_section_cites.sh`  | exit 0       | _(needs a git checkout)_ | no — it only ever saw tracked files |
| `lint_frontmatter.py`    | exit 0       | exit 0                   | no                                  |
| `lint_no_vh_accepted.sh` | PASS=155     | PASS=155                 | no                                  |
| `lint-rfc-anchors.py`    | 588 findings | 619 findings             | **yes, 31**                         |

So the anchor linter is the outlier, and the fix has a precedent in the same
tree. What remains a maintainer decision is not _which pattern_ but whether
baselining 619 pre-existing dangling anchors in the tracked corpus is
acceptable — that is a judgement about the RFCs, not about the tooling.

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

It was first attempted by hand, with `git grep` over tracked files, and came out
wrong in detail. A second attempt reimplemented the matching rule from memory and
came out wrong too. The rule has three arms, not two: an exact match; a heading
beginning with the name plus a space; and a **word-prefix backoff** that retries
the citation truncated to fewer and fewer words until something matches. Only the
third explains the two anchors that behaved most strangely.

Asked through the linter's own `normalize`, `candidates` and `resolves`, all
fourteen fall into one group, and it is the uncomfortable one: **every one of them
is supplied by a gitignored document.** None resolves against a tracked heading.

| Anchor                                            | Resolved by                                                                          | Match arm                                  | File                                       |
| ------------------------------------------------- | ------------------------------------------------------------------------------------ | ------------------------------------------ | ------------------------------------------ |
| `§20.3.1`, `§20.3.2`, `§20.6.1`, `§20.7`          | a heading of that number                                                             | exact                                      | `docs/reviews/` storage-layer analysis     |
| `§error envelope (canonical`                      | `Error Envelope`                                                                     | exact, via the parenthetical-stripped form | `docs/audits/` RFC-0011 spec cycle         |
| `§replay defense`                                 | `Replay Defense`                                                                     | exact                                      | `docs/audits/` developer guidelines        |
| `§appendix c`, `§appendix d`                      | `Appendix C — backward compatibility`, `Appendix D — daemon.api.version progression` | heading-prefix                             | `docs/plans/` WhatsApp phase 2             |
| `§coverage gap`                                   | `Coverage Gap — octo-market crate`                                                   | heading-prefix                             | `docs/plans/` storage-layer execution plan |
| `§cross-rfc harmonization`, and its dated variant | `Cross-RFC Harmonization plan — 2026-08-24`                                          | heading-prefix                             | `docs/plans/` cross-rfc harmonization plan |
| `§layer c`                                        | `Layer C — http proxy + python sdk`                                                  | heading-prefix                             | `docs/audits/` developer guidelines        |
| `§trust level canonical uuids); must` and `; not` | `Trust Level`                                                                        | **word-prefix backoff**                    | `docs/audits/` developer guidelines mesh   |

So the local index is not inflating a count. Each of these is a citation pointing
at a section that exists only in review notes, audit scratchpads and execution
plans nobody else holds, while the citations themselves name RFCs. A reader
following `RFC-0960 §20.3.1` would be sent to a heading present in exactly one
person's uncommitted notes.

## The backoff can resolve to two generic words

The last row is a defect in its own right, independent of the corpus.

The backoff is guarded by its own comment: _"Only word-prefixes of two or more
words are tried, so this cannot resolve an anchor down to a single common
word."_ But `§Trust Level Canonical UUIDs); must` is five words, and the retry
sequence is `trust level canonical uuids);` → `trust level canonical` →
**`trust level`**. The third is two words, so it passes the guard, and the
malformed citation resolves against a heading reading `Trust Level` in an audit
document about something else entirely.

Two common words is not meaningfully longer than one. The guard raises the floor
from one word to two, which is enough to stop `§the` matching a heading `The`,
and not enough to stop `§Trust Level Canonical UUIDs); must` matching `Trust
Level`. The backoff is doing exactly what it was added to do for the cases it was
written for — `§Implementation Phases Phase 1` against a `## Implementation
Phases` heading — and it is also absorbing citations that are simply malformed,
which is the more common case in this corpus.

Three of the fourteen are malformed in that way, where the linter read a
sentence as a section name: `§error envelope (canonical`, `§cross-rfc
harmonization (2026-08-24`, and both `§trust level` forms.

## Two of them are the linter, not the citation

Of the fourteen, two are sections that exist in a **tracked** RFC and that the
linter cannot see, because the heading carries a numbering prefix and the
citation does not:

| Anchor                       | Heading in a tracked RFC           | What the linter has  |
| ---------------------------- | ---------------------------------- | -------------------- |
| `§coverage gap`              | `b. coverage gap` in RFC-0008      | `b. coverage gap`    |
| `§error envelope (canonical` | `6.3 error envelope` in RFC-0015-a | `6.3 error envelope` |

`normalize` strips trailing punctuation and a leading section sign. It does not
strip a leading `B. ` or `6.3 `, so a citation written as `§Coverage Gap` — which
is how RFC-0008 refers to its own appendix B — cannot match its own heading. The
corpus finding and this one compound: the gitignored heading satisfied the
citation first, so nobody saw that the tracked one was invisible too.

This is why the earlier framing of the twelve as "genuine citation defects" was
too confident. Two of them are the linter looking at a section that is right
there. The remaining twelve do not resolve against any tracked file even after a
numbering prefix is stripped, which is the narrower and checkable claim.

## Decisions for maintainers

1. **Extend the baseline by the 31 name-site pairs** the clean checkout reports,
   and re-record it from a clean checkout rather than from a working tree, so
   the baseline describes the environment CI runs in. This changes a CI gate.
2. **Or make the index read only tracked files**, which removes the divergence at
   its source and makes a local run and a CI run the same corpus. It would also
   stop citations resolving against headings that exist only on one developer's
   machine, which is the more correct outcome, since a section a reader cannot
   reach does not exist. Same trade-off as `validate_cites.sh`, and the same
   decision.
3. **Extending the baseline with all 31 banks a working document as a spec.**
   Every one of the fourteen resolves to a heading in `docs/reviews`,
   `docs/audits` or `docs/plans` — a WhatsApp CLI plan, a storage-layer
   execution plan, a developer-guidelines audit — and to nothing tracked.
   Recording those as accepted would enshrine, as the standard, that an RFC's
   section is a heading in one person's uncommitted plan.
4. **Raise the backoff floor**, or require the truncated prefix to be a
   complete heading rather than any two-word prefix. On this corpus that changes
   which findings exist, so it belongs with the baseline decision rather than
   beside it.
5. **The twelve are not all citation work, and the split is checkable.** Two
   are the linter failing to see a tracked heading behind a numbering prefix.
   The other twelve name a section absent from the tracked tree while citing an
   RFC, resolving only against a working document. Fixing the prefix handling
   first would reclassify those two and change which findings the other twelve
   are measured against, so the linter changes and the citation changes do not
   commute.

## What a push would actually do

The prediction above was checked rather than left standing. Every step of the
Lint workflow that can fail the job was run in the clean clone, because the
workflow gives most of its steps a `|| true` suffix and only three lack it:

| Step                                             | Can fail the job | Result in a clean clone    |
| ------------------------------------------------ | ---------------- | -------------------------- |
| Rust format check                                | yes              | pass                       |
| Lint GitHub Actions workflows (actionlint 1.7.7) | yes              | pass                       |
| Rust clippy                                      | no, `            |                            | true` | not run |
| Run Prettier                                     | no, `            |                            | true` | not run |
| Lint RFC section anchors                         | yes              | **fails, 31 new findings** |
| Lint RFC type references                         | yes              | pass                       |
| Python format checks                             | no, `            |                            | true` | not run |

So exactly one step goes red, and it is the one this mission is about. The
actionlint run needed the container to run as the invoking user, since a
temporary directory is created mode 0700 and the image reads as another user;
with that corrected it reports nothing.

`commit-message-lint.yml` is the other workflow whose result is knowable without
building. All 544 commit messages in the range were run through the shipped
`dangerous_present` — whole messages, not lines — and none is flagged, which is
what the commit hook enforced at write time.

The remaining seven workflows triggered by a push to `next` build Rust or run
the test suites. They were not verified: the volume this repository sits on has
about 3.6 GB free, which is not enough to compile, and that is the same
constraint blocking the rebuild tier of the mutation harness. So the honest
statement is that the Lint and commit-message workflows were checked and that
the rest are unverified rather than assumed green.

## Note on what was not done

The baseline was not extended and the index was not changed to read tracked
files only. The first alters a CI gate; the second alters what the linter
accepts. What was done is the measurement, in both environments, and the
finding that they disagree.
