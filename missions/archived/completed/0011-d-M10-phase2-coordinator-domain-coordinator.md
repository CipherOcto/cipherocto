---
name: 0011-d-M10-phase2-coordinator-domain-coordinator
description: Phase 2 (gated) per RFC-0011-d §Mission Decomposition M10 row: extend existing `octo role select` with `--coordinator` + `--domain-coordinator` flags + 2 substrate entrypoints (`octo_role::select_coordinator` + `octo_role::select_domain_coordinator`); gate on RFC-0855p-d + RFC-0855p-e reaching Accepted; substrate-first ordering — DEPENDS ON M11 substrate landing.
metadata:
  node_type: substrate-cli
  type: cli-subcommand-phase2
  originSessionId: RFC-0011-d author session
  created: 2026-08-31
  v: "1.4"
  completed: 2026-09-03
  commit: 96bccc4b
  depends_on:
    - RFC-0011-d
    - RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) (must be Accepted)
    - RFC-0855p-e (must be Accepted)
    - mission 0011-d-M6-octocli-role-commands
    - mission 0011-d-M11-phase2-domain-coordinator-platform-binding
status: Completed
---

# 0011-d-M10-phase2-coordinator-domain-coordinator — Phase 2 `octo role select coordinator` + `domain-coordinator` per RFC-0011-d §Mission Decomposition M10

**Status:** Open — **GATE CLEARED (2026-09-02)**. Both RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) + RFC-0855p-e reached Accepted at commit `0e915618` on `next`. Files at `rfcs/accepted/networking/0855p-d{,-*}-subgroup-*.md` + `rfcs/accepted/networking/0855p-d{1,2,3}-*.md` + `rfcs/accepted/networking/0855p-e-*.md`. Per [[cipherocto-design-principles]] §Discipline at first call site + [[no-line-refs-anywhere]], the gate-clearance was reached via per-concern RFC split (d INDEX + d1 + d2 + d3) + 5-lens adversarial review loop + DRY closure (W12+W12.6 = 2 consecutive zero-finding rounds) per `docs/audits/2026-09-02-rfc-0855p-de-review-dry.md`. Implementation kickoff user-gated per [[feedback_initiation_user_only]] + [[git-workflow]]. M11 must land FIRST per substrate-first pattern (Sub-step 2 of mission sub-steps). v1.3 drift-fix per `docs/audits/2026-09-03-m10-m11-substrate-truth-reconciliation.md` + RFC-0011-d v1.7.1 amendment: substrate signatures corrected to canonical Layer C types (`RoleBinding`, `GroupBinding`, `BindingError`); fictional `DomainCoordinatorRecord` + `DomainCoordinatorError` removed.
**Substrate:** RFC-0011-d §Mission Decomposition M10 row; §Phase 2; §Compatibility partial-prereq caveat
**Parent:** RFC-0011-d
**Depends on:** RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) Accepted + RFC-0855p-e Accepted (gate); `0011-d-M6-octocli-role-commands` (Phase 1 surface); `0011-d-M11-phase2-domain-coordinator-platform-binding` (M11 must land FIRST per substrate-first pattern)

## Status

Claimed (2026-09-02) by @mmacedoeu — **GATE CLEARED (2026-09-02) at commit `0e915618`**. RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) + RFC-0855p-e reached Accepted on `next`. Tenth of 11 atomic missions (9 Phase 1 + 2 Phase 2). Implementation kickoff user-gated per [[feedback_initiation_user_only]] + [[git-workflow]]; M11 (`0011-d-M11-phase2-domain-coordinator-platform-binding`) must land FIRST per substrate-first pattern (Sub-step 2). v1.3 drift-fix per `docs/audits/2026-09-03-m10-m11-substrate-truth-reconciliation.md` + RFC-0011-d v1.7.1 amendment.

## Substrate (RFC-0011-d)

§Mission Decomposition M10 row (canonical drift-fixed):

- "Add `octo role select coordinator` + `domain-coordinator` clap subcommands + 2 substrate entrypoints (`octo_role::select_coordinator` + `octo_role::select_domain_coordinator`); gate on RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) + RFC-0855p-e reaching Accepted"

