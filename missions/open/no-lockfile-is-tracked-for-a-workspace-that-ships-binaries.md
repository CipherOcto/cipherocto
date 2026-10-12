---
name: no-lockfile-is-tracked-for-a-workspace-that-ships-binaries
description: "Nineteen Cargo.lock files exist on disk and none is tracked in git — .gitignore ignores them globally. The root lockfile pins 1266 packages including 12 from git sources, and no workflow uses --locked or --frozen. The workspace ships binaries, where Cargo's guidance is to commit the lock. Only two dependencies are actually pinned, via a tracked vendored glass_pumpkin and a patch for one git source. .gitignore also carries an opt-in instruction that cannot work: the rule that actually matches is **/Cargo.lock, so uncommenting the line it refers to changes nothing."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `no-lockfile-is-tracked-for-a-workspace-that-ships-binaries`

## Status

Open. No lockfile has been staged, no ignore rule changed, no workflow edited.

## The finding

This repository builds and ships binaries — `octo-cli`, `octo-reputation` and
several onboarding tools all declare a `[[bin]]` — and git tracks **none** of
its lockfiles.

|                                          |       |
| ---------------------------------------- | ----- |
| `Cargo.lock` files on disk               | 19    |
| Tracked in git                           | **0** |
| Packages pinned by the root lockfile     | 1266  |
| Of those, resolved from git sources      | 12    |
| Workflows using `--locked` or `--frozen` | **0** |

Cargo's guidance is to commit a lockfile for a workspace that produces binaries,
so this is the configuration where the lockfile matters most, and it is the one
where it is discarded.

## What is actually pinned

Very little. Two dependencies are pinned to something this repository owns:

- `glass_pumpkin`, replaced through `[patch.crates-io]` with a vendored copy
  under `vendor/`, which is tracked (14 files).
- One git source, patched in the same table.

Those are careful, deliberate choices. Everything else — the remaining ~1264
packages — resolves from the registry at build time, every time.

`.cargo/config.toml` sets an environment variable for an `aws-lc-sys` build and
declares no source replacement, so there is no vendoring that would make the
missing lockfile harmless. The two patched dependencies are the exception, not
the rule.

## Why it matters here

With no lockfile and no `--locked`, a build is a function of what the registry
serves at that moment rather than of what the repository says. Three concrete
consequences:

- **A green run proves less than it appears to.** A test that passes today can
  fail tomorrow with no commit in between, when a transitive dependency
  publishes.
- **Local and CI can differ.** A developer on an older resolution and a CI run on
  a newer one are testing different trees, and the difference is invisible in
  the diff because there is nothing in the repository describing either.
- **A yanked version is an unrecoverable build failure** for anyone who does not
  already have it in `~/.cargo`.

The twelve git-sourced dependencies are the sharpest case: those track a moving
reference unless something pins a revision, and a lockfile is the thing that
carries the revision.

## The `.gitignore` instruction is a trap

The file opens with an instruction that cannot work:

```
# Cargo lockfile (optional - comment out if you want it in repo)
# Cargo.lock
```

and separately, further down:

```
Cargo.lock
...
**/Cargo.lock
```

The rule that actually matches was checked rather than inferred:

```
$ git check-ignore -v Cargo.lock
.gitignore:82:**/Cargo.lock	Cargo.lock
```

So a developer who follows the instruction and uncomments line 9 sees **no
effect at all** — line 82 still ignores every lockfile in the tree, and line 25
already ignored the root one. The documented way to opt in does not opt in.

That is worth fixing regardless of the decision below, because the trap costs
someone an afternoon and produces no error.

## What is NOT claimed

- No claim that any build is currently broken. Nothing here is a red build.
- No claim that the lockfiles on disk are wrong or stale. They are the current
  resolutions; the point is that they are not in the repository.
- No claim about the twelve git dependencies specifically — whether they move
  depends on how each is pinned upstream, which was not audited here.

## Acceptance Criteria

- [ ] A decision is recorded on whether lockfiles belong in this repository, and
      the reasoning for it, since Cargo's guidance points one way and the
      repository has chosen the other without saying so
- [ ] If they are tracked, the decision says **which** of the 19 — the root
      workspace, `quota-router`, `sync-e2e-tests`, `octo-sync` and a pile of
      per-crate and fuzz lockfiles are not one decision
- [ ] If they are tracked, CI runs with `--locked` so the lockfile is enforced
      rather than advisory, which is the only way it constrains anything
- [ ] The `.gitignore` instruction is either made to work or removed, so it
      stops pointing at a change that does nothing
- [ ] The two dependencies already pinned by vendored source are noted, so a
      future reader does not assume the whole tree floats

## Why this is filed rather than fixed

Staging lockfiles is a repository-wide decision with real consequences in both
directions, and it is not the agent's to make. Committing 19 of them changes
every developer's first clone, every CI run, and every dependency bump for
everyone. Not committing them is a defensible position for a workspace that
publishes libraries, and this one publishes binaries, which is the case where
the usual guidance says otherwise — but that is exactly the tension a maintainer
should resolve rather than a default to pick.

The `.gitignore` trap is separable and could be fixed on its own, but changing
ignore rules has the side effect of surfacing 19 untracked lockfiles in everyone's
status output, so it belongs with the decision rather than ahead of it.

## Method note

Every figure came from the tree: lockfile count from a filesystem walk
excluding build and metadata directories, tracked count from `git ls-files`,
package and git-source counts from the root lockfile itself, and the ignore
rule from `git check-ignore -v` rather than from reading the file. Reading
`.gitignore` is what surfaced the instruction; asking git which rule matched is
what proved it inert.
