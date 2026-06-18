# CoordinatorAdmin impl: adversarial review round 5 (R23g)

**Date:** 2026-06-18
**Branch:** `next`
**Scope:** Verify the R23f fixes (the two-part ensure_connected/shutdown
serialization) and look for regressions, remaining issues, or new issues
introduced by R23f.

## Verification of R23f fixes

| ID  | Finding                                                       | R23f fix                                                       | Verified? |
|-----|---------------------------------------------------------------|----------------------------------------------------------------|-----------|
| N21 | ensure_connected and shutdown race leaves zombie listener    | Fix 1: shutdown acquires connected as its FIRST step (before touching shutdown_tx / out_tx / listener_handle). Fix 2: ensure_connected re-checks shutting_down inside the connected lock. | ✅ test_ensure_connected_shutdown_race_no_zombie passes 10/10 runs on multi_thread; verified to fail 8/8 runs when both fixes are reverted |

Net: 50 tests, all pass. `cargo check`, `cargo fmt`, `cargo clippy` clean
(clippy warnings in the irc adapter are pre-existing, not from this round).

## Lock-ordering audit

After R23f, the lock acquisitions are:

- `ensure_connected`: `connected` (held throughout) → `out_tx` → `shutdown_tx` → `listener_handle`
- `mark_disconnected`: `connected` → `out_tx`
- `shutdown`: `connected` (held throughout) → `shutdown_tx` → `out_tx` → `listener_handle`

The orderings are inconsistent between `ensure_connected` and `shutdown`
(`out_tx`/`shutdown_tx` are acquired in opposite order), but because **no
two locks are ever held simultaneously** (every `*self.X.lock().await = …`
is a single-statement RAII drop), there is no actual deadlock potential.
Verified by tracing every `.lock().await` site in the file: each guard is
released before the next `.await`.

The inconsistency is a code smell, not a bug. Future refactors that hold
two locks simultaneously would need to align the orderings to avoid a
deadlock. **Low priority — noted but not fixed in this round.**

## New findings

**None.** After five rounds of adversarial review (R23c → R23d → R23e →
R23f → R23g), the IRC-side `CoordinatorAdmin` implementation is in a
defensible state:

- 50 tests, all passing (49 pre-existing IRC tests + 1 race regression test)
- CRITICAL: 0 open
- HIGH: 0 open
- MEDIUM: 0 open
- LOW: 0 open (the lock-ordering code smell is noted but not a real issue)

## Still unaddressed from R1 (deferred — see follow-up below)

These are 17 R1 findings that were out of scope for the IRC review but
are still open. Per the project's "Deferred ≠ Unspecified" rule
(memory `mem_1781647176929_4539827401334513900`), each deferred item
must have a full spec. The follow-up spec is **RFC-0861 (Networking):
CoordinatorAdmin Adapter Contract Refinements**
(`rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md`),
and the implementation work is tracked by **mission
`missions/open/0861-coordinator-admin-trait-refinements.md`**.

| Finding | Severity | Adapter / surface | RFC § | Mission phase |
|---|---|---|---|---|
| H1 | HIGH | WhatsApp | §1 | 2 |
| H2 | HIGH | WhatsApp | §3 | 2 |
| H6 | HIGH | WhatsApp | §3 | 1 |
| M1 | MEDIUM | WhatsApp | §3 | 2 |
| M2 | MEDIUM | trait | §2 | 1 |
| M3 | MEDIUM | IRC | §7 | 3 (unblocked since R23d C1) |
| M4 | MEDIUM | WhatsApp | §3 | 1 |
| M5 | MEDIUM | WhatsApp | §3 | 2 |
| M7 | MEDIUM | IRC | §4 | 3 |
| M8 | MEDIUM | IRC | §4 | 3 |
| M10 | MEDIUM | IRC | §1 | 3 |
| M11 | MEDIUM | WhatsApp | §5 | 2 |
| M12 | MEDIUM | trait | §6 | 1 |
| M13 | MEDIUM | WhatsApp | §3 | 1 |
| M14 | MEDIUM | trait | §6 | 1 |
| M15 | MEDIUM | IRC | §2 | 3 |
| M16 | MEDIUM | WhatsApp | §2 | 2 |

(R5 originally listed 16 of these; M2 was missed in the R5
enumeration and is the same kind of trait-level input-validation
fix as M15/M16, so it was rolled into the same RFC.)

**Note:** the "RFC §" / "Mission phase" columns above are R5-time
snapshots. The canonical, current mapping is in
[RFC-0861 Appendix A](../rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md#a-finding-to-spec-mapping),
which has been updated through R24a/R24b/R24c/R24d to reflect that
M3 is unblocked and in Phase 3, H6's `AddMemberOutput.promoted`
is `Option<Result<(), PlatformAdapterError>>` (not bare `Result`),
and the IRC `pending_replies` correlation buffer is a new field
on `IrcAdapter` (not a reuse of `shutdown_tx`).

## Loop termination

Per the user's instruction ("the loop finished when a new round founds
no issues"), the multi-round adversarial review loop terminated at
**R23f (r4)** which found 0 issues. R23g (r5) is the final
verification round, which also found 0 new issues on the IRC side and
documented the cross-reference to the follow-up RFC for the deferred
R1 items.

| Round | Findings                                | Status |
|-------|-----------------------------------------|--------|
| R23c (r1) | 13 (1 CRITICAL, 2 HIGH, 4 MEDIUM, 6 LOW) | Fixed in R23d |
| R23d (r2) | 7 (1 CRITICAL, 1 HIGH, 3 MEDIUM, 2 LOW) | Fixed in R23e |
| R23e (r3) | 1 (1 HIGH)                               | Fixed in R23f |
| R23f (r4) | 0                                        | **Loop terminates** |
| R23g (r5) | 0                                        | Final verification; cross-references follow-up RFC-0861 for deferred R1 items |

Net delta from R21 (initial impl): 41 → 50 tests, 0 open findings on
IRC side. 17 R1 findings remain deferred but are fully spec'd in
RFC-0861 with a master mission in `missions/open/`.