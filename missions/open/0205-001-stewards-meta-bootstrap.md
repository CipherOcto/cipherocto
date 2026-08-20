---
name: 0205-001-stewards-meta-bootstrap
description: Open 2026-08-20; RFC-0205 v2.0 post-acceptance Phase 0.1 gap closure. Bootstrap cipherocto-stewards-meta trust-anchor repo with 3 FPRs in trusted-keys.txt + first commit GPG-signed by 1-of-3. Operative condition: RFC-0205 v2.0 §Promotion Path Condition 2 ("Phase 0-1 complete").
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-20T00:00:00.000Z
  v: "2.1"
  supersedes: v1.0
  superseded_by: 0205-002-phase1-deliverables
  depends_on:
    - RFC-0205
---

# Mission `0205-001-stewards-meta-bootstrap` — OPEN 2026-08-20

## Scope

Post-acceptance Phase 0.1 gap closure for RFC-0205 v2.0 (Accepted 2026-08-20). Bootstrap `cipherocto-stewards-meta` external trust-anchor repository at https://github.com/CipherOcto/cipherocto-stewards-meta. Satisfies RFC-0205 v2.0 §Promotion Path Condition 2 ("Phase 0-1 complete") by closing the Phase 0.1 gap (trust-anchor repo instantiation).

Covers:

- Create empty repo `cipherocto-stewards-meta` with default branch `main`, **public** visibility (per LOW 2)
- Initial commit: `trusted-keys.txt` with 3 FPRs (one per YubiKey holder), formatted per §HW Key Custody §Quorum (40-hex char FPR per line, blank lines ignored, `#` comments allowed)
- First commit GPG-signed by 1-of-3 FPR (acceptable for bootstrap; production signatures require 2-of-3)
- Document bootstrap commit SHA in new file `docs/runbooks/stoolap-steward-external-trust-root-subsection.md` (owned by this mission; `0205-002-phase1-deliverables` merges the file into the parent runbook `docs/runbooks/stoolap-steward.md`)
- Tag the README commit (commit 2) with `bootstrap-v0` (informational; **lives only on `cipherocto-stewards-meta` trust-anchor repo, never on fork repo**)
- README.md explains role: external trust root, distributes `trusted-keys.txt` to fork freeze-tag ceremony + cipherocto workspace
- Each FPR verified via `ykman fido attest` as FIDO2-only non-exportable (per RFC-0205 §HW Key Custody §Firmware Attestation clause a.1)
- Key ceremony held **off-site** with no CipherOcto personnel present; all 3 FPR holders are external (no CipherOcto-affiliated FPRs participate)
- Repo branch protection enforced: `gh api repos/CipherOcto/cipherocto-stewards-meta/branches/main/protection/required_signatures | jq -r .enabled` == true AND org 2FA enforced

## Acceptance Criterion

- `https://github.com/CipherOcto/cipherocto-stewards-meta` repo exists (created via `gh repo create`) and is **public**: `gh api repos/CipherOcto/cipherocto-stewards-meta | jq -r .private` == false
- `trusted-keys.txt` at HEAD has exactly 3 FPRs (validated by `awk '/^[A-F0-9]{40}$/' | wc -l`)
- First commit SHA captured via `git rev-list --max-parents=0 HEAD` (robust against subsequent README + `bootstrap-v0` tag commits)
- Each FPR in `trusted-keys.txt` verified via `ykman fido attest` as **FIDO2-only non-exportable** (no SSH, no S/MIME, no resident-key-only fallback) per RFC-0205 §HW Key Custody §Firmware Attestation clause a.1
- All 3 FPR holders are external (no CipherOcto personnel); key-ceremony held **off-site** with no CipherOcto personnel present; ceremony minutes filed as new file `docs/runbooks/stoolap-steward-external-trust-root-subsection.md` (owned by this mission; merged into parent runbook by `0205-002-phase1-deliverables`)
- Branch protection on `main`: `gh api repos/CipherOcto/cipherocto-stewards-meta/branches/main/protection/required_signatures | jq -r .enabled` == true
- Org 2FA enforced: `gh api orgs/CipherOcto | jq -r .two_factor_requirement_enabled` == true
- Bootstrap commit SHA recorded in `docs/runbooks/stoolap-steward-external-trust-root-subsection.md`
- Phase 0.1 tree-hash byte-equal: `diff <(git -C cipherocto-stewards-meta show bootstrap-v0:trusted-keys.txt) <(curl -s https://raw.githubusercontent.com/CipherOcto/cipherocto-stewards-meta/bootstrap-v0/trusted-keys.txt) | wc -l` returns 0 (replaces TV-0205-05; Phase 0.1-appropriate since `bootstrap-v0` tag is minted by this mission itself; full TV-0205-05 freeze-tag gate deferred to first freeze-tag ceremony — TBD by `0205-002-phase1-deliverables`). Note: `bootstrap-v0` tags the README commit (commit 2), not the root commit; `^` is invalid since `bootstrap-v0` is not the root commit.
- Compromise window: bootstrap key rotated + re-signed by ≥1 distinct holder within 7 days of mission close as a hard gate. SLA documentation moved to separate handoff AC in `0205-002-phase1-deliverables`.

