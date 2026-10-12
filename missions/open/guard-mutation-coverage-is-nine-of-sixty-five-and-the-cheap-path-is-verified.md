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

Largely resolved. The path-dependency guard's blind spot is **fixed** — see §A
blind spot below — and mutation coverage has moved from 9 of 65 guards to **37 of 65**, a figure the harness now derives from its own
table rather than one maintained by hand — see §Coverage counted twice, and the
count that was wrong. Adding one of them exposed a false pass in the harness
itself, now fixed — see §A false pass found below.

Separately, and more importantly: the harness has now been run in full for the
first time. **All 24 entries observed red**, in 55 seconds, and the table has
since grown to 42 entries. **Re-run in full against the current table: 42
mutations, 42 red, 0 vacuous, 0 bad-mutation, 0 bad-anchor**, with the tree
clean afterwards and disk unmoved (7.5 GB free before, 7.4 GB after). The mission
previously recorded that the expensive entries could not be reached; that was an
estimate mistaken for a constraint, and the measurement is in §The cost
assumption that was wrong.

The pattern for adding one is proven end to end: pick a guard that reads the
tree at run time, plant a violation outside octo-cli's closure where possible,
confirm the guard names the right file and token, restore, then add the entry
and run it through the harness filtered.

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

## Scans that could pass while reading nothing

Seven scanning guards asserted nothing about how much they read. Five turned out
to be safe by construction — one asserts its root list is non-empty, one compares
against a non-empty expected list, one requires that it found something, and one
compares its findings to a single-element list, so an empty scan fails all four.
Two were genuinely exposed and both are now floored:

- the named-vector mission check, which counted the missions it read and printed
  that count in its failure message without ever asserting on it
- the registry-reach scan, whose sibling walks the same tree and already floored
  what it saw

The seventh is now floored too. `the_wallet_crate_does_not_depend_on_storage`
compares a boolean for its manifest half, so that half was never at risk, but the
walk over octo-wallet source feeding its first assertion was. It was left alone
for one commit on the reasoning that a guard whose halves enforce different
properties is worth fixing as a whole or not at all — which was wrong. Adding a
floor is additive protection, not a partial repair, and nothing about that guard
is half-fixed by counting the files its walk already reads.

So all three exposed scans are floored, and the remaining four were cleared by
reading each one rather than by pattern-matching it.

## The cost assumption that was wrong

An earlier revision of this mission claimed the cheap recipe could not reach the
guards whose failures hurt most, because mutating a crate inside octo-cli's
closure forces a recompile, and predicted coverage would stall there until disk
headroom arrived.

That was an estimate presented as a constraint, and it was wrong by a wide
margin. Measured:

|                                         |                           |
| --------------------------------------- | ------------------------- |
| A dependency rebuild inside the closure | 3.3 s, no measurable disk |
| One source-file mutation, end to end    | 3.5 s, no measurable disk |
| The three feature-flag mutations        | 14.6 s, 0.5 GB            |
| **The entire table, all 24 entries**    | **55 s, 0.9 GB**          |

Everything is already compiled, so a mutation only invalidates one crate and its
dependents. The feature-flag entries cost most because changing a feature edge
re-resolves, not because the table is large.

So the whole harness runs in under a minute. There was never a blocker, only an
unmeasured guess — the second time this session that an estimated cost was
treated as a real constraint. The first was deferring a test-crate rebuild that
turned out to take 1.24 seconds. The pattern is worth naming: a cost nobody
measured is indistinguishable from a wall, and writing it down as a blocker
makes the guess durable.

The `--cheap` flag stays, because a fast subset is worth having. Its premise —
that the rest needs room to compile — does not.

## Which harness entries are actually proven

## Which harness entries are actually proven

**All 24 entries have now been executed and observed red.** The full table was
run end to end for the first time: 24 red, 0 vacuous, 0 bad-mutation, 0
bad-anchor, in 55 seconds. Twelve are the file-scanning entries added to close
the coverage gap here; the other twelve are inherited and had never been run
since they were written.

