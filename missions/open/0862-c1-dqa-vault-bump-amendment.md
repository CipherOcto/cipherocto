# Mission: 0862-c1 — RFC-0862 v2.0 (Dqa + vault bump) + 9 spend_ledger TV + 1 vault_id cross-ref TV (S6c)

## Status

**LANDED 2026-08-17 (@mmacedoeu, commit `2750caa7` + Round 1 fixes
follow-up).** S6c of the storage restructure plan per
`docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
§3 row 6 (Stream A.1 continuation; user-chosen S6 split-by-RFC
decision overrides §22 atomic-blocker bundle rule for this session).
Pre-reqs verified landed: S3 (octo-vault crate), S4 (Dqa codemod), S5
(verify-time invariant — LANDED 2026-08-17 in commit `d007de54`),
S6a (RFC-0870 + 1 TV LANDED 2026-08-17), S6b (RFC-0957 + 22 TV
LANDED 2026-08-17).

Round 1 multi-round adversarial review: 4 reviewers (spec/code/drift/security)
returned 12 HIGH + 11 MED + 8 LOW. All HIGH/MED resolved in
follow-up commit; follow-on missions filed for LOW (0862-c2..c6).

## RFC

- Primary: RFC-0862 v1.4.0 → v2.0 (Writer Election + Bootstrap
  Integration) — `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md`
  §Future Work F12 + F13 (promoted from deferred list).
- Co-RFC: RFC-0105 (Numeric) v1.9 → v2.0 (Dqa bump) — coordinate
  with S6g (RFC-0105 asset_id addendum + 109 TV). NOTE: RFC-0105
  v2.0 Dqa bump is bundled with RFC-0862 v2.0 per plan §3 S6 row 6
  "RFC-0862: 8 spend_ledger" descriptor; if RFC-0105 v2.0 is the
  actual destination for the Dqa portion, file follow-on amendment
  to RFC-0105.
- Source review: `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
  §14.1 + §20.6.1 (wire-format blast + verify-time invariant context).

## Summary

RFC-0862 v1.4.0 (LANDED 2026-08-11) added the concrete `WriterElection`
impl (`RaftLikeWriterElection`) + concrete `BootstrapOrchestrator`
impl. RFC-0862 v2.0 (this mission) extends the substrate to
`StoolapSpendLedger` per §Future Work F12/F13 + §Layer Direction
("`StoolapSpendLedger` + `StoolapDidRegistry`"). v2.0 amendment
text + 9 byte-exact spend_ledger TV fixtures (8 substrate + 1
cross-ref in octo-vault per Round 1 review).

§22 atomic-blocker rule applies to the 7 B0 amendments, but
user-chosen S6 split-by-RFC decision lands each amendment separately
(S6a RFC-0870 + S6b RFC-0957 + this S6c RFC-0862 + S6d RFC-0900 +
S6e RFC-0959 + S6f RFC-0960 + S6g RFC-0105). Production deployment
must coordinate the 7 sub-sessions' commits at push time (S8).

## Acceptance Criteria

1. **RFC-0862 §Version History v2.0 row added** documenting:
   - `StoolapSpendLedger` substrate integration (DrainCoordinator
     wire-up per §Atomicity guarantee)
   - Dqa storage form (stoolap `INTEGER` ↔ `i64` carrying
     `Dqa::value` at `scale = 0`; canonical on-wire form is
     16-byte BE `DqaEncoding` per RFC-0105 v1.9)
   - Vault substrate integration (per RFC-0965 §3.7 `Caveat::Vault`
     binding + vault_id BLAKE3 derivation per
     `crates/octo-vault/src/lib.rs` with prefix
     `"cipherocto/vault/v1/"`)
   - **Negative-cost precondition** (S4 Round 2 surface, hardened in
     S6c Round 1): `try_deduct` rejects `cost.value < 0` with
     `SpendLedgerError::NegativeCost`
   - Implementation mission: this file
     (`0862-c1-dqa-vault-bump-amendment.md`)
   - Pre-req: S3 (octo-vault) + S4 (Dqa codemod) + S5 (verify-time)
     all LANDED 2026-08-17
2. **RFC-0862 §SpendLedger Substrate subsection added** (new
   subsection under §Specification, after §DrainCoordinator):
   - `StoolapSpendLedger` impl surface (per
     `crates/quota-router-storage/src/stoolap_spend_ledger.rs`)
   - `spend_ledger` table schema: `(holder_did BLOB, macaroon_id
BLOB, balance INTEGER, updated_at_unix_ms INTEGER, PK(holder_did,
macaroon_id))`
   - `seed` / `balance` / `try_deduct` API per RFC-0862 §SpendLedger
   - Dqa storage form: `i64` column carrying `Dqa::value` at
     `scale = 0` (vs 16-byte BE `DqaEncoding` for on-wire)
   - Vault row cross-ref (per `octo-vault/src/lib.rs` vault_id
     derivation)
   - Cross-reference to RFC-0870 §NodeEnvelope Version Tag (V2
     wire-form per S6a amendment)
   - Negative-cost precondition (new in v2.0)
