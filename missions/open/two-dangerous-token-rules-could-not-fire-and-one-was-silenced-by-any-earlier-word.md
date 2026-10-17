---
name: two-dangerous-token-rules-could-not-fire-and-one-was-silenced-by-any-earlier-word
description: "scripts/wrappers/dangerous-tokens.sh is the single source of truth for the seven-layer defence against the 2026-09-08 commit-message incident. Three of its tokens could never fire at all, and its base64 rule reported a verdict for the whole message while examining one word. The canonical suite passed 56 of 56 over both defects."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-14T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0011-c
---

# Mission `two-dangerous-token-rules-could-not-fire-and-one-was-silenced-by-any-earlier-word`

## Status

Both defects are **fixed, bounded, and mutation-verified**. The canonical suite
is now 68 of 68 and fails 8 of the new cases against the pre-fix library.

This is not a hardening proposal. Three tokens were dead code and one rule
reported on a population it never examined. Both were found by running the
library on inputs the suite did not contain.

## The finding

`scripts/wrappers/dangerous-tokens.sh` is described in its own header as the
**single source of truth** for the prevention framework, shared by all seven
layers (commit-msg hook, pre-receive hook, safe-commit wrapper, the bash guard,
the CI lint workflow, and the memory card). Every layer inherits its defects.

### Defect 1 — `>`, `<` and `!` could never fire

Three tokens were written as:

```bash
'>')
  [[ "$text" =~ $'^\>' ]] && hit+=("leading>")
```

`$'...'` is ANSI-C quoting, which is still a **quoted** expansion. Bash's `=~`
treats a quoted right-hand side as a literal string, not a pattern. The pattern
is therefore the three bytes `5e 5c 3e` — caret, backslash, greater-than — and
no text begins with that.

The token list documents the intent as _"redirect stdout (matched at line start
in `dangerous_match`)"_. The code matched nothing, at string start or anywhere.

Measured, not inferred:

| Input                                | Expected | Actual  |
| ------------------------------------ | -------- | ------- |
| `> out.txt`                          | caught   | escaped |
| `Subject line` + blank + `> out.txt` | caught   | escaped |
| `Subject line` + blank + `!note`     | caught   | escaped |

The same three tokens also failed to fire at string start, which is what makes
this dead code rather than a narrowing.

### Defect 2 — the base64 rule examined one word and judged the message

The Gap I branch read a single capture:

```bash
if [[ "$text" =~ ([A-Za-z0-9+/=]{32,}) ]]; then
    local blob="${BASH_REMATCH[1]}"
```

`BASH_REMATCH[1]` is the **leftmost** 32+ character run. Everything after it was
never examined, while the verdict was reported for the entire message.

A 40-character git SHA is pure `[A-Za-z0-9]` and matches the run pattern. A
commit that cites the SHA it fixes therefore silences the rule for every payload
that follows it:

| Message                                           | Result  |
| ------------------------------------------------- | ------- |
| `artifact: <payload>`                             | caught  |
| `Fixes <40-hex-sha>. artifact: <payload>`         | escaped |
| `artifact: <64-char harmless blob> and <payload>` | escaped |
| `artifact: <payload>. Fixes <40-hex-sha>`         | caught  |

The verdict depends on unrelated text that appears earlier in the message, not
on the payload. This is the fail-open shape that runs through most of this
repository's guards: a result reported about a population that was never
examined.

## Why 56 of 56 passed

The suite is sound; the corpus was not.

- **No case began with `>`, `<` or `!`.** Three dead rules and zero coverage.
- **One base64 case existed** and put the payload first — the single ordering
  under which the rule worked.

Adding twelve cases that fail against the pre-fix library turns 56 into a
meaningful number. Four of the twelve pass against both versions by design:
the pre-existing working case, and three false-positive guards asserting that a
`>` or `<` which is _not_ at line start stays clean.

## Why no linter caught it

No workflow runs shellcheck over `scripts/`. The only shellcheck in CI belongs
to the `rhysd/actionlint` step in the lint workflow, which checks the `run:`
blocks inside workflow YAML. It never reads a shell script in this repository.
The defect class is exactly what a `=~` right-hand-side check exists to catch.

Installing shellcheck was not attempted: the disk has 3.5G free and no local
install was available, so **this claim is reasoned from the workflow contents,
not verified by running shellcheck.**

## Three implementations tried, one shipped

The obvious fix for defect 2 — loop in bash and strip the matched blob — was
measured and thrown away twice before the shipped form was found.

| Implementation                                  | 20 000 runs | Verdict  |
| ----------------------------------------------- | ----------- | -------- |
| `${rest#*"$blob"}` (glob-based strip)           | hung        | rejected |
| `^(.*)([A-Za-z0-9+/=]{32,})` + `${var:off:len}` | hung        | rejected |
| one `grep -oE` fork, then iterate the array     | 3.8s        | shipped  |

