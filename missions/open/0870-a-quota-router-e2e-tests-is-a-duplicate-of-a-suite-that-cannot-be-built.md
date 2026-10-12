---
name: 0870-a-quota-router-e2e-tests-is-a-duplicate-of-a-suite-that-cannot-be-built
description: "quota-router-e2e-tests holds 50 L2 end-to-end tests that duplicate crates/quota-router-integration-tests almost exactly — all 53 distinct function names overlap, and the 16 shared test files differ by a single line each, the crate name inside a use statement. It is not a workspace member, is not in the exclude list, and has no [workspace] table, so cargo refuses to load it at all. No workflow references it. Its last functional change was a cargo fmt in June 2026; the suite that replaced it was fixed six days ago."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0870
---

# Mission `0870-a-quota-router-e2e-tests-is-a-duplicate-of-a-suite-that-cannot-be-built`

## Status

Open. Nothing has been deleted. Removal needs written approval.

## The finding

`quota-router-e2e-tests/` is a copy of an L2 end-to-end suite that already
exists, in a form nobody can run.

## Evidence

### Cargo cannot load it

```
$ cargo metadata --manifest-path quota-router-e2e-tests/Cargo.toml
error: current package believes it's in a workspace when it's not:
current:   .../quota-router-e2e-tests/Cargo.toml
workspace: .../cipherocto/Cargo.toml
```

The root manifest sets `members = ["crates/*"]`, so this package is not a
member. It is not in `exclude` either. And unlike the two sibling packages
that were deliberately moved out of the workspace, it declares no
`[workspace]` table of its own:

| Package                               | `[workspace]` table | In root `exclude`  | Cargo can load it |
| ------------------------------------- | ------------------- | ------------------ | ----------------- |
| `quota-router`                        | yes                 | yes                | yes               |
| `live-tests/octo-telegram-live-tests` | yes                 | — (self-contained) | yes               |
| `quota-router-e2e-tests`              | **no**              | **no**             | **no**            |

So this is not an unrun suite. It is unbuildable — cargo stops at manifest
resolution and never gets as far as compiling a test.

### It duplicates a suite that is actively maintained

`crates/quota-router-integration-tests/` is a workspace member and is run by
`multi-node-nightly.yml`. Comparing the two:

- **53 of 53** distinct function names in the orphaned crate also exist in
  the live one. Unique coverage: **zero**.
- **16** test files are shared by name. Every one differs, and the total
  difference across all sixteen is **237 lines** — about fifteen per file,
  which is one changed line each.
- Spot-checked: `l2_dispatch_counter_regression.rs` differs by exactly one
  line.

```diff
-use quota_router_e2e_tests::TestCluster;
+use quota_router_integration_tests::TestCluster;
```

The signature of a rename that updated the copy in the new location and left
the original in place.

### Nobody runs it, and it has not been maintained

- No workflow file references `live-tests`, `quota-router-e2e-tests` or any
  path under them.
- Last change: `2026-06-30  style: cargo fmt on Phase 4 test files` — a
  formatter pass, no functional edit.
- The suite that replaced it was last touched `2026-10-06`, a substantive
  security remediation.

## Why this matters beyond the wasted lines

A duplicated suite is worse than an absent one. Two copies of the same 50
tests will drift, and the copy nobody runs is the one that will look
current to someone reading the tree. A maintainer looking for L2 coverage
would reasonably find `quota-router-e2e-tests` and conclude it is covered.

It is also a trap for anyone who tries to _run_ them: the failure is a
workspace-resolution error that says nothing about tests.

## What is NOT claimed

- No claim that any L2 coverage is missing. The live suite carries the same
  53 functions plus more, and runs nightly.
- No claim about `live-tests`. It is a deliberate standalone crate, created
  by a commit whose message is "move telegram live_session_test to standalone
  crate outside main workspace". It needs live Telegram credentials, which
  is a legitimate reason for it to be outside CI. It is the _contrast_ case,
  not a problem.

## Acceptance Criteria

- [ ] `quota-router-e2e-tests/` is deleted, or an empty `[workspace]` table
      is added so it at least resolves, with a stated reason for keeping it
- [ ] If it is kept, the crate description says it is superseded by
      `crates/quota-router-integration-tests` and must not be edited
- [ ] A check exists that fails when a package directory sits outside the
      workspace, is absent from `exclude`, and declares no `[workspace]`
      table — the condition that made this one invisible for three months

## Note on the third criterion

The existing `no_path_dependency_is_declared_and_never_referenced` guard
reads every tracked `Cargo.toml`, so it _sees_ this package. It says nothing,
because a package with no buildable target has no dependency to judge. The
gap is one level up: the manifest is well-formed enough to parse and invalid
enough to be unbuildable.

That check is a few lines — read the root manifest's `members` and `exclude`,
then for every tracked manifest assert its directory is a member, is excluded,
or declares `[workspace]`. It is not written here because a guard that
reports a finding nobody has decided about is not yet worth adding, and the
decision on this package is the maintainers.
