---
name: rfcs-cite-sections-of-documents-the-repository-deliberately-does-not-track
description: "RFCs cite sections of local review and planning documents by number. RFC-0960 says 'Per review §20.3 Model B lattice' and RFC-0959 cites 'long-horizon plan v1.6 §Cross-RFC Harmonization'. Those documents live under docs/reviews and docs/plans, which are gitignored by design as local scratchpads, so the citation resolves on the machine that wrote it and nowhere else. The CI-wired anchor linter therefore reports them dangling in any clean checkout. Measured floor: 14 references of the explicit 'review §X' and 'plan vN §X' shapes across 6 tracked RFCs, and at least one more that omits the word review entirely, so the shape undercounts."
metadata:
  node_type: mission
  type: documentation
  created: 2026-10-14T00:00:00.000Z
  v: "1.0"
  depends_on:
    - the-anchor-linter-is-green-locally-and-red-in-a-clean-checkout
---

# Mission `rfcs-cite-sections-of-documents-the-repository-deliberately-does-not-track`

## Status

Open. This is a documentation-convention question rather than a linter defect,
and it is recorded here because it accounts for a large share of what the
anchor linter reports in a clean checkout.

## The finding

An RFC cites a section of a document that is not in the repository.

RFC-0960, in tracked text:

> Per review §20.3 Model B lattice (chain-aware substrate, no organizational
> intermediates), the canonical substrate for vault + transfer-event storage is

RFC-0959's Version History, also tracked:

> **Cross-RFC harmonization amendment per long-horizon plan v1.6
> §Cross-RFC Harmonization (2026-08-24).**

Both resolve here, against headings in `docs/reviews/` and `docs/plans/` — and
both directories are gitignored on purpose. `.gitignore` records
`docs/reviews/` as local review scratchpads never committed, and
`docs/audits/` with a note that twelve files were untracked from the repository
in August. `docs/plans/` is commented as scratchpads the same way.

So the citation is well-formed, the section name is exact, and the target
document exists — for one machine. A reader in any other checkout, and any CI
runner, follows `§20.3` into nothing.

This is not a linter defect. `lint-rfc-anchors.py` is reporting accurately: the
section is not in the corpus it indexes. It is a convention that produces
citations no one else can follow.

## How much of it there is

At least 14 references of two explicit shapes across 6 tracked RFC files:

| Shape        | References |
| ------------ | ---------- |
| `review §X`  | 7          |
| `plan vN §X` | 7          |

That is a **floor, not a total.** RFC-0960 also cites `§20.3` at least once
without the word "review" attached, so the labelled patterns undercount by
construction.

### The total was attempted three times and could not be measured

Worth recording, because the next person will try it too.

1. Enumerating through the linter's own anchor extractor returned **nothing**
   for the `§20.x` family: those anchors begin with a digit, so they are matched
   by the numeric pattern and never by the general one.
2. Counting every section reference whose line mentions review, plan or audit
   returned **997**, which is nonsense — nearly every Version History row in this
   corpus says "R9 review findings applied" and then cites its own sections.
3. Classifying every reference by whether the heading that satisfies it is
   tracked returned **2006 references that would dangle in a clean checkout**,
   against the **619** the linter actually reports there.

The third is the instructive one, because it looks like the right method. It is
off by more than three times because the population is not "every section
reference": the linter drops single non-numeric tokens, evaluates numeric
anchors only on lines that also cite a project RFC, tests numeric anchors in
their unnormalised form, and records findings as `(name, site)` pairs rather
than names. Matching it means reimplementing it.

So no total was published at that point, and the floor above stands.

### It was then measured, by the linter itself

The obstacle turned out to be a category error rather than a real limit. An
instrumented mode that _reports_ does not change what the linter rejects, so it
is not a change to a CI gate — the same distinction as adding a corpus line to
`validate_cites.sh`. Making the omission visible was never a decision.

`lint-rfc-anchors.py --corpus` now prints it, delegating to the linter's own
resolution rather than reimplementing it:

```
corpus report (reporting only; findings and exit code unchanged)
  references resolving inside the tracked tree    : 2475
  resolving ONLY in untracked files, named sections: 548
  resolving ONLY in untracked files, bare numbers : 556
```

**548 named sections** — a section a reader is told to consult, which exists
only in a file this checkout does not track. The 548 is a `(file, section)`
pair count, matching how the linter records findings, so a section cited from
three RFCs counts three times.

The bare-number group is reported separately and deliberately not called
findings: `§2` is satisfied by any heading beginning with the digit 2, so those
556 are weak matches by construction rather than citations to a document.

## Why it matters beyond the finding count

Each of these is a place where an RFC's normative text points outside the
repository. That has two costs, and the second is the one worth weighing:

1. The anchor linter reports them, so the CI gate is red on first push.
2. More importantly, the RFC is not self-contained. A specification that
   derives a storage layout from `§20.3` of a review nobody else holds is
   asserting something it does not contain.

The second does not depend on any linter being run.

## Decisions for maintainers

1. **Promote the referenced sections into the citing RFC, or into a tracked
   document, and cite that.** This is the fix that makes the citations
   followable and costs one edit per reference. It is RFC authoring, so it is
   not an agent's to make.
2. **Or drop the section-level citation and state the rule inline.** RFC-0960's
   "no organizational intermediates" constraint is the actual content; it does
   not need a citation to survive.
3. **Or track the review documents.** That reverses a deliberate policy recorded
   in `.gitignore`, and is worth as a decision of its own rather than as a side
   effect of fixing citations.
4. **Decide this before re-recording the anchor baseline.** Whichever way it
   goes changes which findings the baseline should carry, and a baseline that
   absorbs citations to untracked documents records them as accepted.

## Note on what was not done

No RFC was edited and no citation was rewritten. Each needs the author who
knows whether `§20.3` is still the right reference or whether the constraint has
since moved.
