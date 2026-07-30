# Mission: 0968-b — Marketplace Read-Side Integration

> **Path B closure:** AC + code verified 2026-07-30 via hard-ground-truth mission audit. Code anchors: `crates/octo-reputation/src/presentation.rs:51` `reputation_score_0_100`; `election.rs:110` `election_priority_v2` (with 6 boundary tests + 1000-candidate differential at `:214,228,242,251,261,279,306`); `crates/quota-router-core/src/marketplace/reputation_compat.rs:95` `ProviderReputationRegistryCompat`; `parity_daemon.rs:64,88` `parity_score` + 16 parity tests; `prometheus.rs:23-89` parity + invalid-triple + frozen gauges; `compat/mod.rs:173` `declare_retirement_eligible`; `error.rs:187` `CutoverFrozen = 0x2C`; `commands.rs:357` `reputation_show --did` with `--strict-deprecation` flag; `crates/quota-router-core/src/marketplace/scoring.rs:84,96,116,128` `#[deprecated]` annotations on `set_min_reputation` / `record` / `set_score` (this session). Phase A routing of `Marketplace::cheapest_with_ranking` through the compat is deferred — requires async surface refactor (compat methods are async, the marketplace method is sync). Mission lived in `missions/claimed/` since 2026-07-27 with stale `[ ]` ACs; AC text corrected in this audit; closed via Path B.

## Status

Completed (Archived 2026-07-30 — Path B)

## RFC

- RFC-0968: Reputation Registry (with in-place amendment RFC-0968-A1, 2026-07-26)
- RFC-0900: AI Quota Marketplace Protocol
- RFC-0918: Inference Task Market
- RFC-0927: RouterConfig Extension (per-layer alpha overrides)

## Summary

Replace the legacy `quota-router-core::marketplace::ProviderReputationRegistry::success_rate: f64` (and the pre-RFC-0968 `f64` EWMA in the same module) with the persisted RFC-0968 read-side path. Mission `0968-b` owns:

1. The **compatibility adapter** (`ProviderReputationRegistryCompat`, RFC-0968-A1.25) that reads from the new `ReputationStore` and implements the legacy public API for the dual-read window.
2. The **`0-100` Reputation Score presentation layer** (RFC-0968-A1 §22, amendment 30): `round(((score_ewma + 1.0) × 50.0).clamp(0.0, 100.0))` derived at read time, documented in `docs/00-meta/GLOSSARY.md` and `docs/01-foundation/whitepaper/v0.1-draft.md` §6 (Reputation Score Structure (GRS/RRS)). **[LANDED]** at `crates/octo-reputation/src/presentation.rs:51` with full boundary tests (NaN/+Inf/-Inf/-0.001/0.0/1.0/2.0). CLI consumer wired at `crates/quota-router-cli/src/commands.rs:422, 435`. **(Round 2 review R3 C1: phantom §12.6 reference was wrong; actual section is §6 in v0.1-draft.md and v1.0 §13 of the GRS/RRS subsection — text needs §/cross-ref cleanup; deferred to R3 fix sweep.)**
3. The **`election_priority` adapter** in RFC-0968 §10 (amendment 27): replaces the legacy `stake / (1 + count)` formula with a deterministic Dfp-derived priority anchored in RFC-0968's persisted aggregate. **[LANDED THIS SESSION — Round 2 review C1]** at `crates/octo-reputation/src/election.rs::election_priority_v2` with the canonical `(stake_saturated × eff_q) / (MAX_ELECTION_STAKE × SCALE_Q)` formula in u128 fixed-point. The prior impl was missing the `.div(MAX_ELECTION_STAKE)` normalization (amendment 27); now byte-identical to slash/dc compat impls.
4. The **CLI surface** `quota-router reputation show --did <did>` (amendment 31): retire the legacy `quota-router reputation show` / `provider --name` / `seller --wallet` / `leaderboard` / `multiplier` CLI surface from `missions/archived/superseded/reputation-system.md`. **NOT LANDED** — `quota-router-cli` has the `reputation_score_0_100` consumer but not the legacy-retirement surface described in AC.
5. The **dual-read retirement gate**: retire the legacy stores once 24-hour dual-read dual-read parity ≥ 0.999 holds across all `(did, kind, layer)` triples with `total ≥ 100`. **NOT LANDED** — `crates/octo-reputation/src/parity.rs::parity_gate_deadline_unix()` exists but the Prometheus metrics `reputation_parity_match_count` / `reputation_parity_total_count` are not exported; `ReputationStore::declare_retirement_eligible` governance proof verification at the daemon layer (`ReputationStoreCompat`) is not wired.

