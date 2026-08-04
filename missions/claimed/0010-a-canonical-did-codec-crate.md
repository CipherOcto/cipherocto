# Mission: 0010-a — Canonical OctoID Codec Crate

## Status

Claimed (2026-08-04) by @mmacedoeu

## RFC

RFC-0010: Canonical OctoID Identifier Codec

## Dependencies

- RFC-0009
- RFC-0968
- RFC-0968-A1

## Summary

Implement the codec crate `crates/octo-ident/` with the `DidCodec` trait, `RawDid` / `WireDid` / `LegacyWire` types, and 100-round-trip property tests. Mints fresh DIDs from a 32-byte pubkey via `mint(pubkey)`. Translates legacy `did:octo:b<52>` to canonical wire form during the dual-parse window. Produces no on-chain writes; pure data + digest.

## Acceptance Criteria

- [x] `crates/octo-ident/` is a new crate registered in workspace `Cargo.toml`. (`crates/octo-ident/` ships; workspace member)
- [x] `DidCodec` trait + default impl with `raw_to_wire`, `wire_to_raw`, `legacy_to_wire`, `parse`, `mint`. (`pub trait DidCodec` + `impl DidCodec for CanonicalCodec` in `crates/octo-ident/src/lib.rs:190-212,214`)
- [x] `DidError` enum with `UnrecognizedShape`, `InvalidEncoding`, `InvalidLength`, `HashPartMismatch`, `LegacyFormExpired`. (`crates/octo-ident/src/lib.rs:140-164`)
- [x] `RawDid { hash: [u8; 32], version_discriminator: [u8; 20] }` structured form. (`crates/octo-ident/src/lib.rs:33-40`)
- [x] `WireDid(String)` and `LegacyWire(String)` wrappers. (`crates/octo-ident/src/lib.rs:82-83,116-117`)
- [x] `mint(pubkey: &[u8; 32]) -> RawDid` produces a 52-byte DID with hash + discriminator derived per RFC-0010 §Specification §`mint`. (`crates/octo-ident/src/lib.rs:215-239`)
- [x] `cargo test -p octo-ident --lib`: 100-round-trip property test on random corpus + 10 canonical vectors + edge cases (truncated input, wrong prefix, hash mismatch). **21 tests pass** including `property_100_round_trip_random_corpus`, `canonical_vectors_10_known_answer`, `edge_case_truncated_input_rejects`, `edge_case_wrong_prefix_rejects`, `edge_case_hash_part_mismatch`.
- [x] `cargo clippy -p octo-ident --all-targets -- -D warnings` clean.
- [x] `crates/octo-reputation/Cargo.toml` exports `octo-ident = { path = "../octo-ident" }` as a hard dep.
- [x] `crates/octo-reputation/src/types.rs::RecorderDid::to_wire()` method delegates to codec. (`crates/octo-reputation/src/types.rs:59-73`)

### Test Vector Summary

| Test | AC Coverage |
|------|-------------|
| `mint_produces_52_byte_raw` | hash non-zero, discriminator derived |
| `mint_is_deterministic` | determinism across calls |
| `raw_to_wire_and_back_round_trips` | single round-trip |
| `round_trip_10k_random` | 10k corpus |
| `property_100_round_trip_random_corpus` | AC: 100-round-trip property test |
| `canonical_vectors_10_known_answer` | AC: 10 canonical vectors |
| `edge_case_truncated_input_rejects` | AC: edge case truncated |
| `edge_case_wrong_prefix_rejects` | AC: edge case wrong prefix |
| `edge_case_hash_part_mismatch` | AC: edge case hash mismatch |
| `trait_dispatch_via_impl` | DidCodec trait dispatch |
| `parse_*` | dual-parse window logic |
| `base32_encode_52_matches_decode_52` | legacy base32 round-trip |
| `base58_btc_*` | canonical base58btc round-trip |
| `reputation_types_use_codec` | cross-crate invariant |

### Type Coverage

| RFC Type | Implemented By |
|----------|----------------|
| `RawDid` | This mission |
| `WireDid` | This mission |
| `LegacyWire` | This mission |
| `DidCodec` trait | This mission |
| `DidError` | This mission |
| `mint(pubkey)` algorithm | This mission |
| `parse()` algorithm | This mission |

### Implementation Guide

Reference: `crates/octo-ident/src/lib.rs` (new); `crates/octo-reputation/src/types.rs` (add `to_wire()` method); `crates/octo-reputation/Cargo.toml` (add `octo-ident` dep).

## Claimant

@mmacedoeu

## Pull Request

# pending user push

## Notes

- The codec is pure (no IO, no async). Mission C (deprecation) adds the `during_deprecation_window()` gate later.
- The `RecordDid::to_wire()` method is an additive non-breaking change.

## Closure

**Claimed:** 2026-08-04
**Implemented:** 2026-08-04

### Commits

1. `feat(octo-ident): claim mission 0010-a + DidCodec trait + HashPartMismatch variant + 100-round-trip + 10 canonical vectors + edge cases`

### Deviations

1. **Trait dispatch via `&self`/`Self: Sized`**: The `DidCodec` trait uses associated functions (no `&self`); this means it is dispatchable through `impl DidCodec for CanonicalCodec` but NOT through `Box<dyn DidCodec>`. This matches RFC-0010 §Data Structures verbatim (the RFC declares associated functions, not methods) and avoids unnecessary `Self: Sized` bounds. If Mission 0010-b or 0010-c needs trait-object dispatch, add `&self` to the trait — non-breaking.
2. **`parse()` error for non-`did:octo:`**: fixed to return `DidError::UnrecognizedShape` for inputs that don't even carry the namespace prefix. Bare `did:octo:<X>` post-window returns `LegacyFormExpired` (RFC-0010 §`parse` step 3). Distinguishing "structurally wrong" from "deprecated form" prevents false `LegacyFormExpired` returns on, e.g., `did:foo:z...`.
3. **Caller imports `DidCodec`**: Both `crates/octo-reputation/src/types.rs::RecorderDid::to_wire` and `crates/quota-router-core/src/marketplace/reputation_compat.rs::parse_canonical_did` now `use octo_ident::DidCodec` to bring the trait into scope. This is mechanical and required whenever the trait's associated functions are called on the impl type.

### Follow-up (NOT this mission)

- `mint` signature: per RFC-0957-A1 R6-C3 the catalog parameter is required upstream; for 0010-a `mint(pubkey)` is sufficient (no catalog binding for raw DIDs).
- F1 (W3C DID method registration), F2 (Multi-chain DID resolution), F3 (capability key derivation) tracked in RFC-0010 §Future Work — out of scope.
