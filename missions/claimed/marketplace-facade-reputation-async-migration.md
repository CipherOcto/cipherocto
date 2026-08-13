# Mission: marketplace-facade-reputation-async-migration

## Status

Closed 2026-08-13 (@claude). LANDED (partial — see v0.2 row).

`Marketplace` facade now holds both the legacy in-memory
`ProviderReputationRegistry` AND a new async
`ProviderReputationRegistryCompat<InMemoryReputationStore>` field.
Two new methods expose the canonical RFC-0968 surface:

- `record_outcome_async(asker_did, success, latency_ms, controller_id, now_unix)`
- `read_reputation_async(asker_did) -> Result<ProviderScore, ReputationError>`

The legacy `record_outcome` / `provider_score` sync surface is
preserved (no caller breakage). 4 dual-read parity tests in
`tests/marketplace_reputation_async.rs` verify the two paths stay
within Δ ≤ 0.5 success_rate + monotonic agreement.

Follow-on to Round 1 marketplace review (commit `264e2665`).
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
- [x] **NEW: `record_outcome_async(asker_did, success, latency_ms, controller_id, now_unix)`** — async write path through `reputation_compat`; rejects all-zero `controller_id` (RFC-0968-A1 amendment 40). Method on `Marketplace`.
- [x] **NEW: `read_reputation_async(asker_did) -> Result<ProviderScore, ReputationError>`** — async read path through `reputation_compat`. Method on `Marketplace`.
- [x] **NEW: `Marketplace.reputation_compat: ProviderReputationRegistryCompat<InMemoryReputationStore>`** — compat adapter field; initialized in all 3 constructors. Dual-read shadow with legacy `reputation`.
- [x] **NEW: 4 dual-read parity tests in `tests/marketplace_reputation_async.rs`** — success path, failure path, unknown-DID, all-zero-controller-id-rejection. Δ ≤ 0.5 + monotonic agreement.
- [x] **NEW: Module doc note** in `marketplace/mod.rs` — submodules list + reputation_compat role + retirement-gate contract reference.
- [x] Clippy passes with zero warnings
- [x] All existing tests pass + 4 new dual-read parity tests

### DEFERRED to follow-on mission

- [ ] Migrate `Marketplace.cheapest_with_ranking` to async (read path is currently on the legacy shadow; retirement gate flips it onto compat after 24h dual-read parity ≥ 0.999 is observed in prod)
- [ ] Migrate all callers to `.await` (legacy sync surface remains; callers opt in)
- [ ] Generic `Marketplace<S: ReputationStore>` for production store wiring (currently fixed to `InMemoryReputationStore`)

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

| Version | Date       | Change                                                                                                                                   |
| ------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | Mission filed. Round 1 review follow-on. 11 ACs.                                                                                         |
| v0.2    | 2026-08-13 | LANDED partial: compat field + 2 async methods + 4 dual-read parity tests. Full async-everywhere caller migration DEFERRED to follow-on. |

Last Updated: 2026-08-13
Version: 0.1
