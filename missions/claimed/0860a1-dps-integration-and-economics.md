# Mission: DPS Aggregation Backend Integration + Monthly Earnings Calculation (RFC-0860 §7+§8 follow-up)

## Status

Closed (Band A — 2026-08-07; audit-closure rolled up 2026-08-07). Implementation landed in 1 commit; 13/14 ACs GREEN (DPS aggregation backend wiring + monthly earnings split + 4 TVs + 7 cross-crate compat items). 1/14 AC deferred (`tests/fixtures/porelay/monthly_earnings_goldens.json` not shipped — TV2 happy-path pins values directly so fixture is optional for Band A scope). Deferral now explicit per [[deferred-vs-unspecified]] named-owner rule: owner = @cipherocto, target = 2026-09-15.

**Sub-mission of:** `missions/claimed/0860a-porelay-registry-anti-sybil.md` (Band A closed 2026-08-06).

## RFC

RFC-0860 (Networking): Proof-of-Relay — §7 (Anti-Sybil), §8 (Economic Integration)
RFC-0854 (Networking): Deterministic Proof Substrate (DPS) — Accepted

## Summary

Bind the canonical RFC-0854 DPS module to the recursive relay proof aggregation path in `crates/octo-network/src/porelay/aggregation.rs` (currently a doc-only STARK reference per the ungrounded AC). Author the monthly gateway earnings calculation algorithm: monthly_earnings = relay_bandwidth_revenue + uptime_bonus + diversity_premium, denominated in OCTO-B + OCTO-N per RFC-0860 §8 tokenomics split.

The `0860a` Band A closure deferred this work because (a) the DPS module is owned by the RFC-0854 substrate mission and not yet wired into the PoRelay aggregation path, and (b) the monthly earnings calculation requires the per-axis revenue model that the RFC-0860 §8 tokenomics table does not enumerate explicitly (the band A closure left a doc-level placeholder in `crates/octo-network/src/porelay/economics.rs`).

## Acceptance Criteria

### DPS aggregation backend wiring

- [x] `crates/octo-network/src/porelay/aggregation.rs` — wire `crate::dps::recursive::{RecursiveAggregator, AggregatedProof, AggregationMethod}` + `crate::dps::suite::ProofSystemId` into `aggregate_children`; replaced `Vec::new()` placeholder with real `AggregatedProof` blob (80-byte header + `expected_blob_commitment` slot + `aggregated_blob` body); canonical BLAKE3 cascade computed via `compute_children_cascade_root` mirrors `children_root` for wire-format consistency.
- [x] `crates/octo-network/src/dps/` — pre-existing module facade used (no new module required); substrate exports `RecursiveAggregator::new(system, method).add_proof(commitment).build(blob, public_input_root)` per RFC-0854.
- [x] STARK verification on aggregation: `AggregatedRelayProof::verify(&self) -> Result<(), AggregationError>` — parses wire blob, calls DPS `AggregatedProof::verify(expected_blob_commitment)`, asserts `aggregation_root == children_root`.

### Monthly gateway earnings

- [x] `crates/octo-network/src/porelay/economics.rs` — `pub fn compute_monthly_earnings(gateway: &GatewayMetrics, period_unix: RangeInclusive<i64>) -> EarningsBreakdown` returning `{ octo_b: u64, octo_n: u64 }` denominated in micro-units (1 OCTO = 1_000_000 micro).
- [x] Components: `relay_bandwidth_revenue` (per-GB × `RELAY_RATE_B_MICRO_OCTO_PER_GB`), `uptime_bonus` (power-curve `uptime_fraction^SIGMOID_K` over uptime permille × `UPTIME_BONUS_MAX_OCTO_N` — replaces logistic sigmoid so 0% yields exactly 0), `diversity_premium` (linear in distinct_peer_count × `DIVERSITY_PREMIUM_OCTO_B_PER_PEER`).
- [x] Constant tables: `RELAY_RATE_B_MICRO_OCTO_PER_GB = 100_000`, `UPTIME_BONUS_MAX_OCTO_N = 50_000_000`, `DIVERSITY_PREMIUM_OCTO_B_PER_PEER = 5_000`, `SIGMOID_K = 4.0` — all declared at module top with RFC-0860 §8 source comment.
- [x] `apply_por_earnings_boost(earnings: EarningsBreakdown, relay_score: f64) -> EarningsBreakdown` — multiplies both components by `1.0 + max(0.0, relay_score)` per RFC-0860 §8 PoR boost clause; negative relay_score clamps to 0 (no penalty).

