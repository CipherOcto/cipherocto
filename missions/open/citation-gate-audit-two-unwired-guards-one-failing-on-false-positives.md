---
name: citation-gate-audit-two-unwired-guards-one-failing-on-false-positives
description: "Three gates police §section citations and only one is in CI. lint_section_cites.sh is unwired and fails today with 58 findings that are all adversarial-review finding ids such as §6a and §9b. validate_cites.sh documents itself as blocking on commit, but the only installed git hook is commit-msg; its stated coverage of mission YAMLs is absent from its own file-discovery command; and it passes CLAUDE.md clean while that file holds two dangling section pointers. A full run of it produced no output in over two minutes."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `citation-gate-audit-two-unwired-guards-one-failing-on-false-positives`

## Status

Open. Nothing has been deleted, rewired, or reconfigured.

## The finding

Three gates police `§section` citations in this repository. Exactly one of
them is referenced by a workflow.

| Gate                            | In CI  | State today                                   |
| ------------------------------- | ------ | --------------------------------------------- |
| `scripts/lint-rfc-anchors.py`   | yes    | green, 395-entry baseline                     |
| `scripts/lint_section_cites.sh` | **no** | **fails, exit 1, 58 findings**                |
| `scripts/validate_cites.sh`     | **no** | passes, but on a smaller shape than it claims |

A repository that keeps three implementations of one rule and runs one of them
is not three gates. It is one gate and two artifacts.

## Guard 1 fails today, and would fail for the wrong reason

`scripts/lint_section_cites.sh` run against the tree:

```
Summary: ACCEPTED=3933 NON_CANONICAL=6290 REJECTED=58
FAIL: 58 cite hygiene violation(s) found.
```

Every rejected token inspected is an **adversarial-review finding id**, not a
section reference:

- `RFC-0850p-c §6a "Platform Migration (E2E IS-4.8 fix)"`
- `RFC-0850p-c §9b`
- `RFC-0963 §1a`

Numbering findings `1a`, `6a`, `9b` and citing them as `§1a` is this
repository's own convention, and the CI-wired linter excludes single-token
anchors for exactly that reason — its `is_section_anchor` exists because a lone
capital in this corpus is a finding id far more often than a heading.

So the repo does not currently pass its older cite guard, and would not pass it
for a defensible reason. That is the worst state for a gate: red, so ignored,
and wrong if anyone ever tried to fix it.

## Guard 2 documents itself as commit-blocking and is not installed

`scripts/validate_cites.sh` opens with:

```
# Blocking on commit: YES.
```

There is no such hook. `.git/hooks/` contains exactly one entry, a symlink for
`commit-msg` pointing at `scripts/hooks/commit-msg-no-backtick` — the
no-backtick hook. No hook file references either cite script, and no workflow
references them.

Anyone reading that header believes commits are gated on citation validity.
They are not. The header is the only evidence that they were ever meant to be.

## Guard 2's stated coverage is absent from its own code

The same header claims:

```
# Coverage: Cite instances across all RFCs + research doc + mission YAMLs.
```

Its file discovery is:

```bash
find "$RFC_ROOT" docs -maxdepth "$CITE_MAXDEPTH" -type f -name '*.md'
```

There is no `missions` path and no `*.yml` or `*.yaml` glob. Searching the
script for `missions`, `.yml`, or `.yaml` returns nothing at all. The claim is
not approximately true, it is absent.

## Guard 2 misses what the others find

Run against `CLAUDE.md`:

```
Summary: CHECKED=5 VALID=5 PHANTOM=0 INVALID=0 STALE=0
PASS: all cites valid.
```

`CLAUDE.md` contains two section references that do not resolve — it points
readers at BLUEPRINT.md sections for the RFC lifecycle and for decomposition
thresholds, and neither exists, as recorded in mission
`anchor-lint-coverage-gap-and-two-broken-pointers-in-claude-md`. Guard 2 passes
the file because it validates the `RFC-XXXX §section` shape only. A reference
written as `docs/BLUEPRINT.md §The RFC Process` is a different shape and is
never examined.

That is not a defect in isolation. It is the reason a rule with three
enforcers can still be violated in the file every agent reads first.

## Guard 2 is too slow to be a pre-commit gate

A full-tree run produced no output in over two minutes and was cancelled. The
script forks per cite and carries per-command `timeout` scaffolding precisely
to survive that. Whatever it was designed to guard, a gate that takes minutes
is not one a person keeps installed.

## Acceptance Criteria

- [ ] A decision is recorded for each unwired guard: delete it, wire it, or
      mark it superseded in its own header so the next reader is not misled
- [ ] No guard's header claims behaviour it does not have — the commit-blocking
      claim and the mission-YAML coverage claim are the two that must go one way
      or the other
- [ ] The rule has one implementation, or a stated reason it needs more than one
- [ ] The reference shape Guard 2 misses is either covered by the wired linter
      or recorded as an accepted limit
- [ ] If any guard is kept for local use, its cost on a full run is documented,
      since two minutes of silence is not a usable default

## Why this is filed rather than fixed

Deleting two scripts needs approval, and both are candidates for it rather than
for repair: the CI-wired linter covers the same rule with a calibrated
implementation, and one of these two is demonstrably wrong about its own corpus.
But which of the three is meant to be canonical is a decision about the
repository's long-term shape, and it is not one to make by deleting the files
that happen to be easiest to remove.

Wiring either into CI is equally out of scope here, for the same reason the
anchor lint's reference scope was left alone: CI and lint workflow files are
owner policy surface.

## Method note

Every claim about behaviour comes from running the scripts and from searching
the repository, not from reading the headers. Where a header is cited above it
is cited _as a claim being contradicted_, and each contradiction was confirmed
against the code or by execution:

- Guard 1's failure is a real run today, and its findings were read in context
  rather than counted.
- Guard 2's hook absence is the contents of `.git/hooks` and `core.hooksPath`.
- Its coverage claim was tested with a search for the terms it names, which
  returned nothing.
- Its blindness to the CLAUDE.md pointers is a run against that file.
- Its cost is a measured run that was cancelled, not an estimate.
