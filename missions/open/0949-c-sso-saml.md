# Mission: 0949-c — SAML

## Status

Open

## RFC

RFC-0949 (Economics): Enterprise SSO

## Dependencies

- Mission-0949-a: SSO Core Infrastructure

## Acceptance Criteria

- [ ] Implement SP-initiated SAML SSO flow
- [ ] Implement `SamlAssertionParser` with XML signature validation
- [ ] Audience and recipient checks
- [ ] SAML attribute mapping: `HashMap<String, Vec<String>>` (multi-valued)
- [ ] Map SAML attributes to user properties (name, email, groups)
- [ ] Generate SP metadata XML
- [ ] Parse IdP metadata XML
- [ ] Implement `POST /auth/saml/callback` endpoint
- [ ] Implement `POST /auth/logout` with SAML SLO support
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
