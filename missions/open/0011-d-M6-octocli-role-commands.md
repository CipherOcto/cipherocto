---
name: 0011-d-M6-octocli-role-commands
description: Add `octo role {list,show,select}` clap subcommands + impl dispatch per RFC-0011-d §7.4; thin CLI wrapper calling M3/M4/M5 substrate.
metadata:
  node_type: substrate-cli
  type: cli-subcommand
  originSessionId: RFC-0011-d author session
  created: 2026-08-31
  v: "1.0"
  depends_on:
    - RFC-0011-d
    - mission 0011-d-M4-octorole-select-with-stoolap-tx
    - mission 0011-d-M5-octowallet-nonce-counter
status: Open
---

# 0011-d-M6-octocli-role-commands — `octo role {list,show,select}` clap subcommands per RFC-0011-d §7.4

**Status:** Open (2026-08-31) — unblocked.
**Substrate:** RFC-0011-d §7.4 CLI surface (operator UX)
**Parent:** RFC-0011-d
**Depends on:** `0011-d-M4-octorole-select-with-stoolap-tx` + `0011-d-M5-octowallet-nonce-counter`

## Status

Open (2026-08-31). Sixth of 9 Phase 1 atomic missions. Wires the CLI surface for the 3 role subcommands.

## Substrate (RFC-0011-d)

§7.4 CLI subcommand surface:

- `octo role list [--kind <kind>] [--class <class>] [--requires-octo-min <n>] [--json]`
- `octo role show <role_id> [--json]`
- `octo role select <role_id> [--confirm] [--dry-run] [--json]`

§Key Files row "Command dispatch" (MUST be thin wrapper per [[cipherocto-design-principles]] no-god-object).

## Depends on

- `0011-d-M4-octorole-select-with-stoolap-tx` (`octo_role::select` entrypoint)
- `0011-d-M5-octowallet-nonce-counter` (`next_nonce_counter` + `persist_role_binding`)
- RFC-0011 (parent CLI substrate; existing `Octo` clap root struct)

## Acceptance Criteria

- [ ] `crates/octo-cli/src/commands/role.rs` NEW module
- `Role { kind, class, requires_octo_min }` clap struct added to `Octo::Role`
- `Role::List { kind: Option<String>, class: Option<String>, requires_octo_min: Option<u64>, json: bool }` subcommand
- `Role::Show { role_id: String, json: bool }` subcommand
- `Role::Select { role_id: String, confirm: bool, dry_run: bool, json: bool }` subcommand
- `Role::List` calls `octo_role::list(filter)` (M3); wraps result in `OutputEnvelope<Vec<RoleSummary>>`
- `Role::Show` calls `octo_role::show(role_id)` (M3); wraps result in `OutputEnvelope<RoleRecord>`
- `Role::Select` calls `octo_wallet::next_nonce_counter(identity_did)` → `octo_role::select(role_id, signer, nonce)` → `octo_wallet::persist_role_binding(rb)` (orchestrates M4 + M5)
- `Role::Select --dry-run` does NOT call `next_nonce_counter` (no side effect)
- `Role::Select --confirm` required for non-dry-run (assert in impl; exit code 64 per RFC-0011-d §Exit Codes — see M7 for error variant)
- `Role::Select` exits 0 on success, error code on substrate error
- `cargo test -p octo-cli role_list_with_filter`
- `cargo test -p octo-cli role_show_returns_full_record`
- `cargo test -p octo-cli role_select_dry_run_no_side_effect`
- `cargo test -p octo-cli role_select_requires_confirm`
- `cargo check -p octo-cli` zero warnings
- `cargo clippy -p octo-cli --all-targets -- -D warnings` clean

## Scope

CLI binding only. NO new error variants (M7). NO new test vectors (M8). NO doc changes (M9).

## Sub-steps

1. Add `Role` clap module to `crates/octo-cli/src/commands/mod.rs`
2. Add `Role::List` + `Role::Show` + `Role::Select` clap structs
3. Wire `Octo::Role(Role::List|Show|Select)` into root clap enum
4. Implement `Role::List` → `octo_role::list` call
5. Implement `Role::Show` → `octo_role::show` call
6. Implement `Role::Select` → orchestration (nonce → select → persist)
7. Add `OutputEnvelope<T>` wrapping for all 3 subcommands
8. Add `--dry-run` guard for `Role::Select`
9. Add `--confirm` guard for `Role::Select`
10. Add 4 unit tests
11. Verify cargo check + clippy + fmt

## Test Vectors

N/A — CLI integration tests land in M8 (TV-RX-1 through TV-SEL-8 with full CLI invocation).

## Layer direction (per [[cipherocto-design-principles]])

- `octo-cli` (Layer C; per RFC-0011) — orchestrator
- `octo-role` (Layer B; per RFC-0011-d) — substrate entrypoint
- `octo-wallet` (Layer B; per RFC-0009) — nonce counter + persistence

CLI depends on substrate; substrate NEVER depends on CLI (per layer model).

## Backward compat

Additive: new subcommand. NO existing CLI commands modified. NO `schema_version` bump (output envelope already versioned).

## Risk

- **Synchronous vs async mismatch**: `octo_role::select` is async; clap subcommand fn is sync. Mitigation: use `tokio::runtime::Runtime` block (already wired in `octo-cli` per RFC-0011).
- **`--confirm` bypass via dry-run**: dry-run skips persistence but still validates stake. Mitigation: dry-run returns structured error if stake fails (no signing).
- **Identity DID lookup**: `octo_wallet::next_nonce_counter(identity_did)` requires active identity. Mitigation: call `octo_wallet::active_identity()` first; surface error if no active identity.

## Notes

- Thin wrapper — no business logic in CLI per [[cipherocto-design-principles]]
- Output envelope shape: `{ schema_version, generated_at, data, exit_code }` per RFC-0011 §Output Envelope
- TTY detection: inherited from RFC-0011 §Output Envelope — pretty when TTY, JSON when `--json` or non-TTY

## Claimant

@unassigned (mission lifecycle: Open)
