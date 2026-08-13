# Mission: 0949-c1 — SAML Parser Security Review

## Status

Open. Multi-round adversarial review of the SAML 2.0 parser substrate
landed in 0949-c (commit `0561bb43` + `aaf602e1`). Total surface:
1722 LoC XML/crypto code in `crates/quota-router-core/src/auth/sso/saml.rs`.

## RFC

RFC-0949 (Economics): Enterprise SSO

## Dependencies

- 0949-c drift closure (2026-08-13)

## Acceptance Criteria

### Adversarial review passes (3 parallel reviewers)

- [ ] **Pass 1 — correctness/security:** 3 reviewers (XML signature
  verification, SAML protocol semantics, certificate chain validation).
  Findings ranked by severity.
- [ ] **Pass 2 — design/architecture:** 2 reviewers (parser architecture,
  crypto hygiene including zeroize, constant-time compare).
- [ ] **Pass 3 — test/docs:** 1 reviewer (test coverage vs SAML 2.0
  spec, doc accuracy).

### Specific attack vectors to verify

- [ ] **XXE protection** — `quick-xml` configuration disables external
  entity resolution; verify against billion-laughs / XXE payloads
- [ ] **Signature wrapping** — verify enveloped signature handling
  matches SAML 2.0 §5.4 (XPath transform), not vulnerable to
  XSW (XML Signature Wrapping) attacks
- [ ] **Replay protection** — verify `NotBefore` / `NotOnOrAfter`
  enforced; clock skew tolerance ≤ 300s; check `Conditions/@NotOnOrAfter`
  + `Assertion/@IssueInstant` + `AuthnStatement/@SessionNotOnOrAfter`
- [ ] **Audience enforcement** — verify multi-audience restriction
  (reject if any other audience present beyond sp_entity_id)
- [ ] **Subject confirmation** — verify `Subject/@ConfirmationMethod`
  = `urn:oasis:names:tc:SAML:2.0:cm:bearer` + `SubjectConfirmationData/@Recipient`
  = ACS URL + `SubjectConfirmationData/@NotOnOrAfter` enforced
- [ ] **Certificate pinning** — verify IdP cert fixed per-provider,
  not fetched from IdP metadata without explicit operator opt-in
- [ ] **Constant-time compare** — verify `_eq` for signature,
  audience, recipient, issuer comparisons (no early-exit timing
  side-channels)
- [ ] **Zeroize** — verify secret material (private key, session
  secret) zeroized on drop; certs can stay (public) but preferred
- [ ] **CSV injection** — if attribute values flow into CSV/JSON
  responses, ensure proper escaping
- [ ] **Algorithm negotiation** — RSA-SHA256 mandated over RSA-SHA1;
  verify weak algos rejected

### Findings + fixes

- [ ] All HIGH/CRITICAL findings have a fix-and-test or follow-on
  mission pair
- [ ] All MEDIUM findings have a follow-on mission
- [ ] All LOW findings documented in module docs

### Closure

- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/src/auth/sso/saml.rs` (1722 LoC substrate)
- `crates/quota-router-core/src/auth/sso/mod.rs` — SAML types
- `crates/quota-router-core/src/admin.rs:568-580` — SAML endpoints

**Why this matters** — SAML is the highest-exposure auth surface in
the codebase. A single XML signature wrapping bug allows IdP
impersonation. Replay protection failures allow stolen assertion
re-use. Audience enforcement failures allow cross-SP authentication
confusion. RFC-0949 §SSO inherits these attack surfaces from SAML
2.0; the parser must defend against every published XXE/XSW attack
in the OWASP SAML Cheat Sheet.

**Estimates** — 3 reviewers × 3 passes = 9 review units. Likely
5-8 HIGH/CRITICAL findings, 10-15 MEDIUM, 20+ LOW. Plan 3-week
follow-on mission cascade after pass compaction.

## Version History

| Version | Date       | Change                                                                                                |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | Mission filed. 0949-c drift closure follow-on. ~25 ACs across 3 passes. |

Last Updated: 2026-08-13
Version: 0.1
