---
name: the-settlement-path-records-but-never-settles
description: "Synthesis of four measured findings on the live settlement verification path, plus the severity calibration that keeps them honest: the settlement path computes and records a cost but never moves tokens. produce_burn and produce_settlement both exist with zero callers, and nothing in ask.rs or the marketplace touches a balance. So nothing is mis-billed today, and every defect here is inherited by whatever moves money first. Also records the ordering constraint: writing the audit trail before the checks run would canonize unchecked settlements."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.1"
  depends_on:
    - RFC-0959
    - RFC-0900
---

# Mission `the-settlement-path-records-but-never-settles`

## Status

Deferred — this is a synthesis, not a new defect. It exists so the four
settlement findings can be read as one state of the system rather than
four separate alarms, and so their real severity is stated rather than
inferred. No code changed, no measurement probes left behind.

**v1.1** records why the calibration is structural: the served router
has no dependency edge to the marketplace at all, while the CLI has the
edge and makes no marketplace calls. The v1.0 wording left that open to
misreading.

## Why this document exists

Four findings were recorded independently, each measured, each on the
same live path. Read alone, each is a defect report. Read together they
describe a system that computes a settlement, verifies it with three
checks that do not hold, and writes down nothing about it.

That is the finding. This document is the map.

## The composed picture

| Step                    | Implemented?   | Enforced on the live path?                 | Detail                                                       |
| ----------------------- | -------------- | ------------------------------------------ | ------------------------------------------------------------ |
| Compute cost            | Yes            | Yes, with an RFC deviation                 | see below                                                    |
| Amount bound by hash    | **No**         | No, and it is the only integrity mechanism | `settlement-hash-does-not-bind-the-cost`                     |
| Replay detection        | Yes            | **Vacuous by default**                     | `replay-defense-is-a-no-op-in-the-default-cli-configuration` |
| Vault row + chain match | Yes, correctly | **Never called**                           | `the-cross-chain-settlement-reject-is-never-called`          |
| Event trail written     | Yes            | **Never called**                           | `the-settlement-audit-trail-is-never-written`                |

Plus one conformance deviation rather than a hole: `compute_cost` falls
back to the registry default where RFC-0959 requires `UnknownAxis`, in
`ask-axis-ids-are-never-validated-against-the-axis-registry`.

The end state, measured end to end: a settlement can be verified with an
arbitrary amount, replayed any number of times in the default
configuration, drawn against a vault that does not exist on a chain that
never was — and leave behind a single row in a replay index.

## The severity calibration: nothing is billed today

This is the part that keeps the four findings from being read as either
an active exploit or a non-issue.

**The settlement path never moves tokens.** Verified, not inferred:

- `grep` for `balance` / `transfer` / `debit` / `credit` in the settlement
  code (`ask.rs`) returns nothing outside doc comments. It computes a
  cost, hashes a record, and writes a nonce.
- `produce_burn` (octo-policy) is re-exported from `lib.rs` and has **no
  production caller**.
- `produce_settlement` (quota-router-sm-engine) has **no caller at all**,
  not even a re-export.
- The marketplace never calls into token movement; it references vault
  concepts only in field names and documentation.

So the defects above are defects in the **record** and its
**verification**, not in a live money-movement path. No user is being
overcharged by any of them right now, and that should be said plainly.

## Why it is still material, and not deferred-and-forgotten

RFC-0959 makes the settlement chain the accounting authority: it is an
_independent_ settlement chain that coexists with RFC-0909 rather than
amending it, and it is the record the economic model rests on. The
`settlement_events` schema already carries `cap_root_hash`,
`invocation_hash`, `cost_micro_octo_w` and `router_signature` — the
fields a ledger would need.

A ledger built on top of this path inherits every one of the four
defects at once, and inherits them _silently_, because all three checks
return success. The expensive failure mode is not that someone notices
these are wrong later; it is that nobody does, because the code says OK.

That is the argument for fixing the order before fixing the individual
items.

## The ordering constraint

The obvious sequencing is wrong. Writing the audit trail first would
make `settle-list` meaningful — and would canonise a settlement record
for something that has not been checked.

Recommended order, which is a proposal rather than a decision:

1. **Bind the amount to the hash.** Nothing else is safe to record until
   the record commits to what it claims. This is Layer A and invalidates
   stored hashes, so it needs owner sign-off and should land first.
2. **Wire the vault/chain check.** It is already written and tested;
   this is plumbing plus a decision about where the vault substrate comes
   from at settlement time.
