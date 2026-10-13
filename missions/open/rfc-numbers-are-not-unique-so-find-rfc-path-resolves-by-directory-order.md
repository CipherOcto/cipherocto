---
name: rfc-numbers-are-not-unique-so-find-rfc-path-resolves-by-directory-order
description: "RFC numbers are reused across category directories, and validate_cites.sh find_rfc_path resolves a bare RFC-XXXX cite by preferring accepted/ and then sorting by version. RFC-0105 names three files in two categories. Which file a citation is checked against therefore depends on which directories happen to contain a file with that number, so 117 newly-flagged findings report a section as missing when it exists in a real RFC with the same number."
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

Measured against the completed full-tree run of `scripts/validate_cites.sh`:
**117 of the 577 newly-flagged unique findings** are of this shape, where the
wanted section exists in another file carrying the same RFC number. The
pre-existing baseline contains **63** of the same class, so the ambiguity is not
new — it was simply less visible while the citation regex was truncating most
section names to a single word and reporting INVALID for an unrelated reason.

## Why this is not fixed here

The obvious remedy — check the citation against every file sharing the number,
and pass if any contains the section — was measured and rejected. It would
rescue 402 findings by weakening the lookup until something matched. The
rescued cases are real dangling cites: `§Gas Model` resolving against a `Gas`
heading, `§Writer Election` against `Writer`, `§Key Derivation` against `Key`.
That converts an honest failure into a silent pass and hides the defect rather
than removing it.

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
