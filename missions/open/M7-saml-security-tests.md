# Mission: M7-saml-security-tests

## Status

Open. Filed 2026-08-13 from mission 0949-c1 SAML 3-pass review.
HIGH priority.

## RFC

RFC-0949 (Economics): Enterprise SSO
OWASP SAML Cheat Sheet (XSW, XXE, replay coverage)

## Dependencies

- `crates/quota-router-core/src/auth/sso/saml.rs`
- `tests/fixtures/saml/` (NEW directory)

## Findings covered

- **F3-002:** 40 tests, but signature-coverage tests feed the
  stub. Real cryptographic coverage = zero.
- **F3-007:** No clock-skew negative path. Existing
  `test_saml_assertion_expired` uses 2020-01-01 (far out of
  skew range); doesn't exercise the 30s tolerance.
- **F3-008:** Zero tests for EncryptedAssertion (§6.3.5),
  XSW, XXE, replay.
- **F3-009:** NameID `@Format` attribute never parsed.
- **F3-010:** All 40 tests use hand-crafted `format!()` XML.
  No base64-encoded real SAML responses.
- **F3-012:** Negative-test ratio 42% (target ≥50%);
  cryptographic-negative ratio 30% (target ≥50%).
- **F3-014:** Six SAML error variants defined; only 3 reachable
  from `parse()`. No test for `SamlConditionError`,
  `SamlReplayDetected`.

## Acceptance Criteria

- [ ] Add tests (gated `#[ignore]` if M1 not landed): - `test_saml_signature_valid_rsa_sha256` - `test_saml_signature_byte_tamper_rejected` - `test_saml_signature_wrong_cert_rejected` - `test_saml_signature_weak_algorithm_rejected`
- [ ] Clock-skew tests: - `test_saml_clock_skew_within_window_ok`
      (NotOnOrAfter = now+10s, skew=30 → Ok) - `test_saml_clock_skew_pre_window_rejected`
      (NotBefore = now+1h, skew=30 → Err) - `test_saml_clock_skew_post_window_rejected`
- [ ] Security attack tests: - `test_saml_encrypted_assertion_rejected` - `test_saml_xsw_wrapper_rejected` - `test_saml_xxe_payload_rejected`
      (DOCTYPE SYSTEM 'file:///etc/passwd') - `test_saml_replay_duplicate_assertion_id_rejected` - `test_saml_signed_by_self_against_idp_cert_rejected` - `test_saml_ahead_of_notbefore_in_skew_rejected`
- [ ] NameID Format tests: - `test_saml_nameid_format_email_preserved` - `test_saml_nameid_format_persistent_preserved` - `test_saml_nameid_format_unspecified_ok`
- [ ] Real-world fixtures under `tests/fixtures/saml/`: - `okta_response.xml.b64` (anonymized — strip real user
      data before committing) - `azuread_response.xml.b64` - `onelogin_response.xml.b64` - `signed_test_vector.xml` (from `python-saml` test suite
      or `saml-tools` reference output) - Each fixture < 20 KiB; commit messages MUST contain
      provenance (IdP, date, anonymization log).
- [ ] Reachable-error test: - Assert (compile-time) that
      `SamlReplayDetected { .. }`,
      `SamlSubjectConfirmationInvalid { .. }`,
      `SamlAuthnStatementExpired`,
      `SamlMissingAssertionId` are constructable via
      `SsoError::from(...)` paths. Each variant must have
      ≥1 negative test exercising it.
- [ ] Negative-test ratio target: ≥50% of total SAML tests
      must be negative (must-fail). True cryptographic
      negative target: ≥50% of signature-coverage tests.
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Claimant

(unclaimed)

## Notes

Real-world fixtures MUST be anonymized before commit — strip
real NameID values, real email addresses, real session indexes.
A fixture-anonymization helper script is acceptable (kept under
`scripts/saml-anonymize.py`, gitignored per CLAUDE.md §docs
scratchpad rules).

**Cross-crate test path:** fixtures live under
`tests/fixtures/saml/` so both unit tests inside `saml.rs` and
integration tests under `tests/` can reference them. The
`include_bytes!` macro is preferred over runtime file IO for
deterministic builds.

**Test isolation:** tests that depend on M1 (real verifier) are
`#[ignore]` until M1 lands. CI pipeline can flip the gate via
`RUSTFLAGS="--cfg saml_real_verifier"` once M1 lands.

**Cross-references:**

- F3-002, F3-007, F3-008, F3-009, F3-010, F3-012, F3-014
- M1 (real verifier — gated tests)
- M3 (replay cache — gated tests)
- M8 (assertion_id field — required for F3-004 / F3-014 tests)
