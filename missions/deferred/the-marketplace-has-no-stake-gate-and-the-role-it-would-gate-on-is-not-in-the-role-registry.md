---
name: the-marketplace-has-no-stake-gate-and-the-role-it-would-gate-on-is-not-in-the-role-registry
description: "RFC-0959 states participants MUST satisfy dual-stake requirements and gives the Asker a required stake of OCTO global plus a role-specific token. token-design.md section 10 states every participant must stake both OCTO and a role token. Neither is enforced anywhere in the marketplace: the word stake does not appear once in the quota-router-core marketplace facade, and neither MarketplaceIndex::select_ask nor AskRead::cheapest takes a stake parameter. The enforcement substrate does exist, in octo-role, which carries requires_octo_min and role_token_ticker. But Asker is absent from that registry: its documented slugs are builder, provider, storage, bandwidth, orchestrator, recorder and wallet, and the string Asker appears nowhere in the crate. Two incompatible RoleTag enums exist, with 5 variants in role_binding and 3 in market_delivery, and nothing converts between them, so Asker cannot be mapped onto a stake-bearing role even in principle."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959
    - RFC-0900
    - RFC-0971
---

# Mission `the-marketplace-has-no-stake-gate-and-the-role-it-would-gate-on-is-not-in-the-role-registry`

## Status

Deferred — deciding that `Asker` is stake-bearing, and which registry entry
represents it, is a tokenomics decision. It changes the role taxonomy, which
is Layer B and shared well outside the marketplace. Nothing was actioned; no
measurement probe was needed, since the absence is structural.

This is the cheapest finding in the audit to act on, because unlike most of
them it is **wiring, not building**. The enforcement substrate already exists.

## Finding

The marketplace has no economic entry gate, and the reason it has none is
not that the gate is hard — it is that the role the gate would test does not
exist in the system that holds stake requirements.

### The requirement

RFC-0959 §Economic Analysis is normative:

> Participants **MUST** satisfy dual-stake requirements per
> `docs/04-tokenomics/token-design.md`.

with a per-role table:

| Role                  | Required Stake                                          |
| --------------------- | ------------------------------------------------------- |
| Asker (publishes Ask) | OCTO global stake + role-specific stake (provider role) |

`docs/04-tokenomics/token-design.md` §10 The Dual-Stake Model states:

> Every participant must stake **both**: 1. **OCTO** (global alignment) 2. **Role Token** (local specialization)

and gives the rationale — preventing role tourism and misaligned incentives,
which is the mechanism that makes a marketplace safe to route payments
through.

### The implementation has no gate

The word `stake` does not appear **once** in
`crates/quota-router-core/src/marketplace/mod.rs`.

Neither selection entry point takes a stake parameter:

- `MarketplaceIndex::select_ask(model, jurisdiction, budget_ceiling, axes, now_unix)`
- `AskRead::cheapest(model, now_unix, axes)`

`budget_ceiling` is a per-call spend cap on the _buyer_, not a stake
requirement on the _seller_. So nothing distinguishes a seller who has met
its stake obligation from one who has staked nothing, because there is no
notion of an unstaked seller to distinguish.

### The enforcement substrate exists — elsewhere

Dual-stake **is** implemented, in `octo-role`. `RoleSummary` carries exactly
the fields the model requires:

```rust
pub role_token_ticker: Option<String>,   // display ticker, e.g. OCTO-A
pub requires_octo_min: Option<u64>,      // required OCTO stake, micro-OCTO
```

and `RoleBinding` carries `stake_role_token`, with `select` enforcing the
minimum. `octo-network/porelay/anti_sybil.rs` consults the same vocabulary.

So the marketplace is not missing a capability. It is not calling one that
exists three crates away.

### The role is not in the registry

This is the part that makes the gap non-trivial to close.

`octo-role` documents its role slugs as:

```text
builder, provider, storage, bandwidth, orchestrator, recorder, wallet
```

and the string `Asker` appears **nowhere** in the crate — not as a slug, not
in an enum, not in a test fixture.

Meanwhile the marketplace's own role enums are:

| Enum                                                | Variants                                                              |
| --------------------------------------------------- | --------------------------------------------------------------------- |
| `quota_router_core::node::role_binding::RoleTag`    | `Router`, `TokenIssuer`, `Asker`, `PureForwarder`, `ReputationAnchor` |
| `octo_wallet::capability::market_delivery::RoleTag` | `Asker`, `TokenIssuer`, `Router`                                      |

Two `RoleTag` types with **different variant sets** (five and three), and no
conversion between them anywhere in the tree. `cross_role_data_flow.rs` uses
`role_binding::RoleTag::Asker` directly and never routes it through a role
registry.

So even the obvious fix — "have `select_ask` check the seller's stake" — has
no obvious first step. The question "what stake does an `Asker` owe?" is not
answerable from the code, because the role taxonomy that would answer it
(`octo-role`) does not contain the role, and the taxonomy that does contain
it (`role_binding`) carries no stake fields.

