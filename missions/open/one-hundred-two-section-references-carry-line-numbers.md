---
name: one-hundred-two-section-references-carry-line-numbers
description: "CLAUDE.md states the no-line-refs rule as a hard architectural rule: section references must never use file:line or L### form. A corpus scan finds 102 section references that do exactly that, across 18 files, concentrated in docs/audits. Examples include mission §Test Vectors header L205 and §F.3 L72. These sit outside the tree the anchors linter scans, which is why no gate reports them."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-13T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `one-hundred-two-section-references-carry-line-numbers`

## Status

Open, with the reachable part now fixed. The corpus splits in three, and only
two of the three were ever real work.

| Population                                  | Count | Disposition                             |
| ------------------------------------------- | ----- | --------------------------------------- |
| `docs/audits/`, gitignored since 2026-08-20 | 88    | local-only, not in the repository       |
| Rust source under `crates/`                 | 42    | **fixed**                               |
| tracked markdown                            | 27    | 13 fixed, 14 quote a violation and stay |

The headline count in this entry's name and description is a count of
occurrences across the working tree, most of which are not tracked. Measured
again over git-tracked files the reachable number was 27, not 102.

**Rust, 42 occurrences across 15 files, all removed.** Every section name was
verified to resolve in its cited RFC _before_ the suffix was dropped, not
after. One was not a line-reference problem at all: `sender/mod.rs` cited
`§Network Sender`, a heading that exists nowhere in RFC-0863, where the real
heading is `### NetworkSender Trait`. Stripping `L116` alone would have left a
citation that still dangled, so the name was corrected as well.
`crates/octo-cli/tests/security_regressions.rs` was checked for assertions on
the literal strings before any edit; there were none.

**Tracked markdown, 13 fixed.** One live citation in
`rfcs/accepted/process/0011-d-role-provisioning.md`, and 12 that the tooling
counted but which are inside audit scratchpads that git no longer tracks.

**14 remain and are correct.** They are in the three files that describe or
quote this very violation class — the two entries recording it, plus five rows
in a research document's Version History that quote the citation strings a past
sweep was examining (`RFC-0102 §Vault correctly preserved at L557`). Removing
those would delete the record of the defect.

## The finding

CLAUDE.md §Architectural Principles makes this a hard rule, not a
preference:

> **Section refs not line refs** — these principle descriptions reference
> other RFCs/sections by §section_name or symbol, NEVER by file:line.

The corpus contains 102 section references written in exactly the forbidden
form, across 18 files:

| Location                                                             | Count |
| -------------------------------------------------------------------- | ----- |
| `docs/audits/2026-09-02-m10-m11-rfc-amendment-dry.md`                | 27    |
| `docs/audits/2026-09-16-r1-attachhandle-substrate-faithfulness.md`   | 15    |
| `docs/audits/2026-09-02-m10-m11-upstream-rfc-trace.md`               | 14    |
| `docs/research/2026-08-21-vault-monetary-representation-redesign.md` | 12    |
| `docs/audits/rfc-0205-0206-r8-findings-2026-08-20.md`                | 11    |
| `docs/audits/rfc-0205-0206-r7-findings-2026-08-19.md`                | 5     |
| `docs/audits/rfc-0205-0206-r9-findings-2026-08-20.md`                | 3     |
| 12 further files, including 1 in `rfcs/`                             | 15    |

By tree: `docs/` 100, `missions/` 13, `rfcs/` 1.

Reproduce with:

```bash
python3 scripts/measure_anchor_lint.py --all --line-refs
```

The count excludes this mission file, which quotes several violations and
would otherwise report its own examples.

Representative violations, verbatim:

```markdown
- **File:** mission §F.3 L72 + RFC §F.3 L740 + §Cargo deps L194
- **File:** mission §Test Vectors header L205
- **File:** mission §Layer direction L220 + §Backward compat L237
```

## Why no gate caught it

Two reasons, and the second is the important one.

1. The anchors linter resolves `§name` against headings. `§F.3 L72` is a
   compound string, so it never matches a heading and is reported as
   dangling — but only in the tree it scans, and the concentration here is
   `docs/audits/`.

2. Even once `docs/` is in scope, the compound reference would be reported as
   a _dangling section_, which is the wrong diagnosis. It is not a missing
   heading. It is a present heading followed by a forbidden line reference.
   Fixing the line number is not required to make the citation resolve.

So this defect needs its own check, or its own terminator rule, rather than
being swept up by anchor resolution.

## How it was found

While measuring the cost of widening the anchors linter to `docs/` and
`missions/`, a candidate terminator set that stops a name at `)` and `:` was
tested against those trees.

**Correction.** This section previously said the terminator change unmasked 40
line-number references. Measured with the linter's own functions via
`scripts/measure_anchor_lint.py`, it unmasks **2**:

| Tree        | terminator delta | line-ref shaped |
| ----------- | ---------------- | --------------- |
| `docs/`     | 13               | 2               |
| `missions/` | 24               | 0               |

The 40 came from the same faulty simulation that inflated the other figures in
this area, and it was counted with a pattern requiring the _entire_ name to end
in a line number, so a name such as `§Mission Decomp L1531-1532` was excluded
because the name continues past the number.

