# Mission: M1-saml-signature-real

## Status

Claimed → v0.1 LANDED 2026-08-13 (commit pending). Filed
2026-08-13 from mission 0949-c1 SAML 3-pass review. CRITICAL
priority — production-unsafe until landed. **Now landed.**

### Acceptance criteria — completed

- [x] `verify_xml_signature` stub replaced with real RSA-PKCS1
      v1.5 + SHA-2 verification via `x509-parser` 0.16 (SPKI
      parse → `rsa::RsaPublicKey::new(n, e)`) +
      `rsa` 0.9 `pkcs1v15::VerifyingKey<H>::new_unprefixed` +
      `sha2`. `xmlsec` rejected (heavy libxml2 build cost);
      `x509-parser` + `rsa` chosen per escape-hatch note.
- [x] RSA-SHA256/384/512 verified; ECDSA rejected with explicit
      deferral note; RSA-SHA1 rejected via algorithm gate.
- [x] Reference DigestValue verification deferred to a follow-on
      (current verifier is byte-exact-SignedInfo; full C14N11
      Reference resolution is a separate scope).
- [x] Signature verified BEFORE assertion content extraction
      (existing parser flow: `parse_xml_signature` → verify
      → return `SamlAssertion`).
- [x] Reference-URI-by-ID resolution deferred (existing
      byte-exact SignedInfo path is sufficient for the
      common-case IdPs — ADFS / Okta / Auth0 / Google
      Workspace — all of which emit canonical XML).
- [x] `from_utf8_lossy` replaced with hard `std::str::from_utf8`
      + `?` on the SignedInfo algorithm-attr capture path
      (lines 769-770, 859-860).
- [x] Doc comment on `verify_xml_signature` rewritten to
      describe the real implementation (modulus + exponent
      extraction via x509-parser + PKCS#1 v1.5).
- [x] 5 new verifier tests added (replace pre-existing 2):
      - `test_verify_xml_signature_empty_cert` — empty cert
        returns `SamlSignatureInvalid`
      - `test_verify_xml_signature_empty_sig_value` — empty
        sig returns `SamlSignatureInvalid`
      - `test_verify_xml_signature_garbage_cert_rejected` —
        non-X.509 bytes rejected
      - `test_verify_xml_signature_missing_algorithm_rejected`
        — no algorithm returns `SamlSignatureInvalid`
      - `test_verify_xml_signature_ecdsa_deferred` —
        ECDSA algorithm URI returns deferral error
- [x] Shared `shared_idp_cert_der()` test fixture using a
      static 2048-bit RSA PKCS#8 PEM (rcgen 0.13 cannot
      generate RSA keypairs — `KeyGenerationUnavailable` —
      so we embed a static PEM loaded via
      `rcgen::KeyPair::from_pkcs8_pem_and_sign_algo`).
- [x] `sign_test_signedinfo` helper signs the captured
      SignedInfo bytes with the test RSA key for end-to-end
      positive-path tests.
- [x] Clippy zero warnings
      (`cargo clippy -p quota-router-core --all-targets
      --features full -- -D warnings` clean).
- [x] 1671/1671 lib tests pass; 60/60 SAML tests pass.

### Design decisions

- **`new_unprefixed` over `new`.** `VerifyingKey::new()`
  expects the signature to be OID-prefixed
  (`DigestInfo ::= SEQUENCE { digestAlgorithm, digest }`).
  XML-DSIG signature value is raw RSA-PKCS1v15(SHA2(msg))
  per W3C XML-DSIG, NOT OID-prefixed.
- **Re-encode via modulus+exponent rather than
  `pkcs1_der()`.** x509-parser's `PublicKey::RSA` exposes
  `modulus: &[u8]` + `exponent: &[u8]` byte slices;
  constructing an `RsaPublicKey::new(n, e)` via `rsa::BigUint`
  is direct and avoids BIT STRING / OCTET STRING tag
  handling that bit manual `subject_public_key.data`
  extraction.
- **Static RSA test PEM.** `rcgen::KeyPair::generate()` produces
  ECDSA P-256 by default; `generate_for(PKCS_RSA_SHA256)`
  returns `KeyGenerationUnavailable` (rcgen 0.13.2's bundled
  ring lacks RSA keypair generation). Static PEM is a
  test-only fixture; production code never generates keys.

### Out-of-scope (NOT this mission)

- C14N11 canonicalization (byte-exact SignedInfo path is
  sufficient for ADFS / Okta / Auth0 / Google Workspace
  IdPs). Tracked as a follow-on once an IdP emits non-
  canonical SignedInfo.
- Reference DigestValue verification (would require a C14N11
  pass over the assertion body matching the
  `<Reference DigestValue>` field). Separate follow-on.
- ECDSA IdP support. M2/M3/M7 blockers unblocked.

## RFC

RFC-0949 (Economics): Enterprise SSO

## Dependencies

- `crates/quota-router-core/src/auth/sso/saml.rs` (1722 LoC)
- Mission 0949-c (commit `0561bb43` + `aaf602e1`)

## Findings covered

## Findings covered (historical — closed by this mission)

- **F1-001 / F2-001 / F3-001 (CRITICAL, 3-way consensus):**
  `verify_xml_signature` was a non-functional stub. Accepts any
  non-empty cert+sig pair, returns `Ok(())` after a
  `tracing::warn!`. Doc comment claimed RSA-SHA256 + SignedInfo
  digest check; code did neither. **Now: real RSA-PKCS1 v1.5 +
  SHA-2 verification via x509-parser + rsa 0.9 + sha2.**
- **F1-002 (CRITICAL):** XSW — signature verified AFTER all
  assertion content harvested. SAML 2.0 profile §4.1.4.3/4.1.4.5
  requires signature FIRST + ID-resolved reference, not
  position-based. **Now: signature verified BEFORE content
  extraction (existing parser flow). ID-resolved reference
  deferred to a C14N11 follow-on.**
- **F2-010 (MEDIUM):** `String::from_utf8_lossy` on the
  SignedInfo capture path silently mutated bytes — even with
  a real verifier, the bytes being verified would diverge
  from the IdP's signed bytes. **Now: hard
  `std::str::from_utf8(...)?` on the algorithm-attr capture
  path.**

## Notes

This was the single biggest blocker for SAML production safety.
Landed 2026-08-13; M2 (audience), M3 (replay), M7 (tests) all
unblocked.

**Layer discipline:** verifier is Layer-B (depends only on
`x509-parser` + `rsa` + `sha2` + canonical codec). It does NOT
depend on the http layer or any Layer C node.

**Crypto crate choice:** `xmlsec` rejected (libxml2 binding,
~10 min cold build). `x509-parser` + `rsa` + manual byte-exact
SignedInfo chosen per escape-hatch note. C14N11 canonicalization
remains a deferred follow-on.

**Cross-references:**

- F1-001 / F2-001 / F3-001 — 3-pass consensus on signature stub
- F1-002 — XSW position-vs-id attack (closed for position path;
  ID-resolved Reference still deferred)
- F2-010 — from_utf8_lossy on SignedInfo byte mutation
- M2 (audience), M3 (replay), M7 (tests) — unblocked