That matters more than the count. An inherited mutation harness that nobody has
executed carries the same unbacked-claim problem the guards themselves had, and
the whole table passing on its first full run is the result that says the
tooling is sound rather than merely present.

The classification is still mechanical and worth keeping: a mutation is free of
a recompile unless it edits a crate's Rust sources or its manifest within the
closure of the package under test. `--cheap` runs the free twelve in seconds,
`--list` shows the split, and neither is a hand-maintained column that can go
stale when a dependency edge moves.

## Scans that could pass while reading nothing

## Scans that could pass while reading nothing

Seven scanning guards asserted nothing about how much they read. Five turned out
to be safe by construction — one asserts its root list is non-empty, one compares
against a non-empty expected list, one requires that it found something, and one
compares its findings to a single-element list, so an empty scan fails all four.
Two were genuinely exposed and both are now floored:

- the named-vector mission check, which counted the missions it read and printed
  that count in its failure message without ever asserting on it
- the registry-reach scan, whose sibling walks the same tree and already floored
  what it saw

The seventh is now floored too. `the_wallet_crate_does_not_depend_on_storage`
compares a boolean for its manifest half, so that half was never at risk, but the
walk over octo-wallet source feeding its first assertion was. It was left alone
for one commit on the reasoning that a guard whose halves enforce different
properties is worth fixing as a whole or not at all — which was wrong. Adding a
floor is additive protection, not a partial repair, and nothing about that guard
is half-fixed by counting the files its walk already reads.

So all three exposed scans are floored, and the remaining four were cleared by
reading each one rather than by pattern-matching it.

## The cost assumption that was wrong

An earlier revision of this mission claimed the cheap recipe could not reach the
guards whose failures hurt most, because mutating a crate inside octo-cli's
closure forces a recompile, and predicted coverage would stall there until disk
headroom arrived.

That was an estimate presented as a constraint, and it was wrong by a wide
margin. Measured:

|                                         |                           |
| --------------------------------------- | ------------------------- |
| A dependency rebuild inside the closure | 3.3 s, no measurable disk |
| One source-file mutation, end to end    | 3.5 s, no measurable disk |
| The three feature-flag mutations        | 14.6 s, 0.5 GB            |
| **The entire table, all 24 entries**    | **55 s, 0.9 GB**          |

Everything is already compiled, so a mutation only invalidates one crate and its
dependents. The feature-flag entries cost most because changing a feature edge
re-resolves, not because the table is large.

So the whole harness runs in under a minute. There was never a blocker, only an
unmeasured guess — the second time this session that an estimated cost was
treated as a real constraint. The first was deferring a test-crate rebuild that
turned out to take 1.24 seconds. The pattern is worth naming: a cost nobody
measured is indistinguishable from a wall, and writing it down as a blocker
makes the guess durable.

The `--cheap` flag stays, because a fast subset is worth having. Its premise —
that the rest needs room to compile — does not.

## Which harness entries are actually proven

## Which harness entries are actually proven

Eleven of the harness's twenty-three entries have now been executed and
observed red. Four are the file-scanning entries added to close this gap; the
other seven are pre-existing entries — the session-swap group and world-writable
checks, plus the export-bundle and local-writer entries — run for the first time
in this session and all confirmed still fire. That is worth recording because
the alternative is an inherited harness nobody has run since it was written.

The remaining twelve mutate crates inside octo-cli's dependency closure and so
force a recompile. They were not run, because disk was the binding constraint,
and a person picking this up should treat them as unproven rather than as
passing. They are not in doubt — only unexecuted.

The classification is mechanical: a mutation is cheap exactly when its target
file is outside the closure, which is why the four added here were chosen that
way rather than for any other reason.

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

## A blind spot in one of the guards — FIXED

While probing, one mutation produced a surprise: `no_path_dependency_is_declared_and_never_referenced`
stayed **green** when an unused section-form dependency was planted.

That was the guard working as written, not a bug in the check. Its parse
deliberately skips a dependency spelled as its own table:

```toml
[dependencies.probe_unused_dep]
path = "../cipherocto-encoding"
```

because reading that bare `path =` line as a dependency _named_ `path` is what
made an earlier pass report a crate that does not exist. The section header
`dependencies.probe_unused_dep` also fails the guard's own table test, which
looks for a name ending in `dependencies`.

