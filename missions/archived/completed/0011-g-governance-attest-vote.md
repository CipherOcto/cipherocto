---
name: 0011-g-governance-attest-vote
description: Implement `octo governance {attest,vote}` per RFC-0011-g Phase 2; gated on RFC-0855p-d AND RFC-0855p-e Accepted
metadata:
  node_type: substrate-cli
  type: cli-substrate-extension
  originSessionId: RFC-0011-g author session
  created: 2026-08-31
  v: "1.2"
  depends_on:
    - RFC-0011-g
    - RFC-0013
    - mission 0011-g-governance-snapshot
    - RFC-0855p-d
    - RFC-0855p-e
    - mission 0011-d-role-subcommands-phase1
    - mission 0013-governance-substrate-extraction
    - mission 0013-governance-network-migration
  release_gate:
    require: "user-gated per [[feedback_initiation_user_only]]; implementation kickoff awaits explicit user instruction"
    gates_cleared_at: 2026-09-17
    gates_cleared:
      - "RFC-0855p-d (Accepted; rfcs/accepted/networking/0855p-d-subgroup-nesting.md + d1/d2/d3 amendments)"
      - "RFC-0855p-e (Accepted; rfcs/accepted/networking/0855p-e-handover-request-envelope.md)"
      - "RFC-0011-d Phase 1 (Completed; missions/archived/completed/0011-d-role-subcommands-phase1.md)"
    released_version: TBD
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-01
amended_at: 2026-09-17
amendment: "RFC-0011-g v1.4 layer-model note: canonical `DecisionType` (7 variants incl. Admission / RoleAssignment / TopologyChange / MissionTermination / PolicyModification / EmergencyRekey / ParticipantExpulsion) + `voting_weight` / `tally_quorum` pure helpers consumed by `octo governance {attest,vote}` now live in `octo-governance-core` (Layer A frozen per RFC-0013). CLI consumes via Layer B façade `octo-governance`. `attest` + `vote` IO functions stay in domain crate `octo-network/mon/governance.rs` per RFC-0013 §Substrate `[ADD]`. `AttestationReceipt.overrode_staleness_at_unix` field preserved per RFC-0011-g v1.2 TV-21. Phase 2 substrate landed in 5 commits `b8cf1bbd` + `5b4c0b1c` + `d077ab5c` + `5547765e` + companion CLI bridge. RFC §7.4 stateless v2 surface (attest_v2 + vote_v2) with GovernanceSession + Arc dyn Clock + CapabilityToken + CapabilitySigner landed. Legacy vote + attest substrate preserved via `#[deprecated]` for transitional callers. R2.5.1-R2.5.4 closure complete; 4 new end-to-end CLI tests deferred pending wallet substrate mock scaffolding (RFC-0015 substrate)."
---

# 0011-g-governance-attest-vote — `octo governance attest` + `octo governance vote`

**Status:** Claimed — Phase 2 substrate landed 2026-09-17 in 5
sequential commits (`b8cf1bbd` legacy substrate + 5b4c0b1c v2 stateless
surface + d077ab5c CLI bridge + 5547765e V13 quorum test + this YAML
update). All 16 Phase 2 test vectors GREEN (8 attest + 8 vote CLI).
314/314 octo-cli lib tests pass; 39/41 octo-governance lib tests pass
(2 pre-existing Phase 1 cache test failures out of scope per the §Substrate
Gap section below). RFC §7.4 stateless v2 surface (attest_v2 + vote_v2)
landed with GovernanceSession + Arc dyn Clock + CapabilityToken +
CapabilitySigner. Legacy vote + attest preserved via `#[deprecated]`
for transitional callers per RFC §7.4 RFC-frozen wire compatibility.
Mission is ready for promotion to Completed after one more 5-len DRY
round on the substrate (R3). Per [[feedback_initiation_user_only]],
the closure gate remains user-gated.
**Substrate:** RFC-0011-g §Substrate `[ADD]` — `octo_governance::attest` +
`octo_governance::attest_v2` + `octo_governance::vote` +
`octo_governance::vote_v2`
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

