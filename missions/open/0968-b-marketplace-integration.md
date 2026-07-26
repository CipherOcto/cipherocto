# Mission: 0968-b — Marketplace Read-Side Integration

## Status

Open (2026-07-26) — RFC-0968-A1.19 carries this work out of `missions/claimed/0968-reputation-persistence.md`. Required carrier of the marketplace read-side cutover and the `0-100` Reputation Score presentation layer.

## RFC

- RFC-0968: Reputation Registry (with in-place amendment RFC-0968-A1, 2026-07-26)
- RFC-0900: AI Quota Marketplace Protocol
- RFC-0918: Inference Task Market
- RFC-0927: RouterConfig Extension (per-layer alpha overrides)

## Summary

Replace the legacy `quota-router-core::marketplace::ProviderReputationRegistry::success_rate: f64` (and the pre-RFC-0968 `f64` EWMA in the same module) with the persisted RFC-0968 read-side path. Mission `0968-b` owns:

1. The **compatibility adapter** (`ProviderReputationRegistryCompat`, RFC-0968-A1.18) that reads from the new `ReputationStore` and implements the legacy public API for the dual-read window.
2. The **`0-100` Reputation Score presentation layer** (RFC-0968-A1 §22, amendment 23): `round(((score_ewma + 1.0) × 50.0).clamp(0.0, 100.0))` derived at read time, documented in `docs/00-meta/GLOSSARY.md` and `docs/01-foundation/whitepaper/v0.1-draft.md` §6 (GRS/RRS).
3. The **`election_priority` adapter** in RFC-0968 §10 (amendment 20): replaces the legacy `stake / (1 + count)` formula with a deterministic Dfp-derived priority anchored in RFC-0968's persisted aggregate.
4. The **CLI surface** `quota-router reputation show --did <did>` (amendment 24): retire the legacy `quota-router reputation show` / `provider --name` / `seller --wallet` / `leaderboard` / `multiplier` CLI surface from `missions/archived/superseded/reputation-system.md`.
5. The **dual-read retirement gate**: retire the legacy stores once 24-hour dual-read parity ≥ 0.999 holds across all `(did, kind, layer)` triples with `total ≥ 100`.

## Acceptance Criteria

### Phase A: Compatibility adapter (RFC-0968-A1 amendment 18 / C-P5)

- [ ] `crates/quota-router-core/src/marketplace/scoring.rs` introduces `ProviderReputationRegistryCompat` that reads from `ReputationStore` via `read_aggregate(did, SignalKind::Outcome, ReputationLayer::Market)` (and equivalent `Latency` reads).
- [ ] Legacy `ProviderReputationRegistry` public API is preserved via the compat adapter; existing callers (`quota-router-core::marketplace::Marketplace::cheapest_with_ranking`, etc.) are routed through it.
- [ ] The compat adapter returns the same public API surface (methods, signatures, behaviour) as the legacy implementation, documented in a `[DEPRECATED: replaced by 0968-b Phase C]` doc-comment per deprecated method.
- [ ] Legacy constructor `ProviderReputationRegistry::new()` continues to work for tests that exercise the dual-read path.

### Phase B: Election priority adapter (RFC-0968 §10 / amendment 20)

- [ ] `ProviderReputationRegistryCompat::election_priority_v2(candidate_did, stake, layer, now_unix) -> Result<Option<u128>, ReputationError>` is the canonical priority adapter.
- [ ] Tests:
  - score_ewma = NaN → `ScoreEncodingInvalid`.
  - score_ewma = ±Inf → `ScoreEncodingInvalid`.
  - score_ewma = Dfp::ZERO at samples=0 → returns 0 priority (eligible but deprioritized).
  - stake = u64::MAX, score_ewma ∈ [0, 1] → result fits in u128 (verified by `MAX_PRIORITY_VALUE = u128::MAX / 1_000_000`).
  - candidate excluded (suspended / revoked) → returns `None`.
  - two replicas running the same inputs → byte-identical result (RFC-0968-A1 `Result<…, _>` determinism).

### Phase C: 0-100 presentation layer (RFC-0968-A1 §22, amendment 23)

- [ ] `pub fn reputation_score_0_100(score_ewma: Dfp) -> u8` derived at read time as `round(((score_ewma + 1.0) × 50.0).clamp(0.0, 100.0))`.
- [ ] The presentation value NEVER feeds protocol calculations (routing priority, election deprioritization, severity suspension, election_priority adapter). Unit test: monotonic, bounded, bit-of-fuzz over [-1, 1] × [0, 100] = exactly 101 unique values.
- [ ] `docs/00-meta/GLOSSARY.md:163` updated: "Reputation Score (0-100) is a presentation-layer derivation, computed as `round(((score_ewma + 1.0) × 50.0).clamp(0.0, 100.0))` per RFC-0968 §10".
- [ ] `docs/01-foundation/whitepaper/v0.1-draft.md:500-514` updated: GRS / RRS presentation derives from RFC-0968 `score_ewma`, not a stored integer.
- [ ] `docs/01-foundation/whitepaper/v1.0-whitepaper.md:1239,3929,7442,8132-8135` updated to reconcile 0-100 + 0.5x-2.0x multiplier ranges.