Measured size of the blind spot: the guard examined **305** inline-table path
dependencies and skipped **3**, all in `crates/quota-router-storage/Cargo.toml`.
A grep of that table shows four section-form dependencies there in total. The
uncovered remainder was real dependencies rather than dead ones — all four are
referenced by the crate's own sources — so closing the gap was expected to land
green.

**Resolution.** The dependency name is now taken from the table header for that
form, which removes the hazard the skip existed to avoid instead of trading one
error for another, and the section body is carried along so `optional` and
`features` — which sit on their own lines in this form — are still skipped.

It landed green, as predicted: all 65 guards pass, and the four real
section-form dependencies are correctly not reported. The mutation that
previously slipped through now fails and names `probe_unused_dep`, so the
original mistake does not survive the fix. The inline form was planted
separately and still fails.

The rebuild this needed turned out to cost 1.24 seconds and no measurable disk,
so the reason given below for deferring it — the disk budget — turned out not
to bind. The deferral was wrong on its own reasoning, which is worth recording:
the risk that a test-crate rebuild would exhaust the budget was estimated, not
measured, and estimating a cost you can simply measure is how a small fix gets
pushed off indefinitely.

## Coverage counted twice, and the count that was wrong

The figure in §Status was maintained by hand, one number at a time, as each
mutation landed. It was wrong twice in this mission's own life: it read 13 while
the table held 15, and by the end of the current wave it would have read 15 while
the table held 19. Both times the drift was in the same direction and for the same
reason — two entries were added and the mission was not updated with them, because
the count lived in prose instead of being derived from the table.

This is the mission's own thesis turned on itself. A hand-maintained list is an
unverified claim, and the mitigation is not vigilance but derivation. The count is
now produced by reading the harness table and collecting the distinct
`security_regressions` guard names out of each entry's cargo filter:

```
$ python3 scripts/mutate-zk-guards.py --list | tail -1
  48 entries -> 37 distinct guards in security_regressions.rs  (+7 entries targeting other crates' suites)
```

That line is printed by the harness itself, derived by reading its own table. It
did not exist when the count was first wrong, and putting it there is the fix:
the number can no longer drift from the thing it counts, because it is recomputed
every time anyone asks.

**48 entries cover 37 guards, and the two numbers must not be conflated.** Seven
entries do not target `security_regressions.rs` at all — three run
`zk-vendor`'s `library_path` suite, two run `zk-verifier`, two run
`cipherocto-fs-perms`. They are real mutations against real guards, but those
guards are not among the 65 this mission counts, so counting them would inflate the
coverage figure by seven. Conversely two entries target the **same** guard
(`m15_every_adapter_canonicalize_verifies_the_envelope`, once for the hardcoded
peer key and once for the dropped algorithm argument), so entry count overstates
distinct coverage too. The two errors happen to run in opposite directions, which
is exactly why neither was visible.

Nothing about the harness changed to produce this — the same figure is derivable
from the committed file at any time, and it was not before.

## A VACUOUS verdict that was the mutation's fault, three times

Six entries in one wave came back, and three of them came back **VACUOUS**.
Every one of the three was a broken mutation rather than a broken guard, and
each failed for a different reason worth keeping — which is the argument for
scoring them rather than eyeballing them.

**A shell case arm cannot be loosened by adding to it.** `G-Y` originally
_added_ a newline arm to the `--name` pattern:

```sh
case "$NAME" in
    ''|*[!a-zA-Z0-9._-]*|*$'\n'*)
```

and the guard stayed green. That reads like a guard that cannot see an injected
newline. It cannot: a newline is a character _outside_ `a-zA-Z0-9._-`, so
`*[!a-zA-Z0-9._-]*` already refuses it, and any further arm can only match names
the first arm already rejected. Adding to a `case` pattern moves validation
strictly tighter, never looser. The mutation has to remove the class check.
Checked against the shell directly before rewriting:

```
REJECTED by the existing pattern
```

