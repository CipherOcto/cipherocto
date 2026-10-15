---
name: the-commit-wrapper-scanned-two-of-the-three-ways-git-takes-a-message
description: 'scripts/wrappers/safe-commit re-parses git''s argument list to find commit messages. It recognised -m, --message, -m= and -F, but not the combined short flag that git fully supports, so `safe-commit -am "msg"` committed a dangerous message with exit 0 and no scan. Layer 3 of the commit-message defence was bypassed by ordinary usage.'
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-14T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0011-c
---

# Mission `the-commit-wrapper-scanned-two-of-the-three-ways-git-takes-a-message`

## Status

Fixed and mutation-verified. A new hermetic suite,
`scripts/test-safe-commit.sh`, covers the wrapper's argument handling and
fails on the pre-fix wrapper in exactly the two cases that were broken.

## The finding

`scripts/wrappers/safe-commit` is Layer 3 of the commit-message defence. It
works by re-parsing git's argument list so it can scan only the values that are
commit messages, then handing the **original** arguments to `git commit`.

The parser recognised four shapes:

| Shape              | Recognised |
| ------------------ | ---------- |
| `-m MSG`           | yes        |
| `--message MSG`    | yes        |
| `--message=MSG`    | yes        |
| `-F FILE` / `-F -` | yes        |
| **`-am MSG`**      | **no**     |

`git commit -am "msg"` means `-a -m msg`. `-am` matched none of the patterns,
so nothing was added to the scan list, `scan_text` was never called for it, and
the wrapper executed `git commit -am "msg"` with the dangerous message intact.

### Measured

```
$ safe-commit -am "subj body ; semicolon"
exit=0
[master ed0b4d2] subj body ; semicolon  1 file changed, 1 insertion(+)
REFUSED banners: 0
```

The semicolon is a token the shared library rejects everywhere else. Layer 3
exited 0 and created the commit.

### Why nothing else caught it

The commit-msg hook (Layer 2) runs on the commit regardless of how it was
invoked, so on a machine where `install-hooks.sh` has been run, the hook catches
what the wrapper missed. The bypass is only total where hooks are absent — a
fresh clone, or anywhere the wrapper is used as the intended substitute for
`git commit` without the hook installed.

The wrapper had **no tests at all**. `scripts/test-dangerous-tokens.sh` covers
the token library, which is correct and thorough, and says nothing about whether
the wrapper ever calls it.

## The fix

A short-flag cluster ending in `m` is now recognised:

```bash
-[a-zA-Z]*m)
  SCAN_TARGETS+=("ARG")
  ((IDX+1 < ${#ORIG_ARGS[@]})) && M_ARGS+=("${ORIG_ARGS[$((IDX+1))]}")
  ;;
```

Of git's own short commit options only `-m` contains the letter `m`, and its
value is still the next argument. The character class excludes a second leading
dash, so `--amend` and `--message=` are not caught here — they are handled by
their own branches.

A second defect surfaced while fixing the first: `-m` with no value appended
one entry to `SCAN_TARGETS` and none to `M_ARGS`, so the later read of
`${M_ARGS[$i]}` aborted under `set -u`:

```
./orig: line 88: M_ARGS[$i]: unbound variable
```

The read is now defaulted, and git's own message surfaces instead:
`error: switch 'm' requires a value`, exit 129.

## Verified in both directions

`scripts/test-safe-commit.sh` builds a throwaway repository per case, so each
assertion checks the commit count as well as the exit code — a refused message
must leave the repository at one commit, not merely print a banner.

| Wrapper | Result                                                      |
| ------- | ----------------------------------------------------------- |
| Fixed   | 10 passed                                                   |
| Pre-fix | 8 passed, **2 failed** — `combined -am` and `combined -qam` |

Both failures carry the signature of the defect: `rc=0 refused=0 commits=2` —
a dangerous message committed without a scan.

Safe messages still commit through `-m`, `-am`, `-qam` and `--message=`.

## A flaw in the new test, found by its own output

The first version incremented a counter inside a command substitution to name
its fixture directories. That increment happened in a subshell and was lost, so
two cases sharing a test name reused the same repository — and the bypass test
left two commits behind that the following test then counted.

It surfaced as `commits=4` where the assertion expected 2. Fixture directories
now come from `mktemp -d`, which is unique by construction and has no counter to
lose. Before fixing it, the suite reported 6 passed / 4 failed against the
pre-fix wrapper with two failures that were really one.

## Scope

`-amq` is deliberately **not** matched, because git rejects it: the value-taking
option must end a cluster. The wrapper forwards it and git exits 128 with its
own usage error, which is the correct division of labour.

## Acceptance criteria

- [x] `-am` and `-qam` messages are scanned and refused.
- [x] A refused message leaves the repository at one commit.
- [x] `-m` with no value produces a message, not an unbound-variable crash.
- [x] All supported message spellings still commit clean messages.
- [x] A hermetic suite exists and fails on the pre-fix wrapper.

## Notes

This is the third defect in the seven-layer commit-message defence found by
running a layer rather than reading it. The other two were in the token library
and the pre-receive hook.

The pattern is consistent: each layer is a small piece of argument or path
handling wrapped around a shared library, and each one had its own untested
assumption about how its input arrives. Fixing the library three times did not
help, because the library was never what was wrong in these cases — the callers
were not asking it the right question.
