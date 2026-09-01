---
name: 0011-d-M8-octocli-role-tests
description: Author 11 YAML test vectors per RFC-0011-d §11 canonical distribution (TV-RL-1..3 role list + TV-RS-1..3 role show + TV-RX-1..4 role select + TV-RP-1 partial-prereq guard); wire to `docs/07-developers/octo-cli-implementation-guide.md` + `assert_cmd` integration test suite per RFC §Mission Decomposition M8 row.
metadata:
  node_type: substrate-cli
  type: test-vectors
  originSessionId: RFC-0011-d author session
  created: 2026-08-31
  v: "1.1"
  depends_on:
    - RFC-0011-d
    - mission 0011-d-M6-octocli-role-commands
    - mission 0011-d-M7-octocli-role-error-variants
status: Open
---

# 0011-d-M8-octocli-role-tests — Test vectors per RFC-0011-d §11 canonical distribution

**Status:** Open (2026-08-31) — unblocked.
**Substrate:** RFC-0011-d §11 Test Vectors + RFC-0011 §Output Envelope + RFC-0011-d §Mission Decomposition M8 row
**Parent:** RFC-0011-d
**Depends on:** `0011-d-M6-octocli-role-commands` + `0011-d-M7-octocli-role-error-variants`

## Status

Open (2026-08-31). Eighth of 9 Phase 1 atomic missions. Lands the 11 canonical test vectors per RFC §11 + `assert_cmd` integration tests.

## Substrate (RFC-0011-d)

§11 Test Vectors (canonical distribution per §Mission Decomposition M8 row):

- 3 Role list vectors: TV-RL-1, TV-RL-2, TV-RL-3
- 3 Role show vectors: TV-RS-1, TV-RS-2, TV-RS-3
- 4 Role select vectors: TV-RX-1, TV-RX-2, TV-RX-3, TV-RX-4
- 1 Partial-prereq guard vector: TV-RP-1

