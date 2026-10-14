---
name: three-reference-root-scripts-report-a-root-derived-from-failed-entries
description: "scripts/compute_*_probe_root.py produce the canonical Merkle roots pinned as test vectors. Three of the eight turned a failed entry into a zero hash and still printed a well-formed root, or printed Match: False and exited 0. A run in which every entry failed produced a confident reference value and a success exit code."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-14T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0110
---

# Mission `three-reference-root-scripts-report-a-root-derived-from-failed-entries`

## Status

Fixed and verified in both directions. All eight scripts exit 0 on the healthy
path, and the two reference roots they print are byte-identical to what they
printed before the change.

## The finding

`scripts/compute_*` are hand-run reference implementations. They compute the
canonical Merkle roots and config hashes that get pasted into pinned test
vectors, so **the value they print is copied by a human**. Nothing in this
repository invokes them: zero references in `.github/`, zero in `scripts/`.
Their failure modes are therefore not a CI risk — they are a way for a person to
paste a wrong number into a constant and believe it.

Eight scripts were classified by running them, not by reading their filenames:

| Script                           | Error handling                      | On failure         |
| -------------------------------- | ----------------------------------- | ------------------ |
| `compute_bigint_probe_root.py`   | `except Exception` → zero hash      | **printed a root** |
| `compute_decimal_probe_root.py`  | `except Exception` → zero hash (×2) | **printed a root** |
| `compute_decimal_config_hash.py` | prints `Match: False`               | **exited 0**       |
| `compute_dlae_probe_root.py`     | `raise`, no handler                 | traceback, exit 1  |
| `compute_dmat_probe_root.py`     | none                                | traceback, exit 1  |
| `compute_dvec_probe_root.py`     | none                                | traceback, exit 1  |
| `compute_dact_probe_root.py`     | compares and asserts                | exit 1             |
| `compute_dcs_probe_root.py`      | compares and asserts                | exit 1             |

Five of eight already failed closed — three of them by accident, by having no
error handling at all. Three reported a wrong result as a result.

### Measured

A run in which **all 56 BigInt probe entries raise**:

```
 56 entries -> tree...
MERKLE ROOT: d88ddfeed400a8755596b21942c1497e114c302e6118290f91e6772976041fa1
exit=0
```

The healthy root is `c447fa82db07…`. The failing run produced a different,
well-formed, entirely-placeholder-derived root and reported success. The only
signal was 56 `ERROR:` lines scrolling past above it.

`compute_decimal_config_hash.py` printed `Match: False` and exited 0, so a
caller checking `$?` — a shell loop, a step, an agent — saw success while the
script reported a mismatch. A verdict that is not in the exit status is a
comment, not a check.

### A false claim in the output

`compute_decimal_probe_root.py` printed `57 entries computed` unconditionally,
including runs where entries had errored. It now prints the real count:
`0 of 57 entries computed` in the all-fail case above.

## The fix

Failed entries are still recorded as zero hashes so the tree can be printed for
diagnosis, but the run now **refuses to emit a reference value**:

```
56 of 56 probe entries FAILED.
  0 CANONICALIZE: injected probe failure
  ...
NOT A REFERENCE VALUE (computed over 56 zero-hash placeholder(s)): d88ddfe…
exit=1
```

Deliberately **not** the bare `MERKLE ROOT:` label. That string is the thing a
reader pastes into a constant; a tree built over placeholders is exactly the
value that must never be pasted.

`compute_decimal_config_hash.py` now names `EXPECTED` once — the printed
expectation and the comparison were two separate literals that could drift — and
exits 1 on a mismatch.

## Verified in both directions

| Script                           | Healthy               | Mutation                                                        |
| -------------------------------- | --------------------- | --------------------------------------------------------------- |
| `compute_bigint_probe_root.py`   | exit 0, `c447fa82…`   | all entries raise → **exit 1**, no bare root                    |
| `compute_decimal_probe_root.py`  | exit 0, `496bc803…`   | all entries raise → **exit 1**, no bare root                    |
| `compute_decimal_config_hash.py` | exit 0, `Match: True` | wrong `EXPECTED` → **exit 1**; `PRECISION_CAP` 6→7 → **exit 1** |

The two probe roots are byte-identical before and after the change, so the fix
costs nothing on the healthy path. All eight scripts exit 0 when run normally.

## Two mistakes made while verifying this

**A vacuous mutation that confirmed itself.** The first attempt to break
`compute_bigint_probe_root.py` added `raise` inside the `except` block — which
only runs if an exception already occurred, so nothing ever failed. The mutant
printed the _same_ root as the healthy script, which was briefly
indistinguishable from the far more alarming claim that failures do not change
the root. It was caught only because the two roots were compared directly.
Re-injecting the failure into the _entry computation_ produced the different
root above. A mutation that cannot fail proves nothing, including when it
agrees with you.

**A wrong count in the new diagnostic.** `compute_decimal_probe_root.py` folds
`hashes` down to one node while building the tree, so a summary written after
the build reported `57 of 1 probe entries FAILED`. The count is now captured
before the fold.

## Scope

`compute_dlae`, `dmat` and `dvec` were left alone. They have no error handling,
so a failure crashes with a traceback and a non-zero exit. That is loud and
correct by accident rather than by design, and adding handling to a script that
works is not the same as fixing one that does not. They are recorded here so the
classification is complete.

## Acceptance criteria

- [x] A run with any failed entry exits non-zero.
- [x] No bare `MERKLE ROOT` / `DECIMAL PROBE MERKLE ROOT` label is printed when
      entries failed.
- [x] The "N entries computed" line reports what actually happened.
- [x] `compute_decimal_config_hash.py` exits non-zero on mismatch.
- [x] Both reference roots are unchanged on the healthy path.
- [x] All eight scripts exit 0 when run normally.
- [ ] The pinned roots in the consuming crates are re-derived under the fixed
      scripts. They are byte-identical on the healthy path, so this is expected
      to be a no-op — but it was not run, because no crate in this repository
      is built by a probe script and the check would be circular.

## Notes

This is the same defect the whole framework keeps producing: a value computed
over a population that was not fully examined, reported as though it were. The
difference here is that the population is the script's own inputs, so there is
no external corpus to blame — the script simply decided that a failed entry was
a zero.

The family was classified by running all eight, which took seconds. Reading the
filenames — `compute_*_probe_root.py`, eight near-identical names — would have
produced one guess applied eight times.
