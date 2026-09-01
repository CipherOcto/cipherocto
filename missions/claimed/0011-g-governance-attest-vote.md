---
name: 0011-g-governance-attest-vote
description: Implement `octo governance {attest,vote}` per RFC-0011-g Phase 2; gated on RFC-0855p-d AND RFC-0855p-e Accepted
metadata:
  node_type: substrate-cli
  type: cli-substrate-extension
  originSessionId: RFC-0011-g author session
  created: 2026-08-31
  v: "1.0"
  depends_on:
    - RFC-0011-g
    - mission 0011-g-governance-snapshot
    - RFC-0855p-d
    - RFC-0855p-e
    - mission 0011-d-role-subcommands-phase1
  release_gate:
    require: "RFC-0855p-d AND RFC-0855p-e AND RFC-0011-d Phase 1 reach Accepted"
    released_version: TBD
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-01
---

# 0011-g-governance-attest-vote — `octo governance attest` + `octo governance vote`

**Status:** Open — release-gated on the conjunction of three prerequisite
landings: `RFC-0855p-d`, `RFC-0855p-e`, and the Phase 1
landing of `RFC-0011-d` (role provisioning). Per RFC-0011-g §Compatibility
Mixed-Version Compatibility (Partial Prereqs), the CLI surfaces
`OctoCliError::PrereqNotAccepted { rfc_ref }` (exit 38) for any
`attest` or `vote` invocation until the gate clears. Implementation kickoff
user-gated per [[feedback_initiation_user_only]] + [[git-workflow]] once
ALL three prereqs reach Accepted.
**Substrate:** RFC-0011-g §Substrate `[ADD]` — `octo_governance::attest` +
`octo_governance::vote`
**Parent:** RFC-0011-g
**Depends on:**

- Mission `0011-g-governance-snapshot` — Phase 1 substrate (`SnapshotRef`
  - `OctoGovernanceSnapshotCache` + `SnapshotOutput`) must land first
- `RFC-0855p-d` — sub-group nesting; gates sub-group attestation
  per RFC-0011-g §Subcommand Taxonomy (PREREQ GATE)
- `RFC-0855p-e` — handover request envelope; gates vote quorum
  mechanics per RFC-0011-g §Subcommand Taxonomy (PREREQ GATE)
- Mission `0011-d-role-subcommands-phase1` — RFC-0011-d Phase 1 role
  provisioning; `vote` requires a provisioned `vote` capability per
  RFC-0011-d role provisioning
  **Blocks:** none (terminal mission for RFC-0011-g amendment chain)

## Status

Open — release-gated. The release_gate `require` clause is a 3-way
conjunction: `RFC-0855p-d AND RFC-0855p-e AND RFC-0011-d Phase 1 reach
Accepted`. Until ALL three prereqs land, the CLI surfaces
`OctoCliError::PrereqNotAccepted` (exit 38) for `attest` / `vote`
invocations per RFC-0011-g §Implementation Phases Phase 2. The clap
registration of `attest` / `vote` is deferred until the gate clears.

## RFC

RFC-0011-g §Subcommand Taxonomy — `octo governance attest` +
`octo governance vote` (rfcs/draft/process/0011-g-governance-subcommands.md)

## Dependencies

See YAML frontmatter `depends_on` block above AND `release_gate.require`
clause. Hard sequencing per RFC-0011-g §Implementation Phases:

- Phase 1 (`snapshot`) lands first → `0011-g-governance-snapshot` must
  transition to Completed before this mission transitions to Claimed
- Phase 2 (`attest` + `vote`) waits for `RFC-0855p-d` AND `RFC-0855p-e`
  to reach Accepted (per RFC-0011-g §Compatibility Mixed-Version
  Compatibility)
- `vote` requires a provisioned `vote` capability per RFC-0011-d
  Phase 1 — `0011-d-role-subcommands-phase1` must reach Completed

## Acceptance Criteria

- [ ] Release gate cleared: `RFC-0855p-d`, `RFC-0855p-e`, AND
      `RFC-0011-d Phase 1` all reach Accepted (per
      `release_gate.require`)
- [ ] `octo governance attest` implemented + unit-tested (TV-GOV-A5,
      TV-GOV-A6, TV-GOV-A7, TV-GOV-A8 pass)
- [ ] `octo governance vote` implemented + unit-tested (TV-GOV-V9,
      TV-GOV-V10, TV-GOV-V11, TV-GOV-V12 pass)
