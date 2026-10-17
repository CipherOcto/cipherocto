---
name: mutation-harness-cannot-prime-a-guard-table-or-plant-a-test-module-probe
description: "Mutation coverage stopped at 59 of 65 guards, and every one of the six that remain is blocked on the harness rather than on the guard. Four need two capabilities it does not have: a way to make a recorded table non-empty without editing the guard that reads it, and a way to plant a #[cfg(test)] probe without breaking the build. Two are structurally impossible — cargo refuses one as a cyclic package dependency, and the other's only construction path cannot be deleted without breaking the crate."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `mutation-harness-cannot-prime-a-guard-table-or-plant-a-test-module-probe`

## Status

Open. Carries the six guards that
`missions/archived/completed/guard-mutation-coverage-is-nine-of-sixty-five-and-the-cheap-path-is-verified.md`
could not cover, plus the two harness capabilities that would unblock four of
them. That mission is closed at 59 of 65 with a verified count; this one is the
honest remainder rather than a reopened version of it.

## The finding

A mutation harness can only prove a guard that it can make go red. Six guards in
`crates/octo-cli/tests/security_regressions.rs` cannot be made red by anything
`scripts/mutate-zk-guards.py` currently expresses. Four of the six are blocked on
**two missing capabilities in the harness**, not on anything wrong with the
guards — which is a materially different backlog item from "six guards lack
mutations", and was mis-filed as one until the coverage was re-measured from the
test file rather than from the harness.

### Capability 1 — priming a recorded table

`ac11_every_allowlist_entry_still_exists_and_still_has_a_reason` iterates
`AC11_ALLOWED`, which is **empty**. Its loop body asserts nothing today: not the
path, not the reason, not the constructor site. It is dormant rather than broken
— an empty allowlist is the state the guard wants — but it has never fired and
cannot until something is allowlisted.

A mutation cannot make an empty table non-empty without editing the
`AC11_ALLOWED` constant, which lives in the guard's own file. Doing that proves
the test, not the guard.

**What is needed:** an entry form that adds a _record to a constant the guard
reads_ rather than editing a source file — the guard's input, primed from the
outside. This is the same shape as the multi-file and replace-all entries that
were added to the harness, one level up: those mutate files the guard reads, and
this one mutates the table the guard iterates.

The honest precondition is that a table-driven guard cannot be mutation-proven
while its table is empty. Either the capability is built, or the guard is
documented as dormant and the coverage figure says so. What must not happen is
the current state persisting silently: the guard is listed in
§Where coverage stops as merely unbacked, and nothing says its body asserts
nothing today.

### Capability 2 — planting a `#[cfg(test)]` probe

`the_production_split_survives_an_item_level_test_marker` and
`every_dispatched_payload_kind_is_driven_through_an_envelope_in_tests` both
assert properties of the _test_ side of the tree. A mutation for either has to
sit inside another crate's `#[cfg(test)]` module and keep compiling.

Planting there is where the file-scanning trick stops working: the crate is
inside octo-cli's dependency closure, so the plant is compiled, and an item
that does not compile is reported BAD-MUTATION rather than as a red guard.

**What is needed:** either a probe form that is valid in a test module and does
not require the surrounding crate's imports, or a documented exemption saying
these two guards are mutation-proven by hand and when.

### The two that are not the harness's fault

**`the_two_algorithm_crates_stay_independent_and_additive`** — adding an
algorithm dependency to `octo-ident`, which the guard forbids, is refused by
cargo before the guard runs:

```
cyclic package dependency: package `octo-algo-ed25519` ...
```

Both algorithm crates already depend on `octo-ident`, so the reverse edge is a
cycle and the workspace does not resolve. **The layer rule is already enforced
one level down, by cargo.** The assertion is not wrong — it names the invariant
and would catch a _new_ algorithm crate that does not create a cycle — but for
the two algorithms that exist today it cannot fail, and a reader who believes it
is what holds the line would be wrong about which thing holds it.

**`ac11_the_ed25519_registration_itself_is_where_keys_get_built`** — the guard
asserts `src.contains("VerifyingKey::from_bytes")` and the constructor occurs
exactly **once**, in the only path that builds a key. Deleting it does not
compile. A cosmetic mutation (spacing the `::`) would make the text assertion
fail while the crate still constructs keys perfectly well, which is the wrong
answer and would have been dishonest to record.

## Acceptance Criteria

- [ ] `AC11_ALLOWED` is either primed by a harness capability, or the
      allowlist guard is annotated as dormant with the empty table named as the
      reason — and §Where coverage stops stops presenting the guard as merely
      unbacked rather than as dormant
- [ ] A probe form exists that can be planted inside another crate's
      `#[cfg(test)]` module and still compiles, **or** the two test-side guards
      carry a recorded manual proof with the date and the command
- [ ] The two-algorithm assertion says plainly that cargo holds the line for
      today's two algorithms, and names the case it still catches
- [ ] The coverage figure is re-derived afterwards rather than edited

## Why this is filed separately rather than reopened

The closed mission's coverage figure is derived by the harness from its own
table, and closing it with the figure _and_ a hand-check that agreed is what
makes it trustworthy. Reopening it to carry four blocked guards would put a
number in the title that no measurement supports — `nine-of-sixty-five` was the
finding, `59-of-65` is the result, and neither is `65-of-65`.

## Method note

The counts are measured, not asserted: 65 guards read from the `#[test]`
attributes in the test file, 59 from the `--exact` filters in the harness table,
and the six names were checked to be the same six by hand. See
§An independent re-measurement in the closed mission.
