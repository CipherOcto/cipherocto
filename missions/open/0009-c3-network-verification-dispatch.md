---
name: 0009-c3-network-verification-dispatch
description: RFC-0009 AC#11, octo-network slice. 41 hardcoded-Ed25519 verification sites under octo-network, including 27 public signatures that take a typed VerifyingKey. Blocked on the RFC-0010 canonical DID codec.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-09T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-b-algorithm-registry
    - RFC-0010
    - 0009-c-verification-dispatch
---

# Mission `0009-c3-network-verification-dispatch` — OPEN 2026-10-09 (BLOCKED)

## Status

Open (2026-10-09). **Blocked** on the RFC-0010 canonical OctoID codec. See
§Why this is blocked.

## Dependencies

`depends_on: RFC-0010, 0009-b-algorithm-registry, 0009-c-verification-dispatch`.

The RFC-0010 dependency is the load-bearing one and is not a formality. See
§Why this is blocked.

## Scope

Every hardcoded-Ed25519 verification site in `crates/octo-network`.

| Shape                       | Sites | What they are                                                                                                   |
| --------------------------- | ----- | --------------------------------------------------------------------------------------------------------------- |
| `from_bytes` construction   | 14    | `ocrypt/attestation`, `gossip/reputation`, 4x `porelay`, `drs/route`, `dot/envelope`, `dot/dc`, `dom/admission` |
| typed `&VerifyingKey` param | 27    | the `dot/` transport envelopes — see below                                                                      |

**41 sites, and it is the largest slice.** That is why it is separate rather
than folded into `0009-c`: it is one crate, but a crate long enough to build
that it would dominate the gate, with 27 of the sites needing public signature
changes rather than body edits.

## The 27 typed-key sites

All under `crates/octo-network/src/dot/`: `dc_envelopes` (7), `handover` (6),
`kick_envelopes` (5), `binding` (5), `decommission` (2), `sub_group` (1),
`witness` (1). Twenty-five are `pub fn verify`; the other two are
`validate_bind` and `append`.

Each verifies a **transport role** key — founder, witness, issuer, DC,
coordinator, incumbent, local node — not a holder or capability key.

Three facts establish that this is a different trust domain, not an oversight:

- `dot/` contains **zero** references to `octo-ident`, `VerificationMethod`,
  `DidDocument`, or any DID registry. It never resolves an identity.
- `octo-network` does **not** depend on `octo-ident`. Its manifest comment
  records that it extracts the operator pubkey from `operator_did` "without
  round-tripping through the full octo-ident codec".
- The envelopes are node-to-node protocol messages with a fixed wire format.
  They are not claims a holder makes about itself.

**AC#11's literal text does not reach these.** It names two **constructors** —
`VerifyingKey::from_bytes` and `Signature::from_bytes` — and these sites call
neither. They accept an already-typed key and never choose an algorithm. An
earlier draft of this work treated them as in scope; that was wrong, and
changing 27 signatures to satisfy a grep rather than a requirement is how
abstractions get inflated.

They are still worth fixing. An Ed25519 type in a public signature is a real
coupling — it is just a **different** defect from AC#11's, and naming it
honestly matters more than folding it into this one.

## Why this is blocked

`octo-cap-macaroon::pubkey_from_did` parses `did:octo:0x<hex>` into
`Option<[u8; 32]>`. A bare 32-byte array carries **no algorithm identifier**.

Its own doc comment marks this as temporary: a "Phase 1 form" that "production
swaps in the RFC-0010 §Canonical OctoID Codec once `octo-ident` lands the typed
form".

So every caller of those 27 `verify` methods currently derives its key through
a function that can only ever return Ed25519. **Changing the signatures from
`&VerifyingKey` to `&AlgorithmId` before that codec lands moves nothing** —
every caller resolves to Ed25519 regardless. It is 27 cosmetic edits and a
large diff that prove no new property.

The blocker is the codec, not this mission's refactor.

## Unblock condition

RFC-0010's canonical OctoID codec lands, the DID typed form can carry an
algorithm identifier, and `pubkey_from_did` is retired in favour of it. After
that, a caller can name an algorithm, and dispatch means something here.

Re-verify the 41 sites at that point rather than trusting this count.

## Re-verified 2026-10-10 — the counts hold, but the gate cannot see them

Re-counted rather than trusted, as §Unblock condition instructs.

| Shape                       | File says | Measured                                                   |
| --------------------------- | --------- | ---------------------------------------------------------- |
| `VerifyingKey::from_bytes`  | 14        | **14**                                                     |
| typed `&VerifyingKey` param | 27        | **24** (`&VerifyingKey` in an `fn` signature under `dot/`) |

The `from_bytes` count is exactly right. The typed-key count is 24, not 27 —
three sites were refactored since this file was written. Small enough not to
change the mission, recorded so the next reader does not re-derive it.

### The real finding: `octo-network` is in no AC#11 list at all

`crates/octo-network/src` appears **nowhere** in `AC11_DISPATCHED`,
`AC11_OUT_OF_SCOPE`, `AC11_ALLOWED`, or the `dirs.push` set in `octo-cli`'s
`security_regressions`. All 14 sites are outside the gate's view entirely.

**Proved, not argued.** A `VerifyingKey::from_bytes` site was planted in
`octo-network/src/dom/admission.rs` and all four AC#11 vectors reported
**green**. The gate scans a fixed list, so a crate that is simply absent
cannot fail it. This is the same "found nothing wrong" vs "looked at
nothing" shape as the sibling guards, one level up: those asked a
_scanned_ crate whether it was clean, this one never asks the question.

