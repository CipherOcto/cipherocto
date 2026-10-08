# Mission: 0949-c1 — SAML Parser Security Review

## Status

Closed 2026-08-13 (@claude). LANDED as a 3-pass adversarial
review. 38 raw findings → 8 follow-on fix missions filed.

## RFC

RFC-0949 (Economics): Enterprise SSO

## Dependencies

- 0949-c drift closure (2026-08-13)
- 0949-c SAML substrate (commit `0561bb43` + `aaf602e1`)

## Acceptance Criteria

### Adversarial review passes (3 parallel reviewers)

- [x] **Pass 1 — correctness/security** (1 reviewer): 13
      findings + 1 no-finding. XXE clean. Findings
      F1-001 to F1-013.
- [x] **Pass 2 — architecture/crypto-hygiene** (1 reviewer):
      11 findings. F2-001 to F2-011. New attack surface: DoS
      cap missing, error-path swallowing, log injection via
      format!, type-safety gaps.
- [x] **Pass 3 — test/spec/doc** (1 reviewer): 14 findings.
      F3-001 to F3-014. Heavy overlap with Pass 1 (3-way
      consensus on signature stub) = convergence signal.

### Specific attack vectors verified

- [x] XXE protection — clean. `quick_xml::Reader::from_str`
      does not configure EntityResolver, no billion-laughs
      reachable. (F1-014 / Pass 1 no-finding)
- [x] Signature wrapping (XSW) — CRITICAL. Verifier is a stub
      (F1-001 / F2-001 / F3-001 3-way consensus) and verification
      happens AFTER content extraction (F1-002).
- [x] Replay protection — HIGH. Assertion/@ID never extracted
      (F3-004). No replay cache (F1-006). SubjectConfirmationData
      /@NotOnOrAfter unparsed (F1-005).
- [x] Audience enforcement — HIGH. Single-value overwriting in
      multi-AudienceRestriction (F1-004 / F3-005).
- [x] Subject confirmation — HIGH. Method=bearer not enforced
      (F1-003 / F3-006).
- [x] Certificate pinning — HIGH. ProviderConfig::validate
      allows IdP without pinned cert (F1-009).
- [x] Constant-time compare — HIGH. `!=` on audience + recipient
      (F1-007 / F2-003).
- [x] Zeroize — HIGH. Zero `Zeroize` / `ZeroizeOnDrop` across
      2455 LoC (F1-010 / F2-004).
- [x] CSV injection — MEDIUM. Attribute values unescaped only
      (F1-011).
- [x] Algorithm negotiation — LOW. RSA-SHA1 / DSA accepted
      silently (F1-008).
- [x] AuthnStatement SessionNotOnOrAfter — HIGH. Not extracted
      (F3-003).
- [x] NameID Format attribute — HIGH. Not parsed (F3-009).
- [x] DoS via XML size — MEDIUM. No max-size cap (F2-005).
- [x] Error-path swallowing — MEDIUM. 15+
      `unwrap_or_default()` on `unescape_value()` (F2-006).
- [x] Log injection via format! — MEDIUM. Attacker-controlled
      `recip` in error message (F2-007).
- [x] Type safety — MEDIUM. Bare `String` for entity_id /
      audience / recipient / email / secret (F2-008).
- [x] Error variant shape — MEDIUM.
      `SamlAudienceMismatch` is a unit variant (F2-009).
- [x] UTF-8 lossy on SignedInfo — MEDIUM. Bytes mutated before
      sig verify (F2-010).
- [x] Layer discipline — LOW. `Arc<dyn>` + plain `String` for
      secrets (F2-011).
- [x] Doc accuracy — CRITICAL + MEDIUM. Module doc claims full
      SAML 2.0 conformance; function doc claims RSA-SHA256
      verify; neither true (F3-001 / F3-011).
- [x] Test coverage — HIGH. 40 tests, ~42% negative (target
      50%); ~30% cryptographic-negative (F3-002/007/008/009/012
      /014).
- [x] Real-world fixtures — MEDIUM. All 40 tests use hand-
      crafted `format!()` XML; no real SAML responses (F3-010).
- [x] Missing SamlAssertion fields — LOW. No `assertion_id`,
      no `issuer` (F3-013).

### Closure

- [x] Clippy passes with zero warnings (no diff landed by this
      review mission; deferral to follow-ons)
- [x] All existing tests pass (no diff landed)

## Findings + fixes

- [x] All HIGH/CRITICAL findings have a follow-on mission
      pair (M1-M7, M8)
- [x] All MEDIUM findings have a follow-on mission (M2, M4,
      M5, M6, M7, M8)