Total: 11 vectors. Vectors authored in `docs/07-developers/octo-cli-implementation-guide.md` per RFC §Mission Decomposition M8 row (NOT separate test/*.yaml files).

## Parent

RFC-0011-d §11 Test Vectors; §Mission Decomposition M8 row; RFC-0011 §Output Envelope (envelope shape for vectors).

## Depends on

- `0011-d-M6-octocli-role-commands` (subcommand impl)
- `0011-d-M7-octocli-role-error-variants` (error variants for TV-RP-1 + TV-RX-4 coverage)

## Acceptance Criteria

- [ ] 11 vectors authored in `docs/07-developers/octo-cli-implementation-guide.md` §Test Vectors section per RFC §Mission Decomposition M8 row
- Vector IDs match RFC §11 canonical scheme: TV-RL-1..3 (list) + TV-RS-1..3 (show) + TV-RX-1..4 (select) + TV-RP-1 (guard) = 11 total
- Vectors include 5 CLI integration + 6 substrate (assert_cmd covers CLI; cargo test covers substrate)
- 11 `cargo test` integration tests pass via `assert_cmd::Command::cargo_bin("octo")`
- `cargo test -p octo-cli --test role_integration` passes (all 11 vectors)
- `cargo test -p octo-role --lib` passes (substrate vectors)
- `cargo test -p octo-wallet --lib` passes (nonce counter vector)
- `npx prettier --write docs/07-developers/octo-cli-implementation-guide.md` PASS
- `bash scripts/validate_cites.sh docs/07-developers/octo-cli-implementation-guide.md` 0 INVALID
- `cargo check --workspace --all-targets` zero warnings
- `cargo clippy --workspace --all-targets -- -D warnings` clean

## Scope

Test vectors + integration tests only. NO new subcommand (M6). NO new error variants (M7). NO doc changes (M9).

## Sub-steps

1. Author 3 TV-RL vectors in `docs/07-developers/octo-cli-implementation-guide.md` §Test Vectors (list paths)
2. Author 3 TV-RS vectors (show paths)
3. Author 4 TV-RX vectors (select paths; one per error variant + 1 success)
4. Author 1 TV-RP vector (partial-prereq guard for Phase 2 coordinator + domain-coordinator roles; asserts exit 33 + RoleNotSelectable + prereq RFC names in error message)
5. Wire `assert_cmd` integration test harness `crates/octo-cli/tests/role_integration.rs` referencing the 11 vectors from impl guide
6. Wire fixture loader (octo-role fixture + octo-wallet fixture)
7. Add CLI test for each of 11 vectors
8. Verify all 11 cargo tests pass + clippy + fmt
9. Verify impl guide prettier PASS + cite sweep clean

## Test Vectors (canonical §11 distribution)

Per RFC-0011-d §11, vectors authored in `docs/07-developers/octo-cli-implementation-guide.md`:

```yaml
# Role list (3 vectors per §11)
- id: TV-RL-1
  command: octo role list
  expect:
    exit_code: 0
    data_type: Vec<RoleSummary>
    count: 7 # Phase 1: builder, provider, storage, bandwidth, orchestrator, recorder, wallet

- id: TV-RL-2
  command: octo role list --kind builder
  expect:
    exit_code: 0
    data_type: Vec<RoleSummary>
    filter_applied: { kind: builder }

- id: TV-RL-3
  command: octo role list --kind nonexistent
  expect:
    exit_code: 0
    data_type: Vec<RoleSummary>
    data: [] # empty, not error

# Role show (3 vectors per §11)
- id: TV-RS-1
  command: octo role show builder
  expect:
    exit_code: 0
    data_type: RoleRecord
    field: requires_octo_min
    field: allowed_actions

- id: TV-RS-2
  command: octo role show builder --with-slashing-rules
  expect:
    exit_code: 0
    data_type: RoleRecord
    field: slashing_rules
    field_present: true

- id: TV-RS-3
  command: octo role show nonexistent-role
  expect:
    exit_code: 31
    error: RoleNotFound # exit 31 per M7

# Role select (4 vectors per §11)
- id: TV-RX-1
  command: octo role select builder --dry-run --json
  expect:
    exit_code: 0
    data_type: RoleBinding
    side_effect: NONE
    redaction: SIGNATURE_PROOF_STRIPPED

- id: TV-RX-2
  command: octo role select builder --confirm --json
  expect:
    exit_code: 0
    data_type: RoleBinding
    body_hash_deterministic: true
    redaction: SIGNATURE_PROOF_STRIPPED

- id: TV-RX-3
  command: octo role select nonexistent-role --confirm --json
  expect:
    exit_code: 31
    error: RoleNotFound

- id: TV-RX-4
  command: octo role select builder --confirm --json (signer.did mismatch)
  expect:
    exit_code: 35 # F-16 reserved slot
    error: SignerMismatch

# Partial-prereq guard (1 vector per §11; §Mission Decomposition Phase 2)
- id: TV-RP-1
  command: octo role select domain-coordinator --confirm --json
  expect:
    exit_code: 33
    error: RoleNotSelectable
    error_message_contains: ["RFC-0855p-d", "RFC-0855p-e"] # Phase 2 gate RFCs
```

## Layer direction (per [[cipherocto-design-principles]])

- Tests live with their consumers (RFC-0011-d §11 + RFC §Mission Decomp M8 row)
- Vectors authored in impl guide per RFC §Mission Decomp M8 row (canonical location)

## Backward compat

Additive: 11 new test vectors + 1 new integration test file. NO existing tests modified. NO schema version bump.

## Risk

- **Test vector drift**: YAML format may diverge from impl over time. Mitigation: vectors reference §11 explicitly; assertion harness checks field presence + types.
- **assert_cmd build dependency**: requires `octo` binary built first. Mitigation: existing RFC-0011 test pattern uses `[workspace.test-helpers]` crate; reuse.
- **Test runtime speed**: 11 vectors × assert_cmd invocation may be slow. Mitigation: parallelize via `cargo test --jobs N`; mark slow tests `#[ignore]` if >5s each.
- **Vector ID drift**: TV-* IDs must match RFC §11 canonical scheme. Mitigation: IDs validated against §11 table on each commit.

## Notes

- 11 vectors per RFC §11 canonical distribution (3+3+4+1); NOT 5 CLI + 6 substrate split
- Vectors authored in `docs/07-developers/octo-cli-implementation-guide.md` per RFC §Mission Decomp M8 row (NOT separate test/*.yaml files)
- Vector IDs: TV-RL-* / TV-RS-* / TV-RX-* / TV-RP-* (canonical §11 scheme)

## Cross-references

- RFC-0011-d §11 Test Vectors (canonical 3+3+4+1 distribution)
- RFC-0011-d §Mission Decomposition M8 row (vector location + count)
- RFC-0011 §Output Envelope (envelope shape for vectors)
- RFC-0011-d §Exit Codes (TV-RX-3 exit 31, TV-RX-4 exit 35, TV-RP-1 exit 33)
- `docs/07-developers/octo-cli-implementation-guide.md` §Test Vectors (vector location)

## Claimant

@unassigned (mission lifecycle: Open)
