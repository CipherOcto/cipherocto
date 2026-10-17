---
name: the-installer-accepted-a-misspelled-flag-and-the-test-that-should-have-caught-it-passed-for-the-wrong-reason
description: "scripts/install.sh exits 0 on an unknown flag, so a typo silently drops every other flag on the command line and reports success. scripts/install_test.sh has a test asserting this must fail, and it passed — because a logging crash exited non-zero, not because the flag was rejected. The suite is not wired to anything and had never been run."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-14T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `the-installer-accepted-a-misspelled-flag-and-the-test-that-should-have-caught-it-passed-for-the-wrong-reason`

## Status

Fixed and mutation-verified. `scripts/install_test.sh` goes from **16 passed /
1 failed** on its first-ever run to **17 passed**, and the suite now catches
both defects when either is individually reverted.

## The finding

`scripts/install_test.sh` is a hermetic test suite for the 529-line
`scripts/install.sh`. It redirects `HOME` to a tmpdir, so it is safe to run. It
is referenced by `docs/distribution.md` as a CI snippet and by nothing else —
no workflow runs it, no script invokes it. It had never been executed.

Its first run reported **16 passed, 1 failed**. Both halves of that turned out
to be defects, and one of them was hiding a second defect behind it.

### A typo exits 0

The flag parser routed unknown flags into the same branch as `--help`:

```bash
*) log_error "unknown flag: $1"; PRINT_HELP=1 ;;
...
if [[ $PRINT_HELP -eq 1 ]]; then usage; exit 0; fi
```

Measured, with the state directory present — the state of any machine that has
run the installer once:

| Command                                 | Exit | What happened                              |
| --------------------------------------- | ---- | ------------------------------------------ |
| `install.sh --bogus`                    | 0    | printed usage, reported success            |
| `install.sh --sikp-binary`              | 0    | typo dropped, binary installed             |
| `install.sh --with-aider --sikp-binary` | 0    | typo dropped, **Aider shim not installed** |

The last row is the damaging one. The user asked for two things, the installer
did one, said nothing about the other, and exited 0.

### The test that should have caught it passed for the wrong reason

`test_unknown_flag_exits_nonzero` asserts exactly this must fail. It passed. The
reason was not flag rejection:

```
tee: /tmp/tmp.KIU2oYn9mw/.local/state/octo-whatsapp-install/install.log:
     No such file or directory
```

`log_error` is called from the flag parser, which runs **before** `log_init`
creates `$STATE_DIR`. The `tee` failed on a missing directory and `set -euo
pipefail` killed the installer with exit 1. The test was observing a logging
crash.

So the suite contained a test whose name described the property, whose assertion
matched for the wrong reason, and which would have kept passing while the
property it names was absent on every real machine.

### The test that did fail was failing for a wrong reason too

`test_no_envs_exits_zero_no_writes` failed with `spurious .claude created`. The
installer was right and the test was wrong. `detect_envs` treats `claude_code`
as present when **either** `$HOME/.claude` **or** `$REPO_ROOT/.claude` exists,
and this repository has a `.claude` directory. A fresh tmp `HOME` alone does
not make the environment list empty.

## The fix

`install.sh`:

- An unknown flag sets `BAD_FLAG` and exits **2**, distinct from `--help`'s 0.
  The documented exit-code table gained the usage-error case.
- `log_error` ensures `$STATE_DIR` exists and never lets logging decide the exit
  code (`|| true` on the `tee`).

`install_test.sh`:

- Test 16 now pre-creates the state directory, so the crash path is unavailable
  and only a real rejection can satisfy it. It also covers a misspelled _real_
  flag and asserts `--help` still exits 0.
- Test 16 additionally asserts the error is **recorded** in the log file in a
  fresh `HOME`, which is the property the crash violated.
- Test 17 copies the installer into a throwaway tree with no `.claude`, so
  `REPO_ROOT` resolves where the repo probe cannot fire — the state the test
  actually meant to describe.

## Verified in both directions

| Condition                          | Result                                                      |
| ---------------------------------- | ----------------------------------------------------------- |
| Fixed                              | 17 passed, exit 0                                           |
| Revert the explicit rejection only | 16 passed, **1 failed** (`test_unknown_flag_exits_nonzero`) |
| Revert the logging hardening only  | 16 passed, **1 failed** (same test)                         |

The two mutants were also checked directly:

| Condition                          | Exit | Log written |
| ---------------------------------- | ---- | ----------- |
| Fixed, fresh `HOME`                | 2    | yes         |
| No rejection, state dir present    | 0    | yes         |
| No logging hardening, fresh `HOME` | 1    | **no**      |

The last two rows are the original defects, recovered.

## A false alarm I raised and then disproved

Before running the suite I read its header claiming the tests "auto-detect
`SKIP_BINARY=1`", found no such detection in `install.sh`, and concluded the
suite would run a real `cargo install` — on a disk with 3.6G free, where that
would have been a serious mistake.

That conclusion was wrong, and the reason was a measurement error: I grepped for
the literal string `install.sh` followed by flags. The tests invoke
`"$INSTALL_SH"`, a variable, so my grep never saw the `--skip-binary` flag that
every one of them passes. A second attempt to classify the invocations used
`grep -oE '[^\n]*'`, in which `\n` is not a newline — it truncated every match
and reported all twenty invocations as unsafe.

The safety property was real throughout. **Two of my three checks on this file
were wrong before the direct run**, and only running it settled the question.

## Not fixed

`install_test.sh` creates two `mktemp -d` directories per test and installs no
`trap` to remove them, so every run leaks 34 directories. They are small — disk
moved 3.7G to 3.6G across this whole session's work — but on a volume at 100%
the leak is a real habit. It is recorded rather than fixed, because the fix is
unrelated to the defects above and would deserve its own verification.

## Acceptance criteria

- [x] An unknown or misspelled flag exits non-zero.
- [x] A misspelled flag does not silently drop the other flags on the line.
- [x] `--help` still exits 0.
- [x] An error raised before `log_init` is recorded rather than crashing.
- [x] The suite catches the rejection defect when reverted.
- [x] The suite catches the logging defect when reverted.
- [x] Test 17 describes a state the installer can actually be in.
- [x] All 17 tests pass.

## Notes

Three unrelated-looking failures in one unwired suite, all found in the first
execution: a test passing for the wrong reason, a test failing for the wrong
reason, and a genuine silent-success defect sitting underneath the first.

The suite is 17 tests and runs in under a second with no network and no build.
It was written to be run and never was. Nothing in this repository would have
told anyone that, because a test suite nobody runs cannot report anything —
including that it has never run.