The finding itself is a direct corpus count, not a simulation output — but the recorded mechanism was wrong. The terminator
change is not what exposed these. A direct pattern search does, and that is
what a convention check should use.

## Scope note

The single occurrence in `rfcs/` is inside a Mermaid node label:

```
J["octo-network/src/mon/coordinator (Layer C specialized node; per RFC-0855p-b ...
```

It is `§Key Files L847` inside a diagram, where a line reference is a
navigational convenience rather than a claim in prose. It may be defensible
and is called out separately rather than lumped with the audit documents.

## A second shape the measuring tool does not see

`measure_anchor_lint.py --line-refs` matches one shape: a section reference with
a line number appended to it. It does not match a bare source path followed by a
line suffix, which is the more common form in the tracked corpus.

Measured over git-tracked markdown, that second shape occurs **863 times across
54 files**:

| Tree                     | Occurrences |
| ------------------------ | ----------- |
| `docs/`                  | 808         |
| `.audit/` and `.audits/` | 22          |
| `rfcs/`                  | 15          |
| `missions/`              | 15          |
| `.jcode/` and `crates/`  | 3           |

Reproduce with:

```bash
git ls-files '*.md' | xargs grep -oE '[A-Za-z0-9_./-]+\.(md|rs|py|sh|toml|yml|yaml|json|ts|tsx|js|cfg|ini|lock):[0-9]+'
```

This is a pattern search rather than a linter call, so it counts the shape and
does not rule on scope. Whether a bare path plus a line number is a violation, or
a legitimate pointer inside a technical note, is the question the first
acceptance criterion asks — and when it was written the only number available
was the 27 reachable occurrences of the other shape.

The concentration is `docs/research`, holding 23 of the 54 files. Fifteen
occurrences sit in `rfcs/`, which are the ones that would matter most if the
rule is read as covering normative documents.

### The same shape outside markdown

The 863 figure counts git-tracked markdown only, because the rule is written
about prose and prose is where prose lives. A reader deciding whether the rule
should reach code comments and docstrings needs to know what is on the other
side of that boundary, so it was measured too:

| Corpus                    | Occurrences |
| ------------------------- | ----------- |
| tracked `*.md`            | 863         |
| tracked `*.py` and `*.sh` | 6           |

Six, across 88 files — the markdown corpus is about 99.4 percent of the
tracked population, so widening the rule to comments would not change the size
of the work by any amount worth deciding differently over. It would change
which files need reading.

Of the six, one is a worked example inside the detector's own comment, showing
the shape it exists to find. That one has to stay: a detector that cannot
illustrate what it detects is harder to trust than one that can. Four are in a
single file, in comments pointing at where two discriminant values are defined
in a Rust source file, and one is inside a message that script prints. They are
accurate today and will silently rot.

The first count used a pattern that required a file extension before the line
number, so it missed the other common spelling: a bare RFC number followed by
the word line. That form occurs once in code and once in markdown. It is why
the code figure moved from five to six when the pattern was widened — the
population was measured by a pattern narrower than the rule it was measuring.

`.rs` files are not measured. Rust source carries line references in ordinary
use and including them would swamp the question rather than inform it.

### The tracked audit scratch directories

The disposition table above records the `docs/audits/` concentration as
local-only, because that directory is gitignored. Two root-level scratch
directories are tracked and were never part of that accounting: `.audit/` with
11 files and `.audits/` with 2. Between them they hold 22 occurrences of the
second shape. They are audit artifacts committed to the repository, so the
local-only disposition does not reach them.

## Acceptance criteria

- [ ] Decide whether the bare-path-plus-line shape is in scope at all. It is 863
      tracked occurrences against the 27 reachable of the other shape, so the
      answer decides whether this is a bounded fix or a sweep of every
      technical note in `docs/research`.
- [ ] Decide whether the rule reaches code comments and docstrings. Five
      occurrences sit outside markdown across 88 tracked scripts, one of which
      is the detector's own worked example and must stay. Small enough that it
      does not change the shape of the work, but it is a boundary the rule does
      not currently draw.
- [ ] Decide whether a check should reject line-suffixed section references, or
      whether the rule applies only to prose and not to diagram labels and
      audit tables.
- [ ] If checked, the check reports the file and line like every other linter
      here does, rather than being folded into anchor resolution where the
      diagnosis would be wrong.
- [ ] Decide whether `measure_anchor_lint.py --line-refs` becomes the gate or
      a report. It currently measures and exits 0, which is correct for a tool
      whose other modes describe cost rather than correctness. Promoting it to
      a gate is a separate decision from building it, and the two have been
      treated as the same thing in this file until now.
- [ ] The `docs/audits` concentration is addressed first. Audit documents are
      written fastest and cite most densely, which is why they carry most of
      the violations.
- [ ] Any rewritten reference keeps the section name intact. Stripping `L847`
      from `§Key Files L847` yields a valid reference; deleting the whole
      citation would not.

## Notes

The rule is written in CLAUDE.md, which is now covered by the anchors linter
for the _heading resolution_ half of this problem. It is not covered for
convention compliance, because a convention check and a resolution check are
different tools measuring different things, and conflating them produces a
diagnosis that sends the reader to the wrong fix.
