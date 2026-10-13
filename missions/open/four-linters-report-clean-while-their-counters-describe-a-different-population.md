---
name: four-linters-report-clean-while-their-counters-describe-a-different-population
description: "Four check scripts in scripts/ each report a verdict that their own counters contradict. lint_no_vh_accepted.sh counted every markdown file in CHECKED but only detected Accepted RFCs in PASS, hiding that it examined 7 of 155. lint_status_vh_sync.sh read the oldest Version History row as the newest. mutation-verify-guards.sh treats any argument as a name filter, so an unmatched filter yields passed=0 with no warning. lint_section_cites.sh guarded on ACCEPTED, a classification result rather than a count of work, and reported an empty run after examining every cite. Separately, the two cite linters together examine 10281 of 22169 tracked cite tokens."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `four-linters-report-clean-while-their-counters-describe-a-different-population`

## Status

Open. Three of the four counter defects are fixed. The cite coverage gap
below is recorded and not touched.

## The shape

Four check scripts report a verdict that their own output contradicts. In each
case the script is not lying about the number it prints; it is printing a
number that describes a different population than the reader assumes. That is
why every one of them read as clean.

| Script                      | What the output implies     | What actually happened                                  |
| --------------------------- | --------------------------- | ------------------------------------------------------- |
| `lint_no_vh_accepted.sh`    | 247 Accepted RFCs scanned   | 7 examined; 148 skipped on a failed detection regex     |
| `lint_status_vh_sync.sh`    | newest version compared     | oldest version compared                                 |
| `mutation-verify-guards.sh` | 10 guards verified          | any unmatched filter yielded `passed=0` and exit 0      |
| `lint_section_cites.sh`     | a non-zero count means work | `ACCEPTED` is a classification result, not a work count |

## Fixed: `lint_no_vh_accepted.sh`

Detection required `Accepted` on the same line as the status heading. The
corpus writes it on the following lines, so 148 of 155 Accepted RFCs were
skipped and the summary still read PASS.

The gap was hidden by the counters. `CHECKED` incremented once per markdown
file found, before filtering; `PASS` incremented only for detected Accepted
RFCs. `CHECKED=247` next to `PASS=7` looks like unexamined work. It was a
silent skip.

Now reports `SCANNED=247 markdown files, CHECKED=155 Accepted RFCs`, and the
verdict is `FAIL` with 51 real findings.

## Fixed: `lint_section_cites.sh`

The empty-corpus guard tested `ACCEPTED`:

```bash
if [ "$ACCEPTED" -eq 0 ]; then
    echo "FAIL: no files were examined"
```

`ACCEPTED` counts cites that classified as pure numeric anchors. A corpus of
alphabetic cites (`§Identity`) is fully examined and still reports zero,
because non-canonical cites take a different branch. Confirmed on a two-line
corpus: two cites examined, then `FAIL: no files were examined`.

The guard now tests `SCANNED` and `EXAMINED` separately, so an empty root still
fails, a root of files containing no cites says so as a real result, and only a
root with no files claims the lint verified nothing. The summary also gained
the two counters it never had.

## Fixed: `mutation-verify-guards.sh` treated any argument as a filter

```bash
if [ -n "$FILTER" ] && [[ "$name" != *"$FILTER"* ]]; then
    SKIP=$((SKIP + 1))
    continue
fi
```

Running it with `--help` reported `passed=0 skipped=10` and exited 0. There was
no help output, because `--help` matched no guard name and every guard was
skipped. The summary looked like a result.

With no argument it passed 10/10. The `--help` case is narrow, but a typo in a
guard name failed the same way: the script verified nothing and exited 0.

Both halves are now fixed.

- `-h` / `--help` prints usage and lists every available guard, so the correct
  name is discoverable instead of guessed.
- A filter that matches no guard exits 1 with `FAIL: filter ... matched no
guard`, and says plainly that the run verified nothing.
- A partial filter still exits 0 but prints how many guards it skipped and by
  which filter, so a narrowed run can never be mistaken for a full one.

Verified in all four directions:

| Invocation               | Before                         | After                                  |
| ------------------------ | ------------------------------ | -------------------------------------- |
| no argument              | 10/10, exit 0                  | 10/10, exit 0                          |
| `workflow` (real filter) | 1 pass, 9 skipped, exit 0      | 1 pass, 9 skipped, exit 0, with a note |
| `build_ouptut` (typo)    | 0 pass, 10 skipped, **exit 0** | exit 1, explicit failure               |
| `--help`                 | 0 pass, 10 skipped, exit 0     | usage + guard list, exit 0             |

The second and third rows are the point: identical `passed=0 skipped=10`
output, opposite meanings, and the exit code now distinguishes them.

## Coverage gap: the cite linters see a third of the cites

Measured over git-tracked markdown, excluding `.claude/worktrees/`:

| Location                  | `§` tokens |
| ------------------------- | ---------- |
| whole repository          | 22,169     |
| `rfcs/`                   | 10,281     |
| `docs/`                   | 10,520     |
| `missions/`               | 8,708      |
| `CLAUDE.md` + `README.md` | 12         |

Counts are over git-tracked markdown, excluding `.claude/worktrees/`. The
sub-totals exceed the repository total because the per-directory counts were
taken over the working tree, which still contains worktree copies under
`.claude/`; the tracked-file total is the authoritative one.

`lint_section_cites.sh` and `validate_cites.sh` both default to `RFC_ROOT=rfcs`.
Together they examine **10,281 of 22,169** tracked cite tokens — 46%. The
remainder lives in `docs/`, in mission files, and in the two top-level
governance documents that every agent reads first.

Neither linter printed a total, so the gap was not visible in any output. It
became visible only by counting the repository directly.

Widening the scope is not a mechanical change: `validate_cites.sh` validates
`RFC-XXXX §section` shapes, and the non-RFC corpus contains a different mix of
anchors, including bare section names with no RFC number at all. Widening it
would change what it rejects, which is a maintainer decision.

## Acceptance criteria

- [ ] A decision is recorded on the cite linters' corpus: `rfcs/` only, or
      widened, with the widened set measured rather than assumed.
- [ ] Every check script prints the population it examined alongside its
      verdict, so a scope change is visible without reading the script.
- [ ] No linter derives an "did any work" verdict from a classification result.

## Notes

The unifying question for future review is not "is this lint correct" but
**"does this counter describe the population the verdict is about"**. All four
were individually correct arithmetic over the wrong set.

`scripts/lint-rfc-anchors.py` and `scripts/lint-rfc-type-refs.py`, the two
linters actually wired into CI, both print their corpus size and their
baseline coverage. Neither defect in this mission could survive that. That is
the difference, and it is worth copying rather than fixing case by case.
