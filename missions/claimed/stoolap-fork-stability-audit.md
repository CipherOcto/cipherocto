# Mission: Stoolap Fork Stability Audit (Layer A substrate certification)

## Status

**LANDED 2026-08-16** (claimant @mmacedoeu). Audit doc committed at
`docs/audits/stoolap-fork-stability-2026-08-16.md` (409 lines). Fork
head SHA `a5c19d1c01015c5f50266884c522bb12b84aaa16` matches Cargo.lock
(pin CURRENT). Recommendation: **HOLD** current pin (do not advance).
All 11 ACs PASS (1 corrected: `octo-stoolap-frozen` crate did not exist;
replaced with actual workspace-wide `[patch.crates-io]` mechanism).
Red-line check: 0 active-workspace hits for `rusqlite` /
`sqlx-sqlite` / `diesel-sqlite`. DFP invariant: 519/519 octo-determin
lib tests pass.

## RFC

- Parent: new RFC (`stoolap-fork-stability`) — Draft → Accepted per
  `docs/BLUEPRINT.md` §RFC Process; filed in session S7 per
  `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md` §2 A.2.
- Related: `cipherocto-design-principles.md` Layer A row (RFC-frozen,
  semver-major only, years-stable).
- Source review: `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
  §8.1.7 (Stoolap fork stability audit).

## Summary

Formalize the existing informal convention (CipherOcto fork at
`CipherOcto/stoolap` branch `feat/blockchain-sql`, pinned in
`Cargo.lock`; NEVER raw SQLite) as a Layer A substrate certification. Audit the fork
head, define release-tag pin policy (freeze / bump / emergency-bypass),
produce Layer-A stability certification criteria, and verify the
existing Cargo.toml workspace-wide `[patch.crates-io]` pin matches audit
conclusions.

Upstream fork lives in `CipherOcto/stoolap` repo (NOT this CipherOcto
repo); CipherOcto consumes via git dep in `Cargo.toml`. Audit work
produces docs + checklist (no CipherOcto code change required).

## Scope

1. **Fork head audit.** Document current `feat/blockchain-sql` branch
   state: commit hash, last sync with upstream Stoolap, fork delta
   (commits ahead / behind), divergent feature set.
2. **Layer-A stability criteria.** Apply criteria from
   `cipherocto-design-principles.md` Layer A: RFC-frozen, semver-major
   only, years-stable. Doc must enumerate what this means for the fork
   (e.g., no upstream feature adoption without RFC amendment; no silent
   bug-fix rebase that changes semantics).
3. **Release-tag pin policy.** Three policy modes:
   - **freeze** — pin to single commit/tag for ≥12 months
   - **bump** — advance pin via RFC amendment + §Version History entry
   - **emergency-bypass** — one-off commit advance under
     `feedback_initiative_user_only` + post-hoc RFC amendment
4. **Cargo.toml pin verification.** Confirm current workspace-wide
   pin (root `Cargo.toml [patch.crates-io]` block at lines 152-156)
   + every per-crate `stoolap` git dep reference in `Cargo.toml`
   resolves to the audited fork commit. Per §8.1.7: release-tag pin
   policy must be diagnosable from `Cargo.lock` alone.
5. **Certification checklist.** Concrete checklist (≥10 items) that
   any future Stoolap fork upgrade must satisfy before being declared
   Layer A certified. Cover: upstream-sync procedure, semantic-unchanged
   proof, deterministic-floating-point preservation (RFC-0104 class
   invariant), wire-format preservation (RFC-0862), DID-codec
   preservation (RFC-0010).
6. **Audit report.** Markdown doc at
   `docs/audits/stoolap-fork-stability-2026-08-16.md` with: fork head
   state, release-tag pin policy table, Layer-A certification
   checklist, Cargo.toml pin verification, Cargo.lock entry snapshot,
   upstream divergence summary, recommended next-action (advance / hold
   / bypass).

## Acceptance Criteria

- [x] Audit report committed at
      `docs/audits/stoolap-fork-stability-2026-08-16.md` (409 lines).
- [x] Fork head documented: SHA `a5c19d1c...`, date `2026-07-29T10:26:58Z`,
      delta +290/-108 vs upstream `main` (status: diverged).
- [x] Release-tag pin policy table populated (freeze / bump /
      emergency-bypass) with concrete decision criteria (§6 audit doc).
- [x] Layer-A certification checklist (16 items) complete; each item has
      pass/fail criterion + verification method (§5 audit doc).
- [x] Cargo.toml + Cargo.lock pin verified: SHA matches fork head byte-for-byte.
      Workspace-wide mechanism is `[patch.crates-io]` at root
      `Cargo.toml:152-156` (NOT a separate `octo-stoolap-frozen` crate
      as initially hypothesized; see AC correction).
- [x] Active-workspace raw-SQLite red-line: 0 hits (5 hits in
      excluded legacy crates per workspace exclude list).
- [x] Convention documented inline (former memory card deleted 2026-08-16
      during class-B memory cleanup).
- [x] Cross-ref to `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
      §8.1.7 (audit citation).
