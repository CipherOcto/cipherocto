# 0957-phase2d — Macaroon attenuation stub closure

**Status:** LANDED 2026-08-13 (commit 9fe9071c). Drift-closed via audit 2026-08-13.
**Substrate:** RFC-0957 §3.2 + §3.5 + RFC-0965 §3 caveat chain
**Closes:** wallet-node `handlers/attenuate.rs` `CIPHEROCTO_MINT_V1_ATTENUATED:*` placeholder → real macaroon attenuation via `CapabilityToken::attenuate`

## Scope

The wallet-node `WALLET_ATTENUATE_CAPABILITY` handler currently emits
a placeholder `CIPHEROCTO_MINT_V1_ATTENUATED:<did>:+<N>bytes`. Mission
0957-phase2d replaces the placeholder with the canonical macaroon
attenuation path:

1. Deserialize the existing token wire form (the v1 wire form per
   mission 0957-phase2a) → `CapabilityToken`.
2. Append the requested typed caveat via
   `CapabilityToken::attenuate(new_caveat, catalog)` (per
   RFC-0957 §3.5 — monotonic narrowing; existing caveats preserved).
3. Re-serialize via `serialize_wire(&attenuated_token)` (the v1 wire
   form).
4. Emit the new wire form as the response payload.

`new_caveat` becomes a typed `Caveat` (one of the 24+ variants in
`octo_cap_macaroon::caveat::Caveat`) instead of opaque bytes. The
opaque-byte legacy shape is dropped (the `CIPHEROCTO_MINT_V1` wire
form never existed in production — this is a Phase 1 MVP that was
never deployed; no migration concern).

## Implementation

1. `crates/octo-wallet-node/src/handlers/attenuate.rs`:
   - `AttenuateRequest` gains `existing_token_wire: String` (the
     v1 macaroon wire form) + `new_caveat: Caveat` (typed; one of
     the 24+ variants from `octo_cap_macaroon::caveat::Caveat`).
     The old `existing_token: Vec<u8>` + `new_caveat_payload: Vec<u8>`
     fields are dropped (no backward compat needed — Phase 1 MVP).
   - `AttenuateHandler` holds an `InMemoryCatalog` for the
     `WrappedOnly` chain guard (the guard is a no-op for
     non-WrappedOnly caveats; an empty catalog suffices for the
     common case).
   - `handle()`: validate DID, deserialize wire form (needs
     holder_did + holder_pub from out-of-band DID registry — same
     as `phase2a` deserialize path), attenuate, re-serialize.

## Test vector discipline

- 3 existing tests updated to supply the new typed fields:
  - `attenuate_request_borsh_round_trip` — `new_caveat` becomes a
    `Caveat::Before(...)` typed variant.
  - `handle_rejects_unrecognized_wire_form` — replaced with
    `handle_rejects_unparseable_wire_form` (the v1 wire form is
    always parseable or rejected at the canonical-form check).
  - `handle_attenuates_mvp_token` — replaced with
    `handle_attenuates_real_token` (real macaroon attenuation).
- 3 new TV:
  - TV1 — attenuation appends a `Caveat::Before(...)` and the new
    wire form's macaroon root_id differs from the parent (HMAC
    chain extended).
  - TV2 — `Caveat::Model("gpt-4")` narrowing: child model is
    preserved; parent model widened to wildcard is rejected by
    `verify_full` (subsumption check).
  - TV3 — handler rejects an unparseable wire form (4 segments
    instead of 3) with `ProtocolError::AuthorizationFailed`.

## Depends on

- 0957-phase2a (landed commit `90306f45`) — real wire form substrate
- 0957-phase2b (landed commit `5cda2eb7`) — `Caveat::Payment`
  variant in the central enum
- 0957-phase2c (landed commit `b19fe57f`) — HolderRegistry substrate
  availability (no direct use here; downstream concern)

## Blocks

- Production attenuation end-to-end (wallet → attenuate → verify)
- 0871e Phase 5 paid-query caveat chain (caveat attenuation is the
  attenuation vector for PaidQueryCaveat narrowing)

## Layer direction

- `octo-wallet-node` (Layer C) → `octo-cap-macaroon` (Layer 4) ✓
- No reverse dependencies introduced.

## Validation

- `cargo fmt -p octo-wallet-node --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --lib -p octo-wallet-node` (existing 24 + 3 new
  attenuation TV)

## Cross-references

- `[[0957-phase2-unblocker-map]]` — phase2d sub-mission
- `[[cipherocto-design-principles]]` — Layer C specialized-node pattern
- `[[mission-gap-closure-priorities-2026-08-10]]` — Wave 1 plan
- `[[mission-0957-ext-macaroon-phase2b-status]]` — substrate predecessor
