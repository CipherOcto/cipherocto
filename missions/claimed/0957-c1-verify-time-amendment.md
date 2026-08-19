# Mission: 0957-c1 — RFC-0957 verify-time amendment (S6b)

## Status

**LANDED 2026-08-17 (claimant @mmacedoeu).** S6b second sub-
session per
`docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
§3 row 6 (Stream C.2 continuation; user-chosen S6 split-by-RFC
decision overrides §22 atomic-blocker bundle rule for this session).
Pre-reqs verified landed: S3 (octo-vault crate), S4 (DFP codemod),
S5 (verify-time invariant — LANDED 2026-08-17 commit `d007de54`),
S6a (RFC-0870 NodeEnvelope version_tag — LANDED 2026-08-17
commits `c7f99a47` + `ab2b57b4` + `4f3f3af4`).

## RFC

- Primary: RFC-0957 (Capability Token Format) — §Algorithms
  `verify` amendment + §Data Structures `Caveat` DSL amendment +
  §Version History v2.1 row.
- Co-RFC: RFC-0965 (Capability Extension Format) §3 — 9 new
  Caveat variants (Vault, Permission, ValidRange, MaxPerTx,
  AuditWindow, MaxUses, WrappedOnly, Factory, PolicyReference) +
  PermissionKind enum. (Four pre-existing variants
  `ValidAfter`, `RedemptionContext`, `Sharded`, `Payment` are
  unchanged by this amendment.) These variants land in the
  per-extension crate `crates/octo-cap-macaroon/` per RFC-0957
  v2.0 amendment + mission `0965-a-caveat-dsl` (LANDED).
- Co-RFC: RFC-0870 (Networking) `§NodeEnvelope Version Tag` —
  replay-defense invariant via `envelope_id` (S6a LANDED). The
  verify-time chain re-derivation algorithm in RFC-0957
  §Algorithms now references the `version_tag` byte as part of
  the wire-form discriminator.

## Summary

RFC-0957 §Algorithms `verify` (accepted 2026-07-20) describes HMAC
chain re-derivation + caveat predicate evaluation + discharge
resolution. S5 (LANDED 2026-08-17) extended the verify-time path
with `Macaroon::verify_for_vault_op` (per the review doc §20.6.1 —
4-step algorithm: signature verify → wrapped-chain integrity →
per-vault lookup loop (exist + chain match + state=Active) →
optional attenuation subsumption vs `expected_parent`). S5.1
follow-on (`0957-g1-octo-vault-lookup-glue`) implements the substrate
adapter `OctoVaultLookup`. S6a (LANDED 2026-08-17) added the
`version_tag` wire-form discriminator.

This mission (S6b) back-fills the RFC-0957 amendment text + delivers
22 byte-exact TV fixtures pinning the verify-time path + Caveat DSL
extension variants. The amendment is ADDITIVE to RFC-0957 v2.0
(Per-Extension Crate Layout); it does not modify the existing
macaroon v1 chain construction or attenuation invariant.

> **Note:** §Verify-Time Extension describes a 4-step algorithm
> (signature verify → wrapped-chain integrity → per-vault lookup
> loop → optional attenuation subsumption) — earlier 5-step
> descriptions merged the substrate lookup into a single step; the
> canonical form is 4 distinct steps.

## Acceptance Criteria

1. **RFC-0957 §Version History v2.1 row added** documenting:
   - `verify_for_vault_op` extension (per review doc §20.6.1
     4-step algorithm) — Vault caveat verify-time path
   - 9 new Caveat variants: Vault + Permission + WrappedOnly +
     Factory + AuditWindow map to RFC-0965 §3.x; MaxUses maps
     to §3.4; PolicyReference maps to §3.10; ValidRange +
     MaxPerTx are NEW in RFC-0957 v2.1 (no RFC-0965 §3.x number)
     — landed in `crates/octo-cap-macaroon/src/caveat/`
   - `PermissionKind` enum (5 variants per RFC-0965 §3.2)
   - `WrappedOnly` parent-no-Vault-binding reject — ONLY in
     `verify_for_vault_op` (operational gate), NOT in
     `verify_full` (structural path)
   - Implementation mission: this file
     (`0957-c1-verify-time-amendment.md`)
   - Pre-req: S5 LANDED 2026-08-17 commit `d007de54`; S5.1
     deferred to `0957-g1-octo-vault-lookup-glue`
2. **RFC-0957 §Verify-Time Extension subsection added** (new
   subsection under §Algorithms, after §Macaroon v1 chain
   construction):
   - `Macaroon::verify_for_vault_op` signature
   - 4-step algorithm verbatim per review doc §20.6.1
   - `VaultLookup` trait injection (Layer B extension consumer)
   - `WrappedOnly` chain walk invariant (operational gate only;
     `verify_full` explicitly NOT required to enforce chainless-
     parent reject)
   - Cross-reference to RFC-0965 §3 + RFC-0870 §NodeEnvelope
     Version Tag
3. **RFC-0957 §Caveat DSL Extension subsection added** (new
   subsection under §Data Structures, after the `Caveat` enum
   definition):
   - 9 new variants enumerated with field types
   - `PermissionKind` enum (5 variants)
   - `FactoryVet` struct per RFC-0965 §3
   - Cross-reference to per-extension crate
     `crates/octo-cap-macaroon/src/caveat/`
4. **22 byte-exact TV fixtures** in
   `crates/octo-cap-macaroon/tests/tv_0957_verify_time.rs` (NEW):
   - **TV-0957-01..05** — 5 Caveat DSL variant wire-form pins
     (Vault, Permission, ValidRange, MaxPerTx, AuditWindow)
   - **TV-0957-06..10** — 5 Caveat DSL variant wire-form pins
     (MaxUses, WrappedOnly, Factory, PolicyReference, Raw
     unknown-name rejection)
   - **TV-0957-11..15** — 5 verify-time path pins
     (signature verify step, vault row lookup step, chain match
     step, state=Active step, WrappedOnly chain walk step)
   - **TV-0957-16..20** — 5 regression tests
     (frozen vault `is_active=false`, chain mismatch, missing
     root secret, WrappedChainHasNoVault, attenuation monotonicity
     invariant with the new variants + AttenuationViolation
     negative case via too-early ValidRange tightening)
   - **TV-0957-21** — multi-level WrappedOnly chain (depth=3)
   - **TV-0957-22** — MAX_WRAPPED_DEPTH=16 boundary (depth 17
     reject per RFC-0965 §3.7 R7-F1)
5. Verification gate:
   ```bash
   cargo test -p octo-cap-macaroon --test tv_0957_verify_time    # 22/22 pass
   cargo test --workspace --lib                                  # no regressions (excluding 3 pre-existing S4 DFP Round 2 quota-router-cli::commands::tests::settle_* failures)
   cargo clippy -p octo-cap-macaroon --all-targets -- -D warnings
   cargo fmt --all -- --check
   npx prettier --write missions/open/0957-c1-verify-time-amendment.md
   ```
6. Memory card cross-link: S5 status card
   (`memory/mission-0957-g-verify-time-invariant-status.md`)
   cross-linked from this mission's `## Cross-reference` section;
   new S6b status card at
   `memory/mission-0957-c1-verify-time-amendment-status.md`.

