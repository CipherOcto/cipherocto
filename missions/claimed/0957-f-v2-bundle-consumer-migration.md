# Mission: 0957-f-v2-bundle-consumer-migration — Adopt V2 Wire at Raw-Macaroon Boundaries

## Status

open (2026-08-11).

**Substrate:** Mission `0957-f-v2-bundle` LANDED (commit `b6bc190b`)
provides `CapabilityBundleV2` + `CapabilityTokenV2` in
`crates/octo-cap-macaroon/src/bundle_v2.rs`. RFC-0009 v1.2 §Implementation
Phases Commit 5 demands consumer adoption atomic with the substrate.

## Summary

Recon (`grep -rn "CapabilityBundle" crates/`) revealed the V1
`CapabilityBundle` substrate (in `crates/octo-cap-macaroon/src/bundle.rs`)
has **zero production call sites**. The 3 sites RFC-0009 v1.2 names
(Wallet mint, Capability issuer, `octo-cap-zk`) all emit **raw
`Macaroon` bytes** directly, not bundles.

This mission therefore **wraps the raw `Macaroon` emission path at those
3 sites with `CapabilityBundleV2` envelopes** (instead of migrating V1
consumers — V1 has no consumers to migrate). V1 wire version becomes
dead code and gets removed in the same commit. Net result matches RFC
intent: production traffic carries V2 wire form with `chain_depth` +
`chain_parent` binding.

## V1 substrate disposition

`CapabilityBundle` (in `bundle.rs`) is defined + has unit TV but no
caller. Remove in this mission:

- Delete `crates/octo-cap-macaroon/src/bundle.rs` (the V1 wire struct).
- Remove `CapabilityBundle` + `BUNDLE_VERSION_V1` + `BUNDLE_ID_DOMAIN_V1`
  re-exports from `octo-cap-macaroon/src/lib.rs`.
- Delete the 11 V1 unit TV (in `bundle.rs::tests`) — they test dead
  code.

V1's `CapabilityToken` (in `crates/octo-cap-macaroon/src/token.rs`) is
NOT removed: it remains the inner token substrate (V2 wraps
`CapabilityToken` via `holder_record_bytes` indirection pattern).

## Consumer adoption scope (3 sites)

### Site 1: `octo-wallet-node` mint handler

**File:** `crates/octo-wallet-node/src/handlers/mint.rs`

**Current behavior** (per recon): `token.macaroon` is the constructed
artifact; mint handler persists macaroon via
`token.macaroon.root_id` → `SpendLedger`. No bundle envelope today.

**V2 adoption:** wrap the constructed `CapabilityToken` +
`HolderRecord` into a `CapabilityBundleV2` at the mint boundary:

```rust
use octo_cap_macaroon::{
    CapabilityBundleV2, CapabilityTokenV2,
    BUNDLE_VERSION_V2, MAX_CHAIN_DEPTH,
};

let bundle = CapabilityBundleV2::new(
    CapabilityTokenV2::new_root(
        token.clone(),                  // V1 CapabilityToken substrate
        audience_did.clone(),
        channel_id,
        expires_at_unix_secs,
        issuer_did.clone(),
    )?,
    holder_record_bytes,
    discharge_macaroon_bytes,
)?;
```

The minted `CapabilityBundleV2` is the canonical post-mint
representation returned to the wallet caller + persisted in the
`SpendLedger` (via `macaroon_id` indirection — `SpendLedger` keys by
macaroon_id, not by full bundle bytes).

### Site 2: `octo-capability-issuer-node` issue handler

**File:** `crates/octo-capability-issuer-node/src/handlers/issue.rs`

**Current behavior** (per recon line 70): the issue handler constructs
a `Macaroon` directly via `octo_cap_macaroon::macaroon_id` and returns
it as the wire form.

**V2 adoption:** wrap the issued `Macaroon` in a `CapabilityBundleV2`
at the issue boundary. Issuer becomes the canonical V2 mint point —
the bundle carries `chain_depth = 0` (root), `chain_parent = [0; 32]`
(BLAKE3 null binding for root). Attenuation chains grow depth by 1
per hop (per RFC-0009 v1.2 §Chain Attenuation).

### Site 3: `octo-cap-zk` proof bundling

**File:** `crates/octo-cap-zk/src/lib.rs`

**Current behavior** (per recon line 601): `ProofBundle::from_bytes`
round-trips `canonical_ser` bytes for the AC-3 zk-circuit rewrite.

**V2 adoption:** `ProofBundle` gains an optional `capability_v2:
Option<CapabilityBundleV2>` field carrying the underlying capability
the proof attests to. `ProofBundle::from_bytes` accepts both V1 (raw
macaroon bytes) AND V2 (`b"cipherocto/bundle/v2" || bundle_id ||
borsh_bundle_bytes`) wire forms, dispatching via a 16-byte prefix
discriminator (`CIPHEROCTO_V2_BUNDLE_PREFIX`). The ZK circuit
constraints accept V2 `chain_depth` as a public input.

## Wire form discriminator

To avoid V1/V2 ambiguity, V2 wire form on the network prepends a
fixed 16-byte prefix:

```rust
pub const CIPHEROCTO_V2_BUNDLE_PREFIX: &[u8; 16] =
    b"cipherocto/v2\x00\x00\x00";
```

Rationale: matches the RFC-0009 v1.2 §Forward compatibility
requirement (any receiver detects version from first 16 bytes without
needing to attempt full canonical_de). Prefix is borsh-encoded
alongside the bundle bytes (NOT a separate envelope — the prefix is
the first 16 bytes of the borsh canonical_ser output, achieved by
adding a `prefix: [u8; 16]` field at the start of a top-level
`CapabilityBundleV2Envelope` wrapper struct).

