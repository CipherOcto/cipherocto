# Engineering rules

Workflow and process rules. Code comments cite these by name — for example
"per `[[deferred-vs-unspecified]]`".

These were previously private working notes. They are here so that a citation in
a doc comment resolves for anyone reading the repository.

## Deferred versus unspecified

**A deferral must name its owner and its unblocking condition. A specification
must state what will be built.**

These are different states and conflating them is how work disappears:

- _Deferred_ — the work is understood and scheduled. Someone owns it. The
  thing that would let it start is written down.
- _Unspecified_ — nobody has decided what the work is.

An unowned deferral is indistinguishable from completed work the moment a parent
mission rolls it up into a summary. When a mission defers work, file the
follow-on as its own mission **at the time of the deferral**. A deferral whose
target mission does not exist yet is a promise, not a plan.

The same reasoning applies to tests. A characterisation test written before a fix
must state the direction it expects, so that it flips when the fix lands rather
than quietly staying green.

## No phantom mission pointers

A comment that names a mission must name one that exists. Verify the filename
before writing the pointer; a dangling reference is worse than no reference,
because it reads as though the work is tracked and it is not.

## No fabricated commit claims

Do not describe a commit, a branch state, or a test result that was not observed.
If a step was skipped or a result was not checked, say so in the commit message
rather than in a later summary where it reads as verified.

Verification claims are only as good as the check that produced them, and the
check has to have been run against the implementation rather than against a
test's own assertion.

## No line references in documentation

Markdown and doc comments refer to other things by `§section name` or by symbol
anchor, never by `file.rs:123`. Line numbers rot on the next edit and are not
clickable; section names and symbols survive refactors.

## No memory citations in shipped documentation

A `[[wikilink]]` resolves inside one developer's private notes and nowhere else.
Code comments and documentation in this repository cite
[`design-principles.md`](../architecture/design-principles.md),
[`stoolap-integration.md`](../architecture/stoolap-integration.md), or a named
section of this file instead.

## Git workflow

Commits and pushes are not autonomous. Remote writes — `git push`, `gh pr`,
`gh issue`, `gh release` — happen only on explicit instruction. Locally, commits
on a feature branch are fine; `main` is never pushed to directly.

Branch strategy and protection rules live in `.github/BRANCH_STRATEGY.md` and
`.github/branch-protection-rules.md`.

## Format before commit

`cargo fmt --all`, then clippy with warnings denied, then Prettier for
documentation. A commit that does not compile is not a commit.

The commit hook rejects backticks, `;`, `&&`, `||`, `|`, `$(` and `${` in the
message. Write the message to a file and use `git commit -F` rather than fighting
a shell heredoc — and check the result for those characters yourself, because
the hook will not tell you what it caught before you have already committed.

## Related

- [`design-principles.md`](../architecture/design-principles.md) — the
  engineering principles, cited as `§<section name>` throughout the tree.
- [`docs/BLUEPRINT.md`](../BLUEPRINT.md) — the RFC and mission lifecycle.