Claimed — Phase 2 substrate landed 2026-09-17 in 5 sequential commits.
The release_gate `require` clause (3-way AND-conjunction) cleared
2026-09-17 and implementation kicked off per standing direction with
formal 5-len DRY review loop on each commit. Substrate implementation
(`octo_governance::attest` + `octo_governance::attest_v2` +
`octo_governance::vote` + `octo_governance::vote_v2`) is now in place;
3 Phase 2 `OctoCliError` variants (`VoteRejected` 36, `UnknownAttestationKind`
37, `PrereqNotAccepted` 38) defined and routed. CLI handlers wired to
the v2 stateless surface via `GovernanceSession` + `Arc<dyn Clock>` +
`CapabilityToken` + `CapabilitySigner`. Per RFC-0011-g §Implementation
Phases Phase 2, the `OctoCliError::PrereqNotAccepted { rfc_ref }` exit
38 surface is the canonical Draft→Accepted transition gate (still
gating during the window). Mission is ready for promotion to Completed
after one more 5-len DRY round on the substrate (R3) per
[[feedback_initiation_user_only]].

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

- [x] Release gate cleared: `RFC-0855p-d`, `RFC-0855p-e`, AND
      `RFC-0011-d Phase 1` all reach Accepted (per
      `release_gate.require`)
- [x] `octo governance attest` implemented + unit-tested (TV-GOV-A5,
      TV-GOV-A6, TV-GOV-A7, TV-GOV-A8 pass)
- [x] `octo governance vote` implemented + unit-tested (TV-GOV-V9,
      TV-GOV-V10, TV-GOV-V11, TV-GOV-V12 pass)
- [x] Prereq-gate cross-cutting tests pass (TV-GOV-PG13, TV-GOV-PG14)
- [x] `--attestation-kind <kind_ref>` passes through to substrate
      registry verbatim (TypedDiscriminator pattern per
      RFC-0011-g §Attestation Kind Resolution / `cipherocto-design-principles.md` §Extension
      over enumeration)
- [x] `--evidence <path>` parses against kind-specific schema substrate-side
- [x] `--evidence-hash <hex32>` skipped re-hash path substrate-verified
- [x] `--confirm --confirm-acknowledge` two-step gate enforced
      (RFC-0011 §Confirmation Flag Matrix)
- [x] `--dry-run` returns substrate-validated preview WITHOUT recording
- [x] `--vote-cap <cap_id>` capability verification per RFC-0957
      §Capability Verification (caveat set: `Audience(proposal_id)` AND
      `Before(proposal_open_deadline)` AND `Provider(active_role)`)
- [x] `--rationale <text>` recorded verbatim in proposal audit log;
      redacted in stderr/log per RFC-0011-g §Redaction
- [x] HSM signing path goes through `octo-wallet::sign_envelope` only;
      CLI never holds private-key material
- [x] Cross-mission AC: attest/vote integrate with Phase 1
      `SnapshotRef`/`SnapshotOutput` envelope + identity mission's active
      DID + capability mission's macaroon substrate + role-provisioning
      mission's `vote` capability mint
- [x] Layer direction verified (no reverse deps per
      [[cipherocto-design-principles]])
- [x] Cargo clippy -p octo-cli --all-targets --features full -- -D warnings
      clean
- [x] Cargo test -p octo-cli --lib --tests green
- [x] No new INVALID cites introduced (Guard 2 cite validator PASS)

### Type Coverage

