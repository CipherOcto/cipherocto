---
name: marketplace-accepts-a-listing-whose-did-cannot-be-parsed
description: "Ask::new rejects only an empty asker_did, so any non-empty string can be listed. The DoS half is fixed — the async gate and latency read no longer propagate a DID-parse failure — but the identity policy is unresolved: a listing can carry a DID under which no reputation can ever be recorded, so it can never be reputation-gated. Separately, Ask::id's doc says it hashes canonical_ser(rates.rates) while the code uses serde_json over the whole ModelRateTable."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0010
    - RFC-0900
    - RFC-0968
---

# Mission `marketplace-accepts-a-listing-whose-did-cannot-be-parsed`

## Status

Partially actioned. The **denial-of-service is fixed**; the underlying
identity policy is an RFC-owner question and remains open.

## The defect that was fixed

`Ask::new` validates only that `asker_did` is non-empty:

```rust
if asker_did.is_empty() {
    return Err(AskError::EmptyAskerDid);
}
```

The canonical parser (`reputation_compat::parse_canonical_did`, per
RFC-0010) accepts exactly two shapes: `did:octo:z<base58btc>` and the
legacy `did:octo:b<52 base32>`. Anything else is
`RecorderDidMalformed`. Those two sets do not coincide, so a listing
could carry a DID that the reputation layer can never parse.

That was exploitable, because the async selection path parsed **every
candidate** to consult the circuit breaker and propagated the failure:

```rust
let excluded = if min > 0.0 {
    self.reputation_compat.is_excluded_async(&order.spec.asker_did).await?   // propagates
} else { false };
```

Measured: with `asker_did = "not-a-canonical-did"`, `Marketplace::put`
succeeded, `cheapest` returned that listing, and with the breaker enabled
`cheapest_with_ranking_async` returned
`Err(RecorderDidMalformed("compat requires did:octo:z<base58btc> or did:octo:b<52>"))`
— **for the entire model**, regardless of which provider was wanted. One
listing, supplied by any seller, denied the canonical selection path.
The sync path was unaffected, so the two surfaces disagreed on whether the
marketplace worked at all.

### Two parse points, not one

Patching only the gate would have left the path broken. The latency read
in the same loop parses too, so both were fixed:

| Site                  | Now returns for an unparseable DID  |
| --------------------- | ----------------------------------- |
| `is_excluded_async`   | `Ok(false)` — nothing to exclude    |
| `observed_latency_ms` | `Ok(None)` — no latency observation |

Both are semantically honest: a DID that cannot be parsed **has** no
recorded reputation, which is exactly the position of an unknown
canonical DID. The sync registry already treats unknown as un-excluded,
so this restores agreement rather than inventing a rule.

Both guards were mutation-verified **independently** — removing either one
alone reproduces the original `Err`. Enumerating the sites before patching
is what caught the second.

Strictness is preserved where it belongs: `record_outcome_async` still
rejects an unparseable DID, so reputation can never be _written_ under
one. Store faults still propagate, so a connectivity blip is never
misreported as "no data".

Pinned by `unparseable_listing_does_not_fail_the_async_selection_path` in
`tests/marketplace_reputation_async.rs`, which also asserts that async and
sync agree on the winner — not merely that both survive.

## What was tried and reverted: rejecting at `put`

The obvious fix — validate `asker_did` in `Marketplace::put` with a new
`RepoError::InvalidAskerDid` — **was implemented and then reverted.** It
broke `tests/eleven_step.rs`:

```text
put ask: InvalidAskerDid { asker_did: "did:octo:asker1",
                          reason: "unrecognized DID shape: expected did:octo: prefix" }
```

Three failures, including
`step10_settlement_hash_cross_impl_byte_equivalent`. That suite compares
settlement hashes against another implementation, and it uses short
literal DIDs deliberately.

That is the useful evidence: **the tree does not currently require
listings to carry RFC-0010 canonical DIDs**, and a cross-implementation
test vector depends on them not being so. Imposing canonical form at the
listing boundary is therefore a policy change with test-vector fallout,
not a bug fix — which is exactly the distinction this mission records.

## What remains open

With the DoS fixed, a listing can still carry a DID that no reputation
record can ever exist under. That has a stable consequence:

