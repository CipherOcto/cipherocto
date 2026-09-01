---
name: 0011-d-M9-doc-followon-token-design-md-10
description: Pure-doc follow-on per RFC-0011-d §Mission Decomposition M9 row: add §10 cross-reference in `docs/04-tokenomics/token-design.md` noting that new OCTO-only roles (`recorder`, `wallet`) intentionally absent from §10 table; release-gated on first dual-stake role addition post-Phase 1.
metadata:
  node_type: substrate-cli
  type: doc-followon
  originSessionId: RFC-0011-d author session
  created: 2026-08-31
  v: "1.1"
  release_gate:
    require: "first dual-stake role addition post-Phase 1"
    released_version: TBD
  depends_on: []
status: Open
---

# 0011-d-M9-doc-followon-token-design-md-10 — `token-design.md` §10 cross-reference per RFC-0011-d §Mission Decomposition M9

**Status:** Open (2026-08-31) — release-gated on first dual-stake role addition post-Phase 1.
**Substrate:** `docs/04-tokenomics/token-design.md` (already exists per project docs structure)
**Parent:** RFC-0011-d
**Depends on:** (none — pure docs per RFC §Mission Decomposition M9 row)
**Release gate:** first dual-stake role addition post-Phase 1

## Status

Open (2026-08-31). Ninth of 9 Phase 1 atomic missions. Pure-doc follow-on — no code changes. Per RFC §Mission Decomposition M9 row, M9 is gated on the FIRST dual-stake role addition post-Phase 1 (not on any other mission).

## Substrate (RFC-0011-d)

§Mission Decomposition M9 row (canonical):

- "Follow-on mission (per §Key Files + F-8): add a §10 cross-reference in `docs/04-tokenomics/token-design.md` noting that new OCTO-only roles (`recorder`, `wallet`) intentionally absent from §10 table; gated on first dual-stake role addition post-Phase 1"

§Key Files + F-8 (cross-reference to `token-design.md` §10 Dual-Stake table).

## Parent

RFC-0011-d §Mission Decomposition M9 row; §Key Files + F-8; §7.5 Role Summary (canonical role → role-token mapping).

## Depends on

(none — pure docs per RFC §Mission Decomposition M9 row)

## Acceptance Criteria

- [ ] `docs/04-tokenomics/token-design.md` §10 NEW section added (per RFC §Mission Decomposition M9 row)
- §10 title: "Dual-Stake Role Mapping"
- §10 includes Dual-Stake table per existing format
- §10 footnote: "OCTO-only roles `recorder`, `wallet` (added per RFC-0011-d §7.5) are intentionally absent from this §10 table because they require no role-token stake (dual-stake model is empty for OCTO-only roles; only `requires_octo_min` is enforced)."
- §10 footnote cross-references RFC-0011-d §7.5 Role Summary + §Mission Decomposition
- §10 has NO `(Draft)` / `(Accepted)` parens in prose cross-references (per CLAUDE.md §RFC Reference Conventions Reaffirmed)
- Bare RFC numbers in cross-references (no version pins)
- `npx prettier --write docs/04-tokenomics/token-design.md` PASS
- `bash scripts/validate_cites.sh docs/04-tokenomics/token-design.md` reports 0 INVALID
- NO code changes anywhere in repo
- Release gate ENFORCED: this mission does NOT claim until the FIRST dual-stake role addition post-Phase 1 (per RFC §Mission Decomp M9 row `release_gate:` field)

## Scope

Pure-doc follow-on. NO code. NO mission YAML edits outside this one.

## Sub-steps

1. **VERIFY RELEASE GATE** (Sub-step 1; abort if not met)
2. Read current `docs/04-tokenomics/token-design.md` to find §10 boundary (or add §10 if absent)
3. Add §10 "Dual-Stake Role Mapping" section
4. Add Dual-Stake table per existing format
5. Add §10 footnote noting OCTO-only roles intentionally absent
6. Wire cross-references (bare RFC numbers, no version pins)
7. Run prettier
8. Run cite sweep
9. Verify no code touched (`git diff --stat` shows only docs/)

## Test Vectors

N/A — pure-doc mission. Verification = cite sweep 0 INVALID + prettier PASS + release gate met.

## Layer direction (per [[cipherocto-design-principles]])

- Doc-only — no layer model impact
- Layer model referenced in §10 content (RFC-0900 + RFC-0011-d context)

## Backward compat

Doc-only — no API impact. NO migration concerns.

## Risk

- **Doc-only PR confusion**: PR may be auto-flagged as "no-code" by CI. Mitigation: PR title prefix `[DOC] RFC-0011-d follow-on: token-design.md §10`.
- **Cross-reference rot**: §10 footnote references RFC-0011-d §7.5 + §Mission Decomposition. Mitigation: anchor by section name (not file:line per [[no-line-refs-anywhere]]).
- **Release gate slip**: M9 should land WHEN the first dual-stake role is added, not before. Mitigation: explicit release_gate field; gate check in Sub-step 1.
- **§10 absent vs present**: token-design.md may or may not have §10 today. Mitigation: detect in Sub-step 2; add if absent; append footnote if present.

## Notes

- Pure-doc — fastest of the 9 atomic missions (after release gate clears)
- Lands as standalone PR per [[implementation-workflow-hook]]
- Per RFC §Mission Decomp M9 row: depends_on = (none — pure docs); release_gate = first dual-stake role addition post-Phase 1
- Per [[deferred-vs-unspecified]]: gated release (deferred), not unspecified

## Cross-references

- RFC-0011-d §Mission Decomposition M9 row (canonical scope)
- RFC-0011-d §7.5 (Role Summary canonical role → role-token mapping; OCTO-only role list)
- RFC-0011-d §Key Files + F-8 (cross-reference to `token-design.md` §10)
- RFC-0900 (slash ledger; dual-stake model substrate)
- `docs/04-tokenomics/token-design.md` §10 (target location)
- [[no-line-refs-anywhere]] — §section refs not file:line
- [[implementation-workflow-hook]] — doc-only missions still go through Claimed → Completed cycle
- [[deferred-vs-unspecified]] — gated release is deferred, not unspecified

## Claimant

@unassigned (mission lifecycle: Open — release-gated on first dual-stake role addition post-Phase 1)
