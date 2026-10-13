---
name: the-mutation-harness-is-not-run-by-any-ci-workflow-so-a-guard-can-be-made-vacuous-silently
description: "scripts/mutate-zk-guards.py holds 73 mutations covering 59 of the 65 guards in security_regressions.rs, every one verified red. No workflow under .github/ invokes it, and the red-team audit's claim that the guards are mutation-checked therefore rests on a manual artifact nothing re-runs. A future edit that neuters a guard — an if false in front of the branch, a renamed needle, a table emptied — passes every gate in CI today. The full table measures 3m17s and 1.79 GB peak resident, so the cost is real but affordable; wiring it is owner policy."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `the-mutation-harness-is-not-run-by-any-ci-workflow-so-a-guard-can-be-made-vacuous-silently`

## Status

Open, **owner decision**. It proposes a CI step and changes a workflow file,
which is owner policy surface. Nothing here edits `.github/`.

## The finding

The red-team audit records that "every guard added in this wave was
mutation-checked: a defect was planted, the guard was observed to go red, and
the fix was restored", and names `scripts/mutate-zk-guards.py` as the way to
replay it. That is true, and it was true when written — the harness had 46
entries.

It now has **73 entries covering 59 of the 65 guards**, every one verified red.
**No workflow under `.github/` invokes it.** A repository-wide search for
`mutate-zk-guards` finds exactly three hits: the audit, the mission that built
it, and the follow-on that extends it. None of them is a CI step.

So the audit's central claim about how the guards were validated is a
**manual artifact**. Nothing in CI re-establishes it, and nothing in CI would
notice if it stopped being true.

### Why that matters more than "a script isn't in CI"

Every mutation in the table exists because a check was found to pass while
measuring nothing. Three of them were found _this session_, each of which would
today sail through every CI gate:

| Defect                                                                                                                                                                                  | What CI sees |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------ |
| `l7_active_symlink_swap_is_atomic` built its haystack with a whitespace-preserving join and searched it for a needle containing none — the assertion could not fail for any Rust source | green        |
| The AC-11 key-shape guard skipped any key argument that was not a bare identifier, so an inline `&[0u8; 32]` at a `verify_dispatch` call passed                                         | green        |
| Adding an algorithm dependency to `octo-ident` is refused by cargo as a cyclic package dependency — the layer rule is already enforced one level down                                   | green        |

Each was found by a mutation, and each is invisible to `cargo test`, to clippy,
to both linters, and to review at a glance. **A guard that has been neutered
looks exactly like a guard that was never written.** The harness is the only
thing in this repository that can tell those two apart, and it is the only
thing that is not wired to run.

### The cost, measured

Not estimated. The 24-entry figure recorded in the closed mission — "55 s,
0.9 GB" — was accurate when written and **understates the current table by
about 3.5× in time**. Measured on the 73-entry table:

```
mutations: 73  red: 73  vacuous: 0  bad-mutation: 0  bad-anchor: 0
Elapsed (wall clock) time: 3:17.10
Maximum resident set size:  1,790,996 kbytes
disk before 6.8 GB free / after 6.9 GB free — no measurable growth
```

Three and a quarter minutes and 1.79 GB peak resident, which is mostly `cargo`
rather than the harness. The disk cost is the part that usually decides these:
there is essentially none, because the mutations that rebuild recompile one
crate whose artefacts are already warm.

## What is proposed

**Owner decision, not an implementation.** The proposal, for whoever owns the
workflows:

1. A workflow step running `python3 scripts/mutate-zk-guards.py`, on a schedule
   and on demand, rather than on every push. At 3m17s per run it does not belong
   in the per-PR path, and the file-scanning entries make a partial check
   possible at a fraction of the cost via the existing `--only` and `--cheap`
   flags.
2. **The cheap half first.** 35 of the 73 entries mutate a file outside
   octo-cli's dependency closure or a script cargo never reads, and cost
   seconds. Running `--cheap` in the per-PR path and the full table nightly
   would catch a neutered file-scanning guard immediately and a neutered
   compiled one the same day.
3. Existing disk mitigations apply unchanged: `ci.yml` already sets
   `CARGO_PROFILE_{DEV,TEST}_DEBUG=0` and `CARGO_INCREMENTAL=0` and has a
   documented "Free disk space" step for exactly this class of exhaustion.
4. **The harness's own exit code is the signal.** It returns 1 on any
   VACUOUS, BAD-MUTATION or BAD-ANCHOR, so the step needs no output parsing — and
   BAD-MUTATION matters as much as VACUOUS here, because an entry that stopped
   compiling is an entry that stopped testing anything.

## Why this is filed rather than done

Workflow files are owner policy surface, and CI wall-clock and disk budget are
the owner's call. The measurements above are offered so the decision can be made
on numbers rather than on the same unmeasured guess this arc has now retracted
three times.

## Acceptance Criteria

- [ ] An owner decision is recorded: wire it, schedule it, or decline it with a
      reason
- [ ] If wired, the step runs `--cheap` at minimum, and the full table on a
      schedule
- [ ] The audit's "every guard was mutation-checked" sentence gains a pointer to
      wherever the harness is actually run — or loses the claim if it is not run
      anywhere
- [ ] The cost recorded here (3m17s, 1.79 GB peak, no disk growth for 73
      entries) replaces the 24-entry figure wherever it is still quoted as
      current

## Method note

The three-minute figure is a wall-clock measurement of one full run with
`/usr/bin/time -v`, on the same machine, immediately after a run of the same
table that reported 73 red. The disk figures are `df` before and after. Both
are single measurements, not an average over repeats — stated as one run rather
than dressed up as a benchmark.