3. **TV-0862-01..08 + TV-0862-04b + TV-0862-09 byte-exact fixtures**
   in `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs`
   (NEW, 10 tests) + TV-0862 vault_id cross-ref in
   `crates/octo-vault/tests/tv_0862_vault_id_cross_ref.rs` (NEW, 3 tests):
   - 10 byte-exact TV per plan §3 S6 row 6 "RFC-0862: 8 spend_ledger"
     (extended to 9 substrate + 1 cross-ref per Round 1 review)
   - Pins: spend_ledger row creation (TV-01), balance read (TV-02),
     seed idempotency (TV-03), atomic drain (TV-04),
     UnknownHolder rejection (TV-04b), Dqa encoding round-trip +
     i64 schema column round-trip (TV-05), V2 wire-form on substrate
     side (TV-07), multi-instance drain coordination (TV-08),
     **negative-cost rejection** (TV-09 + TV-09b)
   - Cross-ref: vault_id derivation via production
     `octo_vault::vault_id` (NOT local BLAKE3 recomputation)
   - Test inputs byte-pinned (`TV_0862_*` constants); no RNG
4. Verification gate (per plan §4 S6):
   ```bash
   cargo test -p quota-router-storage --test tv_0862_spend_ledger  # 10/10 pass (9 spec + 1 regression 04b)
   cargo test -p octo-vault --test tv_0862_vault_id_cross_ref      # 3/3 pass
   cargo test --workspace --lib                                     # excludes 3 pre-existing S4 DFP Round 2 quota-router-cli::commands::tests::settle_* failures (commits 19faf380/4ab400bd/18edbe0d); unrelated to S6c
   cargo clippy --workspace --all-targets --features full -- -D warnings
   cargo fmt --all -- --check
   npx prettier --write missions/open/0862-c1-dqa-vault-bump-amendment.md
   ```