## Files / Artifacts

- New external repo `github.com/CipherOcto/cipherocto-stewards-meta` (out-of-workspace, public, branch-protected, 2FA-enforced)
- New file `docs/runbooks/stoolap-steward-external-trust-root-subsection.md` (workspace) — owned by this mission; merged into parent runbook `docs/runbooks/stoolap-steward.md` by `0205-002-phase1-deliverables`. This mission must NOT modify `docs/runbooks/stoolap-steward.md` directly.

## Cross-references

- RFC-0205 v2.0 §Definitions External trust root row
- RFC-0205 v2.0 §HW Key Custody §Quorum
- RFC-0205 v2.0 §HW Key Custody §Key Custody Policy (external-only holder mandate)
- RFC-0205 v2.0 §HW Key Custody §Firmware Attestation clause a.1 (`ykman fido attest` enforcement)
- RFC-0205 v2.0 §Promotion Path Condition 2 ("Phase 0-1 complete") — operative condition
- RFC-0205 v2.0 §Implementation Phases Phase 0.1
- RFC-0206 v2.0 §Cross-RFC Atomicity (referenced for newtype contract across RFC-0205/0206 boundary)

## Out of scope

- 2-of-3 quorum freeze tag ceremony (Phase 1.3 SHA-pinning; first freeze tag lands after this mission — freeze tag is minted post-mission by `0205-002-phase1-deliverables`)
- `firmware-allowlist.toml` content (owned by `0205-002-phase1-deliverables`)
- Production GPG signature gathering (acceptable 1-of-3 for bootstrap)
- Runbook `docs/runbooks/stoolap-steward.md` ownership — owned by `0205-002-phase1-deliverables`; this mission contributes standalone file `docs/runbooks/stoolap-steward-external-trust-root-subsection.md` only, which `0205-002-phase1-deliverables` merges in.
- TV-0205-05 regression test on future freeze-tag mints (tracked as separate Phase 1.3 mission AC by `0205-002-phase1-deliverables`).

## v2.1 Changes from v2.0

R2 review fixes (3 CRIT + 5 HIGH + 8 MED + 4 LOW = 20 findings):

- **CRIT 1:** Dropped `0205-002-phase1-deliverables` from YAML `depends_on:` (cycle: 0205-002 already lists 0205-001 as dep). Sibling missions removed from this mission's dep list to break the cycle.
- **CRIT 2:** Dropped `0205-003-r10-review` from YAML `depends_on:` (cycle: 0205-003 already lists 0205-001). Sibling missions removed from this mission's dep list to break the cycle.
- **CRIT 3:** AC #8 / Scope `bootstrap-v0^:trusted-keys.txt` → `bootstrap-v0:trusted-keys.txt`. Dropped invalid `^` parent traversal. Documented that `bootstrap-v0` tags the README commit (commit 2), not the root commit.
- **HIGH 1:** Replaced `ykman openpgp info` with `ykman fido attest` at all 4 sites (Scope line 30, AC line 39, plus removed old phrase references), per RFC-0205 §HW Key Custody §Firmware Attestation clause a.1.
- **HIGH 2:** Restructured runbook handoff — new file `docs/runbooks/stoolap-steward-external-trust-root-subsection.md` (owned by this mission) that `0205-002-phase1-deliverables` merges into `docs/runbooks/stoolap-steward.md`. This mission no longer writes to `docs/runbooks/stoolap-steward.md` directly.
- **HIGH 3:** AC #11 compromise-window OR-clause removed. OR escape hatch dropped; actual rotation within 7 days is now a hard gate. SLA documentation moved to separate handoff AC in `0205-002-phase1-deliverables`.
- **HIGH 4:** AC #10 Meta-AC forward-requirement removed. TV-0205-05 regression test on future freeze-tag mints is now tracked as separate Phase 1.3 mission AC by `0205-002-phase1-deliverables` (added to Out of scope).
- **HIGH 5:** Fabricated RFC cross-references `§Adversary Analysis HIGH #17` and `HIGH #12` removed from v2.0 Changes list. Replaced with actual RFC clause citations: `§Definitions External trust root row`, `§HW Key Custody §Key Custody Policy`, `§HW Key Custody §Firmware Attestation clause a.1`.
- **MED 1:** Cross-reference `RFC-0206 v2.0 §Cargo.toml Templates Layer A` dropped (irrelevant to trust-anchor bootstrap). Replaced with `RFC-0206 v2.0 §Cross-RFC Atomicity` (relevant to cross-RFC newtype contract).
- **MED 2:** Scope "Tag the bootstrap commit" ambiguity fixed — "Tag the README commit (commit 2) with `bootstrap-v0`" (explicit commit reference).
- **MED 3:** All 3 FPR holders clarified as external — Scope line + AC line explicitly state no CipherOcto-affiliated FPRs participate, per RFC-0205 §HW Key Custody §Key Custody Policy.
- **MED 4:** Description header retained as-is (RFC-0205 v2.0 still operative condition for §Promotion Path Condition 2). No change needed.
- **MED 5:** AC #8 explanatory note added about `^` invalidity + `bootstrap-v0` not being the root commit (CRIT 3 elaboration).
- **MED 6:** v2.0 Changes list wording corrected — rephrased "RFC §Adversary Analysis HIGH #17" → actual RFC clause; rephrased "RFC HIGH #12" → actual RFC clause.
- **MED 7:** AC #11 SLA documentation moved to separate handoff AC in `0205-002-phase1-deliverables` (mirrors HIGH 3).
- **MED 8:** Out of scope entry added for TV-0205-05 regression test tracking (mirrors HIGH 4).
- **LOW 1:** No whitespace/grammar-only edits applied (deferred to v2.2 if reviewer requests).
- **LOW 2:** Scope line FPR holder clarification mirrors MED 3.
- **LOW 3:** Runbook handoff clarification mirrors HIGH 2.
- **LOW 4:** No additional LOW-driven edits; v2.0 LOW list preserved as historical.

