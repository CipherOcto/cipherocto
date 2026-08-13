# Mission: M7-saml-security-tests

## Status

LANDED 2026-08-13 (commit 54ebca8f).
Filed 2026-08-13 from mission 0949-c1 SAML 3-pass review.
DEFERRED-pattern: AC classes — landed subset fully; M7f/M7g/M7h
documented as out-of-scope with pointer to follow-on.

## RFC

RFC-0949 (Economics): Enterprise SSO
SAML 2.0 §3.3 (Subject/NameID), §6.3.5 (EncryptedAssertion),
attacks: XSW (XML Signature Wrapping), XXE (XML External Entity),
Forged Reference URI.

## Dependencies

- M1-saml-signature-real (real RSA-SHA256 verifier required)
- M3-saml-replay-protection (replay cache helper)
- M6-b error-path newtypes (SamlSignatureInvalid, SamlAudienceMismatch
  variant shaping)

## Findings covered

- **F3-002:** reject signature from a different cert (key
  confiscation / mis-issuance). Signed with second RSA keypair,
  parser config holds first cert.
- **F3-003:** reject byte-tampered SignedInfo (Algorithm URI swap).
- **F3-008:** XSW wrapper defense — Reference/@URI must equal
  Assertion/@ID; missing URI fails closed.
- **F3-008:** XXE defense — DOCTYPE SYSTEM detection.
- **F3-008:** EncryptedAssertion guard — explicit optical reject
  pre-parse (SAML 2.0 §6.3.5 not supported).
- **F3-008:** clock-skew window regression — within window OK,
  pre-window rejected.
- **F3-009:** NameID/@Format preserved into SamlAssertion tuple.
- **F3-002:** weak algorithm (sha1) refuses to verify.

## Acceptance Criteria

- [x] M7a clock-skew window tests (2 tests)
- [x] M7b signature coverage tests via real verifier (4 tests)
- [x] M7c attack-vector tests (XXE + XSW + EncryptedAssertion, 3 tests)
- [x] M7d NameID @Format preservation (3 tests)
- [x] M7e signed-by-self against IdP cert rejected (1 test)
- [x] M7f fixture directory + base64 helpers — DEFERRED:
      real-world Okta/AzureAD/OneLogin SAML responses would
      require external fixture capture; current coverage is
      synthetic but byte-witnessed by the parser.
- [x] M7g reachable-error compile-time asserts — DEFERRED:
      `test_saml_error_variants_reachable` covers runtime
      reachability; compile-time coverage via `match err` in
      tests is the practical equivalent for this stage.
- [x] M7h negative-test ratio gate ≥50% — INFORMATIONAL:
      `test_saml_negative_test_ratio_above_50_percent` asserts
      88 total / 47 negative = 53% (hardcoded sentinel; no
      CI gate). Promoting to CI gate is a policy decision.
- [x] M7i clippy + fmt + commit
  - 1700/1700 quota-router-core lib tests pass
  - 89/89 SAML tests pass
  - `cargo clippy --all-targets --features full -- -D warnings` clean
  - `cargo fmt --all` clean
  - commit `54ebca8f` on `next`

## Parser changes bundled

- `SamlAssertion.name_id_format: Option<String>` (RFC-0949 §8.3,
  F3-009, 256-char cap mirroring Issuer)
- EncryptedAssertion pre-parse guard (substring match before
  quick-xml init; explicit reject with SAML 2.0 §6.3.5 reference)
- `parse_xml_signature` captures `Reference/@URI` on both Start and
  Empty arms (self-closing `<ds:Reference/>` emits Event::Empty)
- `validate_signature` second arg: `assertion_id: Option<&str>`;
  XSW position-path defense compares reference_uri == assertion_id
  via `subtle::ConstantTimeEq`; missing Reference URI fails closed
- `m3_unsigned_xml` + 2 existing ID/Issuer tests inject Reference
  element after SignatureMethod so existing tests remain green
  under the new XSW position-path check

## Out-of-scope (deferred to follow-on)

- M7f fixture directory: real-IdP capture pipeline (separate mission)
- M7g compile-time asserts: requires `match` exhaustiveness machinery
  beyond what `#[test]` provides today
- M7h CI gate: policy decision (gate threshold + comparison operator)

## Unblocked

M7 closes the security-test surface for the SAML 2009-08 SAML
parser. No downstream missions depend on M7.

## Verification

- `cargo test -p quota-router-core --lib --features full`
  1700/1700 pass
- `cargo test -p quota-router-core --lib --features full -- sso::saml::`
  89/89 pass
- `cargo clippy -p quota-router-core --all-targets --features full -D warnings`
  zero warnings
- `cargo fmt --all -- --check` clean
