# Mission: 0968 Phase 1 — Symbol Alignment (canonical AC text ↔ substrate)

## Status

OPEN (created 2026-08-07). Sub-mission of `missions/claimed/0968-reputation-persistence.md`. Closes the grand-design audit surfacing 29 unchecked Phase 1 ACs that cite canonical pre-RFC-0968-A1 type names not present in the current substrate.

**Open since:** 2026-08-07
**Blocker:** None (no upstream dependency; substrate is stable at 212/212 lib tests).
**Strategy:** Decide per-symbol whether to add the canonical name to the substrate OR rewrite the AC body to use the actual substrate name. Prefer the lower-cost path per symbol.
**Expected outcome:** 29 ACs either flipped [x] or removed (renamed to substrate-symbol names) per [[deferred-vs-unspecified]] named-owner rule.

## Context

The Phase 1 AC table in `missions/claimed/0968-reputation-persistence.md` (lines 37-117) cites 15+ canonical pre-RFC-0968-A1 type names that are NOT defined as `pub struct` in the current substrate. The audit's §Phase 1 AC Reconciliation section (added 2026-08-07) initially marked them SUBSTRATE-PRESENT based on file-level existence; deeper symbol-level audit (also 2026-08-07) revealed the drift.

Substrate has equivalents — e.g., `auth::AttestorRegistration` (struct) instead of a canonical `RecorderRegistration`; `recorder::StakeCheck` (enum) instead of AC-4's `recorder_state_at` returning 7-state enum. The substrate evolved away from the canonical names during RFC-0968-A1 amendments without rewriting the parent mission's AC body. Per [[no-phantom-mission-pointers]] we cannot silently flip ACs whose cited symbols don't exist.

## Missing symbols (15 AC-1..AC-29 canonical type names)

| AC | Canonical name (in AC body) | Substrate equivalent | Resolution path |
|---|---|---|---|
| AC-1 | `RecorderRegistration` | `auth::ChainRef` (registration input chain ref) | rename AC cite to `ChainRef` |
| AC-1 | `RecorderRegistrationRequest` | `auth::ChainRef` (no explicit request type) | rename AC cite to `ChainRef` |
| AC-1 | `ReplayRecord` | `audit::AuditReplay` (replay state) | rename AC cite to `AuditReplay` |
| AC-1 | `RotationReceipt` | `retirement::RetirementEligibility` (rotation outcome) | rename AC cite to `RetirementEligibility` |
| AC-1 | `AggregateCheckpoint` | `audit::AuditReplay::drop_pre_rotation_events` (no separate checkpoint type) | rewrite AC-22 to cite `AuditReplay` |
| AC-1 | `ResumeProof` | none (no analogous substrate) | add `ResumeProof` struct (small) OR rewrite AC body to use existing `GovernanceProof` |
| AC-1 | `GovernanceRegistry` | `auth::GovernanceSnapshot` + `government::verify_governance_suspension` (registry is implicit) | rename AC cite to `GovernanceSnapshot` |
| AC-1 | `GovernanceError` | `error::ReputationError` (enum covers governance failures) | rename AC cite to `ReputationError` |
| AC-1 | `PublicKey` | identity layer (`octo-ident`); not in `octo-reputation` | rewrite AC cite to `octo_ident::PublicKey` |
| AC-1 | `ReputationPolicy` | none (no policy type) | add `ReputationPolicy` struct (small) OR remove AC reference |
| AC-2 | `RecorderRegistration` | see AC-1 | rename AC cite |
| AC-2 | `RecorderRegistration` fields (`octo_stake_amount`, `role_stake_amount`, `aggregate_stake_amount`, `stake_lock_ref`) | `auth::ChainRef` carries `octo_stake_amount` + `role_stake_amount`; `stake_lock_ref` is `ChainRef` itself | rename + restructure |
| AC-3 | `RecorderId::registered` | `auth::ChainRef::verify_registration` is the canonical minting path | rename AC cite |
| AC-4 | `recorder_state_at` returning `Active | Suspended | Revoked | UnderStaked | Stale | Expired | Unknown` | `recorder::StakeCheck` enum (different shape) | rewrite AC body to cite `StakeCheck` + add mapping table |
| AC-13 | `Did::rotate` | `auth::` rotation primitives elsewhere | rename AC cite |
| AC-14 | `Did::parse` | `types::RecorderDid` (constructor with validation) | rename AC cite to `RecorderDid::from_array` |
| AC-19 | `ReaderAuth` | none (auth is per-method, not per-role) | add `ReaderAuth` struct (small) OR rewrite AC body to be permission-based |
| AC-19 | `AuditorAuth` | `auth::AttestorAuth` (closest analog) | rename AC cite to `AttestorAuth` |
| AC-19 | `RetentionAuth` | none (retention is internal-only, no separate auth) | add `RetentionAuth` struct (small) OR rewrite AC body to cite `Auth::AttestorAuth` with role flag |
| AC-19 | `RETENTION_ROLE` bit | none | add `RETENTION_ROLE` constant (small) |
| AC-19 | `BLAKE3_REPUTATION_RETENTION_DOMAIN` | none | add constant (small) |
| AC-24 | `octo-determin = { path = "../../determin" }` | confirmed present in `Cargo.toml` | no change — AC text is accurate |
| AC-25 | `BUILTIN_MIGRATIONS` v003-v009 | actual migrations v001-v005 + v010-v012 | rewrite AC cite + file rename per §Path Reconciliation migration table |
| AC-26 | `GossipCatchUp` | `gossip::GossipCatchUp` (verified) | rename AC cite to `gossip::GossipCatchUp` |
| AC-27 | `FederatedSuspensionCertificate` | `auth::AnchorGovernanceSnapshot` + `auth::AnchorGovernanceProof` (closest analog) | rename AC cite to `AnchorGovernanceProof` |
| AC-15 | `consume_rotation_receipt` | `retirement::declare_on` (canonical retirement method) | rename AC cite |
| AC-22 | `aggregate_checkpoint.checkpoint_id` BLAKE3 derivation | `audit::drop_pre_rotation_events` (replay checkpoint logic) | rewrite AC body to cite `audit` module constants |
| AC-23 | pointer+recompute model | `audit::` checkpoint primitives | rewrite AC body |

