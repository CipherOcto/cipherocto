---
name: 0009-e9-governance-attestation-signer-binding
description: Close the last bare sign site in the RFC-0009 chain and stop the governance substrate from recording a signer attribution the signer never confirmed.
metadata:
  node_type: mission
  type: research
  created: 2026-10-10T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-e-signable-domain-separation
    - 0009-e6-registration-key-context
---

# Mission `0009-e9-governance-attestation-signer-binding`

## Status

**Implemented.** Closes the last production sign site that emits a bare,
non-domain-separated signature under the canonical operator key, and the last
two that record an attribution the signing key never confirmed.

## §The finding

Two defects, in different crates, sharing one root: the governance signer is
never asked who it is.

### The bare signature

`crates/octo-cli/src/commands/governance.rs` holds `WalletSignerAdapter`, which
wraps the active `Arc<octo_wallet::IdentityKey>` and implements the substrate's
`sign_envelope` by calling the bare `IdentityKey::sign`. Every other production
sign site in this chain now emits `SignablePrefix ‖ payload` — capability
minting, attach handles, lifecycle proofs, the wallet-node oracle. This is the
last that does not, so the signed bytes carry no statement about which key or
which algorithm made them, and their shape is entirely caller-chosen.

### The attribution

The governance `CapabilitySigner` trait has **no method that reports the
signer's identity**. Every caller therefore supplies the attribution itself, and
the substrate writes it into both the signed envelope and the returned receipt
without ever consulting the signer:

- `attest_v2` takes `signer_did: &str` as a parameter.
- `vote_v2` takes `voter_cap: &CapabilityToken` and reads `voter_did` off
  `voter_cap.issuer_did`, then resolves the signing key by a _different_
  field — `session.capability_registry().resolve(voter_cap.cap_id)`.

`vote_v2` is the worse of the two, and it is the production path. The two
fields are independent, so a caller who holds any registered capability token
can set `issuer_did` to any DID they like; the substrate will assemble an
envelope naming that DID, sign it with whatever key was registered under the
`cap_id`, and return a `VoteReceipt` recording the forged `voter_did`. The
signing key and the named voter are never bound. `CapabilityToken` is public,
so this is reachable by any crate that depends on `octo-governance`, not only
through the CLI.

The CLI happens to pass matching values at both call sites, which is exactly
what makes this easy to miss: the production path is correct and the interface
is not.

## §What is live, and what is not

Stating the two defects apart is the point of this section, because the first is
easy to overstate and overstating it would misprice the mission.

**Latent: the bare signature.** All four governance call sites bind it to
`let _signature = …` and drop it. No verifier anywhere in the tree consumes a
governance signature, so there is no present cross-protocol replay — there is no
second bare-signing path to replay one into, and no consumer that would accept
one. The bare form becomes live the moment the signature is persisted, which
§Substrate `[ADD]` already plans for the `governance_envelopes` table.

**Live: the attribution.** `VoteReceipt.voter_did` and
`AttestationReceipt.signer_did` are persisted, returned to the caller, and feed
quorum projection. The forgery above works today. This is the half of the
mission with teeth, and it is the half that must land first.

## §Why the trait is the root

`octo_governance::attest::CapabilitySigner` is a second, unrelated trait that
shares a name with `octo_cap_macaroon::CapabilitySigner` and is a strictly
weaker copy of it:

| Method               | macaroon trait | governance trait |
| -------------------- | -------------- | ---------------- |
| sign                 | yes            | yes              |
| `algorithm_id()`     | yes, required  | **absent**       |
| `did()`              | yes            | **absent**       |
| `public_key_bytes()` | yes            | **absent**       |

Every missing method is one that would have forced the signer to identify itself
at the boundary. `algorithm_id()` absent is why the algorithm cannot be stamped.
`did()` absent is why both attribution defects exist at all. The parallel
abstraction is not a naming annoyance — it is the mechanism that produced both.
§No parallel abstractions names exactly this shape.

## §Scope

1. **Give the governance trait a `did()`**, required, with no default, matching
   the macaroon trait's contract: the canonical substrate-derived identity, not
   a CLI-side derivation.
2. **Make attribution unforgeable.** `attest_v2` drops its `signer_did`
   parameter and reads the value off the signer. `vote_v2` keeps
   `voter_cap.issuer_did` — it is semantic data, not a label — but must refuse
   when it disagrees with `signer.did()`, rather than signing an envelope that
   names one key on behalf of another.
3. **Sign under the identity's own prefix.** `sign_prefixed`, with the signer
   supplying the prefix from its own key context — the rule mission `0009-e6`
   settled for the wallet-node oracle: the signing party names its own domain,
   the caller never does.
4. **Record that `IdentityKey::sign` stays, and say why.** See §What this
   mission did not do.

## §What this mission did not do

The original scope of this mission claimed `IdentityKey::sign` had one
production caller and could be deleted. **The compiler falsified that**, twice,
and the correction matters more than the original claim.

It has two external callers, and neither is a defect:

- `octo-runtime`'s attach-handle path composes `signable_bytes(prefix,
canonical)` itself and calls `sign` only to sign bytes it has already
  domain-separated. That is correct usage of a low-level primitive.
- The `octo_cap_macaroon::CapabilitySigner` blanket impl signs bare. Its form
  is the open question about whether the holder message's prefix travels in
  band — deliberately split out of `0009-e5` and still undecided.

Deleting the method would have forced the macaroon decision into this commit,
which is the bundling §What must not be done exists to prevent. Instead the
method's documentation now states plainly that it applies no prefix, names both
callers, and says a new caller that skips prefix composition is a defect.

What this mission actually removes is the last caller that was signing _bare
without having composed a prefix itself_.

## §Follow-on: the parallel surfaces this exposed

The root cause named in §Why the trait is the root was duplicated code, so the
follow-on removed it rather than leaving it documented.

`attest` and `attest_v2` assembled the canonical envelope **byte for byte
identically**; `vote` and `vote_v2` did the same. Two copies of a canonical
encoding in one crate is a divergence waiting to happen: a fix applied to one
is invisible in the other, and both stay green. Neither pair member was ever
migrated coherently, which is how one attribution defect survived in four
places at once.

Both pre-session entry points are now removed rather than deprecated — nothing
is live, and a deprecated entry point is still an entry point. Removing them
removes the duplication _by construction_, since one function cannot drift
from itself.

What survived that check:

- **Vectors, not just code.** The pre-session tests carried five behaviours
  `attest_v2` did not otherwise exercise: duplicate PK, subgroup gate,
  neither-evidence-supplied, the `allow_stale` override, and evidence hashing.
  Those migrated onto the session path and were mutation-checked against the
  implementation rather than merely re-run. The three that duplicated existing
  vectors were dropped instead of carried along.
- **A second duplicate, found by accident.** `vote` carried its own
  byte-identical `blake3_256`, re-exported as `blake3_256_vote` alongside
  `attest::blake3_256` so a caller could reach either. It had no external
  user. Deleting `vote` exposed it; it is now shared.

The trait name collision with `octo_cap_macaroon::CapabilitySigner` named
above is now closed: the governance trait is `EnvelopeSigner`. It was renamed
rather than merged, because the two have genuinely different contracts — this
one signs opaque envelopes and has no public key, the macaroon one is the
capability substrate's signer abstraction and needs one. The new name states
what it does and reads as the contrast it is. `AnchorGovernanceSigner` in
`octo-reputation` was rejected as a target precisely because it would have
recreated a smaller version of the same grep hazard one crate over.

`EnvelopeSigner` gained `did()` here and is still missing `algorithm_id()`. The
missing `algorithm_id()` is the remaining half of §Why the trait is the root,
and it stays deferred for the reason given in §What must not be done: the
signature is discarded, so an algorithm stamp has no carrier until
§Substrate `[ADD]` persists one.

## §What must not be done

- **Do not add an `algorithm_id()` to the governance trait in this mission.** It
  is the right repair and it is deliberately deferred: the signature is
  discarded, so an algorithm stamp has no carrier until §Substrate `[ADD]`
  persists one. Adding it now would be an untestable field. Record it; land it
  with the persistence.
- **Do not fall back to a bare signature when the prefix is unavailable.**
  `sign_prefixed` refuses with `NoKeyContext`, and that refusal is correct. A
  downgrade path is how an unprefixed message keeps verifying after the prefix
  ships, which is the whole defect class.
- **Do not derive the vote's `voter_did` from the signer and drop the token's.**
  They are different claims — "this capability token names a voter" versus "this
  key signed" — and the vote envelope is supposed to bind both. The fix is a
  consistency check, not a substitution.
- **Do not leave the deprecated entry points unpatched.** `attest` and `vote`
  are `#[allow(deprecated)]` surfaces with zero production callers, and
  removing them is a cleanup rather than a security fix — but both carry the
  same attribution defect and both are still `pub`. Adding the binding there
  too is not scope creep; leaving them out would keep a working forgery
  reachable and make this mission's central claim false.
- **Do not treat the existing CLI-level agreement as evidence the invariant
  holds.** The CLI passing matching values is the reason this went unnoticed.

## §Verification for whoever implements this

The discriminating property is the one that has bitten this chain three times:
**a vector set that only ever exercises Ed25519 cannot distinguish "reads the
prefix" from "assumes Ed25519".** Reverting the prefix call back to the bare
sign must leave the new tests green — that is the signal the set tests nothing.

Four vectors are load-bearing:

1. **The prefix is actually in the bytes.** A signer that records the message it
   was handed must observe `prefix ‖ envelope`, not `envelope`. Asserting only
   that signing _succeeds_ cannot tell the two apart, and this is the vector
   that makes the change to `WalletSignerAdapter` observable at all.
2. **A forged attribution is refused on the production path.** Register a signer
   reporting DID-B under `cap_id`, present a `CapabilityToken` whose `issuer_did`
   names DID-A, and assert `vote_v2` refuses rather than returning a receipt.
   Without this the `did()` addition is an unused method and the live defect
   survives the commit.
3. **The matching case still passes.** DID-A on both sides must succeed. A
   consistency check that simply always refuses would satisfy vector 2 alone,
   and the difference is exactly the regression this vector exists to catch.
4. **`attest_v2` no longer accepts an attribution.** After the parameter is
   dropped it cannot be supplied at all; assert the receipt's `signer_did` is
   the signer's own `did()`.

Also pin the negative: an identity with no `VerificationMethodId` must be refused
by the governance path rather than signed bare. That is the exact downgrade
§What must not be done forbids, and it is the shape the next implementor is most
likely to introduce while making this change.
