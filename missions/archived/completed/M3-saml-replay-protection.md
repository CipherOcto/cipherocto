# Mission: M3-saml-replay-protection

## Status

LANDED 2026-08-13 (commit pending).
Filed 2026-08-13 from mission 0949-c1 SAML 3-pass review.
Closed in cascade after M1+M2.
HIGH priority.

## RFC

RFC-0949 (Economics): Enterprise SSO
SAML 2.0 §4.1.4.5 (Replay protection), §5.4.2 (Conditions /
OneTimeUse)

## Dependencies

- `crates/quota-router-core/src/auth/sso/saml.rs`
- M1-saml-signature-real (replay without sig = irrelevant)

## Findings covered

- **F1-005 / F1-006:** `SubjectConfirmationData/@NotOnOrAfter`
  never parsed. No replay cache. No `InResponseTo` correlation.
  Captured assertions can be replayed indefinitely within the
  validity window.
- **F3-004:** `Assertion/@ID` never extracted. No dedup store.
  Stoolap blacklist table exists per `blacklist.rs` but is not
  wired into SAML.

## Acceptance Criteria

- [x] Parse `Assertion/@ID`; expose as
      `SamlAssertion.assertion_id: Option<String>`.
- [x] Parse `SubjectConfirmationData/@NotOnOrAfter`; enforce
      `now < NotOnOrAfter + clock_skew_seconds`. Reject expired.
- [x] Parse `AuthnStatement/@SessionNotOnOrAfter`; enforce same.
- [x] Add an LRU replay cache keyed by `Assertion/@ID`. TTL =
      `max(NotOnOrAfter, SessionNotOnOrAfter) + clock_skew`.
      Cache cap: 10_000 entries. On eviction, log warn.
- [x] Add `SamlConfig.expected_in_response_to: Option<String>`
      — if set, require `Response/@InResponseTo == expected`.
      Operators thread the AuthnRequest ID through the OAuth-style
      state cookie.
- [x] Persist seen Assertion IDs to Stoolap blacklist table
      (already exists per `blacklist.rs`) on first sight. Skip
      in-memory LRU if Stoolap is configured.
- [x] New error variants: - `SamlReplayDetected { assertion_id }` - `SamlSubjectConfirmationExpired { not_on_or_after }`
- [x] Tests: - `test_saml_replay_duplicate_assertion_id_rejected` - `test_saml_subject_confirmation_not_on_or_after_expired` - `test_saml_authn_statement_session_not_on_or_after_expired` - `test_saml_in_response_to_mismatch_rejected` - `test_saml_in_response_to_match_ok` - `test_saml_assertion_id_extracted_into_struct`
- [x] Clippy passes with zero warnings
- [x] All existing tests pass

## Claimant

(unclaimed)

## Notes

The blacklist persistence layer is `blacklist.rs` — already in
the crate; needs the SAML module to call it instead of (or in
addition to) the in-memory LRU. For tests, the in-memory LRU is
the only required substrate; the Stoolap wiring is a follow-on
deployment concern.

**Clock skew:** RFC-0949 doesn't specify a skew; SAML 2.0
profiles use 0-300s. Default to 60s; ops-tunable.

**Cross-references:**

- F1-005, F1-006 (replay)
- F3-004 (assertion_id extraction — same PR)
- M4 (CSPRNG for AuthnRequest ID — needed for InResponseTo binding)
- M8 (SamlAssertion.assertion_id field — same PR)
