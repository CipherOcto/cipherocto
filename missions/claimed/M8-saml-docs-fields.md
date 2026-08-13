# Mission: M8-saml-docs-fields

## Status

Claimed → v0.2 CLOSED 2026-08-13. Originally filed from mission
0949-c1 SAML 3-pass review. MEDIUM priority — small diff, high
clarity value.

### Acceptance criteria — completed

- [x] Expand module doc to enumerate coverage explicitly
      (Coverage + DEFERRED sections landed at
      `crates/quota-router-core/src/auth/sso/saml.rs`)
- [x] Replace the false doc on `verify_xml_signature`
      with STUB notice (signature verification remains a no-op;
      landing real RSA-SHA256 verifier is M1-saml-signature-real)
- [x] Add `pub issuer: Option<String>` and
      `pub assertion_id: Option<String>` fields to `SamlAssertion`
- [x] Parse `<Issuer>` element text (immediate child of Assertion
      or Response); truncate to 256 chars as defense-in-depth
      (`const ISSUER_TEXT_CAP: usize = 256` in parser)
- [x] Test `test_saml_issuer_extracted_into_struct` — passes
- [x] Test `test_saml_assertion_id_extracted_into_struct` — passes
- [x] Tests include `<ds:Signature><ds:SignatureValue>...</ds:SignatureValue></ds:Signature>`
      block so stub verifier accepts; non-empty byte blob
      threads the validation path
- [x] Clippy zero warnings (`cargo clippy -p quota-router-core --all-targets
      --features full -- -D warnings` clean)
- [x] All existing tests pass (42/42 in `auth::sso::saml` module)
- [x] SAML module's own doc-build clean under `-D warnings`

### Acceptance criteria — partial / external

- [ ] `cargo doc --no-deps -- -D warnings` for the full
      `quota-router-core` crate: BLOCKED by 5 pre-existing
      unrelated doc warnings in `sso_context.rs:347`,
      `secret_manager.rs:177`, `llm_router.rs` (URL not
      auto-linked), plus 2-3 unresolved intra-doc-link
      warnings (`sign_envelope_with`, `0`, `EMAIL_REDACTED`).
      None introduced by M8. Fix tracked separately under
      `mission-quota-router-core-doc-cleanup` (to file).
      The M8-specific intent — that saml.rs itself has no
      doc warnings — is satisfied; verified by
      `RUSTDOCFLAGS="-D warnings" cargo doc -p quota-router-core
      --no-deps 2>&1 | grep saml.rs` returning empty.

### Diff summary

- `crates/quota-router-core/src/auth/sso/saml.rs`:
  - Module doc: added `## Coverage` (4 spec sections) +
    `## DEFERRED` (6 known gaps with mission refs) +
    production-safety warning
  - `SamlAssertion`: 2 new `pub` fields with doc comments
  - Parser state machine: 3 new fields
    (`issuer`, `assertion_id`, `in_issuer`),
    `const ISSUER_TEXT_CAP = 256`,
    capture on `<Assertion ID="...">`, `<Issuer>...</Issuer>`,
    text accumulation guarded by `text.is_empty()` +
    256-char clamp
  - `validate_signature` + `verify_xml_signature` STUB notices
  - 2 new test functions (XML augmented with
    `<ds:Signature><ds:SignatureValue>YWJjMTIz</ds:SignatureValue></ds:Signature>`)
  - 1 doc-comment URL auto-linked (`SP entity ID` at line 100)
  - 2 existing test-construction sites updated for new fields

## RFC

RFC-0949 (Economics): Enterprise SSO

## Dependencies

- `crates/quota-router-core/src/auth/sso/saml.rs`

## Findings covered

- **F3-001:** Doc comment on `verify_xml_signature` (line 410-417)
  claims "Verifies: 1. Signature element exists 2. SignedInfo
  digest matches assertion digest 3. Signature value is valid using
  IdP certificate (RSA-SHA256)". The function does NONE of these.
  Doc materially false.
- **F3-011:** Module doc (line 1-5) references RFC-0949 but omits
  ALL SAML 2.0 spec sections covered (or partially covered).
  Reader assumes full conformance.
