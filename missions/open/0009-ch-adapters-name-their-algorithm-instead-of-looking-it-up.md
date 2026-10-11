---
name: 0009-ch-adapters-name-their-algorithm-instead-of-looking-it-up
description: RFC-0009 AC#11 follow-on. All 24 adapter call sites pass ed25519_algorithm_id() to envelope verification because DeterministicEnvelope names no algorithm and no peer registry exists. Every site now declares the assumption and a guard fails if the debt grows; this mission retires the debt by implementing the peer credential lookup CD-6 D1 was amended to carry.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-ce-a-type-alias-hides-the-same-pin
---

# Mission `0009-ch-adapters-name-their-algorithm-instead-of-looking-it-up`

## Status

Open. The decision is taken; this is the implementation.

## The finding

Measured: **24 call sites across 20 platform adapters** pass
`ed25519_algorithm_id()` explicitly to envelope verification. **None passes
anything else.**

The cause is structural, not careless. `DeterministicEnvelope` carries
`source_peer` and `signature` and names no algorithm, and there is no peer
registry an adapter could consult. So the algorithm is chosen in advance.

CD-6's design names a peer _key_ registry and its acceptance criteria are all
about authentication; the word "algorithm" did not appear in the plan. CD-6 D1
has now been amended to carry an algorithm binding, per an operator decision
that preferred it over adding an algorithm field to the envelope.

## What already exists

- Every one of the 24 sites declares the assumption and names the work that
  would retire it.
- `every_adapter_naming_a_concrete_algorithm_declares_the_assumption` pins that
  declaration, with a measured floor of 24 sites.
- The same 24 sites were declared under CD-6 as owner, and CD-6 D1 now specifies
  the binding.

**The guard will fail when this lands.** That is deliberate: the debt being paid
should be visible rather than absorbed by a guard quietly relaxed to accommodate
it.

## Design, as amended into CD-6 D1

```text
struct PeerCredential {
    key: Vec<u8>,           // the peer's verifying key, any algorithm's width
    algorithm: AlgorithmId, // the algorithm that key speaks
}
```

Adapters pass `registry.credential(peer)?.algorithm` into `verify` instead of
the constant.

Three failure modes are stated explicitly, because each is a way this goes
wrong:

- **No wire change.** DOT/1 keeps its field set. This is why the registry route
  was chosen over an envelope field.
- **Fail closed on a missing entry.** A peer the registry does not know has no
  algorithm, so its frame is refused. It must **not** fall back to Ed25519 — a
  fallback would silently reinstate the assumption this replaces, and would do
  so silently.
- **`key` is bytes, not a 32-byte array.** A fixed-width field would recreate
  the same defect this binding exists to remove, one layer out.

## Acceptance Criteria

- [ ] A peer credential lookup exists and is reachable from every adapter's `canonicalize`.
- [ ] No adapter passes a hardcoded algorithm to envelope verification — grep: no `ed25519_algorithm_id()` at an adapter `canonicalize` call site.
- [ ] A peer absent from the registry has its frame refused, not checked as Ed25519 (negative test per adapter).
- [ ] The declaration comment is removed at each of the 24 sites, since it stops being true.
- [ ] `every_adapter_naming_a_concrete_algorithm_declares_the_assumption` passes with the floor lowered to zero, rather than deleted.

## Dependencies

- Mission `0009-ce-a-type-alias-hides-the-same-pin` (recorded the population)
- CD-6 D1 (amended with the algorithm binding)
- RFC-0009 AC#11
