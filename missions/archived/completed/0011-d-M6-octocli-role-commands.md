---
name: 0011-d-M6-octocli-role-commands
description: Add `octo role {list,show,select}` clap subcommands + impl dispatch per RFC-0011-d §7.4; thin CLI wrapper calling M3/M4/M5 substrate.
metadata:
  node_type: substrate-cli
  type: cli-subcommand
  originSessionId: RFC-0011-d author session
  created: 2026-08-31
  v: "1.0"
  landing_commit: "63ffdf94"
  verified_by: "@mmacedoeu"
  review_commit: "f08d0ca9"
  closed: 2026-09-01
  depends_on:
    - RFC-0011-d
    - mission 0011-d-M4-octorole-select-with-stoolap-tx
    - mission 0011-d-M5-octowallet-nonce-counter
status: Completed
---

# 0011-d-M6-octocli-role-commands — `octo role {list,show,select}` clap subcommands per RFC-0011-d §7.4

**Status:** Completed (2026-09-01). LANDED commit `63ffdf94` (feat: M6 + M7 role subcommand surface + 4 OctoCliError variants) — single combined landing commit per substrate cycle.

> **Retro-supersession (2026-09-01):** M6 + M7 landed together in single commit `63ffdf94` per substrate cycle; M8 landed in subsequent cycle `90ce73bd`. Substrate-truth deviations from original AC text documented inline below per M1 close-out pattern.

**Substrate:** RFC-0011-d §7.4 CLI surface (operator UX)
**Parent:** RFC-0011-d
**Depends on:** `0011-d-M4-octorole-select-with-stoolap-tx` + `0011-d-M5-octowallet-nonce-counter`

## Status

Closed (2026-09-01). Substrate delivered: `Role { List, Show, Select }` clap subcommands + dispatch in `crates/octo-cli/src/commands/role.rs` + `crates/octo-cli/src/error.rs` (4 new OctoCliError variants per M7 AC). 137 octo-cli lib tests + 19 identity tests + 11 integration tests pass per Phase 1 DRY closure.

## Substrate (RFC-0011-d)

§7.4 CLI subcommand surface:

- `octo role list [--kind <kind>] [--class <class>] [--requires-octo-min <n>] [--json]`
- `octo role show <role_id> [--json]`
- `octo role select <role_id> [--confirm] [--dry-run] [--json]`

§Key Files row "Command dispatch" (MUST be thin wrapper per [[cipherocto-design-principles]] no-god-object).

## Depends on

- `0011-d-M4-octorole-select-with-stoolap-tx` (`octo_role::select` entrypoint)
- `0011-d-M5-octowallet-nonce-counter` (`next_nonce_counter` + `binding_nonce` field per F-NEW-3 split)
- RFC-0011 (parent CLI substrate; existing `Octo` clap root struct)

## Acceptance Criteria

- [x] `crates/octo-cli/src/commands/role.rs` NEW module created
- [x] `Role { List, Show, Select }` clap subcommands added to `Octo::Role`
- [x] `Role::List { kind: Option<String>, class: Option<String>, requires_octo_min: Option<u64>, json: bool }` subcommand
- [x] `Role::Show { role_id: String, json: bool }` subcommand
- [x] `Role::Select { role_id: String, confirm: bool, dry_run: bool, json: bool }` subcommand
- [x] `Role::List` calls `octo_role::list(filter)` (M3); wraps result in `OutputEnvelope<Vec<RoleSummary>>`
- [x] `Role::Show` calls `octo_role::show(role_id)` (M3); wraps result in `OutputEnvelope<RoleRecord>`
- [x] `Role::Select` calls `octo_wallet::next_nonce_counter(identity_did)` → `octo_role::select(role_id, signer, nonce, now)` → persists binding_nonce (orchestrates M4 + M5)
- [x] `Role::Select --dry-run` does NOT call `next_nonce_counter` (no side effect)
- [x] `Role::Select --confirm` required for non-dry-run (assert in impl; exit code 64 per RFC-0011-d §Exit Codes — see M7 for error variant)
- [x] `Role::Select` exits 0 on success, error code on substrate error
- [x] `cargo test -p octo-cli role_*` tests pass
- [x] `cargo check -p octo-cli` zero warnings
- [x] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean

## Scope

CLI binding only. NO new error variants (M7 — landed in same commit per combined substrate cycle). NO new test vectors (M8 — landed in `90ce73bd`). NO doc changes (M9).

## Sub-steps

1. Add `Role` clap module to `crates/octo-cli/src/commands/mod.rs`
2. Add `Role::List` + `Role::Show` + `Role::Select` clap structs
3. Wire `Octo::Role(Role::List|Show|Select)` into root clap enum
4. Implement `Role::List` → `octo_role::list` call
5. Implement `Role::Show` → `octo_role::show` call
6. Implement `Role::Select` → orchestration (nonce → select → persist binding_nonce)
7. Add `OutputEnvelope<T>` wrapping for all 3 subcommands
8. Add `--dry-run` guard for `Role::Select`
9. Add `--confirm` guard for `Role::Select`
10. Verify cargo check + clippy + fmt

## Test Vectors

N/A — CLI integration tests land in M8 (TV-RX-1 through TV-SEL-8 with full CLI invocation).

## Layer direction (per [[cipherocto-design-principles]])

- `octo-cli` (Layer C; per RFC-0011) — orchestrator
- `octo-role` (Layer B; per RFC-0011-d) — substrate entrypoint
- `octo-wallet` (Layer B; per RFC-0009) — nonce counter + persistence

CLI depends on substrate; substrate NEVER depends on CLI (per layer model).

## Backward compat

Additive: new subcommand. NO existing CLI commands modified. NO `schema_version` bump (output envelope already versioned).

## Cross-references

- RFC-0011-d §7.4 (CLI subcommand surface; substrate types)
- RFC-0011 (parent CLI substrate)
- [[cipherocto-design-principles]] — thin wrapper rule (Layer C → Layer B)

## Notes

- Thin wrapper — no business logic in CLI per [[cipherocto-design-principles]]
- Output envelope shape: `{ schema_version, generated_at, data, exit_code }` per RFC-0011 §Output Envelope
- TTY detection: inherited from RFC-0011 §Output Envelope — pretty when TTY, JSON when `--json` or non-TTY
- Substrate-truth note: M6 + M7 landed in single combined commit `63ffdf94` (per substrate cycle); M7 error variants wired in same PR
- Per audit 2026-09-01: mission YAML bookkeeping lag addressed at mission close-out via this revision

## Claimant

@mmacedoeu (mission lifecycle: Claimed 2026-09-01 → Closed 2026-09-01)