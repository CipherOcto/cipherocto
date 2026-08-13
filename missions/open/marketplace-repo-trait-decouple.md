# Mission: marketplace-repo-trait-decouple

## Status

Open. Follow-on to Round 1 marketplace review (commit `264e2665`).
`Marketplace.repo` couples to a concrete `AskRepository` struct in
`quota-router-storage`; no trait boundary blocks test isolation or
parallel backends.

## RFC

RFC-0900 (Economics): Marketplace

## Dependencies

- Round 1 marketplace review (commit `264e2665`) — substrate stable

## Acceptance Criteria

- [ ] Define `trait AskRead + Send + Sync` in `quota-router-storage`: `cheapest(model)`, `list_by_asker(did)`, `get(ask_id)`
- [ ] Define `trait AskWrite + Send + Sync`: `put(ask)`, `delete(ask_id)`
- [ ] Combine: `trait AskRepository: AskRead + AskWrite`
- [ ] `pub struct AskRepository { db: stoolap::Database }` impls all three
- [ ] `Marketplace.repo: Arc<dyn AskRepository + Send + Sync>` — replaces concrete struct field
- [ ] `Marketplace::open_in_memory(repo: Arc<dyn ...>)` and `Marketplace::open_path(path: &Path)` constructors wire different impls
- [ ] Remove direct field access on concrete struct from marketplace code
- [ ] Add ≥3 tests: cheapest via in-memory mock, list_by_asker via mock, get on missing id
- [ ] Document the boundary in `crates/quota-router-storage/src/ask_repo.rs` module doc
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-storage/src/ask_repo.rs` — split trait + impl
- `crates/quota-router-core/src/marketplace/mod.rs:120, 144, 156` — field type change + constructor update
- Consumers: `quota-router-cli`, `octo-wallet` — no API surface change (they use the trait methods)

Round 1 review context (Pass 2 HIGH #H1): `Marketplace.repo: AskRepository`
is a concrete struct, not a trait. `quota-router-cli` and `octo-wallet`
cannot mock for testing without stoolap. Stable Abstractions Principle
(CLAUDE.md §Architectural Principles) violated.

Pair with `marketplace-facade-reputation-async-migration` (H2) so the
facade's API churn is one event.

## Version History

| Version | Date       | Change                                                                                                |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | Mission filed. Round 1 review follow-on. 12 ACs. |

Last Updated: 2026-08-13
Version: 0.1