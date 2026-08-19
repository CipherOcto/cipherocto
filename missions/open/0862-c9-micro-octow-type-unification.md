# Mission: 0862-c9 — `MicroOctoW` type-alias unification across workspace

## Status

**RETIRED 2026-08-19 (drift closure).** Superseded by USER MANDATE
2026-08-17 (per memory card `mission-0862-c9-retired-kill-microoctow.md`):
"kill every variant of the alias (`MicroOctoW`, `MicroOCTO_W`,
`MicroOCToW`, `MicroOCTO_WNewtype`) project-wide; use
`octo_determin::Dqa` directly." Original canonical-aliasing plan
(ababged in favor of total alias removal. Original mission scope
("`pub type MicroOctoW = Dqa` + remove redundant definitions")
was **abandoned**; mandate goes further by removing the alias
entirely (not just unifying definitions).

**Implementation status:** Already LANDED via separate commits:
- `2750caa7` + `b20c37dc` — initial alias kill (LANDED 2026-08-17)
- `2a610c3d` — MicroOctoW removed from 27 files (mission `0862-c9 RETIRED kill MicroOctoW`)
- `01a6d43d` + `0ff3e5a3` — borsh re-introduction (subset re-add per mission `0862-c9 borsh re-introduction`)

**This mission file is drift-closed** — superseded by the mandate
above + the actual alias-kill commits. No new ACs apply.

**RFC**

- Primary: RFC-0862 (`StoolapSpendLedger` substrate — type-system
  coherence with the DQA-based spending model per §Future Work F12)
(storage restructure hard-recommendation #1). Closes parallel-model
risk surfaced by audit of `MicroOctoW` type-alias split: 3
definitions, 2 distinct underlying types (`u128` vs
`octo_determin::Dqa`) in same workspace. Capability builder emits
caveats with `MicroOctoW = u128`; spend_ledger substrate reads
`MicroOctoW = Dqa`. Silent runtime drift at boundary.

## RFC

- Primary: RFC-0862 (`StoolapSpendLedger` substrate — type-system
  coherence with the DQA-based spending model per §Future Work F12)
- Co-RFC: RFC-0105 (Dqa substrate — authoritative source type)
- Co-RFC: RFC-0965 (caveat payload bytes — uses
  `MicroOctoW`-bearing payload field)

## Dependency edges

| From                                                      | To                          | Why             | Layer direction |
| --------------------------------------------------------- | --------------------------- | --------------- | --------------- |
| `determin/src/lib.rs` (`pub type MicroOctoW = Dqa`)       | `octo_determin::Dqa`        | Canonical alias | lib → lib       |
| `crates/octo-cap-macaroon/src/caveat/mod.rs` (remove)     | `octo_determin::MicroOctoW` | Cross-crate use | lib → lib       |
| `crates/octo-cap-macaroon/src/caveat/payment.rs` (remove) | `octo_determin::MicroOctoW` | Cross-crate use | lib → lib       |
| `crates/quota-router-storage/src/stoolap_spend_ledger.rs` | `octo_determin::MicroOctoW` | Cross-crate use | lib → lib       |
| `crates/octo-cap-macaroon/Cargo.toml`                     | `octo-determin` (git dep)   | New dep         | Cargo → git     |

No new cyclic edges. Single new crate dep (`octo-cap-macaroon`
gains `octo-determin`).

## Problem

Audit (2026-08-17) found `MicroOctoW` defined 3 times across
workspace with 2 distinct underlying types:

| Site                                                  | Definition                                  |
| ----------------------------------------------------- | ------------------------------------------- |
| `octo-cap-macaroon/src/caveat/mod.rs:19`              | `pub type MicroOctoW = u128;`               |
| `octo-cap-macaroon/src/caveat/payment.rs:29`          | `pub type MicroOctoW = u128;`               |
| `quota-router-storage/src/stoolap_spend_ledger.rs:74` | `pub type MicroOctoW = octo_determin::Dqa;` |

Two parallel type aliases for the same name:

- **Path A** (`u128`) — caveat builder (RFC-0965 §3.x caveat payload
  bytes, `PaymentCaveat.budget`, `AmountMax(MicroOctoW)`,
  `query_cost: MicroOctoW`, etc.)
- **Path B** (`Dqa`) — spend_ledger substrate (`StoolapSpendLedger`:
  `seed()` + `try_deduct()` parameters and return types)

Cross-boundary risk:

- Capability with `PaymentCaveat { budget: 1000_u128 }` is persisted
  in caveat bytes as `u128`; consumer in spend_ledger substrate
  reads `Dqa` from caveat wire form
- Stoolap SpendLedger's `try_deduct(cost: MicroOctoW)` parameter is
  `Dqa`; but quota-router-core's `SpendEvent.cost_amount: u64`
  callers (per 0862-c7) eventually narrow to `i64` and write to
  INTEGER column carrying `Dqa::value` at scale=0

This is the **last parallel model in the spending path**. Per audit
verdict, all three amount representations (caveat payload u128,
spend_ledger Dqa, marketplace/task_market/slash_store u128) must
unify to a single canonical type before S8 PR bundle. RFC-0862 v2.0
(S6c LANDED) introduced `MicroOctoW = Dqa` for spend_ledger but did
not unify across caveat crate.

## Acceptance Criteria

- AC-1: `pub type MicroOctoW = Dqa;` added to
  `determin/src/lib.rs` (canonical alias in workspace-excluded root
  substrate crate; consumed by `octo-determin` API consumers
  workspace-wide)
- AC-2: Three local `pub type MicroOctoW = ...` definitions REMOVED
  from:
  - `crates/octo-cap-macaroon/src/caveat/mod.rs`
  - `crates/octo-cap-macaroon/src/caveat/payment.rs`
  - `crates/quota-router-storage/src/stoolap_spend_ledger.rs`
- AC-3: `crates/octo-cap-macaroon/Cargo.toml` adds `octo-determin`
  git dep (matches `quota-router-storage` Cargo.toml pin per the
  S4 codemod pattern)
- AC-4: All call sites of `crate::MicroOctoW` in
  `crates/octo-cap-macaroon/src/caveat/` re-routed through
  `octo_determin::MicroOctoW` (cross-crate use, no shadow alias)
- AC-5: New TV (TV-0862-17): byte-exact `MicroOctoW` round-trip
  through `caveat::payment::PaymentCaveat::new(budget: MicroOctoW)`
  - decode path. Same byte sequence at caveat payload boundary
    whether caller constructed `Dqa { value: 1000, scale: 0 }` directly
    or via `octo_determin::MicroOctoW`
- AC-6: Existing TV in `octo-cap-macaroon` + `quota-router-storage`
  stay byte-stable (caveat bytes unchanged when constructed from
  `u128` literal vs `Dqa` literal at scale=0 with same `value`)
- AC-7: RFC-0862 §SpendLedger Substrate subsection cross-references
  the canonical alias + workspace-wide type-system coherence
  invariant in §Version History v2.0.3 row
- AC-8: RFC-0965 §3 caveat payload codec spec cross-references the
  canonical alias (caveat payload type field uses
  `octo_determin::MicroOctoW`, not local `u128`)

## Cross-reference

- **Parent:** `missions/open/0862-c1-dqa-vault-bump-amendment.md`
  (LANDED 2026-08-17, commits `2750caa7` + `b20c37dc`)
- **Pattern:** `crates/quota-router-storage/src/stoolap_spend_ledger.rs:74`
  — first workspace-local canonical alias attempt (must be promoted
  to `octo-determin` per AC-1)
- **Sibling:** `missions/open/0862-c7-adjacent-wrap.md` (LANDED),
  `missions/open/0862-c8-seed-hardening.md` (LANDED)
- **Co-mission (parallel):**
  `missions/open/0105-x-s4-deferred-codemod-sites.md` (filed
  2026-08-17) — extends u128→Dqa codemod to deferred sites;
  c9 + x-mission together close audit-verdict Risks #1 + #4
- **Plan:** `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 S7 row (Stream A.2 — B1 RFC-0965 + B2 RFC-0105 amendments
  cover the wire-form side; c9 is the type-system coherence layer
  on top)
- **Audit source:** 2026-08-17 audit verdict, Risk #1 (CRITICAL)

## Critical files

- `determin/src/lib.rs` (modify — add `pub type MicroOctoW = Dqa;`
  canonical alias)
- `crates/octo-cap-macaroon/Cargo.toml` (modify — add
  `octo-determin` git dep)
- `crates/octo-cap-macaroon/src/caveat/mod.rs` (modify — remove
  local `pub type MicroOctoW = u128`, re-route imports to
  `octo_determin::MicroOctoW`)
- `crates/octo-cap-macaroon/src/caveat/payment.rs` (modify — same)
- `crates/quota-router-storage/src/stoolap_spend_ledger.rs` (modify
  — remove local `pub type MicroOctoW = octo_determin::Dqa`,
  re-route to canonical alias via re-export from
  `quota-router-storage/src/lib.rs`)
- `crates/quota-router-storage/src/lib.rs` (modify — re-export
  `octo_determin::MicroOctoW` for ergonomic use across storage
  crate)
- `crates/quota-router-storage/tests/tv_0862_c9_type_alias.rs` (NEW
  — TV-0862-17 cross-crate round-trip)
- `crates/octo-cap-macaroon/tests/tv_0862_c9_caveat_alias.rs` (NEW
  — TV-0862-18 caveat payload bytes unchanged when caller uses
  `u128` literal vs `Dqa` literal)
- `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md`
  (modify — §Version History v2.0.3 row + §SpendLedger Substrate
  cross-ref to canonical alias)
- `rfcs/accepted/networking/0965-caveat-extension-format.md` (modify
  — §3 caveat payload type cross-ref)

## Existing patterns reused

- `StoolapSpendLedger::seed(budget: MicroOctoW)` precondition
  `budget.scale == 0` (assert_eq! from S6c c1) — same invariant
  must hold at the canonical alias call sites
- `StoolapSpendLedger::dqa_to_i64(v: MicroOctoW) -> i64` helper
  (mirrors `determin::Dqa::to_i64_at_scale_0` — verify both
  shapes match byte-exact)
- `crates/octo-cap-macaroon/src/caveat/payment.rs::PaymentCaveat::new`
  constructor (RFC-0965 §3.x) — signature changes from
  `budget: MicroOctoW` (local u128) to
  `budget: octo_determin::MicroOctoW` (canonical Dqa)

## Risks

- **API churn** (HIGH): `pub type MicroOctoW` removal from 2 crates
  ripples to ALL `caveat::*` constructors + `PaymentCaveat::new` +
  `StoolapSpendLedger::{seed, try_deduct, balance}` callers. Caller
  pattern: `let budget: u128 = ...; caveat::PaymentCaveat::new(budget)`
  now becomes `let budget: Dqa = Dqa { value: 1000, scale: 0 };
caveat::PaymentCaveat::new(budget)`. **Migration is
  breaking.** Mitigation: ship via codemod + central migration
  commit; do not split per-RFC (single commit lands all 3 crate
  re-exports together).
- **Caveat wire-form drift** (HIGH): if `MicroOctoW` resolution at
  caveat payload encoding changes (e.g., `u128` → `DqaEncoding`
  instead of `Dqa::value` as `i64`), caveat bytes mutate and
  existing TV fail. Mitigation: AC-6 byte-stable invariant; new
  TV-0862-17 + TV-0862-18 pin the round-trip.
- **Codemod coverage** (MED): S4 codemod touched 155 sites across 8
  crates (per memory card `S4-codemod-2026-08-17-LANDED.md`). 146
  sites deferred to S6/S7. c9 + `0105-x-s4-deferred-codemod-sites`
  together catch the deferred sites; c9 alone covers caveat crate.
- **Cargo workspace dep graph** (LOW): `octo-cap-macaroon` gains
  `octo-determin` dep. Per layer model (CLAUDE.md §Architectural
  Principles), `octo-determin` is Layer A (frozen substrate);
  `octo-cap-macaroon` is Layer B (RFC-driven, additive). Layer B
  depending on Layer A is allowed.

## Version history

| Date       | Author     | Change                                                                                                                                                                                        |
| ---------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-08-17 | @mmacedoeu | Initial filing per audit verdict 2026-08-17 (storage restructure hard-recommendation #1, parallel-model Risk #1 CRITICAL). Co-filed with `0105-x-s4-deferred-codemod-sites` for Risk #4 HIGH. |
| 2026-08-17 | (mandate)  | USER MANDATE: kill every `MicroOctoW`-variant alias project-wide. Original canonical-aliasing plan abandoned. Implementation via separate commits `2750caa7` + `b20c37dc` + `2a610c3d`. |
| 2026-08-18 | (commits)  | Borsh re-introduction subset per mission `0862-c9 borsh re-introduction` (`01a6d43d` + `0ff3e5a3`). `BorshSerialize`/`BorshDeserialize` for `Dqa` shipped (Layer A additive change). |
| 2026-08-19 | @mmacedoeu | Drift closure — mission file superseded by mandate + landed commits. Status → RETIRED. No new ACs. |

## Out of scope

- Backfill u128→Dqa codemod in marketplace/task_market/slash_store/settlement_event_repo
  (separate mission `0105-x-s4-deferred-codemod-sites`)
- Settlement wire-form DqaEncoding conversion for `cost_micro_octo_w`
  (RFC-0959 amendment — S6e mission `0959-c1-wire-format-amendment`)
- Caveat payload codec DqaEncoding conversion for amount-bearing
  variants (RFC-0965 amendment — S7 mission)
- Slash ledger schema DQA(12) + chain_id column promotion
  (RFC-0900 amendment — S6d mission)
- Stoolap native DQA column adoption for spend_ledger v007 (separate
  schema migration; c9 leaves the schema as-is, fixes type alias only)
