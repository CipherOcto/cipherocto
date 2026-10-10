---
name: 0009-e-signable-domain-separation
description: RFC-0009 AC#13. Apply the mandatory algorithm_id + key_id signable prefix across every signer and verifier, so a cross-algorithm signature replay is a malformed signature rather than a wrong equation.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-09T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-b-algorithm-registry
---

# Mission `0009-e-signable-domain-separation` — IMPLEMENTED 2026-10-09

## Status

**Claimed and implemented (2026-10-09), with the signer migration explicitly
outstanding** — see §What this does not cover.

All seven §Test Vectors are in place under their specified names.
`octo-ident` 95, `octo-wallet` 337, `octo-algo-stark-schnorr` 46 + 5
integration. Clippy clean with `--all-targets --all-features` across all three.

### Layer decision, written down as §Layer direction demands

The builder is in **`octo-ident`, Layer B**, and it depends on
**`octo-determin`, Layer A**, for the payload canonicalizer.

The mission's §Layer direction offers two readings of a builder taking
`[u8; 16]` and calls for a decision. A `[u8; 16]` builder really would be
Layer A, but it cannot be type-checked against the discriminators — a caller
passing sixteen arbitrary bytes compiles, and naming `algorithm_id` at all is
the point. So the builder takes `AlgorithmId` and `VerificationMethodId`,
which are Layer B, and it lives beside them. Calling it Layer A would be
wrong; the B → A dependency on `octo-determin` is the correct direction.

### The five findings

**1. `canonical_ser` resolves to none of the five functions by that name.**
The mission frames this as "which of the five is the DCS substrate
canonicalizer". The honest answer is that **none of them is**:

| Function                                    | Encoding                                |
| ------------------------------------------- | --------------------------------------- |
| `zk_circuit::canonical_ser`                 | hand-rolled, its own domain byte `0xA8` |
| `CapabilityBundleV2::canonical_ser`         | borsh                                   |
| `CapabilityBundleV2Envelope::canonical_ser` | borsh                                   |
| `Caveat::canonical_ser`                     | JSON                                    |
| `HolderRecord::canonical_ser`               | JSON                                    |

So there was nothing to point at and the mission's own assumption did not
hold. The builder calls `octo_determin::dcs::dcs_serialize_bytes` by path,
and `canonical_ser_is_the_rfc0126_substrate_canonicalizer` pins that it is
DCS and not JSON and not borsh. The JSON exclusion the mission predicted is
real; the mission just also excluded the other three.

**2. A bare payload longer than 32 bytes parses into an attacker-chosen
prefix.** `verifier_rejects_missing_prefix` was specified as "the verifier
refuses a message with no prefix", which suggests a structural check. There
is no structural check to be had: `SignedMessage::parse` splits the first 32
bytes off as a prefix, and any payload at least that long _parses_. Feed it
the ASCII text `transfer 25 OCTO to did:octo:zrecipient` and the prefix reads
back as whatever those bytes say.

Nothing rejects it structurally, and that is the sharp edge the vector now
asserts: a real signature still fails, on the **key binding**, and a verifier
that only asked "does this parse?" would accept the message. The two checks
that do the work are the `key_id` comparison and dispatching on the prefix's
algorithm — both of which `verification_follows_the_algorithm_the_prefix_names`
pins by asserting on _which_ error comes back.

**3. The cross-algorithm replay is refused by the prefix and by key shape,
and only the key-id swap is attributable to the prefix alone.** Said
precisely in the test file's header rather than claimed as a clean result:
Ed25519 keys are 32 bytes and STARK-curve keys are 33, so a STARK public key
dispatched to Ed25519 is refused on length before any equation runs. The
`key_id` swap is the clean demonstration — same algorithm, same key, same
payload, so the only reason it can fail is that the recomputed signable bytes
differ.

**4. `IdentityKey` cannot fail _open_.** RFC-0009 forbids deriving the
`key_id` from the public key and forbids allocating one at signing time, so
an identity with no `key_id` has nothing to invent. `sign_prefixed` and
`verify_prefixed` return `WalletError::NoKeyContext` rather than falling back
to an unprefixed signature — a downgrade path is precisely how an unprefixed
message keeps verifying after the prefix is deployed.

