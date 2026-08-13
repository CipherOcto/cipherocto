# Mission: marketplace-e2e-strong-scenarios

## Status

Open. Gap from marketplace E2E test audit. 76 marketplace E2E tests
exist across `eleven_step.rs` (1086 LoC), `marketplace_e2e.rs` (304
LoC), `task_market.rs` (658 LoC). Coverage is solid for happy path +
common errors but zero coverage of concurrent, partition, crash,
byzantine, withdraw, rotate, partial-fill adversarial scenarios.

## RFC

RFC-0900 (Economics): Marketplace
RFC-0901 (Economics): Task Market
RFC-0968 (Economics): Reputation Registry

## Dependencies

- Round 1 marketplace review (commit `264e2665`) — CRITICAL + HIGH fixes landed
- Round 1 follow-on missions filed (6 missions, commit `caa1cbfa`)

## Acceptance Criteria

### Strong-scenario E2E tests (each is a new test fn in existing files)

- [ ] **Concurrent settlement on same hash** — 2 producers race to
  settle same `settlement_hash`; only one succeeds, other returns
  `SettlementError::DuplicateSettlement` (or equivalent). New test
  `concurrent_settlement_duplicate_rejected` in eleven_step.rs.
- [ ] **Concurrent ask insertion at same price** — 2 producers race
  to `put_ask` same model/price. New test `concurrent_ask_insertion_seq_increments_monotonic`
  verifying per-book `next_seq` increments atomically and FIFO order
  is preserved.
- [ ] **Provider dies during escrow Locked state** — escrow enters
  Locked via `escrow.lock()`, then process crashes. New process
  reopens, finds Locked escrow in repo, must either commit or
  abort (test both paths; assert no fund loss). Test
  `escrow_recovery_from_locked_state_commits` +
  `escrow_recovery_from_locked_state_aborts` in marketplace_e2e.rs.
- [ ] **Byzantine provider — valid + invalid response interleaved**
  — provider returns valid response 99 times, invalid 1 time;
  assert reputation drops ≥ penalty threshold on the 1 invalid,
  not averaged away. Test
  `byzantine_provider_reputation_drops_on_offense` in marketplace_e2e.rs.
- [ ] **Stake withdrawal race** — provider stakes 1000 micro-OCTO,
  gets banned (5 offenses), tries to withdraw 800 micro-OCTO
  before slash completes. New test
  `stake_withdrawal_rejected_while_pending_slash` in task_market.rs.
- [ ] **Provider key rotation** — provider registers with key_v1,
  accumulates 4.5 reputation score, then rotates to key_v2.
  Asserts: (a) old reputation carries over per RFC-0968
  (reputation is per-controller-did, not per-key); (b) new
  key inherits ledger history. Test
  `provider_key_rotation_preserves_reputation` in marketplace_e2e.rs.
- [ ] **Partial fill at exact boundary** — buyer bids 10 qty, seller
  asks 3 + 7 (same price). Test: first match fills 3 from ask1,
  re-inserts 0 qty for ask1 (or removes), second match fills 7
  from ask2. Asserts no qty loss + no double-spend. Test
  `partial_fill_exact_boundary_no_qty_loss` in marketplace_e2e.rs.
- [ ] **Stale view across process boundary** — process A writes ask,
  process B reads via open_path. Asserts B sees the write per the
  load-on-open contract (verifies H3 marketplace-book-load-on-open
  fix when it lands). Test
  `stale_view_after_restart_sees_writes` in marketplace_e2e.rs.

### Verification

- [ ] All 7 new tests pass under `cargo test -p quota-router-core --test marketplace_e2e`
- [ ] All 7 new tests pass under `--release`
- [ ] Tests run in <5s total (so they're cheap to add to CI fast lane)
- [ ] No new dependencies required (use existing tokio + Arc primitives)
- [ ] Clippy passes with zero warnings
- [ ] All existing 76 tests still pass

## Claimant

(unclaimed)

## Pull Request

#

## Notes

**Why these 7 scenarios** — chosen by mapping the marketplace state
machine surface to the most likely production failure modes. Each
test pins a specific code path:

1. **Concurrent settlement** — SettlementEnvelope dedup contract
   (RFC-0900 §Settlement). Currently tested with single producer
   (settlement_envelope_first_seen_succeeds + replay_rejected) but
   the race is the production failure mode.
2. **Concurrent ask insertion** — per-book `next_seq` counter added
   in C1 fix (commit `264e2665`); race coverage is the only way to
   prove the fix.
3. **Provider dies Locked** — Round 1 C3 fix dropped Escrow Clone
   (prevents double-settle) but didn't add recovery semantics.
   Two test paths: commit-on-restart (auto-settle with provider
   trust signal) vs abort-on-restart (timeout after T seconds →
   buyer refund).
4. **Byzantine provider** — RFC-0968 reputation math must
   penalize per-offense, not average. Slashing test
   `repeated_offenses_eventually_ban_provider` covers the macro
   trend; byzantine test covers the per-offense precision.
5. **Stake withdrawal race** — SlashingLedger (Round 1 M2 follow-on
   in `marketplace-slashing-persistence`) needs a per-stake
   lock; this test pins the contract.
6. **Provider key rotation** — RFC-0968 specifies per-controller
   reputation; tests currently use single key. Rotation test
   covers the HSM seed-rotation path (RFC-0009 §Lifecycle).
7. **Partial fill at exact boundary** — Round 1 C2 fix
   re-inserts residual; exact-boundary test pins zero-qty
   handling (BTreeMap ordering on `(price, seq=0)` for residue).

**Scenario 8 (stale view across process)** depends on
`marketplace-book-load-on-open` (filed but not yet landed); mark
DEFERRED until that follow-on merges.

## Version History

| Version | Date       | Change                                                                                                |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | Mission filed. 7 strong-scenario E2E tests scoped. 1 test (stale view) DEFERRED on Round 1 follow-on landing. |

Last Updated: 2026-08-13
Version: 0.1