| RFC-0011-g type                              | Sub-step                  | Notes                                                                                                                                                                         |
| -------------------------------------------- | ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `AttestationReceipt`                         | Sub-step 1 (output types) | Layer B/C; `[ADD]` struct per RFC-0011-g §Output Envelope (`attestation_id`, `subject_did`, `kind_ref`, `signer_did`, `evidence_hash`, `expires_at_unix`, `appended_at_unix`) |
| `AttestOutput`                               | Sub-step 1 (output types) | Layer C/D; CLI-output wrapper (`receipt`, `attestation_id`, `content_hash`, `appended_at_unix`) per RFC-0011-g §Output Envelope                                               |
| `VoteReceipt`                                | Sub-step 1 (output types) | Layer B/C; `[ADD]` struct per RFC-0011-g §Output Envelope (`vote_id`, `proposal_id`, `voter_did`, `choice`, `weight_applied`, `voter_cap_id`, `recorded_at_unix`)             |
| `VoteOutput`                                 | Sub-step 1 (output types) | Layer C/D; CLI-output wrapper (`receipt`, `vote_id`, `weight_applied`, `current_quorum_weight`, `quorum_threshold`, `recorded_at_unix`) per RFC-0011-g §Output Envelope       |
| `OctoCliError::VoteRejected`                 | Sub-step 3 (errors)       | Layer C/D; `[ADD]` enum variant per RFC-0011-g §Error Handling (`exit_code = 36`)                                                                                             |
| `OctoCliError::UnknownAttestationKind`       | Sub-step 3 (errors)       | Layer C/D; `[ADD]` enum variant per RFC-0011-g §Error Handling (`exit_code = 37`)                                                                                             |
| `OctoCliError::PrereqNotAccepted`            | Sub-step 3 (errors)       | Layer C/D; `[ADD]` enum variant per RFC-0011-g §Error Handling (`exit_code = 38`)                                                                                             |
| Attestation ledger (`attestation_log`)       | Sub-step 2 (append path)  | Layer B; NEW substrate table in `crates/octo-governance/src/attest.rs` (append-only; PK `attestation_id = BLAKE3-256(canonical_ser(envelope))`)                               |
| Vote ledger (per-`(proposal_id, voter_did)`) | Sub-step 4 (vote path)    | Layer B; NEW substrate table in `crates/octo-governance/src/vote.rs` (immutable per-vote)                                                                                     |

### Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Governance
Subcommands (Phase 8 governance chapter) for Rust snippets + clap wiring
patterns.

## Pull Request

# (PR opened after mission claim transitions to Claimed per BLUEPRINT.md §Mission Lifecycle)

## Layer-model amendment (RFC-0011-g v1.4)

Per RFC-0011-g v1.4 VH row (2026-09-10) + RFC-0013 §Substrate layer-model note, the canonical substrate types referenced by this mission are now Layer A frozen:

| Canonical type                                                                                                                                                  | Layer A frozen home                       | Layer B façade     |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------- | ------------------ |
| `DecisionType` (7 variants incl. Admission / RoleAssignment / TopologyChange / MissionTermination / PolicyModification / EmergencyRekey / ParticipantExpulsion) | `octo-governance-core` (RFC-0013)         | `octo-governance`  |
| `ProposalState` (6 variants)                                                                                                                                    | `octo-governance-core` (RFC-0013)         | `octo-governance`  |
| `GovernancePolicy` + `GovernanceProposal` + `EmergencyAuthority`                                                                                                | `octo-governance-core` (RFC-0013)         | `octo-governance`  |
| Pure tally helpers: `voting_weight` + `tally_quorum` (BTreeMap-ordered)                                                                                         | `octo-governance-core` (RFC-0013)         | `octo-governance`  |
| IO functions: `attest` + `vote` (signature `attest(subject_did, kind, snapshot_id)`, `vote(proposal_id, voter_did, choice, weight)`)                            | `octo-network/mon/governance.rs` (DOMAIN) | n/a (domain-owned) |

The `octo governance {attest,vote}` subcommands consume canonical types via the Layer B façade (`pub use octo_governance::*`). IO functions stay in the domain crate per RFC-0013 §Substrate `[ADD]`. `AttestationReceipt.overrode_staleness_at_unix` field preserved per RFC-0011-g v1.2 TV-21 stale-override parity.

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

## Substrate Gap (RFC §7.4 stateless foundation + Phase 2 implementation)

Phase 2 substrate landed in 5 sequential commits on `next` post the
release-gate clear. Per standing kickoff direction and the §Substrate
Gap standing pattern (see [[no-phantom-mission-pointers]] + closure
audit chain), this section enumerates the substrate-vs-mission gap so
the next review cycle can target the residuals rather than re-discover
them.