**5. A pre-existing break, found by running a target this mission had no
reason to.** `crates/octo-wallet/tests/ask_publish_cli.rs` has not compiled
since mission `0009-c2` changed `AskSigned::verify` to take an `algorithm_id`.
That mission's gate ran `cargo test -p octo-wallet --lib`, which never
compiles an integration test, so the break was invisible until
`cargo clippy --all-targets` ran on the crate this mission had to touch.
Fixed, and the lesson is recorded rather than the fix alone: **a `--lib` gate
does not check that the crate's integration tests build.**

### Mutation-verified

Six mutations applied and reverted; five fired red on the first pass.

| Mutation                                                 | Caught by                                                       |
| -------------------------------------------------------- | --------------------------------------------------------------- |
| Verifier ignores the prefix's algorithm                  | `verification_follows_the_algorithm_the_prefix_names`           |
| `key_id` binding check removed                           | `a_message_naming_another_key_id_is_rejected`                   |
| Signer allocates a `key_id` instead of refusing          | `an_identity_with_no_key_id_refuses_to_sign_a_prefixed_message` |
| Payload canonicalized without the `u32_be` length prefix | `canonical_ser_is_the_rfc0126_substrate_canonicalizer`          |
| Prefix emits hex identifiers instead of raw bytes        | `the_prefix_is_thirty_two_bytes_of_raw_identifier_bytes`        |
| Hashed form collapses onto the raw form                  | `the_hashed_form_is_self_checking_because_it_is_a_digest`       |

**A sixth mutation came back green, and the green was the finding.** The
first attempt to test the key-id swap deleted the assertion from the test
rather than breaking the implementation — of course a test that no longer
asserts anything still passes. A vacuous vector is indistinguishable from a
passing one if you never try to break it. Redone against
`SignablePrefix::to_bytes` (drop the `key_id`), it goes red immediately, and
that is the form the vector is worth keeping in.

A deliberate no-op mutation was also run to confirm the harness does not
report red for everything.

### Decisions taken here

- **Raw form, declared.** RFC-0009 prefers the hashed form for new
  implementations; this ships raw and says so, which is what the RFC
  requires of any implementation that chooses. The reason is that raw is the
  only form carrying the algorithm **in band**, and "verify under the
  algorithm the prefix names rather than the one it prefers" is not expressible
  otherwise — a digest is not invertible, so a hashed-form verifier would have
  to take the algorithm out of band, back to trusting its own configuration.
  `SignedMessage::parse` refuses a hashed message for exactly this reason,
  which is why the two rejection vectors are principled rather than heuristic.
- **`SignedMessage` stores the payload uncanonicalized** and canonicalizes on
  demand, so `to_bytes` and `signable` cannot disagree about which bytes the
  signature covers.
- **The replay test lives in `octo-algo-stark-schnorr`, not `octo-ident`.**
  `octo-ident` is Layer B and must not depend on either algorithm crate; the
  only crate that can see both is one that depends on both.
- **`from_record` rehydrates without a `key_id`** and refuses prefixed
  signing. The persisted record does not carry one, and inventing it locally
  is what the RFC forbids. Carrying it through `IdentityRecord` is the
  follow-up that lifts the restriction.

## What this does not cover

The mission's §Scope says "every signer and verifier". What is done is the
**substrate, one real signer end to end, and the two decisive vectors over a
real algorithm pair**. The mechanical migration is not done, and is recorded
here rather than left to be discovered:

