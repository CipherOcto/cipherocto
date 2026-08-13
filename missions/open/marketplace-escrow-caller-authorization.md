# Mission: marketplace-escrow-caller-authorization

## Status

Open. Follow-on to Round 1 marketplace review (commit `264e2665`). The
Escrow state machine now drops `Clone` to prevent double-settle; this
mission hardens it against the *other* side of the same threat —
unauthorized caller advancing state transitions.

## RFC

RFC-0900 (Economics): Marketplace §Escrow Flow

## Dependencies

- Round 1 marketplace review (commit `264e2665`) — substrate stable

## Acceptance Criteria

- [ ] Add `Party` enum: `Buyer(String)`, `Seller(String)`, `Arbitrator(String)`
- [ ] `Escrow::lock(party)` — caller must equal `buyer` (Pending → Locked)
- [ ] `Escrow::settle(party)` — caller must equal `seller` (Locked → Settled)
- [ ] `Escrow::dispute(party)` — caller must equal `buyer` (Locked → Disputed)
- [ ] `Escrow::resolve_valid(party)` — caller must equal `Arbitrator` (Disputed → Slashed)
- [ ] `Escrow::resolve_invalid(party)` — caller must equal `Arbitrator` (Disputed → Settled)
- [ ] New error variant `EscrowError::UnauthorizedCaller { required: Party, actual: String }`
- [ ] Remove old zero-arg versions (`lock()`, `settle()`, etc.) — break API to enforce
- [ ] `TaskEscrow` mirror methods with same authorization
- [ ] Add 6+ tests: each transition with correct caller succeeds; each transition with wrong caller returns `UnauthorizedCaller`
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass + new auth tests (≥6)

## Claimant

(unclaimed)

## Pull Request

#

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

Last Updated: 2026-08-13
Version: 0.1