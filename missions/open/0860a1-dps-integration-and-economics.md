# Mission: DPS Aggregation Backend Integration + Monthly Earnings Calculation (RFC-0860 §7+§8 follow-up)

## Status

Open (filed 2026-08-06 by mission `0860a-porelay-registry-anti-sybil.md` Band A closure). Per [[deferred-vs-unspecified]] named-owner rule, this follow-up mission owns the deferred DPS (RFC-0854) integration for recursive relay proof aggregation backend + monthly gateway earnings calculation (OCTO-B + OCTO-N) per RFC-0860 §8.

**Sub-mission of:** `missions/claimed/0860a-porelay-registry-anti-sybil.md` (Band A closed 2026-08-06).

## RFC

RFC-0860 (Networking): Proof-of-Relay — §7 (Anti-Sybil), §8 (Economic Integration)
RFC-0854 (Networking): Deterministic Proof Substrate (DPS) — Accepted

## Summary

Bind the canonical RFC-0854 DPS module to the recursive relay proof aggregation path in `crates/octo-network/src/porelay/aggregation.rs` (currently a doc-only STARK reference per the ungrounded AC). Author the monthly gateway earnings calculation algorithm: monthly_earnings = relay_bandwidth_revenue + uptime_bonus + diversity_premium, denominated in OCTO-B + OCTO-N per RFC-0860 §8 tokenomics split.

The `0860a` Band A closure deferred this work because (a) the DPS module is owned by the RFC-0854 substrate mission and not yet wired into the PoRelay aggregation path, and (b) the monthly earnings calculation requires the per-axis revenue model that the RFC-0860 §8 tokenomics table does not enumerate explicitly (the band A closure left a doc-level placeholder in `crates/octo-network/src/porelay/economics.rs`).

## Acceptance Criteria

### DPS aggregation backend wiring

- [ ] `crates/octo-network/src/porelay/aggregation.rs` — replace doc-only STARK reference with actual `crate::dps::*` (or RFC-0854 substrate import) call in `aggregate_children`. The aggregate produces a STARK proof that all children are valid; the BLAKE3 `children_root` cascade (R1 review F1 fix) becomes the STARK public input.
- [ ] `crates/octo-network/src/dps/` (NEW) — DPS module facade if not yet present in the workspace; exposes `prove_relay_aggregation(children_root, parent_level, scope, epoch) -> Result<StarkProof, DpsError>` matching the RFC-0854 substrate API.
- [ ] STARK verification on aggregation: `verify_aggregation(proof, public_inputs) -> Result<(), DpsError>` consumed by `AggregatedRelayProof::verify` (new method).

### Monthly gateway earnings

- [ ] `crates/octo-network/src/porelay/economics.rs` — `pub fn compute_monthly_earnings(gateway: &GatewayMetrics, period_unix: RangeInclusive<i64>) -> EarningsBreakdown` returning `{ octo_b: u64, octo_n: u64 }` denominated in micro-units (1 OCTO = 1_000_000 micro).
- [ ] Components: `relay_bandwidth_revenue` (per-GB relayed × relay_rate_b_micro_octo_per_gb), `uptime_bonus` (sigmoid over uptime_pct × uptime_bonus_max_octo_n), `diversity_premium` (linear in distinct_peer_count × diversity_premium_octo_b_per_peer).
- [ ] Constant tables: `RELAY_RATE_B_MICRO_OCTO_PER_GB`, `UPTIME_BONUS_MAX_OCTO_N`, `DIVERSITY_PREMIUM_OCTO_B_PER_PEER`, `SIGMOID_K` (steepness) — all declared at module top with source comment citing RFC-0860 §8.
- [ ] `apply_por_boost(earnings: &EarningsBreakdown, relay_score: f64) -> EarningsBreakdown` — multiplies both components by `1.0 + max(0.0, relay_score)` per RFC-0860 §8 PoR boost clause.

### Test vectors

- [ ] TV1: Aggregation DPS wiring — `aggregate_children` produces a `StarkProof` byte-blob; `verify_aggregation` round-trips successfully for a 3-child local → regional aggregation.
- [ ] TV2: Monthly earnings happy path — gateway with 100 GB relayed + 99.9% uptime + 5 distinct peers returns expected `EarningsBreakdown` (golden values in `tests/fixtures/porelay/monthly_earnings_goldens.json`).
- [ ] TV3: PoR boost — relay_score=0.0 → no boost; relay_score=1.0 → 2x earnings; relay_score=-0.5 → clamped to 0 boost.

### Cross-crate compat

- [ ] `cargo build -p octo-network` green
- [ ] `cargo test -p octo-network --lib porelay` green (77 pre-existing + 3 new TVs = 80+ total)
- [ ] `cargo clippy -p octo-network --all-targets --features full -- -D warnings` clean (per [[feedback_clippy_zero_warnings]])
- [ ] `cargo fmt --check -p octo-network` clean

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

TBD (claim 2026-08-06+)
