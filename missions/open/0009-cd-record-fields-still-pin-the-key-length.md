---
name: 0009-cd-record-fields-still-pin-the-key-length
description: RFC-0009 AC#11 follow-on. 0009-cc widened key PARAMETERS to &[u8] but left the record fields that carry those keys as [u8; 32], so a dispatched site still cannot receive a second algorithm's key through the record path.
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
- [ ] The persistence encoding of all three records is examined and the change
      to it is deliberate, not incidental — these records are stored, and the
      current `[u8; 32]` may be relied on by an existing schema or column width
- [ ] Both store pre-filters either delegate to the length-agnostic check inside
      `auth::verify_governance_signature` or are removed as redundant
- [ ] The `store::stoolap` comment is corrected to describe the code beneath it
- [ ] A second-algorithm vector that round-trips through a **record**, not just
      through the verification function — constructed, stored, read back and
      verified — so the store path is covered and not only the helper
- [ ] Zero-key sentinel behaviour is unchanged for every existing 32-byte key

## Open question

Whether the three record fields should become `Vec<u8>` or a fixed-size
newtype over `Vec<u8>` that validates length at construction. The second is
attractive: it keeps "a key of the wrong length cannot be built" as a type
invariant instead of a runtime check, which matches the reasoning `0009-cb` and
`0009-cc` both applied to parameters. It costs a schema-visible change to
persisted records either way.

## Why this was not folded into `0009-cc`

`0009-cc` is closed and its scope figure — 38 parameters — was measured by
finding values that reach a `verify_dispatch` call. Record fields do not reach
`verify_dispatch`; they reach a store, which reaches the function. Widening
them is a different surface with a different blast radius (persistence), and
folding it into a closed mission would have repeated the error the mission
itself documents: reporting a scope that does not match what was measured.