## Why this is a wiring problem rather than a feature gap

That distinction is the useful part of this finding, and it sets it apart
from most of the audit.

The other marketplace findings need something built: a settlement signature,
a lifecycle state, a jurisdiction field, an `ask_id` bridge. Each is
Layer A or a wire-format change, each is a consensus-visible decision, and
each was correctly deferred.

This one needs a **call**. `octo-role` already computes whether a role meets
its stake minimum. The marketplace does not ask. The blocker is narrower and
more tractable: a taxonomy decision about whether `Asker` is a distinct
stake-bearing role or an alias for the existing `provider` slug.

The RFC's own wording hints at the answer — "role-specific stake (**provider
role**)" — which reads as `Asker` being the marketplace-facing name for
something the registry already calls `provider`. If that reading is right,
the fix is a mapping plus a gate, not a new economic concept.

That reading is not established. Nothing in the code or the RFCs states the
equivalence, and it is recorded here as the likely shape of the fix rather
than as a conclusion.

## Calibration — what the surrounding code gets right

The slashing ledger, which is the other half of the economic security story,
is in good shape. `withdraw_stake` refuses a permanently banned provider
outright, so an operator cannot exit a banned position by withdrawing stake;
it rolls the in-memory balance back when the store write fails; and negative
stake registration was found, measured, and fixed before this audit began.
The reputation registry carries both a sync and an async path with the
dual-read divergences already recorded in their own missions.

The gap here is specifically that **nothing connects the marketplace to any
of it**. The defences exist and are tested; they are simply not in the
marketplace's path.

## What the RFC owner needs to decide

- Is `Asker` a distinct stake-bearing role, or the marketplace-facing name for
  the existing `provider` slug? RFC-0959's "(provider role)" parenthetical
  suggests the latter; nothing confirms it.
- If distinct, does it enter the `token-design.md` §10 table, and with which
  role token? Adding a row to a tokenomics table is an economics decision.
- Which of the two `RoleTag` enums is canonical? `role_binding` has the RFC-0971
  five-variant set and no stake fields; `market_delivery` has three and is
  used by a capability-macaroon surface. Two incompatible role taxonomies is a
  problem independent of the marketplace.
- Should `select_ask` and `AskRead::cheapest` take a stake predicate? That is a
  signature change on both, and it is the point at which the gate becomes
  testable.
- Should the stake check happen at **publish** (an unstaked asker cannot list)
  or at **selection** (an unstaked asker cannot be chosen), or both? RFC-0959's
  table says the Asker's obligation is "publishes Ask", which reads as
  publish-time; the routing safety argument favours selection-time.
- What happens to Asks already listed by a party whose stake later drops? The
  slashing ledger can produce a ban, and `withdraw_stake` honours it — but
  nothing re-evaluates listings on ban.

## Reproduction

```bash
# the normative requirement
grep -n -B2 -A8 'dual-stake' rfcs/accepted/economics/0959-ask-settlement-chain.md
grep -n -A14 '## 10. The Dual-Stake Model' docs/04-tokenomics/token-design.md

# no gate: the word does not appear
grep -c 'stake\|Stake' crates/quota-router-core/src/marketplace/mod.rs

# neither selection entry point takes a stake parameter
grep -n -A8 'pub fn select_ask' crates/quota-router-storage/src/marketplace.rs
grep -n -A6 'fn cheapest' crates/quota-router-storage/src/ask_repo.rs

# the substrate that would enforce it
grep -n -A10 'pub struct RoleSummary' crates/octo-role/src/types.rs
grep -n 'requires_octo_min\|stake_role_token' crates/octo-role/src/types.rs

# Asker is absent from the registry
grep -rn 'Asker' crates/octo-role/src/ || echo 'zero occurrences'

# two incompatible role taxonomies
grep -rn -A8 'pub enum RoleTag' --include=*.rs crates/

# nothing bridges them
grep -rn 'role_binding::RoleTag\|market_delivery::RoleTag' --include=*.rs crates/
```

## Cross-references

- RFC-0959 §Economic Analysis — the MUST, the per-role table, and the
  "(provider role)" parenthetical that this mission does not treat as
  conclusive
- `docs/04-tokenomics/token-design.md` §10 — the dual-stake model and its
  role table, which has no `Asker` row
- RFC-0971 §Phase 1 — the `RoleBindingDeclaration` and five-variant
  `RoleTag` the marketplace uses
- Mission `marketplace-index-is-a-parallel-unwired-abstraction` — the same
  index that would carry the gate, from the consumer side
- Mission `marketplace-escrow-is-not-wired-into-the-marketplace-facade` —
  escrow and slashing are likewise present and unconnected; this finding is
  the entry-gate half of that pattern
- Mission `documentation-that-describes-a-mechanism-nobody-built` — a MUST
  with no implementation point, which is this class in its normative form
