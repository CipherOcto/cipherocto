# 0009-v12-identity-evolution — Hierarchical attenuation + MPC threshold identity

**Status:** unassigned (wave 5; absorbed from RFC-0009 §Future Work on 2026-08-10)
**Substrate:** RFC-0009 §Capability Keys + §HsmAdapter Integration
**Parent:** RFC-0009 §Future Work + mission `missions/claimed/0957-f-future-work.md` Band B

## Scope

Implement RFC-0009 v1.2 (in-place amendment; draft landed
`rfcs/draft/process/0009-identity-evolution-v12.md`):

1. **`derive_capability_key` extension** — gain `parent_cap_key:
Option<&CapabilityKey>` parameter; child derivation uses v2 info
   string `cipherocto/cap/v2/child/<depth>`; depth bounded ≤ 8.
2. **`ThresholdSigner` supertrait** — new on top of `HsmAdapter`;
   `threshold_sign(msg, shares) -> Signature` + `threshold_params()
-> (M, N)`. Two concrete impls: `BLS12381ThresholdSigner` +
   `SchnorrThresholdSigner`.
3. **Cascading revocation** — revocation at any attenuation level
   invalidates all descendants. Implemented via `chain_depth` +
   `chain_parent` fields on `CapabilityToken` (V2 wire form per
   mission `0957-f-future-work.md` Band B).
4. **IdentityKey routing preference** — when `ThresholdSigner`
   available, prefer threshold signing; existing `InMemorySigner`
   fallback unchanged.

### Why this is the gating substrate for 0957-f F4 V2 bundling

Per `missions/claimed/0957-f-future-work.md` Band B, F4 V2 bundling
requires the capability V2 wire form to embed the attenuation chain
as a witness. Without RFC-0009 v1.2, the V2 wire form has no
canonical place to derive the child capability key from the parent
— every consumer re-implements the derivation. Centralizing in
`octo-wallet` makes the wire form canonical.

### Mission scope (4 sub-steps)

1. **`derive_capability_key` signature** — `crates/octo-wallet/src/capability.rs`.
   Add `parent_cap_key: Option<&CapabilityKey>` parameter. Root path
   unchanged at the call site; child path uses v2 info string.
   Add `chain_depth: u8` parameter; `depth > 8` returns
   `CapabilityError::ChainTooDeep`.

2. **`ThresholdSigner` supertrait** — `crates/octo-wallet/src/threshold.rs`.
   New file. `pub trait ThresholdSigner: HsmAdapter { ... }`. Two
   impls in same file: `BLS12381ThresholdSigner` (uses `blst` crate)
   - `SchnorrThresholdSigner` (uses `frost-ed25519` crate).

3. **Cascading revocation** — `crates/octo-cap-macaroon/src/macaroon.rs`.
   `check_wrapped_chain` (existing) extended to enforce `chain_depth`
   bound + `chain_parent` lookup. Revocation of parent invalidates
   all children with `chain_parent == revoked_root_hash`.

4. **IdentityKey routing** — `crates/octo-wallet/src/identity.rs`.
   `IdentityKey::sign` checks `self.signer.as_ref().downcast_ref::<dyn ThresholdSigner>()`
   first; if threshold params `(M, N)` satisfy `M > 1`, use
   `threshold_sign`; else fall back to `HsmAdapter::sign`.

### Cargo deps

- `blst` (BLS12-381 signature aggregation)
- `frost-ed25519` (FROST threshold Ed25519)
- `zeroize` (already in deps for seed handling)

## Test Vectors (per RFC-0009 v1.2 §Test Vectors)

- 6 new TV:
  - `root_vs_child_distinct_keys` — root derivation + child
    derivation produce different keys for same `(audience, channel)`
  - `chain_depth_bounded_at_8` — depth=9 returns `ChainTooDeep`
  - `cascading_revocation_kills_descendants` — revoke parent →
    child resolve returns revoked
  - `bls_threshold_2_of_3_signs_aggregates` — 2 of 3 BLS shares
    aggregate to valid signature; 1 of 3 fails
  - `schnorr_threshold_3_of_5_signs_aggregates` — 3 of 5 FROST
    shares aggregate to valid signature; 2 of 5 fails
  - `threshold_key_share_loss_tolerated` — lose 1 of N shares,
    signing continues (within threshold)

## Layer direction (per [[cipherocto-design-principles]])

- `octo-wallet` (Layer B) — `derive_capability_key` extension +
  `ThresholdSigner` supertrait + impls
- `octo-protocol` (Layer A) — `Authorization::ThresholdSignature`
  variant (already exists per RFC-0871 §Future Work; ratified in
  RFC-0009 v1.2)
- `octo-cap-macaroon` (Layer E) — V2 wire form + cascading revocation

## Validation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --lib
```

## Backward compat

- `derive_capability_key` signature change: v1.1 callers update to
  pass `parent_cap_key = None`. Root mint path unchanged at call site.
- `HsmAdapter` trait unchanged. `ThresholdSigner` is new supertrait;
  existing impls unaffected.
- HKDF info-string bump (`v1/` → `v2/child/`) preserves backward
  derivation (root keys from v1.1 still derivable); new child
  derivations use v2.

## Cross-references

- [[rfc-0010-v13-storage-extension]] — sister storage trait extension
- [[mission-0957-f-future-work]] — F4 V2 bundling requirement
- [[cipherocto-design-principles]] — Layer A additive-only rule
- [[mission-gap-closure-priorities-2026-08-10]] — memory context

## Claimant

@unassigned

## Pull Request

#