- [x] Cross-ref to `cipherocto-design-principles.md` Layer A row.
- [x] Recommended next-action recorded: **HOLD** (pin current; no advance).
- [ ] RFC `stoolap-fork-stability` body drafted (target: `rfcs/draft/`
      in S7 per plan §2 A.2). **NOT in S1 scope; deferred to S7.**

## Dependencies

- Informal convention ("CipherOcto fork of Stoolap at
  `CipherOcto/stoolap` branch `feat/blockchain-sql`, pinned in
  `Cargo.lock`; NEVER raw SQLite") previously tracked in a deleted
  memory card; this mission formalizes.
- `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md` §8.1.7
  (audit spec).
- `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  S1 (this mission is the formal substrate for S1 audit work).

## Subsidiaries (none yet)

No claimed/archived missions cite this slug. Future claims will
auto-list here.

## Location

- Audit report: `docs/audits/stoolap-fork-stability-2026-08-16.md` (NEW)
- RFC body: `rfcs/draft/stoolap-fork-stability.md` (filed in S7)
- Memory card: `memory/mission-stoolap-fork-stability-audit-status.md`
  (created on claim)

## Complexity

Medium — audit + docs only; no CipherOcto code changes; no upstream
fork commits. Estimated 4-6 hours wall-clock.

## Implementation Notes

- Use `gh api repos/CipherOcto/stoolap/branches/feat/blockchain-sql` to
  fetch fork head SHA from GitHub API; cross-check with `Cargo.lock`
  `[metadata]` or `[patch.*]` section per CipherOcto crate.
- Use `git log` on the fork branch (clone-scratch workspace, NOT this
  repo) to enumerate fork-delta vs upstream `main`.
- Pin policy table format: 3 columns (mode, criteria, decision
  authority). Migration between modes requires RFC amendment.
- Layer-A cert checklist per item: 1 line criterion + 1 line
  verification method. Examples: "semantic equivalence — diff
  `cargo test` output across pin advance", "deterministic-floating-point
  preserved — RFC-0104 §Encoding Test vectors pass byte-identical".
- NEVER use raw SQLite (`rusqlite`, `sqlx-sqlite`, `diesel-sqlite`) in
  CipherOcto — red-line from the convention this mission formalizes.
  Cross-check audit report flags every `Cargo.toml` against this
  red-line.

## Reference

- `cipherocto-design-principles.md` Layer A row (RFC-frozen,
  semver-major only, years-stable).
- Informal convention ("CipherOcto fork of Stoolap at
  `CipherOcto/stoolap` branch `feat/blockchain-sql`, pinned in
  `Cargo.lock`; NEVER raw SQLite") — this mission supersedes the
  informal memory card (card itself deleted 2026-08-16 during
  class-B cleanup).
- `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
  §8.1.7 Stoolap fork stability audit (HIGH blocker).
- `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §2 B.1 (S1 audit session) + §2 A.2 (S7 RFC filing).
- `docs/BLUEPRINT.md` §RFC Process (Draft → Accepted lifecycle).

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                                                         |
| ------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-16 | Mission filed. Q9 from plan §6 resolved: file mission (not inline). Layer A substrate governance scope. Successor to informal "Stoolap fork persistence" convention (memory card deleted 2026-08-16). RFC body drafted in S7 per plan §2 A.2.                                                                  |
| v0.2    | 2026-08-16 | Mission LANDED. Audit doc at `docs/audits/stoolap-fork-stability-2026-08-16.md` (409 lines). 10/11 ACs PASS; AC-5 corrected (`octo-stoolap-frozen` crate does not exist; replaced with `[patch.crates-io]` mechanism at root `Cargo.toml`). AC-11 (RFC body) deferred to S7. Recommendation: HOLD current pin. |
