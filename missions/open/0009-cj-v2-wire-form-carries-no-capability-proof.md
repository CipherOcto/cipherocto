---
name: 0009-cj-v2-wire-form-carries-no-capability-proof
description: CapabilityBundleV2 as produced by both mint sites contains no macaroon, no signature and no caveat set. Its holder_record_bytes field is documented as a serialized HolderRecord that must be decoded via HolderRecord::canonical_de, but both producers write raw non-JSON bytes, and the documented decode fails on every bundle production emits. One producer writes the capability root secret into it. Root cause found: RFC-0009 §Forward compatibility specifies holder_record: HolderRecord and discharge_macaroon: DischargeMacaroon as TYPED fields, and the implementation replaced both with Vec<u8> to avoid a layer dependency — which removed the type system from the check and is why two producers could disagree about what the field holds. The wire form that replaced the capability on the response path carries no proof material.
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

A test pinned the behaviour as correct —
`handle_holder_record_bytes_carries_capability_root` asserted the bytes equal the
supplied secret — so this was tested in, exactly like the decode failure. **Both
producer tests have since been rewritten.** The issuer-node one, now
`holder_record_bytes_carries_a_real_record_and_not_the_root_secret`, asserts the
opposite; the wallet-node one still pins a non-record.

**Why the issuer had no caveat to attach.** The same handler documented its
intended flow as `CapabilityToken::mint(&capability, &issuer_key, &holder_did, &[])`
with "the full substrate lands in a later mission", and the code matched the
aspiration, not the flow: it never called `mint`, never signed, never registered.
It built a `CapabilityTokenV2` struct literal and returned it. So the issuer
node did not issue — the caller supplied the root secret and the handler wrapped
it and handed the secret back. **This has since been fixed**: `IssueHandler`
holds the node's `&IdentityKey` and `&dyn HolderRegistry`, calls `mint` for real,
registers the `HolderRecord`, and puts the record's canonical bytes in the
bundle. The holder's pre-signed commitment envelope is still passed as `&[]`,
which is why the ask-binding blocker below is narrower than it was but not
closed.

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

## Root cause: the RFC specified typed fields, and the implementation byte-ified them

This is the part that makes the finding fixable rather than merely real.

RFC-0009 §Forward compatibility gives the schema explicitly:

```rust
pub struct CapabilityBundleV2 {
    pub bundle_version: u8, // = 2
    pub token: CapabilityTokenV2,
    pub holder_record: HolderRecord,          // typed
    pub discharge_macaroon: DischargeMacaroon, // typed
}
```

The implementation has `holder_record_bytes: Vec<u8>` and
`discharge_macaroon_bytes: Vec<u8>`, and `bundle_v2`'s own doc records the second
as a deliberate **deviation from RFC-0009 v1.2** — taken to keep
`octo-cap-macaroon` free of `quota-router-storage` deps (a Layer 4 → Layer B
edge) and to avoid a `Vec<Caveat>` borsh derive cascade.

So the divergence is documented, deliberate, and defensible on its own terms.
What was not considered is what the byte indirection **costs**:

**It removes the type system from the check.** With
`holder_record: HolderRecord`, neither producer _could_ have written a
`capability_id` or a capability root secret into it — the compiler would have
rejected both. With `Vec<u8>`, both compiled, both shipped, and the field's
contract became a comment. That is why the two producers disagree with each
other, and it is why one of them put a secret in there: the type that would have
refused it was replaced by bytes and nothing replaced it.

### The recorded rationale does not survive checking

`bundle_v2` documents the deviation as keeping "layer discipline" and avoiding
a `Borsh` derive cascade. **The layer half is stale, and checking it changes the
conclusion.**

The rationale is to avoid a `Layer 4 → Layer B-substrate` edge by not
depending on `quota-router-storage`. But `HolderRecord` is defined in
`crates/octo-cap-macaroon/src/holder_record.rs` — **the same crate** that
defines `CapabilityBundleV2` — and `DischargeMacaroon` lives in that crate's
`token` module too. There is no cross-crate edge to avoid. The types the RFC
asks for are siblings of the struct, and a typed field would compile with no
dependency change at all.

The derive-cascade half is **still true**: `DischargeMacaroon` derives
`Serialize`/`Deserialize` but not `Borsh`, and it contains `Vec<Caveat>` where
`Caveat` has two dozen variants, so dropping it into a borsh struct cascades.
The honest statement is therefore narrower than the one in the code —
serde-encoding these two fields is defensible; the layer argument for it is not.

