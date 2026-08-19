# Mission: 0105-v2 — Role-token canonicalization (underscore → hyphen form, 8 files, 26 sites)

## Status

**OPEN 2026-08-19 (@mmacedoeu).** Filed per Round-3 adversarial review
finding (defect 7): legacy underscore-form role-tokens (`OCTO_W`,
`OCTO_A`) appear in 8 files (26 sites) across the codebase. Canonical
form per RFC-0105 v2.0 §Asset ID Derivation is hyphen (`OCTO-W`,
`OCTO-A`) — same enum at `determin/src/asset_id.rs:54`.

**Key constraint**: TV-V1-01..10 in `crates/octo-vault/tests/test_vectors.rs`
(lines 65-134 + lines 166-175 hex constants) use legacy underscore form
with **byte-exact pinned hex**. Hex MUST be regenerated after role_token
string change (asset_id bytes change → vault_id bytes change → hex
constants change).

## What will land

- **8 files modified**: rename role-tokens + regenerate 10 hex constants + RFC-0105 v2.1 amendment row.
- **26 sites total** (see Dependency edges for per-file line list).
- **10 hex constants regenerated** in `test_vectors.rs:166-175`.
- **RFC-0105 §Version History v2.1 row** added documenting canonicalization.
- **TV-D9 + TV-V1-MATRIX** already use canonical hyphen form per RFC-0105 v2.0; no change needed.

## RFC

- Primary: RFC-0105 v2.0 → v2.1 (small canonicalization amendment; no spec text change)
- Canonical form source: `determin/src/asset_id.rs:54` `ROLE_TOKENS` enum (hyphen form)

## Dependency edges

| File                                                                                                       | Sites       | Action                                                              |
| ---------------------------------------------------------------------------------------------------------- | ----------- | ------------------------------------------------------------------- |
| `crates/octo-vault/tests/test_vectors.rs` lines 65-134 (`TV_V1_VECTORS`)                                    | 10 + 10     | `role_token: "OCTO_W"` → `"OCTO-W"` / `"OCTO_A"` → `"OCTO-A"`; rename fixture `name:` strings |
| `crates/octo-vault/tests/test_vectors.rs` lines 166-175 (`TV_V1_01..10` hex)                                | 10 hex      | REGENERATE via `cargo run --example capture_tv_v1`                   |
| `crates/octo-vault/src/lib.rs` doctests lines 453, 466, 481, 482, 519, 531                                 | 6 doctests  | `AssetId::derive("OCTO_W"/"OCTO_A")` → `"OCTO-W"`/`"OCTO-A"`         |
| `crates/octo-vault/examples/capture_tv_v1.rs` lines 28, 34, 40, 46, 52, 58, 64, 70, 76                     | 9 examples  | `role_token: "OCTO_W"`/`"OCTO_A"` → hyphen form; rename `name:` strings |
| `crates/octo-vault/tests/apply_migrations.rs` line 191                                                     | 1 site      | `AssetId::derive("OCTO_W")` → `"OCTO-W"`                            |
| `crates/octo-cap-macaroon/src/caveat/payment.rs` line 62                                                   | 1 docstring | `(integer micro-OCTO_W)` → `(integer micro-OCTO-W)`                 |
| `crates/quota-router-storage/src/slash_store.rs` line 44                                                   | 1 docstring | `(integer micro-OCTO_W counts)` → `(integer micro-OCTO-W counts)`   |
| `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs` lines 66, 210, 1229                            | 3 comments  | `1 OCTO_W` / `micro-OCTO_W` / `OCTO_W times 1e6` → hyphen form       |
| `rfcs/accepted/numeric/0105-asset-id-derivation.md` §Version History                                       | 1 row       | Add v2.1 row documenting canonicalization (no spec text change)     |

No new cyclic edges. Pure canonicalization; canonical form unchanged, only
the form of fixture inputs is normalized.

## Problem

`RFC-0105 v2.0 §Asset ID Derivation` declares the canonical 9-role-token
enumeration at `determin/src/asset_id.rs:54`:
```rust
pub const ROLE_TOKENS: &[&str] = &[
    "OCTO-A", // AI Compute
    "OCTO-B", // Bandwidth
    ...
    "OCTO-W", // AI Wholesale
];
```

