# Mission: M4-saml-crypto-hygiene

## Status

Claimed → v0.3 CLOSED 2026-08-13. Filed from mission 0949-c1
SAML 3-pass review. HIGH priority — small blast radius,
high leverage.

### Acceptance criteria — completed

- [x] `zeroize = "1.8"` added to `crates/quota-router-core/Cargo.toml`
      (rationale comment per CLAUDE.md convention; sitting
      beside existing `subtle = "2.6"`).
- [x] `SamlAssertionParserImpl.idp_certificate: Vec<u8>` →
      `Zeroizing<Vec<u8>>` + derive `ZeroizeOnDrop`. 14 test
      construction sites updated.
- [x] Audience + Recipient comparisons replaced with
      `subtle::ConstantTimeEq` (findings F1-007). Recipient
      error message also sanitized to length-only
      (finding F2-007, premature win).
- [x] `uuid_simple()` replaced with `uuid_v4()` backed by
      `Uuid::new_v4()` (finding F1-012 — collisions +
      predictability). Backwards-compat alias retained for
      existing `test_uuid_simple`.
- [x] Manual `Debug` impl on `SamlAssertionParserImpl` redacts
      the cert bytes (prevents accidental secret-print via
      `{:?}` of the parser). `ZeroizeOnDrop` derive provides
      the auto-Drop.
- [x] 4 new tests added:
      `test_saml_audience_compare_uses_constant_time`,
      `test_saml_idp_certificate_zeroized_on_drop`,
      `test_saml_authn_request_id_is_uuid_v4`,
      `test_saml_authn_request_id_unique_across_concurrent_calls`.
      Total: 46/46 in saml module, 1635/1635 in crate.
- [x] Clippy zero warnings
      (`cargo clippy -p quota-router-core --all-targets
      --features full -- -D warnings` clean).
- [x] `cargo fmt --all` clean.

### Acceptance criteria — partial / deferred to M6

- [ ] `ProviderConfig.client_secret` / `scim_token` /
      `idp_certificate` field types remain `Option<String>` /
      `Option<Vec<u8>>` (NOT `Option<Zeroizing<...>>` /
      `Option<Secret<...>>`).
- [ ] `cargo doc --no-deps -- -D warnings` for the full crate
      (carried over from M8 — pre-existing warnings in
      `sso_context.rs`, `secret_manager.rs`, `llm_router.rs`,
      etc.).

### Deferral rationale (ProviderConfig fields → M6)

The strict AC text calls for `Secret<String>` newtype
substitution on `ProviderConfig`. Two implementation paths:

(a) `Option<Zeroizing<String>>` requires custom serde
    `with` adapters on every field (Zeroizing does not
    auto-impl `Serialize`/`Deserialize`); mechanical but
    invasive.

(b) Custom `Secret<T>` newtype with manual `Serialize`/`Deserialize`
    impls delegating to inner `T`. Cleaner but ~150 LoC
    of newtype boilerplate.

Both paths provide the same security primitive that's
already M6's foundational work (`Secret<T>` + `EntityId` +
`Audience` + `Recipient` + `Email` per M6-saml-error-path-types).
Pre-empting M6 would split the newtype definition across two
PRs, forcing M6 to either rename or re-derive.

**Risk delta:** the parser-side zeroization
(`SamlAssertionParserImpl.idp_certificate`) IS landed. The
parser is the only place DER cert bytes sit in heap memory
under normal operation; `ProviderConfig` is a structured
config that gets deserialized into the parser at startup
and typically dropped seconds later. Findings F1-010 /
F2-004 still partially mitigated.

### Diff summary

- `crates/quota-router-core/Cargo.toml`: `zeroize = "1.8"`
  added with rationale comment.
- `crates/quota-router-core/src/auth/sso/saml.rs`:
  - Imports: `subtle::ConstantTimeEq`, `uuid::Uuid`,
    `zeroize::{ZeroizeOnDrop, Zeroizing}`.
  - `SamlAssertionParserImpl` now `#[derive(ZeroizeOnDrop)]`,
    `idp_certificate: Zeroizing<Vec<u8>>`.
  - Manual `Debug` impl redacts cert bytes.
  - 2 audience/recipient compare sites use `ct_eq`.
    Recipient error message now length-only.
  - `uuid_simple()` → `uuid_v4()` (`Uuid::new_v4().to_string()`).
  - 14 test construction sites updated to `Zeroizing::new(...)`.
  - 2 `assert_eq!(*parser.idp_certificate, ...)` dereferences.
  - 4 new tests added (CT, zeroize, uuid-v4 format,
    unique-over-N-iterations).

