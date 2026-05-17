# Mission: 0949-c — SAML

## Status

Open

## RFC

RFC-0949 (Economics): Enterprise SSO

## Dependencies

- Mission-0949-a: SSO Core Infrastructure

## Acceptance Criteria

### SAML Flow
- [ ] Implement SP-initiated SAML SSO flow
- [ ] Implement `GET /auth/sso/:provider` — Generate AuthnRequest, redirect to IdP
- [ ] Implement `GET /auth/sso/:provider/callback` — SAML callback (validates assertion, creates session)

### Assertion Validation
- [ ] Implement `SamlAssertionParser` with XML signature validation using idp_certificate
- [ ] Audience validation: verify Audience matches sp_entity_id (error: `sso_saml_audience_mismatch`)
- [ ] Recipient validation
- [ ] Assertion expiry check (error: `sso_saml_assertion_expired`)
- [ ] Signature validation (error: `sso_saml_signature_invalid`)
- [ ] Certificate pinning for IdP impersonation prevention

### Attribute Mapping
- [ ] SAML attribute mapping: `HashMap<String, Vec<String>>` (multi-valued)
- [ ] Map SAML attributes to user properties (name, email, groups)

### Metadata
- [ ] Generate SP metadata XML at `GET /auth/sso/saml/metadata`
- [ ] Parse IdP metadata XML

### Logout
- [ ] SAML SLO support (delegated to `POST /auth/logout` in 0949-b — single endpoint handles both OAuth2 and SAML)

### Error Handling
- [ ] SAML-specific error codes: sso_saml_signature_invalid, sso_saml_assertion_expired, sso_saml_audience_mismatch

### XML Processing
- [ ] Use `quick-xml` crate for XML parsing (standard Rust XML library)

### Verification
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/src/auth/sso/saml.rs` — New
- `crates/quota-router-core/src/admin.rs` — Add SAML endpoints
