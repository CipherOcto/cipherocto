# Mission: M4-saml-crypto-hygiene

## Status

Open. Filed 2026-08-13 from mission 0949-c1 SAML 3-pass review.
HIGH priority — small blast radius, high leverage.

## RFC

RFC-0949 (Economics): Enterprise SSO

## Dependencies

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
