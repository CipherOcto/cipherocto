---
name: 0205-001-stewards-meta-bootstrap
description: Open 2026-08-20; RFC-0205 v2.0 Phase 0.1 precondition. Bootstrap cipherocto-stewards-meta trust-anchor repo with 3 FPRs in trusted-keys.txt + first commit GPG-signed by 1-of-3.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-20T00:00:00.000Z
---

# Mission `0205-001-stewards-meta-bootstrap` — OPEN 2026-08-20

## Scope

Bootstrap `cipherocto-stewards-meta` external trust-anchor repository at https://github.com/CipherOcto/cipherocto-stewards-meta. This repo is the precondition (Phase 0.1) for RFC-0205 v2.0 acceptance-path.

Covers:

- Create empty repo `cipherocto-stewards-meta` with default branch `main`
- Initial commit: `trusted-keys.txt` with 3 FPRs (one per YubiKey holder), formatted per §HW Key Custody §Quorum (40-hex char FPR per line, blank lines ignored, `#` comments allowed)
- First commit GPG-signed by 1-of-3 FPR (acceptable for bootstrap; production signatures require 2-of-3)
- Document bootstrap commit SHA in `docs/runbooks/stoolap-steward.md` §External Trust Root (cross-reference)
- Tag the bootstrap commit with `bootstrap-v0` (informational; not subject to 2-of-3 ceremony)
- README.md explains role: external trust root, distributes `trusted-keys.txt` to fork freeze-tag ceremony + cipherocto workspace

## Acceptance Criterion

- `https://github.com/CipherOcto/cipherocto-stewards-meta` repo exists (created via `gh repo create`)
- `trusted-keys.txt` at HEAD has exactly 3 FPRs (validated by `awk '/^[A-F0-9]{40}$/' | wc -l`)
- First commit GPG signature verified: `git verify-commit HEAD` reports valid signature from one of the 3 FPRs
- Bootstrap commit SHA recorded in `docs/runbooks/stoolap-steward.md` §External Trust Root
- TV-0205-05 gate command: `diff <(git show <freeze_tag>:trusted-keys.txt) <(curl -s https://raw.githubusercontent.com/CipherOcto/cipherocto-stewards-meta/<known-sha>/trusted-keys.txt) | wc -l` returns 0 (after first freeze tag ceremony lands)

## Files / Artifacts

- New external repo `github.com/CipherOcto/cipherocto-stewards-meta` (out-of-workspace)
- New local file `docs/runbooks/stoolap-steward.md` (workspace) — bootstrap SHA + ceremony procedures

## Cross-references

- RFC-0205 v2.0 §HW Key Custody §Quorum
- RFC-0205 v2.0 §Implementation Phases Phase 0.1
- RFC-0205 v2.0 §Implementation Phases Phase 1.8 (runbook)
- RFC-0205 v2.0 TV-0205-05

## Out of scope

- 2-of-3 quorum freeze tag ceremony (requires Phase 1.3 first; first freeze tag lands after this mission)
- `firmware-allowlist.toml` content (owned by `0205-002-phase1-deliverables`)
- Production GPG signature gathering (acceptable 1-of-3 for bootstrap)