### Phase D: Dual-read cutover (RFC-0968-A1 amendment 18)

- [ ] `reputation_parity_match_count` and `reputation_parity_total_count` Prometheus metrics exported.
- [ ] Compute `parity_score = match / total` only when `total >= 100` triples observed in window (per `rfc0968-reputation-persistence.md` Phase 2.5 acceptance).
- [ ] **Dual-read retirement gate:** legacy stores (`SlashReputationStore`, `DcRootedSlashReputationStore`, `ProviderReputationRegistry`) can be retired ONLY when `parity_score >= 0.999` for 24h sustained with `total >= 100`.
- [ ] Retirement PR deletes the legacy in-memory stores and removes `crates/octo-network/src/{mon,dc}/reputation.rs` files; their replacement is the compat adapter in `quota-router-core`.

### Phase E: CLI surface (RFC-0968-A1 amendment 24)

- [ ] `quota-router reputation show --did <did>` (canonical DID) replaces the legacy `quota-router reputation show` (Address), `provider --name openai`, `seller --wallet 0x...`, `leaderboard`, `multiplier`.
- [ ] Output displays: `did:octo:b<52>` + `score_ewma` (Dfp) + `0-100` presentation score + `samples` + `last_signal_at_unix`.
- [ ] Backwards-compat CLI subcommands emit a deprecation warning + refuse under flag `--strict-deprecation`.

### Phase F: Cross-mission dependencies

- [ ] Dependencies on missions 0855p-b (federation) and 0855p-c (DC reputation) become soft dependencies for Phase E only (the marketplace can ship without gossip but the unified registry is the canonical read side).
- [ ] Mission `missions/open/0855p-b-cross-mission-reputation.md` and `missions/open/0855p-c-reputation.md` receive cross-references to mission 0968-b via their acceptance criteria.

### Phase G: Readiness gate

- [ ] `cargo test -p quota-router-core --features marketplace --lib` passes.
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` clean.
- [ ] Integration test: legacy `ProviderReputationRegistry` and compat adapter return byte-identical `(success_rate, count, threshold, min_reputation)` outputs over a 1000-event fixed sequence.
- [ ] Integration test: election priority ordering is identical for honest + slash-farmed candidate sets.

## Dependencies

**Hard:**
- Mission 0968 (claimed) must reach Phase 1 (storage layer live).
- RFC-0968-A1 amendments 18, 19, 20, 23, 24 are folded into the mission 0968 acceptance criteria.

**Soft:**
- Mission 0855p-b (Cross-mission coordinator reputation) — federation optional for marketplace read-side but canonical.
- Mission 0855p-c (DomainCoordinator reputation) — DC reputation optional for marketplace.
- Mission 0968a (on-chain anchoring) — anchored scores not required for marketplace display.

## Complexity

Medium. Two adapter files + presentation helper + CLI surface + dual-read retirement tests. ~400-600 LOC + tests.

## Claimant

(unassigned)

## Pull Request

(none)

## Location

- New: `crates/quota-router-core/src/marketplace/scoring_compat.rs`
- Modified: `crates/quota-router-core/src/marketplace/scoring.rs` (route through compat adapter)
- Modified: `crates/quota-router-cli/src/commands.rs` (replace reputation subcommands)
- Modified: `crates/quota-router-core/src/marketplace/Marketplace::cheapest_with_ranking` (use `election_priority_v2`)
- Modified: `docs/00-meta/GLOSSARY.md`
- Modified: `docs/01-foundation/whitepaper/v0.1-draft.md`, `docs/01-foundation/whitepaper/v1.0-whitepaper.md`
- New (follow-on): `docs/07-developers/reputation-marketplace-bridge.md`

## Notes

### Why a separate mission?

`missions/claimed/0968-reputation-persistence.md` Phase 1-5 contains the **storage** layer. This mission owns the **read side**: marketplace routing, listing display, election deprioritization, CLI. The split is intentional — write-side correctness (Phase 1-2 shadow write) and read-side correctness (Phase C presentation + Phase B election adapter) require different review panels and operate on different layers of the dependency stack.

### Why retire legacy stores after dual-read parity?

The legacy stores do not retain raw event history (per Session 2 C-P5 / Session 3 I-X5). They use String-keyed maps, integer counts, and `f64` computations. Direct equivalence against a persisted Dfp EWMA is structurally impossible: a 24-hour dual-read parity window of ≥ 0.999 across all `(did, kind, layer)` triples with `total ≥ 100` is the strongest evidence available that the in-memory behaviour and the persisted behaviour are functionally indistinguishable to consumers.

### Why not a sub-mission of 0968?

Mission 0968's Phase 2 (Shadow-Write) is currently blocked on `crates/oct-reputation/` (does not exist). Mission 0968-b can ship independently against a stable in-memory `ProviderReputationRegistry` plus a stubbed `ReputationStore` interface, while the implementation lands.
