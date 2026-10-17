# Mission: marketplace-repo-trait-decouple

## Status

Closed. LANDED 2026-08-13.

## RFC

RFC-0900 (Economics): Marketplace

## Dependencies

- Round 1 marketplace review (commit `264e2665`) — substrate stable

## Acceptance Criteria

- [x] `pub trait AskRead: Send + Sync` in `quota-router-storage`: `get`, `cheapest(model, now_unix, axes)`, `list_by_asker`, `list_all_active_asks`
- [x] `pub trait AskWrite: Send + Sync`: `put(ask)`, `delete(ask_id)`
- [x] Combined: `pub trait AskRepository: AskRead + AskWrite` + `pub trait CombinedAskRepository: AskRead + AskWrite` (named supertrait so `(dyn ... + Send + Sync)` is well-formed)
- [x] `pub struct StoolapAskRepository { db: stoolap::Database }` impls both `AskRead` + `AskWrite` (delegating to inherent methods)
- [x] `Marketplace.repo: Arc<DynAskRepository>` — `DynAskRepository = dyn CombinedAskRepository + Send + Sync`
- [x] `Marketplace::open_in_memory()` / `open_path()` / `from_repo()` wrap `StoolapAskRepository` in `Arc`
- [x] No direct field access on concrete struct from marketplace code
- [x] 3 mock tests: `cheapest_via_in_memory_mock`, `list_by_asker_via_in_memory_mock`, `get_on_missing_id_via_in_memory_mock` using `InMemoryMockRepo` (Mutex<Vec<Ask>>-backed)
- [x] Module doc updated to document the trait boundary
- [x] Clippy passes with zero warnings (per-crate)
- [x] All existing tests pass — 15 ask_repo (12 existing + 3 mock) + 92 marketplace + 15 e2e

## Claimant

mmacedoeu (2026-08-13)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-storage/src/ask_repo.rs` — split trait + struct (StoolapAskRepository) + trait impls + 3 mock tests
- `crates/quota-router-storage/src/lib.rs` — re-export AskRead/AskWrite/AskRepository/CombinedAskRepository/StoolapAskRepository/DynAskRepository
- `crates/quota-router-core/src/marketplace/mod.rs` — `repo: Arc<DynAskRepository>` + 3 constructors wire Arc
- `crates/quota-router-core/tests/marketplace_e2e.rs` — `setup_match` now uses `Escrow::with_arbitrator(id, buyer, seller, "arb-1", amount)` so dispute/resolve calls with `Party::Arbitrator("arb-1")` pass the auth check (follow-on from `marketplace-escrow-caller-authorization`).
- `crates/quota-router-core/tests/fixtures_asks.rs` — renamed `AskRepository::` to `StoolapAskRepository::` in 2 test sites

Design notes:
- `CombinedAskRepository` named supertrait is the Rust 2024 workaround for `dyn TraitA + TraitB + Send + Sync` syntax; non-auto traits aren't allowed in `dyn` sums without parens, and a named supertrait sidesteps the syntax while preserving Interface Segregation.
- `pub trait AskRepository: AskRead + AskWrite` retained as documentation handle (blanket impl makes every concrete impl an `AskRepository`); `CombinedAskRepository` is the actually-dispatched surface.
- Trait object dispatch (one indirect call per operation) is the cost; cheap method bodies make this negligible on the hot path. Mock tests prove the boundary.

## Version History

| Version | Date       | Change                                                                                                |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | Mission filed. Round 1 review follow-on. 12 ACs. |
| v0.2    | 2026-08-13 | Mission CLOSED. AskRead/AskWrite/CombinedAskRepository + StoolapAskRepository + 3 mock tests land. `Marketplace.repo: Arc<DynAskRepository>`. setup_match updated for escrow-caller-auth follow-on. |

Last Updated: 2026-08-13
Version: 0.2
