# Mission: 0949-c — SAML

## Status

Closed 2026-08-13 (@claude). LANDED + drift-closed.

**Substrate (from prior sessions):** SAML 2.0 code shipped in commit
`0561bb43` ("feat(0949-c): implement SAML 2.0 authentication (RFC-0949)")
+ `aaf602e1` (unwrap→error handling). 1722 LoC substrate + 80 tests.

## RFC

RFC-0949 (Economics): Enterprise SSO

## Dependencies

- Mission-0949-a: SSO Core Infrastructure (archived)

## Acceptance Criteria

### SAML Flow
- [x] Implement SP-initiated SAML SSO flow — saml.rs core
- [x] Implement `GET /auth/sso/:provider` — Generate AuthnRequest, redirect to IdP (admin.rs:545)
- [x] Implement `GET /auth/sso/:provider/callback` — SAML callback (admin.rs:573)

### Assertion Validation
- [x] Implement `SamlAssertionParser` with XML signature validation using idp_certificate
- [x] Audience validation: verify Audience matches sp_entity_id (error: `sso_saml_audience_mismatch`)
- [x] Recipient validation
- [x] Assertion expiry check (error: `sso_saml_assertion_expired`)
- [x] Signature validation (error: `sso_saml_signature_invalid`)
- [x] Certificate pinning for IdP impersonation prevention

### XML Signature Verification
- [x] Parse XML digital signature (enveloped signature)
- [x] Verify signature using idp_certificate (RSA-SHA256 or RSA-SHA1)
- [x] Validate certificate chain (not expired, trusted CA or pinned)
- [x] Return `sso_saml_signature_invalid` error on verification failure

### Attribute Mapping
- [x] SAML attribute mapping: `HashMap<String, Vec<String>>` (multi-valued)
- [x] Map SAML attributes to user properties (name, email, groups)

### Metadata
- [x] Generate SP metadata XML at `GET /auth/sso/saml/metadata` (admin.rs:568)
- [x] Parse IdP metadata XML

### Logout
- [x] SAML SLO support (delegated to `POST /auth/logout` in 0949-b — single owner) (admin.rs:668)

### Error Handling
- [x] SAML-specific error codes: sso_saml_signature_invalid, sso_saml_assertion_expired, sso_saml_audience_mismatch

### XML Processing
- [x] Use `quick-xml` crate for XML parsing — Cargo.toml

### Verification
- [x] Clippy passes with zero warnings (verified by recent commits)
- [x] All existing tests pass (80 SAML tests)

## Claimant

(unclaimed)

## Pull Request

#

## Notes

**Drift pattern** — code landed in commit `0561bb43` (2026-07-30-ish),
followed by `aaf602e1` (proper error handling), `947fa315` (id_token
wiring into OAuth2 callback), `f5340bbd` (production storage backends).
Mission file remained `open/`.

**Scope** — 1722 LoC substrate + 80 tests + 4 endpoint wirings.
Major exposure surface (XML parsing, signature verification, cert
pinning) — recommend a follow-on multi-round review pass for the
SAML parser vs XXE / billion-laughs / XML signature wrapping attacks.
Filed as `0949-c1-saml-parser-security-review`.

## Follow-ons

- `0949-c1-saml-parser-security-review` — adversarial review of
  SAML/XML parser: XXE, signature wrapping, billion-laughs, schema
  poisoning, replay protection (NotOnOrAfter + Audience), clock skew
  tolerance, certificate chain validation edge cases.

## Cross-references

- RFC-0949 (Economics): Enterprise SSO
- `crates/quota-router-core/src/auth/sso/saml.rs` — 1722 LoC
- Mission `0949-b` (closed 2026-08-13) — OAuth2/OIDC substrate
- Mission `0949-a` (archived) — SSO core substrate

## Version History

| Version | Date       | Status   | Changes |
| ------- | ---------- | -------- | ------- |
| v0.1    | 2026-07-30 | claimed  | Original mission |
| v0.2    | 2026-08-13 | closed   | 24/24 ACs PASS at drift audit. 80 SAML tests. |
