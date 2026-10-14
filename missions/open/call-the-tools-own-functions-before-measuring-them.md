---
name: call-the-tools-own-functions-before-measuring-them
description: >-
  A python reimplementation of one guard's Accepted-detection logic counted 148 RFCs where the guard itself counted 155. The gap was a third rendering, where the status is written on the heading rather than below it. The guard was right and the reimplementation was wrong. The repository already contains the right tools for this and states the rule in their docstrings; the failure was not lacking a tool but not reaching for the existing one first.
metadata:
  node_type: mission
  type: tooling
  created: 2026-10-13T00:00:00.000Z
  v: "1.0"
  depends_on:
    - five-checks-report-clean-while-their-counters-describe-a-different-population
---

# Mission `call-the-tools-own-functions-before-measuring-them`

## Status

Not a defect in the repository. A rule, written down because it was broken in
practice rather than merely acknowledged.

## What happened

Auditing `lint_no_vh_accepted.sh`'s population counter, I reimplemented its
Accepted-detection logic in Python to get a second measurement. That
reimplementation counted **148** Accepted RFCs.

The guard reports **155**.

The 7-file gap was a rendering my reimplementation did not handle:

```markdown
## Status: Accepted
```

— the status written **on the heading itself**, rather than on the lines below
it. The corpus uses three shapes: bare heading with the status below, inline on
the heading, and YAML frontmatter. The guard reads `grep -A5` from the heading,
so it sees all three.

**The guard was right and my measurement was wrong.** Had I not cross-checked,
I would have filed a defect against a correct counter — and the fix would have
been to make the guard match my bug.

## The rule

**Measure a tool with the tool, not with a reimplementation of it.**

When a second measurement is wanted:

1. **Look for an existing tool first.** `scripts/classify_validate_cites.py`
   already classifies this linter's findings, and its docstring states that the
   heading list is produced by the exact command the linter uses:
   `grep -oE '^##+ [^#].*$' FILE | sed 's/^##* //'`. Verified equivalent against
   the shell this session.
2. **If none exists, shell out to the same command the linter runs.** That is
   what the classifier does, and it is why it stayed correct while my ad-hoc
   script did not.
3. **Never reimplement the rule being measured.** A reimplementation can only
   agree by coincidence, and when it disagrees there is no way to tell which is
   wrong without reading the corpus — which is the expensive thing the tool
   already automated.

## Why this keeps happening

Because the reimplementation is faster to type than the tool is to find. The
existing classifier sits in `scripts/`, unmentioned by the guard it measures.
Three commits in this session used ad-hoc Python where the classifier or a
direct shell call would have been both faster and correct.

The deeper cost: a reimplementation that disagrees is **more expensive** than no
measurement at all, because it produces a confident wrong number that has to be
investigated. The 7-file gap cost a full debugging pass to attribute.

## What would make this cheap to follow

Not another rule document. The classifier exists and is correct; what is missing
is that a guard names the tool that measures it, the way the guards here already
name their corpus and their counters. A one-line pointer in
`lint_no_vh_accepted.sh` — "to measure this counter, call `classify_…` or the
same `grep`" — would put the tool in reach at the moment someone is about to
reimplement it.

**Done.** Three of the four guards that lack a pointer now carry one:

- `scripts/lint_no_vh_accepted.sh` names the classifier, states the measured
  discrepancy, and says not to reimplement the rule in order to measure it.
- `scripts/lint-rfc-anchors.py` names `measure_anchor_lint.py` and cites the
  four-wrong-numbers precedent, where a copy of its resolution loop reported
  1237 findings against the linter's 599.
- `scripts/lint-rfc-type-refs.py` names the string-literal extraction its
  comment-stripping does, and the bot token that held a baselined entry until
  that step existed.

All three outputs are unchanged; only comment blocks were added.

The remaining guards need no pointer because they already have one by another
name: `scripts/mutation-verify-doc-guards.sh` drives eight of them from fixture
corpora and asserts each can fail, which is the same need expressed as a
re-runnable harness rather than an ad-hoc measurement. Checking for that before
building a measuring tool is itself an instance of the rule.

## Related

- The guard audited here was found sound. Its counter closes exactly:
  155 Accepted, 91 not Accepted, 1 index file, 247 scanned.
- `measure_anchor_lint.py` is the same pattern for the anchor linter, and
  likewise partitions findings by calling the linter's own functions rather than
  reimplementing them.
- `five-checks-report-clean-while-their-counters-describe-a-different-population`
  is the mission this rule exists to prevent repeating.
