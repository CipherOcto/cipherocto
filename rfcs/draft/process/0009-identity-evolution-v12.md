# RFC-0009 v1.2 — Identity Evolution: Hierarchical Attenuation + MPC Threshold

**Status:** Draft (2026-08-10)
**Author:** @cipherocto + @mmacedoeu
**Substrate:** RFC-0009 §Capability Keys + §HsmAdapter Integration
**Parent:** `missions/open/0009-v12-identity-evolution.md`

> **Promotion note:** In-place additive amendment to RFC-0009. This
> RFC ratifies two capability-key extensions originally listed as
> §Future Work items: "Capability attenuation protocols beyond
> pairwise" (parent → child → grandchild chains) and "MPC threshold
> identity" (RFC-0853 §F3). Both feed the 0957-f F4 V2 bundling
> requirement (mission `0957-f-future-work.md` Band B).

## Summary

Extend RFC-0009 §Capability Keys with:

1. **Hierarchical attenuation chains** — a downstream capability
   holder derives its capability key as a child of the parent's
   capability key, not as a child of the root identity. The chain
   depth is bounded (≤ 8 levels per W3C VC-DID best practice).
   Revocation at any level cascades to all descendants.
2. **MPC threshold identity** — `IdentityKey::sign` routes through
   a `ThresholdSigner: HsmAdapter` impl that splits the signing
   operation across M-of-N key shares (e.g., 2-of-3, 3-of-5).
   Reconciles the gap between `HsmAdapter` (single-key abstraction)
   and `Authorization::ThresholdSignature` (RFC-0871 §Future Work).

## Why Now

0957-f F4 V2 bundling (per mission `missions/claimed/0957-f-future-work.md`
Band B) requires capability V2 to embed the full attenuation chain
as a witness. Without RFC-0009 v1.2, the V2 wire form has no
canonical place to derive the child capability key from the parent —
every consumer re-implements the derivation. RFC-0009 v1.2
centralizes the derivation in `octo-wallet` so the wire form has a
canonical contract.

## Specification

### Hierarchical attenuation chains

`derive_capability_key` (RFC-0009 §Capability Keys) gains a third
parameter: the parent capability key. New signature:

```rust
pub fn derive_capability_key(
    identity_key: &IdentityKey,
    audience_did: &DID,
    channel_id: &ChannelId,
    parent_cap_key: Option<&CapabilityKey>,  // NEW: None = root derivation
) -> CapabilityKey;
```

Derivation:

- **Root derivation** (`parent_cap_key = None`): same as v1.1 — HKDF-BLAKE3 with `salt = identity_seed`, `ikm = audience_did`, `info = "cipherocto/cap/v1/" + channel_id`.
- **Child derivation** (`parent_cap_key = Some(key)`): HKDF-BLAKE3 with `salt = parent_cap_key`, `ikm = audience_did`, `info = "cipherocto/cap/v2/child/" + parent_chain_depth_be_bytes`.

The info-string version bump (`v1/` → `v2/child/`) ensures root and
child derivations are unlinkable across versions. A V2 capability
token's `chain_depth` field (new per 0957-f F4) carries the depth
counter; depth > 8 returns `CapabilityError::ChainTooDeep`.

### MPC threshold identity

`HsmAdapter` trait (RFC-0009 §HsmAdapter Integration) gains a
`ThresholdSigner` supertrait:

```rust
pub trait ThresholdSigner: HsmAdapter {
    /// M-of-N threshold signing: collect `threshold` shares,
    /// aggregate via BLS or Schnorr (per key scheme), return
    /// aggregated signature.
    fn threshold_sign(
        &self,
        msg: &[u8],
        shares: Vec<[u8; 32]>,  // M shares out of N
    ) -> Result<[u8; 64], HsmError>;

    /// Number of shares required (M) and total (N).
    fn threshold_params(&self) -> (usize, usize);
}
```

Concrete impls:

- **`BLS12381ThresholdSigner`** — BLS signature aggregation over
  BN254 curve. M-of-N key generation via `blst::SecretKey::keygen`
  - Shamir secret sharing. Aggregation via `blst::AggregateSignature::aggregate`.
- **`SchnorrThresholdSigner`** — FROST-style Schnorr threshold
  signing over Ed25519. M-of-N key generation via
  `frost_ed25519::keygen`. Signing per `frost_ed25519::sign`.

IdentityKey routing (RFC-0009 §HsmAdapter Integration) MUST prefer
`ThresholdSigner` when `self.threshold_params() != (1, 1)`. The
existing `InMemorySigner` continues to satisfy `HsmAdapter` only;
production `LedgerSigner` (when M-of-N configured) satisfies both.

### §Future Work reconciliation

- "Capability attenuation protocols beyond pairwise" — closed in
  v1.2 (this RFC). The "parent → child → grandchild with
  revocation at any level" requirement is satisfied by the chain
  derivation + cascading revocation contract.
- "MPC threshold identity (Phase I)" — closed in v1.2 (this RFC).
  RFC-0853 §F3 promoted from §Future Work to §Specification via
  this amendment.

### Backward compatibility

- `derive_capability_key` signature change: existing v1.1 callers
  MUST be updated to pass `parent_cap_key = None`. The wallet's
  primary mint path (root derivation) is unchanged at the call
  site — `octo-wallet/src/capability.rs::mint_root_capability_key`
  passes `None`.
- `HsmAdapter` trait is unchanged. `ThresholdSigner` is a NEW
  supertrait; existing impls continue to satisfy `HsmAdapter`.
- HKDF info-string bump (`v1/` → `v2/child/`) means v1.1-derived
  capability keys remain valid (old derivation still works); new
  child derivations use v2 info string.

## Test Vectors (preview)

- 6 new TV: root-vs-child-distinct-keys (unlinkability); chain-
  depth-bounded-at-8; cascading-revocation-kills-descendants;
  bls-threshold-2-of-3-signs-aggregates; schnorr-threshold-3-of-5-
  signs-aggregates; threshold-key-share-loss-tolerated.

## Layer direction

- `octo-wallet` (Layer B) — `derive_capability_key` signature +
  `ThresholdSigner` supertrait + new impls
- `octo-protocol` (Layer A) — `Authorization::ThresholdSignature`
  variant (already exists per RFC-0871 §Future Work; ratified in
  this RFC)
- `octo-cap-macaroon` (Layer E) — V2 wire form embeds `chain_depth`

## Validation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --lib
```

## Cross-references

- [[rfc-0010-v13-storage-extension]] — sister storage trait extension
- [[mission-0957-f-future-work]] — F4 V2 bundling requirement (this
  RFC is its gating substrate)
- [[mission-0871b-storage-backend]] — sister storage substrate
- [[cipherocto-design-principles]] — Layer A additive-only rule

## Version History

| Version | Date       | Status               | Changes                                           |
| ------- | ---------- | -------------------- | ------------------------------------------------- |
| 1.0     | 2026-07-19 | Accepted             | Initial specification                             |
| 1.1     | 2026-08-08 | Accepted (amendment) | HSM routing + wallet audience validation          |
| 1.2     | 2026-08-10 | Draft                | Hierarchical attenuation + MPC threshold identity |

## Review Process

Multi-round adversarial review per BLUEPRINT §RFC Process. R1
expected 2026-08-11+. Convergence target: R3.
