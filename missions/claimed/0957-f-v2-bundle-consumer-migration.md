# Mission: 0957-f-v2-bundle-consumer-migration — Adopt V2 Wire at Raw-Macaroon Boundaries

## Status

Landed in part (2026-08-11). Sites 1 and 2 shipped; **site 3 did not**, and the
gap between what this file specified and what the code does is recorded below.

The status line this section previously carried said `open` while the file sat in
`missions/claimed/`. Both were wrong in the same direction — it read as work in
progress when most of it had already shipped, which is how the stale claims below
survived long enough to be copied into `octo-cap-zk`'s field documentation.

| Part                               | State                                                                  |
| ---------------------------------- | ---------------------------------------------------------------------- |
| V1 substrate removal (`bundle.rs`) | landed — the file and all three V1 symbols are gone                    |
| Site 1 — `octo-wallet-node` mint   | landed                                                                 |
| Site 2 — capability issuer         | landed                                                                 |
| Site 3 — `capability_v2` field     | the **field** landed; the **circuit** it was specified to feed did not |

The unbuilt part of site 3 is tracked in mission `0009-ci`. The `chain_parent`
binding site 3 inherited but nobody computes is `0009-cf`.

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
dead code and gets removed in the same commit.

**Correction to the original wording of this section.** It concluded: "Net
result matches RFC intent: production traffic carries V2 wire form with
`chain_depth` + `chain_parent` binding." Production traffic does carry the
V2 wire form. It does **not** carry a `chain_parent` binding — that value
is never computed by anything and never checked by anything, and every
production mint writes the zero value. The field travels as reserved
space. See `0009-cf`.

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

**Current behavior** (per recon): the issue handler constructs
a `Macaroon` directly via `octo_cap_macaroon::macaroon_id` and returns
it as the wire form.

**V2 adoption:** wrap the issued `Macaroon` in a `CapabilityBundleV2`
at the issue boundary. Issuer becomes the canonical V2 mint point —
the bundle carries `chain_depth = 0` (root), `chain_parent = [0; 32]`
(BLAKE3 null binding for root). Attenuation chains grow depth by 1
per hop (per RFC-0009 v1.2 §Chain Attenuation).

### Site 3: `octo-cap-zk` proof bundling

**File:** `crates/octo-cap-zk/src/lib.rs`

**Current behavior** (per recon): `ProofBundle::from_bytes`
round-trips `canonical_ser` bytes for the AC-3 zk-circuit rewrite.

**V2 adoption:** `ProofBundle` gains an optional `capability_v2:
Option<CapabilityBundleV2>` field carrying the underlying capability
the proof attests to.

**Two parts of this section never shipped. Both are recorded here because this
file is where the stale version of the claim originated, and it was copied
verbatim into the `capability_v2` field documentation before anyone checked it.**

1. **"The ZK circuit constraints accept V2 `chain_depth` as a public input."**
   They do not. `PublicInputs` has no `chain_depth` field, no `chain_parent`
   field, and no `issuer_did` field, and no source in `zk-circuit`, `zk-verifier`
   or `zk-vendor` mentions any of them. The field carries the bundle as data and
   does not affect what the proof proves. Tracked as `0009-ci`.

   The likely reason this survived is a near-miss: `cairo/src/lib.cairo` defines
   `CHAIN_DEPTH = 3`, which is the **caveat** chain depth and is unrelated.
   Grepping `chain_depth` across the circuit returns that constant, which looks
   like confirmation and is not.

2. **"`ProofBundle::from_bytes` accepts both V1 and V2 wire forms, dispatching via
   a 16-byte prefix discriminator (`CIPHEROCTO_V2_BUNDLE_PREFIX`)."** There is
   no `from_bytes` on `ProofBundle`. The prefix discriminator does exist and is
   real, but it lives on `CapabilityBundleV2Envelope` in `octo-cap-macaroon`,
   which validates the prefix on decode — a different mechanism serving a
   different purpose. This section also gives the prefix as
   `b"cipherocto/bundle/v2"`; the implemented constant is
   `b"cipherocto/v2\x00\x00\x00"`.

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

Checked against the tree rather than left as written. `from_bytes` does not exist,
so the two criteria that depend on it could not have been met by any commit.

- [x] `CapabilityBundleV2Envelope` struct in
      `crates/octo-cap-macaroon/src/bundle_v2.rs` with
      `CIPHEROCTO_V2_BUNDLE_PREFIX` + `canonical_ser` + `canonical_de`.
      The constructor landed as `new`, not `wrap` as specified here.
- [x] `octo-wallet-node` mint handler emits
      `CapabilityBundleV2Envelope::canonical_ser()` bytes (not raw
      `Macaroon`).
- [x] `octo-capability-issuer-node` issue handler emits
      `CapabilityBundleV2Envelope` (root: `chain_depth = 0`,
      `chain_parent = [0; 32]`).
- [~] `octo-cap-zk::ProofBundle` gains
  `capability_v2: Option<CapabilityBundleV2>` field — **landed**.
  `from_bytes` dispatch via 16-byte prefix — **not built, and not
  buildable as written because `from_bytes` does not exist.**
  Unbuilt part carried to `0009-ci`.
- [x] V1 `CapabilityBundle` deleted (file + re-exports + 11 unit TV).
- [x] V2 envelope unit TV (new): `v2_envelope_prefix_is_16_bytes`,
      `v2_envelope_canonical_ser_roundtrip`, `v2_envelope_rejects_wrong_prefix`,
      `v2_envelope_rejects_truncated_bytes`,
      `v2_envelope_legacy_v1_bytes_fail_decode` (V1 raw macaroon bytes do
      NOT parse as V2 envelope — discriminator catches them).
      All five exist under the exact names given here.
- [ ] Integration TV: `octo-wallet-node` mint end-to-end produces
      envelope bytes verifiable by `octo-cap-zk::ProofBundle::from_bytes`.
      **Not satisfiable** — depends on a function that was never written. The
      envelope decodes through `CapabilityBundleV2Envelope::canonical_de`, not
      through `ProofBundle`.
- [x] All existing `octo-cap-macaroon` lib tests still pass. (The count
      written here, 188, is stale; the suite has since grown to 415.)
- [x] `cargo clippy --all-targets --all-features -- -D warnings`
      passes across all 4 touched crates (`octo-cap-macaroon`,
      `octo-wallet-node`, `octo-capability-issuer-node`, `octo-cap-zk`).
- [x] `cargo fmt --all -- --check` passes.

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

| Version | Date       | Status  | Changes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| ------- | ---------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.2    | 2026-10-12 | partial | Reconciled the file against the tree. Status said `open` while the file sat in `claimed/` and sites 1-2 had shipped; corrected to landed-in-part. Two claims here never shipped and had propagated: the circuit was specified to accept V2 `chain_depth` as a public input and does not (`0009-ci`), and `ProofBundle::from_bytes` was specified as the V1/V2 prefix dispatcher and does not exist, making the integration-TV criterion unsatisfiable as written. Also corrected the "chain_parent binding" conclusion, the prefix literal (`b"cipherocto/bundle/v2"` specified vs `b"cipherocto/v2\x00\x00\x00"` implemented), the constructor name (`wrap` specified, `new` implemented), and the stale 188-test count. |
| v0.1    | 2026-08-11 | open    | Filed after recon revealed V1 substrate is dead; scope pivoted from "V1→V2 migration" to "introduce V2 envelope at raw-Macaroon boundaries + delete V1 substrate"                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
