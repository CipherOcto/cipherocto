# CoordinatorAdmin impl: adversarial review round 4 (R23f)

**Date:** 2026-06-18
**Branch:** `next`
**Scope:** Verify the R23e fixes (hard shutdown, doc/test hygiene, .. server,
poison warning) and look for regressions or remaining issues.

## Verification of R23e fixes

| ID  | Finding                                                       | R23e fix                                                       | Verified? |
|-----|---------------------------------------------------------------|----------------------------------------------------------------|-----------|
| N14 | shutdown() doc contradicted code; ensure_connected could respawn after shutdown | New `shutting_down: AtomicBool` field; ensure_connected returns Err("shut down") after shutdown; doc updated | ✅ test_shutdown_prevents_respawn asserts both the flag is set AND ensure_connected/send_raw_line return Err post-shutdown |
| N15 | test_shutdown_clears_state_and_listener_can_respawn didn't verify respawn | Renamed to test_shutdown_prevents_respawn, added actual assertions: flag set, ensure_connected returns Err, send_raw_line returns Err | ✅ Test passes; renamed appropriately to reflect new contract |
| N16 | test_join_by_invite_records_runtime_channel doc misdescribed flow | Rewrote doc to explain listener IS spawned, send_raw_line SUCCEEDS, push happens AFTER send_raw_line succeeds | ✅ Test passes with corrected understanding |
| N17 | Tests leaked listener tasks (no shutdown call)                | Added `adapter.shutdown().await.unwrap()` to the two tests that actually spawn listeners (the two reject tests don't leak because validate_channel_name fails before send_raw_line) | ✅ No more listener tasks outliving tests |
| N18 | runtime_channels doc said "channel_for is sync helper"        | Rewrote rationale: lock IS safe in async context because critical section has no .await | ✅ |
| N19 | validate_server accepted ".." (empty labels)                 | Added contains("..") rejection; extended test | ✅ test_irc_config_validate_rejects_bad_server_names now covers ".." and "irc.example.com.." |
| N20 | runtime_channels poison silently swallowed after successful JOIN | Added tracing::warn! so operator sees the divergence | ✅ |

Net: 49 → 50 tests (one new test added in this round). All pass.

## New findings introduced by R23e (R23f)

### HIGH

**N21 — `ensure_connected` and `shutdown` have a race that leaves a zombie listener.**

The R23e fix added the `shutting_down: AtomicBool` flag and a check in
`ensure_connected`, but the check was OUTSIDE the `connected` lock
acquisition. Worse, `shutdown` took the related state (`shutdown_tx`,
`out_tx`, `listener_handle`) *before* acquiring `connected`. The
window where they can race:

```
ensure_connected                              shutdown
─────────────────────────────────────────────────────────────
shutting_down check (false) ←── store(true)
validate config
lock connected                                shutting_down.store(true)
check connected (false)                       
build out_tx                                   take shutdown_tx (None!)
install out_tx                                 set out_tx = None (or no-op)
build shutdown_tx                              take listener_handle (None!)
install shutdown_tx ←── races                  
spawn task                                     take connected (blocks on T1)
install listener_handle                        set connected = false
set connected = true                           abort (None, no-op)
release connected                              release
                                               ←── T1 finished install
                                               END STATE: zombie listener
                                                       (shutdown_tx=Some,
                                                        listener_handle=Some,
                                                        neither signalled
                                                        nor aborted)
```

If `ensure_connected` is mid-spawn when `shutdown` runs, `shutdown`'s
`shutdown_tx.take()` and `listener_handle.take()` may both see `None`
(ensure_connected hasn't installed yet) while `ensure_connected`'s
remaining installs succeed *after* shutdown has finished its work.
The result: a `JoinHandle` in `listener_handle`, a watch sender in
`shutdown_tx`, but neither will ever be signalled or aborted by
shutdown. The listener runs until the adapter is dropped.

#### Two-part fix

**Fix 1 (root cause): `shutdown` must acquire `connected` as its FIRST step.**
After setting the `shutting_down` flag, shutdown takes `connected` *before*
touching `shutdown_tx` / `out_tx` / `listener_handle`. This means shutdown
is serialized with `ensure_connected`'s entire spawn sequence:
- If `ensure_connected` is mid-spawn (holds `connected`), shutdown blocks
  until it finishes, then takes all the `Some`s shutdown_tx / out_tx /
  listener_handle, signals, aborts.
- If `shutdown` acquires first, all the `take()`s see `None` (because
  ensure_connected hasn't installed yet), then `ensure_connected` acquires
  `connected` after shutdown releases — but now `connected == false` and
  no spawns have been registered, so ensure_connected proceeds (or, with
  Fix 2, refuses due to `shutting_down`).

**Fix 2 (defense in depth): `ensure_connected` re-checks `shutting_down` inside the `connected` lock.**
If shutdown completed between `ensure_connected`'s *outside* check and
its `connected.lock().await` acquisition, the `connected` lock might be
acquired after shutdown has set `connected = false`. The inside-lock check
catches this case and returns `Err`, preventing a spawn-after-cleanup.

Both fixes are required:
- Fix 1 alone closes the "shutdown takes None, then ensure_connected
  installs Some" race window by serializing the two state machines on
  the connected lock.
- Fix 2 alone is *insufficient* if shutdown can complete its `take`s
  before ensure_connected acquires connected (then ensure_connected
  sees a clean state and proceeds to spawn — zombie).
- Fix 1 alone is also insufficient for the "shutdown completed before
  ensure_connected's lock acquisition" case (then ensure_connected
  would proceed without re-checking). Fix 2 catches this.

#### Regression test

`test_ensure_connected_shutdown_race_no_zombie` runs the race 100 times
under `multi_thread` with 4 workers. Without the fix, **8 of 8 runs
panicked** at the `!handle_still_present` assertion. With the fix,
**10 of 10 runs pass**.

### MEDIUM

None.

### LOW

None.

## Still unaddressed from R1

These were in the original R1 review and are not IRC-blocking; they're
WhatsApp-side or pre-existing design choices. They remain deferred to a
future WhatsApp-focused review round:

- **H1:** WhatsApp `can_join_by_invite=true` but `join_by_invite` is `Unimplemented`
- **H2:** WhatsApp `create_group` signature disambiguation footgun
- **H6:** WhatsApp `add_member` partial-success
- **M1, M4, M5, M10-M16:** WhatsApp-side
- **M3:** `health_check` ignores `use_tls` (IRC)
- **M7:** `add_member` doesn't require op (IRC, by design)
- **M8:** `health_check` doesn't call `ensure_connected` (IRC)

## Action plan for R23g (this round's fixes)

1. **N21 (HIGH)** — Apply the two-part fix described above; add
   `test_ensure_connected_shutdown_race_no_zombie` regression test.

This is the only finding in this round. The other 7 R23e findings
(N14-N20) were addressed correctly and their tests pass.