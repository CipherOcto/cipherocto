---
name: 0009-cd-record-fields-still-pin-the-key-length
description: RFC-0009 AC#11 follow-on. 0009-cc widened key PARAMETERS to a slice but left the record fields carrying those keys as [u8; 32], so a dispatched site still cannot receive a second algorithm's key through the record path. Confirmed in octo-reputation and, inverting the problem, in KeyRotation, whose two-algorithm verify cannot accept the second algorithm.
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

## Scope: candidates versus confirmed

The five `octo-reputation` sites and the two `KeyRotation` fields are
**confirmed** — each was read and each reaches a `verify_dispatch` call.

The workspace holds **23** struct fields declared as `pub <name>: [u8; 32]` where
the name is a public-key name. That figure is a **name match** and is an upper
bound, not a finding. Most are not verification sites at all: `octo-ident`'s
registry and resolver backend define keys rather than verify them, and the
`orr` onion-routing types may belong to an entirely separate scheme that never
reaches the `AlgorithmRegistry`.

Of those 23, **16** sit in the nine crates the AC#11 gate lists as dispatched —
7 in `octo-network`, 7 in `octo-reputation`, 1 in `octo-wallet`, 1 in
`octo-wallet-node`. The other 7 are in `octo-core`, `octo-ident`,
`octo-identity-resolver-node` and `octo-policy`, none of which the gate scans.

Establishing which of the 16 actually reach a dispatch site requires
call-graph resolution, not a text scan. That approach was prototyped and
**rejected on measurement** for the guard that would need it: closure by name
pulled 3265 of 5457 workspace function names into the graph and produced 15
hits and zero true positives. Do not repeat it. The honest position is that 5
sites are confirmed, 11 are unexamined, and a text scan cannot close the gap.

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
- [ ] Zero-key sentinel behaviour is unchanged for every existing 32-byte key
- [ ] The 11 unexamined fixed-size public-key fields in dispatch-site crates are
      each confirmed as reaching a dispatch site or ruled out, by reading them —
      not by extending the text scan, which is known to be unsound here

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
