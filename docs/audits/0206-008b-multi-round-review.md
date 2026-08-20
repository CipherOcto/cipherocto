# 0206-008b Multi-Round Post-Landing Review

**Date:** 2026-08-20
**Scope:** 11 commits across missions 0206-011b (RFC), 0206-001 v3.0b (substrate), 0206-008b (consumer dep drop)
**Reviewer:** Substrate redesign cascade R1-R4
**Result:** 5 issues found, 5 fixed, 4 commits added. TV-0206-A9(b) gate green (4 ≤ 5).

## Round 1 — RFC-0206 v2.2 body (commits a96c50ce + f064a653)

| # | Sev | Location | Defect | Fix commit |
|---|-----|----------|--------|------------|
| 1 | CRIT | `rfcs/.../0206-octo-storage-split.md:442-453` | **Duplicate Phase 3 section** — v2.2 amendment inserts new Phase 3 (lines 442-447) but old v2.1 Phase 3 (lines 449-453) not removed. Both `**Phase 3 — Legacy removal**` headings present. | f96d26aa |
| 2 | CRIT | `rfcs/.../0206-octo-storage-split.md:116-119, 381, 397, 429` | **File path mismatch** — RFC says `crates/octo-storage-core/src/stoolap_reexport.rs` in 5 sites but actual code is `stoolap.rs` (per 0206-001 v3.0b commit 026c99f0). | f96d26aa |
| 3 | HIGH | `rfcs/.../0206-octo-storage-split.md:5, 96, 122, 131, 204, 394, 402, 406, 644, 645` | **Line refs in non-code prose** — 8 sites cite `audit lines 64-66`, `audit lines 101-114`, `lines 99-110`, `line 244` (×4), `line 187`, `line 440`, `lines 46-52`. Violates CLAUDE.md §Core Engineering Principles + `no-line-refs-anywhere` memory card. | f96d26aa |

## Round 2 — Re-export count mismatch

| # | Sev | Location | Defect | Fix commit |
|---|-----|----------|--------|------------|
| 4 | HIGH | `rfcs/.../0206-octo-storage-split.md:8 sites` | **Re-export count 4 vs 5** — RFC body cited "4 nested re-exports" of `stoolap` types (`ResultRow`, `ApiTransaction`, `Rows`, `Error`) but actual substrate code (commit 026c99f0) re-exports 5 types — `Value` added per consumer reality (octo-whatsapp + octo-adapter-telegram-mtproto + octo-reputation all use `stoolap::Value`). 8 sites across Status header, §Cargo.toml Templates Layer A code example, §Substrate Re-export Block prose + code example, §Migration Order Phase 2.5, condition 2 row, v2.2 Version History row. | c4225ed1 |

## Round 3 — Cap carve-out + deprecation metadata

| # | Sev | Location | Defect | Fix commit |
|---|-----|----------|--------|------------|
| 5 | HIGH | `rfcs/.../0206-octo-storage-split.md` + `crates/octo-storage-core/src/lib.rs` | **Production-surface cap carve-out ambiguity** — RFC §Substrate Newtype Refactor "≤ 8 cap" rule applies to production surface but actual code has 14 top-level `pub use` (8 production + 6 `_legacy_*` aliases). RFC did not document the carve-out. Substrate code additionally had stale deprecation `since = "1.0.0"` + note "removed in v2.0" — but substrate version is currently 2.0.0, so the "removed in v2.0" promise was already broken. | dbeacb54 |

## Round 4 — 0206-008b consumer dep drop verification

| # | Sev | Crate | Finding |
|---|-----|-------|---------|
| 6 | LOW | octo-reputation | `pub use stoolap::StoolapReputationStore` (store/mod.rs:19) ambiguous: could read as upstream-or-local. Local `mod stoolap` (line 16) inside the crate. No actual upstream dependency. Cosmetic only. |

**Verification:**

- `cargo check --workspace --all-targets` — exit 0 (2m 42s)
- `cargo clippy --workspace --all-targets --features full -- -D warnings` — exit 0 (1m 17s)
- `cargo fmt --all -- --check` — exit 0
- `rg -l '^\s*stoolap\s*=\s*\{' crates/*/Cargo.toml | wc -l` — **4** (TV-A9(b) ≤ 5 PASS)

**Breakdown of 4 remaining direct deps:**

| Crate | Reason | Approved |
|-------|--------|----------|
| `octo-storage-core` | Layer A substrate (§Substrate owns the dep) | ✓ RFC v2.2 §Cargo.toml Cross-Cuts |
| `octo-adapter-whatsapp` | DataType Layer B internal pin | ✓ per 0206-008b audit |
| `octo-adapter-telegram-mtproto` | DataType Layer B internal pin | ✓ per 0206-008b audit |
| `quota-router-core` | DataType/pubsub Layer B internal pin | ✓ per 0206-008b audit |

## Commits produced by review

| Commit | Mission | Description |
|--------|---------|-------------|
| `f96d26aa` | 0206-011b | RFC R1: duplicate Phase 3 + file path + line refs |
| `c4225ed1` | 0206-011b | RFC R2: re-export count 4 → 5 |
| `dbeacb54` | 0206-001-v3.0b | RFC R3 + substrate deprecation metadata: production-surface cap carve-out + `since = "2.0.0"` |
| `caef6797` | 0206-008b | quota-router-core import reorder (style) |

## Architectural notes

1. **Trait-vs-struct ApiTransaction aliasing** — substrate `pub use stoolap::ApiTransaction` (the STRUCT per fork lib.rs:192) replaces prior `pub use stoolap::Transaction as ApiTransaction` which would have aliased the `storage::Transaction` TRAIT. Comment in stoolap.rs:23-31 documents the trap. The comment cites line 144/192 of the fork — code-exception per `no-line-refs-anywhere` card.

2. **`pub mod stoolap` re-export block pattern** — matches `pub mod migrations` (3 nested) and `pub mod _legacy` (planned but rolled back — 6 `pub use as _legacy_*` at top level instead due to external consumer breakage). RFC documents carve-out at §Cargo.toml Templates Layer A.

3. **Cap rule semantics** — clarified that "≤ 8 pub-use cap" applies to production surface only; `_legacy_*` aliases are deprecated carve-out for ≥ 6-month coexistence window per §Migration Order. The 14-statement actual count is documented intent.

## Out of scope (deferred to later missions)

- `pub mod _legacy` refactor — would require updating 10+ external consumer sites that import `_legacy_*` from top level. Defer to 0206-001 v3.0c.
- RFC v3.0 (Phase 3 legacy removal) — ≥ 2027-02-20 per RFC §Migration Order.
- Cargo.lock fork-pin to `80fd701` (per [[feedback_initiation_user_only]]) — `0900-d2` blocker awaiting user push.

## Conclusion

**0206-008b batch reviewed across 4 rounds. 5 issues found, 5 fixed. TV-0206-A9(b) gate green (4 ≤ 5). All builds green.**

Push to remote awaits explicit user instruction per `feedback_initiation_user_only`.