A single 1 MB run took 4.3s on the **pre-fix** library and over 60s on both
bash-only walks. Bash's glob matcher is quadratic in the pattern, and the POSIX
matcher backtracks. `grep -oE` is linear and is one fork.

The decode count is bounded at **64 candidates**, and exceeding the bound
**fails closed** with a named hit rather than passing. A bound that silently
stopped examining would reintroduce defect 2 one line above itself. Measured on
20 000 mixed-case runs: **45.7s unbounded, 3.8s bounded**.

## The bound is not load-bearing for detection, only for cost

Removing the budget entirely still detects a payload buried at run 5 000 — it
just takes 45.7s. Two claims about the budget were wrong before being measured:

- The budget was first reported as not load-bearing, because the test blobs were
  uppercase-only and the `[a-z]` pre-filter short-circuited before `base64 -d`
  ever forked. The test never reached the code it claimed to measure.
- Re-tested with mixed-case blobs, which pass the pre-filter, the 12x difference
  is real.

## Scope

`grep -oE` extracts runs whose characters are all in `[A-Za-z0-9+/=]`, matching
the original pattern. One pre-existing limitation is unchanged and is not a
regression: a payload glued to a prefix inside a single word (`x=<payload>`) is
still missed, because the run includes the prefix and no longer decodes. Fixing
that changes matching semantics and risks false positives; it is recorded, not
attempted.

## A documentation table recorded the dead rule as passing

Found later, and it belongs here rather than in its own entry: this mission
concluded that the canonical suite was sound and its corpus incomplete. A
second artefact made the same false claim.

`scripts/wrappers/safe-commit-skill.md` carried a ten-row verification table
with a **Got** column reading `OK` on every row, introduced as "Verified 10/10
cases pass". Row H was:

| #   | Input                | Want   | Got (as written) | Actual, pre-fix |
| --- | -------------------- | ------ | ---------------- | --------------- |
| H   | `> redirect attempt` | refuse | OK               | **allowed**     |

Every row was re-run against the pre-fix library. **Nine were honest; exactly
one was false**, and it was row H — the rule this mission is about. A
verification table had recorded a pass for a behaviour that could not occur.

The table now names the suites that actually exercise each row, cites
`LEADING_GT_TOP` for row H, and states plainly that the row was untrue until
the fix. It previously pointed at a "Layer 3 harness" that did not exist;
`scripts/test-safe-commit.sh` is the first, and it covers argument handling
rather than these rows.

The same shape as the original finding, one level up: a report of a result
about a check that was never run.

## A latent fail-open that is NOT reachable, tested rather than assumed

The commit-msg hook reads the library as `dangerous_present "$TEXT" || true`,
which conflates "no offenders" with "the scan failed". That is the same shape as
both defects above, so it was probed before being called a defect.

Nine adversarial inputs were run through the library under `set -u -o pipefail`:
empty string, a NUL byte, 100k characters, 5000 newlines, a 40-digit number, a
64-character uppercase run, a 64-character mixed-case run, 5000 multibyte
characters, and control characters. **All nine returned exit 1.** None produced
an abnormal status.

So the swallowed-failure path is **latent, not live**, and is left alone.
Distinguishing "failed" from "clean" would require the shared library to report
completion separately, which is an API change to a file seven layers depend on —
not justified by a path no input can reach.

## Cost on real input

Measured across all 554 commit messages in this repository: the longest
`[A-Za-z0-9+/=]` run is **28 characters**, so no real commit produces a single
candidate and the branch never fires. The fix is free on real input and bounded
on adversarial input.

## Acceptance criteria

- [x] `>`, `<` and `!` fire at line start, at string start and at body-line start.
- [x] Every 32+ run is examined, not only the leftmost.
- [x] The candidate loop is bounded and fails closed when the bound is exceeded.
- [x] Twelve cases added; eight fail against the pre-fix library.
- [x] All three protections mutation-verified independently.
- [x] The commit-msg hook refuses a leading-`>` message end to end.
- [ ] A shellcheck pass over `scripts/` is added to the lint workflow, so the
      next `=~` defect is caught by a tool rather than by inspection. This is
      owner-policy surface and belongs to a maintainer.

## Notes

Found by asking which enforcement scripts were outside every harness.
`mutation-verify-doc-guards.sh` covers ten document guards and
`mutation-verify-guards.sh` covers ten Rust guards; the two hook scripts and the
token library they both source had no coverage at all. The suite's own header
says _"If you add an entry, add a test case"_ — which binds token-list entries,
not the inline branches, and the inline branches are where both defects were.
