---
name: the-pre-receive-hooks-documented-install-command-produces-a-hook-that-refuses-every-push
description: "Layer 5 of the commit-message defence carries its own install instructions in its header. Following them verbatim copies only the hook file, leaving the token library it sources missing. The hook then fails every push. The same hook says nothing on the success path, so a push that examined zero commits is indistinguishable from a push that examined five hundred."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-14T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0011-c
---

# Mission `the-pre-receive-hooks-documented-install-command-produces-a-hook-that-refuses-every-push`

## Status

Both reachable defects are **fixed and verified against a real push**. The
install instructions were followed verbatim, and the failure they produced was
reproduced before the fix and gone after it.

## The finding

`scripts/hooks/pre-receive-no-backtick` is Layer 5 of the seven-layer defence
against the 2026-09-08 commit-message incident. Its header documents its own
installation:

```
cp scripts/hooks/pre-receive-no-backtick /path/to/repo.git/hooks/pre-receive
chmod +x /path/to/repo.git/hooks/pre-receive
```

The hook resolves its token list as
`"$HOOK_DIR/../wrappers/dangerous-tokens.sh"` — **Layer 5 is defined entirely by
that shared library**, the same one the other six layers source. The command
copies one file and omits the other.

Reproduced by following the instruction verbatim against a fresh bare
repository and pushing:

```
remote: hooks/pre-receive: line 23: .../hooks/../wrappers/dangerous-tokens.sh:
       No such file or directory
 ! [remote rejected] pushme -> p1 (pre-receive hook declined)
error: failed to push some other refs
```

Every push is refused. The failure is loud and names the missing path, so this
is not a silent fail-open — it is a guard that cannot be installed by following
its own documentation, which is why it was never noticed. Self-hosted installs
are exactly the deployments Layer 5 exists for; GitHub cannot run pre-receive
at all, so the server side is the only place this guard runs.

**Fixed and re-verified.** The instructions now create the `wrappers/`
directory and copy the library. Following the corrected commands verbatim: the
dangerous push is refused by the token scan, and the clean push is accepted.
No "No such file or directory".

## The success path said nothing

The hook printed only on failure. A push that scanned five hundred commits and a
push that scanned **zero** produced byte-identical output: nothing, exit 0.

Zero is reachable and legitimate. For a newly created branch the hook selects
`git rev-list "$newrev" --not --all`, which excludes anything already on the
server. When every pushed revision is already present, the list is empty:

```
pre-receive: 1 ref(s), 0 new commits (1 already present on this server)
```

That line is new. The hook now states the population it examined on every path:

| Situation                       | Disclosure                                                 |
| ------------------------------- | ---------------------------------------------------------- |
| deletion only                   | `no ref updates to examine`                                |
| ref fully present on the server | `1 ref(s), 0 new commits (1 already present...)`           |
| normal push                     | `scanned 2 commit(s) across 1 ref(s), no dangerous tokens` |

A refusal prints the banner and no coverage line, so a blocked push is never
followed by a reassuring one.

## A third defect that is real but NOT reachable, tested rather than assumed

The stdin loop was `while read -r oldrev newrev refname`, which discards a final
line that has no trailing newline — the classic bash EOF-without-delimiter trap.
Fed directly, that drops a dangerous ref and exits 0:

```
evil on last line, no trailing newline -> exit=0    (ref silently dropped)
evil on last line, WITH newline      -> exit=1    (ref refused)
```

Whether git ever produces that input was **not assumed**. `od -c` on the real
stdin of an actual push:

```
0000140   o   n   s  \n
0000143
```

The newline is the last byte. The path is therefore **latent, not live**. It is
hardened anyway — `|| [[ -n "$newrev" ]]` costs nothing and the same test now
returns 1 where it returned 0 — but it is recorded as a hardening, not as a fix
for an exploitable defect.

## Two harness mistakes this test produced

Both would have been reported as findings and both were wrong.

1. The first run committed directly onto `main`, so `--not --all` correctly
   excluded the new commit and the hook scanned nothing. A pushed commit is not
   yet on any ref **on the server**; a local ref is not the same thing. Fixed by
   building the commit and then deleting its ref, leaving an unreachable object.
2. A later re-test returned exit 0 for a dangerous commit, which looked like a
   regression from the disclosure edit. It was not: the branch was still a live
   local ref, so the hook was right to skip it. Confirmed with
   `git rev-list --not --all` before changing anything.

A test that passes for the wrong reason is worse than one that fails.

## Acceptance criteria

- [x] Documented install instructions copy the token library the hook sources.
- [x] The corrected instructions were followed verbatim against a fresh bare
      repository: dangerous push refused, clean push accepted.
- [x] The hook states the population it examined on every success path.
- [x] The success path distinguishes zero commits from a full scan.
- [x] A refusal prints no coverage line, so a blocked push is never followed by
      a reassuring one.
- [x] The unterminated-last-line drop is hardened, with its unreachability
      proven from real push bytes rather than assumed.

## Notes

Found by asking a narrower question than last time. The previous mission came
from asking which enforcement scripts sat outside every harness; this one came
from asking whether the guard just fixed was the guard that actually runs.

That question is worth its own answer: `.git/hooks/commit-msg` and
`.git/hooks/pre-receive` are **symlinks to the tracked scripts**, and
`core.hooksPath` is set to `.git/hooks`. The Layer 2 fix in
`two-dangerous-token-rules-could-not-fire-and-one-was-silenced-by-any-earlier-word`
therefore reaches the live guard rather than a dead copy. That was checked
before the work was called done, not after.

A hook that has never been run is a hypothesis. This one had not been executed
once; every claim above came from pushing to a real bare repository.