5. Memory card: `memory/mission-0862-c1-dqa-vault-bump-amendment-status.md`
   (this session's LANDED receipt + Round 1 fixes receipt + cross-link to S5 status card).

## Cross-reference

- **Pre-req:** `memory/mission-0957-g-verify-time-invariant-status.md`
  (S5 LANDED 2026-08-17, commit `d007de54`) — S5 verify-time
  invariant is load-bearing for spend_ledger vault-binding reject
  (per RFC-0862 §SpendLedger vault_id cross-ref).
- **Sibling missions:**
  - `missions/open/0870-c1-version-tag-amendment.md` (S6a LANDED)
  - `missions/open/0957-c1-verify-time-amendment.md` (S6b LANDED)
  - S6d RFC-0900 + 10 TV (pending)
  - S6e RFC-0959 + 25 TV (pending)
  - S6f RFC-0960 + 108 TV (pending)
  - S6g RFC-0105 + 109 TV (pending)
  - **Follow-ons (Round 1):**
    - `missions/open/0862-c2-clock-trait.md` (Clock trait refactor)
    - `missions/open/0862-c3-cross-process-drain.md` (cross-process drain + advisory file lock)
    - `missions/open/0862-c4-assert-to-error.md` (`dqa_to_i64` assert → error)
    - `missions/open/0862-c5-domain-sep.md` (domain-sep hygiene for untagged hash prefixes)
    - `missions/open/0862-c6-fixture-keyspace.md` (production keyspace fixture risk)
- **Status card:** `memory/mission-0862-c1-dqa-vault-bump-amendment-status.md`
  (LANDED).
- **Plan:** `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 row 6 (Stream A.1 S6c continuation).
- **Review source:** `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
  §14.1 (envelope.version_tag spec) + §20.6.1 (verify-time chain
  invariant) + §20.3 (vault substrate).

## Out of scope (deferred beyond S6c)

- S6d RFC-0900 amendment (10 TV) — next sub-session
- S6e RFC-0959 amendment (25 TV) — pending
- S6f RFC-0960 amendment (108 TV in 9×3×4 matrix per §24) — pending
- S6g RFC-0105 amendment (109 TV = 9 TV-D9 + 100 TV-D10) — pending
- S7 (B1/B2 amendments + 2 NEW RFCs + 56 TV) — pending
- S8 (24 mission closures + PR bundle staged) — pending
- §22 atomic-blocker PR bundle (user-chosen split-by-RFC overrides
  atomic-blocker rule for these sub-sessions; user may bundle at
  push time)
- Follow-ons c2..c6 (substrate hardening + fixture hygiene)

## Dependency edges (no changes)

| From                                                              | To                            | Why             | Layer direction     |
| ----------------------------------------------------------------- | ----------------------------- | --------------- | ------------------- |
| RFC-0862 amendment                                                | RFC-0105 §DqaEncoding         | Cross-reference | n/a (RFC text only) |
| RFC-0862 amendment                                                | RFC-0965 §3.7 `Caveat::Vault` | Cross-reference | n/a (RFC text only) |
| `crates/octo-vault/tests/tv_0862_vault_id_cross_ref.rs` (NEW)     | `octo-vault::vault_id`        | Test consumer   | test → lib          |
| `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs` (MOD) | `quota-router-storage`        | Test consumer   | test → lib          |

No new cyclic edges. No new crate deps.

## Critical files

- `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md`
  (modify — §Version History v2.0 row + §SpendLedger Substrate
  subsection; Round 1: drop phantom line refs + phantom RFC-0960
  §20.3, fix crate path, drop non-load-bearing version pins)
- `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs` (NEW
  — 10 TV fixtures; Round 1: remove MIGRATION_LOCK, TV-05 schema
  round-trip, TV-04b dedicated macaroon_id, TV-07 byte-array pin,
  magic literal constants, TV-09 negative-cost rejection)
- `crates/octo-vault/tests/tv_0862_vault_id_cross_ref.rs` (NEW per
  Round 1 — vault_id derivation cross-ref moved here from
  quota-router-storage)
- `crates/quota-router-storage/src/stoolap_spend_ledger.rs` (modify
  per Round 1 — `NegativeCost` error variant + negative-cost
  precondition guard)
- `memory/mission-0862-c1-dqa-vault-bump-amendment-status.md` (NEW
  — LANDED status card)

## Existing patterns reused

- RFC version history row format (RFC-0862 §Version History rows
  v1.0..v1.4.0) → new v2.0 row mirrors same shape.
- RFC subsection format (RFC-0862 §DrainCoordinator v1.4.0) → new
  §SpendLedger Substrate v2.0 subsection mirrors the pattern.
- `crates/quota-router-storage/tests/tv_*` byte-exact TV layout
  → new `tv_0862_spend_ledger.rs` mirrors the test scaffolding.
- `crates/octo-vault/tests/tv_*` byte-exact TV layout → new
  `tv_0862_vault_id_cross_ref.rs` mirrors the test scaffolding.
- S6a + S6b mission YAML structure (this file follows the pattern).

## Risks

- **B.3 verify-time invariant load-bearing** (HIGH per plan §5):
  S5 implementation already passed gate; S6c amendment text + 10 TV
  is documentation-only + 10 fixtures + 1 substrate hardening
  (NegativeCost guard). Low blast if anything regresses.
- **§22 atomic-blocker rule bypass** (MED per plan §5): user-chosen
  S6 split-by-RFC decision lands each amendment separately, NOT in
  the prescribed single PR bundle. Production deployment must
  coordinate the 7 sub-sessions' commits at push time (per S8).
- **Plan label ambiguity** (LOW): plan §3 S6 row 6 describes
  "RFC-0862 (Dqa + vault bump)" but neither existing
  `rfcs/accepted/networking/0862-*` RFC explicitly owns Dqa + vault
  territory. Resolution: RFC-0862 v1.4.0 already references
  "RFC-0862 v2.0 future amendment" at §Future Work F12/F13; v2.0
  amendment is the correct destination for the spend_ledger + Dqa +
  vault integration. RFC-0105 v1.9 → v2.0 Dqa bump is the related
  S6g amendment; cross-reference between the two is required.
- **Dqa wire-format coordination** (MED): S4 DFP codemod already
  landed Dqa wire-form (16-byte BE DqaEncoding); RFC-0862 v2.0 must
  cite the actual wire-form. Verified against
  `determin/src/dqa.rs` (NOT `crates/octo-determin/`) before drafting
  amendment text — fixed in Round 1.

## Version history

| Date       | Author     | Change                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| ---------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-08-17 | @mmacedoeu | Initial claim as S6c (third S6 sub-session per user split-by-RFC decision). RFC-0862 v2.0 amendment back-fills S5 + S6a + S6b substrate integration. 8 TV fixtures pin spend_ledger + Dqa + vault wire form.                                                                                                                                                                                                                                         |
| 2026-08-17 | @mmacedoeu | LANDED via commit `2750caa7` (RFC amendment + 9 TV in quota-router-storage). Round 1 review fixes applied: drop phantom line refs (1801/1629/1730), drop phantom RFC-0960 §20.3, fix crate path `octo-determin/src/dqa.rs:104` → `determin/src/dqa.rs:510`, drop non-load-bearing version pins, add `NegativeCost` precondition guard + TV-09, move TV-06 to `crates/octo-vault/tests/`, remove MIGRATION_LOCK. 5 follow-on missions filed (c2..c6). |
