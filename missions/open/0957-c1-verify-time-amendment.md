# Mission: 0957-c1 — RFC-0957 verify-time amendment (S6b)

## Status

**PROPOSED (2026-08-17, claimant @mmacedoeu).** S6b second sub-
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
  PermissionKind enum. These variants land in the per-extension
  crate `crates/octo-cap-macaroon/` per RFC-0957 v2.0 amendment +
  mission `0965-a-caveat-dsl` (LANDED).
- Co-RFC: RFC-0870 (Networking) §14.1 — replay-defense invariant
  via `envelope_id` (S6a LANDED). The verify-time chain
  re-derivation algorithm in RFC-0957 §Algorithms now references
  the `version_tag` byte as part of the wire-form discriminator.

## Summary

RFC-0957 §Algorithms `verify` (accepted 2026-07-20) describes HMAC
chain re-derivation + caveat predicate evaluation + discharge
resolution. S5 (LANDED 2026-08-17) extended the verify-time path
with `Macaroon::verify_for_vault_op` (RFC-0957 §20.6.1 — 5-step
algorithm: signature verify → vault row lookup → chain match →
state=Active → WrappedOnly chain walk). S5.1 follow-on
(`0957-g1-octo-vault-lookup-glue`) implements the substrate
adapter `OctoVaultLookup`. S6a (LANDED 2026-08-17) added the
`version_tag` wire-form discriminator.

This mission (S6b) back-fills the RFC-0957 amendment text + delivers
20 byte-exact TV fixtures pinning the verify-time path + Caveat DSL
extension variants. The amendment is ADDITIVE to RFC-0957 v2.0
(Per-Extension Crate Layout); it does not modify the existing
macaroon v1 chain construction or attenuation invariant.

## Acceptance Criteria

1. **RFC-0957 §Version History v2.1 row added** documenting:
   - `verify_for_vault_op` extension (RFC-0957 §20.6.1 5-step
     algorithm) — Vault caveat verify-time path
   - 9 new Caveat variants per RFC-0965 §3 (Vault, Permission,
     ValidRange, MaxPerTx, AuditWindow, MaxUses, WrappedOnly,
     Factory, PolicyReference) — landed in
     `crates/octo-cap-macaroon/src/caveat/`
   - `PermissionKind` enum (5 variants per RFC-0965 §3.5)
   - `WrappedOnly` parent-no-Vault-binding reject per §20.6.1
     line 1328
   - Implementation mission: this file
     (`0957-c1-verify-time-amendment.md`)
   - Pre-req: S5 LANDED 2026-08-17 commit `d007de54`; S5.1
     deferred to `0957-g1-octo-vault-lookup-glue`
2. **RFC-0957 §Verify-Time Extension subsection added** (new
   subsection under §Algorithms, after §Macaroon v1 chain
   construction):
   - `Macaroon::verify_for_vault_op` signature
   - 5-step algorithm verbatim per §20.6.1
   - `VaultLookup` trait injection (Layer B extension consumer)
   - `WrappedOnly` chain walk invariant
   - Cross-reference to RFC-0965 §3 + RFC-0870 §14.1
3. **RFC-0957 §Caveat DSL Extension subsection added** (new
   subsection under §Data Structures, after the `Caveat` enum
   definition):
   - 9 new variants enumerated with field types
   - `PermissionKind` enum (5 variants)
   - `FactoryVet` struct per RFC-0965 §3
   - Cross-reference to per-extension crate
     `crates/octo-cap-macaroon/src/caveat/`
4. **20 byte-exact TV fixtures** in
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
     invariant with the new variants)
5. Verification gate:
   ```bash
   cargo test -p octo-cap-macaroon --test tv_0957_verify_time    # 20/20 pass
   cargo test --workspace --lib                                  # no regressions (excluding 3 pre-existing S4 DFP Round 2 quota-router-cli::commands::tests::settle_* failures)
   cargo clippy --workspace --all-targets --features full -- -D warnings
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

| From                                                | To                        | Why                                 | Layer direction        |
| --------------------------------------------------- | ------------------------- | ----------------------------------- | ---------------------- |
| RFC-0957 amendment text                             | RFC-0965 §3 + §3.5 + §3.7 | Cross-reference                     | n/a (RFC text only)    |
| RFC-0957 amendment text                             | RFC-0870 §14.1            | Cross-reference                     | n/a (RFC text only)    |
| `crates/octo-cap-macaroon/tests/tv_0957_*.rs` (NEW) | `octo-cap-macaroon`       | Test consumer                       | test → lib             |
| `octo-cap-macaroon`                                 | (none new)                | `VaultLookup` trait uses primitives | Layer B → Layer A only |

No new cyclic edges. No new crate deps.

## Critical files

- `rfcs/accepted/economics/0957-capability-token-format.md`
  (modify — §Version History v2.1 row + §Verify-Time Extension
  subsection + §Caveat DSL Extension subsection)
- `crates/octo-cap-macaroon/tests/tv_0957_verify_time.rs` (NEW —
  20 TV fixtures)
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
  `tv_0957_verify_time.rs` extends with 20 fixtures across 4
  categories.

## Risks

- **B.3 verify-time invariant load-bearing** (HIGH per plan §5):
  S5 implementation already passed gate; S6b amendment text + 20
  TV is documentation-only + 20 fixtures. Low blast if anything
  regresses.
- **§22 atomic-blocker rule bypass** (MED per plan §5): user-chosen
  S6 split-by-RFC decision lands each amendment separately, NOT in
  the prescribed single PR bundle. Production deployment must
  coordinate the 7 sub-sessions' commits at push time (per S8).
- **Caveat DSL variant count creep** (MED): 9 new variants per
  RFC-0965 §3 + 5 PermissionKind enum = 14 new types. Each must be
  covered by at least one TV or its wire-form is unpinned.
- **WrappedOnly chain walk complexity** (LOW): the S5
  `Macaroon::verify_for_vault_op` already implements the 5-step
  algorithm + `WrappedChainHasNoVault` reject; S6b adds 5 TV
  fixtures pinning each step's error path.

## Version history

| Date       | Author     | Change                                                                                                                                                                                                                     |
| ---------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-08-17 | @mmacedoeu | Initial proposal as S6b (second S6 sub-session per user split-by-RFC decision). RFC-0957 amendment back-fills S5 implementation + RFC-0965 §3 Caveat DSL extension. 20 TV fixtures pin verify-time path + Caveat variants. |
