# Mission: M5-saml-cert-pinning-weak-algo

## Status

Claimed → v0.3 CLOSED 2026-08-13. Filed from mission 0949-c1
SAML 3-pass review. HIGH priority.

### Acceptance criteria — completed

- [x] `ACCEPTED_SIGNATURE_ALGORITHMS: &[&str]` const list
      (6 entries: rsa-sha256/384/512 + ecdsa-sha256/384/512).
- [x] `check_signature_algorithm(algorithm: Option<&str>)`
      gate function — rejects RSA-SHA1, DSA, HMAC variants
      with length-only error message (no attacker-URI echo —
      log-injection hygiene from F2-007).
- [x] `SignedInfo/SignatureMethod/@Algorithm` attribute
      captured in `parse_xml_signature` for both
      `Event::Start` and `Event::Empty` (self-closing element
      is the common serialization).
- [x] `XmlSignatureComponents.signature_method_algorithm:
      Option<String>` field added; threaded through parser
      state machine and constructor site.
- [x] `validate_signature` calls
      `check_signature_algorithm(...)` BEFORE
      `verify_xml_signature(...)` so a real verifier landing
      in M1 cannot accidentally accept a SHA-1 assertion.
- [x] `ProviderConfig::validate` for `GenericSaml` now
      requires `idp_certificate: Some(non-empty)` and returns
      a descriptive error otherwise (was silently `Ok(())`).
- [x] `ProviderConfig::warn_unpinned_idp` warn-only startup
      health check added; emits `tracing::warn!` for SAML
      providers with `idp_metadata_url` set but no pinned
      cert. Returns warned provider IDs so callers can
      surface them in startup health endpoints.
- [x] 9 algorithm tests added:
      `test_saml_algorithm_rsa_sha256_accepted`,
      `_rsa_sha384_accepted`, `_rsa_sha512_accepted`,
      `_ecdsa_sha256_accepted`, `_rsa_sha1_rejected`,
      `_dsa_rejected`, `_hmac_sha1_rejected`,
      `_missing_rejected`, `_unknown_uri_rejected`
      (last asserts the error message does NOT echo the
      attacker URI — log-injection hygiene).
- [x] `test_provider_config_validation` updated to
      `GenericSaml`'s new cert-required rule; two new
      negative cases for missing cert and empty cert.
- [x] 3 new `warn_unpinned_idp` tests:
      `_saml_no_cert_warns`, `_saml_with_cert_no_warn`,
      `_non_saml_provider_no_warn`.
- [x] Clippy zero warnings
      (`cargo clippy -p quota-router-core --all-targets
      --features full -- -D warnings` clean).
- [x] All existing tests pass — 165/165 in `auth::sso::*`.

### Acceptance criteria — partial / deferred

- [ ] Real RSA-SHA-256 verifier (M1-saml-signature-real).
      The algorithm gate is enforced NOW; the verifier
      itself is still a stub.

### Diff summary

- `crates/quota-router-core/src/auth/sso/saml.rs`:
  - New `ACCEPTED_SIGNATURE_ALGORITHMS` const + new
    `check_signature_algorithm` fn at module top.
  - `XmlSignatureComponents` +1 field
    `signature_method_algorithm: Option<String>`.
  - `parse_xml_signature` state machine captures
    `SignatureMethod/@Algorithm` for both `Event::Start` and
    `Event::Empty`.
  - `validate_signature` calls the gate BEFORE the verifier.
  - 9 algorithm tests + 2 fixed XML test updates.

- `crates/quota-router-core/src/auth/sso/mod.rs`:
  - `use tracing;` added.
  - `ProviderConfig::validate` for `GenericSaml` requires
    non-None non-empty `idp_certificate`.
  - New `ProviderConfig::warn_unpinned_idp` warn-only helper
    for operator UX.
  - Test site updated for new cert rule; 3 new warn-helper
    tests.

### Cross-references

- F1-008 (weak algo gate) ✓
- F1-009 (cert pinning via validate) ✓
- M1 (real verifier — must enforce algorithm list once real)
- M8 docs — module doc already references the algorithm list

## RFC

RFC-0949 (Economics): Enterprise SSO
SAML 2.0 §5.4.1 (Signature)

## Dependencies

- `crates/quota-router-core/src/auth/sso/saml.rs`
- `crates/quota-router-core/src/auth/sso/mod.rs` (`ProviderConfig::validate`)

## Findings covered

- **F1-008:** `SignedInfo/SignatureMethod/@Algorithm` not parsed.
  Combined with the signature stub, RSA-SHA1 (and worse) is
  silently accepted.
- **F1-009:** `ProviderConfig::validate` for `GenericSaml` only
  checks `idp_metadata_url` / `sp_entity_id` / `acs_url`. Operator
  who configures only metadata URL with no `idp_certificate` is
  told `validate() -> Ok(())` — silent failure mode.

## Acceptance Criteria

- [ ] Parse `SignedInfo/SignatureMethod/@Algorithm`. Accept: - `http://www.w3.org/2001/04/xmldsig-more#rsa-sha256` - `http://www.w3.org/2001/04/xmldsig-more#rsa-sha384` - `http://www.w3.org/2001/04/xmldsig-more#rsa-sha512` - `http://www.w3.org/2001/04/xmldsig-more#ecdsa-sha256` - `http://www.w3.org/2001/04/xmldsig-more#ecdsa-sha384` - `http://www.w3.org/2001/04/xmldsig-more#ecdsa-sha512`
      Reject all others (RSA-SHA1, DSA, HMAC variants) with
      `SamlSignatureInvalid { reason: WeakAlgorithm }`.
- [ ] Update `ProviderConfig::validate` for `GenericSaml`:
      require `idp_certificate` to be `Some`. Return
      `ConfigError::MissingIdpCertificate` otherwise.
- [ ] Add a startup health check on the SAML handler
      (`crates/quota-router-core/src/admin.rs:568-580`) that
      logs a `tracing::warn!` when an IdP provider has
      `idp_metadata_url` set without a pinned cert. (Warn-only;
      operator UX, not a hard error.)
- [ ] Tests: - `test_saml_algorithm_rsa_sha256_accepted` - `test_saml_algorithm_rsa_sha384_accepted` - `test_saml_algorithm_rsa_sha512_accepted` - `test_saml_algorithm_ecdsa_sha256_accepted` - `test_saml_algorithm_rsa_sha1_rejected` - `test_saml_algorithm_dsa_rejected` - `test_saml_algorithm_hmac_sha1_rejected` - `test_provider_config_validate_requires_idp_certificate`
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Claimant

(unclaimed)

## Notes

Algorithm enforcement is independent of the real-verifier
implementation (M1). Even with the stub, parsing the
`@Algorithm` attribute gives the caller an early rejection
signal before falling through to the (currently no-op)
verifier. Once M1 lands, the algorithm list becomes the
post-parse validation step.

**Cross-references:**

- F1-008 (weak algo)
- F1-009 (cert pinning via validate)
- M1 (verifier — must enforce algorithm list once real)
- M8 (docs — update module doc with the algorithm list)
