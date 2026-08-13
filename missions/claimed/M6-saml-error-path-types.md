# Mission: M6-saml-error-path-types

## Status

Claimed → v0.4 STAGE-1 CLOSED 2026-08-13. Filed from
mission 0949-c1 SAML 3-pass review. MEDIUM priority,
biggest LoC.

**Two further stages DEFERRED to follow-on mission
`M6-b-error-path-newtypes`** — see "Deferred work" below.

### Stage 1 — closed (this landing)

Acceptance criteria completed:

- [x] `const MAX_SAML_XML_BYTES: usize = 64 * 1024` landed at
      `crates/quota-router-core/src/auth/sso/saml.rs:178`
      (next to `SamlAssertionParserImpl` impl block). `parse()`
      early-rejects above this size with descriptive
      `ProviderError` (length-only echo).
- [x] 13 mechanical replacements: every
      `attr.unescape_value().unwrap_or_default().to_string()`
      and `unescape(...).unwrap_or_default()` site now
      propagates errors via `.map_err(...)?`. Quick-xml's
      unescape rarely fails on well-formed XML; the explicit
      error path catches malformed DTD entities (XSW / XXE
      hardening) and unknown-namespace references. The error
      message names the failure mode for operators debugging
      IdP integrations.
- [x] Recipient mismatch error now length-only
      (`expected len=N, got len=M`). Already landed in M4;
      here it gets an explicit regression test.
- [x] 3 new tests:
      `test_saml_xml_oversize_rejected`,
      `test_saml_oversize_at_boundary_accepted_size_check`
      (boundary = exactly 64 KiB), and
      `test_saml_recipient_mismatch_error_no_attacker_data`
      (asserts the error message does NOT echo the attacker
      URI bytes — OWASP A09 logging hygiene).
- [x] `cargo clippy -p quota-router-core --all-targets
      --features full -- -D warnings` clean.
- [x] 1650/1650 lib tests pass.

### Stage 2 / 3 — deferred to `M6-b-error-path-newtypes`

- [ ] `Secret<T>` newtype (Layer B substrate boundary) +
        per-field substitution on `ProviderConfig.client_secret`
        / `scim_token` / `idp_certificate`.
- [ ] `EntityId` / `Audience` / `Recipient` / `Email` newtypes
        across `SamlAssertion`, `ProviderConfig`, `IdpMetadata`,
        `TokenClaims`, `SsoUser`.
- [ ] `BlacklistQuery` port-trait refactor (move
        `Arc<dyn TokenBlacklistStorage>` behind a Layer B
        substrate trait).
- [ ] HTTP-boundary `Content-Length ≤ 64 KiB` enforcement
        on the SAML POST body at
        `crates/quota-router-core/src/admin.rs:573-590`.
- [ ] Carrier-error type widening (`SamlAudienceMismatch`
        could become a struct variant like
        `SamlAudienceMismatch { expected: Audience, got:
        Audience }` per M2 cross-ref).

### Deferral rationale

Stage 2/3 work is the largest of the M1-M8 cascade — touches
every module under `auth/sso/` (~12 files, ~200 LoC of
mechanical substitutions across construction sites,
accessors, type signatures, and serde shape). The
newtype work in particular requires either:

(a) Custom `serde_with` shim on every secret-field accessor
    (transparent-on-the-wire), OR

(b) A custom `Secret<T>` newtype with manual
    `Serialize`/`Deserialize` impls delegating to inner `T`.

Either path is correct and substantial. Landing it in a
dedicated follow-on mission (rather than bolting onto this
PR) keeps the diff reviewable and lets the follow-on be
tracked separately for completion.

The HTTP-boundary `Content-Length` check is small but
**must be coupled to the admin.rs SAML POST wiring** which
is currently out of scope in this crate's test surface
(the SAML callback production caller is wired elsewhere —
quota-router-cli / py-sdk territory).

### Diff summary

- `crates/quota-router-core/src/auth/sso/saml.rs`:
  - New `pub const MAX_SAML_XML_BYTES: usize = 64 * 1024`.
  - `parse()` early-rejects above the cap.
  - 13 mechanical replacements: every
    `attr.unescape_value().unwrap_or_default()` /
    `unescape(...).unwrap_or_default()` now propagates via
    `.map_err(...)?`.
  - 3 new tests (oversize-rejected, boundary, recipient
    mismatch no-attacker-data).

### Cross-references

- F2-005 (DoS cap) — Stage 1 closed ✓
- F2-006 (error propagation) — Stage 1 closed ✓
- F2-007 (log injection) — Stage 1 closed ✓
- F2-008 (newtypes) — Stage 2 → M6-b
- F2-011 (layer discipline: BlacklistQuery port) —
  Stage 3 → M6-b
- M2 (`SamlAudienceMismatch` struct variant) — Stage 2 → M6-b
- M4 (`Secret<T>` dependency) — landed at the parser
  side; config-side wrap is M6-b's first item
