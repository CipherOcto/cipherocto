# Mission: M1-saml-signature-real

## Status

Open. Filed 2026-08-13 from mission 0949-c1 SAML 3-pass review.
CRITICAL priority — production-unsafe.

## RFC

RFC-0949 (Economics): Enterprise SSO

## Dependencies

- `crates/quota-router-core/src/auth/sso/saml.rs` (1722 LoC)
- Mission 0949-c (commit `0561bb43` + `aaf602e1`)

## Findings covered

- **F1-001 / F2-001 / F3-001 (CRITICAL, 3-way consensus):**
  `verify_xml_signature` is a non-functional stub. Accepts any
  non-empty cert+sig pair, returns `Ok(())` after a `tracing::warn!`.
  Doc comment claims RSA-SHA256 + SignedInfo digest check; code does
  neither. Every signed assertion passes.
- **F1-002 (CRITICAL):** XSW — signature verified AFTER all assertion
  content harvested. SAML 2.0 profile §4.1.4.3/4.1.4.5 requires
  signature FIRST + ID-resolved reference, not position-based.
- **F2-010 (MEDIUM):** `String::from_utf8_lossy` on the SignedInfo
  capture path (line 232 + 540) silently mutates bytes — even with
  a real verifier, the bytes being verified would diverge from
  the IdP's signed bytes.

## Acceptance Criteria

- [ ] Replace `verify_xml_signature` stub with real XML-DSIG
      verification. Acceptable implementations: - `xmlsec` crate (preferred for full SAML compliance) - `x509-parser` + `ring`/`rsa` + manual C14N11 canonicalization
      (escape-hatch for size constraints)
- [ ] Verify RSA-SHA256 (and RSA-SHA384/512 if offered) over
      canonicalized SignedInfo. Reject RSA-SHA1 and DSA.
- [ ] Verify `Reference DigestValue` matches canonicalized
      assertion digest (SHA-256 minimum).
- [ ] Verify signature BEFORE extracting assertion content
      (NameID / Conditions / SubjectConfirmationData / Attributes).
- [ ] Build a `Reference` resolver that uses ID attribute
      (`xml:id` / SAML `ID` registration) so the signed assertion
      is matched by ID, not position.
- [ ] Reject if SignedInfo's Reference URI does not point at the
      top-level assertion by ID.
- [ ] In the SignedInfo capture path, replace `from_utf8_lossy`
      with `std::str::from_utf8(...).map_err(...)?` — hard error
      on invalid UTF-8.
- [ ] Update doc comment on `verify_xml_signature` to reflect the
      real behavior (drop the false RSA-SHA256 claim).
- [ ] Add tests (gated `#[ignore]` if the chosen crypto crate is
      not yet a dependency): - `test_saml_signature_valid_rsa_sha256` — real key pair - `test_saml_signature_byte_tamper_rejected` - `test_saml_signature_wrong_cert_rejected` - `test_saml_signature_weak_algorithm_rejected`
- [ ] Clippy passes with zero warnings
- [ ] All existing tests still pass

## Claimant

(unclaimed)

## Notes

This is the single biggest blocker for SAML production safety.
Land before any other SAML mission — without it, every downstream
check (audience / replay / subject-conf) is moot since unsigned
attacker payloads sail through.

**Layer discipline:** the new verifier is Layer-B (depends only
on `ring`/`rsa`/`x509-parser` + canonical codec). It does NOT
depend on the http layer or any Layer C node.

**Crypto crate choice:** `xmlsec` is a libxml2 binding with
heavy build dependencies (~10 min cold build). For a smaller
build surface, `x509-parser` + `rsa` + manual C14N11 is workable
but reinvents ~500 LoC of canonicalization. Pick based on
operational cost vs maintenance cost.

**Cross-references:**

- F1-001 / F2-001 / F3-001 — 3-pass consensus on signature stub
- F1-002 — XSW position-vs-id attack
- F2-010 — from_utf8_lossy on SignedInfo byte mutation
- M2 (audience), M3 (replay), M7 (tests) all blocked on this
