---
name: the-settlement-hash-that-binds-cost-is-built-and-not-on-the-live-path
description: "RFC-0959-v28 specifies a ten-field settlement body hash that binds cost, ledger_height, governance_pubkey, nonce and two vault identities, and it is implemented correctly in quota-router-sm-engine with a green determinism test. The live quota-router settle path does not use it, because the CLI crate does not depend on sm-engine at all. Two other functions also compute something called the settlement hash: ask::compute_settlement_hash, which omits cost, and SettlementEnvelope::compute_settlement_hash, which the CLI does use and which omits cost, nonce-as-an-input is absent entirely, and carries no domain separator where the other two have one. A fourth construction lives in anti_fraud under a comment claiming it mirrors the envelope, and it does not: different field set, unkeyed BLAKE3, no domain separator, and it backs a settled-event invariance test that therefore cannot catch a production divergence. This extends the existing settlement-hash mission, which knew two of the four surfaces."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959
    - RFC-0959-v28
    - RFC-0960
---

# Mission `the-settlement-hash-that-binds-cost-is-built-and-not-on-the-live-path`

## Status

Deferred. Consolidating the surfaces into one canonical hash is a Layer A
content-address decision with an existing migration queued, and the CLI
dependency edge is an architecture call. Nothing was actioned.

## Why this is a new mission and not a fourth surface of an old one

`settlement-hash-does-not-bind-the-cost` (v1.1) is careful and correct about
what it found: a live `SettlementEnvelope::compute_settlement_hash` that omits
`cost`, and a latent `ask::compute_settlement_hash` that omits it too, with the
signature binding the amount as the only thing that catches tampering.

It does not know a third surface exists. That surface is the one that gets the
problem right, and it changes what the finding means.

## Surface C — the v28 implementation, which is correct

`quota-router-sm-engine`'s `settlement_event` module implements RFC-0959-v28's
`compute_settlement_body_hash`, and it is complete. Ten fields, including
everything the earlier surfaces drop:

```rust
buf.extend_from_slice(BODY_HASH_DOMAIN);
buf.extend_from_slice(settlement.settlement_id.0.as_ref());
buf.extend_from_slice(settlement.ask_id.0.as_ref());
buf.extend_from_slice(settlement.cost_vault_id.as_bytes());
buf.extend_from_slice(settlement.cost_asset_id.as_bytes());
buf.push(settlement.asset_kind as u8);
buf.push(settlement.cost.scale);
buf.extend_from_slice(&settlement.cost.value.to_le_bytes());
buf.extend_from_slice(&settlement.ledger_height.to_le_bytes());
buf.extend_from_slice(settlement.evidence_ref.0.as_ref());
buf.extend_from_slice(&settlement.governance_signature.as_bytes()[..32]);
buf.extend_from_slice(settlement.nonce.as_bytes());
```

`cost` is bound with its **scale** as a separate byte, which is stronger than
the RFC's pseudocode — it prevents a value from being reinterpreted at a
different scale, which is precisely the confusion RFC-0105 exists to prevent.
There is a domain separator, and a determinism test.

So the finding is not "nobody worked out how to bind cost into the settlement
hash." Somebody did, in the amendment, with the field-set factored out so
`new()` and `validate()` cannot drift — the exact hazard v28 §2.1 warns about in
a comment.

## And it is not reachable from the live path

`quota-router-cli` does not depend on `quota-router-sm-engine`. The dependency
grep is unambiguous:

```text
crates/octo-settlement/Cargo.toml          depends on sm-engine
crates/quota-router-core/Cargo.toml         depends on sm-engine
crates/quota-router-cli/Cargo.toml          does not
```

The two surfaces that the `settle` and `settle-replay` commands actually use
are Surfaces A and B, neither of which binds cost. **The correct implementation
is three files and one dependency edge away from the live path**, and the edge
has never been drawn.

That reframes the severity of the existing mission. Its recommendation reads as
"bind cost into the hash, and this needs an RFC decision." The truer statement
is: bind cost into the hash by routing settlement through the surface that
already does, and the remaining question is only whether the CLI should gain
that dependency or the hash should be relocated into the crate both can reach.

## Four constructions, three domain-separator policies

| Surface | Location                                         | Binds `cost`        | Domain separator             | Live       |
| ------- | ------------------------------------------------ | ------------------- | ---------------------------- | ---------- |
| A       | `ask::compute_settlement_hash`                   | no                  | `cipherocto/settlement/v1\n` | no         |
| B       | `SettlementEnvelope::compute_settlement_hash`    | no                  | **none**                     | **yes**    |
| C       | `sm_engine::compute_settlement_body_hash`        | **yes, with scale** | `BODY_HASH_DOMAIN`           | no         |
| D       | `anti_fraud`'s private `compute_settlement_hash` | no                  | **none**, unkeyed            | tests only |

