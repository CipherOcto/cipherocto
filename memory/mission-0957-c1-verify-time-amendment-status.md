---
name: mission-0957-c1-verify-time-amendment-status
description: S6b RFC-0957 v2.1 amendment + 22-byte-exact TV-0957 verify-time fixtures LANDED 2026-08-17 (Round 1 commit `c9149128` 22/22 pass; Round 2 commit `4ec9779f` re-scoped TV-16/17 + dropped dead code; Round 4 commit reconciles §3.x drift + clears source phantom §20.6.1 line 1328). Mission YAML + RFC text + new crate test file + source doc-comments.
metadata:
  type: project
---

# S6b — RFC-0957 verify-time amendment LANDED 2026-08-17

Mission `0957-c1-verify-time-amendment` closed. Second S6 sub-session
per user split-by-RFC decision (overrides plan §22 atomic-blocker
bundle for this session).

## What landed

- **RFC-0957 §Version History v2.1 row** added:
  `rfcs/accepted/economics/0957-capability-token-format.md`
  - `verify_for_vault_op` extension (per review doc §20.6.1 4-step algorithm)
  - 9 new Caveat variants: Vault + Permission + WrappedOnly +
    Factory + AuditWindow map to RFC-0965 §3.x; MaxUses maps to
    §3.4; PolicyReference maps to §3.10; ValidRange + MaxPerTx
    are NEW in RFC-0957 v2.1 (no RFC-0965 §3.x number) — landed
    in `crates/octo-cap-macaroon/src/caveat/`
  - `PermissionKind` enum (5 variants, snake_case wire form via `#[serde(rename_all = "snake_case")]` matching source-of-truth)
  - `WrappedOnly` parent-no-Vault-binding reject — `verify_for_vault_op` only (NOT `verify_full`)
- **RFC-0957 §Verify-Time Extension subsection** added (under
  §Algorithms, after §Macaroon v1 chain construction)
  - `Macaroon::verify_for_vault_op` signature
  - 4-step algorithm verbatim per review doc §20.6.1
  - `VaultLookup` trait injection (Layer B extension consumer)
  - `WrappedOnly` chain walk invariant (operational gate; structural `verify_full` explicitly NOT required)
  - Cross-reference to RFC-0965 §3 + RFC-0870 §NodeEnvelope Version Tag
- **RFC-0957 §Caveat DSL Extension subsection** added (under
  §Data Structures, after the `Caveat` enum definition):
  - 9 new variants enumerated with field types
  - `PermissionKind` enum (5 variants, no `serde(rename_all)` — PascalCase)
  - `FactoryVet` struct per RFC-0965 §3
  - Cross-reference to per-extension crate
