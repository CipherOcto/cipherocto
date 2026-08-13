# Mission: marketplace-slashing-persistence

## Status

Open. Follow-on to Round 1 marketplace review (commit `264e2665`).
`SlashingLedger` is in-memory but its doc claims "production backed by
stoolap". Banned providers must remain banned across restart per
RFC-0900 §Slashing Model; current impl loses all state on process exit.

## RFC

RFC-0900 (Economics): Marketplace §Slashing Model

## Dependencies

- Round 1 marketplace review (commit `264e2665`) — substrate stable

## Acceptance Criteria

- [ ] Define `trait SlashStore` in `quota-router-storage`: `load_all() -> Vec<ProviderStake>`, `save(stake: &ProviderStake)`, `append_outcome(provider_id: &str, outcome: SlashOutcome)`
- [ ] Implement `StoolapSlashStore` in `quota-router-storage` backed by `slash_ledger` table
- [ ] `SlashingLedger::open(store: Arc<dyn SlashStore>)` constructor hydrates from `store.load_all()`
- [ ] `register`, `slash`, `slash_with_pct` write through to `store.append_outcome`
- [ ] Add restart-equivalence test: register + slash alice, drop ledger, open new ledger against same store, alice's cumulative_loss_pct + offense_count + ban status preserved
- [ ] Fix the misleading module doc (`in-memory; production backed by stoolap`)
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass + new persistence tests (≥3)

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/src/marketplace/slashing.rs:130-133` — doc + struct
- `crates/quota-router-core/src/task_market/slashing.rs:17` — wrapper
- `crates/quota-router-storage/` — new slash_ledger table + trait impl

Round 1 review context (Pass 2 MEDIUM #M2): production slashing state
evaporates on restart. Banned providers can re-enter marketplace.
Either introduce persistence OR correct the misleading doc + file
follow-on. This mission chooses the persistence path.

Pair with `marketplace-slash-reason-typed-discriminator` since both
touch `slashing.rs`.

## Version History

| Version | Date       | Change                                                                                                |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | Mission filed. Round 1 review follow-on. 9 ACs. |

Last Updated: 2026-08-13
Version: 0.1