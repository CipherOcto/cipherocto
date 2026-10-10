---
name: 0009-e5-capability-signer-algorithm-binding
description: RFC-0009 AC#11/#13. Stop octo-cap-macaroon from stamping a caller-supplied algorithm onto a capability holder signature it did not make under, which silently downgrades any non-Ed25519 signer to Ed25519 at mint.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-10T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-e-signable-domain-separation
---

# Mission `0009-e5-capability-signer-algorithm-binding` — CLAIMED 2026-10-10

## Status

Claimed (2026-10-10). Surveyed against the source; the defect below is
demonstrated, not hypothesised. The RFC-0009 prefix migration for this trait is
deliberately **not** bundled with this mission — see §Why the prefix is a
separate mission.

## Dependencies

`0009-e` for the prefix substrate. None of the prior implementation missions
are prerequisites.

## §The defect: mint claims an algorithm the signer never named

`CapabilityToken::mint` and `attenuate_with_signer` both do this:

```rust
let sig_bytes = holder.sign(&msg)?;
let holder_sig = HolderSignature::new(
    crate::governance_signature::governance_algorithm_id(),  // always Ed25519
    &sig_bytes,
)?;
```

`CapabilitySigner::sign` returns `[u8; 64]` and names no algorithm. The mint
path then stamps `governance_algorithm_id()` — which is
`ed25519_algorithm_id()` — onto whatever came back. So the algorithm recorded
on the token is not the algorithm the signature was made under; it is a
constant chosen by the mint site.

`verify_holder_sig` dispatches on `self.holder_sig.algorithm_id()`, so it
verifies faithfully under the algorithm the token claims. The claim is what is
wrong.

**Demonstrated.** A scratch probe wrapping a `HolderSignature` stamped with
`AlgorithmId::derive("stark-schnorr")`, pushing it through exactly what
`serialize_wire` / `deserialize_wire` do (carry the 64 bytes, re-wrap on the
way back), and printing both ids before and after:

```
BEFORE algorithm_id = AlgorithmId { bytes: [130, 234, 241, ...] }
AFTER  algorithm_id = AlgorithmId { bytes: [202, 38, 148, ...] }
DOWNGRADED = true
```

That is the wire form, so the downgrade is not confined to the mint path — it
is what a persisted, relayed, or stored capability token does. `wire.rs`
documents the choice ("the algorithm is not on the wire ... a caller holding a
non-Ed25519 key re-stamps after parsing"), which is a fair thing to write if
the wire never carried anything else, but the consequence is that a token
produced under any other algorithm verifies as Ed25519 or not at all, and the
token itself carries no evidence of what it was.

`HolderSignature::new` is also not the only stamp site: `discharge.rs`, both
`wire.rs` parse paths, and three fixtures. Eleven `HolderSignature::new` sites
in the tree.

## §Severity, stated without inflation

**Latent, not an active forgery.** `CapabilitySigner::sign` returns
`[u8; 64]` and `public_key_bytes` returns `[u8; 32]`, so the trait cannot
currently represent a non-Ed25519 signer at all — a 33-byte STARK-Schnorr key
does not fit. Every implementor that matters signs Ed25519, so the stamped id
happens to be correct today.

The failure mode when a second algorithm does arrive is a **silent wrong
answer**, not a rejection: the token claims Ed25519, `verify_holder_sig`
dispatches to Ed25519, and a real STARK-Schnorr signature fails as though it
were forged. That is the same shape AC#11 exists to prevent, one layer up from
where the `octo-reputation` fix found it — a hardcoded Ed25519 property
surviving a dispatch refactor.

## Scope

1. **`CapabilitySigner` names its own algorithm.** Add
   `fn algorithm_id(&self) -> AlgorithmId` to the trait. The single production
   impl (`IdentityKey`) returns `self.algorithm_id()`; fixtures return
   `ed25519_algorithm_id()` or whatever they actually sign with.
2. **Mint stamps what the signer says**, not a constant.
   `attenuate_with_signer` likewise.
3. **`HolderSignature::new` stops being constructible with an unrelated
   algorithm.** Either take the algorithm from the signer at the only place a
   signature is produced, or assert the two agree at the boundary.
4. **The wire form carries the algorithm**, or the parse path takes it as a
   parameter from the resolved key context rather than inventing Ed25519.
   Which of the two is right is the open design question below.

## Non-goals

- Applying the RFC-0009 prefix to `holder_msg`. See §Why the prefix is a
  separate mission.
- Making a non-Ed25519 `CapabilitySigner` implementable. The `[u8; 64]` and
  `[u8; 32]` return types are a separate widening question; this mission only
  removes the lie in the algorithm field so that widening is not blocked by
  it.
- `octo-governance`'s same-named `CapabilitySigner`. Separate trait, separate
  crate, recorded as a gap in `0009-e`.

## §Why the prefix is a separate mission

It is tempting to answer this by prefixing `holder_msg`. That is a different
change with a different risk, and bundling them would hide both.

`holder_msg` is already length-prefixed per field
(`u32(len) ‖ root_id ‖ u32(len) ‖ caveats_wire`) precisely to prevent
concatenation collisions — the code documents that reasoning. So the
concatenation-ambiguity attack is already handled. What the prefix would add is
the **key_id**, which `holder_msg` has no field for, and the algorithm, which
this mission fixes a different way.

There is also a question this mission's evidence raises but does not answer:
whether a capability token _travels_ the way an attach-handle token does. The
attach-handle needed the prefix as a field because the verifier rebuilds the
message from the token's fields and has no out-of-band `key_id` source. The
capability wire format explicitly does **not** carry `holder_pub` or
`holder_did` — the caller resolves those out of band — which suggests the same
constraint applies and the prefix would need to be a field there too. That
would change the wire format and the three-segment shape, and it should be
decided on its own evidence.

## Test vectors

| Vector                                                 | What it pins                              |
| ------------------------------------------------------ | ----------------------------------------- |
| `mint_records_the_signers_own_algorithm`               | the token claims what the signer named    |
| `a_non_ed25519_survivor_is_not_downgraded_by_the_wire` | the round-trip does not silently re-stamp |
| `the_algorithm_survives_the_wire_round_trip`           | the field is transported, not re-invented |
| `mismatched_algorithm_and_signature_are_refused`       | a wrong pairing cannot be constructed     |

## Acceptance criteria

- [ ] `CapabilitySigner` carries `algorithm_id`, and no mint site calls
      `governance_algorithm_id()` to stamp a signature.
- [ ] `serialize_wire` / `deserialize_wire` preserve the holder signature's
      algorithm, or the parse path takes it as an explicit parameter and the
      wire does not invent one.
- [ ] Every vector above exists under its specified name and is
      mutation-checked.
- [ ] All eleven `HolderSignature::new` sites accounted for — production,
      fixture, or deliberately refusing.
- [ ] Clippy clean with `--all-targets --all-features` on `octo-cap-macaroon`,
      `octo-wallet` and `octo-cli`.

## Open question

**Does the capability wire form gain a fifth field, or does the parse path take
the algorithm as a parameter?** The wire already resolves `holder_did` and
`holder_pub` out of band, so a parameter is consistent with what the format
already does; a field is consistent with RFC-0009's "algorithm travels in band"
and with the attach-handle precedent. Both are defensible. Deciding this needs
evidence about who resolves the holder context today and whether that resolver
has the algorithm available, which has not been gathered. Not guessed.