- [ ] Prereq-gate cross-cutting tests pass (TV-GOV-PG13, TV-GOV-PG14)
- [ ] `--attestation-kind <kind_ref>` passes through to substrate
      registry verbatim (TypedDiscriminator pattern per
      RFC-0011-g §Attestation Kind Resolution / `cipherocto-design-principles.md` §Extension
      over enumeration)
- [ ] `--evidence <path>` parses against kind-specific schema substrate-side
- [ ] `--evidence-hash <hex32>` skipped re-hash path substrate-verified
- [ ] `--confirm --confirm-acknowledge` two-step gate enforced
      (RFC-0011 §Confirmation Flag Matrix)
- [ ] `--dry-run` returns substrate-validated preview WITHOUT recording
- [ ] `--vote-cap <cap_id>` capability verification per RFC-0957
      §Capability Verification (caveat set: `Audience(proposal_id)` AND
      `Before(proposal_open_deadline)` AND `Provider(active_role)`)
- [ ] `--rationale <text>` recorded verbatim in proposal audit log;
      redacted in stderr/log per RFC-0011-g §Redaction
- [ ] HSM signing path goes through `octo-wallet::sign_envelope` only;
      CLI never holds private-key material
- [ ] Cross-mission AC: attest/vote integrate with Phase 1
      `SnapshotRef`/`SnapshotOutput` envelope + identity mission's active
      DID + capability mission's macaroon substrate + role-provisioning
      mission's `vote` capability mint
- [ ] Layer direction verified (no reverse deps per
      [[cipherocto-design-principles]])
- [ ] Cargo clippy -p octo-cli --all-targets --features full -- -D warnings
      clean
- [ ] Cargo test -p octo-cli --lib --tests green
- [ ] No new INVALID cites introduced (Guard 2 cite validator PASS)

### Type Coverage

| RFC-0011-g type                              | Sub-step                  | Notes                                                                                                                                                             |
| -------------------------------------------- | ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `AttestationReceipt`                         | Sub-step 1 (output types) | Layer B/C; `[ADD]` struct per RFC-0011-g §Output Envelope (`attestation_id`, `subject_did`, `kind_ref`, `signer_did`, `evidence_hash`, `expires_at_unix`, `appended_at_unix`) |
| `AttestOutput`                               | Sub-step 1 (output types) | Layer C/D; CLI-output wrapper (`receipt`, `attestation_id`, `content_hash`, `appended_at_unix`) per RFC-0011-g §Output Envelope                                               |
| `VoteReceipt`                                | Sub-step 1 (output types) | Layer B/C; `[ADD]` struct per RFC-0011-g §Output Envelope (`vote_id`, `proposal_id`, `voter_did`, `choice`, `weight_applied`, `voter_cap_id`, `recorded_at_unix`)             |
| `VoteOutput`                                 | Sub-step 1 (output types) | Layer C/D; CLI-output wrapper (`receipt`, `vote_id`, `weight_applied`, `current_quorum_weight`, `quorum_threshold`, `recorded_at_unix`) per RFC-0011-g §Output Envelope       |
| `OctoCliError::VoteRejected`                 | Sub-step 3 (errors)       | Layer C/D; `[ADD]` enum variant per RFC-0011-g §Error Handling (`exit_code = 36`)                                                                                            |
| `OctoCliError::UnknownAttestationKind`       | Sub-step 3 (errors)       | Layer C/D; `[ADD]` enum variant per RFC-0011-g §Error Handling (`exit_code = 37`)                                                                                            |
| `OctoCliError::PrereqNotAccepted`            | Sub-step 3 (errors)       | Layer C/D; `[ADD]` enum variant per RFC-0011-g §Error Handling (`exit_code = 38`)                                                                                            |
| Attestation ledger (`attestation_log`)       | Sub-step 2 (append path)  | Layer B; NEW substrate table in `crates/octo-governance/src/attest.rs` (append-only; PK `attestation_id = BLAKE3-256(canonical_ser(envelope))`)                   |
| Vote ledger (per-`(proposal_id, voter_did)`) | Sub-step 4 (vote path)    | Layer B; NEW substrate table in `crates/octo-governance/src/vote.rs` (immutable per-vote)                                                                         |

### Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Governance
Subcommands (Phase 8 governance chapter) for Rust snippets + clap wiring
patterns.

## Pull Request

# (PR opened after mission claim transitions to Claimed per BLUEPRINT.md §Mission Lifecycle)

## Risk

- **Partial prereq drift.** `RFC-0855p-d` and `RFC-0855p-e` may land in
  different release cycles. Mitigation: `release_gate.require` is a
  3-way conjunction; the gate does NOT clear on partial prereq landing.
