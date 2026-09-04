---
name: 0011-d-M11-phase2-domain-coordinator-platform-binding
description: Phase 2 (gated) per RFC-0011-d §Mission Decomposition M11 row: extend `crates/octo-network/src/dc/admin_attest.rs` (REAL substrate home; co-located with `PlatformAdminAttestError`) with `bind_domain_coordinator(role_binding, group_binding, platform_admin_proof) -> Result<GroupBinding, BindingError>`. Atomic with RFC-0850p-c binding ceremony + RFC-0855p-c §5a `PlatformEvent::AdminTransfer` envelope emission. Substrate-first: M11 lands BEFORE M10.
metadata:
  node_type: substrate-cli
  type: substrate-entrypoint-phase2
  originSessionId: RFC-0011-d author session
  created: 2026-08-31
  v: "1.3"
  depends_on:
    - RFC-0011-d
    - RFC-0855p-c (must be Accepted)
    - RFC-0850p-c (must be Accepted; binding ceremony substrate)
    - mission 0011-d-M4-octorole-select-with-stoolap-tx
status: Open
---

# 0011-d-M11-phase2-domain-coordinator-platform-binding — Phase 2 substrate `bind_domain_coordinator` per RFC-0011-d §Mission Decomposition M11

**Status:** Open — Phase 2 substrate for `bind_domain_coordinator` per RFC-0011-d §Mission Decomposition M11. Substrate prereqs cleared (RFC-0855p-c Accepted 2026-08-31; RFC-0850p-c Accepted 2025-Q3). Gate CLEARED 2026-08-31. No release-cycle gate. Implementation kickoff user-gated per [[feedback_initiation_user_only]] + [[git-workflow]] + [[implementation-workflow-hook]]. v1.3 drift-fix per `docs/audits/2026-09-03-m10-m11-substrate-truth-reconciliation.md` + RFC-0011-d v1.7.1 amendment: substrate home corrected to `crates/octo-network/src/dc/admin_attest.rs` (REAL, contains `PlatformAdminAttestError`); signature uses canonical Layer C types (`RoleBinding`, `GroupBinding`, `BindingError`); fictional `DomainCoordinatorRecord` + `DomainCoordinatorError` removed per RFC-0855p-c drift-fix B/C.
**Substrate:** RFC-0011-d §Mission Decomposition M11 row; §7.4 Substrate `[ADD]`; RFC-0850p-c binding ceremony + RFC-0855p-c §5a `PlatformEvent::AdminTransfer` envelope
**Parent:** RFC-0011-d
**Depends on:** RFC-0855p-c Accepted (gate CLEARED 2026-08-31); RFC-0850p-c Accepted (binding ceremony substrate); `0011-d-M4-octorole-select-with-stoolap-tx` (substrate precedent: BEGIN IMMEDIATE + signed envelope + Stoolap pattern)

## Status

Claimed (2026-09-02) by @mmacedoeu — **gate CLEARED 2026-08-31** (RFC-0855p-c Accepted) + RFC-0850p-c Accepted (binding ceremony substrate). Eleventh of 11 atomic missions (9 Phase 1 + 2 Phase 2). M11 substrate-first ordering: M11 must land BEFORE M10 per substrate-first workflow. v1.3 drift-fix per `docs/audits/2026-09-03-m10-m11-substrate-truth-reconciliation.md` + RFC-0011-d v1.7.1 amendment.

## Substrate (RFC-0011-d)

§Mission Decomposition M11 row (canonical drift-fixed):

- "Extend `crates/octo-network/src/dc/admin_attest.rs` (REAL substrate home; co-located with `PlatformAdminAttestError`) with `bind_domain_coordinator(role_binding, group_binding, platform_admin_proof) -> Result<GroupBinding, BindingError>` per §7.4. Atomic: verifies platform admin proof (RFC-0855p-c §5a freshness + DC pubkey match), invokes RFC-0850p-c binding ceremony state transition (GroupBinding::state → Bound + bound_peer_id → operator peer_id), emits `PlatformEvent::AdminTransfer` envelope post-commit (Layer D side-effect; non-transactional)."

§7.4 Substrate `[ADD]` signature (drift-fixed):