- **F3-013:** `SamlAssertion` struct (line 23-34) does not expose
  `assertion_id` or `issuer`. Downstream replay-protection or
  issuer-pinning must re-parse XML.

## Acceptance Criteria

- [ ] Expand module doc to enumerate coverage explicitly:
      `rust
    //! SAML 2.0 Authentication (RFC-0949)
    //!
    //! ## Coverage
    //! - §5.4.2 Conditions / NotBefore / NotOnOrAfter + clock skew
    //! - §2.5.1.4 AudienceRestriction (single-value; multi-value
    //!   enforcement in M2-saml-audience-subject-conf)
    //! - §5.4.3 SubjectConfirmationData.Recipient
    //! - §5.5 partial: AuthnStatement/SessionIndex only
    //!   (SessionNotOnOrAfter + Assertion/@ID + replay in
    //!   M3-saml-replay-protection)
    //!
    //! ## DEFERRED (known gaps)
    //! - ⚠️ §5.4.1 real RSA-SHA256 verification is a STUB.
    //!   See M1-saml-signature-real.
    //! - ⚠️ §5.4.3 SubjectConfirmationMethod NOT enforced
    //!   (any method accepted). See M2.
    //! - ⚠️ §6.3.5 EncryptedAssertion NOT supported (returns
    //!   ProviderError). No mission filed yet.
    //! - ⚠️ §5.4.2 replay / AssertionID NOT enforced (no cache).
    //!   See M3.
    //! - ⚠️ Constant-time compare NOT used. See M4.
    //! - ⚠️ Zeroize NOT used on cert / secrets. See M4.
    //!
    //! Until the DEFERRED items land, this module is
    //! SUITABLE FOR DEVELOPMENT ONLY — production deployments
    //! MUST pin to a SAML IdP that:
    //!   1. Replays assertions only to signed-and-verified SPs,
    //!      AND
    //!   2. Uses audience restrictions compatible with
    //!      single-value matching, AND
    //!   3. Pins AuthnRequest IDs out-of-band (InResponseTo
    //!      correlation is not yet enforced).
    `
- [ ] Replace the false doc on `verify_xml_signature`:
      `rust
    /// ⚠️ STUB: Verifies only that the certificate and
    /// SignatureValue byte blobs are non-empty. Does NOT
    /// load the cert as a public key, canonicalize SignedInfo,
    /// or verify RSA-SHA256. See M1-saml-signature-real.
    fn verify_xml_signature(
        _signed_info_xml: &[u8],
        signature_value: &[u8],
        certificate_der: &[u8],
    ) -> Result<(), SsoError>
    `
- [ ] Add fields to `SamlAssertion`:
      `rust
    pub struct SamlAssertion {
        pub name_id: String,
        pub issuer: Option<String>,        // NEW — Issuer entity
        pub session_index: Option<String>,
        pub assertion_id: Option<String>,   // NEW — Assertion/@ID
        pub attributes: HashMap<String, Vec<String>>,
        pub not_before: DateTime<Utc>,
        pub not_on_or_after: DateTime<Utc>,
    }
    `
- [ ] Parse `Issuer` element text (only the immediate child of
      Assertion or Response). Truncate to 256 chars as
      defense-in-depth.
- [ ] Tests: - `test_saml_issuer_extracted_into_struct` - `test_saml_assertion_id_extracted_into_struct` - `test_saml_module_doc_compiles` (compile-time check via
      `cargo doc --no-deps -- -D warnings`)
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass
- [ ] `cargo doc --no-deps -- -D warnings` passes

## Claimant

(unclaimed)

## Notes

Easiest mission of the M1-M8 cascade. Land FIRST as a
clarity-builder — once the doc accurately reflects what's
implemented vs deferred, downstream contributors and reviewers
have a trustworthy reference.

**Doc accuracy is a security property.** A future contributor
reading the existing doc assumes full SAML 2.0 conformance
and may use this module in a context where the deferred
behaviors would have been load-bearing. The new doc block
makes the gaps unmissable.

**Cross-references:**

- F3-001, F3-011 (doc accuracy)
- F3-013 (assertion_id + issuer fields)
- M3 (replay cache uses `assertion_id`)
- All other M1-M7 (the DEFERRED section enumerates them)