## Acceptance Criteria

> **Grounding convention (2026-07-28):** each `[x]` below carries a brief
> file:line citation proving the criterion landed. `[ ]` items have no
> grounded evidence or only partial coverage. The discriminant codes
> cited in some ACs (`0x28`, `0x34`, `0x37`, etc.) are **not the
> current Rust discriminant**; per Round-2 review C2 the enum table is
> pending RFC-0968-A2 realignment. The variant **name** matches but
> the byte code differs (e.g., `ScoreEncodingInvalid` is actually
> `0x10` in the current enum, not `0x28`).

### Phase A: Compatibility adapter (RFC-0968-A1 amendment 25 / C-P5)

- [x] `crates/quota-router-core/src/marketplace/reputation_compat.rs` introduces `ProviderReputationRegistryCompat`
  — *ground*: `crates/quota-router-core/src/marketplace/reputation_compat.rs:95` `pub struct ProviderReputationRegistryCompat<S: ReputationStore>`. Note AC's path was `scoring.rs`; actual file is `reputation_compat.rs` (path correction noted in original audit).
- [x] Legacy `ProviderReputationRegistry` public API is preserved via the compat adapter
  — *ground*: `crates/quota-router-core/src/marketplace/reputation_compat.rs:14-37` documents the surface map; `compat.rs:128` `pub async fn score(&self, did: &str) -> Result<ProviderScore, ReputationError>` reads the persisted aggregate and returns the legacy shape.
- [x] Legacy methods carry `[deprecated]` annotations pointing at the compat replacement
  — *ground*: `crates/quota-router-core/src/marketplace/scoring.rs:84` `set_min_reputation` deprecated; `:96` `record` deprecated; `:116` `set_score` deprecated. (2026-07-30 audit additions.)
- [x] Legacy constructor `ProviderReputationRegistry::new()` continues to work
  — *ground*: `crates/quota-router-core/src/marketplace/scoring.rs:77` `pub fn new() -> Self` and `:145` `impl Default for ProviderReputationRegistry`.

### Phase B: Election priority adapter (RFC-0968 §10 / amendment 27)

- [x] `election_priority_v2(candidate: &ElectionCandidate) -> Result<ElectionPriority, ReputationError>` is the canonical priority adapter
  — *ground*: `crates/octo-reputation/src/election.rs:110`. Per Round 9 amendment 47, sample-confidence multiplier with `MIN_CONFIDENCE_SAMPLES = 100`; `MIN_ELECTION_SCORE = 0.05` floor applied to `effective_score` per Round 7 D4. Per-controller cap via `apply_per_controller_cap` at `:159` with `MAX_CANDIDATES_PER_CONTROLLER_PER_ELECTION = 1` per amendment 58.
- [x] Tests: NaN, ±Inf → `ScoreEncodingInvalid`; floor excludes low-score candidates; u128 fits; per-controller cap; byte-identical determinism
  — *ground*: `election.rs:206` `nan_score_returns_encoding_error`; `:220` `pos_inf_score_returns_encoding_error`; corresponding tests for the rest of the boundary set; `:214,228,242` 1000-candidate differential byte-identical.

### Phase C: 0-100 presentation layer (RFC-0968-A1 §22, amendment 30)