```rust
// In crates/octo-network/src/dc/admin_attest.rs (extend existing module)

/// Phase 2 substrate entry. Invoked by `octo_role::select_domain_coordinator`
/// when the operator binds the `domain-coordinator` role kind to a transport
/// group binding (RFC-0850p-c §4).
///
/// **Substrate home:** `crates/octo-network/src/dc/admin_attest.rs`
/// (REAL, Layer C specialized node; co-located with existing
/// `PlatformAdminAttestError` per RFC-0855p-c §Drift-fix B/C).
///
/// **Substrate-truth invariant:** `platform_admin_proof` MUST verify
/// (freshness via `MAX_ATTEST_AGE_EPOCHS` + DC pubkey match per
/// RFC-0855p-c §5a); the operator's pubkey (derived from `RoleBinding`)
/// MUST equal the DC pubkey encoded in the proof; `group_binding.state`
/// MUST be `Bound` (no transition from `Unbound` / `Quarantined` /
/// `ReBinding`). On success: `GroupBinding::state` remains `Bound` (idempotent
/// if already Bound); `GroupBinding::bound_peer_id` is set to the
/// operator's canonical peer_id; `PlatformEvent::AdminTransfer` envelope
/// is emitted post-commit (Layer D side-effect; non-transactional).
///
/// **Atomicity:** returns updated `GroupBinding` on success; on any
/// verification failure returns `BindingError` (canonical Layer C error;
/// NOT fictional `DomainCoordinatorError`). Phase 2 production wires
/// this into a single Stoolap `BEGIN IMMEDIATE` transaction covering
/// both the role binding (from `octo_role::select`) AND the binding
/// ceremony update — see M10's `select_domain_coordinator`.
///
/// **Phase 2 gating:** RFC-0855p-c + RFC-0850p-c must both be Accepted
/// (both gates CLEARED). CLI gates the `domain-coordinator` role kind
/// via `RoleError::RoleNotSelectable { reason: "M11 substrate not landed" }`
/// until this mission closes (per §Mission Decomposition M10 row
/// substrate-first ordering).
pub fn bind_domain_coordinator(
    role_binding: &RoleBinding,
    group_binding: &GroupBinding,
    platform_admin_proof: &PlatformAdminProof,
    current_epoch: u64,
) -> Result<GroupBinding, BindingError>
```

## Parent

RFC-0011-d §Mission Decomposition M11 row; §7.4 Substrate `[ADD]` signatures; §Phase 2.

## Depends on (GATE)

**HARD GATE** _(gate CLEARED 2026-08-31 per RFC-0855p-c + RFC-0850p-c Acceptance; retained as documentation per [[cipherocto-design-principles]] §Discipline at first call site)_: RFC-0855p-c + RFC-0850p-c MUST reach Accepted status. Per [[deferred-vs-unspecified]] + BLUEPRINT §Mission Lifecycle.

- `RFC-0855p-c` (Domain Coordinator role; `PlatformAdminAttestError` substrate; §5a `PlatformEvent::AdminTransfer` envelope; gate)
- `RFC-0850p-c` (binding ceremony substrate; `GroupBinding` + `BindingError` + `GroupRegistry::register_binding`; gate)
- `0011-d-M4-octorole-select-with-stoolap-tx` (substrate precedent: BEGIN IMMEDIATE + signed envelope + Stoolap pattern)

## Acceptance Criteria