- admin.rs:573-590 (HTTP Content-Length) — Stage 2 → M6-b
  (admin-side, deployment-coupled)

## RFC

RFC-0949 (Economics): Enterprise SSO
§no-central-enums, §typed-discriminator, §storage-is-not-a-protocol
(CLAUDE.md §Architectural Principles)

## Dependencies

- `crates/quota-router-core/src/auth/sso/saml.rs`
- `crates/quota-router-core/src/auth/sso/mod.rs`

## Findings covered

- **F2-005:** No max-size cap on `assertion_xml`. Parser happily
  walks a 100MB assertion. DoS via deeply-nested DTD entity
  expansion (or just a giant `AttributeValue` blob).
- **F2-006:** 15+ sites use `attr.unescape_value().unwrap_or_default()`
  and `unescape(...).unwrap_or_default()` — failures silently become
  empty strings. SAML fail-closed requires these be hard errors.
- **F2-007:** Recipient error message `format!`s attacker-controlled
  `recip` into `SsoError::ProviderError`, which surfaces in logs
  and HTTP 502 JSON. Log-injection / XSS sink.
- **F2-008:** Per §no-central-enums / §typed-discriminator,
  identifier-typed fields should be newtypes:
  `EntityId`, `Audience`, `Recipient`, `Email`, `Secret<String>`.
  Currently bare `String` for all.
- **F2-011:** `SsoConfig` carries `Arc<dyn TokenBlacklistStorage>`
  (Layer-D-adjacent) and `ProviderConfig.client_secret` as plain
  `String`. Layer B should not see `Arc<dyn>` or bare `String`
  for secrets.

## Acceptance Criteria

- [ ] Add `const MAX_SAML_XML_BYTES: usize = 64 * 1024` (64 KiB
      — generous for a real assertion; bounded for DoS).
      `parse()` early-rejects above this size.
- [ ] At the admin HTTP boundary (`admin.rs:573-590`), enforce
      `Content-Length` ≤ 64 KiB on the SAML POST body before
      calling `handle_saml_callback`.
- [ ] Replace every `attr.unescape_value().unwrap_or_default()`
      with `.map_err(|e| SsoError::ProviderError(format!(
    "attribute unescape failed: {}", e)))?`.
      Replace every `unescape(...).unwrap_or_default()` similarly.
      15+ call sites; mechanical fix.
- [ ] Replace `format!("Recipient mismatch: expected {}, got {}",
    self.acs_url, recip)` with the length-only variant
      `format!("Recipient mismatch: expected len={}, got len={}",
    self.acs_url.len(), recip.len())`. Apply same sanitization
      to other format!s in saml.rs that include attacker data.
- [ ] Newtype identity-bearing fields at the Layer B substrate
      boundary:
      `rust
    pub struct EntityId(String);
    pub struct Audience(String);
    pub struct Recipient(String);
    pub struct Email(String);
    pub struct Secret<T: Zeroize + AsRef<[u8]>>(T);
    `
      Implement `CanonicalCodec` + `Display` + `FromStr` for
      each. Update `SamlAssertion`, `ProviderConfig`, `IdpMetadata`,
      `TokenClaims`, `SsoUser`.
- [ ] Move `Arc<dyn TokenBlacklistStorage>` behind a Layer B
      port trait `sso::port::BlacklistQuery` with
      `async fn is_blacklisted(&self, id: &str) -> Result<bool>`.
      Inject via constructor; production wiring uses a thin
      Stoolap-backed adapter; tests use an in-memory adapter.
- [ ] Tests: - `test_saml_xml_oversize_rejected` - `test_saml_attribute_unescape_error_propagates` - `test_saml_text_unescape_error_propagates` - `test_saml_recipient_mismatch_error_no_attacker_data` - `test_saml_newtype_entity_id_prevents_string_mixup` - `test_saml_blacklist_query_port_trait_wired`
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Claimant

(unclaimed)

## Notes

This is the LARGEST of the M1-M8 cascade by LoC — the newtype
work touches every module under `auth/sso/`. Land M4 (crypto
hygiene) FIRST so `Secret<T>` is available; then this mission
can lean on `Secret<String>` for `client_secret`.

**DoS cap rationale:** 64 KiB is large enough for any legitimate
SAML assertion (typical real-world assertion ~5-20 KiB) and
small enough to bound per-request memory to a few hundred bytes
of parser state. If real-world deployments report larger
assertions, lift to 256 KiB; do not go higher without revisiting.

**Layer port:** the `sso::port::BlacklistQuery` trait is the
canonical Layer-B substrate for the blacklist. Layer-C node
code wires the Stoolap adapter; production wiring is out of
scope for this mission (deployment concern).

**Cross-references:**

- F2-005 (DoS cap)
- F2-006 (error propagation)
- F2-007 (log injection)
- F2-008 (newtypes — biggest LoC)
- F2-011 (layer discipline)
- M2 (SamlAudienceMismatch struct variant — same PR or follow-on)
- M4 (`Secret<T>` dependency)