**A guard's needle is a prefix, and rewriting the rest of the line leaves it
standing.** `G-aa` changed the right-hand side of `if let Some(pk) = …` to
fall back to a default key. The guard looks for `if let Some(pk)` — which the
mutation left in place. Moving the key inside the block is what removes the
opt-in shape the guard exists to notice.

**A definition count counts one name.** `G-bb` added a second private writer
called `write_private_file_local`, and the guard stayed green. It counts
`pub fn write_private_file(`, and the local variant does not match. That is a
real limit of the guard, not a broken mutation, so it is recorded on the entry
rather than treated as a pass: **the guard catches a duplicate, not a
reimplementation.** The H-7 entries have the same shape for `create_private`.
Closing the gap means looking for the mode constant beside the open rather than
for a name — a different check, and not one this entry claims to have made.

The third mistake would have been the expensive one: reading three VACUOUS
verdicts as three broken guards and "fixing" them into agreement.

## A guard whose race assertion could never fire — FIXED

`l7_active_symlink_swap_is_atomic` is named for a TOCTOU race: `remove_file`
on `active`, then `symlink` back to it, lets a same-UID process create the path
itself in the gap. Its second assertion exists to reject exactly that shape.

That assertion was unreachable. The guard built its haystack with

```rust
body.split_whitespace().collect::<Vec<_>>().join(" ")
```

which **keeps one space between tokens**, and then searched it for

```
let_=fs::remove_file(&active)
```

which contains none. For any Rust source the needle is absent, so
`!flat.contains(needle)` is true of every possible input and the assertion
cannot fail. The guard would have passed with the race fully reinstated.

Found by the mutation `G-O`, which came back **VACUOUS** — the guard stayed
green under a mutation that reintroduces the defect it exists for. That is the
harness earning its keep: the entry reported a non-pass rather than being read
as a harness bug.

**Two things were wrong at once,** which is why neither showed up separately.
Fixing the normalisation alone made the assertion fire — on unmutated source.
`use_account` legitimately calls `remove_file(&active)`, in the
`#[cfg(not(unix))]` branch: the Windows copy path overwrites the file directly
and has no symlink to race on, and the source says so in a comment. So the
assertion was also over-broad — a check nobody can satisfy is a check nobody
reads, and it would have been removed on first encounter.

The resolution scopes the negative assertion to the unix branch by splitting on
the `#[cfg(not(unix))]` attribute, and **fails loudly if that split is absent**
rather than falling back to the whole function. A fallback that silently checks
everything is precisely how the assertion became unreachable; reintroducing one
as the "safe" default would restore the original defect.

Verified in both directions: the mutation is red, and the unmutated suite is
green at 65.

**The generalisable part.** `dense()` exists in this file precisely to strip
whitespace for needle matching, and most guards use it. The defect was a guard
hand-rolling a different normalisation and then writing needles for the other
one. A sweep for `split_whitespace().collect::<Vec<_>>().join(" ")` found two
live occurrences — this one and `m15_every_adapter_canonicalize_verifies_the_envelope`,
whose needles (`&env.source_peer`, `Ok(`, `.verify(`) are all single tokens
that survive either normalisation, and whose relevant assertions are proven by
`G-J` and `G-K` regardless. So the class is one instance deep here, and it is
recorded rather than assumed absent elsewhere.

## Acceptance Criteria

- [x] The section-form path dependency is parsed, with the dependency name taken
      from the table header rather than from the `path` key, which is the whole
      hazard the current skip exists to avoid
- [x] That change is verified in both directions: a planted unused
      section-form dependency goes red, and the four real ones stay green
