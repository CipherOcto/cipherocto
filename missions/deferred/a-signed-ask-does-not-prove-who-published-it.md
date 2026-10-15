---
name: a-signed-ask-does-not-prove-who-published-it
description: "Two measured defects in what a signed Ask proves about its publisher. First, AskSigned::verify checks that the supplied public key signed the payload but never checks that the key belongs to payload.asker_did, so Alice can sign an Ask claiming to be Bob and verify returns Ok with Alice's key and Err with Bob's. The pre-existing sign_then_verify_roundtrip test pins this: its fixture uses a fabricated asker_did ('did:octo:asker1') unrelated to the seed, so the test would fail if the binding existed. Second, octo-wallet's derive_asker_did emits did:octo:b plus hex of the raw public key (74 chars), which CanonicalCodec::parse rejects; the canonical form is did:octo:z plus base58btc (54 chars) per RFC-0009, and the legacy form is did:octo:b plus 52 base32 chars per RFC-0968. Two accepted RFCs mandate different canonical encodings and the only live producer matches neither."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959
    - RFC-0009
    - RFC-0968
---

# Mission `a-signed-ask-does-not-prove-who-published-it`

## Status

Deferred — binding the DID to the key inside `verify` changes its signature
and its error surface, and is a Layer A identity decision (the canonical DID
encoding is specified in three places that disagree, so "which encoding is
authoritative" has to be answered before a fix can be written). Both hazards
are documented on the code they affect.

This is the highest-consequence finding in the marketplace audit that does
**not** require the unwired facade to be reachable.

## Finding 1 — the verifier does not bind identity to key

`AskSigned::verify` performs two checks: re-derive `ask_id` from the payload,
and verify the signature over `canonical_ser(payload)` against a caller-supplied
public key. It never compares that key to `payload.asker_did`.

So the function proves exactly one thing: _the holder of the public key you
passed in signed this payload_. It does not prove that the Ask was published
by the identity the payload names.

### Measured

Alice signs an Ask whose `asker_did` is Bob's:

```text
alice_seed = [0x11; 32]
bob_seed   = [0x22; 32]
payload.asker_did = did_for(bob_seed)

claimed_did        = did:octo:ba09aa5f47a6759802ff955f8dc2d2a14a5c99d23be97f864127ff9383455a4f0
bob_real_did       = did:octo:ba09aa5f47a6759802ff955f8dc2d2a14a5c99d23be97f864127ff9383455a4f0

verify_with_ALICE_key = Ok(())
verify_with_BOB_key   = Err(AskSignatureInvalid)
```

The signed envelope carries Bob's identity, verifies against Alice's key, and
is correctly rejected against Bob's. Nothing in `verify` notices the
disagreement.

### Why the existing test does not catch it

`sign_then_verify_roundtrip` builds its payload with
`asker_did: "did:octo:asker1"` — a literal unrelated to the seed it signs
with — and asserts `verify` succeeds. The test pins the _absence_ of the
binding: an implementation that correctly rejected the DID mismatch would
fail it. The fixture would need a DID derived from the signing key for the
test to be asserting anything about identity.

That is a narrower version of a pattern this audit has hit repeatedly: a
check written against a fixture the check does not actually exercise.

### What is at stake

`asker_did` is the identity the marketplace attributes everything to —
`list_by_asker`, reputation, slashing, escrow, stake. RFC-0959 §Roles
asserts the Asker's signature **is** the seller signature ("R13-N8 fix:
`seller_signature ≡ Asker signature`"). That equivalence is precisely the
claim `verify` does not enforce: a signature by _A_ establishes nothing about
_DID(B)_.

The CLI is honest by construction — `derive_asker_did` computes the DID from
the same seed it signs with, so `ask_publish` cannot produce a mismatch. The
defect is entirely in the verifier, and a verifier must not assume its
producer is the wallet.

## Finding 2 — three incompatible canonical DID encodings

The wallet derives the asker DID by hand, bypassing the identity substrate
that RFC-0009 specifies:

```rust
fn derive_asker_did(seed_arr: &[u8; 32]) -> String {
    let id = IdentityKey::from_seed(*seed_arr);
    format!("did:octo:b{}", hex::encode(id.public_key_bytes()))
}
```

That is a raw public key in hex. RFC-0009 §Identity Key Format specifies
"`did:octo:` + multibase(z) of raw 32 bytes", and RFC-0009 §Verification
requires checking that "suffix decodes as multibase(z) → 32 bytes". Hex is
not multibase-z, so the form the wallet emits fails the check RFC-0009 names.

RFC-0968 §6 specifies something different again: `did:octo:b<52>`, base32
lowercase-no-padding of BLAKE3-256 of the public key, 62 chars total.

`CanonicalCodec::parse` accepts exactly two forms — the canonical `z` form
(43–44 char payload) and the legacy `b` form (62 chars total, 52 base32
chars) — and the wallet's output is neither.

### Measured

Same public key, three derivations:

```text
wallet_did    = did:octo:bd04ab232742bb4ab3a1368bd4615e4e6d0224ab71a016baf8520a332c9778737  len=74
canonical_did = did:octo:zBTHmU4m1ELJJ71UtwwtuTCNefNt8qdyzr3rU9nn5FNnU                       len=54

parse(wallet_did)                  = Err(InvalidLength("did:octo:b legacy form requires 52-char base32 payload"))
parse(wallet_did, legacy_bare=true) = Err(InvalidLength("did:octo:b legacy form requires 52-char base32 payload"))
```

Enabling the legacy-bare flag does not help, because the problem is the
payload encoding and length, not the prefix.

Every DID `octo-wallet ask publish` emits is therefore rejected by the
canonical identity substrate — and `Ask`'s own field doc cites
"RFC-0009 §Identity Key Format" for exactly that field.

The failure is not a clean prefix rejection. The wallet's output _starts_
with `did:octo:b`, the same prefix as the valid legacy form, which is why the
diagnostic blames the base32 payload. It is a plausible-looking forgery of
the legacy encoding rather than an obviously wrong string.

## Why this is deferred rather than fixed

Both fixes need an answer this audit cannot supply.

1. **Which DID encoding is authoritative?** RFC-0009 says multibase-z;
   RFC-0968 §6 says base32-`b`. Both are accepted RFCs, and the codec
   accommodates both. A binding check has to re-derive the expected DID, so
   it cannot be written until the authoritative derivation is chosen — and
   re-deriving through the codec changes the output of every signer.
2. **`verify` is public and multi-crate.** Its signature takes a bare key
   rather than a key-and-expected-DID pair, so enforcing the binding is a
   breaking change, and every caller has to be re-examined for whether it is
   verifying an Ask it minted or one it received.
3. **The wallet's derivation is a Layer A concern.** It is the sole live
   producer of marketplace identity; changing it invalidates every published
   Ask's `asker_did`.

What was done instead: both hazards are documented at the code that carries
them, so a reader of `verify` or `derive_asker_did` learns the limit without
having to rediscover it.

## What the RFC owner needs to decide

- Is the canonical DID `did:octo:z<base58btc>` (RFC-0009) or
  `did:octo:b<52-base32>` (RFC-0968 §6)? One of the two RFCs needs a
  version bump, and the codec's dual acceptance should be narrowed to match.
- Should `AskSigned::verify` take the expected DID and enforce the binding, or
  should a separate `verify_identity` be added and the bare form deprecated?
  Enforcing it in `verify` breaks callers; leaving it optional leaves the
  footgun for anyone who reaches for the obvious function.
- Is `asker_did` in the hashed payload the right place at all? It is
  currently attacker-chosen data inside the signed bytes. If identity is
  meant to be _derived_ from the key rather than _asserted_ in the payload,
  the field may be redundant and the derivation the only truth.
- Should the wallet derive the DID through `CanonicalCodec::mint` +
  `raw_to_wire` instead of hand-rolling hex? If the answer is yes, every Ask
  already published carries a non-canonical `asker_did`.

## Reproduction

```bash
# the verifier's two checks, and the absent third
grep -n -A28 'pub fn verify(' crates/quota-router-storage/src/ask.rs

# the roundtrip test whose fixture is not tied to its key
grep -n -A12 'fn sign_then_verify_roundtrip' crates/quota-router-storage/src/ask.rs
grep -n -A20 'fn sample_payload' crates/quota-router-storage/src/ask.rs

# the wallet's hand-rolled derivation
grep -n -A6 'fn derive_asker_did' crates/octo-wallet/src/bin/octo-wallet.rs

# the two forms the codec accepts, and their length rules
grep -n -A20 'fn legacy_to_wire' crates/octo-ident/src/lib.rs
grep -n -A20 'pub fn parse' crates/octo-ident/src/lib.rs

# the three specifications
grep -n 'multibase(z)' rfcs/accepted/process/0009-identity-management.md
grep -n 'base32-lowercase-no-padding' rfcs/accepted/economics/0968-reputation-registry.md

# the field that cites RFC-0009
grep -n -B2 -A2 'RFC-0009 §Identity Key Format' crates/quota-router-storage/src/ask.rs
```

## Cross-references

- RFC-0009 §Identity Key Format — the multibase-z derivation the wallet
  bypasses, and §Verification — the format check it fails
- RFC-0968 §6 — the conflicting base32-`b` specification
- RFC-0959 §Roles — the `seller_signature ≡ Asker signature` claim that
  `verify` does not enforce
- Mission `the-ask-lifecycle-state-machine-has-no-representation-in-the-ask-type`
  — the same area's structural gap: the Ask carries no state and no identity
  binding
- Mission `ask-publish-fails-with-its-own-default-nonce` — the other defect in
  the command that mints these identities
- Mission `the-marketplace-facade-has-no-production-caller` — why Finding 1 is
  latent rather than exploited today, and why that is not a reason to defer it
  further
