# Mission: M2-saml-audience-subject-conf

## Status

LANDED 2026-08-13 (commit pending).
Filed 2026-08-13 from mission 0949-c1 SAML 3-pass review.
HIGH priority. Closed in cascade after M1.

## RFC

RFC-0949 (Economics): Enterprise SSO
SAML 2.0 §2.5.1.4 (AudienceRestriction), §3.3 (Subject /
NameID), §5.4.3 (SubjectConfirmation)

## Dependencies

- `crates/quota-router-core/src/auth/sso/saml.rs`
- M1-saml-signature-real (signature must be real before audience
  enforcement matters; otherwise attacker controls Audience)

## Findings covered

- **F1-003 / F3-006:** `SubjectConfirmationData/@Recipient` parsed
  but `@ConfirmationMethod` never read. Bearer-only check missing.
- **F1-004 / F3-005:** Single `audience: Option<String>` silently
  collapses multi-`<AudienceRestriction>` blocks. F1-004 says
  strict reject-if-other; F3-005 says spec-correct "ANY match".
  Resolved per SAML 2.0 §2.5.1.4: accept if `audiences` contains
  `sp_entity_id` AND the audience list is bounded (1 entry if
  strict mode, N entries if allow-list mode — operator-configurable).
- **F2-009:** `SsoError::SamlAudienceMismatch` is a unit variant
  carrying no payload; triage-incompatible. Promote to struct
  `{ expected: String, actual: String }`.

## Acceptance Criteria

- [x] Change parser state from `audience: Option<String>` to
      `audiences: Vec<String>` — collect every `<Audience>` text
      across every `<AudienceRestriction>` in the assertion's
      `<Conditions>`.
- [x] Add `SamlConfig.strict_audience: bool` (default true): - `true`: reject unless `audiences.len() == 1 &&
      audiences[0] == sp_entity_id` - `false`: reject unless `audiences.contains(&sp_entity_id)`
- [x] Read `SubjectConfirmation/@Method`; require
      `urn:oasis:names:tc:SAML:2.0:cm:bearer`. Reject other methods
      with new error variant `SamlSubjectConfirmationInvalid { method }`.
- [x] Promote `SamlAudienceMismatch` to
      `SamlAudienceMismatch { expected: String, actual: Vec<String> }`.
      Update every pattern-match site (saml.rs:356, tests at 1035,
      1311; mod.rs handler sites).
- [x] Replace `!=` comparisons on `audiences`/`recipients` with
      `subtle::ConstantTimeEq` (overlaps with M4).
- [x] Tests: - `test_saml_multi_audience_match_strict` - `test_saml_multi_audience_no_match_strict` - `test_saml_multi_audience_allow_list_match` - `test_saml_subject_confirmation_method_bearer_ok` - `test_saml_subject_confirmation_method_sender_vouches_rejected` - `test_saml_subject_confirmation_method_holder_of_key_rejected` - `test_saml_audience_mismatch_error_carries_payload`
- [x] Clippy passes with zero warnings
- [x] All existing tests pass

## Claimant

(unclaimed)

## Notes

F1-004 vs F3-005 conflict resolution: SAML 2.0 §2.5.1.4 says
"the relying party MUST be a member of the audience". Both
strict (1 audience = us) and lenient (N audiences including us)
satisfy the spec; the strict interpretation is safer. Default
strict, allow override via config.

**Cross-references:**

- F1-003, F3-006 (subject-confirmation)
- F1-004, F3-005 (audience) — 3-way conflict resolved here
- F2-009 (error variant shape) — same PR
- M4 (constant-time) — overlap on comparison
