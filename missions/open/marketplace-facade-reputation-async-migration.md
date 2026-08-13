# Mission: marketplace-facade-reputation-async-migration

## Status

Open. Follow-on to Round 1 marketplace review (commit `264e2665`).
`Marketplace` facade still delegates to deprecated
`ProviderReputationRegistry` (sync); migration to async
`ProviderReputationRegistryCompat` blocked by sync/async mismatch.

## RFC

RFC-0968 (Economics): Reputation Registry — retirement gate

## Dependencies

- Mission `0968-b-marketplace-integration` (parent migration plan)
- Mission `marketplace-repo-trait-decouple` (trait split lands first)

## Acceptance Criteria

- [ ] Replace `Marketplace.reputation: ProviderReputationRegistry` with `Arc<ProviderReputationRegistryCompat<S>>` for generic `S: ReputationStore + Send + Sync + 'static`
- [ ] Replace `parking_lot::Mutex` locks on `reputation`/`book` with `tokio::sync::Mutex` (or expose async API surface)
- [ ] Update `Marketplace::record_outcome` to async signature with `controller_id` parameter
- [ ] Update `Marketplace::cheapest_with_ranking` to async + caller-supplied controller_id
- [ ] Update all callers (`tests/eleven_step.rs`, `tests/marketplace_e2e.rs`, `tests/task_market.rs`, `quota-router-cli`, `octo-wallet`) to thread identity and `.await`
- [ ] Drop `#[allow(deprecated)]` from `Marketplace` surface
- [ ] Add ≥3 migration tests: dual-read parity ≥ 0.999 across a 24h synthetic fixture (per RFC-0968 retirement gate)
- [ ] Document retirement gate trigger + deprecation removal in module docs
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass + new dual-read parity tests (≥3)

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/src/marketplace/mod.rs` — facade migration
- `crates/quota-router-core/src/marketplace/scoring.rs` — drop legacy `#[deprecated]` methods once migration lands
- `crates/quota-router-core/src/marketplace/reputation_compat.rs` — already async-first
- `crates/quota-router-core/tests/eleven_step.rs` — caller update
- `crates/quota-router-cli/` + `octo-wallet/` — caller update

Round 1 review context (Pass 2 HIGH #H2): three legacy methods are
`#[deprecated]`; `Marketplace` still delegates to them. Retirement
gate (24h dual-read parity ≥ 0.999) cannot fire until facade migrates.

Pair with `marketplace-repo-trait-decouple` so all consumer-facing
API changes land in one coordinated PR.

## Version History

| Version | Date       | Change                                                                                                |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | Mission filed. Round 1 review follow-on. 11 ACs. |

Last Updated: 2026-08-13
Version: 0.1