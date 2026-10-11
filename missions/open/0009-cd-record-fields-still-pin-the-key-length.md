---
name: 0009-cd-record-fields-still-pin-the-key-length
description: RFC-0009 AC#11 follow-on. 0009-cc widened key PARAMETERS to a slice but left the record fields carrying those keys as [u8; 32], so a dispatched site still cannot receive a second algorithm's key through the record path. Confirmed in octo-reputation, in KeyRotation whose two-algorithm verify cannot accept a second algorithm, and at the root in HsmAdapter, the extension trait whose own signature forbids the adapter its own docs promise.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-11T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-b-algorithm-registry
    - 0009-cc-key-length-still-binds-the-algorithm
---

# Mission `0009-cd-record-fields-still-pin-the-key-length`

## Status

Open. Filed by testing what `0009-cc`'s vector actually proved.

## The finding

`0009-cc` widened 38 key **parameters** from `&[u8; 32]` to `&[u8]`, on the
reasoning that a fixed-size array still decides the algorithm in advance: a
stark-schnorr public key is 33 bytes and cannot be passed to a 32-byte slot.

That reasoning was right about parameters and was not carried to the **record
fields** those parameters are filled from. A verification site that takes
`&[u8]` is still handed a 32-byte array by everything upstream of it.

Measured by role — find every `governance_pubkey` that is fixed-size or
compared against a fixed-size literal — there are exactly **five sites in
`octo-reputation`**, and they are two different bugs:

| Site                                           | Kind                       |
| ---------------------------------------------- | -------------------------- |
| `auth::GovernanceProof::governance_pubkey`     | record field, `[u8; 32]`   |
| `auth::SuspensionAuth::governance_pubkey`      | record field, `[u8; 32]`   |
| `auth::ResumeProof::governance_pubkey`         | record field, `[u8; 32]`   |
| `store::memory::verify_governance_suspension`  | pre-filter, `== [0u8; 32]` |
| `store::stoolap::verify_governance_suspension` | pre-filter, `== [0u8; 32]` |

## Why this is not cosmetic

**`SuspensionAuth` carries the algorithm and cannot express it.** The struct
holds both `governance_algorithm_id` and `governance_pubkey: [u8; 32]`. The
whole point of moving the algorithm onto the record — the reason
`0009-cb`'s companion work added the field — was that a record naming an
unregistered algorithm gets refused rather than silently verified as Ed25519.
But a 33-byte key cannot be stored on that record at all, so for any algorithm
whose keys are not 32 bytes the field is decorative: the algorithm id travels,
and the key length quietly decides the answer before dispatch is reached.

This is the cross-boundary attenuation failure: the type-level widening in
`auth::verify_governance_signature` is undone by the type of the value the
stores read out of it.

## The same failure exists in `octo-network`, and there it inverts

`ocrypt::KeyRotation` is the clearest statement of this bug anywhere in the
workspace, because its documented purpose is the thing its field types forbid.

`KeyRotation` carries `old_public_key: [u8; 32]` and `new_public_key: [u8; 32]`,
and `KeyRotation::verify` takes **two** algorithm ids, dispatching once per key.
Its own doc comment says why two:

> Two algorithms, not one: a rotation is exactly the case where the old and new
> key need not belong to the same algorithm, and a single parameter would assert
> that they do.

So the API is built to accept a rotation from Ed25519 to anything else. The
field types forbid it. Both keys are 32 bytes, so a stark-schnorr key can
neither rotate in nor rotate out, and the second algorithm parameter has no
value it can ever legitimately be given. The two-algorithm design is unreachable
in exactly the cases it was written for.

`to_signing_bytes` makes the same assumption structurally, reserving
`32 + 32 + 8` for a concatenation that is defined by those two field widths.

## The root of the `octo-wallet` half: the extension point pins its own key

Every other instance in this file is a record or struct that happens to be typed
at 32 bytes. `HsmAdapter` is different — it is the **extension mechanism**, and
its own documentation promises the extension:

