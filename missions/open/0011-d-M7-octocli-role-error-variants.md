---
name: 0011-d-M7-octocli-role-error-variants
description: Add 4 `OctoCliError` variants per RFC-0011-d §Mission Decomposition M7 row: `RoleNotFound` (31), `StakeInsufficient` (32), `RoleNotSelectable` (33), `SignerMismatch` (35 per F-16 reserved); redaction boundary per RFC-0011 §Redaction Layer.
metadata:
  node_type: substrate-cli
  type: cli-error
  originSessionId: RFC-0011-d author session
  created: 2026-08-31
  v: "1.1"
  depends_on:
    - RFC-0011-d
    - mission 0011-d-M6-octocli-role-commands
status: Claimed
---

# 0011-d-M7-octocli-role-error-variants — `OctoCliError` variants for `octo role` per RFC-0011-d §Mission Decomposition M7

**Status:** Claimed 2026-09-01 by @mmacedoeu — unblocked.
**Substrate:** RFC-0011-d §Mission Decomposition M7 row; RFC-0011 §Redaction Layer
**Parent:** RFC-0011-d
**Depends on:** `0011-d-M6-octocli-role-commands`

## Status

Claimed (2026-09-01) by @mmacedoeu. Seventh of 9 Phase 1 atomic missions. Adds the 4 CLI error variants + exit codes for `octo role` subcommands (per RFC §Mission Decomposition M7 row + F-16 reserved exit codes).

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
- RFC-0011 (parent CLI error enum; 28 existing variants)

## Acceptance Criteria

- [ ] `OctoCliError` enum extended in `crates/octo-cli/src/error.rs` with 4 new variants (per RFC §Mission Decomp M7 row)
- Variants: `RoleNotFound { role_id: String }`, `StakeInsufficient { required: u64, available: u64 }`, `RoleNotSelectable { role_id: String, reason: String }`, `SignerMismatch { expected: String, actual: String }`
- All 4 variants derive `thiserror::Error` + `Serialize` + `Deserialize`
- Exit codes wired per F-16 reserved slot: `RoleNotFound` → 31, `StakeInsufficient` → 32, `RoleNotSelectable` → 33, `SignerMismatch` → 35 (NOT 33)
- Exit code 34 remains RESERVED (per F-16 reserved range 36-63 for future amendment additions; 34 was previously shared with capability attenuation — moved to 10 in parent RFC; see RFC-0011 §Exit Codes)
- Error message format follows RFC-0011 §Error Handling pattern: `Error: <variant> <context> (<exit_code>)`
- Redaction: error variants NEVER include `signature_proof` field (assert in test)
- `Display` impl redacts `signature_proof` if any error variant accidentally carries it (defensive)
- `OctoCliError` `#[non_exhaustive]` attribute preserved (per RFC-0011 F-14)
- `cargo test -p octo-cli role_not_found_exit_31`
- `cargo test -p octo-cli stake_insufficient_exit_32`
- `cargo test -p octo-cli role_not_selectable_exit_33`
- `cargo test -p octo-cli signer_mismatch_exit_35` (asserts F-16 reserved slot)
- `cargo test -p octo-cli error_redacts_signature_proof`
- `cargo test -p octo-cli exit_34_reserved_for_future_amendment`
- `cargo check -p octo-cli` zero warnings
- `cargo clippy -p octo-cli --all-targets -- -D warnings` clean

## Scope

Error variants + exit codes only. NO new subcommand (M6 done). NO new test vectors (M8).

## Sub-steps

1. Extend `OctoCliError` enum with 4 variants in `crates/octo-cli/src/error.rs`
2. Wire `From<RoleError> for OctoCliError` for substrate error mapping (4 source variants)
3. Wire exit code 31, 32, 33, 35 in `impl OctoCliError { fn exit_code(&self) -> i32 }`
4. Wire `Display` impl per RFC-0011 §Error Handling format
5. Wire redaction check (assert no `signature_proof` in any variant)
6. Add 6 unit tests
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

Exit codes 31, 32, 33, 35 are NEW codes; exit 34 reserved per F-16 (parent RFC-0011 §Exit Codes). Per RFC migration etiquette (1 release cycle), exit code table is canonical — additive change.

## Risk

- **Exit code collision**: 31-33 + 35 unused per RFC-0011 §Error Handling + §Exit Codes. Mitigation: verify with `grep -r "exit_code = 31\|32\|33\|35" crates/octo-cli/` before landing; 34 reserved.
- **`signature_proof` leak via error context**: substrate redacts; CLI re-checks. Mitigation: test TV-ERR-5 enforces.
- **Error variant cardinality**: `#[non_exhaustive]` preserves upgrade path. Mitigation: per F-14 RFC-0011 finding.
- **F-16 reserved slot enforcement**: exit 35 for `SignerMismatch` must not collide with capability attenuation exit 10 (parent) or future amendments. Mitigation: §Exit Codes table in RFC-0011-d lists 36-63 as future-amendment range.

## Notes

- Error mapping is unidirectional: `RoleError → OctoCliError`. Never reverse (per [[cipherocto-design-principles]] no-premature-coupling).
- Display message format matches RFC-0011 §Error Handling for shell scriptability
- Redaction pattern: `tracing` Layer per RFC-0011 §Redaction Layer + per-variant `Display` impl defense-in-depth
- 4 variants per RFC §Mission Decomposition M7 row (NOT 5; do NOT add RoleBindingConflict or RoleAmbiguousId — those are substrate concerns surfaced as RoleNotSelectable in CLI)

## Cross-references

- RFC-0011-d §Mission Decomposition M7 row (canonical 4-variant list)
- RFC-0011-d §Exit Codes (F-16 reserved slot for `SignerMismatch` at 35)
- RFC-0011 §Redaction Layer (signature_proof stripping)
- RFC-0011 §Error Handling (error message format)
- F-14 (#[non_exhaustive] on OctoCliError)
- F-16 (reserved exit code slot 35)
- [[cipherocto-design-principles]] — no premature coupling (substrate → CLI unidirectional)

## Claimant

@mmacedoeu (mission lifecycle: Claimed 2026-09-01)
