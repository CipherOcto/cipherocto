---
name: rfc-numbers-are-not-unique-so-find-rfc-path-resolves-by-directory-order
description: "RFC numbers are reused across category directories, and validate_cites.sh find_rfc_path resolves a bare RFC-XXXX cite by preferring accepted/ and then sorting by version. Measured blast radius: 296 of the linter 1300 INVALID findings are a section that is an exact heading in a sibling file carrying the same number, and a further 71 are an exact heading in docs/ or missions/. Re-measured under exact-heading matching, which SUPERSEDES the earlier 625/436 split: that split credited 293 substring-only matches as recoveries, and sampling them shows they are noise, matching fragments such as out against manual setup without the installer and dual against individual user. Those 293 are unresolved, not recovered. 28 RFC numbers are carried by more than one file, 110 files in total, and RFC-0011 alone names 26. Which file a citation is checked against therefore depends on which directories happen to contain a file with that number."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `rfc-numbers-are-not-unique-so-find-rfc-path-resolves-by-directory-order`

## Status

Open. Not fixed. Nothing has been deleted, renumbered, or rewired.

## The finding

RFC numbers are not unique across the tree. `find_rfc_path` in
`scripts/validate_cites.sh` assumes they are.

```bash
accepted=$(echo "$matches" | grep "/accepted/" | grep -ivE "/${prefix}-(a|d|r)[0-9]+-" || true)
if [ -n "$accepted" ]; then
    local latest
    resolved=$(echo "$accepted" | sort -t- -k2 -V -r | head -1 || true)
```

Selection is: files under `accepted/`, excluding sub-amendment filenames, then
lexicographically latest by version. There is no check that the chosen file is
the one the citing document means.

`RFC-0105` resolves to three files:

- `rfcs/accepted/numeric/0105-deterministic-quant-arithmetic.md`
- `rfcs/accepted/economics/0105-v35-payment-caveat-asset-generality.md`
- `rfcs/accepted/economics/0105-v34-private-asset-namespace.md`

A numeric RFC and an economics RFC share the number. They are unrelated
documents. Every citation of `RFC-0105` is checked against the economics one.

## Consequence

A section that genuinely exists is reported missing. Example:
`rfcs/draft/numeric/0131-bigint-to-dqa.md` cites `RFC-0105 §Canonical
Representation`. That heading exists — but in
`rfcs/accepted/numeric/0105-deterministic-quant-arithmetic.md`, not in the
economics file the resolver selects.

Measured against a completed run of `scripts/validate_cites.sh` by
`scripts/classify_validate_cites.py`, which reads the linter's own output and
builds each file's heading list with the same command `get_rfc_sections` uses:

| Class of INVALID finding                                       | Count   | Verdict                 |
| -------------------------------------------------------------- | ------- | ----------------------- |
| section is an exact heading in a sibling carrying that number  | **296** | mechanically resolvable |
| section is an exact heading in docs/ or missions/ or CLAUDE.md | 71      | mechanically resolvable |
| name occurs only INSIDE a heading in a sibling (substring)     | 293     | **not a recovery**      |
| genuinely missing, absent from every file with that number     | 436     | author intent           |
| near-miss: dropping the trailing word resolves it              | 113     |
| near-miss: a two-word prefix is a heading                      | 60      |
| identifier-qualifier rule would resolve it                     | 34      |
| remaining qualifier cases the rule does not help               | 23      |

**367 of 1,300 INVALID findings — 28% — are mechanically resolvable**, being the
wrong file being opened, and 933 need author intent.

This **supersedes the earlier 625/436 split**, which credited 293 substring-only
matches as recoveries. Sampling that class shows what it contains:

| Cited section | Matched against                                                          |
| ------------- | ------------------------------------------------------------------------ |
| `§out`        | `manual setup (without the installer)`                                   |
| `§dual`       | `🏘️ individual user`                                                     |
| `§dqa`        | `1. deterministic numeric tower (rfc-0104 (dfp) + rfc-0105 (dqa) + ...)` |

Two-character fragments matching any heading that happens to contain them.
Absorbing those would convert 293 honest failures into silent passes and make
the linter unable to fail, which is the opposite of what this mission asks for.
So they are recorded as unresolved, not recoverable.

The remaining 933 are tracked in mission
`four-hundred-and-thirty-six-section-citations-name-a-section-that-does-not-exist`,
whose title carries the older figure and is superseded by this section.

**The duplicate-number blast radius, measured directly:** 28 RFC numbers are
carried by more than one file, 110 files in total. RFC-0011 names 26, RFC-0850
and RFC-0855 name 8 each, RFC-0903 names 5. `RFC-0011` resolves to
`0011-x-wallet-store-identity-substrate.md`, so a cite naming a section that
lives in any of the other 25 files is reported as broken because the wrong file
was opened.

Figures in the table above are from a re-measurement against the current tree.
Earlier figures quoted here (332 / 625 / 436) came from a classifier that
credited substring matches, and are superseded.

> The earlier figure in this section, 117 of 577 "newly-flagged unique
> findings", came from the simulation that produced most of this session's
> wrong numbers. It is superseded by the table above.

## Why this is not fixed here

The obvious remedy — check the citation against every file sharing the number,
and pass if any contains the section — was measured and rejected. It would
rescue 402 findings by weakening the lookup until something matched. The
rescued cases are real dangling cites: `§Gas Model` resolving against a `Gas`
heading, `§Writer Election` against `Writer`, `§Key Derivation` against `Key`.
That converts an honest failure into a silent pass and hides the defect rather
than removing it.

Measuring the split makes the same rejection concrete rather than theoretical.
The findings in which a sibling carries **that exact heading** are genuine
recoveries: `RFC-0105 §Canonical Representation` names a real `### Canonical
Representation` in `0105-deterministic-quant-arithmetic.md`, verified by hand.
The ones in which the name merely occurs _inside_ a sibling heading are not
recoveries at all. A citation of `§Phase` is not satisfied by a heading reading
`Phase 1: Data Structures + Algorithms` — that is a different section being
substituted for the one that was written, which is precisely the failure mode
the rejected remedy would institutionalise.

So a `pass if any sibling matches` rule would convert roughly 293 honest
failures into silent passes while fixing roughly 332 real ones, and the ratio alone would make it
look like progress.

The durable fix is at the other end: make the number unique, or make the
citation carry enough information to be unambiguous. Both are decisions with
repo-wide blast radius.

## Acceptance criteria

- [ ] A decision is recorded on whether RFC numbers become globally unique or
      citations carry a category/disambiguator.
- [ ] If numbers become unique, the duplicate set is enumerated and resolved,
      and every citation is updated in the same change.
- [ ] `find_rfc_path` fails closed when a bare cite resolves to more than one
      candidate, rather than silently choosing one by sort order.
- [ ] The 117 findings are re-measured after the change and the count is
      reported, not assumed.

## Notes

This was found by measuring the effect of an unrelated fix, not by reading the
resolver. The citation regex had a dead constant and truncated every
multi-word section name; fixing it changed 307 findings from flagged to clean
and 577 from clean to flagged. Classifying the new findings by hand against the
real headings of each named RFC is what separated "the section does not exist"
from "the section exists in a different file with the same number."