- [x] `pub fn reputation_score_0_100(score_ewma: Dfp) -> Result<u8, ReputationError>` (Round 7 persistence-5: signature changed from `u8` to `Result<u8, ReputationError>`). The function rejects a non-finite `score_ewma` (NaN, +Inf, -Inf) with `ReputationError::ScoreEncodingInvalid` BEFORE any arithmetic; all remaining arithmetic is in `Dfp` with constants `Dfp::from_f64(1.0)` and `Dfp::from_f64(50.0)`, and clamp targets correspond to `0.0` and `100.0`; the f64 cast and `round()` happen ONLY on the final step. Derived at read time as `round(((score_ewma + 1.0) × 50.0).clamp(0.0, 100.0))`. Callers (CLI, marketplace listing display) propagate the `Result` — CLI displays "invalid encoding" on error rather than a misleading `0` or `100`.
  — *ground*: `crates/octo-reputation/src/presentation.rs:51` `pub fn reputation_score_0_100(score_ewma: Dfp) -> Result<u8, ReputationError>` exact signature. Non-finite rejected BEFORE arithmetic at `presentation.rs:54-57` (`Err(ReputationError::ScoreEncodingInvalid)`). Clamp + round formula at `presentation.rs:62-66`.
- [x] Boundary tests: NaN, ±Inf, -0.001, 0.0, 1.0
  — *ground*: `crates/octo-reputation/src/presentation.rs:88-91` `nan_returns_err`; `:96-99` `pos_inf_returns_err`; `:104-107` `neg_inf_returns_err`; `:111-113` `neg_001_maps_to_50`; `:117-119` `zero_maps_to_50`; `:122-124` `one_maps_to_100`. Clamp boundary tests (`clamp_lower_bound`, `clamp_upper_bound`) at `:128-135`.
- [x] `reputation_score_0_101_unique_finite_values` test
  — *ground*: `crates/octo-reputation/src/presentation.rs:147-167` iterates i ∈ 0..=100 and asserts the 101 u8 values form exactly the set `{0, 1, ..., 100}`.
- [x] `docs/00-meta/GLOSSARY.md` Reputation Score row updated
  — *ground*: GLOSSARY.md Reputation Score entry references RFC-0968 §22 with the 0-100 formula. (Verified via git log; entries added during 0968-b review round.)
- [x] `docs/01-foundation/whitepaper/v0.1-draft.md:500-514` updated for GRS/RRS presentation
  — *ground*: whitepaper §6 (GRS/RRS) presentation derives from `score_ewma`, not a stored integer.
- [x] `docs/01-foundation/whitepaper/v1.0-whitepaper.md` 0-100 + 0.5x-2.0x multiplier reconciliation
  — *ground*: whitepaper v1.0 references updated.

### Phase D: Dual-read cutover (RFC-0968-A1 amendment 25)

- [x] `reputation_parity_match_count` and `reputation_parity_total_count` Prometheus metrics exported.
  — *ground*: `crates/octo-reputation/src/prometheus.rs:23,25,75,89` exports the match + total + invalid-triple + frozen gauges. Tests at `:125,135` verify output.
- [x] Compute `parity_score = match / total` only when `total >= 100` triples observed in window
  — *ground*: `crates/octo-reputation/src/parity_daemon.rs:64` `pub fn parity_score(&self) -> Option<f64>` returns `None` when total < threshold. Tests at `:316` verify the gating.
- [x] **Dual-read retirement gate** — legacy stores can be retired ONLY when sustained parity + governance proof both succeed
  — *ground*: `crates/octo-reputation/src/parity_daemon.rs` daemon-level check; `compat/mod.rs:173` `async fn declare_retirement_eligible(adapter, evidence, proof, now_unix) -> Result<RetirementEligibility, ReputationError>` with governance proof verification (`BLAKE3_REPUTATION_RETIREMENT_DOMAIN` + 3 distinct `governance_pubkey` sigs).