The hyphen form is canonical. Production code calls
`asset_id_for("OCTO-W")` → BLAKE3-256 of `"cipherocto/asset/v1/OCTO-W"`.

Legacy underscore form (`"OCTO_W"`, `"OCTO_A"`) produces DIFFERENT bytes
(BLAKE3 of `"cipherocto/asset/v1/OCTO_W"` ≠ BLAKE3 of `"cipherocto/asset/v1/OCTO-W"`).
Round-3 review found 26 sites across 8 files using legacy form. TV-V1-01..10
fixtures in `test_vectors.rs` lock to the legacy form's bytes.

## Acceptance Criteria

- AC-1: All 10 `TV_V1_VECTORS` fixtures use canonical hyphen form (`OCTO-W` / `OCTO-A`).
- AC-2: All 10 fixture `name:` strings updated (`_octo_w_` → `_octo-w_` etc.).
- AC-3: All 10 `TV_V1_0X` hex constants regenerated + pinned (canonical form bytes).
- AC-4: All 6 doctests in `crates/octo-vault/src/lib.rs` use canonical form.
- AC-5: `crates/octo-vault/examples/capture_tv_v1.rs` uses canonical form + updated name strings.
- AC-6: `crates/octo-vault/tests/apply_migrations.rs:191` uses canonical form.
- AC-7: All 3 docstring/comment sites use canonical form (payment.rs, slash_store.rs, tv_0862_spend_ledger.rs).
- AC-8: RFC-0105 §Version History v2.1 row added documenting canonicalization.
- AC-9: `cargo test -p octo-vault --test test_vectors` → 15/15 green.
- AC-10: `cargo test -p octo-vault --doc` → all doctests green.
- AC-11: `cargo test -p octo-cap-macaroon --lib` → all green (no regression).
- AC-12: `cargo test -p quota-router-storage --lib` → all green.
- AC-13: `cargo clippy -p octo-vault -p octo-cap-macaroon -p quota-router-storage --all-targets -- -D warnings` clean.
- AC-14: `cargo fmt --all -- --check` clean.
- AC-15: Verification grep `grep -rEn 'OCTO_[A-Z]' crates/octo-vault crates/octo-cap-macaroon crates/quota-router-storage crates/quota-router-core` returns zero hits.

## Hex regen procedure

1. Update role_token strings + name strings in `crates/octo-vault/examples/capture_tv_v1.rs` FIRST.
2. Run `cargo run -p octo-vault --example capture_tv_v1` — prints 10 vault_id hex values.
3. Copy new hex into `crates/octo-vault/tests/test_vectors.rs:166-175`.
4. Update role_token strings + name strings in `TV_V1_VECTORS` block (lines 65-134).
5. Run `cargo test -p octo-vault --test test_vectors` — all 15 TV must pass.
6. Run `cargo test -p octo-vault --doc` — all 6 doctests must pass.

## Risks

- **Hex regen byte mismatch** (LOW): capture binary computes via canonical
  `vault_id` helper; same toolchain revision guarantees byte equality.
- **Doctest break** (LOW): doctests at lib.rs:453+ are run via `cargo test --doc`.
- **RFC-0105 v2.1 row** (LOW): pure documentation; no spec text change.

## Out of scope (NOT this mission)

- Defect 6 + N4 — M1 + M3 separate.
- Mission 0111 (DECIMAL/DFP) — off-limits per user constraint.

## Termination condition

- All 8 files updated per AC-1 through AC-7.
- RFC-0105 v2.1 row per AC-8.
- All tests green per AC-9..AC-12.
- Clippy + fmt clean per AC-13..AC-14.
- Verification grep clean per AC-15.
- Memory card created + MEMORY.md updated.
- Mission file `git mv` to `missions/claimed/0105-v2-role-token-canonicalization.md`.
- 1 commit: `chore(crate-octo-vault): 0105-v2 — canonicalize role-token form (8 files, 26 sites)`.
- NO push performed — push awaits user instruction per `feedback_initiation_user_only`.