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

Open. Nothing changed. The references are in audit and research documents
under `docs/`, plus one RFC, and rewriting them is author work.

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

## Acceptance criteria

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
