---
name: cite-hygiene-sweep-2-rfcs-were-never-covered-and-three-classes-survive-in-crates
description: "The completed cite-hygiene-sweep was scoped to doc-comments in crates/. Its own stated pattern §X.Y L<num> still has 42 instances, AC L<num> has 9, bare L<num> has 130, and path.ext:NNN has 117 in .rs files. rfcs/ was never in scope at all and still carries 192 bare L<num>, 15 path.md:NNN, and 11 §NNN. Eleven of the §NNN cases are line numbers wearing a section sign, and they have rotted: they cite lines 651 to 735 of an RFC template that is now 132 and 296 lines long."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `cite-hygiene-sweep-2-rfcs-were-never-covered-and-three-classes-survive-in-crates`

## Status

Open. Nothing has been changed. No line reference has been rewritten.

## The finding

`missions/archived/completed/cite-hygiene-sweep.md` swept RFC version pins and
line references out of doc-comments. Its scope line reads: _Strip RFC version
pins + line refs from all doc-comments in `crates/`._ Two consequences, both
measured against the tree rather than read off the mission.

### The sweep left the pattern it named

The mission's second sub-step is explicit: `§X.Y L<num>` → `§X.Y`. That pattern
still has instances in `crates/`, including three of exactly this shape:

```rust
//! Per RFC-0855p-b §Key Files, the `mon/election.rs` path is the
```

in `crates/octo-network/src/mon/election.rs`, and the sibling files
`coordinator.rs` and `slashing.rs`, which cite `L847` and `L849`. Forty-two
instances of `§<name> L<num>` remain across `crates/`.

> **Resolved.** All 42 were removed, across 15 files, and the section names
> were verified to resolve before the suffix was dropped rather than after.
> One of them was not a line-reference problem at all: `sender/mod.rs` cited
> `§Network Sender`, which is a heading that exists nowhere in RFC-0863, where
> the heading is `### NetworkSender Trait`. Stripping the line suffix would have
> left a citation that still dangled, so the name was corrected too. Nothing
> else in the crates matched `§<name> L<num>` afterwards, and
> `crates/octo-cli/tests/security_regressions.rs` was checked for assertions on
> the literal strings before the edit.

Three further classes sit outside the patterns the sweep listed, so they are not
a failure of it but are violations of the same rule:

- `AC L<num>`, nine instances, as in `the AC L33 differential test`
- bare `L<num>`, 130 instances in `.rs` files once all forms are counted,
  or 133 including `Cargo.toml`
- `path.ext:NNN`, 117 instances in `.rs` files, as in
  `crates/octo-network/src/dot/binding.rs:NNN`

### rfcs/ was never covered at all

The sweep scoped itself to `crates/`. RFC prose was outside it, and RFC prose is
where line references are densest:

| Form                             | Count in `rfcs/` |
| -------------------------------- | ---------------- |
| bare `L<num>`                    | 192              |
| `path.md:NNN`                    | 15               |
| `§NNN` with three or more digits | 11               |

## The part that has actually rotted

Eleven references are line numbers wearing a section sign — written `§693`
rather than `L693`, which is why they read as section citations and escape a
rule aimed at `L` prefixes. Four of the eleven are already caught, because
their lines happen to cite an RFC and so fall inside the anchor lint's new
numbered check. The other seven escape it, and all seven were read directly:

- `rfcs/accepted/process/0009-identity-management.md` cites `template §651`,
  `template §693`, `template §731`, and `template §735` as requirements of the
  RFC template.
- `rfcs/accepted/process/0010-canonical-did-codec.md` cites `template §693` for
  the same requirement.
- `rfcs/accepted/numeric/0105-deterministic-quant-arithmetic.md` cites
  `per review §1336` and `review §1362` for a cross-section reconciliation.

**None of them can be followed.** The two RFC templates in the tree are
`rfcs/draft/process/0000-template.md` at 296 lines and
`docs/07-developers/rfc-0011-pr-description.md` at 132 lines. Lines 651 through
735 do not exist in either. These pointers were written against a template long
enough to have those lines and were never revisited after it was shortened.

This is the whole argument for the rule in one instance: a `§section_name`
reference either resolves or is visibly broken, while a line reference decays
into a plausible-looking citation that sends the reader nowhere.

## Why no gate catches this

The anchor lint now checks numbered anchors, but only on a line that cites an
RFC this repository owns. That gate is deliberate — without it the lint reports
external specifications such as `RFC 8032 §5.1.6` — and these lines cite
`template` or `review` instead of an RFC, so they fall outside it. The remaining
`§Name L<num>` and `path.ext:NNN` forms are not anchors at all.

So the coverage for this rule is a completed one-time sweep and nothing else.
The sweep was not verified after it landed, which is how the pattern it named
came to survive in it.

## What is measured versus what is sampled

Stated plainly so the next person does not inherit it as verified:

- **Read directly:** the `L847`, `L848`, `L849` instances and their files, the
  `AC L33` instances, all eleven `§NNN` references with their files, and the
  two template lengths.
- **Counted, not sampled:** every aggregate in both tables. They come from
  `grep -oE` over tracked files, not from reading each hit, and the sweep
  mission's own precedent is that a re-count at mission start is mandatory
  because an earlier draft of it was off by 102. The `path.ext:NNN` counts in
  particular are the least trustworthy, since that form also matches paths into
  third-party dependencies such as `matrix-sdk-0.18.0/src/room/mod.rs:NNN`,
  where the correct remedy may differ from a project file.

## Acceptance Criteria

- [ ] Every `L<num>` and `path.ext:NNN` in `crates/` and `rfcs/` is re-counted at
      mission start, and each class is verified by reading its hits before any
      edit, so no aggregate above is treated as ground truth
- [ ] The `§<name> L<num>` form the first sweep named and missed is removed
- [ ] Each `§NNN` instance is resolved to a section name or removed, and none is
      left pointing at a line number
- [ ] The distinction between a project file reference and a third-party
      dependency path is decided before the `path.ext:NNN` class is swept, since
      removing a reference into an upstream crate loses information the reader
      may want
- [ ] A gate exists for line references in at least one form, so this does not
      depend on a one-time sweep staying true
- [ ] The archived sweep mission's status header is reconciled with its location
      under `missions/archived/completed`, since it still reads `claimed`

## Why this is filed rather than fixed

Rewriting 200-odd references across `crates/` and `rfcs/` is a mechanical sweep
by intent, and the first one was. Two things make it a maintainer's call rather
than a sed:

- RFC version history is a historical record. Rewriting what a past amendment
  said it changed is not obviously right even when the citation is now wrong.
  The honest fix may be a note rather than an edit.
- The third-party path cases need a policy answer before they are swept, since
  a reference into an upstream crate is a different thing from a reference into
  this one.

Either way the re-count is mandatory first, and the archived mission records
that its own first draft got that badly wrong.