## v2.0 Changes from v1.0

- **CRIT 1:** Scope retitled "post-acceptance Phase 0.1 gap closure"; reference RFC-0205 v2.0 §Promotion Path Condition 2 as operative condition (RFC-0205 v2.0 already Accepted 2026-08-20).
- **CRIT 2:** External trust root mandate enforced — 1-of-3 external auditor FPR + off-site key ceremony with no CipherOcto personnel (RFC §Adversary Analysis HIGH #17).
- **HIGH 1:** AC #3 changed from `git verify-commit HEAD` to `git rev-list --max-parents=0 HEAD` (robust to README + `bootstrap-v0` tag commits).
- **HIGH 2:** TV-0205-05 replaced with Phase 0.1-appropriate tree-hash byte-equal check on `bootstrap-v0^:trusted-keys.txt` vs raw.githubusercontent `bootstrap-v0/trusted-keys.txt`; full freeze-tag gate deferred.
- **HIGH 3:** `docs/runbooks/stoolap-steward.md` double-ownership resolved — this mission contributes §External Trust Root subsection only; runbook owned by `0205-002-phase1-deliverables`.
- **HIGH 4:** Explicit runbook handoff declared: `docs/runbooks/stoolap-steward.md` owned by `0205-002-phase1-deliverables` Phase 1.8.
- **HIGH 5:** FIDO2-only enforcement AC added — each FPR verified via `ykman openpgp info` as non-exportable FIDO2-only (RFC HIGH #12).
- **HIGH 6:** Branch protection + org 2FA enforcement AC added — `gh api .../protection/required_signatures` == true AND org 2FA enforced.
- **HIGH 7:** Fabricated "§External Trust Root (cross-reference)" heading replaced with RFC-0205 v2.0 §Definitions External trust root row.
- **HIGH 8:** Phase 1.8 (runbook) cross-reference removed — handled by `0205-002-phase1-deliverables`.
- **MED 1:** YAML frontmatter `depends_on:` added (RFC-0205, `0205-002-phase1-deliverables`, `0205-003-r10-review`).
- **MED 2:** RFC-0206 v2.0 §Cargo.toml Templates Layer A added to Cross-references.
- **MED 3:** TV-0205-05 placeholders replaced with `bootstrap-v0` self-mint check; meta-AC for regression on each freeze-tag mint added.
- **MED 4:** `bootstrap-v0` tag scope clarified — trust-anchor repo only, never fork repo.
- **MED 5:** Compromise window AC added — bootstrap key rotated + re-signed by ≥1 distinct holder within 7 days OR 1-of-3 window with explicit SLA documented in runbook.
- **MED 6:** RFC-0205 v2.0 §Definitions External trust root row added to Cross-references.
- **MED 7:** Out of scope Phase 1.3 wording fixed — Phase 1.3 SHA-pinning (freeze tag minted post-mission).
- **LOW 1:** `awk '/^[A-F0-9]{40}$/' | wc -l` retained (RFC permissive format).
- **LOW 2:** Repo visibility AC added — `gh api repos/CipherOcto/cipherocto-stewards-meta | jq -r .private` == false.
- **LOW 3:** `superseded_by: 0205-002-phase1-deliverables` metadata added.
