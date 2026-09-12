---
name: 0011-d-M7-octocli-role-error-variants
description: Add 4 `OctoCliError` variants per RFC-0011-d §Mission Decomposition M7 row: `RoleNotFound` (31), `StakeInsufficient` (32), `RoleNotSelectable` (33), `SignerMismatch` (35 per F-16 reserved); redaction boundary per RFC-0011 §Redaction Layer.
metadata:
  node_type: substrate-cli
  type: cli-error
  originSessionId: RFC-0011-d author session
  created: 2026-08-31
  v: "1.1"
  landing_commit: "63ffdf94"
  verified_by: "@mmacedoeu"
  review_commit: "f08d0ca9"
  closed: 2026-09-01
  depends_on:
    - RFC-0011-d
    - mission 0011-d-M6-octocli-role-commands
status: Completed
---

# 0011-d-M7-octocli-role-error-variants — `OctoCliError` variants for `octo role` per RFC-0011-d §Mission Decomposition M7

**Status:** Completed (2026-09-01). LANDED commit `63ffdf94` (combined M6+M7 landing per substrate cycle).

> **Retro-supersession (2026-09-01):** M7 landed in same substrate cycle as M6 (combined landing commit `63ffdf94`). Substrate-truth deviations from original AC text documented inline below per M1 close-out pattern.

**Substrate:** RFC-0011-d §Mission Decomposition M7 row; RFC-0011 §Redaction Layer
**Parent:** RFC-0011-d
**Depends on:** `0011-d-M6-octocli-role-commands`

## Status

Closed (2026-09-01). Substrate delivered: 4 new `OctoCliError` variants + exit code mapping in `crates/octo-cli/src/error.rs`. 137 octo-cli lib tests pass per Phase 1 DRY closure.

## Substrate (RFC-0011-d)

§Mission Decomposition M7 row (canonical):

| Exit | Variant             | Trigger                                                                                             |
| ---- | ------------------- | --------------------------------------------------------------------------------------------------- |
| 31   | `RoleNotFound`      | `octo_role::show` / `select` with missing `role_id`                                                 |
| 32   | `StakeInsufficient` | `select` with stake < `requires_octo_min`                                                           |
| 33   | `RoleNotSelectable` | `select` with `RoleNotSelectable { reason }` (e.g., auditor mode, partial-prereq guard for Phase 2) |
| 35   | `SignerMismatch`    | `select` with `signer.did() ≠ operator_did` (F-16 reserved slot)                                    |

§Redaction (RFC-0011 §Redaction Layer): `signature_proof` field stripped from any error context (already redacted in substrate; CLI re-checks).

## Parent

RFC-0011-d §Mission Decomposition M7 row; §Exit Codes (F-16 reserved slot); RFC-0011 §Redaction Layer.

## Depends on

- `0011-d-M6-octocli-role-commands` (clap subcommand impl needs error variants to surface)
- RFC-0011 (parent CLI error enum)

## Acceptance Criteria

- [x] `OctoCliError` enum extended in `crates/octo-cli/src/error.rs` with 4 new variants (per RFC §Mission Decomp M7 row)
- [x] Variants: `RoleNotFound { role_id: String }`, `StakeInsufficient { required: u64, available: u64 }`, `RoleNotSelectable { role_id: String, reason: String }`, `SignerMismatch { expected: String, actual: String }`
- [x] All 4 variants derive `thiserror::Error` + `Serialize` + `Deserialize`
- [x] Exit codes wired per F-16 reserved slot: `RoleNotFound` → 31, `StakeInsufficient` → 32, `RoleNotSelectable` → 33, `SignerMismatch` → 35
- [x] Exit code 34 remains RESERVED (per F-16 reserved range)
- [x] Error message format follows RFC-0011 §Error Handling pattern: `Error: <variant> <context> (<exit_code>)`
- [x] Redaction: error variants NEVER include `signature_proof` field (assert in test)
- [x] `Display` impl redacts `signature_proof` if any error variant accidentally carries it (defensive)
- [x] `OctoCliError` `#[non_exhaustive]` attribute preserved (per RFC-0011 F-14)
- [x] `cargo test -p octo-cli role_*_exit_*` tests pass
- [x] `cargo test -p octo-cli error_redacts_signature_proof` passes
- [x] `cargo test -p octo-cli exit_34_reserved_for_future_amendment` passes
- [x] `cargo check -p octo-cli` zero warnings
- [x] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean

## Scope

Error variants + exit codes only. NO new subcommand (M6 done). NO new test vectors (M8).

## Sub-steps

1. Extend `OctoCliError` enum with 4 variants in `crates/octo-cli/src/error.rs`
2. Wire `From<RoleError> for OctoCliError` for substrate error mapping (4 source variants)
3. Wire exit code 31, 32, 33, 35 in `impl OctoCliError { fn exit_code(&self) -> i32 }`
4. Wire `Display` impl per RFC-0011 §Error Handling format
5. Wire redaction check (assert no `signature_proof` in any variant)
6. Add unit tests
7. Verify cargo check + clippy + fmt

## Test Vectors

- TV-ERR-1: `RoleNotFound { role_id: "builder-v2" }` exits 31
- TV-ERR-2: `StakeInsufficient { required: 10000, available: 500 }` exits 32
- TV-ERR-3: `RoleNotSelectable { role_id: "domain-coordinator", reason: "auditor mode" }` exits 33 (partial-prereq guard for Phase 2)
- TV-ERR-4: `SignerMismatch { expected: "did:octo:abc", actual: "did:octo:def" }` exits 35 (F-16 reserved slot)
- TV-ERR-5: any error variant carrying `signature_proof` is redacted in Display (defensive test)
- TV-ERR-6: exit code 34 NOT mapped to any role variant (reserved for future amendment additions per F-16)

## Layer direction (per [[cipherocto-design-principles]])

- `octo-cli` (Layer C; per RFC-0011) — operator UX errors
- Error mapping: substrate `RoleError` → CLI `OctoCliError` (typed mapping; preserves structured fields per [[cipherocto-design-principles]] no-god-object)

## Backward compat

Additive: 4 new error variants + 4 new exit codes. NO existing variants modified. NO `schema_version` bump (output envelope already versioned; errors don't carry schema version).

Exit codes 31, 32, 33, 35 are NEW codes; exit 34 reserved per F-16 (parent RFC-0011 §Exit Codes).

## Cross-references

- RFC-0011-d §Mission Decomposition M7 row (canonical 4-variant list)
- RFC-0011-d §Exit Codes (F-16 reserved slot for `SignerMismatch` at 35)
- RFC-0011 §Redaction Layer (signature_proof stripping)
- RFC-0011 §Error Handling (error message format)
- F-14 (#[non_exhaustive] on OctoCliError)
- F-16 (reserved exit code slot 35)
- [[cipherocto-design-principles]] — no premature coupling (substrate → CLI unidirectional)

## Notes

- Error mapping is unidirectional: `RoleError → OctoCliError`. Never reverse (per [[cipherocto-design-principles]] no-premature-coupling)
- Display message format matches RFC-0011 §Error Handling for shell scriptability
- Redaction pattern: `tracing` Layer per RFC-0011 §Redaction Layer + per-variant `Display` impl defense-in-depth
- 4 variants per RFC §Mission Decomposition M7 row (NOT 5; no `RoleBindingConflict` or `RoleAmbiguousId` — those are substrate concerns surfaced as `RoleNotSelectable` in CLI)
- Substrate-truth note: landed in combined M6+M7 commit `63ffdf94` (per substrate cycle)
- Per audit 2026-09-01: mission YAML bookkeeping lag addressed at mission close-out via this revision

## Claimant

@mmacedoeu (mission lifecycle: Claimed 2026-09-01 → Closed 2026-09-01)