And the reason the producers cannot simply put the real type in is neither of
those. **Neither production mint has a `HolderRecord` to put there.**
`HolderRecord::from_capability` has six call sites in the workspace and every
one sits inside a `#[cfg(test)] mod tests`. There is no constructor to call, so
the field has no legitimate content — which is exactly why one site writes a
capability id and the other writes a root secret.

That is the real root cause, and it is one layer down from where the search
started.

This is §Stable Abstractions Principle failing in an unusual direction. The
abstraction was _removed_ to avoid a dependency, and the dependency was the thing
enforcing the contract. The resolution is probably not "put the concrete type
back" — the layer concern that motivated the deviation is real, and this arc has
a record of overrides that ignored it. It is more likely a constructor that
takes a `HolderRecord` and serializes it, so the type still does the checking
even if the wire stays bytes.

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

This is `docs/architecture/design-principles.md` §Storage is not a protocol, and §No
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
- [x] `holder_record_bytes` either holds canonical-JSON `HolderRecord` bytes that `HolderRecord::canonical_de` accepts, or it is replaced by a typed field and its documentation stops naming a type it does not enforce. **Done** — both producers write the serialized `HolderRecord`, and both tests decode it against real minted output. The leak of the capability root secret through this field is closed, and so is the second leak: the wallet node was writing a bare 32-byte `capability_id`, which is the record's primary key with none of the record around it.
- [x] Both producers agree, byte for byte, on what the field contains. **Done**, and the route there is worth recording because the first fix created the divergence rather than removing it: correcting only the issuer node left two producers writing two different meanings, where before they had written the same wrong one. The wallet node's `MintHandler` now writes the same canonical bytes and inserts the record into its own node's registry, so the bytes on the wire are provably the bytes the registry stored. **Two production construction sites, and two is right**: RFC-0871 puts the wallet node and the capability issuer node on separate payload-kind namespaces, both mint, and RFC-0957-A1 §Roles and Authorities gives the Token Issuer — the `IdentityKey` of an issuing node — the authority _"mint + revoke + register"_. `compute_capability_id` is what `compute_cap_root_hash_from_wire` delegates to, so the wallet node was already deriving the correct registry PK; only the record around it was missing. The guard `only_the_issuing_nodes_build_a_holder_record_in_production` pins the set exactly in both directions, so a third site and a deleted registrant both go red.
- [ ] A minted V2 envelope round-trips into something a verifier accepts — not merely into a decoded struct. **This is the first criterion, and it is the next blocker in the chain.** Measured on real output from both producers, not read off the struct: the mint response payload is not UTF-8 at all — it is the 16-byte `CIPHEROCTO_V2_BUNDLE_PREFIX` followed by borsh — so it cannot be a v1 macaroon wire, which is base64url ASCII with `.` separators. `discharge_macaroon_bytes` is empty for a root mint. The `HolderRecord` carries no macaroon. What the holder receives is `bundle_version`, `audience_did`, `channel_id`, `expires_at_unix_secs`, `issuer_did` and a holder record: **a descriptor of a capability, not the capability.** The registry knows it — that part works, and is what the two commits closing §Both producers agree just fixed — but nothing the holder holds can be presented to `ProductionCapabilityVerifier`, so nothing reaches it. Fixing persistence moved the chain one link and exposed this as the one behind it.
- [ ] `expires_at_unix_secs` reflects the capability's real expiry rather than `u64::MAX`, or the field is removed and the caveat path is restored.

## What must not happen

**This rule was correct when written and is now overturned by the fix.** The
prohibition on a test asserting that `holder_record_bytes` decodes to a
`HolderRecord` stood on one premise: that the test would have to be written
against fixed fixture bytes, because real minted output could not decode. That
premise was true while both producers wrote 32 raw bytes, and false the moment
they wrote a real record. Both mint handlers now have a test that decodes
genuinely minted output, and it is the strongest test either handler has — a
fixture-byte test would still pass if the producer regressed, which is the exact
failure this rule was written to prevent.

The transferable part is the reasoning, not the prohibition: before pinning a
contract with a test, check whether the test can be written against real output
rather than a fixture. If it cannot, the contract is probably the thing that is
wrong, not the test.

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
