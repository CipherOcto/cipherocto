---
name: mission-0957-g-verify-time-invariant-status
description: S5 verify-time invariant (RFC-0957 §20.6.1) — VaultLookup trait + Macaroon::verify_for_vault_op + WrappedOnly chain Vault guard + NodeEnvelope.version_tag + TV-C1 4 fixtures LANDED 2026-08-17
metadata:
  type: project
---

# S5 — C.2 verify-time invariant LANDED 2026-08-17

Mission `0957-g-verify-time-invariant` closed. Closes the load-bearing
half of the 14-RFC storage restructure plan §S5 (verify-time invariant).
Substrate half (OctoVaultLookup glue crate) deferred to **S5.1**.

## Pillars LANDED

### Pillar 1 — `NodeEnvelope.version_tag` field (RFC-0870 §14.1)

- `crates/octo-protocol/src/envelope.rs` — added `pub version_tag: u8`
  field + `VERSION_TAG_V1 = 0xA0` / `VERSION_TAG_V2 = 0xA1` constants.
- `NodeEnvelope::build` now requires 8 args; rejects unknown
  `version_tag` with `ProtocolError::UnsupportedVersion(v)`.
- `verify_version()` helper exposes runtime gate for incoming receipts.
- `#[allow(clippy::too_many_arguments)]` on `build` — RFC-0871 §14.1
  pins wire-form parameter ordering; comment names the rule source.
- All `NodeEnvelope::build` callsites updated (8 octo-protocol/src,
  8 octo-protocol/tests/tv*, 4 downstream node.rs files, 7
  cross_node_chain.rs test fns, 1 wallet-node). Total 28+ callsites.

### Pillar 2 step 1 — `VaultLookup` trait in octo-cap-macaroon

- `crates/octo-cap-macaroon/src/vault_lookup.rs` (new, 156 lines) —
  `pub struct VaultRowSnapshot { chain_id: [u8; 32], is_active: bool }`
  - `pub trait VaultLookup: Send + Sync` + `VaultLookupExt::require_vault`.
- **Layer model**: primitive `bool` for `is_active` (NOT `octo_vault::
VaultState` enum). Avoids Layer B → Layer B reverse dep — matches
  the existing `CapabilityCatalog` pattern.
- 4 unit tests for `InMemoryLookup` hit/miss/require_vault.

### Pillar 2 step 3 — `Macaroon::verify_for_vault_op` method

- `crates/octo-cap-macaroon/src/macaroon.rs` — new verify method
  implementing the review doc §20.6.1 4-step algorithm:
  1. Signature verify via `verify_full`
  2. Vault row lookup via `VaultLookup`
  3. Chain match (`vault.chain_id == op_chain`)
  4. State check (`vault.is_active == true`)
  5. WrappedOnly chain walk: at least one ancestor must carry
     `Caveat::Vault(vault_id)`; chainless parent → `WrappedChainHasNoVault`
- `MacaroonError` gained `WrappedChainHasNoVault` variant +
  `#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]` (was only
  Debug+Error — needed for `VaultVerifyError` wrapping).
- New error type `VaultVerifyError` in
  `crates/octo-cap-macaroon/src/vault_verify_error.rs` (new, 51 lines)
  with variants: `VaultRowMissing`, `ChainMismatch`, `VaultNotActive`,
  `WrappedChainHasNoVault`, `Macaroon(MacaroonError)`.
- **Architectural correction**: verify surface is `Macaroon.caveats` NOT
  `CapabilityTokenV2.caveats` (discovered mid-implementation). Mission
  YAML AC #7 still says `CapabilityTokenV2::verify_vault_bound` —
  doc-bug, correction filed in this session.

### Pillar 3 — `WrappedOnly` parent-no-Vault reject in `verify_full`

- `macaroon.rs::verify_full` (existing path) — `WrappedOnly` chain walk
  now aborts with `MacaroonError::WrappedChainHasNoVault` if no ancestor
  carries `Caveat::Vault`. Distinct from the `verify_for_vault_op`
  path because `verify_full` does NOT have a `VaultLookup` (it doesn't
  know the target chain). The split is correct: `verify_full` enforces
  the structural invariant (chain must contain Vault); `verify_for_vault_op`
  enforces the full operational invariant (vault exists + chain matches
  - state=Active).

### TV-C1 fixtures — 4 byte-exact vectors + tests

