# Mission: M6-b-error-path-newtypes

## Status

Claimed → v0.1 CLOSED 2026-08-13. Filed from
M6-saml-error-path-types (Stage 2/3 deferral). This mission
is the **declaration-only** scope; substitution into
`SamlAssertion` / `ProviderConfig` / `IdpMetadata` /
`TokenClaims` / `SsoUser` is tracked under M6-c.

### Acceptance criteria — completed

- [x] New module
      `crates/quota-router-core/src/auth/sso/newtypes.rs`
      exports the 5 newtypes: `EntityId`, `Audience`,
      `Recipient`, `Email`, `Secret(Vec<u8>)`.
- [x] `mod.rs` registers + re-exports the newtypes at the
      sso boundary (`pub mod newtypes;` +
      `pub use self::newtypes::*;`).
- [x] `Secret` uses `Zeroizing<Vec<u8>>` (concrete, not
      generic — generic form has unstable trait bounds
      across `zeroize` versions and the use case is
      exclusively bytes). `Debug` redacts
      (`<redacted>`); `Display` intentionally NOT
      provided.
- [x] `Email::from_str` validates RFC 5321-ish structure
      (one `@`, non-empty local part, non-empty domain
      part, domain contains at least one `.`). Invalid
      input returns `Err(NewtypeError::InvalidEmailFormat)`.
- [x] 14 unit tests:
      `entity_id_round_trip`,
      `entity_id_empty_rejected`,
      `audience_round_trip`,
      `recipient_round_trip`,
      `email_valid`, `email_no_at_rejected`,
      `email_empty_local_rejected`,
      `email_no_dot_in_domain_rejected`,
      `newtype_mismatch_prevention_compile_time`,
      `secret_debug_redacts`,
      `secret_empty_rejected`,
      `secret_zeroized_after_drop`,
      `secret_from_string_via_into`,
      `entity_id_distinct_from_audience_at_runtime`.
- [x] Compile-time type-mismatch prevention verified by
      the `takes_audience(a: Audience)` test pattern.
- [x] Clippy zero warnings
      (`cargo clippy -p quota-router-core --all-targets
      --features full -- -D warnings` clean).
- [x] 252 auth-module tests pass; 1655/1655 lib tests pass.

### Design decision: concrete `Secret(Vec<u8>)`

Original AC proposed `pub struct Secret<T: Zeroize +
AsRef<[u8]>>(Zeroizing<T>)`. Dropped the generic form
because (a) `Zeroizing<T>` requires `T: Zeroize` (not
just `ZeroizeOnDrop`), (b) the `derive(Zeroize)` macro
is not stable across `zeroize` 1.x versions, (c) all
real call sites take byte strings (`client_secret`,
`scim_token`, `idp_certificate`). The concrete form
`Secret(Zeroizing<Vec<u8>>)` is simpler and stable. If
a non-byte secret materializes later (e.g., symmetric
key), wrap in `Secret` at the conversion site.

### Out-of-scope (NOT this mission)

- Substituting `String` fields with the newtypes in
  `SamlAssertion`, `ProviderConfig`, `IdpMetadata`,
  `TokenClaims`, `SsoUser`. → M6-c
- `BlacklistQuery` port-trait. → M6-d
- admin.rs:573-590 HTTP Content-Length. → M6-e
- `SamlAudienceMismatch` struct variant widening. → M6-f

## RFC

RFC-0949 (Economics): Enterprise SSO
§no-central-enums, §typed-discriminator, §storage-is-not-a-protocol
(CLAUDE.md §Architectural Principles)

## Dependencies (output)

- Unblocks M2 (audience / subject-confirmation-method) by
  providing the `Audience` newtype.
- Unblocks M3 (replay protection) by providing canonical
  encoding for assertion_id canonicalization.
- Foundational for M7 security tests (newtypes make negative-
  test assertions about identity-mixup possible).

## Foundational newtype module

Create `crates/quota-router-core/src/auth/sso/newtypes.rs`
with:

```rust
//! Identity-bearing + secret-material newtypes at the sso
//! Layer B substrate boundary.
//!
//! These types implement `CanonicalCodec` (RFC-04 canonical
//! binary form for canonical-log use), `Display`, and
//! `FromStr`. The string representation is the canonical
//! human-readable form. **Two distinct newtypes must NOT
//! be substitutable** even if their inner String types
//! match: passing a `Recipient` where an `Audience` is
//! expected is a compile error, not a runtime panic.
//! (Per CLAUDE.md §no-central-enums.)

pub struct EntityId(String);
pub struct Audience(String);
pub struct Recipient(String);
pub struct Email(String);

pub struct Secret<T: Zeroize + AsRef<[u8]>>(Zeroizing<T>);
```

Plus `impl CanonicalCodec` (re-export from
`crates/octo-types`) and `Display`/`FromStr`/`Debug` (Debug
redacts Secrets) for each.

## Acceptance Criteria (this PR scope — declaration only)

- [ ] New module
      `crates/quota-router-core/src/auth/sso/newtypes.rs`
      exports the 5 newtypes above with the trait impls.
- [ ] `mod.rs` re-exports them at the sso boundary so external
      crates can `use quota_router_core::auth::sso::EntityId`
      without naming the submodule.
- [ ] `Secret<T>` derives `Zeroize + ZeroizeOnDrop`.
      `Debug` for `Secret<T>` is REDACTED (prints `<redacted:
      Secret<…>>`). Other newtypes use plain `Debug`.
- [ ] `Email::from_str` validates RFC 5321-ish structure
      (one `@`, non-empty local part, non-empty domain part,
      domain contains at least one `.`). Invalid input
      returns `Err(SsoError::InvalidEmail)`.
- [ ] Tests for each newtype: round-trip through `Display`
      + `FromStr`, type-mismatch prevention at compile time
      (separate `fn takes_audience(a: Audience)` cannot
      accept `EntityId`), Secret Debug redacts.
- [ ] Clippy zero warnings; all existing tests pass.

## Out-of-scope (NOT this PR)

- Substituting `String` fields with the newtypes in
  `SamlAssertion`, `ProviderConfig`, `IdpMetadata`,
  `TokenClaims`, `SsoUser`. Tracked under
  `M6-c-newtype-wiring` (to file at end of this mission).
- `BlacklistQuery` port-trait. Tracked under
  `M6-d-blacklist-port-trait`.
- admin.rs:573-590 HTTP Content-Length. Tracked under
  `M6-e-admin-content-length`.
- `SamlAudienceMismatch` struct variant widening. Tracked
  under `M6-f-audience-mismatch-struct`.

## Claimant

(unclaimed)
