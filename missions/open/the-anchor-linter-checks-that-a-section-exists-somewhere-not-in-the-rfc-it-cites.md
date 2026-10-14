---
name: the-anchor-linter-checks-that-a-section-exists-somewhere-not-in-the-rfc-it-cites
description: "lint-rfc-anchors.py verifies that a section name exists somewhere in its index and never checks which document the citation names. The RFC number on the line is read only in the numeric branch, to exclude external specifications. So RFC-0970 citing RFC-0862 §Replay Defense is satisfied by a heading of that name in any indexed file, including a gitignored WhatsApp CLI plan. Four separate gaps in the matching surface follow from that shape, each verified against the corpus: letter-prefixed headings are unaddressable and their citations are not extracted at all, number-prefixed headings are addressable only by their number, the word-prefix backoff resolves to two-word generic phrases against its own stated guard, and the index is the filesystem rather than the tracked tree."
metadata:
  node_type: mission
  type: tooling
  created: 2026-10-14T00:00:00.000Z
  v: "1.0"
  depends_on:
    - the-only-ci-wired-anchor-linter-checks-rfcs-but-indexes-headings-from-the-whole-tree
    - the-anchor-linter-is-green-locally-and-red-in-a-clean-checkout
---

# Mission `the-anchor-linter-checks-that-a-section-exists-somewhere-not-in-the-rfc-it-cites`

## Status

Open. The matching surface is characterised below, with each gap verified
against the corpus rather than inferred. No behaviour was changed, because every
change here alters which findings exist and the linter gates CI.

This consolidates findings that had been accumulating as corrections to
`the-anchor-linter-is-green-locally-and-red-in-a-clean-checkout`. That mission
remains the record of the corpus divergence and the fourteen anchors; this one
is about why the linter resolves the way it does.

## The pipeline

For a non-numeric citation the linter does this, and nothing else:

1. `ANCHOR` captures the text after `§`, stopping at a backtick, pipe, comma,
   dash, newline, a lowercase word, or a dot.
2. `normalize` strips emphasis, a leading `§`, trailing `.,;:)]}`, and case.
3. `candidates` yields progressively shorter forms, dropping parentheticals.
4. `is_section_anchor` requires two or more words.
5. `resolves` tries an exact match, then a heading beginning with the name plus a
   space, then a word-prefix backoff down to two words.

The RFC number is not an input to any of those. It is read in the numeric branch
only, to tell `RFC-0957 §3.5.7` from `RFC 8032 §5.1.6` and skip the external
specification.

## The structural property

**A non-numeric section citation is satisfied by a heading of that name anywhere
in the index, regardless of which document the sentence names.**

That is what makes a heading in a gitignored execution plan sufficient to
resolve an RFC's own section reference, and it is why the fourteen anchors in the
sibling mission resolve at all. It is also why `§Replay Defense`, cited as
`RFC-0862 §Replay Defense`, is a genuine defect that this linter cannot report:
RFC-0862 has no such section, but the linter has no way to ask.

Checking that a name exists is a weaker property than checking that it exists
where the sentence says. The corpus reason for the weak form is real — a section
may be defined in `docs/` and cited from `rfcs/` — but it means the linter cannot
distinguish a correct citation from one pointing at the wrong document.

## Four verified gaps

**Letter-prefixed headings are unaddressable, and their citations are not
extracted at all.** `### B. Coverage Gap` yields exactly one name, `b. coverage
gap`, because `SECTION_NUMBER` matches digits only. And `§B. Coverage Gap` is not
captured in the first place: `ANCHOR` requires a leading letter _and_ an
alphanumeric character, so the single letter before the dot fails the pattern and
the regex produces nothing. RFC-0008 cites its own appendix in the prose form
`§Coverage Gap`, which cannot match `b. coverage gap`. Both halves fail, in
opposite directions.

**Number-prefixed headings are addressable only by their number.**
`## 6.3 Error Envelope` yields `6.3` and `6.3 error envelope`, never
`error envelope`. A citation in the prose form `§Error Envelope` cannot resolve,
and the numbered form `§6.3 Error Envelope` is not captured either, since
`ANCHOR` will not start with a digit. RFC-0015-a is cited that way.

**The word-prefix backoff resolves to two-word generic phrases, against its own
stated guard.** The comment on that loop says only word-prefixes of two or more
words are tried, "so this cannot resolve an anchor down to a single common word".
For `§Trust Level Canonical UUIDs); must` the retries are four words, three
words, then two: `Trust Level`. Two common words passes the guard, and the
citation resolves against a heading reading `Trust Level` in an audit document
about something else. Raising the floor from one word to two stops `§the`
matching a heading `The` and does nothing for a malformed citation.

**The index is the filesystem, not the tracked tree.** `INDEX_SOURCES` is walked
with `rglob`, so the 711 gitignored markdown files under `docs/reviews`,
`docs/audits` and `docs/plans` supply headings that no checkout else has. This
is the sibling mission's subject and is only listed here because it is the fourth
way a citation can resolve to something it should not.

## What this does not change

`§Replay Defense` is still a real defect. RFC-0862 has no Replay Defense section
in either file carrying that number, and no linter change makes it otherwise —
only reading the cited document can decide it, which is what the RFC author is
for. The finding here is that the linter cannot distinguish that case from one
where the citation is fine, not that the citation is fine.

## Decisions for maintainers

1. **Make resolution document-aware, or stop claiming otherwise.** A citation
   naming `RFC-0862` could be checked against RFC-0862's headings first, falling
   back to the global index only when the citation names no document. That is a
   larger change than any of the other four and it is the one that would let the
   linter report `§Replay Defense` correctly.
2. **Strip a leading numbering prefix from headings**, so `b. coverage gap`
   also answers to `coverage gap` and `6.3 error envelope` to `error envelope`.
   This changes which findings exist, so it belongs with the baseline decision.
3. **Raise the backoff floor**, or require a truncated prefix to be a complete
   heading rather than any two-word run.
4. **Re-record the baseline from a clean checkout** after whichever of the above
   is adopted, because each changes the finding set and the current baseline was
   recorded against an index no runner builds.

## Note on what was not done

No behaviour was changed. Every fix here alters which findings the linter
produces, and it gates CI. What was done is the characterisation, each gap
demonstrated against the corpus, so the decisions are made against evidence
rather than against a symptom.
