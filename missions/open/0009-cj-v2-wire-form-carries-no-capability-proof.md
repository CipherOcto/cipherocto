---
name: 0009-cj-v2-wire-form-carries-no-capability-proof
description: CapabilityBundleV2 as produced by both mint sites contains no macaroon, no signature and no caveat set. Its holder_record_bytes field is documented as a serialized HolderRecord that must be decoded via HolderRecord::canonical_de, but both producers write raw non-JSON bytes, and the documented decode fails on every bundle production emits. The wire form that replaced the capability on the response path carries no proof material.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - RFC-0957
    - 0009-cf-capability-chain-parent-binding-is-not-enforced
---

# Mission `0009-cj-v2-wire-form-carries-no-capability-proof`

## Status

Open. Filed from measured evidence, not from a failing test.

## The finding

RFC-0957 makes the macaroon the capability: bearer authorization proved by
replaying the macaroon bytes against a root secret. `CapabilityBundleV2` was
introduced to be the wire form that carries that capability across the network.

It does not carry it. Measured against the struct and both production producers:

| What a capability needs                    | In `CapabilityBundleV2`              |
| ------------------------------------------ | ------------------------------------ |
| macaroon bytes (caveat chain, root secret) | **absent**                           |
| holder signature                           | **absent**                           |
| caveat set                                 | **absent**                           |
| `channel_id` (macaroon root id)            | present — an identifier, not a proof |
| 32 bytes of `capability_id`                | present — an identifier, not a proof |

The two fields that survive are identifiers. Nothing in the bundle can be
presented to a verifier or checked by one.

### The sharpest instance: a field whose documented decode fails on all real data

`CapabilityBundleV2::holder_record_bytes` is documented twice, in the struct and
on the constructor:

> Serialized bytes of the `HolderRecord`. Deserialize via
> `HolderRecord::canonical_de` at the boundary.

> `holder_record_bytes` MUST be the serialized form of a `HolderRecord`.

`HolderRecord::canonical_de` is `serde_json::from_slice`, so the contract is
canonical JSON. **Both production producers write raw bytes instead:**

| Producer                            | What it writes                                             |
| ----------------------------------- | ---------------------------------------------------------- |
| `octo-wallet-node` mint handler     | 32 raw bytes of `compute_capability_id(&token.macaroon)`   |
| `octo-capability-issuer-node` issue | `req.capability.to_vec()` — the **capability root secret** |

The two producers do not even agree with each other about what the field holds.

Verified empirically, not by reading: feeding each producer's actual byte shape
to `HolderRecord::canonical_de` fails with `expected value at line 1 column 1`.
A consumer following the documented instruction fails on **every** bundle either
mint site produces.

### Escalation: one of those two values is a secret, not just the wrong bytes

This mission originally described the issuer-node row as "the raw request bytes",
which is accurate and much too mild. `IssueRequest.capability` is documented as

> 32-byte capability root secret (random nonce per mint).

and the handler's own module doc passes it as the first argument to
`CapabilityToken::mint`, whose first parameter is `root_secret`. So the issuer
node writes a capability's **master key** into the issued artifact, in a field
documented as holder-record bytes, in the clear.

The consequence is not that a decode fails. It is that the bundle is no longer
safe to store or relay: anyone who reads one can mint arbitrary caveats against
that capability root, because they hold the key the macaroons are HMAC'd with.
Every other field in the bundle is derived or structural; this one is the input.

A test pins the behaviour as correct —
`handle_holder_record_bytes_carries_capability_root` asserts the bytes equal the
supplied secret — so this is tested in, exactly like the decode failure.

**Why the issuer has no caveat to attach.** The same handler documents its
intended flow as `CapabilityToken::mint(&capability, &issuer_key, &holder_did, &[])`
with "the full substrate lands in a later mission", and the code matches the
aspiration, not the flow: it never calls `mint`, never signs, never registers.
It builds a `CapabilityTokenV2` struct literal and returns it. So the issuer
node does not issue — the caller supplies the root secret and the handler wraps
it and hands the secret back.