## Resolution strategies

**Path A: Add canonical names to substrate** (preferred when substrate extension is small):
- Add `pub struct ResumeProof`: 5-line struct in `auth.rs` (mirrors `GovernanceProof` shape)
- Add `pub struct ReputationPolicy`: 8-line struct in `types.rs`
- Add `pub struct ReaderAuth`: 4-line struct in `auth.rs` (single field: `reader_id: RecorderId`)
- Add `pub struct RetentionAuth`: 4-line struct in `auth.rs` (single field: `auth: AttestorAuth` + role bit)
- Add `pub const RETENTION_ROLE: u8 = 0x08;` in `constants.rs`
- Add `pub const BLAKE3_REPUTATION_RETENTION_DOMAIN: &[u8] = b"cipherocto/reputation/retention/v1";` in `constants.rs`

**Path B: Rewrite AC body to use substrate names** (preferred when substrate covers the function):
- AC-1, AC-2, AC-3, AC-15, AC-22, AC-23, AC-25, AC-26, AC-27: rewrite AC body text
- AC-4: rewrite the 7-state enum cite to `StakeCheck` + add a mapping table

**Path C: Defer AC entirely** (used when symbol neither exists nor has clear substrate equivalent):
- AC-13 + AC-14: `Did::rotate` and `Did::parse` have substrate equivalents (`auth::` rotation methods, `RecorderDid::from_array` constructor) but the canonical form is non-trivial. Choose Path B.

## Acceptance Criteria

- [ ] All 15 missing symbols resolved per column 4 of the table above (Path A or B chosen per row)
- [ ] 29 ACs (`AC-1` through `AC-29`) in `0968-reputation-persistence.md` either flipped [x] (per [[cargo-fmt-workflow]] + verified green) or rewritten to cite substrate names
- [ ] `cargo test -p octo-reputation --features stoolap --lib` still passes (212+ tests)
- [ ] `cargo clippy -p octo-reputation --all-targets --all-features -- -D warnings` clean
- [ ] `cargo fmt -p octo-reputation -- --check` clean
- [ ] §Phase 1 AC Reconciliation (2026-08-07) in parent mission updated to reflect per-AC resolution

## Dependencies

**Requires (mission gates):**
- `missions/claimed/0968-reputation-persistence.md` — parent mission (in progress, AC-30 flipped 2026-08-07)
- RFC-0968-A1 (Accepted) — canonical symbol names documented

**Blocks (downstream missions):**
- 0968 Phase 2-3 AC flips (already closed in `bf9ef1d7` + `f99e53c3`)
- 0968 Phase 4 (Federation, gated on 0855p-b archived)

## Location

- `crates/octo-reputation/src/auth.rs` (MODIFY) — add `ResumeProof`, `ReaderAuth`, `RetentionAuth` structs (Path A options)
- `crates/octo-reputation/src/types.rs` (MODIFY) — add `ReputationPolicy` struct (Path A option)
- `crates/octo-reputation/src/constants.rs` (MODIFY) — add `RETENTION_ROLE` + `BLAKE3_REPUTATION_RETENTION_DOMAIN` (Path A options)
- `missions/claimed/0968-reputation-persistence.md` (MODIFY) — rewrite AC body text for Path B resolutions + flip [x] for verified ACs

## Claimant

@mmacedoeu (created 2026-08-07)

## Notes

- This is a documentation-heavy sub-mission. Code adds (Path A) are bounded — at most 5 small structs/constants (~30 lines total).
- The bulk of the work is AC body rewrites (Path B), which is mechanical but requires careful reading of each AC to ensure the rewrite preserves the original spec-intent.
- The mission can be split into 2 sessions: (1) Path A code adds + clippy/test verification; (2) Path B AC rewrites + parent mission update.
- AC-13 + AC-14 require rotation + DID construction primitives that may not be in `octo-reputation` substrate at all. If neither `auth::` nor `types::` has the canonical form, mission may need to scope-defer to a future `0968-p1-rotation-did` sub-mission.

## Version History

| Version | Date       | Change |
| ------- | ---------- | ------ |
| v0.1    | 2026-08-07 | Mission created. 15 missing symbols + 29 affected ACs catalogued. Per [[deferred-vs-unspecified]] named-owner rule, scope = align AC text with substrate (Path B) OR add canonical names (Path A). |