§Compatibility partial-prereq caveat: until both prereq RFCs reach Accepted, `octo role select coordinator` and `octo role select domain-coordinator` return exit 33 (`RoleNotSelectable` per M7 variant) + prereq RFC names in error message (TV-RP-1 carries forward from M8).

§Substrate signatures (drift-fixed, canonical Layer C types):

```rust
// In crates/octo-role/src/select.rs (extend existing Phase 1 file)

// ----- Mission Coordinator -----

/// Phase 2 substrate entry. Binds operator to `coordinator` role.
/// Emits HandoverRequest envelope per RFC-0855p-e (HandoverRequest Envelope
/// & Coordinator Term Handover substrate).
///
/// **Atomic:** writes `RoleBinding` to `BindingStore` + emits
/// `HandoverRequestEnvelope` post-commit (Layer D side-effect; non-transactional).
/// On any failure: `RoleBinding` NOT persisted (atomic rollback).
pub fn select_coordinator(
    role_id: &str,
    operator_did: &str,
    signer: &dyn CapabilitySigner,
    chain_id: &ChainId,
    store: &BindingStore,
    mission_id: [u8; 32],
    current_epoch: u64,
) -> Result<RoleBinding, RoleError>;

// ----- Domain Coordinator -----

/// Phase 2 substrate entry. Binds operator to `domain-coordinator` role.
/// **Atomic with RFC-0850p-c binding ceremony**: single Stoolap
/// `BEGIN IMMEDIATE` tx covers role binding + GroupBinding state
/// transition (via M11 `bind_domain_coordinator`).
///
/// **Side effect post-commit:** `PlatformEvent::AdminTransfer` envelope
/// emission per RFC-0855p-c §5a (Layer D side-effect; non-transactional).
///
/// **Returns both updated bindings** so caller can render CLI output
/// with both canonical substrate artifacts.
pub fn select_domain_coordinator(
    role_id: &str,
    operator_did: &str,
    signer: &dyn CapabilitySigner,
    chain_id: &ChainId,
    store: &BindingStore,
    group_binding: &GroupBinding,
    platform_admin_proof: &PlatformAdminProof,
    current_epoch: u64,
) -> Result<(RoleBinding, GroupBinding), RoleError>;
```

## Parent

RFC-0011-d §Mission Decomposition M10 row; §Phase 2; §Compatibility partial-prereq caveat; §Test Vectors Phase 2 (+3 vectors beyond Phase 1).

## Depends on (GATE)

**HARD GATE** _(gate CLEARED 2026-09-02 per commit `0e915618` on `next`; retained as documentation per [[cipherocto-design-principles]] §Discipline at first call site)_: RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) + RFC-0855p-e MUST reach Accepted status before this mission claims. Per [[deferred-vs-unspecified]] + BLUEPRINT §Mission Lifecycle.

**SUBSTRATE-FIRST GATE**: M11 (`0011-d-M11-phase2-domain-coordinator-platform-binding`) MUST land before this mission begins implementation. Per RFC-0011-d §Mission Decomposition M10 row Prereq RFCs column addition (v1.7 substrate-truth reconciliation).

- `RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3)` (Sub-Domain / Sub-Group Nesting substrate) — required for `domain-coordinator` (sub-group nesting is a prereq for `domain-coordinator` per §Compatibility)
- `RFC-0855p-e` (HandoverRequest Envelope & Coordinator Term Handover substrate) — required for BOTH `coordinator` AND `domain-coordinator` role bindings
- `0011-d-M11-phase2-domain-coordinator-platform-binding` (substrate-first pattern; M11 must land BEFORE M10 per substrate-first workflow; provides `bind_domain_coordinator` substrate entrypoint + `PlatformAdminProof` typed envelope)
- `0011-d-M6-octocli-role-commands` (Phase 1 surface; Phase 2 extends with 2 new role_id args under existing `octo role select`)

## Acceptance Criteria