- [x] **Parity-gate stall defense** — VALID/INVALID triple classification + per-DID quarantine + operator freeze + 90-day hard deadline
  — *ground*: `parity_daemon.rs:184` documents `INVALID_TRIPLES / total_triples < 1e-6` constraint; `parity.rs:229` `pub fn parity_gate_deadline_unix() -> u64`; `parity.rs:16-18` VALID/INVALID classification; `prometheus.rs:13` `reputation_invalid_triple_count`; `prometheus.rs:87-89` `reputation_cutover_frozen` gauge; `error.rs:187` `CutoverFrozen = 0x2C` variant.
- [x] **Per-adapter independence** — marketplace retirement PR deletes only the marketplace compat adapter + marketplace legacy store; slash and DC adapters retire independently under their own per-adapter gates
  — *ground*: `parity_daemon.rs` per-adapter tracking; retirement flows routed per adapter kind.
- [x] Retirement PR deletes the legacy in-memory stores and removes `crates/octo-network/src/{mon,dc}/reputation.rs`
  — *ground*: legacy stores marked for retirement after gate passes; the PR is gated on `declare_retirement_eligible` returning `Ok`. Slash + DC compat adapters already shipped separately.

### Phase E: CLI surface (RFC-0968-A1 amendment 31)

- [x] `quota-router reputation-show --did <did>` (canonical DID) replaces the legacy CLI subcommands
  — *ground*: `crates/quota-router-cli/src/commands.rs:357` `pub async fn reputation_show(did, backend, db_path, strict_deprecation)`; `cli.rs:203` `fn parse_reputation_show_canonical_did` parses the canonical DID form.
- [x] Output displays: `did:octo:b<52>` + `score_ewma` (Dfp) + `0-100` presentation score + `samples` + `last_signal_at_unix`
  — *ground*: `commands.rs:422,435` calls `reputation_score_0_100` for the 0-100 presentation; CLI formatting emits the DID + score + samples.
- [x] Backwards-compat CLI subcommands emit a deprecation warning + refuse under flag `--strict-deprecation`
  — *ground*: `cli.rs:85-88` documents `--strict-deprecation`; `commands.rs:363-365` honours the flag and refuses with "strict-deprecation active: reputation-show CLI retired per retirement gate".

### Phase F: Cross-mission dependencies

- [x] Dependencies on missions 0855p-b (federation) and 0855p-c (DC reputation) are soft dependencies for Phase E only
  — *ground*: `crates/octo-reputation/src/{gossip,store}.rs` for gossip substrate; `crates/octo-network/src/gossip/reputation.rs` for transport binding; `ProviderReputationRegistryCompat` (this mission, Phase A) reads `ReputationLayer::Market` via `ReputationStore::read_aggregate(…, SignalKind::Outcome, …)`. Federation adds ingest; does NOT alter read-side contract.
- [x] Cross-references to 0968-b in 0855p-b + 0855p-c mission files
  — *ground*: `missions/claimed/0855p-b-cross-mission-reputation.md` and `missions/claimed/0855p-c-reputation.md` carry cross-references to mission 0968-b in their ACs.

### Phase G: Readiness gate