- `IdentityKey::sign` and `IdentityKey::verify` still sign and verify **bare**
  messages. `sign_prefixed` / `verify_prefixed` are the prefixed path and are
  complete, but nothing forces a caller onto them.

  An earlier draft of this section said "30 in-crate call sites and roughly ten
  downstream crates". **That number was wrong** and is corrected here. It came
  from a raw `grep -c '\.sign('` over `crates/octo-wallet/src`, which counted
  `ed25519_dalek::SigningKey::sign`, `HsmAdapter` internals, and test bodies
  alongside genuine `IdentityKey::sign` calls. Counting only non-test
  production sites gives **eight**:

  | Site                                 | What it signs                                     | State                        |
  | ------------------------------------ | ------------------------------------------------- | ---------------------------- |
  | `octo-wallet` §`revoke`              | `b"revoke"` — a 6-byte constant, bound to nothing | migrated in `0009-e2`        |
  | `octo-wallet` §`begin_rotation`      | `b"rotate" ‖ new_pub`                             | migrated in `0009-e2`        |
  | `octo-wallet` §`IdentityKey::sign`   | the caller's bare message                         | closed in `0009-e6`          |
  | `octo-wallet` §`identity_store`      | a wrapper over the above                          | closed in `0009-e6`          |
  | `octo-wallet-node` §sign handler     | the request bytes                                 | migrated in `0009-e3`        |
  | `octo-runtime` §handle/signing (two) | the holder message                                | migrated in `0009-e4` (v1)   |
  | `quota-router-core` §`envelope_v2`   | a domain-separated preimage                       | **not migrated — see below** |

  **All sites are now migrated.** The last two — the bare `IdentityKey::sign`
  substrate escape and the `identity_store` wrapper — closed in `0009-e6`, which also
  found something larger: `register` never attached a verification method, so every
  normally-registered operator identity had no key context and could not sign under
  the prefix at all. Every migrated site above was correct and unreachable. See
  `0009-e6` §Defect one.

  The `octo-wallet-node` handler was taken first of the four because it is the
  only remotely reachable one, and the only one where the missing binding is
  directly exploitable rather than merely untidy: it signs whatever 32 bytes it
  is handed, so before `0009-e3` its output was replayable anywhere a
  bare-message Ed25519 signature is accepted. The design decision that
  mattered there is written up as `0009-e3` §Who builds the prefix — in
  particular, that the **signer** supplies the prefix and the requester never
  names the domain, which is the opposite of how `SignedMessage` would
  otherwise travel.

### §`envelope_v2` is already domain-separated, and must not be prefixed

The open question was whether `quota-router-core` §`envelope_v2` should take
the RFC-0009 prefix. **It should not.** Reading `signature_preimage` settles
it: the preimage is already a **32-byte domain-separated digest**,
`blake3::derive_key(DOMAIN_SIGNATURE, envelope_id ‖ from_did ‖ payload)`, and
RFC-0853 §Domain Separation is the very mechanism the RFC-0009 prefix
implements. Prefixing it would bind two domains inside one signature, and
would change an RFC-0871 byte-pinned wire format in order to do so.

Three things the prefix would supply are already present:

1. **A domain.** `DOMAIN_SIGNATURE` is a dedicated context string — exactly
   the role the prefix's domain field plays.
2. **The signer, in band.** `from_did` sits inside the preimage, which binds
   the signer _more_ tightly than a `key_id` would: the DID resolves to the
   key rather than merely naming it.
3. **The algorithm, by type.** `Authorization::Signature` carries
   `Ed25519SignatureBytes`, so the algorithm is fixed by the wire type and no
   confused message can substitute another.

What the prefix would add is a `key_id`, and RFC-0871 has no
verification-method identifier on the envelope to carry one. Supplying it is
an RFC change, not a migration, so this site stays as it is.

**The one property the preimage does not obviously have, and why it does not
need it.** `envelope_id ‖ from_did ‖ payload` joins two variable-length
fields with no length prefix, and
`octo-settlement-core` §`router_signature_preimage` length-prefixes
`router_id` for exactly this reason — so the shape invites the objection that
the boundary is ambiguous. It is not exploitable, and _not_ because of how
base58 happens to length-encode. Moving the `from_did`/`payload` boundary
changes the DID, and the verifier derives the verifying key _from that same
DID_: `verify_ed25519_signature` base58-decodes the signer DID and builds the
`VerifyingKey` from the result. A shifted DID resolves to a different key, so
the signature check fails closed. Signer binding and key selection read one
field, which makes the boundary self-defending — and because the argument is
independent of base58's length behaviour, a future multibase or multicodec
change cannot silently invalidate it.

### A defect found while checking this: `envelope_id` did not commit to the envelope

Answering the question meant reading how `envelope_id` was derived, and the
derivation was wrong. `compute_envelope_id` zeroed `envelope_id` but **not**
`authorization`, while `build_node_envelope` builds with an empty authorization
and then populates it. So a delivered envelope's `envelope_id` was a digest of
the _pre-signature_ bytes and did **not** equal `compute_envelope_id` of the
envelope as delivered. A comment claimed the two were "structural-equivalent in
the borsh serialization"; they are not — `[]` and
`[Authorization::Signature { .. }]` are different bytes.

Nothing caught it, for two compounding reasons. No verify path recomputes the
id: the verifier reads it off the wire and rebuilds the preimage from it, which
is sound only because the id is inside the preimage. And every test that
recomputed the id built its envelope with an empty authorization — the single
case where the old definition and the intended one agree. The field had
quietly stopped being a content digest, and so could not have served as a
replay or dedup key, which is the one job an envelope id exists for.

