# Mission: marketplace-escrow-caller-authorization

## Status

Closed 2026-08-13 (@claude). LANDED.

**H1 from Round 1 marketplace review closed.** `Escrow` state-machine
methods now take a `&Party` and verify caller identity against the
escrow's stored counterparty before advancing state. The state
machine is fail-closed: non-buyer holding `&mut Escrow` cannot drive
`Locked → Disputed → Slashed` without authority.

## RFC

RFC-0900 (Economics): Marketplace §Escrow Flow

## Dependencies

- Round 1 marketplace review (commit `264e2665`) — substrate stable

## Acceptance Criteria

- [x] `Party` enum added: `Buyer(String)`, `Seller(String)`,
      `Arbitrator(String)` — `crates/quota-router-core/src/marketplace/escrow.rs`
- [x] `Escrow::lock(&Party)` — caller must equal `buyer` (Pending → Locked)
- [x] `Escrow::settle(&Party)` — caller must equal `seller` (Locked → Settled)
- [x] `Escrow::dispute(&Party)` — caller must equal `buyer` (Locked → Disputed)
- [x] `Escrow::resolve_valid(&Party)` — caller must equal `arbitrator` (Disputed → Slashed)
- [x] `Escrow::resolve_invalid(&Party)` — caller must equal `arbitrator` (Disputed → Settled)
- [x] New error variant `EscrowError::UnauthorizedCaller { required, caller_role, caller_identity }`
- [x] Old zero-arg versions removed — API breaks to enforce auth
- [x] `TaskEscrow` mirror methods with same authorization
      (`TaskEscrowError::Escrow(UnauthorizedCaller)`)
- [x] `Escrow::with_arbitrator()` constructor for arbiter-wired escrows
- [x] `arbitrator` field on `Escrow` + `EscrowSnapshot`
- [x] 11 new auth tests in `escrow.rs::tests`:
      `lock_rejects_seller_caller`, `lock_rejects_arbitrator_caller`,
      `lock_rejects_wrong_buyer_identity`, `settle_rejects_buyer_caller`,
      `settle_rejects_wrong_seller_identity`, `dispute_rejects_seller_caller`,
      `resolve_valid_rejects_buyer_caller`,
      `resolve_invalid_rejects_seller_caller`,
      `resolve_rejects_when_no_arbitrator_set`,
      `unauthorized_wins_over_state_invalid`,
      `unauthorized_caller_error_carries_required_role`
- [x] 3 new TaskEscrow auth tests in `task_market/escrow.rs::tests`:
      `task_escrow_full_happy_path`,
      `task_escrow_rejects_unauthorized_caller`,
      `task_escrow_snapshot_carries_arbitrator`
- [x] All 24 escrow lib tests pass; all 9 task_market escrow tests pass
- [x] All existing marketplace_e2e + task_market integration tests pass
      (after migrating call sites to thread Party through)
- [x] Clippy passes with zero warnings

## Claimant

(@claude)

## Pull Request

(in progress)

## Notes

Key files:
- `crates/quota-router-core/src/marketplace/escrow.rs` — add `Party` enum + new method signatures
- `crates/quota-router-core/src/task_market/escrow.rs` — mirror auth

Round 1 review context (Pass 1 HIGH #H1): the Escrow type carried
`buyer`/`seller` fields but did not enforce caller identity on
transitions. `dispute`/`resolve_*` are especially sensitive — a
non-buyer holding `&mut Escrow` could drive `Locked → Disputed → Slashed`
without authority. Adding `Party` parameter makes the primitive
fail-closed; existing callers must thread identity through.

Coupling: callers (proxy.rs settle orchestrator, marketplace::put
post-match flow) need to be updated to thread identity. Pair this
work with `marketplace-facade-reputation-async-migration` so callers
get refactored once.

## Version History

| Version | Date       | Change                                                                                                |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | Mission filed. Round 1 review follow-on. 12 ACs. |
| v0.2    | 2026-08-13 | Closed. H1 fixed: Party enum + Escrow/TaskEscrow auth + 14 new tests. 24/24 escrow + 9/9 task_market escrow tests pass. |