- **Capability gating regression.** A `vote` capability minted before
  `RFC-0011-d Phase 1` reaches Accepted could lack the `Audience` caveat
  per RFC-0957 §Attenuation Invariant. Mitigation: substrate verifies
  caveat set per RFC-0957; CLI surfaces `VoteRejected` (exit 36) on
  insufficient capability.
- **Attestation history rewriting.** Substrate compromise could enable
  update / delete paths. Mitigation: substrate enforces append-only at
  the storage layer; CLI surfaces no update / delete flags per
  RFC-0011-g §Adversarial Review HIGH row.
- **Sybil attestation flooding.** Non-coordinator DIDs could flood the
  attestation ledger. Mitigation: substrate enforces RFC-0855p-c
  `SignerNotAuthorized`; CLI surfaces `VoteRejected` exit 36.
- **Quorum manipulation.** Quorum threshold could drift between cast
  and completion. Mitigation: substrate pins `quorum_threshold` at
  receipt time per RFC-0855 §Economic Analysis Recording; CLI surfaces
  `quorum_threshold` in `VoteOutput` for operator audit.

## Notes

Phase 2 carries the partial-prereq caveat explicitly per RFC-0011-g
§Compatibility Mixed-Version Compatibility. Until ALL three prereqs
land (`RFC-0855p-d` AND `RFC-0855p-e` AND `RFC-0011-d Phase 1`), the
clap tree does NOT register `attest` / `vote`; if a stale CLI binary
somehow dispatches the subcommand, the substrate returns
`GovernanceError::PrereqNotAccepted` and the CLI surfaces exit 38.
RFC-0011-d Phase 1 (role provisioning) is required because `vote` needs
a provisioned `vote` capability (operator invokes `octo role provision
--cap vote --proposal <proposal_id> --deadline <unix>` per RFC-0011-d
to mint; CLI then forwards the minted `cap_id` via `--vote-cap`).

## Scope

Land `octo governance attest` + `octo governance vote` per RFC-0011-g §Subcommand Taxonomy
Subcommand Taxonomy (full Phase 2 scope per RFC-0011-g §Implementation
Phases).

## Sub-steps

1. **Output types** — `AttestationReceipt`, `AttestOutput`, `VoteReceipt`,
   `VoteOutput` wired into `crates/octo-cli/src/commands/governance.rs`.
   Inherits `OutputEnvelope<T>` from `0011-core-output-envelope-redaction`.

2. **Attest substrate** — `[ADD]` `octo_governance::attest` in
   `crates/octo-governance/src/attest.rs` per RFC-0011-g §Subcommand Taxonomy signature.
   Append-only ledger `attestation_log` in the substrate's persistence
   layer (PK `attestation_id = BLAKE3-256(canonical_ser(envelope))`).

3. **Errors** — `OctoCliError::{VoteRejected, UnknownAttestationKind,
PrereqNotAccepted}` added (exit codes 36, 37, 38) per RFC-0011-g §Error Handling.
   `SnapshotStale` (exit 35) already landed in Phase 1.

4. **Vote substrate** — `[ADD]` `octo_governance::vote` in
   `crates/octo-governance/src/vote.rs` per RFC-0011-g §Subcommand Taxonomy signature.
   Capability verification per RFC-0957 §Capability Verification
   (caveat set: `Audience(proposal_id)` AND `Before(proposal_open_deadline)`
   AND `Provider(active_role)`); weight derived from RFC-0855p-c role
   stake at proposal snapshot time.

5. **Clap wiring** — register `octo governance attest <subject_did>
<attestation_kind>` and `octo governance vote <proposal_id>
<vote_choice>` (DEFERRED until release_gate clears).

6. **Redaction** — apply RFC-0011 §Redaction Layer patterns per
   RFC-0011-g §Redaction (rationale redacted in stderr/log; evidence bytes
   NEVER echoed in logs; `evidence_hash` only).

7. **Confirmation gates** — `--confirm --confirm-acknowledge` two-step
   gate per RFC-0011 §Confirmation Flag Matrix (pastejacking defense).

8. **Prereq gate integration** — substrate returns
   `GovernanceError::PrereqNotAccepted` until `RFC-0855p-d` AND
   `RFC-0855p-e` reach Accepted; CLI surfaces exit 38 (no operator
   cost; advisory).

### Cargo deps