### Test vectors

- [x] TV1: Aggregation DPS wiring — `tv1_aggregate_children_dps_round_trip` asserts non-empty `proof_blob`, round-trips through `verify()`.
- [x] TV2: Monthly earnings happy path — `tv2_monthly_earnings_happy_path` asserts 100 GB + 99.9% uptime + 5 peers → OCTO-B = 10_025_000 micro, OCTO-N in 49M-50M range.
- [x] TV3: PoR boost — `tv3_por_earnings_boost_multipliers` asserts 0.0→1.0x, 1.0→2.0x, -0.5→clamped to 1.0x.
- [x] TV4 (bonus): `verify_rejects_blob_body_tamper` + `verify_rejects_root_mismatch` + `verify_rejects_truncated_proof_blob` — security guard against in-transit blob mutation.

### Cross-crate compat

- [x] `cargo build -p octo-network` green
- [x] `cargo test -p octo-network --lib porelay` green — 90/90 pass (77 pre-existing + 7 new aggregation tests + 6 new economics tests)
- [x] `cargo test -p octo-network --lib` green — 1351/1351 pass
- [x] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [x] `cargo fmt --check -p octo-network` clean

### Deferred (out-of-scope for this mission)

- [ ] `tests/fixtures/porelay/monthly_earnings_goldens.json` — fixture file path referenced in mission text but not shipped; future mission can add golden-value pinning for multi-gateway tabular comparison (TV2 already covers the canonical happy-path via direct assertion). **Deferral:** owner = @cipherocto, target = 2026-09-15 per [[deferred-vs-unspecified]] named-owner rule. Fixture scope = multi-row tabular regression (e.g., 4-8 gateway profiles with distinct bandwidth/uptime/peer combos asserting `compute_monthly_earnings` produces byte-stable outputs); distinct from TV2 single-row happy-path assertion. Tracked as hygiene follow-up; AC-13/AC-14/AC-15/AC-16 (TV1-TV4 + clippy/fmt) green today.

## Dependencies

**Requires (RFC gates):**

- RFC-0854 — DPS substrate (deterministic proof system)
- RFC-0860 — PoRelay tokenomics (per-axis revenue model)

**Requires (mission gates):**

- `missions/claimed/0860a-porelay-registry-anti-sybil.md` (Band A closed 2026-08-06) — provides `aggregated_proof_root`, `apply_por_boost` stub, `relay_score_to_trust_factor`, and 12 unit tests for the trust/score layer

```yaml
depends_on:
  - 0860a-porelay-registry-anti-sybil # AggregatedRelayProof substrate + RelayScore + apply_por_boost stub
  - RFC-0854 # DPS substrate for aggregation backend
```

## Location

- `crates/octo-network/src/porelay/aggregation.rs` (MODIFY) — wire `crate::dps::*` into `aggregate_children` + new `AggregatedRelayProof::verify` method
- `crates/octo-network/src/porelay/economics.rs` (MODIFY) — `compute_monthly_earnings` impl + constants table
- `crates/octo-network/src/dps/` (NEW, optional) — DPS module facade if not yet present in the workspace

## Claimant

@mmacedoeu (claimed 2026-08-07)

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                                                                                                                      |
| ------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-06 | Filed open by mission `0860a-porelay-registry-anti-sybil` Band A closure. 14 ACs.                                                                                                                                                                                                                                                                                                                                           |
| v0.2    | 2026-08-07 | Claimed + landed same-session. 13/14 ACs green; 1 AC deferred (`tests/fixtures/porelay/monthly_earnings_goldens.json` not shipped — TV2 happy path already pins exact values directly so fixture is optional). 90/90 porelay tests pass (77 pre-existing + 7 new aggregation + 6 new economics); full `cargo test -p octo-network --lib` green (1351/1351); clippy `-D warnings` clean; fmt clean. Single commit on `next`. |
| v0.3    | 2026-08-07 | Audit-closure roll-up. Status header Claimed → Closed (Band A — 2026-08-07); 13/14 GREEN + 1/14 DEFERRED with named owner (@cipherocto) + target (2026-09-15) per [[deferred-vs-unspecified]] named-owner rule. Deferral scope explicit: multi-row tabular regression fixture distinct from TV2 single-row happy-path. No new substrate (mission text was already accurate; closure is documentation discipline only).      |