- [x] All LOW findings documented (F3-013, F1-008 in M5;
      F2-011 in M6)

## Follow-on mission cascade

Findings consolidated into 8 missions ordered by safety + leverage:

1. **M1-saml-signature-real** (CRITICAL, BLOCKER) — replace
   stub with real XML-DSIG verifier; canonicalize SignedInfo;
   ID-based Reference; reject lossy utf8.
2. **M8-saml-docs-fields** (MEDIUM, easiest) — rewrite module
   doc + verify_xml_signature doc to reflect reality; add
   `assertion_id` + `issuer` fields.
3. **M4-saml-crypto-hygiene** (HIGH, small blast) — subtle,
   zeroize, UUID v4.
4. **M2-saml-audience-subject-conf** (HIGH) — multi-audience,
   Method=bearer, error variant struct.
5. **M5-saml-cert-pinning-weak-algo** (HIGH) — algorithm list,
   ProviderConfig::validate requirement.
6. **M3-saml-replay-protection** (HIGH) — Assertion/@ID,
   SubjectConfirmationData NotOnOrAfter, LRU cache.
7. **M6-saml-error-path-types** (MEDIUM, biggest LoC) — DoS
   cap, error propagation, newtypes, log sanitization,
   Layer B port.
8. **M7-saml-security-tests** (HIGH) — real signature tests,
   attack fixtures, negative ratio ≥50%.

M1 must land FIRST (everything else is moot without a real
verifier). M8 lands SECOND (clarity-builder so subsequent
contributors see accurate docs). M4, M2, M5, M3, M6 can land
in any order after M8. M7 lands LAST (depends on M1-M6 to
exercise the real attack paths).

## Claimant

(@claude)

## Pull Request

(in progress)

## Notes

**3-way convergence signal.** 4 findings achieved 3-way
consensus across all 3 reviewers:

- F1-001 / F2-001 / F3-001 — signature stub (CRITICAL)
- F1-007 / F2-003 — constant-time compare (HIGH)
- F1-010 / F2-004 — zeroize (HIGH)
- F1-012 / F2-002 — AuthnRequest ID from nanos (HIGH)

These are the highest-confidence findings; M1, M4 land these.

**Conflict resolution (Pass 1 vs Pass 3 on audience):** F1-004
says strict-mode (reject if other audience present); F3-005
says spec-correct ANY-match. Per SAML 2.0 §2.5.1.4, BOTH
satisfy the spec; strict is safer. Resolved in M2: default
strict, allow-list override via config.

**Why 3 reviewers × 1 lens each (not 3 reviewers × 3 lenses
each):** the lens separation produces independent perspectives;
cross-pollination between reviewers would re-converge on the
same findings. With distinct lenses, Pass 2 surfaced DoS-cap +
newtype-typo + log-injection that Pass 1 missed; Pass 3
surfaced doc-accuracy + NameID-Format + test-coverage-gaps
that Passes 1+2 missed. The 38-finding total = sum of
independent attention.

**Files covered by review:**

- `crates/quota-router-core/src/auth/sso/saml.rs` (1722 LoC)
- `crates/quota-router-core/src/auth/sso/mod.rs` (733 LoC)
- `crates/quota-router-core/src/admin.rs:568-580` (SAML
  endpoint dispatch)

## Cross-references

- RFC-0949 (Enterprise SSO)
- SAML 2.0 core spec (§2.5.1.4, §3.3, §5.4.1, §5.4.2,
  §5.4.3, §5.5, §6.3.5)
- OWASP SAML Cheat Sheet (XSW, XXE patterns)
- Mission `0949-c` (SAML substrate commit `0561bb43` +
  `aaf602e1`)
- Follow-on missions M1-M8 (each filed with full AC + attack
  scenario + fix proposal)

## Version History

| Version | Date       | Status | Change                                                                                                                                                                                                                                                                                                                                                                                                                     |
| ------- | ---------- | ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | open   | Mission filed. 0949-c drift closure follow-on. ~25 ACs across 3 passes.                                                                                                                                                                                                                                                                                                                                                    |
| v0.2    | 2026-08-13 | closed | LANDED as 3-pass adversarial review. 38 raw findings (Pass 1: 13+1nf, Pass 2: 11, Pass 3: 14) → 8 follow-on fix missions. 3-way consensus on signature stub (CRITICAL), constant-time compare (HIGH), zeroize (HIGH), AuthnRequest ID predictability (HIGH). Pass 1 vs Pass 3 conflict on audience resolved: strict mode default, allow-list override per SAML 2.0 §2.5.1.4. Cascade order: M1 → M8 → M4/M2/M5/M3/M6 → M7. |