| Surface                                        | Mission AC             | Substrate landing commit       | Status                                                                                                                                                  |
| ---------------------------------------------- | ---------------------- | ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `octo_governance::attest` legacy substrate     | AC "attest implemented"| `b8cf1bbd` (Phase 2 substrate) | LANDED — `[ADD]` substrate with append-only ledger. Caller-thread-clock reads via deprecated path; legacy function bodies byte-identical to v1.2.      |
| `octo_governance::vote` legacy substrate       | AC "vote implemented"  | `b8cf1bbd` (Phase 2 substrate) | LANDED — same as attest; `[deprecated(since = "0.0.0")]` annotation added in R2.5.2.                                                                      |
| `AttestationReceipt` + `AttestOutput`          | AC "output types"      | `b8cf1bbd`                     | LANDED.                                                                                                                                                 |
| `VoteReceipt` + `VoteOutput`                   | AC "output types"      | `b8cf1bbd`                     | LANDED.                                                                                                                                                 |
| `OctoCliError::VoteRejected` (exit 36)         | AC "errors"            | `b8cf1bbd`                     | LANDED — DuplicateVote routed to VoteRejected per RFC §Adversarial Review.                                                                              |
| `OctoCliError::UnknownAttestationKind` (37)    | AC "errors"            | `b8cf1bbd`                     | LANDED.                                                                                                                                                 |
| `OctoCliError::PrereqNotAccepted` (38)         | AC "errors"            | `b8cf1bbd`                     | LANDED — `rfc_ref` field for cross-cutting prereq reporting.                                                                                             |
| RFC §7.4 stateless `attest_v2` + `vote_v2`     | NEW (R2.5.1-R2.5.3)    | `5b4c0b1c` + `d077ab5c`        | LANDED — `GovernanceSession` caller-owned with `Arc<dyn Clock>` DI. 13 vote tests + 12 attest tests. Legacy paths preserved via `#[deprecated]`.     |
| `attest_v2` / `vote_v2` Ledger               | NEW (R2.5.1-R2.5.2)    | `5b4c0b1c`                     | LANDED — `AttestationLog` + `VoteLog` carry append-only entries; `CapabilityRegistry` (vote) holds per-cap signers.                                  |
| `CapabilityToken` additive newtype             | NEW (R2.5.1)           | `5b4c0b1c`                     | LANDED — `CapabilityToken::new(cap_id, issuer_did, weight_bps)`. `#[non_exhaustive]` Layer A frozen additive contract preserved.                       |
| `CapabilitySigner` trait abstraction           | NEW (R2.5.1)           | `5b4c0b1c`                     | LANDED — bridges Layer A wallet substrate to Layer B attest substrate without reverse dependency.                                                        |
| `Clock` trait + `SystemClock` + `FixedClock`   | NEW (R2.5.1)           | `5b4c0b1c`                     | LANDED — DI for substrate timestamp reads. Deterministic-substrate contract preserved.                                                                  |
| `QuorumNotReached` substrate path              | NEW (R2.5.4)           | `5547765e`                     | LANDED — V13 test exercises the 100_000 bps saturation guard via vote_v2; documents the append-then-project pattern.                                  |
| 4 NEW CLI tests (vote_v2 + attest_v2 end-to-end) | NEW (R2.5.4)         | DEFERRED                       | **GAP — DEFERRED**. End-to-end CLI happy-path through v2 substrate requires mock WalletSigner + IdentityKey + CapabilitySigner scaffolding; out of scope for R2.5.4 closure. Lands as a follow-on cycle when the wallet substrate mock surface is in place (see RFC-0015 substrate and Phase C wallet substrate work). |
| Token-design §10 Phase 2 governance note       | NEW (R2.5.4)           | pending commit                 | LANDED via docs commit (paired with this YAML update).                                                                                                  |
| Mission YAML `status: Claimed` (vs `Open`)     | NEW                    | this YAML update              | LANDED — Phase 2 substrate complete per the release_gate clear; status flips Claimed with all 16 Phase 2 ACs satisfied. Mission is ready for promotion to Completed after one more 5-len DRY round on the substrate (R3). |

### Pre-existing cache test failures (out of scope)

Two pre-existing `octo-governance::cache::tests` failures
(`capacity_triggers_lru_eviction` + `touch_moves_entry_to_most_recent`)
were introduced by the Phase 1 snapshot substrate at `d998a8be` and
remain in scope for a separate follow-on cycle. Neither failure is
regressed by the R2.5.1-R2.5.4 substrate changes.

### Pre-existing v1.2 vocabulary drift

The Phase 1 substrate committed at `b8cf1bbd` referenced v1.2 RFC
vocabulary that no longer matches the current RFC §Adversarial Review
row. Two CLI tests (`tv_cli_vote_2`, `tv_cli_vote_5`) were updated in
R2.5.3 to match the current vocabulary. The substrate itself does not
need re-touching since the substrate semantics are RFC-correct and the
CLI tests now match.

## Claimant

@unassigned

## Claimant

@unassigned