## `CapabilityBundleV2Envelope` (NEW)

To carry the prefix without modifying `CapabilityBundleV2` substrate,
add a thin envelope wrapper in `octo-cap-macaroon/src/bundle_v2.rs`:

```rust
pub const CIPHEROCTO_V2_BUNDLE_PREFIX: &[u8; 16] =
    b"cipherocto/v2\x00\x00\x00";

#[derive(BorshSerialize, BorshDeserialize)]
pub struct CapabilityBundleV2Envelope {
    pub prefix: [u8; 16],  // must equal CIPHEROCTO_V2_BUNDLE_PREFIX
    pub bundle: CapabilityBundleV2,
}

impl CapabilityBundleV2Envelope {
    pub fn wrap(bundle: CapabilityBundleV2) -> Self { ... }
    pub fn canonical_ser(&self) -> Vec<u8> { ... }
    pub fn canonical_de(bytes: &[u8]) -> Result<Self, BundleV2Error> { ... }
}
```

`CapabilityBundleV2` substrate stays clean (no prefix field);
`envelope` is the wire carrier.

## Acceptance Criteria

- [ ] `CapabilityBundleV2Envelope` struct in
  `crates/octo-cap-macaroon/src/bundle_v2.rs` with
  `CIPHEROCTO_V2_BUNDLE_PREFIX` + `wrap` + `canonical_ser` +
  `canonical_de`.
- [ ] `octo-wallet-node` mint handler emits
  `CapabilityBundleV2Envelope::canonical_ser()` bytes (not raw
  `Macaroon`).
- [ ] `octo-capability-issuer-node` issue handler emits
  `CapabilityBundleV2Envelope` (root: `chain_depth = 0`,
  `chain_parent = [0; 32]`).
- [ ] `octo-cap-zk::ProofBundle` gains
  `capability_v2: Option<CapabilityBundleV2>` field; `from_bytes`
  dispatch via 16-byte prefix.
- [ ] V1 `CapabilityBundle` deleted (file + re-exports + 11 unit TV).
- [ ] V2 envelope unit TV (new): `v2_envelope_prefix_is_16_bytes`,
  `v2_envelope_canonical_ser_roundtrip`, `v2_envelope_rejects_wrong_prefix`,
  `v2_envelope_rejects_truncated_bytes`,
  `v2_envelope_legacy_v1_bytes_fail_decode` (V1 raw macaroon bytes do
  NOT parse as V2 envelope — discriminator catches them).
- [ ] Integration TV: `octo-wallet-node` mint end-to-end produces
  envelope bytes verifiable by `octo-cap-zk::ProofBundle::from_bytes`.
- [ ] All existing 188 `octo-cap-macaroon` lib tests still pass.
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
  passes across all 4 touched crates (`octo-cap-macaroon`,
  `octo-wallet-node`, `octo-capability-issuer-node`, `octo-cap-zk`).
- [ ] `cargo fmt --all -- --check` passes.

## Implementation Guide

1. Add `CapabilityBundleV2Envelope` + `CIPHEROCTO_V2_BUNDLE_PREFIX` in
   `octo-cap-macaroon/src/bundle_v2.rs`.
2. Add 5 envelope unit TV.
3. Migrate `octo-wallet-node/src/handlers/mint.rs`: import
   `CapabilityBundleV2`, `CapabilityBundleV2Envelope`,
   `CapabilityTokenV2`; wrap mint output in envelope.
4. Migrate `octo-capability-issuer-node/src/handlers/issue.rs`:
   wrap issued macaroon in envelope (root bundle).
5. Migrate `octo-cap-zk/src/lib.rs`: add `capability_v2` field to
   `ProofBundle`; teach `from_bytes` the prefix discriminator.
6. Delete `crates/octo-cap-macaroon/src/bundle.rs` (V1 substrate +
   11 unit TV).
7. Strip V1 re-exports from `octo-cap-macaroon/src/lib.rs`.
8. Run full test + clippy + fmt sweep across 4 crates.
9. Add 1 integration TV: wallet mint → octo-cap-zk verify path.

## Atomic commit guarantee

This mission lands in ONE commit per RFC-0009 v1.2 §Implementation
Phases Commit 5 ("Consumers migrated in SAME commit as V2 wire form").
The commit is the second half of the V2 bundle series — the first
half (substrate only) already landed in commit `b6bc190b`.

## Cross-references

- RFC-0009 v1.2 §Implementation Phases Commit 5 — atomic consumer
  adoption rule
- Mission `0957-f-v2-bundle` (commit `b6bc190b`) — substrate source
- Mission `0957-phase1-fixture-author` — R8 H1 fixture owner (V2
  envelope fixtures land here in follow-on mission)
- `crates/octo-cap-macaroon/src/bundle.rs` — V1 substrate to DELETE
- `crates/octo-cap-macaroon/src/bundle_v2.rs` — V2 substrate source
- `crates/octo-wallet-node/src/handlers/mint.rs` — Site 1
- `crates/octo-capability-issuer-node/src/handlers/issue.rs` — Site 2
- `crates/octo-cap-zk/src/lib.rs` — Site 3

## Version History

| Version | Date       | Status | Changes |
| ------- | ---------- | ------ | ------- |
| v0.1    | 2026-08-11 | open   | Filed after recon revealed V1 substrate is dead; scope pivoted from "V1→V2 migration" to "introduce V2 envelope at raw-Macaroon boundaries + delete V1 substrate" |