- `crates/octo-cap-macaroon/tests/tv_c1_verify_time.rs` (new, ~400 lines)
  — 7 tests pass:
  - `tv_c1_01` Vault caveat + lookup hit + chain match → Ok
  - `tv_c1_02` Vault caveat + lookup miss → `VaultRowMissing`
  - `tv_c1_03` WrappedOnly chain WITH parent Vault → Ok
  - `tv_c1_04` WrappedOnly chain WITHOUT parent Vault → `WrappedChainHasNoVault`
  - 3 regression tests: frozen vault, chain mismatch, wrong root secret
- Test catalog + lookup stand-ins (TestCatalog / TestVaultLookup) —
  `InMemoryCatalog` / `InMemoryLookup` are `#[cfg(test)]`-gated in
  source modules, NOT visible to integration tests.
- All inputs byte-pinned (`TV_C1_*` constants); no RNG.

## Verify gate (this session)

- `cargo test -p octo-protocol --tests` → all 8 tv files pass
- `cargo test -p octo-cap-macaroon --tests` → 11 bundle_v2 + 7 tv_c1 pass
- `cargo test --workspace --lib` → all S5-touched crates pass;
  3 quota-router-cli failures PRE-EXISTING (S4 DFP Round 2), unrelated
- `cargo clippy --workspace --all-targets --features full -- -D warnings`
  → clean
- `cargo fmt --all -- --check` → clean

## DEFERRED to S5.1 follow-on (mission to file)

- **Pillar 2 step 2 — `OctoVaultLookup` impl**: bridge trait in
  octo-cap-macaroon to Stoolap-fork substrate rows. Cannot live in
  octo-vault crate directly (Layer B → Layer E forbidden). Pattern
  matches `TransportDeliveryCatalog` glue crate.
- File: `missions/open/0957-g1-octo-vault-lookup-glue.md` (to be filed
  by user / next session)

## Doc-bugs (to correct in next session)

1. `missions/open/0957-g-verify-time-invariant.md` AC #7 says
   `CapabilityTokenV2::verify_vault_bound` — verify surface is actually
   `Macaroon::verify_for_vault_op` (Macaroon.caveats, not
   CapabilityTokenV2.caveats). Fix AC #7 wording + add Mermaid sequence
   diagram showing the verify path (per CLAUDE.md "Mermaid over ASCII").

## Files changed (this session)

NEW:

- `crates/octo-cap-macaroon/src/vault_lookup.rs`
- `crates/octo-cap-macaroon/src/vault_verify_error.rs`
- `crates/octo-cap-macaroon/tests/tv_c1_verify_time.rs`

MODIFIED:

- `crates/octo-cap-macaroon/src/lib.rs` (re-exports)
- `crates/octo-cap-macaroon/src/macaroon.rs` (verify_for_vault_op +
  WrappedChainHasNoVault variant + MacaroonError derives)
- `crates/octo-protocol/src/envelope.rs` (version_tag field +
  VERSION_TAG_V1/V2 consts + build arg + verify_version + allow)
- `crates/octo-protocol/src/error.rs` (UnsupportedVersion variant)
- `crates/octo-protocol/src/signing.rs` (2 build arg additions)
- `crates/octo-protocol/src/dispatch.rs` (1 build arg addition)
- `crates/octo-protocol/tests/tv1..tv8.rs` (VERSION_TAG_V2 added +
  imports)
- `crates/octo-identity-resolver-node/src/backend.rs` (VERSION_TAG_V2 +
  build arg)
- `crates/octo-identity-resolver-node/src/node.rs` (3 build arg adds)
- `crates/octo-identity-resolver-node/tests/cross_node_chain.rs` (7
  build arg adds + 7 imports)
- `crates/octo-wallet-node/src/node.rs` (1 build arg add)
- `crates/octo-capability-issuer-node/src/node.rs` (build arg)
- `crates/octo-reputation-anchor-node/src/node.rs` (build arg)
- `crates/quota-router-core/src/node/envelope_v2.rs` (build arg)

## Why this works

The verify-time invariant is split across two distinct paths:
**structural** (`verify_full` — chain must contain Vault) and
**operational** (`verify_for_vault_op` — vault row exists + chain
matches + state=Active). Both reject the `WrappedOnly` chainless-
parent failure mode, but only the operational path needs a substrate
adapter (VaultLookup). This split keeps the Layer B → Layer E
dependency direction correct: trait + struct types live in the
extension (octo-cap-macaroon); the substrate adapter (OctoVaultLookup)
will live in the S5.1 glue crate.