Fixed by excluding `authorization` from the derivation, which is what RFC-0871
§Adversary Analysis A6 describes and what makes the chicken-and-egg resolve at
all. The RFC-0871 byte-pinned vectors are unaffected, because they all build
with an empty authorization, where the two definitions coincide — so no pinned
digest changed. Two vectors now pin the property:
`envelope_id_commits_to_contents_independently_of_authorization` and
`envelope_id_is_unchanged_by_adding_a_signature`. Both were confirmed red
against the old derivation before the fix was applied.

The second of those two was initially **vacuous**: it cloned a built envelope
and mutated the `authorization` field, which of course left the stored
`envelope_id` untouched and so asserted nothing. Rewritten to send both
variants through `NodeEnvelope::build`, so the constructor genuinely
recomputes in each case. The populated-authorization build is the control that
distinguishes them — the empty one is exactly the case that agrees with the
old behaviour.

One process note for the next implementer: `cargo clippy -p quota-router-core
--all-features` is **not** a valid invocation. `litellm-mode` and
`any-llm-mode` are mutually exclusive behind a `compile_error!`. Use
`--all-targets` without `--all-features` for that crate.

### The `CapabilitySigner` trait change — scope surveyed, key_id answered

`CapabilitySigner::sign(&self, msg: &[u8])` in `octo-cap-macaroon` is
unchanged, and it carries no key context at all — so the RFC-named "real
signers" list is still only half covered. This was an open question in this
mission and is now answered from the source rather than deferred.

**The "11 implementors across 6 crates" figure overstates the blast radius.**
Enumerating every `impl CapabilitySigner for` site in the tree gives 16, split
cleanly between the two same-named traits: 12 for the macaroon one and 4 for
the governance one. Of the 12, exactly **one** is production —
`octo_wallet::IdentityKey`. Two are the `Box`/`Arc` blanket impls. The other
nine are test fixtures and dev-mode stubs spread across `octo-cap-macaroon`
(`signer`, `token`, `wire`, `discharge`, `cli_fns`), `octo-role`, and
`octo-cli`. So this is one production impl to migrate and a wide fan of test
scaffolding to update — real work, but not a cross-cutting substrate change,
and it should not be scoped by the raw implementor count.

**A defect found while surveying this: mint stamps an algorithm the signer
never named.** `CapabilityToken::mint` and `attenuate_with_signer` call
`holder.sign(&msg)` and then wrap the result with
`HolderSignature::new(governance_algorithm_id(), ...)`. The trait names no
algorithm, so the id on the token is a constant chosen by the mint site rather
than a fact about the signature. Demonstrated by probe: a
`HolderSignature` stamped `stark-schnorr`, pushed through what the wire
round-trip does, comes back stamped Ed25519. Latent rather than live — the
trait's `[u8; 64]` and `[u8; 32]` return types cannot represent a 33-byte key
at all — but the failure mode when a second algorithm arrives is a silent
wrong answer, which is the shape AC#11 exists to prevent. Fixed in
`0009-e5` on both halves: mint now stamps what the signer reports, and the wire
segment carries the algorithm so the round-trip cannot re-stamp it. The
"resolved out of band by the caller" comment named a source that does not
exist — `HolderRecord` carries `holder_pub` and no algorithm at all. The prefix
migration for this trait stays split out; `holder_msg` still has no field for a
`key_id`.

**Where a capability key's `key_id` comes from: the holder identity's
verification method.** No new concept is needed. The trait already carries
`did()`, and its contract is substrate truth — the same DID the slash ledger
binds to, not a CLI-side derivation. A capability _holder_ is therefore an
identity, and its `key_id` is that identity's `verification_method_id`,
exactly as at every other site in this table. The production implementor
already has the field; the trait change is what exposes it.

**The trait carries neither `algorithm_id` nor `key_id` today.** This is
known rather than accidental: a fixture in `octo-cap-macaroon` §`signer` says
so in as many words and calls widening the trait a follow-on, noting that the
verifier takes the algorithm id as a parameter so nothing is blocked. Both
ids belong on the trait, because the verifier currently learns the algorithm
from its own configuration — which is the preference RFC-0009 forbids.

### A site this table does not list: `octo-governance`'s `CapabilitySigner`

