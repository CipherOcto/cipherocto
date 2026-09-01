---
name: 0011-g-governance-commands-phase1
description: Implement `octo governance snapshot` per RFC-0011-g Phase 1 (snapshot only; unblocked at Draft)
metadata:
  node_type: substrate-cli
  type: cli-substrate-extension
  originSessionId: RFC-0011-g author session
  created: 2026-08-31
  v: "1.0"
  depends_on:
    - RFC-0011-g
    - RFC-0002
    - RFC-0011
    - mission 0011-core-output-envelope-redaction
    - mission 0011-identity-commands
    - mission 0011-capability-commands
    - mission 0011-policy-commands
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-01
---

# 0011-g-governance-commands-phase1 — `octo governance snapshot`

**Status:** Open — unblocked. Phase 1 of RFC-0011-g; substrate prereqs (RFC-0011,
RFC-0855, RFC-0855p-b, RFC-0855p-c, RFC-0010) all Accepted at RFC-0011-g
filing time per RFC-0011-g §Implementation Phases. Implementation kickoff
user-gated per [[feedback_initiation_user_only]] + [[git-workflow]] once
RFC-0011-g reaches Accepted.
**Substrate:** RFC-0011-g §Substrate `[ADD]` — `octo_governance::snapshot`
**Parent:** RFC-0011-g
**Depends on:**

- Mission `0011-core-output-envelope-redaction` — `OutputEnvelope<T>` +
  `OctoCliError` + clap root substrate
- Mission `0011-identity-commands` — `active_signer()` substrate exposed by
  identity mission (snapshot does not require HSM but reuses identity plumbing)
- Mission `0011-capability-commands` — `OctoCliError` variants inherit the
  capability-mission redactor + clap tree conventions
- Mission `0011-policy-commands` — establishes the `OctoCliRedactor` patterns
  that governance redaction mirrors
  **Blocks:** `0011-g-governance-attest-vote` (Phase 2; depends on Phase 1
  substrate landing first; release-gated on RFC-0855p-d AND RFC-0855p-e)

## Status

Open — unblocked. All Required substrate RFCs are Accepted; Phase 1 ships on
RFC-0011-g acceptance alone per RFC-0011-g §Implementation Phases Phasing
Rationale. RFC-0002 (agent manifest) accepted in parallel — the manifest
re-exposes snapshot fields for agent-side consumption.

## RFC

RFC-0011-g §Subcommand Taxonomy — `octo governance snapshot`
(rfcs/draft/process/0011-g-governance-subcommands.md)

## Dependencies

See YAML frontmatter `depends_on` block above. Hard sequencing per
RFC-0011-g §Implementation Phases: Phase 1 (`snapshot`) must land before
Phase 2 (`attest` + `vote`).

## Acceptance Criteria

- [ ] `octo governance snapshot` implemented + unit-tested (TV-GOV-S1,
      TV-GOV-S2, TV-GOV-S3, TV-GOV-S4 pass)
- [ ] `OctoGovernanceSnapshotCache` (LRU + 600s TTL) wired in
      `crates/octo-governance/src/cache.rs` + unit-tested
- [ ] `--chain-id <chain-id>` filter parsed via RFC-0010 canonical form +
      unit-tested
- [ ] `--proposal-filter <state>` filter parsed substrate-side +
      unit-tested (note: substrate param is `proposal_filter` singular)
- [ ] `--force-refresh` flag bypasses cache + unit-tested
- [ ] `--json` flag forces machine-readable JSON output (TTY override per
      RFC-0011 §Output Envelope) + unit-tested
- [ ] `OutputEnvelope<SnapshotOutput>` rendered per RFC-0011-g §Output Envelope
      (TTY table / JSON per TTY detection)
- [ ] `SnapshotOutput { remaining_seconds }` surfaced per RFC-0011-g §Subcommand Taxonomy
      (TTL = 600s; `expires_at_unix = taken_at_unix + 600`)
- [ ] `SnapshotRef.snapshot_id` content-addressed (BLAKE3-256 of canonical
      projection) + unit-tested
- [ ] `OutputEnvelope<T>::schema_version = 6` pinned for RFC-0011-g
- [ ] Cross-mission AC: snapshot integrates with core mission's
      `OutputEnvelope<T>` + identity mission's active DID plumbing + capability
      mission's `OctoCliError` shape + RFC-0002 agent manifest re-exposure
- [ ] Layer direction verified (no reverse deps per
      [[cipherocto-design-principles]])
- [ ] Cargo clippy -p octo-cli --all-targets --features full -- -D warnings
      clean
- [ ] Cargo test -p octo-cli --lib --tests green
- [ ] No new INVALID cites introduced (Guard 2 cite validator PASS)

### Type Coverage

