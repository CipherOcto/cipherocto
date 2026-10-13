---
name: four-hundred-and-thirty-six-section-citations-name-a-section-that-does-not-exist
description: >-
  validate_cites.sh reports 1300 INVALID. Re-measured with exact-heading matching: 367 resolve mechanically, 296 against a sibling file carrying the same RFC number and 71 against a heading in docs/ or missions/. The remaining 933 name a section that exists nowhere and need author intent. Supersedes the earlier 625/436 split, which counted substring matches as resolutions.
metadata:
  node_type: mission
  type: tooling
  created: 2026-10-13T00:00:00.000Z
  v: "2.0"
  depends_on:
    - one-thousand-nine-hundred-rfc-references-carry-a-version-pin-and-only-a-fifth-are-cross-references
---

# Mission `four-hundred-and-thirty-six-section-citations-name-a-section-that-does-not-exist`

## Status

Re-measured. The headline figure is wrong and is corrected here in place rather
than left standing next to a newer number.

The original split — 625 path-ambiguity, 436 genuinely dangling — counted
**substring** matches as resolutions. Under exact-heading matching the
resolvable set is **367**, not 625, and the genuinely-dangling set is **933**,
not 436.

## Current measurement

`bash scripts/validate_cites.sh`, full tree:

```
Summary: CHECKED=29273 VALID=26627 PHANTOM=339 INVALID=1300 STALE=1007
         SCOPE: 422 of 1263 tracked markdown files lie under (rfcs docs); 841 unexamined
```

The 1300 `INVALID [section missing]` findings break down as:

| Count | Class                                                                                     | Needs            |
| ----- | ----------------------------------------------------------------------------------------- | ---------------- |
| 296   | the section exists as an exact heading in a **sibling file** carrying the same RFC number | a mechanical fix |
| 71    | the section exists as an exact heading in `docs/`, `missions/`, or `CLAUDE.md`            | a mechanical fix |
| 933   | the section exists nowhere, under exact matching                                          | author intent    |

## Why 296 exist at all: RFC numbers are not unique

**28 RFC numbers are carried by more than one file — 110 files in total.**

| RFC  | Files |
| ---- | ----- |
| 0011 | 26    |
| 0850 | 8     |
| 0855 | 8     |
| 0903 | 5     |
| 0959 | 4     |
| 0960 | 4     |
| 0206 | 4     |

`find_rfc_path` resolves a bare cite to exactly one of them by a sort
heuristic. `RFC-0011` resolves to `0011-x-wallet-store-identity-substrate.md`,
so a cite naming a section that lives in any of the other 25 files is reported
as broken because the wrong file was opened.

This is the same shape as every other finding in this area: the check opened one
file, concluded the section was missing, and reported a fact about the wrong
population.

Measured across the 28 duplicated numbers, 2928 of 3502 heading names are
present in exactly one file and so are disambiguated by searching siblings.
The 574 shared names are dominated by boilerplate — `Summary`, `Status`,
`Authors`, `Version History` — where the citation was never specific to begin
with.

## Why the substring class is noise, and must not be "fixed"

An earlier count credited 559 findings to substring matches in `docs/` and
`missions/`. Sampling them shows what that class contains:

| Cited section             | Matched against                                                        |
| ------------------------- | ---------------------------------------------------------------------- |
| `§out`                    | `manual setup (without the installer)`                                 |
| `§dual`                   | `🏘️ individual user`                                                   |
| `§dqa`                    | `1. deterministic numeric tower (rfc-0104 (dfp) + rfc-0105 (dqa) + …)` |
| `§bootstrap orchestrator` | `q6. bootstrap orchestrator`                                           |

Two-character fragments matching any heading that happens to contain them. This
is the same leniency `lint_section_cites.sh` measures explicitly in its
coverage line, and the reason its substring semantics are documented as
"generous by design".

**Absorbing these would turn the linter into something that cannot fail.** A
guard that resolves `§out` against `without the installer` is worse than one
that reports the citation, because it teaches readers that a green result means
the citation resolves.

So the substring class is recorded as unresolved, not as recoverable.

## What a mechanical fix would look like

Search every file carrying the cited RFC number, not one; and index headings
from `docs/`, `missions/`, and `CLAUDE.md` the way `lint-rfc-anchors.py`
already does. That is a change to what this linter accepts, so it is a
maintainer decision — but it is a bounded one: 367 findings, no interpretation
required, and the change is to the resolver rather than to the corpus.

The durable fix for the remaining 933 is unique RFC numbers or unambiguous
citations, which is a corpus change and needs author intent.

## Also unexamined

- 339 `PHANTOM [RFC missing]` — cites whose RFC number resolves to no file at
  all. Not yet classified.
- 1007 `STALE [version pin mismatch]` — see
  `one-thousand-nine-hundred-rfc-references-carry-a-version-pin-and-only-a-fifth-are-cross-references`.
- 841 of 1263 tracked markdown files are unexamined, and 789 of those are
  missions. The linter reports this rather than hiding it.

## Corrected history

This mission's own title carried the figure 436. That number came from a
classification that treated substring matches as recoveries. The correct figure
under exact matching is 933. Both numbers describe the same 1300 findings; they
differ in what counts as evidence that a citation resolves.