```toml
# (octo-cli/Cargo.toml [ADD])
# RFC-0957 macaroon substrate (Layer B years-stable) — vote capability caveat gating
octo-cap-macaroon = { path = "../octo-cap-macaroon" }
```

Existing `octo-governance` and `octo-wallet` deps from Phase 1 carry over;

## No additional deps added by Phase 2.

## Test Vectors (per RFC-0011-g §Test Vectors — attest + vote + prereq)

At least 10 test vectors covering the attest subcommand + vote subcommand

- prereq-gate cross-cutting:

* `tv_gov_a5_happy_path` —
  `<subject_did:peer> route-quality:uptime-30d --evidence good.json
--confirm --confirm-acknowledge` returns `AttestOutput` with
  `attestation_id` + `content_hash` (TV-GOV-A5 per RFC-0011-g §Test
  Vectors) — NEW
* `tv_gov_a6_unknown_kind` —
  `<subject_did:peer> route-quality:unknown-subkind --confirm
--confirm-acknowledge` returns `UnknownAttestationKind` (exit 37)
  (TV-GOV-A6 per RFC-0011-g §Test Vectors) — NEW
* `tv_gov_a7_confirm_gate` —
  `<subject_did:peer> route-quality:uptime-30d` (no
  `--confirm-acknowledge`) returns `ConfirmationRequired` (exit 2)
  (TV-GOV-A7 per RFC-0011-g §Test Vectors) — NEW
* `tv_gov_a8_subgroup_prereq` —
  `<subject_did:subgroup>` (RFC-0855p-d Draft) returns
  `PrereqNotAccepted { rfc_ref: "RFC-0855p-d" }` (exit 38) (TV-GOV-A8
  per RFC-0011-g §Test Vectors) — NEW
* `tv_gov_v9_happy_path` —
  `<proposal_id> Yes --vote-cap <cap_id> --confirm
--confirm-acknowledge` returns `VoteOutput` with `weight_applied`,
  `current_quorum_weight`, `quorum_threshold` (TV-GOV-V9 per
  RFC-0011-g §Test Vectors) — NEW
* `tv_gov_v10_bad_capability` —
  `<proposal_id> Yes --vote-cap <bad_cap_id> --confirm
--confirm-acknowledge` returns `VoteRejected` (exit 36)
  (TV-GOV-V10 per RFC-0011-g §Test Vectors) — NEW
* `tv_gov_v11_confirm_gate` —
  `<proposal_id> Yes --vote-cap <cap_id>` (no `--confirm-acknowledge`)
  returns `ConfirmationRequired` (exit 2) (TV-GOV-V11 per RFC-0011-g
  §Test Vectors) — NEW
* `tv_gov_v12_vote_prereq` —
  `<proposal_id> Yes --vote-cap <cap_id> --confirm
--confirm-acknowledge` (RFC-0855p-e Draft) returns
  `PrereqNotAccepted { rfc_ref: "RFC-0855p-e" }` (exit 38)
  (TV-GOV-V12 per RFC-0011-g §Test Vectors) — NEW
* `tv_gov_pg13_subgroup_cross_cutting` —
  `attest <subgroup_did> ...` (Draft prereq scenario) returns
  `PrereqNotAccepted { rfc_ref: "RFC-0855p-d" }` (exit 38)
  (TV-GOV-PG13 per RFC-0011-g §Test Vectors) — NEW
* `tv_gov_pg14_vote_cross_cutting` —
  `vote ...` (Draft prereq scenario) returns
  `PrereqNotAccepted { rfc_ref: "RFC-0855p-e" }` (exit 38)
  (TV-GOV-PG14 per RFC-0011-g §Test Vectors) — NEW

Phase 1 + Phase 2 total = 6 + 10 = 16 test vectors (RFC minimum: ≥12
per RFC-0011-g §Test Vectors).

## Layer direction (per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — new `GovernanceAction::{Attest, Vote}`
  dispatch + 4 output structs (`AttestationReceipt`, `AttestOutput`,
  `VoteReceipt`, `VoteOutput`) + 3 new `OctoCliError` variants +
  capability gating pass + rationale redactor
- `octo-governance` (Layer B/C) — substrate `[ADD]`
  `octo_governance::attest` (with `attestation_log` append-only ledger)
  - `octo_governance::vote` (with capability verification + per-(
    `proposal_id, voter_did`) vote ledger)
- `octo-cap-macaroon` (Layer B) — `CapabilityToken` + caveat set
  verification per RFC-0957 §Capability Verification (existing primitive;
  no new substrate additions)