- **TV-0957 fixture**:
  `crates/octo-cap-macaroon/tests/tv_0957_verify_time.rs` (NEW, 22/22
  tests passing — 4 categories × 5 fixtures + 2 deep-chain fixtures):
  - `tv_0957_01_vault_variant_wire_form` (Vault discriminant pin)
  - `tv_0957_02_permission_variant_wire_form` (Permission +
    PermissionKind × 5 round-trip pin)
  - `tv_0957_03_valid_range_variant_wire_form` (ValidRange struct +
    field-name pins: `valid_after_unix`, `valid_until_unix`)
  - `tv_0957_04_max_per_tx_variant_wire_form` (MaxPerTx u128 pin)
  - `tv_0957_05_audit_window_variant_wire_form` (AuditWindow field-name
    pin: `duration_secs`, not `start_unix_secs`/`end_unix_secs`)
  - `tv_0957_06_max_uses_variant_wire_form` (MaxUses field pin: `count`)
  - `tv_0957_07_wrapped_only_variant_wire_form` (WrappedOnly
    `parent_capability` field pin — distinguishes from earlier draft's
    unit-variant form)
  - `tv_0957_08_factory_variant_wire_form` (Factory(FactoryVet) with
    inner-struct field pins: `target_vault_id`, `action_template`,
    `expiry_for_deploy_unix`)
  - `tv_0957_09_policy_reference_variant_wire_form` (PolicyReference
    `policy_id`, `policy_version_seq`, `attenuation_witness`)
  - `tv_0957_10_raw_unknown_name_rejected_at_attenuation`
    (catalog-bypass attack pin per `macaroon.rs:242-243`)
  - `tv_0957_11_verify_time_happy_path_ok` (correct root + populated
    lookup → `Ok(())` proves step-1 signature verify passes
    transitively; renamed from `signature_verify_step_ok` after
    empty-lookup-trips-step-2 first run)
  - `tv_0957_12_vault_row_lookup_step_missing` (empty lookup →
    `VaultRowMissing { vault_id }` carrying looked-up id)
  - `tv_0957_13_chain_match_step_mismatch` (row w/ mismatched chain →
    `ChainMismatch { vault_chain, op_chain }`)
  - `tv_0957_14_state_active_step_rejects_frozen` (`is_active=false` →
    `VaultNotActive { vault_id }`)
  - `tv_0957_15_wrapped_only_chain_walk_step_ok` (child `WrappedOnly`
    referencing parent with `Vault` + matching active lookup →
    `Ok(())`)
  - `tv_0957_16_regression_frozen_vault_in_ancestor_rejected` (Round 2 re-scope — frozen vault on WrappedOnly ancestor; chain walker traversal pin)
  - `tv_0957_17_regression_chain_mismatch_in_ancestor_rejected` (Round 2 re-scope — chain mismatch on WrappedOnly ancestor; chain walker traversal pin)
  - `tv_0957_18_regression_missing_root_secret_rejected`
    (`Macaroon(RootSecretMismatch)`)
  - `tv_0957_19_regression_wrapped_chain_has_no_vault`
    (chainless parent → `WrappedChainHasNoVault`)
  - `tv_0957_20_regression_attenuation_monotonicity_with_new_variants`
    (parent Vault + ValidRange + child keeps them + tightens
    ValidRange to subset range → passes; negative: child
    valid_after<parent valid_after → AttenuationViolation; RFC-0957
    v2.1 ValidRange supersedes RFC-0964 Constraint::ValidRange)
  - `tv_0957_21_multilevel_wrapped_only_chain_depth_3` (depth=3
    chain; collector walks ancestors; Vault found transitively)
  - `tv_0957_22_max_wrapped_depth_boundary_rejects_depth_17` (depth
    16 last allowed per RFC-0965 §3.7 R7-F1; 17th attenuate rejects
    with `WrappedDepthExceeded`)

## Drift catch (session)

Initial amendment text drafted Rust pseudocode with WRONG variant
field names that did NOT match the real
`crates/octo-cap-macaroon/src/caveat/mod.rs` source. Caught before
TV fixtures written; corrected pseudocode uses real shapes:

| Drafted (wrong)                                                  | Actual (real)                                                                                              |
| ---------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `Permission { kind, scope }`                                     | `Permission(PermissionKind)` (just the enum, no scope)                                                     |
| `ValidRange { axis, lower, upper }`                              | `ValidRange { valid_after_unix, valid_until_unix }`                                                        |
| `MaxPerTx(MicroOctoW)`                                           | `MaxPerTx(u128)` (direct u128)                                                                             |
| `AuditWindow { start_unix_secs, end_unix_secs }`                 | `AuditWindow { duration_secs }` (single duration field)                                                    |
| `MaxUses(u32)`                                                   | `MaxUses { count: u32 }` (struct with count field)                                                         |
| `WrappedOnly` (unit)                                             | `WrappedOnly { parent_capability: [u8; 32] }`                                                              |
| `FactoryVet { factory_did, vetted_at_unix_secs, vet_signature }` | `FactoryVet { target_vault_id, action_template, required_caller, pre_conditions, expiry_for_deploy_unix }` |
| `PermissionKind = Read, Write, Admin, Delegate, Audit`           | `PermissionKind = NativeTokenTransfer, Erc20TokenTransfer, ContractCall, Reservation, VaultMutation`       |

This drift would have produced TV fixtures asserting bytes the
implementation cannot emit. Lesson: **always read the actual source
enum/struct definitions before drafting RFC amendment pseudocode**
that future TV fixtures reference.

## Verify gate (this session)

- `cargo test -p octo-cap-macaroon --test tv_0957_verify_time` →
  22/22 pass