- [x] Extend `crates/octo-role/src/lib.rs` with 3 new public exports: `select_coordinator` + `select_domain_coordinator` + `build_handover_request` (drift-fix: `build_handover_request` added as separate public helper per [[cipherocto-design-principles]] §Interface Segregation)
- [x] Extend `crates/octo-role/src/select.rs` with 2 new substrate entrypoints per drift-fixed signatures above:
  - `pub fn select_coordinator(role_id, operator_did, signer, chain_id, store, mission_id, current_epoch) -> Result<RoleBinding, RoleError>` — calls Phase 1 `select()` for role binding + emits `HandoverRequestEnvelope` per RFC-0855p-e (Layer D side-effect; post-commit)
  - `pub fn select_domain_coordinator(role_id, operator_did, signer, chain_id, store, group_binding, platform_admin_proof, current_epoch) -> Result<(RoleBinding, GroupBinding), RoleError>` — atomic with M11 `bind_domain_coordinator` (single Stoolap BEGIN IMMEDIATE tx)
- [x] `select_coordinator` atomicity: writes `RoleBinding` to `BindingStore` + emits `HandoverRequestEnvelope` post-commit; on HandoverRequest emission failure, role binding rolls back (atomic)
- [x] `select_domain_coordinator` atomicity: single Stoolap `BEGIN IMMEDIATE` tx covers role binding (Phase 1 `select`) + GroupBinding update (M11 `bind_domain_coordinator`); on any failure, both roll back; post-commit emits `PlatformEvent::AdminTransfer` envelope (Layer D side-effect)
- [x] New error variant `RoleError::GroupBindingRejected { reason }` for M11 proof verification failures (drift-fix: existing `RoleNotSelectable` reuse abandoned in favor of typed variant per exit-code precision; exit code 36 in CLI)
- [x] Extend existing `crates/octo-cli/src/commands/role.rs` (NOT new top-level `coordinator` subcommand group per RFC §Mission Decomp M10 row):
  - Add `octo role select coordinator` clap subcommand (extends existing `Role::Select` with `--coordinator` flag + `--mission-id <hex>` + `--current-epoch <N>`)
  - Add `octo role select domain-coordinator` clap subcommand (extends existing `Role::Select` with `--domain-coordinator` flag + `--group-jid <str>` + `--platform <str>` + `--platform-admin-proof <json>` + `--mission-id <hex>` + `--current-epoch <N>`)
  - `octo role select coordinator` calls `octo_role::select_coordinator(...)` (M10 substrate)
  - `octo role select domain-coordinator` calls `octo_role::select_domain_coordinator(...)` (M10 substrate); atomic with M11 `bind_domain_coordinator` via shared Stoolap `BEGIN IMMEDIATE` tx
- [x] `--confirm` required for both (existing `require_confirm(cli, "role select")` gate)
- [x] `parse_hash32_hex` validator (canonical lowercase hex per RFC-0010 §OctoID Codec; rejects uppercase/wrong-length/non-hex)
- [x] Phase 2 test vectors pass (M10 substrate + M10 CLI):
  - `select_coordinator_binds_role_and_returns_binding`
  - `select_coordinator_emits_signed_handover_envelope_via_helper`
  - `build_handover_request_rejects_non_canonical_did`
  - `coordinator_role_for_role_id_maps_known_slugs`
  - `select_domain_coordinator_happy_path_binds_role_and_updates_group`
  - `select_domain_coordinator_rolls_back_on_stale_proof`
  - `select_domain_coordinator_rolls_back_on_invalid_transition`
  - `select_domain_coordinator_rejects_dc_pubkey_mismatch`
  - `select_domain_coordinator_propagates_signer_mismatch`
  - `deterministic_nonce32_differs_per_epoch`
  - `deterministic_nonce32_is_deterministic`
  - `map_role_error_6_variants`
  - `map_role_error_all_variants`
  - `parse_hash32_hex_round_trip`
  - `parse_hash32_hex_rejects_wrong_length`
  - `parse_hash32_hex_rejects_non_hex`
  - `parse_hash32_hex_rejects_uppercase`
  - `hex_nibble_round_trip`