Three of the four are named "the settlement hash" and none of them agrees with
another. Surface B is the one the CLI uses and the only one with no domain
prefix at all — so a value hashed under B is not distinguishable from one hashed
under D by anything but its contents, and neither is domain-separated from any
future settlement digest.

## Surface D's comment is false

`anti_fraud` carries a private helper introduced with:

> Mirrors `SettlementEnvelope::compute_settlement_hash` for the settled-event
> invariance test. Standalone BLAKE3 over `(model || axes || ask_id ||
invocation)`.

It does not mirror it. Surface B's preimage is
`model || axes || ask_id || nonce || timestamp || cost_vault_id || chain_id`,
with the last two Option-encoded behind presence tags. The helper's fourth
field is `invocation`, a 32-byte value where B has a 16-byte nonce, an 8-byte
timestamp and two tagged optionals. It uses `blake3::Hasher::new()` where both
A and C use a domain separator.

The cost is not only that the comment lies. The helper backs the
settled-event invariance assertion — the check that a settled event's hash does
not change — so **the test proving invariance is computed over a different
function than production uses.** It cannot fail on a production divergence,
because it never calls production.

This is the same pattern as `tv_0959_settlement_wire.rs`: a helper or fixture
whose name or comment asserts it covers the production canonicalisation, and
which does not.

## Two errors of mine, both from searching less than I thought

While establishing the above I asserted `compute_settlement_body_hash` did not
exist, from

```bash
grep -rn 'compute_settlement_body_hash\|body_hash' --include=*.rs crates/ | head -15
```

The function is in the tree. `head -15` cut the list before it reached
`quota-router-sm-engine`, and I read absence out of a truncated result. Earlier
in the same session I made the same class of error in the other direction —
searching only `crates/` and concluding `octo-sync` did not exist, which
produced a wrong verdict in an IA-4 row until I checked `Cargo.toml`.

Neither cost anything beyond a wrong sentence, because both were caught before
being committed. But the rule is the one this audit keeps relearning: **a
negative claim needs a complete search, and `head` on a grep makes a search
incomplete.** Absence of evidence from a bounded query is not evidence of
absence.

## What the RFC owner should do

- **Decide whether the CLI should depend on sm-engine, or the body hash should
  move down to a crate both can reach.** Leaving two settlement digests in two
  crates is the status quo this audit keeps finding.
- **Re-point `settlement-hash-does-not-bind-the-cost` at Surface C.** Its
  recommendation to "bind cost into the hash" is already implemented; what it
  should say is that the implementation exists and is not wired.
- **Fix or delete Surface D.** It is a fourth preimage whose comment claims
  equivalence it does not have, and it stands in for production in an
  invariance test.
- **Give Surface B a domain separator**, or retire it in favour of A or C.
- **Give the four surfaces distinct names.** Three functions called
  `compute_settlement_hash` with different preimages is the root cause of every
  reading error in this area, including mine.

## Reproduction

```bash
# four constructions, and the domain-separator policy of each
grep -rn 'pub struct SettlementEvent' --include=*.rs crates/
grep -rn 'fn compute_settlement_body_hash\|fn compute_settlement_hash' --include=*.rs crates/
grep -n 'SETTLEMENT_HASH_DOMAIN\|BODY_HASH_DOMAIN' -r --include=*.rs crates/

# Surface B has no domain separator; read its preimage in full
grep -n 'pub fn compute_settlement_hash' -A 30 \
  crates/quota-router-storage/src/ask.rs

# Surface C binds cost and scale
grep -n 'pub fn compute_settlement_body_hash' -A 16 \
  crates/quota-router-sm-engine/src/settlement_event.rs

# Surface D's false mirror
grep -n 'Mirrors .SettlementEnvelope::compute_settlement_hash' -A 12 \
  crates/quota-router-storage/src/anti_fraud.rs

# the live CLI does not reach the correct surface
grep -n 'quota-router-sm-engine' crates/quota-router-cli/Cargo.toml \
  || echo "cli does not depend on sm-engine"
grep -rln 'quota-router-sm-engine' --include=Cargo.toml crates/

# and the live CLI does reach Surface B
grep -n 'compute_settlement_hash' crates/quota-router-cli/src/commands.rs
```

## Cross-references

- Mission `settlement-hash-does-not-bind-the-cost` — knew Surfaces A and B;
  this adds C and D, and C changes the recommendation
- Mission `the-settlement-receipt-has-no-receipt-id-and-two-disjoint-stacks-exist`
  — the same two-stack shape one layer up
- Mission `rfc-0959-test-vectors-contain-no-expected-values` — v28 §8 repeats
  the base RFC's pending-test-vectors pattern in the amendment
- Mission `the-same-ask-has-two-different-content-addresses` — content
  addresses diverging across surfaces, the same failure on the Ask side
- Mission `0959-d-market-delivery-roll-up-is-green-on-work-measurement-contradicts`
  — an open mission already recording a GREEN claim that measurement contradicts;
  the v28 surface is the clearest case in the tree of a specification being
  honoured in one crate and not in another