Adding the crate to `AC11_DISPATCHED` today would fail the gate on 14
sites, which is correct and is the point — but it is dispatch work, not
gate work. **The gate change is to make the absence visible, not to
pretend the crate is clean:** an explicit entry recording that this crate
is pending mission `0009-c3`, with its measured site count, so the count
cannot drift silently and a new site is caught by the count moving rather
than by someone remembering.

### A second gap, in a crate the gate _does_ call dispatched

The gate greps exactly one pattern, `VerifyingKey::from_bytes`. AC#11 names
**two** constructors — `VerifyingKey::from_bytes` **and**
`Signature::from_bytes`. The second was invisible to the gate everywhere,
including in crates it certifies as clean.

One such site was live production code in `octo-wallet`:

- `crates/octo-wallet/src/identity.rs` §`IdentityKey::sign` built
  `ed25519_dalek::Signature::from_bytes(&sig_bytes)`.

`octo-wallet/src` **is** in `AC11_DISPATCHED`, and the gate passed it. The
asymmetry inside that one file was the tell: `IdentityKey::verify`
dispatches through
`octo_algo_ed25519::verify_dispatch(&self.signer.algorithm_id(), …)` while
`IdentityKey::sign` hardcoded the dalek constructor. Mission `0009-e9`
migrated `WalletSignerAdapter` to `sign_prefixed`, and `sign_prefixed` does
carry the algorithm — but the plain `sign` entry point did not go with it.

So this is one more inherited premise that did not survive checking: the
record said `octo-wallet` was dispatched, and for every `VerifyingKey`
site it is. For this one it is not.

## Both gaps closed 2026-10-10

`IdentityKey::sign` now returns the raw `[u8; 64]` the adapter produced, so
`ed25519_dalek` no longer appears in its signature and the
`use ed25519_dalek::Signature` import is gone from `identity.rs` entirely.
The algorithm stays recoverable from `IdentityKey::algorithm_id`, which
sources it from the same signer, so a caller pairing `sign` with `verify`
cannot mismatch them.

The AC#11 gate now greps **both** constructors. Extending it immediately
turned the gate red on three sites the old single-pattern scan could never
have seen — and two of those three were false positives the moment it did,
because `GovernanceSignature::from_bytes` contains the _substring_
`Signature::from_bytes`. The gate matches whole path segments now
(`contains_path_call`), so a longer name cannot masquerade as a shorter
one. Without that fix the first exception the gate reported would have
been one nobody could act on, which is how a guard teaches people to
ignore it.

The third was real and is allowlisted with a stated reason:
`quota-router-sm-engine`'s `unsigned_canonical_ser` blanks the signature
field to build the message that is _about_ to exclude it. The zero value
is never serialized and never verified, so dispatching there would be
dispatching on a value with no cryptographic meaning.

**The gate earned its keep immediately.** `octo-wallet`'s AC#14 vector
shells out to a workspace clippy and caught a `quota-router-core` test
still calling `.to_bytes()` on the new return type — a cross-crate
consumer this change broke, and one that a grep of the two edited crates
would have missed. Fixed there too.

Proved by mutation in both directions: a `Signature::from_bytes` planted in
`octo-wallet/src/hsm.rs` turns the extended gate red, and the same plant in
`identity.rs` is what the `sign` change removes.

## What would make this gate whole

Three separable changes, in increasing cost:

1. ~~Extend the scan to `Signature::from_bytes` as well.~~ **Done**,
   2026-10-10. It found a live site in a crate the gate certified clean.
2. ~~Give un-scanned crates an explicit _pending_ entry with a measured site
   count, so `octo-network`'s absence is a recorded fact rather than an
   accident.~~ **Done**, 2026-10-10. `AC11_PENDING` in the `octo-cli` guard
   lists all ten `octo-network` files with their individual site counts, and a
   vector asserts each count still matches — in **both** directions, so a new
   site and a dispatched site each turn the gate red. That second direction is
   the one that matters: it is what stops the table outliving its blocker, the
   way the AC#11 allowlist entry did.
3. Dispatch the `octo-network` sites. Blocked on the codec, as
   §Why this is blocked says.

## Acceptance Criteria

Verified by the §Test Vectors list.

## Test Vectors

- `no_verification_site_hardcodes_an_algorithm` — scoped to `octo-network`,
  **both shapes**. A vector that greps only `from_bytes` reports green with all
  27 typed-key sites live.
- `transport_envelope_verification_is_algorithm_agnostic` — a DOT envelope
  verifies under a non-Ed25519 algorithm. This is the vector that proves the
  dispatch is real rather than cosmetic, and **it cannot be written until the
  codec lands** — that is the block, stated as a test.
- `octo_network_does_not_reach_into_ed25519_dalek` — no `from_bytes` outside the
  Ed25519 registration.

## Layer direction (per `cipherocto-design-principles`)

`octo-network` is Layer C. **C depends on B is permitted**, so adding
`octo-ident` is directionally correct. But per the current architecture it is a
_new_ edge for this crate, and it must carry a manifest comment naming the
layer and the RFC that mandates it — the same justification `0009-c2` requires.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-network --all-targets --all-features -- -D warnings
cargo test -p octo-network --lib
```

`octo-network` is long to build. Do not run the full workspace suite, and do
not add `octo-network` to another mission's gate to make that gate stronger —
it makes it slow instead.

## Backward compat

None required. Nothing is deployed.

## Cross-references

- `rfcs/accepted/process/0009-identity-evolution-v12.md` — AC#11
- `rfcs/accepted/process/0010-canonical-did-codec.md` — the codec this blocks on
- `missions/claimed/0009-c-verification-dispatch` — the core slice
- `missions/open/0009-c2-verification-dispatch-new-dependency-sites`
- `missions/claimed/0009-b-algorithm-registry`

## Claimant

@unassigned

## Pull Request

#
