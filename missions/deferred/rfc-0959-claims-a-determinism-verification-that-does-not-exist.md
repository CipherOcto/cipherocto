---
name: rfc-0959-claims-a-determinism-verification-that-does-not-exist
description: "RFC-0959 section Determinism violations ends its only bullet with a completed verification: Verified by property test, 10K random (ask, axes_consumed) pairs replay identically across 2 nodes. No such test exists. The nearest candidate in the tree, settlement_hash_property_10k_replays, is a real ten-thousand-iteration test but takes SettlementEvent values rather than Ask pairs and recomputes twice inside a single process, so it is false in three specifics while reading as a completed check in the Security section. Two smaller findings in the same section: the adversary table's cost-to-attacker for A1 is computed against HMAC-BLAKE3, a construction the design explicitly rejects in favour of BLAKE3 keyed mode, and the work factor is stated per bit rather than per collision; and collision resistance is expressed three ways, as the probability 2^-128, the work factor 2^128, and the output width 256-bit collision resistance, with the probability form propagated into the cache_key module docs. Two claims checked out and are recorded as such: A5's circuit-breaker threshold of 50 unique keys over a 1000-call window matches the RFC exactly, and the breaker's f64 threshold does not threaten the Class-A determinism invariant because the rate is one correctly-rounded division of exact integers."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959
    - RFC-0008
    - RFC-0853
---

# Mission `rfc-0959-claims-a-determinism-verification-that-does-not-exist`

## Status

Deferred. All four findings are documentation defects in an accepted RFC plus
one doc comment in the tree; none is a code change, and none is mine to make.
One probe was run and reverted.

Fifth systematic sweep of a structure RFC-0959 points at, after the roles
table, the acceptance gates, the implementation assumptions and the test
vectors.

## Finding

§Determinism violations is four lines long. Three of them state prohibitions.
The fourth states a verification that has already happened:

> **Floating-point settlement arithmetic. PROHIBITED.** Per RFC-0008 Class A
> requirement. Verified by property test: 10K random `(ask, axes_consumed)`
> pairs replay identically across 2 nodes.

**No such test exists.** This is the only place in RFC-0959 where the Security
section asserts a completed check with a method and a sample size attached, and
it is the one assertion in the document that is not true.

## What exists instead

`settlement_hash_property_10k_replays` in `quota-router-storage`'s `ask` module
is the nearest thing, and it deserves credit: it runs **10,000 real
iterations**, builds a fresh `SettlementEvent` each pass from a seeded xorshift
generator, and asserts two independent recomputations agree. It is not a
10K-iteration test in name only, and it is the strongest test in this part of
the tree.

It still is not what the RFC claims, on three counts:

| RFC claims                   | The test does                                                                    |
| ---------------------------- | -------------------------------------------------------------------------------- |
| `(ask, axes_consumed)` pairs | `SettlementEvent` values — no `Ask` is constructed                               |
| replay across **2 nodes**    | recomputes twice in **one process**                                              |
| `(ask, axes_consumed)`       | `cap_root_hash`, `ask_id`, `invocation_hash` are seeded too, so it is a superset |

The second is the substantive gap. Same-process determinism cannot catch a
serialisation, endianness or platform difference between two nodes, which is
precisely the failure the sentence is claiming to have ruled out. A test that
passed on one machine says nothing about a second machine, so "across 2 nodes" is
doing all the work in that sentence and none of it is being done.

## A1 prices an attack against a construction the design rejected

The adversary table's cost-to-attacker cell for A1 — the row covering
`settlement_hash` binding `cap_root_hash`, rated LOW and ACCEPTED — reads:

> Forge HMAC-BLAKE3 chain: 2^128 work per bit

`HMAC` appears **exactly once in the entire RFC**, and it is here. Everywhere
else the document specifies BLAKE3 keyed-hash mode per RFC-0853, and the
implementation says so in as many words:

> BLAKE3's native keyed-hash mode is used (**NOT HMAC-BLAKE3**) — matches the
> convention established by mission 0957-a

So the Q2 "cost to attacker" for a security decision is computed against a
construction the design deliberately chose not to use. The two are not
interchangeable in the analysis: native keyed mode and the HMAC construction
have different length-extension and key-recovery stories, and an adversary
analysis that reasons about the wrong one cannot be inherited by the right one.

"2^128 work per bit" is separately wrong in its unit. BLAKE3's 2^128 is the
work to find a **collision**, in total. There is no per-bit quantity here; the
phrasing belongs to linear cryptanalysis, which keyed BLAKE3 does not invite.

## Collision resistance, three ways

The same document expresses one fact in three incompatible-looking ways:

- §Design Goals G2 — "256-bit BLAKE3 output; **collision risk 2^-128**" (correct,
  and correctly phrased as a probability)
- §Security / Consensus attacks — "BLAKE3 256-bit, **2^-128 collision
  resistance**. ACCEPTED" (a probability labelled as a resistance)
- §Adversary A2 — Q2 "**2^128 for collision**", Q5 "ACCEPTED — **256-bit
  collision resistance**" (a work factor, then an output width)