- `octo-wallet` (Layer A) — `sign_envelope` HSM-bound path (existing
  primitive; CLI never holds private-key material)
- `octo-policy` / identity commands — unchanged; CLI reuses existing
  primitives

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-cli --all-targets --features full -- -D warnings
cargo test -p octo-cli --lib --tests
# Prereq gate smoke (Draft state):
octo governance vote <proposal_id> Yes --vote-cap <cap_id> --confirm --confirm-acknowledge
# expect: PrereqNotAccepted { rfc_ref: "RFC-0855p-e" } exit 38
octo governance attest <subgroup_did> route-quality:uptime-30d --confirm --confirm-acknowledge
# expect: PrereqNotAccepted { rfc_ref: "RFC-0855p-d" } exit 38
```

## Backward compat

- **Additive only.** New subcommands under existing `octo-cli` binary;
  existing commands unaffected.
- `OutputEnvelope<T>` gains two new `T` payload types (`AttestOutput`,
  `VoteOutput`); the envelope structure is unchanged per RFC-0011
  §Output Envelope.
- `OctoCliError` is `#[non_exhaustive]` (per RFC-0011); three new
  variants added (`VoteRejected`, `UnknownAttestationKind`,
  `PrereqNotAccepted`). Existing variants unchanged.
- `OutputEnvelope<T>::schema_version = 1` for RFC-0011-g Phase 2.
- Prereq gate (`PrereqNotAccepted` exit 38) is the canonical surface
  during Draft; no CLI change required once `RFC-0855p-d` AND
  `RFC-0855p-e` reach Accepted.

## Cross-references

- RFC-0011-g §Subcommand Taxonomy — `octo governance attest` +
  `octo governance vote`
- RFC-0011-g §Attestation Kind Resolution — TypedDiscriminator pattern
- RFC-0011-g §Vote Capability Gating — caveat set per RFC-0957
- RFC-0011-g §Implementation Phases Phase 2 (multi-prereq gate)
- RFC-0011-g §Compatibility Mixed-Version Compatibility (Partial
  Prereqs)
- RFC-0011 — parent CLI substrate (`OutputEnvelope<T>`, `OctoCliError`,
  `OctoCliRedactor`, clap tree)
- RFC-0855 — Mission Overlay Networks (governance envelope substrate)
- RFC-0855p-b — Coordinator Lifecycle (attestation signing authority)
- RFC-0855p-c — Domain Coordinator Role (vote-weight derivation)
- RFC-0855p-d — Sub-Group Nesting (Draft; gates sub-group attestation)
- RFC-0855p-e — Handover Request Envelope (Draft; gates vote quorum)
- RFC-0010 — Canonical DID Codec
- RFC-0011-d — Role Provisioning (vote capability issuance)
- RFC-0957 — Macaroon Substrate (`Audience(proposal_id)` caveat)
- RFC-0900 — Economic Substrate (informational; dual-stake model)
- [[cipherocto-design-principles]] — Layer A/B/C/D/E stability contract

## Why 1 release cycle gate (Phase 2 multi-prereq)

Per RFC-0011-g §Compatibility Mixed-Version Compatibility (Partial
Prereqs), Phase 2 has THREE substantive prereqs (two substrate RFCs in
Draft + one parent RFC in development). The gate is a 3-way
**conjunction** (`AND`), not a disjunction (`OR`); none of the three
subcommands (`attest` sub-group targeting, full `vote`) land until
ALL three prereqs reach Accepted.

The multi-prereq gate is necessary because:

1. `attest` against sub-group DIDs depends on `RFC-0855p-d` (sub-group
   nesting); attestation against peer DIDs and vault-owner DIDs is
   unblocked at Phase 1, but the sub-group surface is gated.
2. `vote` quorum mechanics depend on `RFC-0855p-e` (handover request
   envelope); the entire subcommand is gated until the envelope
   substrate lands.
3. `vote` requires a `vote` capability minted via `RFC-0011-d`; without
   Phase 1 role provisioning, the operator cannot mint the capability
   that the CLI forwards via `--vote-cap`.

Per RFC-0011-g §Implementation Phases Phasing Rationale, gates-on-gates
are blocked until ALL prereqs reach Accepted; the substrate tracks
prereq acceptance; the CLI surfaces `PrereqNotAccepted` (exit 38)
during the Draft window. Operators receive the Phase 2 surface on
the CLI's next re-installation after the gate clears (no CLI change
required — substrate behavior flips on `RFC-0855p-d` + `RFC-0855p-e`
acceptance).

## Claimant

@unassigned