`crates/octo-governance/src/attest.rs` declares a **second, unrelated trait
also named `CapabilitySigner`**, with `fn sign_envelope(&self, envelope_bytes:
&[u8]) -> Result<[u8; 64], String>`. It is not the macaroon one, it is not in
the eight sites counted above, and it signs bare. The same-name collision
across two crates is itself the hazard: an implementer grepping for
`CapabilitySigner` finds the wrong one.

**Answered: the encoding is sound, the prefix gap remains.** Two questions,
two different answers, and the second one cost a wrong hypothesis to get right.

_Is the canonical envelope ambiguous?_ **No.** It concatenates with bare `0x00`
delimiters and emits `expires_at_unix` as either zero or eight bytes, which is
exactly the concatenation-collision shape `octo-cap-macaroon` §`holder_msg`
length-prefixes against. It is not, because **both optional fields are fixed
width** — `Option<u64>` and `Option<&[u8; 32]>` — so the presence flags are
recoverable from the total length and no two distinct presence/value
combinations can produce the same bytes.

A collision was constructed and the vector came back **green**, which is the
only reason that is written down rather than left as a claim. It is now pinned
by `optional_fields_cannot_collide_by_shifting_a_boundary`, mutation-checked by
making the snapshot variable-width — which produces a real
`DuplicateAttestation`, two semantically distinct attestations collapsing to one
id.

_Does it need the prefix?_ **Yes, for the same reason as everywhere else.**
`sign_envelope` takes `[u8; 64]`, names no algorithm, and binds no key — so a
governance signature carries neither. What it does have, unlike `envelope_v2`,
is an algorithm already threaded through `verify_governance_signature` as an
explicit parameter, and a signer whose DID is inside the envelope. That is
narrower than the macaroon case, not wider.

Also worth recording: `attest` and `attest_v2` **duplicate the envelope
construction verbatim**, so any fix has to land twice or the deprecated path
becomes a second source of truth. That duplication is itself a
§No parallel abstractions problem, independent of the prefix.

### The `octo-runtime` attach-handle paths, and what the token actually binds

`octo-runtime` §`handle/signing` has two, and only one is live.
`octo-attach-key-rotation` is **opt-in and off by default**, so v2 — which
binds a `key_id` — does not compile in a default build, while v1 is what
`mint_attach_handle`, `attach_with_token`, `decode_token` and the CLI's
`agent run --detach` path all use.

`AttachHandle` has six fields and carries **no `key_id` and no algorithm**;
`encoding.rs` and the struct definition contain zero `key_id` references. So
neither path has the token naming its own binding. v1's verifier takes a bare
32-byte pubkey from the caller, and v2's takes `key_id: KeyId` from the caller
and folds that _claim_ into the signed bytes. v2's suffix is therefore a real
anti-swap property — the code documents the forgery it prevents, where a
grace-key holder mints a token claiming a victim's `key_id` — but it binds a
`key_id` the **verifier** chose, not one the token carries.

And in both, the algorithm is a hardcoded verifier preference via
`attach_algorithm_id()`. Stated so it is not over-claimed: that is **latent,
not a live break**, because `IdentityKey`'s signer is Ed25519-typed today, so
every identity able to mint a token is Ed25519 and the assumption currently
holds. It becomes real the moment a second algorithm is usable as a holder key.

Two readings of this were recorded and both were wrong, in opposite
directions: first that v2's suffix is redundant with the prefix, then that it
is the one case where a hand-rolled suffix does a job the prefix cannot. The
data flow settles it — the verifier rebuilds signed bytes from the token's
_fields_, so a prefix spliced into the canonical bytes is unreadable, which is
why the prefix has to become a **field**. With it as a field the suffix is
genuinely redundant, because it binds the same `key_id`, is readable rather
than verifier-chosen, and adds the algorithm. See `0009-e4` §Why this one is
not like the others.

**v1 is migrated; v2 is not.** `0009-e4` put the prefix on the token as a
field, made v1 signing and verification go through it, and retired the
hardcoded Ed25519 from the v1 verify path. The wire form grew 32 bytes and the
encoding version moved to `0x01`. The feature-gated v2 suffix survives and
`attach_algorithm_id` still has that one caller; the `KeySet` grace-window work
is deliberately separate, since the KeySet resolves _which pubkey_ to try and
the prefix binds _which key_.

The `KeySet` grace-window lookup is a separate concern and survives: it
resolves _which pubkey_ to try, whereas the prefix binds _which key_.

- ~~`verify_successor_proof` / `verify_revocation_proof` take an
  `algorithm_id` **out of band**~~ **Done**: both now take a `key_id` and
  verify under the prefix. See `0009-e2`.
