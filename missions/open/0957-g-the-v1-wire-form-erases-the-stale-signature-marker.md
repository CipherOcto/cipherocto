---
name: 0957-g-the-v1-wire-form-erases-the-stale-signature-marker
description: The v1 macaroon wire form has nowhere to put holder_sig_stale, so deserialize_wire hardcodes it to false. Every staleness guard in the token substrate therefore applies only to in-process tokens: a stale capability that arrives over the wire is silently reclassified as fresh, and the diagnosis is lost even though the signature still fails and the outcome stays fail-closed. Found while fixing unsigned wallet attenuation, where the real failure mode turned out to be cryptographic rather than the stale branch the guard was written for.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0957
    - 0957-phase2d-attenuation
    - 0957-a
---

# Mission `0957-g-the-v1-wire-form-erases-the-stale-signature-marker`

## Status

Open. Filed from the unsigned-attenuation fix in `octo-wallet-node`, where the
mechanism this mission is about turned out to be load-bearing for the diagnosis.

## The finding

`CapabilityToken` carries `holder_sig_stale: bool`. `CapabilityToken::attenuate`
sets it; `attenuate_with_signer` clears it. `verify_holder_sig` checks it first
and refuses a stale token outright, before any cryptography:

```rust
if self.holder_sig_stale {
    return Err(MintError::HolderSig("holder_sig_stale: ..."));
}
```

That guard was added deliberately — mission `0957-a`'s R6 fix, described as
surfacing stale state at the verify boundary "rather than letting it propagate".

**The wire form cannot carry the flag.** `deserialize_wire` constructs the token
with `holder_sig_stale: false` — hardcoded, not derived from anything in the
wire. The v1 form is three base64url segments (macaroon, signature, discharge)
and there is no segment or bit for it.

So the guard cannot fire on any token that arrived over the wire. It protects
in-process tokens only.

## Why this was not caught

An existing test in `octo-wallet-node` already _documented_ the erasure:

> Note: `holder_sig_stale` is NOT preserved across the wire roundtrip (the wire
> parser hardcodes `false` on deserialize — a known follow-on for the wire
> substrate; mission 0957-phase2d-1).

The follow-on mission it names was never filed. That is the same shape as the
unowned-deferral pattern elsewhere in this arc: a gap disclosed by the work that
created it, pointed at a mission that does not exist.

## The failure it actually produces

The real behaviour was measured while fixing unsigned attenuation, not inferred.
With the handler calling `attenuate` (no re-sign):

- The emitted signature covers the **pre**-attenuation caveat list.
- Attenuation appends a caveat, so the signature no longer matches.
- `holder_sig_stale` is true in memory, but the wire erases it.
- `verify_holder_sig` skips the stale branch and runs the real check.
- The real check fails, cryptographically, with no hint that staleness was the
  cause.

**Fail-closed, and no hole.** An unsigned or stale capability is still rejected —
the signature genuinely does not match. This is a diagnosis and
defense-in-depth problem, not a forgery problem. That distinction matters and is
easy to get backwards, so it is stated here rather than left implied.

What is lost:

1. The operator sees a generic signature failure for a token that is stale for a
   completely different reason. The two causes look identical at the boundary.
2. The R6 guard is dead weight on the production path. It is a check that only
   ever fires in tests and in-process code.
3. Any future guard conditioned on `holder_sig_stale` inherits the same blind
   spot without anyone deciding that.

## Acceptance Criteria

- [ ] The wire form carries staleness, **or** the substrate stops relying on a
      flag the wire cannot express and the stale state is derived at the verify
      boundary instead. Either is acceptable; a fourth segment and a hand-rolled
      bit in an existing segment are both heavier than they look.
- [ ] Whatever replaces the flag is exercised by a test that goes through
      `serialize_wire` → `deserialize_wire` and asserts the stale state survives.
      A test that constructs the token in memory does not count — that is the
      case that already works.
- [ ] `verify_holder_sig`'s doc comment stops claiming it surfaces stale state
      from a wire token, or becomes true. Today the comment and the wire disagree
      and only the comment says so.
- [ ] The `0957-phase2d-1` follow-on named in the existing test comment is either
      this mission or is deleted as a dangling reference.

## What must not happen

- **Do not fix this by making the erasure `true` on deserialize.** That is the
  dangerous direction: it would make every unsigned attenuation fail _for the
  stated reason_, which reads like an improvement, while breaking any legitimate
  token whose staleness was merely an artifact. The flag's meaning has to be
  decided, not defaulted.
- **Do not delete the R6 guard as "redundant".** It is not redundant for
  in-process tokens; it is unreachable only for wire tokens, which is a narrower
  claim than "redundant" and should not be paraphrased into one.
- **Do not write a test that asserts the current hardcoded `false` is correct.**
  That would pin the erasure as a contract, which is what this mission exists to
  resolve.

## Dependencies

- `0957-a` — the R6 stale-state guard being protected here.
- `0957-phase2d` — recorded the original wallet → attenuate → verify gap; now
  delivered in `octo-wallet-node`. This mission is what that fix's diagnosis
  turned up.
- `0009-cj` — the V2 bundle path, which has the same class of
  "documented contract the wire does not meet" problem in a different field.