- [ ] Extend `crates/octo-network/src/dc/admin_attest.rs` (REAL substrate home; Layer C specialized node; co-located with `PlatformAdminAttestError` per RFC-0855p-c; NOT new file per [[cipherocto-design-principles]] §No parallel abstractions)
- [ ] Add `pub struct PlatformAdminProof` typed envelope in `crates/octo-network/src/dc/admin_attest.rs` (extends existing module):
  - `pub platform: Platform` (canonical RFC-0855p-c enum)
  - `pub platform_admin_id: String` (canonical `participant_id` per RFC-0850p-c §Appendix A)
  - `pub dc_pubkey: Vec<u8>` (operator's pubkey; used to bind proof to role_binding)
  - `pub adapter_signature: [u8; 64]` (adapter is platform-trust-root)
  - `pub nonce: [u8; 32]`
  - `pub signed_at_epoch: u64`
- [ ] Add `pub fn bind_domain_coordinator(role_binding, group_binding, platform_admin_proof, current_epoch) -> Result<GroupBinding, BindingError>` per RFC §Mission Decomp M11 row + RFC-0011-d v1.7.1 §7.4 Substrate `[ADD]` Signatures
- [ ] Atomic: verifies `PlatformAdminProof` via existing `verify_attest(...)` (freshness + DC pubkey match per RFC-0855p-c §5a)
- [ ] Atomic: verifies operator pubkey (`RoleBinding` derived) equals `PlatformAdminProof::dc_pubkey`
- [ ] Atomic: verifies `group_binding.state == GroupState::Bound` (no transition from other states — returns `BindingError::InvalidTransition`)
- [ ] Atomic: invokes RFC-0850p-c binding ceremony state transition (updates `group_binding.bound_peer_id` to operator peer_id; state stays Bound; idempotent on re-bind)
- [ ] Returns canonical `BindingError` on any failure path (NOT fictional `DomainCoordinatorError`):
  - `BindingError::SignatureInvalid { reason }` on `PlatformAdminProof::adapter_signature` verification failure
  - `BindingError::InvalidTransition { from, to }` on non-`Bound` source state
  - `BindingError::NonceReplay { nonce }` on proof nonce reuse (canonical `BindingError` variant; existing at `dot/binding.rs:691`)
- [ ] Post-commit: emit `PlatformEvent::AdminTransfer` envelope per RFC-0855p-c §5a (Layer D side-effect; non-transactional; fire-and-forget; logged via existing `tracing-subscriber` redactor layer)
- [ ] `#[non_exhaustive]` on `PlatformAdminProof` per F-14 (matches existing `PlatformAdminAttestError` pattern)
- [ ] NO new substrate entrypoints (unbind/list/show are NOT in RFC §Mission Decomp M11 row scope; do NOT add)
- [ ] `cargo test -p octo-network bind_domain_coordinator_updates_binding_atomically`
- [ ] `cargo test -p octo-network bind_domain_coordinator_rolls_back_on_signature_invalid`
- [ ] `cargo test -p octo-network bind_domain_coordinator_rolls_back_on_invalid_transition`
- [ ] `cargo test -p octo-network bind_domain_coordinator_rolls_back_on_nonce_replay`
- [ ] `cargo test -p octo-network platform_admin_proof_serialization_roundtrip`
- [ ] `cargo check -p octo-network` zero warnings
- [ ] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [ ] **GATE CHECK**: `git log --oneline rfcs/accepted/networking/0855p-c-*.md rfcs/accepted/networking/0850p-c-*.md` shows both Accepted

## Scope

Single substrate entrypoint per RFC §Mission Decomp M11 row + `PlatformAdminProof` typed envelope. NO new substrate entrypoints (unbind/list/show OUT OF SCOPE). NO CLI binding (M10 owns CLI extension).

## Sub-steps

1. **VERIFY GATE** (RFC-0855p-c + RFC-0850p-c Accepted)
2. Add `pub struct PlatformAdminProof` to `crates/octo-network/src/dc/admin_attest.rs` (extend existing module)
3. Add `pub enum PlatformAdminProofError` extension or reuse existing `PlatformAdminAttestError` for proof verification (decision: REUSE existing `PlatformAdminAttestError`; semantics align — both check freshness + DC pubkey match)
4. Add `pub fn verify_platform_admin_proof(proof, expected_dc_pubkey, current_epoch) -> Result<(), PlatformAdminAttestError>` helper (wraps existing `verify_attest` with adapter-signature extension point)
5. Add `pub fn bind_domain_coordinator(...) -> Result<GroupBinding, BindingError>` per Acceptance Criteria
6. Wire `PlatformEvent::AdminTransfer` emission post-commit (call existing `attest_topic(domain_id, platform)` for logging destination; emit envelope via existing `tracing-subscriber` redactor layer)
7. Add 5 unit tests listed in Acceptance Criteria
8. Verify cargo check + clippy + fmt

## Test Vectors

- TV-DC-SUB-1: `bind_domain_coordinator(role_binding, group_binding, valid_proof)` with matching `GroupBinding::state == Bound` → returns updated `GroupBinding` with `bound_peer_id = operator peer_id`; `PlatformEvent::AdminTransfer` emitted post-commit
- TV-DC-SUB-2: `bind_domain_coordinator(...)` with `PlatformAdminProof::adapter_signature` invalid → returns `BindingError::SignatureInvalid { reason }`; `GroupBinding` unchanged (atomic rollback)
- TV-DC-SUB-3: `bind_domain_coordinator(...)` with `GroupBinding::state == Unbound` → returns `BindingError::InvalidTransition { from: Unbound, to: Bound }`; `GroupBinding` unchanged
- TV-DC-SUB-4: `bind_domain_coordinator(...)` with already-used proof `nonce` → returns `BindingError::NonceReplay { nonce }`; `GroupBinding` unchanged
- TV-DC-SUB-5: `PlatformAdminProof` round-trip serialization (canonical bytes per RFC-0104 DFP) — same inputs → same bytes

## Layer direction (per [[cipherocto-design-principles]])

- `octo-network/src/dc/admin_attest` (Layer C specialized node; RFC-0855p-c substrate) — `bind_domain_coordinator` + `PlatformAdminProof` + `PlatformEvent::AdminTransfer` emission
- `octo-network/src/dot/binding` (Layer C specialized node; RFC-0850p-c substrate) — `GroupBinding` + `BindingError` + `GroupRegistry::register_binding` (called by `bind_domain_coordinator`)
- `octo-network/src/dc` (Layer C specialized node; RFC-0855p-c substrate home) — re-exports
- `octo-role` (Layer C specialized node; RFC-0011-d substrate) — `RoleBinding` (input; output of M10 `select_domain_coordinator`)
- Stoolap (Layer D adapter; per RFC-0010) — `BEGIN IMMEDIATE` tx (single tx wraps M10 role binding + M11 binding ceremony update for full atomicity)

## Backward compat

Additive: 1 new public fn + 1 new struct + reuse existing error types on existing `crates/octo-network/src/dc/admin_attest.rs` module. NO existing crates modified.

## Risk

- **Gate not met**: cannot claim until RFC-0855p-c + RFC-0850p-c Accepted. Mitigation: explicit GATE CHECK in Sub-step 1.
- **Atomicity boundary**: `bind_domain_coordinator` MUST verify proof + update GroupBinding atomically. Mitigation: single Stoolap `BEGIN IMMEDIATE` tx covers verify + update; explicit rollback tests (TV-DC-SUB-2/3/4). Phase 1 production wires this into M10's `select_domain_coordinator` for full atomicity.
- **Scope creep**: unbind/list/show are tempting to add but NOT in RFC §Mission Decomp M11 row. Mitigation: explicit "NO new substrate entrypoints" in Acceptance Criteria.
- **Adapter signature verification**: `PlatformAdminProof::adapter_signature` is per-adapter (WhatsApp / Matrix / Telegram have different admin verification APIs). Mitigation: signature verification helper delegates to adapter at substrate boundary per RFC-0855p-c drift-fix B (per-adapter local state; surfaced as `PlatformEvent` envelopes; not centralized).
- **Drift-fix re-application**: the v1.7 amendment (substrate home + error type rename) was applied to RFC-0011-d at `next` `0e915618`+; this M11 YAML drift-fix aligns the mission YAML to the canonical substrate truth. Cross-RFC invariants preserved: `RecorderDid` canonical keying, `HARD_THRESHOLD = 5`, `SlashReasonCode::Extension(0x0100)`.

## Notes

- Phase 2 mission — gated
- Per [[deferred-vs-unspecified]]: deferred (not unspecified)
- Substrate-first ordering: M11 must land BEFORE M10 (M10 calls M11 substrate)
- Sub-step 1 = gate check; if unmet, mission pauses
- v1.3 drift-fix supersedes v1.2 substrate-home reconciliation: `crates/octo-network/src/mon/domain_coordinator.rs` (file never existed) → `crates/octo-network/src/dc/admin_attest.rs` (REAL; contains `PlatformAdminAttestError`)
- v1.3 drift-fix supersedes v1.2 error type rename: `DomainCoordinatorError` (fictional) → canonical `BindingError` (existing at `crates/octo-network/src/dot/binding.rs:648`; Layer C; semantically correct for binding ceremony state transitions)

## Cross-references

- RFC-0011-d §Mission Decomposition M11 row (canonical single-entrypoint scope; v1.7.1 drift-fixed signature)
- RFC-0011-d §7.4 (Substrate `[ADD]` signatures; v1.7.1 drift-fixed)
- RFC-0855p-c (`PlatformAdminAttestError` substrate home; §5a `PlatformEvent::AdminTransfer` envelope; gate; drift-fix B/C closed at commit `205f1434`)
- RFC-0850p-c (binding ceremony substrate; `GroupBinding` + `BindingError` + `GroupRegistry`; gate)
- M10 (substrate-first ordering: M10 calls M11)
- [[deferred-vs-unspecified]] — gated release
- `docs/audits/2026-09-03-m10-m11-substrate-truth-reconciliation.md` — drift-fix verdict + audit closure
- `memory/rfc-0011-d-m10-m11-substrate-impl-2026-09-03.md` — implementation closure card

## Claimant

@mmacedoeu (mission lifecycle: Claimed 2026-09-02; v1.3 drift-fix per `docs/audits/2026-09-03-m10-m11-substrate-truth-reconciliation.md` + RFC-0011-d v1.7.1 amendment)