## Out of scope (deferred beyond S6b)

- S6c RFC-0862 amendment (8 TV) — next sub-session
- S6d RFC-0900 amendment (10 TV)
- S6e RFC-0105 amendment (109 TV)
- S6f RFC-0959 amendment (25 TV)
- S6g RFC-0960 amendment (108 TV)
- §22 atomic-blocker PR bundle (user-chosen split-by-RFC
  overrides atomic-blocker rule; user may bundle at push time)
- S5.1 `OctoVaultLookup` glue crate (separate mission
  `0957-g1-octo-vault-lookup-glue`)
- Production deployment wiring (config-time injection of
  `Arc<OctoVaultLookup>` into the verify path)

## Dependency edges (no changes)

| From                                                | To                                 | Why                                 | Layer direction        |
| --------------------------------------------------- | ---------------------------------- | ----------------------------------- | ---------------------- |
| RFC-0957 amendment text                             | RFC-0965 §3 + §3.2 + §3.7          | Cross-reference                     | n/a (RFC text only)    |
| RFC-0957 amendment text                             | RFC-0870 §NodeEnvelope Version Tag | Cross-reference                     | n/a (RFC text only)    |
| `crates/octo-cap-macaroon/tests/tv_0957_*.rs` (NEW) | `octo-cap-macaroon`                | Test consumer                       | test → lib             |
| `octo-cap-macaroon`                                 | (none new)                         | `VaultLookup` trait uses primitives | Layer B → Layer A only |

