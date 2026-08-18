---
name: mission-0862-c11-tv-coverage-gap
description: S6c Round 3 Tier-3 TV-coverage closure LANDED 2026-08-18 — TV-22 (zero-cost no-op), TV-24 (macaroon_id edge), TV-25 (seed zero-budget); TV-23 dropped (Dqa::new enforces scale upper boundary at type layer)
metadata:
  type: project
---

# Mission 0862-c11-tv-coverage-gap — TV-coverage edge cases (LANDED 2026-08-18)

## Verdict

S6c Round 3 adversarial review (sprint `wf_bd836955-609`, 204 agents, 4 rounds, 106 confirmed findings) surfaced FOUR TV-coverage findings (#13..#16) out-of-scope'd in mission 0862-c10 (doc-drift consolidation). All closed in this mission. TV-only addition; no new substrate surface. 3 new TV landed (TV-22, TV-24, TV-25); TV-23 dropped at discovery.

## TV landed

### TV-0862-22 — `tv_0862_22_try_deduct_zero_cost_no_op`

Pre-seed balance=1000; call `try_deduct(holder, mac, Dqa(0, 0))`; assert returned balance = 1000 (unchanged), assert no error, assert stored balance unchanged. Pins free-tier query / sanity-ping no-op semantics. AC-1 ✅.

### TV-0862-24 — `tv_0862_24_macaroon_id_accepts_any_bytes`

Four representative `macaroon_id` shapes (empty slice, single byte, canonical 16-byte, 64-byte binary garbage). Substrate accepts all four; each persists as a distinct row. Mirrors TV-14 (holder_did axis) for the macaroon_id axis per mission 0862-c6 contract ("any bytes; canonical validation lives at wallet-node boundary"). AC-3 ✅.

### TV-0862-25 — `tv_0862_25_seed_zero_budget_persists`

`seed(holder, mac, Dqa(0, 0))` succeeds + balance returns `Some(0)`. Cross-check: positive-cost `try_deduct` against the zero-balance row surfaces `InsufficientBalance` (proves the row is wired into the check path). AC-4 ✅.

## TV dropped

### TV-0862-23 — `Dqa::new(100, 255)` is rejected at construction

**Finding:** the `Dqa::new` constructor itself rejects `scale = 255` (u8::MAX) with `DqaError::InvalidScale`. The scale upper boundary is enforced at the **type layer**, not at the substrate. Substrate never sees scale=255.

**Discovery:** attempted `let scale_max_dqa = Dqa::new(100, 255).expect(...);` in TV-23 → `Result::Err(InvalidScale)` at construction; substrate unreachable.

**Resolution:** AC-2 dropped from mission. The upper-scale boundary is already enforced at `Dqa::new` (covered by determin-layer tests, not in scope for RFC-0862). Documented in RFC-0862 v2.0.11 row under TV-0862-23 (DROPPED).

## AC closeout

- AC-1 ✅ TV-0862-22 (zero-cost no-op)
- AC-2 ❌ TV-0862-23 DROPPED — `Dqa::new` enforces scale upper boundary at type layer
- AC-3 ✅ TV-0862-24 (macaroon_id edge)
- AC-4 ✅ TV-0862-25 (seed zero-budget)
- AC-5 ✅ RFC-0862 v2.0.11 row documenting AC-1/3/4 + AC-2 DROPPED
- AC-6 ✅ clippy zero + cargo fmt clean + 23/23 TV green

## Files changed

- `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs` — TV-0862-22 + TV-0862-24 + TV-0862-25 added; TV-0862-23 attempted then dropped
- `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md` — v2.0.11 row appended (includes TV-23 DROPPED note)
- `missions/open/0862-c11-tv-coverage-gap.md` → `missions/claimed/0862-c11-tv-coverage-gap.md` (LANDED)

## Layer direction

TV-only addition; no substrate surface change. Layer A frozen-substrate principle preserved (no additive API surface; TV pins existing surface).

## Related

- [[mission-0862-c11-lock-file-hardening]] — v2.0.10 (lock-file hardening) — parent mission that closed Tier-2 HIGH security.
- [[mission-0862-c10-doc-drift]] — v2.0.9 (doc-only consolidation) — sibling doc-only mission that out-of-scope'd TV-coverage findings.
- [[mission-0862-c6-fixture-keyspace]] — v2.0.7 (no-DID-validation convention) — TV-24 mirrors TV-14 (holder_did axis) per c6 contract.
- [[cipherocto-design-principles]] — Layer A frozen substrate (TV-only additive).