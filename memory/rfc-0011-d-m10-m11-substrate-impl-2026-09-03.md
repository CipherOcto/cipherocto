---
name: rfc-0011-d-m10-m11-substrate-impl-2026-09-03
description: M10 + M11 substrate + CLI impl landed 2026-09-03 with drift-fix v1.7.1
metadata:
  type: project
---

# RFC-0011-d M10 + M11 Substrate Impl — CLOSED 2026-09-03

`next` branch. Goal complete: drift-fix reconciliation INLINE + M10 + M11
substrate + M10 CLI impl landed. 33 + 7 + 17 = 57 lib tests pass;
workspace clippy clean.

## Substrate surfaces

**M11 (`octo_network::dc::admin_attest`):**
- `pub struct PlatformAdminProof` (`#[non_exhaustive]`, drift-fix C
  typed envelope replacing fictional `platform_admin_id: &str`).
- `pub fn PlatformAdminProof::new(...)` constructor (closes
  `#[non_exhaustive]` external-build gap).
- `pub fn verify_platform_admin_proof(proof, expected_dc_pubkey: &[u8; 32], current_epoch) -> Result<(), PlatformAdminAttestError>`.
- `pub fn bind_domain_coordinator(operator_did: &str, group_binding: &GroupBinding, platform_admin_proof: &PlatformAdminProof, current_epoch: u64) -> Result<GroupBinding, BindingError>` (canonical Layer C error at `crates/octo-network/src/dot/binding.rs:648`).

**M10 (`octo_role::select`):**
- `pub fn select_coordinator(role_id, operator_did, signer, chain_id, store, mission_id, current_epoch) -> Result<RoleBinding, RoleError>` — wraps `select_with_chain_id` + emits `HandoverRequestEnvelope` (RFC-0855p-e substrate, discarded in Phase 1; production wires DOT broadcast topic).
- `pub fn select_domain_coordinator(role_id, operator_did, signer, chain_id, store, group_binding, platform_admin_proof, current_epoch) -> Result<(RoleBinding, GroupBinding), RoleError>` — atomic with M11 `bind_domain_coordinator`.
- `pub fn build_handover_request(operator_did, signer, coordinator_role, mission_id, reason, current_epoch) -> Result<HandoverRequestEnvelope, RoleError>` — public helper.
- `RoleError::GroupBindingRejected { reason }` — 6th variant (exit code 36 in CLI).

**Helper (`octo_cap_macaroon::signer`):**
- `pub fn pubkey_from_did(did: &str) -> Option<[u8; 32]>` — Phase 1
  `did:octo:0x<hex>` inverse of `did_from_pubkey`. Production swaps
  in `octo_ident::WireDid` round-trip per RFC-0010.

## CLI surface (M10)

`octo role select <role_id> [--coordinator | --domain-coordinator] --mission-id <hex> --current-epoch <N> [--group-jid <str> --platform <str> --platform-admin-proof <json>]`

3 mutually exclusive invocation modes:
- Plain: `octo role select <role>` → `substrate_select`
- `--coordinator`: → `substrate_select_coordinator` (emits HORQ)
- `--domain-coordinator`: → `substrate_select_domain_coordinator`
  (atomic with RFC-0855p-c §5a group binding ceremony)

## Registry extension

3 coordinator roles added to `crates/octo-role/src/registry.rs`:
- `domain-coordinator` (governance, 1000 OCTO min)
- `mission-coordinator` (governance, 2000 OCTO min)
- `witness-coordinator` (governance, 500 OCTO min)

OCTO-only (no role token), slashing rules: `handover_timeout` +
`double_sign`. Total roles: 7 → 10.

## Drift-fix resolution (per user direction: INLINE, not separated amendments)

- `missions/open/0011-d-M10-phase2-coordinator-domain-coordinator.md` — drift-fixed (fictional `octo_network::mon::domain_coordinator` references removed).
- `missions/open/0011-d-M11-phase2-domain-coordinator-platform-binding.md` — drift-fixed (substrate home relocated to `crates/octo-network/src/dc/admin_attest.rs`).
- `rfcs/accepted/process/0011-d-role-provisioning.md` — v1.7.1
  amendment (Status header, §7.2, §7.4, §Key Files, §Mission
  Decomposition, VH row). No `git mv` to v1.7 file needed; in-place
  version-history append.

## Layer direction verified

- B → B: `octo-role` → `octo-network` (coordinator substrate).
- C → B: `octo-cli` → `octo-role` + `octo-network`.
- B → B: `octo-network` → `octo-cap-macaroon` (`pubkey_from_did`).
- No inversions. See audit `docs/audits/2026-09-03-rfc-0011-d-m10-m11-substrate-truth-reconciliation.md`.

## Test summary

| Crate | New tests | Total |
|-------|-----------|-------|
| `octo-role` | 10 (+2 helpers) | 33 lib tests pass |
| `octo-cli` | 7 | 218 lib tests pass (7 in role) |
| `octo-network` (admin_attest) | 6 | 17 lib tests pass |
| `octo-cap-macaroon` | 2 | 26 lib tests pass |

`cargo clippy --workspace --all-targets -- -D warnings` → clean (1m41s).

## User-owned follow-on actions

- `git push origin next` + PR `next → main`.
- `git mv missions/open/0011-d-M10-phase2-coordinator-domain-coordinator.md missions/archived/completed/`.
- `git mv missions/open/0011-d-M11-phase2-domain-coordinator-platform-binding.md missions/archived/completed/`.
- RFC-0011-d v1.7.1 PR review (7-day RFC review window).

NO PUSH.

## Cross-RFC invariants preserved

- `RecorderDid` keying (RFC-0968 §28.4 amend 22).
- `HARD_THRESHOLD=5` (slash_store + dc_store).
- `0x0100` cross-domain slash code (RFC-0855p-c §9c).
- `CoordinatorRole` enum (`MissionCoordinator=0x00`, `DomainCoordinator=0x01`, `WitnessCoordinator=0x02`).
- `HandoverReason::Voluntary = 0x00` for self-selecting coordinators.
- `MAX_ATTEST_AGE_EPOCHS = 100` (RFC-0855p-c §5a).
- `pubkey_from_did` is Phase 1 form; production swaps in `octo_ident::WireDid`.

## Why

Goal: "proceed with reconciling if anything pending, after it go for substrate impl" — reconciliation closed 2026-09-03 (drift-fix v1.7.1 + M10/M11 YAML drift-fix); M10 + M11 substrate + CLI landed same day. RFC-0011-d Phase 2 entrypoints now substrate-complete; CLI can bind role / coordinator / domain-coordinator end-to-end.

**How to apply:** when next RFC-0011 amendment lands (RFC-0011-e vault ops, RFC-0011-f mesh ops, RFC-0011-g governance), use the same drift-fix-then-impl workflow. Substrate seams are RFC-0855p-e (coordinator handover) + RFC-0855p-c (DC substrate) — both reuse without modification.