> Such a provider is **permanently reputation-invisible**. Its circuit
> breaker can never exclude it, on either path, because there is no
> aggregate to read. It is always treated as "unknown", and unknown is
> not excluded.

That is the same shape as the zero-stake finding in mission
`a-zero-stake-provider-can-never-be-banned` — a class of provider the
mechanism can never act on — but reached by identity rather than by
collateral.

It is consistent, it is what the sync path has always done, and it is not
a bug in itself. It is a policy question.

## Also found: `Ask::id` hashes something other than its doc says

The doc states:

> `AskId = BLAKE3(canonical_ser(asker_did || model || axes_hash || nonce))`
> where `axes_hash = BLAKE3(canonical_ser(rates.rates))`

The code computes:

```rust
let rates_canonical = serde_json::to_vec(&self.rates).expect("serializable");
let axes_hash = blake3::hash(&rates_canonical);
```

Two divergences:

1. **`serde_json`, not `canonical_ser`.** The doc specifies the canonical
   encoder; the code uses a general-purpose JSON serializer. For a struct
   with named fields this is deterministic for a given type version, so it
   is not a collision risk today — but it is not the encoding the doc
   claims, and it would silently change if the struct's field order or
   representation changed.
2. **`rates` vs `rates.rates`.** The doc hashes the rate _vector_; the
   code hashes the whole `ModelRateTable`, which also contains
   `rates.model`.

Consequence of (2): `AskId` is not a pure function of priced content.
Two Asks with identical rates but different `rates.model` get different
ids. That matters because `rates.model` is itself never validated against
`Ask.model` — nothing in the tree compares them, and `settlement_cost`
uses only the axis rates, so `rates.model` has no pricing effect at all.
It is unvalidated, unread metadata that nonetheless participates in a
content-addressable identifier.

Not actioned: correcting the hash would change every existing `AskId`,
invalidating stored rows and any TV vector. Correcting the _doc_ is safe
but then describes `serde_json`, which may itself be the thing that
should change. That is an owner call.

## What the RFC owner needs to decide

- Should `Marketplace::put` require a canonical `asker_did`? The evidence
  against is `tests/eleven_step.rs`, which would need regenerating. The
  evidence for is that a provider with an unparseable DID can never be
  reputation-gated by any mechanism.
- If listings need not be canonical, should the marketplace surface it —
  e.g. a listing flagged `reputation_trackable: false`, or a warning
  telemetry event when an ask is accepted whose DID cannot be parsed?
- Should an unparseable listing be selectable at all, or should it be
  excluded from ranking on _both_ paths (fail closed) rather than treated
  as unknown-and-therefore-eligible?
- Should `Ask::id` hash `canonical_ser(rates.rates)` as documented, or
  should the doc be corrected to match `serde_json` over the whole table?
  Changing the hash invalidates stored ids and any TV vector.
- Should `rates.model` be validated against `Ask.model`, or removed as
  unread metadata?

## Reproduction

```bash
# only emptiness is checked
grep -n -A6 'if asker_did.is_empty()' crates/quota-router-storage/src/ask.rs

# the two parse points now guarded
grep -n -B2 -A2 'parse_canonical_did(did).is_err()\|parse_canonical_did(asker_did).is_err()' \
  crates/quota-router-core/src/marketplace/reputation_compat.rs

# write path still strict
grep -n 'parse_canonical_did(asker_did)?' crates/quota-router-core/src/marketplace/reputation_compat.rs

# the id divergence
grep -n -A4 'let rates_canonical' crates/quota-router-storage/src/ask.rs

# the DoS, as pinned
cargo test -p quota-router-core --test marketplace_reputation_async \
  unparseable_listing -- --nocapture
```

## Cross-references

- RFC-0010 §Data Structures — `CanonicalCodec`, and the dual-parse window
  that accepts `did:octo:b<52>` alongside the canonical `z` form
- RFC-0968 — the canonical reputation store whose parse rules these follow
- Mission `a-zero-stake-provider-can-never-be-banned` — the same
  never-actionable-provider shape, reached by collateral rather than
  identity
- Mission `the-marketplace-facade-has-no-production-caller` — why nothing
  shipping is affected today
- Mission `marketplace-record-outcome-surfaces-require-a-dual-write` — the
  other gap where a surface can be written but not read back