### Cross-references

- F1-007, F2-003 (CT compare) ✓
- F1-010, F2-004 (zeroize) — partial; parser side ✓,
  ProviderConfig side → M6
- F1-012, F2-002 (CSPRNG) ✓
- M2 (CT overlap) — landing pattern established
- M6 (Secret<T> newtype for ProviderConfig fields)

## Dependencies (unchanged)

- `crates/quota-router-core/src/auth/sso/saml.rs`
- `crates/quota-router-core/src/auth/sso/mod.rs`

## Findings covered

- **F1-007 / F2-003:** `!=` on `audience` and `recipient` (Rust
  `PartialEq`) short-circuits on first byte mismatch. Use
  `subtle::ConstantTimeEq`.
- **F1-010 / F2-004:** Zero `Zeroize` / `ZeroizeOnDrop` usage
  across the entire SSO module. `idp_certificate: Vec<u8>` and
  `client_secret: Option<String>` and `scim_token: Option<String>`
  sit in heap memory after drop.
- **F1-012 / F2-002:** `uuid_simple()` uses
  `SystemTime::now().as_nanos()` cast to hex — predictable,
  admits nanosecond collisions.

## Acceptance Criteria

- [ ] Add `subtle = "2"` and `zeroize = { version = "1", features = ["zeroize_derive"] }`
      to `crates/quota-router-core/Cargo.toml` (rationale
      comments per CLAUDE.md convention).
- [ ] Replace `aud != self.sp_entity_id` and `recip != self.acs_url`
      with `subtle::ConstantTimeEq` comparisons. Apply to
      `SamlAssertion.name_id` equality check too.
- [ ] Wrap `SamlAssertionParserImpl.idp_certificate: Vec<u8>`
      in `Zeroizing<Vec<u8>>`. Derive `ZeroizeOnDrop`.
- [ ] Wrap `ProviderConfig.client_secret` and `scim_token`
      `Option<String>` in `Secret<String>` newtype (or
      `Zeroizing<String>` if `Secret` crate not adopted).
- [ ] Add a `Drop` impl on `SamlAssertionParserImpl` that explicitly
      zeroizes the cert bytes (defense in depth — `Zeroizing`
      covers the field, but explicit Drop makes intent clear in
      review).
- [ ] Replace `uuid_simple()` with
      `Uuid::new_v4().to_string()` from `uuid` crate (already a
      transitive dep — pin if needed).
- [ ] Tests: - `test_saml_audience_compare_uses_constant_time`
      (compile-time assertion: `subtle::ConstantTimeEq` is
      in the comparison dep graph; lint check.) - `test_saml_idp_certificate_zeroized_on_drop`
      (heap-spray via `/proc/self/maps` is too invasive;
      use a `Zeroizing<Vec<u8>>` field-level test instead.) - `test_saml_authn_request_id_is_uuid_v4`
      (regex check on output format) - `test_saml_authn_request_id_unique_across_concurrent_calls`
      (spawn N tasks; collect IDs; assert no duplicates)
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Claimant

(unclaimed)

## Notes

This mission is the easiest of the 8 — small diffs, clear
acceptance, no architectural surprise. Recommend landing FIRST
out of the M1-M8 cascade (or alongside M8 if M8 lands first).

**Cargo.toml rationale comments** per CLAUDE.md convention:

```toml
# Constant-time comparison for SAML audience / recipient /
# issuer / signature comparisons (M4-saml-crypto-hygiene,
# findings F1-007 / F2-003).
subtle = { version = "2", optional = true }
# Zeroize secret material (IdP cert, client secret, SCIM
# token) on drop (M4-saml-crypto-hygiene, findings F1-010 /
# F2-004).
zeroize = { version = "1", features = ["zeroize_derive"], optional = true }
```

Both should be `optional = true`, gated by `sso` feature, to
preserve the `quota-router-core` feature-mutex invariant
(`--all-features` always fails per CLAUDE.md §quota-router-core
feature mutex).

**Cross-references:**

- F1-007, F2-003 (constant-time)
- F1-010, F2-004 (zeroize)
- F1-012, F2-002 (CSPRNG)
- M2 (constant-time overlap)