The numbers are the same fact; the _labels_ are not. "256-bit collision
resistance" in A2's residual-risk cell is the one that would mislead a reader —
256 bits is the **output width**. The collision resistance of a 256-bit hash is
2^128. A reader who takes A2's Q5 literally believes the ask-identity collision
resistance is 2^256.

The probability form propagated into the code, where the same module states
"BLAKE3 collision resistance 2^-128" about `cache_key`. Same number, same
mislabel, now in a source file.

To be clear about scale: none of this is a vulnerability, and none of it
changes a decision. It is a security-analysis section whose prose does not
match the construction it analyses, in a document that is otherwise unusually
careful about saying which of its claims are verified.

## Two claims that hold, recorded so they are not re-raised

**A5's circuit-breaker threshold is exactly right.** The adversary row specifies
"MIN_PROMPT_DIVERSITY = 50 unique BLAKE3 keys over last 1K calls". The code has
`MIN_PROMPT_DIVERSITY: usize = 50` and `WINDOW_SIZE: usize = 1_000`, and the
trip condition is `unique < MIN_PROMPT_DIVERSITY` under a full window. Values,
comparison direction and window size all match.

**The `f64` in the breaker is not a determinism hazard.** §Determinism
violations prohibits floating-point _settlement_ arithmetic, and G4 demands "no
float anywhere" scoped to the settlement path. `circuit_breaker` uses `f64` for
its threshold, which looked like a violation until the expression was read:

```rust
pub fn cache_hit_rate(&self) -> f64 {
    let hits = self.cache_hits.iter().filter(|&&h| h).count();
    hits as f64 / self.cache_hits.len() as f64
}
```

One division of two exactly-representable integers, compared against a fixed
literal. IEEE-754 requires correctly-rounded division, so this is
bit-identical across conforming targets. It cannot introduce the
platform-dependent settlement divergence the prohibition exists to prevent, and
the breaker is advisory-only, so its output cannot mutate a settled
`axes_consumed`.

## What the RFC owner should do

- **Delete or correct the "Verified by property test" sentence.** Either the
  test is written, or the sentence is not true. It currently reads as a check
  that was performed.
- **If cross-node determinism is meant to be verified,** the cheapest honest
  form is a fixture: one canonical event, its expected digest, run by two
  processes. That is also what the missing TV3 fixture would have been.
- **Rewrite A1's Q2 cell** against BLAKE3 keyed mode, and drop "per bit".
- **Fix A2's Q5** to say 2^128 collision resistance over a 256-bit output.
- **Align the two `collision resistance` labels** in §Consensus attacks and in
  the `cache_key` module docs with G2's phrasing, which is the correct one.

## Reproduction

```bash
# the claim, and the nearest real test
grep -n 'Verified by property test' \
  rfcs/accepted/economics/0959-ask-settlement-chain.md
grep -n 'fn settlement_hash_property_10k_replays' -A 4 \
  crates/quota-router-storage/src/ask.rs

# no Ask-level 10K replay test, and no two-node determinism test
grep -rn 'fn .*propert' --include=*.rs crates/quota-router-storage/src/ask.rs
grep -rniE 'two_node|across_2_nodes|cross_node' --include=*.rs crates/ \
  | grep -i 'market\|settle\|ask' || echo "no cross-node settlement determinism test"

# HMAC appears once in the RFC, against a construction the code rejects
grep -n 'HMAC' rfcs/accepted/economics/0959-ask-settlement-chain.md
grep -n 'HMAC' crates/quota-router-storage/src/cache_key.rs

# the three phrasings of one fact
grep -noE '2\^-128[^|]*|2\^128[^|]*|256-bit collision resistance[^|]*' \
  rfcs/accepted/economics/0959-ask-settlement-chain.md

# A5's threshold matches exactly
grep -n 'MIN_PROMPT_DIVERSITY\|WINDOW_SIZE\|CACHE_HIT_RATE_TRIP' \
  crates/quota-router-storage/src/circuit_breaker.rs

# and the float is a single correctly-rounded division
grep -n 'fn cache_hit_rate' -A 4 crates/quota-router-storage/src/circuit_breaker.rs
```

## Cross-references

- RFC-0959 §Determinism violations — the false verification claim
- RFC-0959 §Adversary Analysis — the A1 cost-to-attacker cell
- Mission `four-of-rfc-0959s-six-acceptance-gates-are-not-met` — G4 passes as
  literally scoped to `ask.rs`; the only float in this surface is outside it,
  in the breaker, and is deterministic
- Mission `the-cache-stuffing-defense-is-absent-from-every-live-pricing-path` —
  A5's HIGH-rated defence exists as a mechanism and is wired into no live
  pricing path, which is why the severity table's "CRITICAL: None identified"
  is the place this surfaces and does not
- Mission `rfc-0959-test-vectors-contain-no-expected-values` — the same pattern
  one section earlier: a verification named, never run
- Mission `four-of-rfc-0959s-ten-implementation-assumptions-fail` — IA-5, the
  same `canonical_ser` the A2 row's BLAKE3(canonical_ser) input depends on