No new cyclic edges. No new crate deps.

## Critical files

- `rfcs/accepted/economics/0957-capability-token-format.md`
  (modify — §Version History v2.1 row + §Verify-Time Extension
  subsection + §Caveat DSL Extension subsection)
- `crates/octo-cap-macaroon/tests/tv_0957_verify_time.rs` (NEW —
  22 TV fixtures)
- `memory/mission-0957-g-verify-time-invariant-status.md`
  (existing — add cross-reference backlink)
- `memory/mission-0957-c1-verify-time-amendment-status.md` (NEW)
- `missions/open/0957-c1-verify-time-amendment.md` (this file)

## Existing patterns reused

- RFC version history row format (RFC-0957 §Version History rows
  v0.1..v2.0) → new v2.1 row mirrors same shape.
- RFC subsection format (RFC-0957 §Algorithms subsections) → new
  §Verify-Time Extension v2.1 subsection mirrors the pattern.
- `tv_c1_verify_time.rs` byte-exact TV layout → new
  `tv_0957_verify_time.rs` extends with 22 fixtures across 4
  categories (5+5 wire-form + 5 verify-time + 5 regression +
  2 deep-chain/boundary).

## Risks

- **B.3 verify-time invariant load-bearing** (HIGH per plan §5):
  S5 implementation already passed gate; S6b amendment text + 22
  TV is documentation-only + 22 fixtures. Low blast if anything
  regresses.
- **§22 atomic-blocker rule bypass** (MED per plan §5): user-chosen
  S6 split-by-RFC decision lands each amendment separately, NOT in
  the prescribed single PR bundle. Production deployment must
  coordinate the 7 sub-sessions' commits at push time (per S8).
- **Caveat DSL variant count creep** (MED): 9 new variants per
  RFC-0965 §3 + 5 PermissionKind enum = 14 new types. Each must be
  covered by at least one TV or its wire-form is unpinned.
- **WrappedOnly chain walk complexity** (LOW): the S5
  `Macaroon::verify_for_vault_op` already implements the 4-step
  algorithm + `WrappedChainHasNoVault` reject; S6b adds 5 TV
  fixtures pinning each step's error path.

## Cross-reference

- Plan: `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 row 6
- Pre-req S5 status card (cross-linked):
  `memory/mission-0957-g-verify-time-invariant-status.md`
- Pre-req S6a status card:
  `memory/mission-0870-c1-version-tag-amendment-status.md`
- Pre-req Caveat DSL extension source code:
  `memory/mission-0965-a-caveat-dsl-status.md` (mission
  `0965-a-caveat-dsl`)
- S6b status card (this landing):
  `memory/mission-0957-c1-verify-time-amendment-status.md`
- RFC amendment: `rfcs/accepted/economics/0957-capability-token-format.md`
  §Version History v2.1 row + §Verify-Time Extension +
  §Caveat DSL Extension
- Review source: `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
  (review doc §20.6.1 4-step algorithm)

## Version history

| Date       | Author     | Change                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| ---------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-08-17 | @mmacedoeu | Initial proposal as S6b (second S6 sub-session per user split-by-RFC decision). RFC-0957 amendment back-fills S5 implementation + RFC-0965 §3 Caveat DSL extension. 22 TV fixtures pin verify-time path + Caveat variants.                                                                                                                                                                                                                                |
| 2026-08-17 | @mmacedoeu | LANDED. RFC-0957 v2.1 row + §Verify-Time Extension + §Caveat DSL Extension subsections added. TV-0957 22/22 pass. Memory card cross-link added to S5 status card. Status flipped PROPOSED → LANDED after verify gate. Round 2 follow-on (`4ec9779f`) re-scoped TV-16/17 to ancestor coverage + dropped dead code; Round 3+4 follow-on reconciled §3.x numbering drift (caveat/mod.rs + RFC + pseudocode) + cleared source phantom §20.6.1 line 1328 refs. |
