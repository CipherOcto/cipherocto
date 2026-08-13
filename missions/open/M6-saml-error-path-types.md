# Mission: M6-saml-error-path-types

## Status

Open. Filed 2026-08-13 from mission 0949-c1 SAML 3-pass review.
MEDIUM priority.

## RFC

RFC-0949 (Economics): Enterprise SSO
§no-central-enums, §typed-discriminator, §storage-is-not-a-protocol
(CLAUDE.md §Architectural Principles)

## Dependencies

- `crates/quota-router-core/src/auth/sso/saml.rs`
- `crates/quota-router-core/src/auth/sso/mod.rs`

## Findings covered

- **F2-005:** No max-size cap on `assertion_xml`. Parser happily
  walks a 100MB assertion. DoS via deeply-nested DTD entity
  expansion (or just a giant `AttributeValue` blob).
- **F2-006:** 15+ sites use `attr.unescape_value().unwrap_or_default()`
  and `unescape(...).unwrap_or_default()` — failures silently become
  empty strings. SAML fail-closed requires these be hard errors.
- **F2-007:** Recipient error message `format!`s attacker-controlled
  `recip` into `SsoError::ProviderError`, which surfaces in logs
  and HTTP 502 JSON. Log-injection / XSS sink.
- **F2-008:** Per §no-central-enums / §typed-discriminator,
  identifier-typed fields should be newtypes:
  `EntityId`, `Audience`, `Recipient`, `Email`, `Secret<String>`.
  Currently bare `String` for all.
- **F2-011:** `SsoConfig` carries `Arc<dyn TokenBlacklistStorage>`
  (Layer-D-adjacent) and `ProviderConfig.client_secret` as plain
  `String`. Layer B should not see `Arc<dyn>` or bare `String`
  for secrets.

## Acceptance Criteria

- [ ] Add `const MAX_SAML_XML_BYTES: usize = 64 * 1024` (64 KiB
      — generous for a real assertion; bounded for DoS).
      `parse()` early-rejects above this size.
- [ ] At the admin HTTP boundary (`admin.rs:573-590`), enforce
      `Content-Length` ≤ 64 KiB on the SAML POST body before
      calling `handle_saml_callback`.
- [ ] Replace every `attr.unescape_value().unwrap_or_default()`
      with `.map_err(|e| SsoError::ProviderError(format!(
    "attribute unescape failed: {}", e)))?`.
      Replace every `unescape(...).unwrap_or_default()` similarly.
      15+ call sites; mechanical fix.
- [ ] Replace `format!("Recipient mismatch: expected {}, got {}",
    self.acs_url, recip)` with the length-only variant
      `format!("Recipient mismatch: expected len={}, got len={}",
    self.acs_url.len(), recip.len())`. Apply same sanitization
      to other format!s in saml.rs that include attacker data.
- [ ] Newtype identity-bearing fields at the Layer B substrate
      boundary:
      `rust
    pub struct EntityId(String);
    pub struct Audience(String);
    pub struct Recipient(String);
    pub struct Email(String);
    pub struct Secret<T: Zeroize + AsRef<[u8]>>(T);
    `
      Implement `CanonicalCodec` + `Display` + `FromStr` for
      each. Update `SamlAssertion`, `ProviderConfig`, `IdpMetadata`,
      `TokenClaims`, `SsoUser`.
- [ ] Move `Arc<dyn TokenBlacklistStorage>` behind a Layer B
      port trait `sso::port::BlacklistQuery` with
      `async fn is_blacklisted(&self, id: &str) -> Result<bool>`.
      Inject via constructor; production wiring uses a thin
      Stoolap-backed adapter; tests use an in-memory adapter.
- [ ] Tests: - `test_saml_xml_oversize_rejected` - `test_saml_attribute_unescape_error_propagates` - `test_saml_text_unescape_error_propagates` - `test_saml_recipient_mismatch_error_no_attacker_data` - `test_saml_newtype_entity_id_prevents_string_mixup` - `test_saml_blacklist_query_port_trait_wired`
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Claimant

(unclaimed)

## Notes

This is the LARGEST of the M1-M8 cascade by LoC — the newtype
work touches every module under `auth/sso/`. Land M4 (crypto
hygiene) FIRST so `Secret<T>` is available; then this mission
can lean on `Secret<String>` for `client_secret`.

**DoS cap rationale:** 64 KiB is large enough for any legitimate
SAML assertion (typical real-world assertion ~5-20 KiB) and
small enough to bound per-request memory to a few hundred bytes
of parser state. If real-world deployments report larger
assertions, lift to 256 KiB; do not go higher without revisiting.

**Layer port:** the `sso::port::BlacklistQuery` trait is the
canonical Layer-B substrate for the blacklist. Layer-C node
code wires the Stoolap adapter; production wiring is out of
scope for this mission (deployment concern).

**Cross-references:**

- F2-005 (DoS cap)
- F2-006 (error propagation)
- F2-007 (log injection)
- F2-008 (newtypes — biggest LoC)
- F2-011 (layer discipline)
- M2 (SamlAudienceMismatch struct variant — same PR or follow-on)
- M4 (`Secret<T>` dependency)
