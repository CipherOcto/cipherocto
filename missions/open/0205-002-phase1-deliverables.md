---
name: 0205-002-phase1-deliverables
v: "2.0"
supersedes: v1.0
description: Open 2026-08-20; RFC-0205 v2.0 Phase 1.3-1.11 deliverables — substrate Cargo.toml rev pin + 4 allowlist tomles + runbook + 3 audit directories.
depends_on:
  - 0205-001-stewards-meta-bootstrap
  - 0206-001-substrate-newtype
  - RFC-0205
  - RFC-0206
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-20T00:00:00.000Z
---

## v2.0 Changes from v1.0

R1 review (18 findings: 2 CRIT + 5 HIGH + 6 MED + 5 LOW) applied:

- **CRIT 1**: Deliverable count 11 → 9 (Phase 1.3-1.11 = 9 items; the "commit pinning cipherocto-stewards-meta SHA" is a property of 1.7, not a separate deliverable).
- **CRIT 2**: SCOPE CONFLICT resolution documented — `0205-002` takes ownership of `crates/octo-storage-core/Cargo.toml` `rev =` line; `0206-001-substrate-newtype` v2.0 drops its own `rev =` line per scope-conflict handoff.
- **HIGH 1**: `0206-001-substrate-newtype` added to `depends_on:` (Phase 1.3 edits the same file mission owns).
- **HIGH 2**: AC `rg` count ≤ 5 replaced with exact gating: 0 hits outside substrate + test-harness allowance.
- **HIGH 3**: TV-0205-05/-07/-21/-22/-23/-24 marked as forward-requirement (gate at mission close, not AC).
- **HIGH 4**: Runbook ownership resolved — `0205-002` owns full file; `0205-001` v2.0 restricted to §External Trust Root subsection handoff.
- **HIGH 5**: Freeze-tag ceremony added as explicit precondition (NOT mission output).
- **MED 1**: AC added — `rg '^\s*branch\s*=' crates/octo-storage-core/Cargo.toml` exits 1 (no `branch =` line per RFC §Cargo.toml Pinning).
- **MED 2**: AC added — `[features] default = ["allow-listed-ddl"]` referenced (handoff note to `0206-001`).
- **MED 3**: "freeze-tag ceremony 4-step procedure" renamed → "SHA256SUMS wrapper ceremony 4-step" (TV-0205-14 has 4 sub-steps for SHA256SUMS wrapper, not freeze-tag).
- **MED 4**: `mode = "local-pin"` for `external-root-config.toml` removed (fabricated; RFC §HW Key Custody §Quorum does not define `mode`).
- **MED 5**: Precondition AC added — fork repo cloned with `--object-format=sha256` (RFC §Determinism Requirements row 1) + git ≥ 2.42 (RFC §Security Considerations #3).
- **MED 6**: `vendor/stoolap/` added to Files/Artifacts (required by TV-0205-22).
- **MED 7**: All `1.X` references in mission text prefixed with `RFC-0205 §Implementation Phases` to disambiguate from RFC-0206 v2.0 numbering collision.
- **LOW 1**: `0205-001` SHA handoff disambiguated — pin COMMIT SHA (not tag SHA).
- **LOW 2**: Runbook content check expanded with concrete `rg`/`grep` gates per item.
- **LOW 3**: 1.4 capture procedure made explicit — `ykman fido attest` + `openssl verify` against vendored Yubico root cert.
- **LOW 4**: Half-claim acknowledged — `0205-002` covers 1.3-1.11 only; `0205-001` owns 1.1-1.2 (Phase 0.1 equivalents).
- **LOW 5**: Already addressed in HIGH 2.

---

# Mission `0205-002-phase1-deliverables` — OPEN 2026-08-20

## Scope

Land RFC-0205 v2.0 §Implementation Phases 1.3-1.11 deliverables in the cipherocto workspace. These are the substrate + allowlist + runbook + audit-log artifacts that gate RFC-0205 acceptance-path Conditions 2.

**Half-claim note:** RFC-0205 §Implementation Phases 1.1-1.2 (Phase 0.1 equivalents) are owned by `0205-001-stewards-meta-bootstrap`. This mission covers only 1.3-1.11 (9 deliverables).

**SCOPE CONFLICT resolution (`crates/octo-storage-core/Cargo.toml` `rev =` pin):** The `rev = "<sha-0>"` line is claimed by BOTH `0205-002` (Phase 1.3) AND `0206-001-substrate-newtype` (substrate skeleton). Resolution: `0205-002` takes ownership — this mission lands the `rev =` pin. `0206-001-substrate-newtype` v2.0 drops its own `rev =` line per scope-conflict handoff (documented separately).

**Runbook ownership resolution (`docs/runbooks/stoolap-steward.md`):** The runbook is claimed by BOTH `0205-001` (bootstrap commit SHA) AND `0205-002` (procedures runbook). Resolution: `0205-002` owns the full file. `0205-001` v2.0 is restricted to §External Trust Root subsection handoff (bootstrap commit SHA only, written into `external-root-config.toml`).

**Precondition — freeze-tag ceremony:** The freeze-tag ceremony is a PRECONDITION, not an output of this mission. The freeze-tag ceremony must complete BEFORE Phase 1.3 (`rev = "<sha-0>"`) lands, because the `rev =` pin requires the freeze-tag commit SHA. Per RFC-0205 v2.0 OOS line, the freeze-tag ceremony lands after this mission — therefore a strict ordering constraint applies: `0205-002` Phase 1.3 will not land until the freeze-tag ceremony (owned by `0205-001` Phase 0.2) completes first. The OOS line on this mission will be amended to reflect the actual ordering when `0205-001` Phase 0.2 lands.

**Precondition — fork clone determinism (RFC-0205 §Determinism Requirements row 1 + §Security Considerations #3):** Before Phase 1.3 lands, the fork repo MUST be cloned with `git clone --object-format=sha256 <fork-url>` (determinism requirement) AND git client version MUST be ≥ 2.42 (security consideration). Both preconditions are gated by AC.

Covers (per RFC-0205 v2.0 §Implementation Phases 1.3-1.11):

- **RFC-0205 §Implementation Phases 1.3** Land `crates/octo-storage-core/Cargo.toml` `rev = "<sha-0>"` pin (substrate sole fork consumer; pending RFC-0206 v2.0 §Cargo.toml Templates Layer A delivery). AC: `branch = "feat/blockchain-sql"` line REMOVED per RFC §Cargo.toml Pinning "branch = removed entirely" + TV-0205-01.
- **RFC-0205 §Implementation Phases 1.4** Land `crates/octo-storage-core/firmware-allowlist.toml` with initial 3 entries from current quorum tokens (`(AAGUID, firmware_version, attestation_certificate_sha256)` tuples per RFC-0205 v2.0 §HW Key Custody §Firmware Attestation). Capture procedure: `ykman fido attest <serial> --format openssl` per device + `openssl verify -CAfile vendor/yubico-root-ca.pem <(echo "<attestation_certificate>")` against vendored Yubico root cert.
- **RFC-0205 §Implementation Phases 1.5** Land `crates/octo-storage-core/test-harness-allowlist.toml` (initial empty; populates as tests land per TV-0205-07 gate)
- **RFC-0205 §Implementation Phases 1.6** Land `crates/octo-storage-core/frozen-source-allowlist.toml` (initial entry for v0 freeze tag tree-hash via `git rev-parse <freeze_tag>^{tree}`). Required artifact `vendor/stoolap/` created (per TV-0205-22).
- **RFC-0205 §Implementation Phases 1.7** Land `crates/octo-storage-core/external-root-config.toml` (pins cipherocto-stewards-meta bootstrap COMMIT SHA — note: COMMIT SHA, not tag SHA — per `0205-001` v2.0 §External Trust Root handoff)
- **RFC-0205 §Implementation Phases 1.8** Land `docs/runbooks/stoolap-steward.md` (procedures for SHA256SUMS wrapper ceremony 4-step procedure + bump ceremony + key revocation + firmware CVE replacement + emergency revocation). Note: the 4-step procedure is for the SHA256SUMS wrapper ceremony per TV-0205-14, NOT for freeze-tag.
- **RFC-0205 §Implementation Phases 1.9** Land `docs/audits/cve-bumps/` directory (empty; populated on bump per Phase 2.1)
- **RFC-0205 §Implementation Phases 1.10** Land `docs/audits/stoolap-ci-heterogeneity-log.md` (empty; populated by TV-0205-23 secondary-vendor runs)
- **RFC-0205 §Implementation Phases 1.11** Land `docs/audits/stoolap-firmware-cve-replacements.md` (empty; distinct from §Emergency Revocation log per RFC-0205 v2.0 §HW Key Custody)

**Note on `[features] default = ["allow-listed-ddl"]`:** Per RFC-0206 v2.0 §Cargo.toml Templates, the substrate `Cargo.toml` must include `[features] default = ["allow-listed-ddl"]`. This is owned by `0206-001-substrate-newtype` (substrate skeleton) — AC is gated via handoff: `0205-002` Phase 1.3 does NOT land until `0206-001` skeleton (with `[features]`) is in place.

## Acceptance Criterion

**Artifact count (9 deliverables):**

- 9 artifacts exist (1 Cargo.toml edit + 4 toml files + 1 runbook + 3 directory-or-markdown entries)
- The "commit pinning cipherocto-stewards-meta SHA" is a property of 1.7 (`external-root-config.toml`), not a separate deliverable

**Substrate sole-consumer gate (HIGH 2 + MED 1):**

- `rg '^\s*stoolap\s*=' crates/*/Cargo.toml | grep -v 'crates/octo-storage-core/' | grep -v 'sync-e2e-tests/stoolap-node' | wc -l` equals 0 (test-harness allowance per RFC-0205 §Cargo.toml Pinging — substrate is SOLE direct consumer; test-harness crates may reference `stoolap` for fixture purposes)
- `rg '^\s*branch\s*=' crates/octo-storage-core/Cargo.toml` exits 1 (no `branch =` line per RFC §Cargo.toml Pinning "branch = removed entirely" + TV-0205-01)

**Preconditions (HIGH 5 + MED 5):**

- Freeze-tag ceremony (owned by `0205-001` Phase 0.2) has completed BEFORE Phase 1.3 lands — verified by `git rev-parse <freeze_tag>` exit 0
- Fork repo was cloned with `git clone --object-format=sha256 <fork-url>` — verified by `git config --get core.repositoryformatversion` returning 1 + `cat .git/objects/info/alternates` (or equivalent) confirming sha256 object format
- Git client version ≥ 2.42 — verified by `git --version` output matching `git version 2.4[2-9]` or later

**Runbook content gates (LOW 2) — `docs/runbooks/stoolap-steward.md` MUST include all of:**

- `rg '^\s*## SHA256SUMS wrapper ceremony 4-step procedure' docs/runbooks/stoolap-steward.md` exits 0 (SHA256SUMS wrapper, NOT freeze-tag)
- `rg '^\s*## Bump ceremony' docs/runbooks/stoolap-steward.md` exits 0
- `rg '^\s*## Key revocation procedure' docs/runbooks/stoolap-steward.md` exits 0
- `rg '^\s*## Firmware CVE replacement procedure' docs/runbooks/stoolap-steward.md` exits 0
- `rg '^\s*## Emergency revocation procedure' docs/runbooks/stoolap-steward.md` exits 0

**Forward-requirement TVs (HIGH 3):** TV-0205-05, TV-0205-07, TV-0205-21, TV-0205-22, TV-0205-23, TV-0205-24 are forward-requirement — they gate Phase 2 / Phase 3 missions, NOT this mission's close-out AC. Their gates must pass at the close-out of the respective Phase 2 / Phase 3 mission that introduces them. This mission only ENSURES the artifacts (allowlist files, runbook, audit dirs) that those TVs will eventually validate.

## Files / Artifacts

- `crates/octo-storage-core/Cargo.toml` (RFC-0205 §Implementation Phases 1.3 — `0205-002` owns `rev =` line; `0206-001` owns newtype skeleton + `[features] default = ["allow-listed-ddl"]`)
- `crates/octo-storage-core/firmware-allowlist.toml` (RFC-0205 §Implementation Phases 1.4 — NEW)
- `crates/octo-storage-core/test-harness-allowlist.toml` (RFC-0205 §Implementation Phases 1.5 — NEW)
- `crates/octo-storage-core/frozen-source-allowlist.toml` (RFC-0205 §Implementation Phases 1.6 — NEW)
- `crates/octo-storage-core/external-root-config.toml` (RFC-0205 §Implementation Phases 1.7 — NEW; pins cipherocto-stewards-meta COMMIT SHA, not tag SHA)
- `docs/runbooks/stoolap-steward.md` (RFC-0205 §Implementation Phases 1.8 — NEW; `0205-002` owns full file)
- `docs/audits/cve-bumps/` (RFC-0205 §Implementation Phases 1.9 — NEW directory)
- `docs/audits/stoolap-ci-heterogeneity-log.md` (RFC-0205 §Implementation Phases 1.10 — NEW)
- `docs/audits/stoolap-firmware-cve-replacements.md` (RFC-0205 §Implementation Phases 1.11 — NEW)
- `vendor/stoolap/` (RFC-0205 §Implementation Phases 1.6 dependency — NEW directory; required by TV-0205-22)

## Cross-references

- RFC-0205 v2.0 §Implementation Phases 1.3-1.11
- RFC-0205 v2.0 §Cargo.toml Pinning
- RFC-0205 v2.0 §Determinism Requirements
- RFC-0205 v2.0 §Security Considerations
- RFC-0205 v2.0 §HW Key Custody §Quorum / §Firmware Attestation / §Emergency Revocation
- RFC-0205 v2.0 TV-0205-05, TV-0205-07, TV-0205-14, TV-0205-21, TV-0205-22, TV-0205-23, TV-0205-24
- RFC-0206 v2.0 §Cargo.toml Templates Layer A

## Out of scope

- cipherocto-stewards-meta repo creation (owned by `0205-001-stewards-meta-bootstrap`)
- Substrate newtype refactor + `[features] default = ["allow-listed-ddl"]` (owned by `0206-001-substrate-newtype`)
- 29 Layer B TYPE renames (owned by `0206-002-layer-b-type-renames`)
- 5 adapter crates (owned by `0206-004-adapter-crates`)
- First 2-of-3 freeze tag ceremony (owned by `0205-001` Phase 0.2; PRECONDITION for Phase 1.3 pin — must complete BEFORE this mission's Phase 1.3 lands)
- TV-0205-05, TV-0205-07, TV-0205-21, TV-0205-22, TV-0205-23, TV-0205-24 gate commands (forward-requirement — gate at Phase 2 / Phase 3 mission close-out, NOT at this mission's close-out)

## Dependencies

- `0205-001-stewards-meta-bootstrap` (cipherocto-stewards-meta bootstrap COMMIT SHA pinned in `external-root-config.toml` — COMMIT SHA, not tag SHA; also owns freeze-tag ceremony precondition for Phase 1.3)
- `0206-001-substrate-newtype` (substrate skeleton + `[features] default = ["allow-listed-ddl"]` must land BEFORE Phase 1.3 `rev =` pin; `0206-001` v2.0 drops its own `rev =` line per scope-conflict handoff)
- `RFC-0205` (governing RFC for Phase 1.3-1.11)
- `RFC-0206` (governing RFC for substrate newtype + `[features]` layer)
