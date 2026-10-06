# RFC-0972 (Economics): Capability `root_id` Derivation Reconciliation (Amendment)

## Status

Draft

> **Note:** This is an **in-place amendment** to RFC-0957. It does NOT renumber.
> It resolves the spec-versus-implementation drift in the `root_id` derivation
> documented at `docs/audits/2026-10-04-red-team-security-audit.md` (finding
> L-12). It does **not** change the macaroon chain, the Ed25519 holder
> signature, the discharge protocol, or the `HolderRecord` schema.

## Authors

- Author: @mmacedoeu
- Contributor: @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu

## Summary

RFC-0957 §Mint specifies

```
root_id = blake3::keyed_hash(root_secret,
                             "cipherocto/macaroon/v1:" + hex(nonce))[..16]
```

but the implementation is

```
root_id = blake3::keyed_hash(root_secret, nonce)[..16]
```

The implementation has been in the second form since the capability substrate
was first written, and the drift is self-documented at
`crates/octo-cap-macaroon/src/lib.rs` (`macaroon_id`), which notes that the raw
form was chosen deliberately for byte-identical compatibility and that "a future
audit mission will reconcile the algorithm-versus-implementation drift."

This RFC makes that reconciliation explicit. **The spec is amended to match the
implementation.** The rationale is in §Decision; the short version is that
changing the _implementation_ to match the spec would be a wire-breaking change
to every capability ever minted, in exchange for domain separation that is
already provided at the neighbouring derivation.

## Motivation

The two derivations in the mint path must not collide:

- `root_id = macaroon_id(root_secret, nonce)` — the replay-dedup identifier
- `chain[0] = HMAC(root_secret, MACAR_ID_DOMAIN || nonce)` — the chain anchor

The chain derivation already applies `MACAR_ID_DOMAIN`. The id derivation does
not. Because both hash a key-derived value under the same key with the same
message material, domain separation matters at this boundary.

## Decision

**Amend RFC-0957 §Mint to specify the implemented form**, and record the domain
separation that the id derivation _does_ have and the chain derivation _does
not_, rather than introducing it after the fact.

```rust
/// `root_id = BLAKE3-keyed(root_secret, nonce)[..16]`
///
/// The nonce is 16 bytes of CSPRNG output per mint and is NOT retained
/// (see §Nonce Handling — its unrecoverability is the replay defence).
#[must_use]
pub fn macaroon_id(root_secret: &[u8; 32], nonce: &[u8; 16]) -> MacaroonId {
    let mac = hmac_blake3(root_secret, nonce);
    let mut id = [0u8; 16];
    id.copy_from_slice(&mac[..16]);
    id
}
```

### Consequences

1. **No wire change.** `root_id` values are unchanged. No capability minted under
   either the spec or the implementation is affected, because they never agreed
   in the first place — only the implementation was ever live.
2. **No `HolderRecord` change.** `cap_root_hash` is derived from the whole
   canonical macaroon serialization (`compute_capability_id`), which is
   unaffected.
3. **Domain separation is recorded, not retrofitted.** The chain derivation keeps
   `MACAR_ID_DOMAIN`; the id derivation does not have and will not gain one.

### Alternative considered and rejected

Amend the implementation to match the spec
(`keyed_hash(root_secret, "cipherocto/macaroon/v1:" + hex(nonce))`). Rejected:
it changes `root_id` for every future mint while the current form is
byte-compatible with all existing persisted rows, which makes the migration
strictly costlier for no verified benefit — the two derivations are already
separated by their distinct downstream use (`root_id` is a dedup key; `chain[0]`
is a verification anchor).

## Nonce Handling

**The mint-time nonce is not retained, and this amendment does not change that.**
It is load-bearing, not an omission.

RFC-0957 §Replay Protection specifies that "per-mint nonce: `root_id` includes
CSPRNG nonce; replays with same `root_id` detected by verifier-side dedup." The
nonce supplies the 128 bits of entropy that make two mints under the same
`root_secret` distinguishable, and `root_id` is the key dedup operates on.

Because `root_id` is a deterministic function of the nonce, **a recoverable
nonce would let any holder of a token regenerate its `root_id` and defeat
verifier-side dedup.** The nonce's unrecoverability is therefore the mechanism,
not a missing field. Storing it is explicitly out of scope for this amendment
and would convert a security control into an attack surface.

RFC-0853 §Key Derivation independently constrains this area: consensus nonce
generation is Class A (deterministic derivation required), with OS randomness
permitted only for Class C (non-consensus). Minting is local and non-consensus,
so the CSPRNG nonce at mint is correctly Class C; the _identifier derived from
it_ is what crosses the consensus boundary via gossip, and is treated as opaque.

## Acceptance Criteria

- [ ] RFC-0957 §Mint updated to the implemented `root_id` formula.
- [ ] The byte-pinning guard remains green
      (`l12_macaroon_id_derivation_is_pinned`,
      `crates/octo-cli/tests/security_regressions.rs`) — the wire form does not
      move, so the guard must not change.
- [ ] No `HolderRecord`, `StoolapHolderRegistry`, or `StoolapSpendLedger` schema
      change.
- [ ] The `MACAR_ID_DOMAIN` asymmetry between the chain and the id is documented
      in code as deliberate, not pending.

## References

- RFC-0957 — Capability Token Format (§Mint, §Replay Protection, §Adversary
  Analysis)
- RFC-0957-A1 — Holder Registry + Catalog Storage (gossip replication of
  `HolderRecord`)
- RFC-0853 — Overlay Cryptography (§Key Derivation, nonce class table)
- RFC-0009 — Identity Evolution (§Identity Key Format)
