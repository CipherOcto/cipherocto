# Mission: reputation-compat-controller-id-test-drift

## Status

Open. Pre-existing drift surfaced by `cargo test -p
quota-router-core --lib` on 2026-08-13 (1/1634 fail). NOT caused by
the #102 admin_rate_limit diff.

## RFC

RFC-0968-A1 amendment 40 (controller_id reservation).

## Dependencies

- Commit `eb6aaf34` (feat(reputation): canonical
  `ControllerIdMissing` variant, 0x34) — already landed on `next`,
  queued, not pushed.

## Acceptance Criteria

- [ ] Update
      `crates/quota-router-core/src/marketplace/reputation_compat.rs::tests::record_with_now_rejects_zero_controller_id`
      (line 425) to assert
      `ReputationError::ControllerIdMissing` instead of
      `ReputationError::RecorderDidMalformed`.
- [ ] Remove the now-stale "Surfaces via RecorderDidMalformed
      pending the dedicated ControllerIdMissing variant" comment.
- [ ] Re-run `cargo test -p quota-router-core --features full --lib`
      with `LD_LIBRARY_PATH` set → 1634/1634 pass.
- [ ] Clippy clean.

## Claimant

(unclaimed — auto-surfaced during #102 verify sweep)

## Notes

The variant rename (`RecorderDidMalformed` → `ControllerIdMissing`)
landed via commit `eb6aaf34` on 2026-08-13 (this session). The
existing test was NOT updated. The marketplace async tests
(`crates/quota-router-core/tests/marketplace_reputation_async.rs`)
were updated as part of mission `octo-reputation-controller-id-missing-variant`
(#96); the in-module test was missed.

**Fix is mechanical** — 1 line of code + 3 lines of comment. ~5
minutes of work. Landed alongside the next `quota-router-core`
patch (or as its own micro-commit).

**Why this is its own mission rather than folded into the next
patch:** the test-drift is conceptually independent of #102
(admin_rate_limit). Folding it into a future patch makes blame
noisy ("did the rate-limit work break the reputation test?") — a
single-line mission row preserves history clarity.

## Cross-references

- Commit `eb6aaf34` feat(reputation): canonical
  `ControllerIdMissing` variant (0x34)
- Mission `octo-reputation-controller-id-missing-variant` (#96)
- RFC-0968-A1 amendment 40 (all-zero controller_id reserved)
