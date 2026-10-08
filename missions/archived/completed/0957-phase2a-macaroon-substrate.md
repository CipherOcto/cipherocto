# 0957-phase2a — Macaroon Substrate (master unblocker root)

**Status:** LANDED 2026-08-13 (commit 90306f45). Drift-closed via audit 2026-08-13.
**Substrate:** RFC-0957 §3.1, RFC-0957-A1 §Persistence-Free Mint, RFC-0965 §3.7
**Closes:** wallet-node `handlers/mint.rs` `CIPHEROCTO_MINT_V1:*` placeholder → real macaroon wire form

## Scope

`WalletNode::handle_envelope(WALLET_MINT_CAPABILITY)` currently emits a
placeholder byte string `CIPHEROCTO_MINT_V1:<holder_did>`. Phase 2a
replaces that placeholder with the canonical macaroon wire form
produced by `octo_cap_macaroon::wire::serialize_wire`.

The `CapabilityToken::mint` call already routes through the migrated
substrate (per `octo_wallet::capability::CapabilityToken` →
`octo_cap_macaroon::CapabilityToken` re-export). Only the wire-form
emission is stubbed.

## Implementation

1. Add `octo-cap-macaroon = { path = "../octo-cap-macaroon" }` to
   `crates/octo-wallet-node/Cargo.toml` dependencies. Wire-form
   substrate lives in the Layer 4 extension crate (per-extension
   pattern; see `[[cipherocto-design-principles]]`).
2. Edit `crates/octo-wallet-node/src/handlers/mint.rs`:
   - Import `octo_cap_macaroon::wire::{serialize_wire, WireError}`.
   - Replace the `format!("CIPHEROCTO_MINT_V1:{}", token.holder_did).into_bytes()`
     placeholder with `serialize_wire(&token).map_err(wire_error_to_protocol)?.into_bytes()`.
   - Add `wire_error_to_protocol(WireError) -> ProtocolError` helper in
     `handlers/mod.rs` (mirrors `wallet_error_to_protocol` / `did_error_to_protocol`).
3. Update the existing `handle_mints_with_canonical_did` test to assert
   the real v1 wire shape (3 base64url-no-pad segments; not the
   `CIPHEROCTO_MINT_V1:` prefix).

## Test vector discipline (byte-exact TV)

Per `[[mission-gap-closure-priorities-2026-08-10]]` §Test vector
discipline, 5 wallet-node wire-form TV must remain byte-exact across
placeholder → real-wire cutover. Pin via 5 deterministic mint tests
(fixed seed per TV; macaroon nonce is RNG-derived but the resulting
wire form is reproducible from the deterministic seed path of the
holder signer + capability root bytes):

- **TV1** — fixed seed `[0..32]`, capability = `[0xab; 32]`, empty
  caveats: 3 base64url-no-pad segments; first segment decodes to
  canonical JSON macaroon with `root_id` length-prefixed.
- **TV2** — same inputs as TV1 but mint+wire+`deserialize_wire`
  roundtrip recovers macaroon with identical `root_id` + `caveats`.
- **TV3** — `holder_sig` verifies after wire roundtrip
  (`token.verify_holder_sig()` from deserialized token).
- **TV4** — `compute_cap_root_hash_from_wire(&wire)` matches
  `compute_capability_id(&macaroon)` byte-for-byte.
- **TV5** — non-canonical DID rejected before any mint work runs (the
  `InvalidDid` branch is preserved by the canonical-codec check).

The `handle_rejects_invalid_did` test in the existing module covers
TV5's negative path; the four positive-path TV are additive.

## Depends on

- `0957-ext-macaroon` Phase 1 (landed: `f123fe1b` + `8e30d6b7`)
- `0957-ext-macaroon` Phase 2b (`CapabilitySigner` trait + token +
  discharge + wire + discharge channels — landed: `abf2c927` +
  `50dda236` + `58340e35`)
- `0957-ext-macaroon` Phase 2c (cross-layer dep cleanup — landed:
  `a471843b` + `4cfe7165`)

## Blocks

- wallet-node end-to-end mint → wire → deserialize → verify path
- 0957-phase2b (`PaidQueryCaveat` migration) — needs wire form
- 0957-phase2c (`CapabilityIssuerNode` wiring) — needs real macaroon substrate
- 0957-phase2d (attenuation stub closure)
- 0957-e mint-txn-parameter (claimed, in flight)
- 0871e-phase5b (atomic drain)

## Layer direction

- `octo-wallet-node` (Layer C) → `octo-cap-macaroon` (Layer 4) ✓
- No reverse dependency. Phase 2c-2 closed the previous cross-layer
  edge (no `quota-router-storage` dep).

## Validation

- `cargo fmt --check` (per `[[cargo-fmt-workflow]]`)
- `cargo clippy --all-targets --all-features -- -D warnings`
  (per `[[feedback_clippy_zero_warnings]]`)
- `cargo test --lib -p octo-wallet-node` (per
  `[[feedback_stoolap_test_performance]]` avoid full build)
- `cargo test --lib -p octo-cap-macaroon` (TV unaffected by cutover)

## Cross-references

- `[[0957-phase2-unblocker-map]]` — master unblocker
- `[[cipherocto-design-principles]]` — per-extension crate model
- `[[mission-gap-closure-priorities-2026-08-10]]` — Wave 1 plan
- `[[mission-0957-ext-macaroon-phase2b-status]]` — predecessor substrate
- `[[no-line-refs-anywhere]]` — §section refs only