- [x] 3 coordinator roles added to `crates/octo-role/src/registry.rs` (`domain-coordinator`, `mission-coordinator`, `witness-coordinator`; governance class; OCTO-only; `handover_timeout` + `double_sign` slashing rules)
- [x] `pubkey_from_did` Phase 1 helper added to `crates/octo-cap-macaroon/src/signer.rs` (inverse of `did_from_pubkey`; canonical lowercase hex; production swap point for `octo_ident::WireDid` per RFC-0010)
- [x] `cargo test -p octo-role --lib` → 33 passed
- [x] `cargo test -p octo-cli --lib commands::role` → 7 passed
- [x] `cargo test -p octo-network --lib dc::admin_attest` → 17 passed
- [x] `cargo test -p octo-cap-macaroon --lib signer::` → 19 passed
- [x] `cargo check -p octo-cli -p octo-role -p octo-network -p octo-cap-macaroon` zero warnings
- [x] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [x] **GATE CHECK**: `git log --oneline rfcs/accepted/networking/0855p-d-*.md rfcs/accepted/networking/0855p-e-*.md` shows both Accepted _(verified 2026-09-02 at commit `0e915618`)_

## Scope

Phase 2 CLI extension + substrate entrypoints. NO Phase 2 platform binding atomic-update (M11 owns `bind_domain_coordinator` substrate entrypoint; M10 wraps it atomically with role binding).

## Sub-steps

1. **VERIFY GATE** (first step; abort if not met)
2. **VERIFY M11 LANDED** (second step; substrate-first pattern)
3. Extend `crates/octo-role/src/lib.rs` with 2 new substrate entrypoint exports: `select_coordinator` + `select_domain_coordinator`
4. Extend `crates/octo-role/src/select.rs` with 2 new functions per drift-fixed signatures above
5. Wire `select_coordinator` to `octo_network::dot::handover` HandoverRequest envelope substrate (per RFC-0855p-e §Layer placement; landed commit `b0f14119`)
6. Wire `select_domain_coordinator` to M11 `octo_network::dc::admin_attest::bind_domain_coordinator` (drift-fixed substrate home; provides `PlatformAdminProof` envelope + `GroupBinding` update atomic with role binding)
7. Extend `crates/octo-cli/src/commands/role.rs` with 2 new role_id args: `coordinator` + `domain-coordinator` + `--group-jid` + `--platform` + `--platform-admin-proof`
8. Add partial-prereq guard returning exit 33 + `RoleNotSelectable` + prereq RFC names
9. Add 4 unit tests listed in Acceptance Criteria
10. Verify cargo check + clippy + fmt

## Test Vectors (Phase 2 +3 per RFC)

Per RFC-0011-d §Test Vectors Phase 2:

- TV-RP-1 (carried forward from M8): partial-prereq guard returns exit 33 + `RoleNotSelectable` + prereq RFC names in error message
- TV-RC-1: `octo role select coordinator --confirm` success path (post-0855p-e Accept); `HandoverRequestEnvelope` emitted per RFC-0855p-e substrate
- TV-RDC-1: `octo role select domain-coordinator --confirm --group-jid X --platform whatsapp --platform-admin-proof proof.json` success path (post-0855p-d + 0855p-e Accept); atomic role binding + GroupBinding update + PlatformEvent::AdminTransfer emitted
- TV-RDC-2: `octo role select domain-coordinator --confirm` with mismatched `platform_admin_proof` → `RoleError::SigningFailed { reason: "M11 proof verification failed: ..." }`; role binding NOT persisted (atomic rollback per M11 `BindingError::SignatureInvalid`)

## Layer direction (per [[cipherocto-design-principles]])

- `octo-cli` (Layer C) — thin CLI wrapper extending existing `Role::Select` (no new top-level `coordinator` group per RFC §Mission Decomp M10 row)
- `octo-role` (Layer C specialized node; per RFC-0011-d §7.4 v1.7 layer label fix) — Phase 2 substrate entrypoints `select_coordinator` + `select_domain_coordinator`
- `octo-network/src/dot/handover` (Layer C specialized node; per RFC-0855p-e §Layer placement + landed commit `b0f14119`) — HandoverRequest envelope (RFC-0855p-e substrate)
- `octo-network/src/dc/admin_attest` (Layer C specialized node; per RFC-0855p-c substrate home; drift-fixed v1.3) — M11 `bind_domain_coordinator` + `PlatformAdminProof` + `PlatformEvent::AdminTransfer` emission
- `octo-network/src/dot/binding` (Layer C specialized node; per RFC-0850p-c substrate) — `GroupBinding` + `BindingError` + `GroupRegistry::register_binding` (called by M11)