- ~~`IdentityRecord` does not persist a `key_id`.~~ **Done** in
  `missions/claimed/0009-e2-lifecycle-proof-domain-separation`: the record now
  carries `verification_method_id`, and both lifecycle proofs are migrated.
  The persistence was the prerequisite — without it, binding the successor
  proof would have made rotation work before a store save and fail after the
  reload.

The follow-up is a migration mission, not an extension of this one: replace
`sign`/`verify` at each site and delete the bare path, since RFC-0009's
§Backward compat forbids a downgrade route and nothing is deployed to
deprecate towards. **Until then, a prefixed message is rejected by a bare
verifier and a bare message is rejected by a prefixed verifier** — the two
shapes are not interoperable, which is the property that keeps the partial
migration safe rather than silently wrong.

## Dependencies

`depends_on: 0009-b-algorithm-registry` — the prefix names a
`VerificationMethodId` and an `AlgorithmId`, both of which that mission adds.

## Scope

Make the signable prefix real at every signer and verifier.

```
signable = algorithm_id ‖ key_id ‖ canonical_ser(payload)
```

**This mission ships the raw form**, and states so because RFC-0009 requires an
implementation to declare which of the two it uses — the vectors below include
`verifier_rejects_hashed_form_when_declaring_raw` and its reverse, and neither
means anything until the choice is on the record. RFC-0009 prefers the hashed
form for new implementations, so shipping raw here is a deliberate departure
from that preference: it is recorded in RFC-0009 rather than left implicit, and
every verifier in this mission MUST reject the hashed form. If the raw form's
fixed-width precondition ever breaks, that is the moment to switch, and the
switch is an RFC amendment, not an implementation detail.

## Why this is not hygiene

Ed25519 signatures are 64 bytes (`R ‖ S`). STARK-curve Schnorr signatures are
also 64 bytes (`r ‖ s`). Same length, different meaning, and neither is a type
error for the other. A verifier that dispatches on its own configured algorithm
rather than the one named in the message gets a silent wrong answer — no
exception, no rejection, just a check run against the wrong equation. Same
shape as the sign-bit defect recorded in RFC-0974 §Step 1, where every vector
in the original suite happened to have the sign bit clear.

## The prefix is specified byte-exactly. Follow the spec, not the sketch

RFC-0009 §Dual-format signature algorithms and the ZK-capable key gives the
normative rules, and each one exists because the naive version has a real
failure mode:

- **`algorithm_id` is the 16 raw bytes of its `Uuid`**, not the 36-character
  hyphenated form. Both are in use in this codebase — borsh encodes the raw
  form, serde the textual one — and an implementation that picks differently
  from its peers signs a different domain.
- **`key_id` is the 16 raw bytes of the `VerificationMethodId` `Uuid`** — not
  the 36-character hyphenated form, exactly as above. It MUST NOT be derived
  from the public key, and MUST NOT be locally allocated at signing time. A
  locally-allocated id makes the signed domain depend on who signed, which
  defeats the point.
- **Both components are fixed at 16 bytes, so the raw prefix is
  self-delimiting and needs no length framing.** An earlier draft of this
  mission dropped this bullet and was wrong: it is the precondition that makes
  the raw form safe. `VerificationMethodId` is `struct VerificationMethodId(Uuid)`
  — RFC-0009 now defines it, and until that amendment it had **no declared
  representation anywhere**, so nothing fixed the width. The repository's
  existing convention for a verification-method key id is a _string fragment_
  (`DidDocument::authenticates` matches values like `"#key-0"`), which is what
  an implementer reaches for — and at variable width the raw form collides:

  | triple A                        | triple B                        | identical signable bytes |
  | ------------------------------- | ------------------------------- | ------------------------ |
  | `key_id = ""`, payload `"abc…"` | `key_id = "a"`, payload `"bc…"` | identical                |
  | `key_id = "ab"`, payload `"c"`  | `key_id = "a"`, payload `"bc"`  | identical                |

  The empty-`key_id` row needs no crafted attack, only a payload whose first
  byte looks like an id character. If `key_id` is ever variable-width, the raw
  form MUST move to the hashed form in the same change that makes it
  variable.

