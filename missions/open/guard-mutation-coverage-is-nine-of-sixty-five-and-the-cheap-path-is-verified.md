---
name: guard-mutation-coverage-is-nine-of-sixty-five-and-the-cheap-path-is-verified
description: "The repo's mutation harness states that a guard which has never failed is not a guard, and that checking by hand produces a process claim nobody can re-run. Of the 65 guards in security_regressions.rs, 9 have a mutation in scripts/mutate-zk-guards.py; 56 have none. All 19 existing mutation anchors are still live. Guards that scan files at runtime can be mutation-tested against the already-built test binary with no rebuild at all, verified on no_test_has_an_empty_body. Separately, that harness's path-dependency guard skips section-form path dependencies, 3 of which exist in one manifest."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `guard-mutation-coverage-is-nine-of-sixty-five-and-the-cheap-path-is-verified`

## Status

Open. No guard has been changed. No mutation campaign has been run.

## The finding

`scripts/mutate-zk-guards.py` states its own case better than a mission could:

> A guard that has never failed is not a guard.

and then explains why it is a script rather than prose:

> Performing that by hand produces a _process claim_ — a statement about what
> someone did once — which is not checkable by anyone else afterwards.

That is the correct standard, and by it the guard suite is mostly unbacked.

|                                     | Count  |
| ----------------------------------- | ------ |
| Guards in `security_regressions.rs` | 65     |
| With a mutation in the harness      | **9**  |
| With none                           | **56** |

The 9 are the red-team remediation set: two `c1_`, one `h6_`, two `h7_`, three
`m11_`, and one `m4_`. Everything else in the suite — including the guards added
recently for dead dependency edges, empty test bodies, and placeholder decoders
— is protected only by whatever someone remembers doing.

## What is not wrong

The harness itself is healthy. All 19 of its mutations still have a live
anchor: for each one, the exact text it expects to replace is still present in
the file it names. Zero stale anchors, zero missing target files. A harness
whose anchors have rotted is a common failure and it does not apply here.

The three outcomes it distinguishes — red, bad-mutation, vacuous — are the right
three, and the bad-mutation case is explicitly called out as _not_ a red guard,
which is the distinction most mutation harnesses get wrong.

## The cheap path is verified

Most guards in this suite do not compute anything at test time. They read
files off disk and assert on their contents. For those, a mutation can be
verified **without rebuilding anything**: mutate the file the guard reads, then
run the already-compiled test binary against it.

Verified end to end on `no_test_has_an_empty_body`:

1. An empty-bodied test was appended to a source file.
2. The prebuilt `security_regressions` binary was invoked directly with a name
   filter. It went **red**, exit 101, naming the guard.
3. The file was restored. The same binary went **green** again.
4. The restore was confirmed with an empty diff.

No compile step, no linker, no `target/` growth. At the current disk budget that
is the difference between mutation-verifying a guard and not being able to.

### The hazard this introduces

A prebuilt binary can be **stale relative to the guard source**. If the guard
has changed and the binary predates it, a red result proves nothing about the
guard as it now stands, and a green result proves less still. Any harness built
on this path must fail loudly when the binary is older than the test source,
rather than quietly validating against a stale artifact. Getting that wrong
produces exactly the false confidence this whole exercise exists to remove.

## A blind spot in one of the guards

While probing, one mutation produced a surprise: `no_path_dependency_is_declared_and_never_referenced`
stayed **green** when an unused section-form dependency was planted.

That is the guard working as written, not a bug in the check. Its parse
deliberately skips a dependency spelled as its own table:

```toml
[dependencies.probe_unused_dep]
path = "../cipherocto-encoding"
```

because reading that bare `path =` line as a dependency _named_ `path` is what
made an earlier pass report a crate that does not exist. The section header
`dependencies.probe_unused_dep` also fails the guard's own table test, which
looks for a name ending in `dependencies`.

Measured size of the blind spot: the guard examines **305** inline-table path
dependencies and skips **3**, all in `crates/quota-router-storage/Cargo.toml`.
A grep of that table shows four section-form dependencies there in total. So
the guard covers roughly 99 percent of the corpus, and the uncovered remainder
is real dependencies rather than dead ones — all four are referenced by the
crate's own sources. Closing the gap should therefore land green, and needs
verifying rather than assuming.

## Acceptance Criteria

- [ ] The section-form path dependency is parsed, with the dependency name taken
      from the table header rather than from the `path` key, which is the whole
      hazard the current skip exists to avoid
- [ ] That change is verified in both directions: a planted unused
      section-form dependency goes red, and the four real ones stay green
- [ ] A mutation is added for each guard that reads files at runtime, starting
      with the ones whose names promise the most
- [ ] Any harness using the prebuilt-binary path refuses to run when the binary
      is older than the guard source, rather than reporting a result that
      proves nothing
- [ ] The guard count and the covered count are re-measured when this mission
      closes, and recorded the way the archived cite sweep recorded its own
      re-count requirement

## Why this is filed rather than fixed

The blind-spot fix is small and safe on the evidence above, but it needs a
recompile of the `octo-cli` test binary, and disk is the binding constraint at
roughly 8.8G free. Spending that on a one-percent coverage improvement before
the budget question is settled is the wrong trade, and the mission is worth
more intact than the fix is urgent.

The coverage gap is not urgent at all. Fifty-six unbacked guards is a large
number that reads as alarming and is not: these guards were written by
adversarial review and several were mutation-checked by hand during that work.
What is missing is the _re-runnable record_, which is what this harness would
supply — and the cheap path above shows it can be supplied without the disk
budget that the full harness assumes.

## Method note

The 9-of-65 figure counts distinct guard names appearing in the harness's test
commands against the 65 attributes in the suite, not test invocations. The
anchor check verified each mutation's expected text against its target file
directly. The blind spot was found by planting a real violation and observing a
green result, then reading the guard's parse to find out why — not by reading
the guard and speculating.
