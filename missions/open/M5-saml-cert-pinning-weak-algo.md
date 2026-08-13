# Mission: M5-saml-cert-pinning-weak-algo

## Status

Open. Filed 2026-08-13 from mission 0949-c1 SAML 3-pass review.
HIGH priority.

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
