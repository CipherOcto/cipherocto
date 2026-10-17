---
name: one-thousand-nine-hundred-rfc-references-carry-a-version-pin-and-only-a-fifth-are-cross-references
description: "CLAUDE.md forbids status and version metadata on RFC references. Measured across git-tracked files there are 1954 occurrences of RFC-number-with-version in 301 files, but the rule exempts Version History tables and the occurrences split into three populations: 103 inside Version History tables which are permitted, 22 in Rust string literals where the version is the subject matter, and a remainder that is genuinely mixed. The 171 Rust line comments have been fixed. The markdown remainder is not mechanically separable because a heading naming its subject is not a cross-reference."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-13T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `one-thousand-nine-hundred-rfc-references-carry-a-version-pin-and-only-a-fifth-are-cross-references`

## Status

Partially fixed. Rust comments are done. The markdown remainder is not
mechanically separable and is recorded here rather than guessed at.

## The rule

`CLAUDE.md` §RFC Reference Conventions (Reaffirmed): references use only the
number, never status or version pins. Its stated reason is that status and
version in references cause sync bugs and verbose noise, and that **only the
RFC's own Status header and Version History table carry version info**.

## Measured, over git-tracked files

| Population                                                  | Count  |
| ----------------------------------------------------------- | ------ |
| inside a Version History or Changelog table — **permitted** | 103    |
| archived mission history                                    | 311    |
| Rust string literals — the version _is_ the subject         | 22     |
| Rust line comments — **fixed**                              | 171    |
| tracked markdown prose and other tables                     | ~1,347 |

The headline 1,954 is the raw count and describes almost nothing on its own.
The rule's own exemption for Version History tables removes 103 of it, and a
further 333 are history or literal text.

## Why the Rust strings are left alone

```rust
assert_eq!(entries.len(), 57, "RFC-0111 v1.20 specifies 57 entries");
```

Stripping the version produces a sentence with no subject. That is a mention
of a version, not a reference to a document.

```rust
//! | RFC-0011 v1 field | RFC-0011-c v4 field | Note |
```

Seven remaining comment sites are wire-format discriminators in a comparison
table, where the two versions being contrasted are the content. Those were left
in place for the same reason.

## Why the markdown is not separable

The line below is a heading naming its subject:

```markdown
### RFC-0131 v1.27 — BIGINT→DQA
```

The line below is a cross-reference:

```markdown
- [ ] Implement `bigint_to_bigint(...)` per RFC-0131 v1.27
```

Both match the same pattern, and the second must lose the version while the
first must not. Worse, prose that _discusses_ a version is often shaped like
the second:

```markdown
RFC-0131 v1.27 renamed `limbs` to `digits`.
```

Removing the version there deletes the subject of the sentence.

So the mechanical rule "strip the version pin" is wrong for an unknown
fraction of roughly 1,347 markdown sites, and applying it would trade a
documented convention violation for a different class of error: prose that no
longer says what it meant. That is the same trade this session rejected when
it declined to repair 436 dangling citations on a guess.

## What would make it separable

A marker that distinguishes the two positions. In practice the heading form is
already distinguishable — a `###` or `##` line naming an RFC is a title, not a
citation — so roughly a third of the population could be classified
mechanically and left alone. The remainder needs a per-site judgement about
whether the version is the subject or an attribute of the reference.

If the intent is to enforce the convention, a checker is the durable answer,
with a baseline for the permitted Version History rows and the archived
history, since both are legitimately full of versions. The same shape as the
`no-line-refs` rule, which has the identical carve-outs.

## The STALE backlog is now classified

The 1007 `STALE [version pin mismatch]` findings this linter reports are
classified in
`a-thousand-stale-version-pins-are-ninety-nine-percent-real-but-the-linter-reports-the-wrong-version`.

Summary: **997 genuine, 7 false positives, 3 citing a document with no Version
History at all.** Unlike the `INVALID` and `PHANTOM` classes, this backlog is
not inflated by a measurement error — the pins really are behind the documents
they cite.

The linter does name the wrong version in its message, having read the first
version-shaped table row anywhere in a file rather than the latest Version
History row. That is a report-quality defect rather than a verdict defect: a
correct reader still reports the same 997 documents, with a message a
maintainer can act on.

## Acceptance criteria

- [x] Measure the population over git-tracked files rather than the working
      tree.
- [x] Separate the permitted Version History rows from genuine violations.
- [x] Separate mentions from cross-references, and record why the two cannot
      be told apart by pattern.
- [x] Fix the 171 Rust comment sites, and compile-check the crates.
- [ ] Decide whether markdown prose is repaired by hand or by a checker with a
      baseline.
- [ ] If a checker is added, it must distinguish a heading that names its
      subject from an inline cross-reference, and must exempt Version History
      rows and archived missions.

## Notes

The count is not the finding. A rule expressed as "never include a version"
produces, in this corpus, sentences with no subject and comparison tables
with one column removed. Both are worse than the violation, and neither is
visible in a count.

This is the second time this session that a rule with a stated rationale could
not be applied mechanically, and both times the obstruction was the same:
the pattern cannot tell a reference to something from a mention of it.
