---
name: the-trailing-punct-reject-could-not-match-anything-and-its-repair-exposed-a-greedy-extractor
description: Two of lint_section_cites.sh's three reject rules were dead, for two different reasons. The trailing-punct pattern used [.,/();:]], and grep -E reads the escaped closing bracket as an escaped terminator, so the class never closed and matched nothing at all. Repairing it produced 1431 findings, every one of them prose punctuation that a greedy extraction had absorbed into the cite token. The extractor was the real defect and is now fixed. Both are verified in both directions.
metadata:
  node_type: mission
  type: tooling
  created: 2026-10-13T00:00:00.000Z
  v: "1.0"
  depends_on:
    - five-checks-report-clean-while-their-counters-describe-a-different-population
---

# Mission `the-trailing-punct-reject-could-not-match-anything-and-its-repair-exposed-a-greedy-extractor`

## Status

Both defects fixed in `scripts/lint_section_cites.sh` and mutation-verified in
both directions. The guard is green, and it is green for a reason it can now
demonstrate.

The second defect is the more interesting one, because fixing the first created
the evidence for it.

## Defect one: a character class that matched nothing

Reject 1 was written:

```bash
if grep -qE '[.,/();:\]]$' <<< "$cite" 2>/dev/null; then
```

`grep -E` reads `\]]` as an escaped closing bracket, so the character class
never closes and the pattern matches **no string at all**. Bisected:

| Pattern       | Result          |
| ------------- | --------------- |
| `[.]`         | matches `.`     |
| `[.,]`        | matches `.`     |
| `[.;]`        | matches `.`     |
| `[(;)]`       | matches nothing |
| `[.,/();:]`   | matches `.`     |
| `[.,/();:\]]` | matches nothing |

The rule could never fire. It reported zero findings, which is exactly what a
clean corpus looks like — the two are indistinguishable from the output.

## Defect two: the extractor, not the rule

Repairing the class to `[]().,;:/]` made the rule fire, and it immediately
reported **1431 violations**. Every one turned out to be prose punctuation that
the _extraction_ had absorbed into the cite token:

| Extracted as   | Actually written as                                    |
| -------------- | ------------------------------------------------------ |
| `§caveats)`    | `(per RFC-0957 §caveats)`                              |
| `§3.2)`        | `(per RFC-0104 §3.2)`                                  |
| `§9c):`        | `` `DomainCoordinatorMisbehavior`, RFC-0855p-c §9c):`` |
| `§Algorithms.` | `... per RFC-0957 §Algorithms. Until the host`         |

The extraction was `§[A-Za-z0-9._/(),;:]+`, which includes `()`, `;` and `:` so
it swallowed the sentence's own closing paren into the token. Every downstream
rule — all three rejects, plus the accept/non-canonical classification — was
examining a token that was not a citation.

**Not one of the 1431 was a malformed citation.** All of them were correct prose.

So the honest fix was to the extractor, not the rule:

```
§/?[A-Za-z0-9]+([._/][A-Za-z0-9]+)*
```

A name is alphanumeric, and every separator must have alphanumeric characters on
both sides. That is the only way to tell `§2.1` (an internal separator) from
`§6.6.` (a sentence period) — both are the same character, and a single
character class cannot distinguish them.

Verified against the corpus: **10276 cite tokens before and after**, so nothing
was lost, only re-tokenised. All 1431 now extract without their punctuation.

## The leading-slash token that nearly went missing

Requiring an alphanumeric first character dropped `§/v1/realtime`, a real API
path in RFC-0846, and the only evidence was the examined count dropping from
10276 to 10275.

A single token is exactly the kind of loss a total is meant to surface, so it
was traced rather than accepted. The pattern now allows a leading `/`.

## What Reject 1 is now

The fixed extractor **cannot** produce a token ending in punctuation, so the
rejection rule became unreachable for a second time. Rather than delete it, it
is kept as an assertion on the extractor's shape: if the pattern is ever
loosened back to a greedy class, the check fires immediately and says the
extractor is absorbing prose again.

Verified: reverting the extractor produces 1431 findings with that diagnosis;
restoring returns `PASS`.

A rejection rule that cannot reject is worse than no rule, because it reads as
coverage. An assertion that fails loudly when its invariant breaks is the honest
form of the same check.

## A second rendering the extraction never matched

Repairing the extractor was not the end of it. The corrected pattern still did
not match the **quoted** citation name:

```markdown
See §"Roles and Authorities" for the normative rules.
```

Measured over the corpus: **264 cited sections are written this way, and the
guard extracted none of them.** A file containing only two quoted citations
reported

```
EXAMINED=0 cite tokens
NOTE: 1 file(s) scanned, none contained a §cite token.
PASS: 0 cite hygiene violations.
```

That is the failure this whole line of work exists to catch, produced by the
guard itself: a clean result, and a note claiming the corpus held no citations
while two sat in the file unread. The `EXAMINED=0` guard further down the script
exists precisely to catch an empty run — and it did not, because the run was not
empty, it had two citations the extractor could not name.

The pattern now reads a quoted name to its closing quote. `EXAMINED` moved from
10276 to **10536**, which is the 260 quoted names the corpus contains outside the
four counted during measurement, and rejections stayed at zero.

## Why this is the fourth instance

The same shape, four times over:

1. `lint_status_vh_sync.sh` read the Status version off a heading line that
   never carries one, skipping 210 of 247 files and still reporting PASS.
2. `lint_no_vh_accepted.sh` counted every markdown file against an
   Accepted-only total, so the gap looked like unexamined work.
3. Here, a reject rule that matched nothing produced a clean-looking summary.
4. And the extractor that rule examined could not read a quoted citation name,
   so 264 cited sections produced no token and no finding.

All four reported a result that described a population the guard never
examined. The fourth is the sharpest, because the guard printed a note saying
the file contained no citations while two sat in it unread.

## Follow-ups

- The remaining 12650 tracked section references outside `rfcs/` are still
  unexamined by this guard. It reports that as `COVERAGE=44%`, which is why
  widening it is a maintainer decision rather than a tooling detail.
- Reject 3 (phantom subsection tokens) now runs on correctly re-tokenised input
  for the first time. It reports zero, having been repaired earlier.
- Each reject rule here now has one harness case pinning it, via the clean
  fixture and the planted-defect direction. See
  `seven-numbered-guards-exist-and-none-of-them-are-mutation-verified`.