- [x] A mutation is added for each guard that reads files at runtime, starting
      with the ones whose names promise the most — **eleven landed**
      (`no_test_has_an_empty_body`, `no_path_dependency_is_declared_and_never_referenced`,
      `no_source_or_doc_file_cites_a_private_working_note`,
      `missions_claiming_named_vectors_have_those_names_in_the_tree`,
      `no_production_code_calls_a_placeholder_decoder`,
      `no_verification_site_hardcodes_an_algorithm`,
      `ac11_every_out_of_scope_crate_is_justified_and_disjoint`,
      `ac11_every_crate_that_verifies_a_signature_is_scanned_or_explained`,
      `every_adapter_naming_a_concrete_algorithm_declares_the_assumption`,
      `m15_every_adapter_canonicalize_verifies_the_envelope` — twice, once for the
      hardcoded key and once for the dropped algorithm argument — plus the L-series
      and M-16 findings: `l10_quic_drops_a_replayed_ping_nonce`,
      `l8_pairing_uri_is_withheld_unless_show_secret_is_set`,
      `l6_unix_socket_bind_refuses_to_delete_a_non_socket`,
      `l7_active_symlink_swap_is_atomic`,
      `m16_bluetooth_stub_does_not_issue_a_delivery_receipt`,
      `m8_bearer_required_defaults_to_true`,
      `m9_events_log_files_are_opened_owner_only`,
      `m13_cdp_trace_is_private_and_defaults_outside_tmp`,
      `m12_no_chrome_launcher_disables_the_renderer_sandbox`,
      `c6_systemd_script_rejects_non_absolute_or_bracket_paths`,
      `h10_quic_accept_loop_consults_the_replay_cache`,
      `l5_health_server_does_not_trust_client_forwarded_for`,
      `c6_systemd_heredoc_quotes_the_interpolated_execstart_arguments`,
      `c6_systemd_script_rejects_names_that_can_inject_a_unit_directive`,
      `m10_persist_contacts_rejects_non_integer_limit_and_offset`,
      `m15_the_telegram_adapter_is_a_documented_opt_out`,
      `h6_the_private_writer_is_shared_not_duplicated` and
      `h10_quic_replay_cache_is_reachable_from_the_accept_loop`),
      taking coverage from 9 of 65 to **37 of 65**
- [x] Any harness using the prebuilt-binary path refuses to run when the binary
      is older than the guard source — **satisfied a different way**: the
      committed harness runs through cargo, which rebuilds by construction, so
      the staleness hazard cannot arise there
- [ ] The guard count and the covered count are re-measured when this mission
      closes, and recorded the way the archived cite sweep recorded its own
      re-count requirement

## A false pass found in the harness while doing this

The first version of the second mutation named an invented crate. Cargo refused
to resolve it, and the harness reported **RED ok** with exit 0 — a pass it had
not earned, because the guard never ran. Its rule only excluded
`could not compile`, so any other cargo failure counted as a red guard.

This is the same failure class as a guard that reports for the wrong reason, in
the tooling whose entire purpose is to detect that class. It was caught only
because the entry printed `(no test-result line)` instead of a result line, and
that was noticed.

The outcome rule now requires evidence: a genuine red always prints a
test-result line, so its absence is classified as a bad mutation with cargo's
own error quoted. Verified both ways — the corrected entry reports RED ok with a
real result line, and re-introducing the invented name is reported BAD-MUTATION
with exit 1, where it previously exited 0.

Two lessons worth carrying: a mutation must name something that exists, because
a dependency key has to match the package it points at unless renamed with
`package =`; and a verification harness needs the same scepticism about its own
green as it applies to a guard's.

## Why the rest is filed rather than fixed

The blind-spot fix is done and needs no defence. What remains is the coverage
gap, and that is not urgent: twenty-eight unbacked guards is a large number that
reads as alarming and is not. These guards were written by adversarial review
and several were mutation-checked by hand during that work. What is missing is
the _re-runnable record_, which is what this harness would supply — and the
cheap path above shows it can be supplied without the disk budget that the full
harness assumes.

The first draft of this mission deferred the fix on the grounds that a recompile
would risk the disk budget. That reasoning was estimated rather than measured,
and the measurement came back at 1.24 seconds and no measurable disk. Recording
it because the pattern is the general one: an unmeasured cost estimate is
indistinguishable from a real constraint, and it is the kind that quietly
defers small work forever.

## Method note

The 9-of-65 figure counts distinct guard names appearing in the harness's test
commands against the 65 attributes in the suite, not test invocations. The
anchor check verified each mutation's expected text against its target file
directly. The blind spot was found by planting a real violation and observing a
green result, then reading the guard's parse to find out why — not by reading
the guard and speculating.