> This is what makes `IdentityKey::verify` dispatch instead of deciding: the
> key's algorithm comes from the signer that produced it, so adding an algorithm
> means adding an adapter, not editing every verification site (RFC-0009 AC#11).

The trait delivers that promise for the algorithm and breaks it for the key:

```rust
fn get_public_key(&self) -> Result<[u8; 32], HsmError>;
fn sign(&self, msg: &[u8]) -> Result<[u8; 64], HsmError>;
fn algorithm_id(&self) -> octo_ident::AlgorithmId;
```

`algorithm_id` is required rather than defaulted, and the reasoning is recorded:
a default returning Ed25519 would let a future Schnorr or threshold adapter be
wired up without declaring what it is. That reasoning is correct and it is why
an adapter is the right extension point. But an adapter is then **required by
the trait signature** to return its public key in exactly 32 bytes and its
signature in exactly 64. A stark-schnorr adapter has a 33-byte key and cannot
be written.

So the documented extension path — write a new adapter — cannot deliver a
non-32-byte algorithm. The pin is not in the verification sites at all; it is
one layer below them, in the trait that is supposed to be the way out.

`IdentityKey` then copies the pin: its own `public_key: [u8; 32]` is what
`IdentityKey::verify` and `IdentityKey::verify_prefixed` dispatch against, so
widening those two sites without widening the adapter buys nothing.

## Scope: candidates versus confirmed

An earlier draft of this section reported **23** such fields. That number was
wrong, and the reason is worth recording because it is the same class of error
the workspace keeps making in a different costume.

The scan pattern required a `pub ` prefix. `IdentityKey::public_key` is
**private**, so it was never counted — and it turned out to be a confirmed
instance. Dropping the visibility requirement gives **35** matches workspace-wide,
several of which sit inside `#[cfg(test)]` bodies and are not production sites.

Every figure here is a name match and is labelled one. It is an upper bound.

### Confirmed — read, and each reaches a `verify_dispatch` call

| Site                                        | Reaches                                  |
| ------------------------------------------- | ---------------------------------------- |
| `auth::GovernanceProof::governance_pubkey`  | `verify_governance_signature`            |
| `auth::SuspensionAuth::governance_pubkey`   | `verify_governance_signature`            |
| `auth::ResumeProof::governance_pubkey`      | `verify_governance_signature`            |
| `ocrypt::KeyRotation::old_public_key`       | `KeyRotation::verify`, dispatch one      |
| `ocrypt::KeyRotation::new_public_key`       | `KeyRotation::verify`, dispatch two      |
| `wallet::identity::IdentityKey::public_key` | `IdentityKey::verify`, `verify_prefixed` |

The `octo-reputation` half is decisive rather than inferred: that crate contains
**exactly one** `verify_dispatch` call site in the whole of `src`, so those three
fields exhaust its possibilities.

### Ruled out — read, and does not reach a dispatch site

- `ocrypt::OverlayIdentity::public_key` — the struct carries a signature field
  and **no verifier anywhere in the crate reads it**. Not a length problem; a
  different question, and out of scope here.
- `dot::GatewayIdentity::public_key` — reaches `derive_gateway_id`, a BLAKE3
  identity derivation. Nothing verifies it.
- `dot::InviteEnvelope::invitee_pubkey` — reaches `compute_invite_token` and
  `compute_invite_hash`, which hash the key rather than verify against it.
- `orr::HopConstructionParams::relay_public_key` and
  `orr::HopLayer::ephemeral_public_key` — **X25519**, documented as such, used
  for key agreement through `x25519_dalek`. They never reach the
  `AlgorithmRegistry`, which holds signature algorithms. An earlier draft
  guessed this; it is now read and confirmed rather than assumed.
- `auth::AttestorRegistration::pubkey`, `auth::AttestorAuth::attestor_pubkey` —
  `AttestorAuth` records on its own doc that real signature verification is
  deferred to the signer subsystem. A recorded deferral, not drift.
- `auth::AnchorGovernanceSigner::pubkey` — reaches `canonical_bytes`, a 96-byte
  encoding used for quorum counting.
- `compat::CompatMapping::coordinator_pubkey` — a legacy DID-to-pubkey lookup.
- `wallet::identity_record::IdentityRecord::pubkey_bytes` — a stored identity
  record with a `serde_bytes_32` helper; an identity substrate record, not a
  verification site.
- `wallet_node::handlers::resolve::ResolveDIDResponse::public_key` — a response
  DTO, documented as a placeholder for the storage-pubkey form.

### The last five, now resolved

- `dot::GatewayIdentity::new` and `dot::build_invite` — **constructor
  parameters**, not fields. They are the only ways to build
  `GatewayIdentity` and `InviteEnvelope`, so they reinforce the pins on those
  two already-ruled-out records rather than adding an instance. Worth noting
  because a scan cannot tell a constructor from a declaration, and the workspace
  has already once counted constructor calls as verification sites.
- `wallet::mpc::Xor2Of3Signer::public_key` — a 2-of-3 XOR threshold signer, and
  **not** an `HsmAdapter`. It is outside the adapter extension path entirely and
  reaches no dispatch site.
- `octo-ident-storage`'s registry row — that crate contains **zero**
  `verify_dispatch` call sites, so it is outside this mission's scope by the
  criterion used throughout.
- `octo-ident`'s `VerificationMethod::ed25519` — a constructor, and the one
  place in the workspace where a fixed-size key is **honest about it**: the
  algorithm is in the method's name. It is the contrast that makes the others
  look wrong, and it is worth keeping as the pattern to copy when a type
  genuinely must stay 32 bytes.

### A note on the shape of the 35

The figure counts constructor parameters and test-body locals alongside struct
declarations, because no text scan distinguishes them. That is most of why the
confirmed set is six rather than thirty-five, and it is the same distinction the
mission's own predecessor was written to correct.

Deciding this generally requires call-graph resolution, not a text scan. That
approach was prototyped and **rejected on measurement** for the guard that would
need it: closure by name pulled 3265 of 5457 workspace function names into the
graph and produced 15 hits and zero true positives. Do not repeat it.

## The store pre-filters are a second instance

Both stores open `verify_governance_suspension` with

```rust
if auth.governance_pubkey == [0u8; 32] {
    return Err(ReputationError::GovernanceSignatureInvalid);
}
```

This duplicates a check that `auth::verify_governance_signature` already
performs, and `0009-cc` already made **that** one length-agnostic — it compares
`iter().all(|b| *b == 0)` so that it fires for a key of any length, on the
documented grounds that a 32-byte comparison "would silently never fire for any
algorithm whose keys are a different size — a check that looks present and is
unreachable."

The store copies are the exact shape that comment warns about. They cannot
fire for a non-32-byte key, and they are unreachable for the same reason.

Worse, the `store::stoolap` copy sits directly beneath a comment reading
"Same ed25519 check as the memory store, via the shared helper — see
`auth::verify_governance_signature`". The check the comment describes is not the
check on the next line. The comment is wrong about the code immediately below
it.

## Evidence, and what it does not establish

`a_governance_signature_verifies_under_stark_schnorr` in `auth::tests` signs a
governance digest with a 33-byte stark-schnorr key and verifies it through
`auth::verify_governance_signature`. Hardcoding the algorithm turns that one
test red and nothing else, so the **function** genuinely dispatches.

It does not prove the record path, because the test calls the function
directly and never constructs a `SuspensionAuth`. There is no vector that could
prove the record path today, because no second-algorithm key can be placed on
the record. That is the gap this mission closes.

## Acceptance Criteria

- [ ] `HsmAdapter::get_public_key` returns a key whose length is not decided by
      the trait, so an adapter for an algorithm with non-32-byte keys can
      actually be written — this is the root, and the other `octo-wallet` items
      are downstream of it
- [ ] `HsmAdapter::sign` is considered at the same time as the key: a trait that
      fixes the signature length at 64 has the same defect one step over
- [ ] `IdentityKey::public_key` carries whatever the adapter returned, rather
      than a second independent 32-byte copy
- [ ] `GovernanceProof::governance_pubkey`, `SuspensionAuth::governance_pubkey`
      and `ResumeProof::governance_pubkey` carry a key whose length is not
      decided by the type
- [ ] `KeyRotation::old_public_key` and `KeyRotation::new_public_key` carry a
      key whose length is not decided by the type, so the two-algorithm
      rotation its `verify` exists to accept becomes reachable
- [ ] The persistence encoding of all three records is examined and the change
      to it is deliberate, not incidental — these records are stored, and the
      current `[u8; 32]` may be relied on by an existing schema or column width
- [ ] The `#[repr(C)]` wire form of `KeyRotation` is treated as a wire change,
      with its two fixed-width key slots and two 64-byte signature slots
      considered together rather than independently
- [ ] Both store pre-filters either delegate to the length-agnostic check inside
      `auth::verify_governance_signature` or are removed as redundant
- [ ] The `store::stoolap` comment is corrected to describe the code beneath it
- [ ] A second-algorithm vector that round-trips through a **record**, not just
      through the verification function — constructed, stored, read back and
      verified — so the store path is covered and not only the helper
- [ ] A second-algorithm vector that runs through an `HsmAdapter`, proving a
      non-Ed25519 adapter can be written and that `IdentityKey` dispatches on
      the algorithm it reports
- [ ] Zero-key sentinel behaviour is unchanged for every existing 32-byte key
- [x] The five previously untraced fields are read and each confirmed or ruled
      out — by reading, not by extending the text scan, which is known to be
      unsound here

## Open question

Whether the three record fields should become `Vec<u8>` or a fixed-size
newtype over `Vec<u8>` that validates length at construction. The second is
attractive: it keeps "a key of the wrong length cannot be built" as a type
invariant instead of a runtime check, which matches the reasoning `0009-cb` and
`0009-cc` both applied to parameters. It costs a schema-visible change to
persisted records either way.

`KeyRotation` may force the question the other way. It is `#[repr(C)]`, so its
field widths are the wire format, and a length-validated newtype over `Vec<u8>`
has no fixed width to publish. If the rotation must stay `repr(C)` then its key
slots stay fixed-width, and the honest outcome is that a rotation record can
only carry one algorithm's key length — which would mean dropping the
two-algorithm `verify` rather than pretending to support it. That is a real
design fork and should be decided deliberately, not by whichever change compiles
first.

## Why this was not folded into `0009-cc`

`0009-cc` is closed and its scope figure — 38 parameters — was measured by
finding values that reach a `verify_dispatch` call. Record fields do not reach
`verify_dispatch`; they reach a store, which reaches the function. Widening
them is a different surface with a different blast radius (persistence), and
folding it into a closed mission would have repeated the error the mission
itself documents: reporting a scope that does not match what was measured.