That is the root of the remaining ask-binding blocker, and it is worth stating
plainly because the two findings look unrelated and are the same thing: **no
production path can attach a caveat to a capability**, because the only
production issue handler never mints one. Adding `Caveat::AskBinding` to the
issuer is not a one-line change there; the mint has to exist first, and
`IssueRequest` has no ask field for it to carry.

### The capability never leaves the node

`octo-wallet-node`'s mint handler computes the V1 macaroon wire form, then:

```rust
let _ = minted_wire;
```

with the comment "The V2 envelope is the on-the-wire payload; V1 stays local." The
real capability is produced and discarded. A caller of this handler receives a
bundle it cannot use.

`expires_at_unix_secs` is set to `u64::MAX` with the comment "macaroon substrate
lacks expiry field; root TTL lives in caveat" — and the caveats are among the
things the envelope does not carry. The expiry was moved out of reach along with
everything else.

## Why this is filed rather than closed

The field documentation is corrected here to state what the bytes actually are.
Correcting the documentation does not put a capability back in the bundle.

**The risk is latent, not live.** Nothing consumes the V2 output today: every
`CAPABILITY_ISSUE` reference in the workspace is internal to the issuer node
(handler, node dispatch, payload-kind definition), and the capability
verification production performs works on the V1 macaroon substrate. So no
holder is being handed an unusable capability, because no holder is being handed
anything yet.

The cost of leaving it is that the wire form is load-bearing for a plan and
empty for a present. The next implementer to build a consumer will discover this
at the point where it is hardest to change.

## Design-principle reading

This is `cipherocto-design-principles` §Storage is not a protocol, and §No
premature coupling, failing together. The field is named after a type and
documented as carrying that type; nothing enforces it, and the contents are
producer-specific. That shape forces every future consumer to either discover the
producer's convention out of band or guess it, which is the business-rule
replication the principle exists to prevent.

The principled fix is a typed boundary — decode at the edge, hand consumers a
`HolderRecord` — rather than a longer comment on a `Vec<u8>`.

Note that `CapabilityLookupResponse` in the same issuer node has a field of the
same name that **does** carry canonical-JSON `HolderRecord` bytes. One name,
two meanings, one of them honest.

## Acceptance Criteria

- [ ] The V2 wire form carries whatever a verifier needs to check a capability, or it is withdrawn as a wire form and the response payload reverts to the V1 macaroon it replaced.
- [ ] `holder_record_bytes` either holds canonical-JSON `HolderRecord` bytes that `HolderRecord::canonical_de` accepts, or it is replaced by a typed field and its documentation stops naming a type it does not enforce.
- [ ] Both producers agree, byte for byte, on what the field contains.
- [ ] A minted V2 envelope round-trips into something a verifier accepts — not merely into a decoded struct.
- [ ] `expires_at_unix_secs` reflects the capability's real expiry rather than `u64::MAX`, or the field is removed and the caveat path is restored.

## What must not happen

Do not add a test asserting that `holder_record_bytes` decodes to a `HolderRecord`.
That test would have to be written against fixed fixture bytes, because it cannot
pass against real minted output — which makes it a test that validates the
fixture rather than the code, and a green board for a broken contract.

Do not resolve this by making the field `Option<Vec<u8>>` or emptying it. The
field exists to carry a holder record; making it optional removes the signal that
something is missing without supplying what is missing.

Do not treat "nothing consumes V2 yet" as a reason to defer. It is the reason the
gap is cheap to fix now and expensive to fix after a consumer lands.

## Dependencies

- **There is no production consumer to be broken or protected by this fix.**
  Measured while filing: the ingress `CapabilityVerifier` trait has no production
  implementation, `verify_with_resolve` has no production caller, and
  `GatewayAuthenticator::new` is called only from its own test module. That is why
  the empty bundle is latent rather than live — and why deciding _what a V2
  capability is_ is a design question rather than a bug fix. See §The root cause
  under four of the five missions in mission `0009-ce`.
- Mission `0009-cf-capability-chain-parent-binding-is-not-enforced` — the chain
  binding now exists but has no verification path, and this mission's criteria
  are where that path would have to live.
- Mission `0957-f-v2-bundle-consumer-migration` — the mission that made the V2
  envelope the response payload.
- RFC-0957 §3.1–3.2 (the macaroon is the capability)
- RFC-0009 §Capability Keys