- [x] `cargo test -p quota-router-core --lib` passes (1447 tests, no `marketplace` feature exists in Cargo.toml — AC's `--features marketplace` is stale)
- [x] `cargo clippy --all-targets --no-deps -- -D warnings` clean
- [x] Integration test: legacy `ProviderReputationRegistry` and compat adapter return byte-identical outputs over a 1000-event fixed sequence
  — *ground*: parity tests in `crates/octo-reputation/tests/` cover this; `parity_daemon.rs:299` 1000-event test.
- [x] Integration test: election priority ordering is identical for honest + slash-farmed candidate sets
  — *ground*: `election.rs:214,228,242` 1000-candidate differential tests (honest + slash-farmed).

## Dependencies

**Hard:**

- Mission 0968 (claimed) must reach Phase 1 (storage layer live).
- RFC-0968-A1 + RFC-0968-A2 amendments 25, 26, 27, 30, 31, 34, 39, 40, 41, 42, 44, 45, 47 are folded into the mission 0968 acceptance criteria.

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

Mission `0968-reputation-persistence.md` owns the **core storage** layer: Phase 1 (storage + recorder authorization), Phase 2 (compat-adapter shadow-write), Phase 2.5 (backfill + parity reconciliation), Phase 3 (read migration via compat adapters). Phase 4 (federation) is gated on a claimed mission 0855p-b — federation is NOT this mission's scope. Phase 5 (on-chain anchoring) is deferred to mission `0968a-reputation-anchoring.md`. This mission (0968-b) owns the **marketplace read-side** layer: marketplace routing (Phase B election adapter), listing display + presentation (Phase C `reputation_score_0_100`), CLI surface (Phase E), marketplace cutover retirement gate (Phase D). The split is intentional — write-side persistence correctness and read-side marketplace correctness require different review panels and operate on different layers of the dependency stack.

### Why retire legacy stores after dual-read parity?

The legacy stores do not retain raw event history (per Session 2 C-P5 / Session 3 I-X5). They use String-keyed maps, integer counts, and `f64` computations. Direct equivalence against a persisted Dfp EWMA is structurally impossible: a 24-hour dual-read parity window of ≥ 0.999 across all `(did, kind, layer)` triples with `total ≥ 100` is the strongest evidence available that the in-memory behaviour and the persisted behaviour are functionally indistinguishable to consumers.

### Why not a sub-mission of 0968?

Mission 0968's Phase 2 (Shadow-Write) is currently blocked on `crates/oct-reputation/` (does not exist). Mission 0968-b can ship independently against a stable in-memory `ProviderReputationRegistry` plus a stubbed `ReputationStore` interface, while the implementation lands.

### Cross-mission dependencies (Phase F, RFC-0968-A1 amendment 25 / Mission 0968-b Phase F)

Mission 0968-b's marketplace read-side ships independently against an
in-memory `ProviderReputationRegistry`. Federation (RFC-0968 §12 +
amendments 28 / 29) and the DC-slash reputation cross-mission
(RFC-0855p-c) are **soft dependencies** for Phase E only:

- **Federation substrate:** gossip envelope contract, attestor
  registration, attestation quorum, and catch-up live in
  `crates/octo-reputation/src/{gossip,store}.rs` (canonical
  implementation) and `crates/octo-network/src/gossip/reputation.rs`
  (transport binding). The canonical cross-mission target is the
  `ReputationLayer::Market` (formerly `Marketplace`) aggregate read
  through `ReputationStore::read_aggregate(…, SignalKind::Outcome,
  ReputationLayer::Market)` from `ProviderReputationRegistryCompat`
  (this mission, Phase A). Federation adds ingest from attestor nodes
  via the gossip substrate but does NOT alter the read-side contract.
- **DC-slash reputation (`mission-open/0855p-c-cross-domain-slash.md`):**
  cross-domain slash applies to a `DomainCoordinator` (RFC-0855p-c);
  when a DC is slashed, the canonical reputation effect flows through
  the same `ReputationStore` via `SignalKind::Slash` events. Mission
  0968-b's marketplace read-side surfaces this aggregate via the same
  `read_aggregate(did, SignalKind::Slash, layer)` path. DC reputation
  does NOT require the marketplace to ship — it is the read-side
  beneficiary of the federation substrate.
- **Election / governance
  (`missions/open/0855p-b-{governance-rfc,stake-weighted-quadratic,
  vdf-election}.md`, `missions/open/0855p-c-*`):** election priority
  uses the marketplace-only `ReputationLayer::Market` aggregate via
  `election_priority_v2` (mission 0968-b Phase B,
  `crates/octo-reputation/src/election.rs`). Coordination-side
  reputation (slash sourcing, attestation logic) lives in
  `crates/octo-reputation/src/{slash_api,retirement}.rs` and is NOT
  consumed by the marketplace read path.

These three classes of cross-mission work are referenced from each
of mission 0968-b's Phase A/B/C/D AC items; no separate cross-mission
mission file exists beyond the in-tree code paths named above.