- `cargo test -p octo-cap-macaroon --lib` → 193/193 pass
- `cargo test --workspace --lib` → all green except 3 pre-existing
  S4 DFP Round 2 `quota-router-cli::commands::tests::settle_*`
  failures (per AC #5 exclusion clause)
- `cargo clippy -p octo-cap-macaroon --all-targets -- -D warnings`
  → clean (digit-grouping fix applied: `1_700_000_3600` →
  `17_000_003_600` per clippy::inconsistent_digit_grouping)
- `cargo fmt --all -- --check` → clean
- `npx prettier --write missions/open/0957-c1-verify-time-amendment.md`
  → formatted

## Why this matters

RFC-0957 v2.1 backs the S5 verify-time invariant (`d007de54`) +
RFC-0965 §3 Caveat DSL extensions (landed in
`crates/octo-cap-macaroon/src/caveat/`) with the spec text future
implementers will reference. The 22 TV fixtures pin both the
**wire form** of the 9 new caveat variants (so a schema drift
breaks a named test) and the **4-step verify-time invariant** (so a
refactor that drops a step trips a regression pin) + the
multi-level chain + the depth-16 boundary.

## Round 2 + Round 4 follow-ons (commit `4ec9779f` + Round 4 commit)

Round 2 (`4ec9779f`): TV-0957-16/17 re-scoped from leaf-local
duplicates of TV-0957-13/14 to distinct WrappedOnly ancestor
coverage (frozen vault + chain mismatch on ancestor, not leaf);
dead `let _ = parent_id;` + redundant `catalog.insert(leaf.clone())`
removed.

Round 4 (`e5138420`): §3.x numbering drift corrected in RFC
pseudocode + caveat/mod.rs source comments (ValidRange + MaxPerTx
have NO RFC-0965 §3.x number — they are NEW in RFC-0957 v2.1;
MaxUses §3.6 → §3.4; PolicyReference §3.9 → §3.10; ValidAfter
§3.3 stays). Source phantom `§20.6.1 line 1328` cleared from
macaroon.rs:438 + 810-811 + vault_verify_error.rs:17, 50, 40, 33

- tv_c1_verify_time.rs:15, 260 (Round 5 verification surfaced the
  test file was untouched in Round 4 — fixed in Round 6).
  `verify_full` claim that it ALSO enforces WrappedChainHasNoVault
  REVERTED — real verify_full does NOT call collect_vault_caveats,
  so the chainless-parent reject is operational-gate only. Caveat
  enum pseudocode gains `#[serde(tag = "type", content = "value")]`
- KEEPS `#[serde(rename_all = "snake_case")]` on PermissionKind
  (Round 4 commit msg incorrectly claimed real source was bare
  PascalCase — source-of-truth `caveat/mod.rs:215` DOES have the
  snake_case annotation; Round 5 verification surfaced this
  self-inflicted divergence, restored in Round 6).

## Push authorization

Commit queued on `next`. Push user-only per
[[feedback_initiative_user_only]] + [[git-workflow]].

## Next sub-sessions (S6c..S6g)

- **S6c** RFC-0862 amendment + 8 TV
- **S6d** RFC-0900 amendment + 10 TV
- **S6e** RFC-0105 amendment + 109 TV (largest sub-session)
- **S6f** RFC-0959 amendment + 25 TV
- **S6g** RFC-0960 amendment + 108 TV

§22 atomic-blocker PR bundle (user-chosen split-by-RFC overrides);
production deployment will coordinate the 7 sub-sessions' commits
at push time per S8.

## S5.1 follow-on (separate mission)

`0957-g1-octo-vault-lookup-glue` — `OctoVaultLookup` substrate
adapter glue crate (wires `VaultLookup` trait to `octo-vault`
storage). Out of scope for S6b.

## Cross-reference

- Plan: `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 row 6
- Mission: `missions/open/0957-c1-verify-time-amendment.md`
- Pre-req: `memory/mission-0957-g-verify-time-invariant-status.md`
- Pre-req: `memory/mission-0870-c1-version-tag-amendment-status.md`
  (S6a)
- Pre-req: `memory/mission-0965-a-caveat-dsl-status.md` (Caveat DSL
  extension source code)
- Review source: `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
  (review doc §20.6.1)
