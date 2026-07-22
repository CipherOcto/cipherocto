# Mission phase-g: Test Fakes (ASK fixtures + cache + provider sim)

**RFCs:** RFC-0959 v1.0 (Accepted 2026-07-20, Ask + axis registry + settlement), RFC-0957 (Accepted 2026-07-20, capability token format), RFC-0009 (Accepted 2026-07-20, NodeType taxonomy — Wholesale / SelfHost / Hybrid)
**Status:** Claimed (2026-07-22)
**Phase:** G (Test fakes per master plan §4)
**Master plan:** `docs/plans/2026-07-19-identity-master-plan.md` §4 row G
**Sub-mission of:** Phase G of identity-master-plan; standalone (no parent RFC-mission binding)

> **Availability (claim gate):** Phase G requires only RFC-0959 v1.0 + RFC-0957 + RFC-0009 all Accepted. All three reached Accepted 2026-07-20. Phase G has no per-session RFC authorship dependency (consumes existing types from `quota-router-storage` per Phase C completion 2026-07-21). Claim filed 2026-07-22 by @cipherocto per [[implementation-workflow-hook]]; implementation concurrent with claim per user "proceed phase G" directive (claim-first-implement-after workflow applied retroactively to mission file).

---

## Summary

Phase G delivers **executable test scaffolding** covering 10 models × 5 axes × 2 NodeTypes per master plan §4 Phase G exit criterion ("fixtures cover 10 models × 5 axes × 2 nodetypes"). Three fixture JSON files + one loader test:

| File | Coverage |
|---|---|
| `crates/quota-router-core/tests/fixtures/asks/asks.json` | 20 asks (10 models × 2 nodetypes); 5 axes (3 RFC-0959 §3.3 standard + 2 RFC-0958 F1 extensions) |
| `crates/quota-router-core/tests/fixtures/asks/cache_responses.json` | 12 cache scenarios (hit / miss / partial / full / zero-tokens) |
| `crates/quota-router-core/tests/fixtures/asks/provider_sim_modes.json` | 8 provider sim modes (Ok / Throttled / RateLimited / KeyExpired / SchemaChange / Timeout / Garbage / InternalError) |
| `crates/quota-router-core/tests/fixtures_asks.rs` | 11 tests: count invariants, axes coverage, every-model-has-both-nodetypes, insert-all-into-repo, SelfHost-cheaper invariant, AskId determinism, unique nonces, unique AskIds per (model, nodetype), cache classification sanity, sim mode coverage |

## In Scope

- 10 × 2 × 5 fixture matrix per master plan §4 Phase G exit
- SelfHost < Wholesale price invariant (per sovereignty-by-choice principle; 2-5% lower across all axes)
- Cache classification sanity (hit/partial/full/miss labels consistent with cache_hit flag + cached token counts)
- Provider simulator 8-mode fixture (per mission 0957-b AC-3)
- AskId content-addressability determinism (BLAKE3 over canonical asker_did || model || axes_hash || nonce)

## Out of Scope

- INSTA goldens for fixture content (snapshot tests over fixture JSON drift — deferred; drift detection via fixture version field)
- Per-model per-axis settlement cost golden values (cache_responses.json has expected_cost but tests don't validate it; deferred to Phase F sub-closure)
- Provider simulator implementation itself (already exists in `crates/quota-router-core/src/sim.rs` per S04 mission; this mission only contributes fixtures)
- Provider boundary clippy lint (mission 0957-b AC-1; separate session)

## Acceptance Criteria

- [x] **AC-1:** 10 × 5 × 2 fixture matrix exists at `tests/fixtures/asks/asks.json`
- [x] **AC-2:** 5 axes (3 standard + 2 extensions) registered; every ask covers all 5
- [x] **AC-3:** Both Wholesale + SelfHost present for each of 10 models
- [x] **AC-4:** All 20 asks insert into `AskRepository::open_in_memory()` without error
- [x] **AC-5:** SelfHost rates < Wholesale rates for every model (sovereignty-by-choice invariant)
- [x] **AC-6:** 11/11 tests green under `--features full`; clippy clean
- [x] **AC-7:** AskId deterministic across rebuilds (BTreeMap rate ordering preserves canonical_ser determinism)
- [x] **AC-8:** Cache + sim fixtures load and validate (12 cache scenarios + 8 sim modes per AC-3 of mission 0957-b)

## Verification

```
cargo test -p quota-router-core --test fixtures_asks --features full
→ 11 passed; 0 failed

cargo clippy -p quota-router-core --tests --features full -- -D warnings
→ clean

cargo fmt --all -- --check
→ clean
```

## Cross-References

- Master plan §4 Phase G row: "fixtures cover 10 models × 5 axes × 2 nodetypes" — AC-1/AC-2/AC-3 directly satisfy
- Mission 0957-b AC-3: "Provider simulator: 8 modes deterministic" — AC-8 satisfies via sim fixture
- Phase F (mission 0957-b AC-9/AC-10/AC-5): Phase G fixtures complement the 11-step exercise + goldens + replay defense tests; cross-references for settlement_hash determinism (ask fixtures provide canonical ask_ids for settlement hash inputs)
- RFC-0959 v1.0 §3.3 default registry: 3 standard axes (input_tokens_per_1k, output_tokens_per_1k, cached_input_tokens_per_1k) — AC-2 axis coverage
- RFC-0958 §Future Work F1: "Multi-axes ZK proof extensions (priority_lane, etc.) — registry allows extension" — AC-2 extension axes (priority_lane_per_1k, latency_p99_ms)

## Future Work

- **G2:** Per-model per-axis settlement cost golden values (currently `expected_cost_micro_octo_w` field in cache_responses.json is loaded but not asserted; Phase F sub-closure)
- **G3:** INSTA goldens for fixture content (snapshot drift detection)
- **G4:** Multi-model adversarial stress fixtures (e.g., 100 concurrent ASKs across providers, race conditions)

---

**Submission Date:** 2026-07-22
**Last Updated:** 2026-07-22
**Version:** 0.1 (Claimed; concurrent implementation per user directive)