- **`canonical_ser` means RFC-0126 §Part 3: Deterministic Canonical
  Serialization, as this repository implements it — `octo-determin`'s
  `dcs_serialize_*`.** RFC-0009 corrected this bullet; the earlier text
  resolved the symbol "by name" against RFC-0126, which defines no such symbol,
  and counted three functions where there are **five**
  (`zk_circuit::canonical_ser`, `CapabilityBundleV2::canonical_ser`,
  `CapabilityBundleV2Envelope::canonical_ser`, `Caveat::canonical_ser`,
  `HolderRecord::canonical_ser`). Two of those five are JSON, and RFC-0126
  §JSON Allowed Contexts lists cryptographic proofs among the contexts where
  JSON is **forbidden** — so `Caveat::canonical_ser` and
  `HolderRecord::canonical_ser` are excluded by rule, not preference. A signer
  that reached either by grepping for the bare symbol would be signing a domain
  the serialization RFC prohibits.
- **A verifier MUST reject a message with no prefix**, and MUST verify under
  the algorithm the prefix names rather than the one it prefers.

The hashed form —

```
signable = BLAKE3-256("cipherocto/sigdomain/v1" ‖ algorithm_id ‖ key_id)
           ‖ canonical_ser(payload)
```

— is preferred for new implementations: it removes every framing question in
one step. The two forms are **not** interoperable. An implementation states
which it uses; a verifier MUST reject the other.

## Where the signer gets `key_id` — the part the RFC forbids and the mission must solve

RFC-0009 forbids the two obvious answers: deriving `key_id` from the public
key, and locally allocating it at signing time. Those are the only two sources
a signer currently has, and neither is available.

`IdentityKey` in `octo-wallet` carries no DID document, no
`VerificationMethodId` and no algorithm identifier. `CapabilitySigner::sign`
in `octo-cap-macaroon` takes `(&self, msg: &[u8])` — no key context at all.
So neither signer can currently produce a correct prefix, and the highest
probability implementation is precisely the forbidden one.

This mission must therefore supply the key context, not assume it. The options
are narrow: the signer holds or is handed its `VerificationMethodId` alongside
its private key (a constructor parameter, a registry keyed by key, or a DID
resolution the signer performs). Whichever is chosen, it is a **public API
change on the signer type**, and it is the deliverable that makes the rest of
this mission possible. An implementer who skips it and reaches for
`Uuid::new_v4()` has implemented RFC-0009's explicitly forbidden repair.

If that turns out to require a signing API break that collides with
`0957-ext-macaroon-crate`, declare the conflict rather than routing around it
— a prefix whose key id is not stable is worse than no prefix, because it
looks implemented.

## The gate must compile the crates that actually sign and verify

The original gate was `cargo test -p octo-ident --lib`. **`octo-ident` has no
signing or verification code and no `ed25519-dalek` dependency** — its
manifest is `blake3`, `thiserror`, `hex`, `bs58`, `async-trait`, `parking_lot`,
and `borsh` behind the optional `borsh` feature. The only
Ed25519-adjacent symbols there are the `VerificationMethod` data carrier and a
64-byte signature field on `ResolverBackend`.

The real signers are `IdentityKey::sign` and `HsmAdapter::sign` in
`octo-wallet`, and `CapabilitySigner::sign` in `octo-cap-macaroon`. The real
verifiers are the `VerifyingKey::from_bytes` sites and the 27
`&VerifyingKey`-parameter `verify` functions `0009-c` enumerates.

So the original gate can go fully green with the whole mission implemented
inside `octo-ident`, while every signer and verifier in the repository still
signs the unprefixed shape. That is the "prefix computed but never covered"
defect, and the gate was structurally incapable of catching it.

Verified by the §Test Vectors list. Each vector is a named, independently
runnable check; a vector passing is the acceptance condition met.

## Test Vectors

Six of these are this mission's own additions, not RFC acceptance criteria —
see the note below the list.

- `signable_prefix_is_prefix_free` — no two distinct
  `(algorithm_id, key_id, payload)` triples produce the same byte string
- `signable_prefix_survives_algorithm_swap` — same key material, different
  `algorithm_id` prefix, different signable bytes
- `signable_prefix_survives_key_swap` — same algorithm, different
  `verification_method_id`, different signable bytes
- `verifier_rejects_missing_prefix`
- `verifier_rejects_hashed_form_when_declaring_raw` and the reverse — the two
  encodings are not interoperable and that is a correctness property
- `canonical_ser_is_the_rfc0126_substrate_canonicalizer` — pins the
  disambiguation; this exists because there are **five** functions by that
  name, two of them JSON and therefore forbidden for proofs