| RFC-0011-g type               | Sub-step                  | Notes                                                                                                                                          |
| ----------------------------- | ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `SnapshotRef`                 | Sub-step 1 (output types) | Layer B/C; `[ADD]` struct per RFC-0011-g §Snapshot Ref (`snapshot_id`, `chain_id`, `taken_at_unix`, `expires_at_unix`, `root_manifest_hash`)            |
| `SnapshotOutput`              | Sub-step 1 (output types) | Layer C/D; CLI-output wrapper (`snapshot`, `open_proposals`, `attestation_count`, `resolved_at_unix`, `remaining_seconds`) per RFC-0011-g §Output Envelope   |
| `ProposalSummary`             | Sub-step 1 (output types) | Layer B/C; substrate return type for `open_proposals` per RFC-0011-g §Output Envelope (`proposal_id`, `chain_id`, `state`, `deadline_unix`, quorum fields) |
| `OctoCliError::SnapshotStale` | Sub-step 3 (errors)       | Layer C/D; `[ADD]` enum variant per RFC-0011-g §Error Handling (`exit_code = 35`)                                                                         |
| `OctoGovernanceSnapshotCache` | Sub-step 2 (cache)        | Layer C/D; NEW struct in `crates/octo-governance/src/cache.rs` (LRU + 600s TTL)                                                                |

### Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Governance
Subcommands (Phase 8 governance chapter) for Rust snippets + clap wiring
patterns.

## Pull Request

# (PR opened after mission claim transitions to Claimed per BLUEPRINT.md §Mission Lifecycle)

## Risk

- Substrate governance envelope shape evolves between RFC-0855 acceptance and
  CLI substrate additions. Mitigation: substrate pins to a major-versioned
  `octo-governance` per RFC-0011-g §Implicit Assumptions Audit.
- `OctoGovernanceSnapshotCache` TTL drift (stale snapshot shown without
  operator awareness). Mitigation: substrate TTL pinned at 600s; CLI surfaces
  `remaining_seconds` in `SnapshotOutput` per RFC-0011-g §Performance Targets.
- Operator runs `snapshot` before Phase 2 prereqs (`RFC-0855p-d`,
  `RFC-0855p-e`) land; snapshot itself is unblocked at Phase 1 per
  RFC-0011-g §Implementation Phases. Mitigation: clap tree registers only
  `snapshot` until Phase 2 mission (`0011-g-governance-attest-vote`)
  transitions to Claimed.

## Notes

`snapshot` is the only subcommand in RFC-0011-g that is **unblocked** at
Draft time; `attest` and `vote` are Phase 2 per the partial-prereq caveat
(RFC-0855p-d + RFC-0855p-e Draft). Per RFC-0011-g §Implementation Phases
Phasing Rationale: Phase 1 ships on RFC-0011-g acceptance alone; Phase 2
waits for both Draft RFCs to land.

This mission is the canonical Phase 1 mission referenced by
RFC-0011-g §Implementation Phases Implementation roadmap Phase 1 row and
§Key Files to Modify DOC-ONLY Phase 1 mission entry.

## Scope

Land `octo governance snapshot` per RFC-0011-g §Subcommand Taxonomy Subcommand Taxonomy
(full Phase 1 scope per RFC-0011-g §Implementation Phases).

## Sub-steps

1. **Output types** — `SnapshotRef`, `ProposalSummary`, `SnapshotOutput`
   wired into `crates/octo-cli/src/commands/governance.rs`. Inherits
   `OutputEnvelope<T>` from `0011-core-output-envelope-redaction`.

2. **Cache layer** — `OctoGovernanceSnapshotCache` (LRU + 600s TTL) lives
   in `crates/octo-governance/src/cache.rs`; cache key
   `(chain_id, operator_did)` per RFC-0011-g §Subcommand Taxonomy.

3. **Errors** — `OctoCliError::SnapshotStale { snapshot_id, age_secs }`
   variant added (exit 35) per RFC-0011-g §Error Handling. `VoteRejected`,
   `UnknownAttestationKind`, `PrereqNotAccepted` deferred to Phase 2.

4. **Clap wiring** — register `octo governance snapshot` with the four
   flags (`--chain-id`, `--proposal-filter`, `--force-refresh`, `--json`).

5. **Redaction** — apply RFC-0011 §Redaction Layer patterns to
   `SnapshotOutput` (no fields redacted; redaction parity per
   RFC-0011-g §Redaction).

6. **Substrate addition** — `[ADD]` `octo_governance::snapshot` in
   `crates/octo-governance/src/lib.rs` per RFC-0011-g §Subcommand Taxonomy signature
   (Layer B/C substrate `[ADD]`).

### Cargo deps

```toml
# (octo-cli/Cargo.toml [ADD])
# Governance substrate — Layer B/C extension per RFC-0011-g §Substrate [ADD]
octo-governance = { path = "../octo-governance" }
```

