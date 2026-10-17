---
name: three-hundred-and-thirty-nine-phantom-citations-are-ninety-one-percent-pre-renumbering-references
description: >-
  validate_cites.sh reports 339 PHANTOM findings for RFC numbers that resolve to no file. Measured: 308 are references to the pre-category-renumbering 0xxx numbers, often inside an explicit renumbering note stating the change, and 15 are genuine external standards that share the RFC- prefix such as RFC-3339, RFC-4122 and RFC-5869. Sixteen fall outside both classes. Neither class is a typo to repair, so mass-editing them would corrupt historical record and external references alike.
metadata:
  node_type: mission
  type: tooling
  created: 2026-10-13T00:00:00.000Z
  v: "1.0"
  depends_on:
    - four-hundred-and-thirty-six-section-citations-name-a-section-that-does-not-exist
---

# Mission `three-hundred-and-thirty-nine-phantom-citations-are-ninety-one-percent-pre-renumbering-references`

## Status

Classified. **Not fixed, and deliberately so** — no repair here is safe without
author intent. The measurement is what makes that decision possible.

## The measurement

From a full `bash scripts/validate_cites.sh` run against the current tree:

```
Summary: CHECKED=29273 VALID=26627 PHANTOM=339 INVALID=1300 STALE=1007
```

The 339 `PHANTOM [RFC missing]` findings, across **66 distinct citing files**:

| Count | Class                                               | Share |
| ----- | --------------------------------------------------- | ----- |
| 308   | reference to a **pre-renumbering `0xxx` number**    | 91%   |
| 16    | external standard **plus identifier collisions**    | 5%    |
| 15    | external standard sharing the `RFC-` prefix, mostly | 4%    |

## Class C: pre-renumbering references (308)

CipherOcto renumbered RFCs into category bands. RFC-0156 became RFC-0956, and
the archived files say so in their own text:

> **Note:** This RFC was renumbered from RFC-0156 to RFC-0956 as part of the
> category-based numbering system.

The citation sits _inside that note_. Rewriting it to `RFC-0956` would delete
the only statement of what the RFC used to be called.

Where the old number appears without a note, it is still a historical reference
rather than a typo — the citing file's own number is in the 0900s, and the cited
one is in the 0100s, which is the old band, not a mistyped digit.

A digit-swap repair was measured and rejected:

- 28 of the 48 distinct numbers are one transposition away from a real RFC
- **only 20 are unambiguous** — the rest reach several real RFCs, e.g. `RFC-0100`
  is equally reachable from `0001` and `0010`
- and the premise is wrong anyway, per the renumbering notes above

Fixing 20 citations by arithmetic while 288 share the same cause would be
arbitrary. Repairing all 308 mechanically would corrupt the historical record.

## Class A: external standards (15)

These are real, correctly cited, and nothing to do with CipherOcto:

| Cited                 | Standard                             |
| --------------------- | ------------------------------------ |
| `RFC-3339`            | date and time on the Internet        |
| `RFC-4122`            | UUID                                 |
| `RFC-5869`            | HKDF                                 |
| `RFC-6979`            | ECDH                                 |
| `RFC-8032`            | Ed25519                              |
| `RFC-952`, `RFC-1123` | DHC, Requirements for Internet hosts |

Seen in context:

```
pub generated_at: String,   // RFC-3339 UTC, e.g. "2026-08-29T03:14:15.123Z"
```

The linter resolves every `RFC-NNNN` against `rfcs/`, so an IETF citation is
indistinguishable from a typo. **These are not defects.** A finding that asks a
reader to "fix" a correct RFC-4122 UUID reference is a false positive, and the
fix belongs in the linter's classification, not the corpus.

## The 16 that are neither

Listed individually rather than left as a remainder, because three of them are
real findings hiding in a class I had assumed was noise:

| Count | Cited                                 | Verdict                                                                                                 |
| ----- | ------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| 8     | `RFC-9591`                            | **typo for `RFC-0959`**, an extra digit. The only genuinely repairable class here.                      |
| 3     | `RFC-1`, `RFC-2`, `RFC-3` in RFC-0201 | matched inside `XRFC-1`, `XRFC-2`, `XRFC-3` — review-finding identifiers, not citations                 |
| 1     | `RFC-8785`                            | text reads `RFC-8785 crate reference`, and **no crate of that name exists**. A real dangling reference. |
| 1     | `RFC-6962`                            | external (Certificate Transparency)                                                                     |
| 1     | `RFC-8785` in RFC-0909                | as above                                                                                                |
| 1     | `RFC-016-a`                           | malformed sub-amendment suffix in the operator guide                                                    |
| 1     | `RFC-9591-compliant`                  | the same typo, with a suffix                                                                            |

A hypothesis was tested and rejected here: that `XRFC-1` and `RFC-8785` were
matches inside longer identifiers, which would have been a word-boundary defect
in the cite regex. Measured directly — **0 of the 339 findings match `RFC-`
inside a word**, because the regex is applied per line without a lookbehind and
reports the offset of the match within it. The `XRFC-1` case is a genuine
identifier collision, not a regex bug.

## Why nothing was changed

Both large classes are cases where the citation is right and the finding is
wrong. The linter cannot currently tell them apart, so:

- repairing class C destroys history,
- repairing class A destroys correctness,
- and suppressing both in the linter is a policy decision about what
  `validate_cites.sh` is for.

That is a maintainer decision and is left open.

## The cheapest correct step, if it is wanted

Distinguish the two populations in the linter rather than in the corpus:

1. a cited number in a retired band (`0xxx`) is reported as `HISTORICAL`, not
   `PHANTOM`, so the two are never confused;
2. `RFC-NNNN` where `NNNN` is a known external standard number is excluded from
   RFC resolution entirely.

Both narrow what the linter claims about the corpus. Neither changes a
document.

## Related

- The 1300 `INVALID` findings are classified in
  `four-hundred-and-thirty-six-section-citations-name-a-section-that-does-not-exist`
  and `rfc-numbers-are-not-unique-so-find-rfc-path-resolves-by-directory-order`:
  367 mechanically resolvable, 933 needing author intent.
- The 1007 `STALE` version-pin findings are tracked in
  `one-thousand-nine-hundred-rfc-references-carry-a-version-pin-and-only-a-fifth-are-cross-references`.
- 841 of 1263 tracked markdown files remain unexamined by this linter, and the
  run says so.