- `cross_algorithm_signature_replay_rejected` — the decisive one. A signature
  made under algorithm A, presented as algorithm B, must be **rejected**.
  **Owned by this mission**, not shared with `0009-c`: it is a property of the
  prefix, and RFC-0009 AC#13 names it here. Note it is specified over the
  Ed25519 / STARK-Schnorr pair, so it cannot be satisfied until `0009-d` has
  registered a second algorithm — declare that ordering rather than mocking it
  away, because a mock proves dispatch rejects a mismatched identifier, which
  is `0009-c`'s concern and not this one.

### Most of this list is not in the RFC

The heading above attributes these vectors to RFC-0009 §Acceptance Criteria.
**Only one of the seven appears there** — `cross_algorithm_signature_replay_rejected`
at AC#13. The other six are this mission's own additions and are not RFC
acceptance criteria. That attribution is the same defect class A15 records: a
normative citation that resolves to nothing.

Also note that five of the seven only compare **byte strings**. They never
build a signature, call `verify`, or assert a rejection. A prefix helper that
returns the right bytes and is never called by any signer satisfies all five.
The vectors that can actually detect the vulnerability are
`cross_algorithm_signature_replay_rejected` and
`verifier_rejects_missing_prefix`; treat those as the acceptance condition and
the rest as supporting checks.

Gate: `cargo test -p octo-ident --lib -p octo-wallet --lib -p octo-cap-macaroon --lib`.
The last two are the ones that matter, and `octo-ident` alone is not an
acceptance condition for this mission — see §The gate must compile the crates
that actually sign and verify.

## Layer direction (per `cipherocto-design-principles`)

**Name the crate and the function, or this section does not do anything.** An
earlier draft called the prefix construction "Layer A — pure canonical encoding,
frozen" while the builder's parameters are `AlgorithmId` and
`VerificationMethodId`, which `0009-b` places in `octo-ident` — Layer B. Two
readings follow, with opposite dependency directions:

- a builder taking `[u8; 16]` is genuinely Layer A, but cannot be
  type-checked against the discriminators, so a caller passing a non-Uuid
  16 bytes compiles
- a builder taking `AlgorithmId` / `VerificationMethodId` is Layer B by
  construction, which is fine and consistent, but then calling it Layer A is
  wrong

Decide this in the mission and write the crate name down. What must not happen
is the current state: a warning about parallel encodings with no named owner,
which is a warning nobody can act on.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-ident -p octo-wallet -p octo-algo-stark-schnorr \
  --all-targets --all-features -- -D warnings
cargo test -p octo-ident --lib
cargo test -p octo-wallet --lib
cargo test -p octo-wallet --test ask_publish_cli
cargo test -p octo-algo-stark-schnorr
```

`octo-ident --lib` alone cannot observe whether the prefix is applied, because
it does not sign or verify anything — the mission's own §The gate must compile
the crates that actually sign and verify. `cross_algorithm_signature_replay_rejected`
lives in an integration test of the crate that can see both algorithms.

The `--all-targets` flag is not decoration. It is what caught the
`ask_publish_cli` break described in §Status, which a `--lib` gate cannot see.

## Backward compat

Additive on the message format, but every existing signer and verifier MUST
change together. A verifier reading an unprefixed message MUST reject it rather
than fall back to the legacy shape — prefix-stripping must not be a downgrade
path. Nothing is deployed, so there is no compatibility window and no shim.

## Cross-references

- `rfcs/accepted/process/0009-identity-evolution-v12.md` — §Domain separation
  is mandatory, not hygiene; AC#13; A12
- `rfcs/accepted/numeric/0126-deterministic-serialization.md` — §Part 3:
  Deterministic Canonical Serialization, and §JSON Allowed Contexts. The
  previous reference `rfcs/accepted/process/rfc-0126-*` resolved to nothing:
  wrong directory and wrong glob
- `rfcs/draft/economics/0974-zk-capability-platform-extension-surface.md` —
  §Step 1, the sign-bit defect that is the same class of bug
- `missions/claimed/0009-b-algorithm-registry`, `0009-c-verification-dispatch`,
  `0009-d-stark-schnorr-algorithm`
- `crates/octo-ident/src/signable_prefix.rs` — the prefix substrate
- `crates/octo-algo-stark-schnorr/tests/signable_replay.rs` — the two decisive
  vectors, over a real Ed25519 / STARK-Schnorr pair

## Claimant

@mmacedoeu

## Pull Request

#