## Backward compat

Additive: 2 new role_id args + 2 new substrate entrypoints + 3 new CLI flags (`--coordinator` + `--domain-coordinator` + `--group-jid` + `--platform` + `--platform-admin-proof`). NO existing CLI commands modified (extends existing `octo role select`).

## Risk

- **Gate not met**: this mission cannot claim until RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) + RFC-0855p-e Accepted. Mitigation: explicit GATE CHECK in Sub-step 1; mission stays Open until met.
- **Substrate-first ordering**: M10 calls M11 substrate; M11 must land FIRST. Mitigation: explicit M11 LANDED check in Sub-step 2.
- **Error code drift**: `RoleNotSelectable { reason }` carries prereq RFC names; reason string format must match TV-RP-1. Mitigation: canonical reason string per RFC-0011-d §Compatibility partial-prereq caveat.
- **Atomicity boundary**: `select_domain_coordinator` MUST roll back role binding on M11 `bind_domain_coordinator` failure. Mitigation: single Stoolap `BEGIN IMMEDIATE` tx wraps both writes; explicit rollback test (TV-RDC-2).
- **Adapter signature verification**: per-adapter (WhatsApp / Matrix / Telegram). Mitigation: delegation to adapter at substrate boundary per RFC-0855p-c drift-fix B.

## Notes

- Phase 2 mission — gated
- Per [[deferred-vs-unspecified]]: deferred (not unspecified); gate enforced
- Sub-step 1 = gate check; Sub-step 2 = M11 substrate-first check
- Extends existing `octo role select` (NOT new top-level `coordinator` group per RFC §Mission Decomp M10 row)
- v1.3 drift-fix supersedes v1.2 substrate-home reconciliation: `octo_network::mon::domain_coordinator::bind_domain_coordinator` (file never existed) → `octo_network::dc::admin_attest::bind_domain_coordinator` (REAL drift-fixed substrate home; provides `PlatformAdminProof` typed envelope)
- v1.3 drift-fix supersedes v1.2 error type rename: `DomainCoordinatorError` (fictional) → canonical `BindingError` (existing at `crates/octo-network/src/dot/binding.rs:648`) for M11; M10 surface uses existing `RoleError` + new variant reuse `RoleNotSelectable`

## Cross-references

- RFC-0011-d §Mission Decomposition M10 row (canonical scope; extends `octo role select` + substrate entrypoints; v1.7.1 drift-fixed signatures)
- RFC-0011-d §Phase 2
- RFC-0011-d §Compatibility partial-prereq caveat
- RFC-0011-d §Test Vectors Phase 2 (+3 vectors beyond Phase 1; TV-RC-1 + TV-RDC-1 + TV-RDC-2)
- RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) (Sub-Domain / Sub-Group Nesting; gate)
- RFC-0855p-e (HandoverRequest Envelope; gate)
- RFC-0855p-c (`PlatformAdminAttestError` substrate; §5a `PlatformEvent::AdminTransfer` envelope; drift-fix B/C closed at commit `205f1434`)
- RFC-0850p-c (binding ceremony substrate; `GroupBinding` + `BindingError`)
- M11 (substrate-first ordering: M10 wraps M11 `bind_domain_coordinator` atomically)
- [[deferred-vs-unspecified]] — gated release
- `docs/audits/2026-09-03-m10-m11-substrate-truth-reconciliation.md` — drift-fix verdict + audit closure
- `memory/rfc-0011-d-m10-m11-substrate-impl-2026-09-03.md` — implementation closure card

## Claimant

@mmacedoeu (mission lifecycle: Claimed 2026-09-02; v1.3 drift-fix per `docs/audits/2026-09-03-m10-m11-substrate-truth-reconciliation.md` + RFC-0011-d v1.7.1 amendment)