## Test Vectors (per RFC-0011-g §Test Vectors — snapshot + envelope)

At least 6 test vectors covering the snapshot subcommand + envelope schema
parity:

- `tv_gov_s1_cache_hit` — no flags; cache-hit returns `SnapshotOutput` with
  `remaining_seconds ~= 600` (TV-GOV-S1 per RFC-0011-g §Test Vectors) — NEW
- `tv_gov_s2_force_refresh` — `--force-refresh`; `cache_hit: false`, fresh
  `snapshot_id`, `remaining_seconds: 600` (TV-GOV-S2 per RFC-0011-g
  §Test Vectors) — NEW
- `tv_gov_s3_chain_filter` — `--chain-id chain-a --proposal-filter Open`;
  filter applied substrate-side (TV-GOV-S3 per RFC-0011-g §Test Vectors)
  — NEW
- `tv_gov_s4_json_tty_override` — `--json` on TTY; single-line JSON
  envelope per RFC-0011-g §Output Envelope (TV-GOV-S4 per RFC-0011-g §Test Vectors)
  — NEW
- `tv_gov_env15_schema_version` — any subcommand + `--json`; envelope
  `schema_version: 6` (TV-GOV-ENV15 per RFC-0011-g §Test Vectors) — NEW
- `tv_gov_env16_invalid_filter` — `--proposal-filter invalid`;
  `OctoCliError::Internal` (exit 64) (TV-GOV-ENV16 per RFC-0011-g
  §Test Vectors) — NEW

Snapshot + envelope coverage = 6 tests (RFC minimum: ≥20 spread across
Phase 1 + Phase 2; Phase 2 carries the remaining 14 vectors per
`0011-g-governance-attest-vote` §Test Vectors).

## Layer direction (per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — new `GovernanceAction::Snapshot` dispatch +
  `SnapshotOutput` envelope wrapper + `SnapshotRef` re-exposure +
  clap flag parsers + redactor pass
- `octo-governance` (Layer B/C) — substrate `[ADD]`
  `octo_governance::snapshot` + `OctoGovernanceSnapshotCache` (LRU + 600s
  TTL) + `SnapshotRef` projection
- `octo-policy` / `octo-cap-macaroon` / `octo-wallet` — unchanged; CLI
  reuses existing primitives per RFC-0011-g §Subcommand Taxonomy

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-cli --all-targets --features full -- -D warnings
cargo test -p octo-cli --lib --tests
# Manual smoke:
octo governance snapshot --json | jq '.payload.snapshot.remaining_seconds'
octo governance snapshot --force-refresh
```

## Backward compat

- **Additive only.** New subcommand under existing `octo-cli` binary;
  existing commands (`octo whoami`, `octo identity show`, `octo vault list`,
  etc.) unaffected.
- `OutputEnvelope<T>` gains one new `T` payload type (`SnapshotOutput`); the
  envelope structure is unchanged per RFC-0011 §Output Envelope.
- `OctoCliError` is `#[non_exhaustive]` (per RFC-0011); one new variant
  added (`SnapshotStale`).
- `OutputEnvelope<T>::schema_version = 6` for RFC-0011-g Phase 1.

## Cross-references

- RFC-0011-g §Subcommand Taxonomy — `octo governance snapshot`
- RFC-0011-g §Snapshot Ref — `SnapshotRef` content-addressed structure
- RFC-0011-g §Implementation Phases Phase 1
- RFC-0011-g §Key Files to Modify DOC-ONLY Phase 1 mission entry
- RFC-0011 — parent CLI substrate (`OutputEnvelope<T>`, `OctoCliError`,
  `OctoCliRedactor`, clap tree)
- RFC-0855 — Mission Overlay Networks (governance envelope substrate;
  attestation-kind registry; proposal lifecycle substrate)
- RFC-0855p-b — Coordinator Lifecycle (peer-as-coordinator gating
  context)
- RFC-0855p-c — Domain Coordinator Role (vote-weight derivation
  context for Phase 2)
- RFC-0010 — Canonical DID Codec (chain-ID canonical form + canonical
  DID wire form)
- RFC-0002 — Agent Manifest (agent-side consumption of snapshot fields)
- [[cipherocto-design-principles]] — Layer A/B/C/D/E stability contract

## Why 1 release cycle gate (Phase 1)

N/A — Phase 1 ships on RFC-0011-g acceptance alone per RFC-0011-g
§Implementation Phases Phasing Rationale. All required substrate RFCs
(RFC-0011, RFC-0855, RFC-0855p-b, RFC-0855p-c, RFC-0010) are Accepted
at RFC-0011-g filing time; no additional gating applies.

## Claimant

@unassigned