3. **Make the replay check real by default**, or make the CLI refuse to
   return a verdict without a persistent store.
4. **Append the event** — last, once 1–3 hold.
5. **Only then** consider the money-movement side (`produce_settlement`,
   the vault draw), which is currently unwired in both directions.

Reversing 1 and 4 produces a canonical record of an unverified
settlement, which is worse than no record: it gives the defect somewhere
authoritative to live.

## The calibration is structural, not accidental

Worth stating precisely, because the dependency graph invites the wrong
conclusion:

- `octo-network` — which provides `QuotaRouterNode`, the type the
  network-exposed `quota-router serve` mode runs — declares **no
  dependency at all** on `quota-router-core` or `quota-router-storage`.
  The served router cannot reach the marketplace or the settlement
  crates; there is no edge to traverse.
- `quota-router-cli` **does** declare a dependency on both. So the
  dependency edge exists and the calls do not: `grep` for
  `Marketplace`, `cheapest`, `scoring` and `ProviderReputation` across
  `crates/quota-router-cli/src/` returns nothing.

That second point is a trap for the next auditor. Seeing
`quota-router-core` in the CLI's `Cargo.toml` is easy to read as "the
marketplace is wired into the CLI". It is not. The CLI reaches
`quota-router-storage` for settlement — which is exactly why the
settlement findings in this audit are live — and reaches
`quota-router-core` for quota and pricing, not for the marketplace.

So the split observed across these findings is structural: the
settlement record is reachable from a command line, and the marketplace
is not reachable from anywhere.

## What the RFC owner needs to decide

- Is this ordering right, or does the owner intend the event trail first?
- Which of the four is the gate for shipping a settlement that moves
  money? Arguably all four, since each independently permits a
  settlement that should not have succeeded.
- Should the verification surface fail closed when it cannot check
  something, rather than reporting success? Every finding here shares
  the shape "cannot check, reports OK".
- Does RFC-0959 need an event-sourcing section at all? Four code
  locations cited one that does not exist, repointed in the audit commit.

## Reproduction

The calibration claims, all verifiable. **The greps below are expected
to produce no output** — a non-zero exit from `grep` is the passing
result, not breakage:

```bash
# the settlement code computes and records, never moves tokens
grep -nE 'balance|transfer|debit|credit' crates/quota-router-storage/src/ask.rs \
  | grep -vE '///|//!'          # no matches

# the money-movement producers have no callers
grep -rn 'produce_burn' --include=*.rs crates/ | grep -v event_log_producer.rs
grep -rn 'produce_settlement' --include=*.rs crates/ \
  | grep -v 'quota-router-sm-engine/src/event_log_producer.rs'

# the marketplace never reaches token movement
grep -rn 'produce_burn|produce_settlement|TransferEventLog' \
  --include=*.rs crates/quota-router-core/src/marketplace/

# the structural split (v1.1) — no dependency edge at all
grep -c 'quota-router' crates/octo-network/Cargo.toml

# ...but the CLI has the edge and makes no marketplace calls
grep -n 'quota-router-core\|quota-router-storage' crates/quota-router-cli/Cargo.toml
grep -rn 'Marketplace|cheapest|scoring|ProviderReputation' \
  --include=*.rs crates/quota-router-cli/src/

# the four findings' own reproduction blocks
# missions/deferred/settlement-hash-does-not-bind-the-cost.md
# missions/deferred/replay-defense-is-a-no-op-in-the-default-cli-configuration.md
# missions/deferred/the-cross-chain-settlement-reject-is-never-called.md
# missions/deferred/the-settlement-audit-trail-is-never-written.md
```

## Cross-references

- Mission `settlement-hash-does-not-bind-the-cost` — step 1 above
- Mission `the-cross-chain-settlement-reject-is-never-called` — step 2
- Mission `replay-defense-is-a-no-op-in-the-default-cli-configuration` —
  step 3
- Mission `the-settlement-audit-trail-is-never-written` — step 4
- Mission `ask-axis-ids-are-never-validated-against-the-axis-registry` —
  the conformance deviation in the cost computation
- Mission `marketplace-advertised-price-ignores-default-rate-fallback` —
  the same `cost` value, wrong for a different reason
- Mission `marketplace-escrow-is-not-wired-into-the-marketplace-facade` —
  the dispute path that would catch an inflated amount is also unwired
- Mission `the-marketplace-facade-has-no-production-caller` — the
  standing caveat for the marketplace side of the same system
