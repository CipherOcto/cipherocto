# 0016-a-audit-write-path-promotion — RFC-0016-a write-path amendment promotion

**Status:** Open
**Substrate:** RFC-0016 (audit read-path) + RFC-0016-a (audit write-path)
**Parent:** RFC-0016 (Accepted 2026-09-14) + RFC-0015-a (Accepted 2026-09-14)
**Depends on:** RFC-0016 Accepted; RFC-0015-a Accepted

## Scope

DRY review loop + promote RFC-0016-a (write-path amendment) from
Draft → Accepted. RFC-0016-a was **not** part of the R12 + R13
plateau declaration (only RFC-0015 + RFC-0015-a + RFC-0016 were in
scope for the plateau). RFC-0016-a remains in Draft status pending
its own DRY closure.

Per [[memory-is-never-status-ground-truth]]: status language in this
file describes CURRENT state at draft time. The DRY loop + promotion
are the implementation work; the `status:` field reflects that the
mission is Open, not the RFC's Draft status.

## Why this exists

RFC-0016-a (audit write-path) is the missing paired amendment to
RFC-0016 (audit read-path). RFC-0016 was promoted Accepted as part
of the 2026-09-14 plateau trio (RFC-0015 + RFC-0015-a + RFC-0016).
RFC-0016-a was deferred because the plateau declaration surfaced 7
substrate defects that affect RFC-0015-a + RFC-0015 but **also affect
RFC-0016-a** (audit write-path inherits substrate defects from the
same `octo-wallet::transition_agent` + `octo-audit::append_agent_transition_event`
chain).

The RFC-0016-a promotion is sequenced AFTER the RFC-0015-b amendment

- paired implementation missions land, so that RFC-0016-a inherits
  the corrected substrate (rather than landing against the uncorrected
  substrate + requiring a re-amendment cycle).

## Hard sequencing

Per [[no-phantom-mission-pointer]] + RFC-0015-a §6.4 paired-invariance:

1. RFC-0015-b amendment mission lands FIRST (Draft → Accepted)
2. `0015-b-substrate-defect-impl` mission lands SECOND
3. **THIS mission** lands THIRD (RFC-0016-a DRY loop + promotion)
4. Subsequent substrate missions (audit-commands Phase 3 CLI wiring
   per `missions/claimed/0011-a-audit-commands.md`) land LAST

This ordering ensures RFC-0016-a references the corrected substrate
(`AlreadyInTransition` activated, TOCTOU closed, phantom-event
detection in place).

## DRY review pattern

R1 spawn 5-len reviewers on RFC-0016-a. Aggregate → R1.5 fix → R2
(DRY verification round 1) → R2.5 fix → R3 (DRY verification round
2 = DRY CLOSED) → promotion (Draft → Accepted via `git mv`) → closure
artifacts (audit doc + memory card).

5 reviewers per round:

1. correctness (does RFC-0016-a cover the audit write-path surface?)
2. layer-model (Layer B substrate additions; no CLI changes; no reverse deps)
3. simplification (canonical surfaces; no parallel abstractions)
4. hygiene (file:line vs §symbol refs; prettier compliance)
5. substrate-faithfulness (CRITICAL: does RFC-0016-a match the corrected
   substrate from RFC-0015-b? Reference defects 1-5 from plateau
   declaration to verify each amendment §x matches the operative fix)

## Promotion pattern (paired with RFC-0015 + RFC-0015-a + RFC-0016)

Per parent trio promotion precedent (2026-09-14 commit `bac77cfd`):

1. `git mv rfcs/draft/process/0016-a-audit-write-path.md rfcs/accepted/process/0016-a-audit-write-path.md`
2. Update Status header: `Draft v3` → `Accepted v3.1`
3. Update Version History table with promotion entry
4. Grep for `rfcs/draft/process/0016-a` stale references + update to
   `rfcs/accepted/process/0016-a` (post-promotion self-ref path update)
5. Commit (NO PUSH per [[Initiative user-only]] + [[git-workflow]])
6. User owns: `git push origin next` + `gh pr create next:main`

## Acceptance Criteria

- [ ] RFC-0016-a DRY CLOSED (R3 = zero-finding round 2)
- [ ] RFC-0016-a promoted Draft → Accepted (via `git mv` + status
      header bump + VH entry)
- [ ] Cross-references to RFC-0015-b (parent amendment) + `0015-b-substrate-defect-impl`
      (paired implementation) added to RFC-0016-a header section
- [ ] Substrate-faithfulness verified post-RFC-0015-b landing: each
      RFC-0016-a §x.x matches the corrected substrate from RFC-0015-b
- [ ] Prettier compliance on RFC-0016-a (Mermaid diagrams; consistent
      heading hierarchy; markdown formatting)
- [ ] Closure audit doc written (`docs/audits/2026-09-XX-rfc-0016-a-promotion.md`)
- [ ] Memory card written (`~/.claude/projects/.../memory/rfc-0016-a-promotion-YYYY-MM-DD.md`)
- [ ] MEMORY.md index updated with closure card
- [ ] Commit (NO PUSH)

## Substrate-faithfulness verification

Per [[substrate-faithfulness-verification]], before drafting the
DRY review prompt for RFC-0016-a, the agent MUST verify the
following substrate claims against actual code (NOT against RFC text):

- `crates/octo-wallet/src/agent.rs::transition_agent` exists at
  `transition_agent` and uses the corrected lock discipline
  (`parking_lot::Mutex::try_lock` per RFC-0015-b §X.1)
- `crates/octo-wallet/src/agent.rs::lookup_agent` normalizes
  `AgentNotFound` per RFC-0015-b §X.3
- `crates/octo-wallet/src/agent.rs::AgentRecord` has `state_version`
  field per RFC-0015-b §X.5
- `crates/octo-audit/src/audit_write.rs::append_agent_transition_event`
  surfaces `audit_chain.tip.state_version` per RFC-0015-b §X.5
- `crates/octo-wallet/src/error.rs::WalletError::AlreadyInTransition`
  is REACHABLE (not dead surface) per RFC-0015-b §X.1

These verifications gate the DRY review spawn.

## Cross-references

- `missions/open/0015-b-substrate-defect-amendment.md` (parent amendment RFC)
- `missions/open/0015-b-substrate-defect-impl.md` (paired implementation)
- RFC-0016 (Accepted 2026-09-14) — read-path substrate contract
- RFC-0015-a (Accepted 2026-09-14) — write-path surface contract
- `docs/audits/2026-09-14-rfc-0015-0016-plateau-declaration.md` — substrate defect context
- [[no-phantom-mission-pointer]] — paired RFC + implementation mission must both be valid
- [[Initiative user-only]] + [[git-workflow]] — push + remote writes user-owned

## Why gate

Release-gated on RFC-0015-b Accepted + `0015-b-substrate-defect-impl`
mission landing. This sequencing ensures RFC-0016-a inherits the
corrected substrate and avoids the re-amendment cycle that would
result from landing RFC-0016-a against uncorrected substrate.
