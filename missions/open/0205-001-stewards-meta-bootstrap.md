---
name: 0205-001-stewards-meta-bootstrap
description: Open 2026-08-20; RFC-0205 v2.0 post-acceptance Phase 0.1 gap closure. Bootstrap cipherocto-stewards-meta trust-anchor repo with 3 FPRs in trusted-keys.txt + first commit GPG-signed by 1-of-3. Operative condition: RFC-0205 v2.0 §Promotion Path Condition 2 ("Phase 0-1 complete").
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-20T00:00:00.000Z
  v: "2.0"
  supersedes: v1.0
  superseded_by: 0205-002-phase1-deliverables
  depends_on:
    - RFC-0205
    - 0205-002-phase1-deliverables
    - 0205-003-r10-review
---

# Mission `0205-001-stewards-meta-bootstrap` — OPEN 2026-08-20

## Scope

Post-acceptance Phase 0.1 gap closure for RFC-0205 v2.0 (Accepted 2026-08-20). Bootstrap `cipherocto-stewards-meta` external trust-anchor repository at https://github.com/CipherOcto/cipherocto-stewards-meta. Satisfies RFC-0205 v2.0 §Promotion Path Condition 2 ("Phase 0-1 complete") by closing the Phase 0.1 gap (trust-anchor repo instantiation).

Covers:

- Create empty repo `cipherocto-stewards-meta` with default branch `main`, **public** visibility (per LOW 2)
- Initial commit: `trusted-keys.txt` with 3 FPRs (one per YubiKey holder), formatted per §HW Key Custody §Quorum (40-hex char FPR per line, blank lines ignored, `#` comments allowed)
- First commit GPG-signed by 1-of-3 FPR (acceptable for bootstrap; production signatures require 2-of-3)
- Document bootstrap commit SHA in `docs/runbooks/stoolap-steward.md` §External Trust Root subsection only (this mission contributes the subsection; the runbook itself is owned by `0205-002-phase1-deliverables`)
- Tag the bootstrap commit with `bootstrap-v0` (informational; **lives only on `cipherocto-stewards-meta` trust-anchor repo, never on fork repo**)
- README.md explains role: external trust root, distributes `trusted-keys.txt` to fork freeze-tag ceremony + cipherocto workspace
- Each FPR verified via `ykman openpgp info` as FIDO2-only non-exportable
- Key ceremony held **off-site** with no CipherOcto personnel present; 1-of-3 external auditor FPRs participate
- Repo branch protection enforced: `gh api repos/CipherOcto/cipherocto-stewards-meta/branches/main/protection/required_signatures | jq -r .enabled` == true AND org 2FA enforced

## Acceptance Criterion

- `https://github.com/CipherOcto/cipherocto-stewards-meta` repo exists (created via `gh repo create`) and is **public**: `gh api repos/CipherOcto/cipherocto-stewards-meta | jq -r .private` == false
- `trusted-keys.txt` at HEAD has exactly 3 FPRs (validated by `awk '/^[A-F0-9]{40}$/' | wc -l`)
- First commit SHA captured via `git rev-list --max-parents=0 HEAD` (robust against subsequent README + `bootstrap-v0` tag commits)
- Each FPR in `trusted-keys.txt` verified via `ykman openpgp info` as **FIDO2-only non-exportable** (no SSH, no S/MIME, no resident-key-only fallback)
- 1-of-3 external auditor FPR present in `trusted-keys.txt`; key-ceremony held **off-site** with no CipherOcto personnel present; ceremony minutes filed as `docs/runbooks/stoolap-steward.md` §External Trust Root subsection (contributed by this mission; runbook owned by `0205-002-phase1-deliverables`)
- Branch protection on `main`: `gh api repos/CipherOcto/cipherocto-stewards-meta/branches/main/protection/required_signatures | jq -r .enabled` == true
- Org 2FA enforced: `gh api orgs/CipherOcto | jq -r .two_factor_requirement_enabled` == true
- Bootstrap commit SHA recorded in `docs/runbooks/stoolap-steward.md` §External Trust Root subsection
- Phase 0.1 tree-hash byte-equal: `diff <(git -C cipherocto-stewards-meta show bootstrap-v0^:trusted-keys.txt) <(curl -s https://raw.githubusercontent.com/CipherOcto/cipherocto-stewards-meta/bootstrap-v0/trusted-keys.txt) | wc -l` returns 0 (replaces TV-0205-05; Phase 0.1-appropriate since `bootstrap-v0` tag is minted by this mission itself; full TV-0205-05 freeze-tag gate deferred to first freeze-tag ceremony — TBD by `0205-002-phase1-deliverables`)
- Meta-AC: TV-0205-05 gate regression-tested on each future freeze-tag mint
- Compromise window: bootstrap key rotated + re-signed by ≥1 distinct holder within 7 days of mission close, **OR** 1-of-3 window with explicit compromise SLA documented in runbook §External Trust Root (decision logged in mission-close-out)

## Files / Artifacts

- New external repo `github.com/CipherOcto/cipherocto-stewards-meta` (out-of-workspace, public, branch-protected, 2FA-enforced)
- `docs/runbooks/stoolap-steward.md` §External Trust Root subsection only (workspace) — owned by `0205-002-phase1-deliverables`; this mission contributes the subsection content

## Cross-references

- RFC-0205 v2.0 §Definitions External trust root row
- RFC-0205 v2.0 §HW Key Custody §Quorum
- RFC-0205 v2.0 §Adversary Analysis HIGH #17 (external trust root mandate)
- RFC-0205 v2.0 §Promotion Path Condition 2 ("Phase 0-1 complete") — operative condition
- RFC-0205 v2.0 §Implementation Phases Phase 0.1
- RFC-0206 v2.0 §Cargo.toml Templates Layer A (newtype contract referenced by RFC-0205)

## Out of scope

- 2-of-3 quorum freeze tag ceremony (Phase 1.3 SHA-pinning; first freeze tag lands after this mission — freeze tag is minted post-mission by `0205-002-phase1-deliverables`)
- `firmware-allowlist.toml` content (owned by `0205-002-phase1-deliverables`)
- Production GPG signature gathering (acceptable 1-of-3 for bootstrap)
- Runbook `docs/runbooks/stoolap-steward.md` ownership — owned by `0205-002-phase1-deliverables`; this mission contributes §External Trust Root subsection only

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
