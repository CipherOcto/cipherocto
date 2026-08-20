---
name: 0205-002-phase1-deliverables
description: Open 2026-08-20; RFC-0205 v2.0 Phase 1.3-1.11 deliverables — substrate Cargo.toml rev pin + 4 allowlist tomles + runbook + 3 audit directories.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-20T00:00:00.000Z
---

# Mission `0205-002-phase1-deliverables` — OPEN 2026-08-20

## Scope

Land RFC-0205 v2.0 Phase 1.3-1.11 deliverables in the cipherocto workspace. These are the substrate + allowlist + runbook + audit-log artifacts that gate RFC-0205 acceptance-path Conditions 2.

Covers (per RFC-0205 v2.0 §Implementation Phases 1.3-1.11):

- **1.3** Land `crates/octo-storage-core/Cargo.toml` `rev = "<sha-0>"` pin (substrate sole fork consumer; pending RFC-0206 v2.0 §Cargo.toml Templates Layer A delivery)
- **1.4** Land `crates/octo-storage-core/firmware-allowlist.toml` with initial 3 entries from current quorum tokens (`(AAGUID, firmware_version, attestation_certificate_sha256)` tuples per RFC-0205 v2.0 §HW Key Custody §Firmware Attestation)
- **1.5** Land `crates/octo-storage-core/test-harness-allowlist.toml` (initial empty; populates as tests land per TV-0205-07 gate)
- **1.6** Land `crates/octo-storage-core/frozen-source-allowlist.toml` (initial entry for v0 freeze tag tree-hash via `git rev-parse <freeze_tag>^{tree}`)
- **1.7** Land `crates/octo-storage-core/external-root-config.toml` (mode = `local-pin` for CI; pins cipherocto-stewards-meta bootstrap SHA)
- **1.8** Land `docs/runbooks/stoolap-steward.md` (procedures for freeze-tag ceremony + bump ceremony + key revocation + firmware CVE replacement)
- **1.9** Land `docs/audits/cve-bumps/` directory (empty; populated on bump per Phase 2.1)
- **1.10** Land `docs/audits/stoolap-ci-heterogeneity-log.md` (empty; populated by TV-0205-23 secondary-vendor runs)
- **1.11** Land `docs/audits/stoolap-firmware-cve-replacements.md` (empty; distinct from §Emergency Revocation log per RFC-0205 v2.0 §HW Key Custody)

## Acceptance Criterion

- 11 artifacts exist (1 Cargo.toml edit + 4 toml files + 1 runbook + 3 directory-or-markdown entries + 1 commit pinning cipherocto-stewards-meta SHA + 1 fork SHA bound)
- TV-0205-05, TV-0205-07, TV-0205-21, TV-0205-22, TV-0205-23, TV-0205-24 gate commands pass (allowlist integrity + frozen-source + CI heterogeneity + firmware allowlist tuple well-formedness)
- `crates/octo-storage-core/` is sole direct fork consumer at workspace level (verified by `rg '^\s*stoolap\s*=' crates/*/Cargo.toml | wc -l` ≤ 5: 4 adapter + substrate)
- `docs/runbooks/stoolap-steward.md` includes: freeze-tag ceremony 4-step procedure + bump ceremony + key revocation + firmware CVE replacement + emergency revocation

## Files / Artifacts

- `crates/octo-storage-core/Cargo.toml` (1.3 — pending RFC-0206-001)
- `crates/octo-storage-core/firmware-allowlist.toml` (1.4 — NEW)
- `crates/octo-storage-core/test-harness-allowlist.toml` (1.5 — NEW)
- `crates/octo-storage-core/frozen-source-allowlist.toml` (1.6 — NEW)
- `crates/octo-storage-core/external-root-config.toml` (1.7 — NEW)
- `docs/runbooks/stoolap-steward.md` (1.8 — NEW)
- `docs/audits/cve-bumps/` (1.9 — NEW directory)
- `docs/audits/stoolap-ci-heterogeneity-log.md` (1.10 — NEW)
- `docs/audits/stoolap-firmware-cve-replacements.md` (1.11 — NEW)

## Cross-references

- RFC-0205 v2.0 §Implementation Phases 1.3-1.11
- RFC-0205 v2.0 §HW Key Custody §Quorum / §Firmware Attestation / §Emergency Revocation
- RFC-0205 v2.0 TV-0205-05, TV-0205-07, TV-0205-21, TV-0205-22, TV-0205-23, TV-0205-24

## Out of scope

- cipherocto-stewards-meta repo creation (owned by `0205-001-stewards-meta-bootstrap`)
- Substrate newtype refactor (owned by `0206-001-substrate-newtype`)
- 29 Layer B TYPE renames (owned by `0206-002-layer-b-type-renames`)
- 5 adapter crates (owned by `0206-004-adapter-crates`)
- First 2-of-3 freeze tag ceremony (requires all Phase 1 deliverables; lands after this mission)

## Dependencies

- `0205-001-stewards-meta-bootstrap` (cipherocto-stewards-meta bootstrap SHA pinned in `external-root-config.toml`)