**Why**: without the split, every Macaroon verify call would need a
VaultLookup injected — but capability verifications that don't touch
vaults (e.g. permission caveats, expiry caveats) have no business
querying the substrate. Layering + dependency direction forced the
architectural correction mid-implementation.

## Push authorization

Commits queued on `next` await user go-ahead per
[[feedback_initiative_user_only]] + [[git-workflow]]. 3 prior S4 DFP
codemod commits + this session's S5 commit chain all wait for explicit
user instruction to push.

## S6a follow-on (RFC-0870 amendment + TV-0870-01)

Follow-on mission `0870-c1-version-tag-amendment` (S6a of the storage
restructure plan) back-fills the RFC-0870 amendment text + adds the
TV-0870-01 byte-exact wire-form fixture. Pre-req satisfied by this S5
LANDED state. Mission file:
`missions/open/0870-c1-version-tag-amendment.md`.

S6a deliverables (per mission YAML AC #1–#5):

- RFC-0870 §Version History **v2.1 row** added (`rfcs/accepted/networking/0870-distributed-quota-router-network.md`)
- RFC-0870 §**NodeEnvelope Version Tag** subsection added under §Specification
- TV-0870-01 fixture (`crates/octo-protocol/tests/tv_0870_version_tag.rs`)
  — 7/7 tests passing (5 original + 2 added in Round 1 review fix
  commit `ab2b57b4`): V2 build + round-trip, V1 build (legacy),
  unknown tag rejected, verify_version gate (V2 ok, V1 rejected,
  unknown rejected), V1 vs V2 distinct envelope_id
  (version_tag-participates-in-hash invariant, NOT literal
  V1-replay-defense per Round 1 HIGH-2 fix), byte_position_pin
  (`bytes[32] == 0xA1` for V2, `0xA0` for V1), and
  runtime_gate_rejects_bypassed_unknown_tag (rejects even when
  struct-literal-bypassed).

## S6b follow-on (RFC-0957 amendment + 22 TV)

Follow-on mission `0957-c1-verify-time-amendment` (S6b of the
storage restructure plan) back-fills the RFC-0957 amendment text +
adds the TV-0957 20-fixture suite. Pre-req satisfied by this S5
LANDED state. Mission file:
`missions/open/0957-c1-verify-time-amendment.md`.

S6b deliverables (per mission YAML AC #1–#5):

- RFC-0957 §Version History **v2.1 row** added (`rfcs/accepted/economics/0957-capability-token-format.md`)
- RFC-0957 §**Verify-Time Extension** subsection added under §Algorithms
  (4-step algorithm verbatim per review doc §20.6.1, `VaultLookup` trait
  injection, `WrappedOnly` chain walk invariant)
- RFC-0957 §**Caveat DSL Extension** subsection added under §Data
  Structures (9 new `Caveat` variants per RFC-0965 §3 + `PermissionKind`
  enum + `FactoryVet` struct, all field names matched to the real
  `crates/octo-cap-macaroon/src/caveat/mod.rs` source after the
  Round 1 drift catch — `Permission(PermissionKind)` not
  `Permission { kind, scope }`; `ValidRange { valid_after_unix,
valid_until_unix }` not `ValidRange { axis, lower, upper }`; etc.)
- TV-0957 fixture (`crates/octo-cap-macaroon/tests/tv_0957_verify_time.rs`)
  — **22/22 tests passing** (4 categories × 5 tests + 2 deep-chain/boundary):
  - **TV-0957-01..05** — 5 Caveat DSL variant wire-form pins (Vault,
    Permission, ValidRange, MaxPerTx, AuditWindow)
  - **TV-0957-06..10** — 5 Caveat DSL variant wire-form pins (MaxUses,
    WrappedOnly, Factory, PolicyReference, Raw unknown-name rejection
    at attenuation per `macaroon.rs:242-243`)
  - **TV-0957-11..15** — 5 verify-time path pins (happy path
    signature-verify + all steps transitively, lookup step missing,
    chain match step, state-active step, WrappedOnly chain walk step)
  - **TV-0957-16..20** — 5 regression tests (frozen vault, chain
    mismatch, missing root secret → `Macaroon(RootSecretMismatch)`,
    `WrappedChainHasNoVault`, attenuation-monotonicity with new
    variants)

Status: **LANDED 2026-08-17** (this session). Pre-req verified: S5
LANDED, S6a LANDED. Drift catch: RFC amendment text initially
drafted Rust pseudocode with WRONG field names; corrected against
real `caveat/mod.rs` source before TV fixtures written.
