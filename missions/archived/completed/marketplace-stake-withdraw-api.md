# Mission: marketplace-stake-withdraw-api

## Status

Closed 2026-08-13 (@claude). LANDED.

## RFC

RFC-0900 (Economics): Marketplace + RFC-0901 (Task Market) — provider
staking model. The slashing ledger tracks `stake_micro_octo_w` per
provider DID but has no `withdraw` API; the
`stake_withdrawal_rejected_after_ban` strong-scenario test (mission
`marketplace-e2e-strong-scenarios` v0.3) currently models the
withdrawal attempt as a `register()` call with a larger amount, which
exercises the ban-stability invariant but does not model the
production withdrawal code path.

## Dependencies

- Mission `marketplace-slashing-persistence` v0.2 (SlashStore trait in
  place — `withdraw_stake` should persist through the store just like
  `register` / `slash`)
- Mission `marketplace-e2e-strong-scenarios` v0.3 (current
  `stake_withdrawal_rejected_after_ban` test wired to re-register
  proxy; real `withdraw` path replaces the proxy)

## Acceptance Criteria

- [x] Add `pub fn withdraw_stake(&mut self, provider_id: &str, amount: u128) -> Result<u128, SlashError>` to `SlashingLedger`
- [x] Reject if `amount == 0` (`SlashError::InvalidAmount`)
- [x] Reject if `amount > stake_micro_octo_w` (`SlashError::InsufficientStake { available, requested }`)
- [x] Reject if provider is unknown (`SlashError::UnknownProvider`)
- [x] Reject if provider is banned (`SlashError::BannedProvider { ... }`) — withdraw is gated by the ban check
- [x] Decrement `stake_micro_octo_w`; do NOT touch `offense_count` or `cumulative_loss_pct` (withdraw preserves the ban-stability invariant)
- [x] Persist through `SlashStore` (write-through if `store: Some(...)`)
- [x] Add `pub fn can_withdraw(&self, provider_id: &str, amount: u128) -> Result<(), SlashError>` (non-mutating query used by tests + RPC surface)
- [x] Add `InvalidAmount` + `InsufficientStake` variants to `SlashError`
- [x] Round-trip test (register, partial withdraw, full withdraw, banned-reject, zero-reject, over-balance-reject, unknown-reject)
- [x] Replace `stake_withdrawal_rejected_after_ban` proxy with `stake_withdrawal_full_amount_after_ban_rejected` (using new API)
- [x] Add `stake_withdrawal_partial_preserves_ledger_state` (offense_count + ban-status unchanged after partial withdraw)
- [x] Clippy passes with zero warnings
- [x] All existing tests pass + 4 new tests

### DEFERRED to follow-on mission

- [ ] Add `min_stake_micro_octo_w` to `SlashingRules` + `withdraw_stake` rejection that pushes the post-withdraw stake below the floor

## Claimant

(@claude)

## Pull Request

(in progress)

## Notes

**Semantics:** `withdraw_stake` is a SYNC mutation (matches the
existing `register` / `slash` sync surface — the slashing ledger is
in-memory + persist-through-store; making it async would cascade
the same migration the reputation registry just went through in
mission `marketplace-facade-reputation-async-migration`).

**Why this matters:** the existing test models "withdrawal attempt"
as a re-register with a larger amount. This is a stress-test of the
ban-stability invariant but does NOT cover the production withdraw
code path. A real `withdraw_stake` API:

- Models operator-driven stake withdrawal (goodbye-and-good-riddance
  for a banned provider after the ban period lapses)
- Closes the Strong-Scenario gap for the stake-withdrawal-race
  condition
- Adds a clean "exit liquidity" primitive that downstream
  settlement / slashing subsystems can hook into

**Files touched:**

- `crates/quota-router-core/src/marketplace/slashing.rs` — add
  `withdraw_stake` + `can_withdraw` + `InvalidAmount` +
  `InsufficientStake` variants
- `crates/quota-router-core/tests/marketplace_e2e.rs` — replace
  `stake_withdrawal_rejected_after_ban` with
  `stake_withdrawal_full_amount_after_ban_rejected`; add 2 new tests
- `crates/quota-router-core/tests/task_market.rs` — round-trip test
  for withdraw semantics

**Scope discipline:** migration of `SlashError::BannedProvider` is
NOT part of this mission — it already exists. The new
`InvalidAmount` + `InsufficientStake` variants follow the existing
naming convention.

## Cross-references

- Mission `marketplace-slashing-persistence` v0.2 (write-through store
  pattern)
- Mission `marketplace-e2e-strong-scenarios` v0.3 (test gap)
- RFC-0900 §Slashing Model (stake accounting)

## Version History

| Version | Date       | Status  | Change                                                                                                                                                                                   |
| ------- | ---------- | ------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | claimed | Mission filed from v0.3 deferred row of marketplace-e2e-strong-scenarios.                                                                                                                |
| v0.2    | 2026-08-13 | closed  | `withdraw_stake` + `can_withdraw` methods added; 2 SlashError variants; 2 new e2e tests; 1 proxy test replaced. 24/24 marketplace_e2e tests pass. min-stake-floor DEFERRED to follow-on. |
