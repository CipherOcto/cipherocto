---
name: 0206-011b-rfc-0206-v22-amendment-stoolap-reexport
description: OPEN 2026-08-20 v1.0; RFC-0206 v2.1 → v2.2 amendment to add `pub mod stoolap` re-export block in substrate (ResultRow + ApiTransaction + Rows + Error). Resolves D1 deviation in 0206-002 v3.0 + 0206-008 audits. Unblocks 0206-008b consumer dep drop. Layer A semver-major change.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-99c2545bccf7
  created: 2026-08-20T00:00:00.000Z
  v: "1.0"
  supersedes: null
  depends_on:
    - 0206-001-substrate-newtype
    - RFC-0206
phase: 1.9 substrate re-export prerequisite
layer: A
rfc_authority: RFC-0206 v2.4
tvs:
  - TV-0206-A9
---

# Mission `0206-011b-rfc-0206-v22-amendment-stoolap-reexport` v1.0 — OPEN 2026-08-20

## Why this mission exists

Phase 1.9 terminal TV sweep (task #878) found **TV-0206-A9(b) FAILS** at 13 vs ≤ 5 stoolap deps workspace-wide. The plan §Verification gate `rg -l '^\s*stoolap\s*=' crates/*/Cargo.toml | wc -l` ≤ 5 is the load-bearing constraint.

**Root cause**: 0206-002 v3.0 (commit `2e25668b`) + 0206-008 (commit `927008d6`) both landed with explicit **D1 deviation** documented in their audits:

> "stoolap direct dep RETAINED in consumer crates — substrate v3.0 does NOT yet re-export `stoolap::ResultRow` / `stoolap::ApiTransaction` / `stoolap::Rows` / `stoolap::Error`. Consumer crates need direct dep because they decode rows returned by `Database::execute_checked`."

The substrate's `Database::execute_checkedTypedStatement` returns `Result<Rows, SubstrateError>` (or similar typed-surface result), but the underlying row type is `stoolap::ResultRow` — which the substrate does NOT re-export. Therefore consumers must import `stoolap::ResultRow` directly to type their decoding code, which forces a direct `stoolap` Cargo.toml dep.

**Resolution**: Substrate MUST re-export these 4 types so consumers can drop direct `stoolap` dep entirely. Per RFC-0206 v2.1 §Substrate Newtype Refactor (the RFC already mandates the substrate as the abstraction layer — consumers should NOT know about `stoolap::ResultRow`).

## Scope (RFC only, no new crate)

RFC-0206 v2.1 → v2.2 changes:

1. **Add §Substrate Re-export Block** section: substrate exposes `pub mod stoolap` containing:
   - `ResultRow` (alias for `stoolap::ResultRow`)
   - `ApiTransaction` (alias for `stoolap::ApiTransaction`)
   - `Rows` (alias for `stoolap::Rows`)
   - `Error` (alias for `stoolap::Error`)

2. **Update §Cargo.toml Templates Layer A** table to add `pub mod stoolap` alongside `pub mod migrations`. The 8 top-level `pub use` cap is UNCHANGED (the re-export block is a `pub mod`, not 4 top-level `pub use`).

3. **Update §RFC Process Audit Condition 2**: "Layer B consumer crates drop direct `stoolap` dep" — currently in D1 deviation status, transition to PASS after 0206-008b lands.

4. **Add §Migration Order follow-on**: substrate v2.2 `pub mod stoolap` re-export block is the prerequisite for 0206-008b to drop direct `stoolap` dep in 13 consumer crates.

5. **Version bump**: v2.1 → v2.2. Layer A semver-major.

## Critical files

**MODIFY (0206-011b — RFC body only)**:
- `rfcs/draft/0206-cipherocto-octet-storage.md` — sections + Version History row + v2.1→v2.2 changelog

**NEW (0206-001 v3.0b — substrate re-export block)**:
- `crates/octo-storage-core/src/stoolap.rs` — module wrapper exposing the 4 re-exports
- `crates/octo-storage-core/src/lib.rs` — add `pub mod stoolap;` declaration (8 top-level `pub use` cap unchanged)

**MODIFY (0206-001 v3.0b)**:
- `crates/octo-storage-core/Cargo.toml` — Layer A semver-major version bump (1.0.0 → 2.0.0)

**MODIFY (0206-008b — 13 consumer crates drop direct dep)**:
- All 13 crates identified in TV-A9(b) audit: `quota-router-storage`, `octo-vault`, `octo-core`, `octo-adapter-whatsapp`, `octo-ident-storage`, `octo-reputation`, `octo-whatsapp`, `octo-matrix-session-store`, `octo-storage-core`, `quota-router-cli`, `octo-adapter-telegram-mtproto`, `quota-router-core`, `quota-router-sm-engine`
- For each: remove `stoolap = { ... }` line from `[dependencies]`; update `use stoolap::...` imports to `use octo_storage_core::stoolap::...` (with optional `#[cfg(feature = "stoolap-fork")] optional-feature gate` for adapter crates that already have it)

## Existing functions / patterns to reuse

- `octo_storage_core::typed_statement` (existing `pub mod`) — pattern reference for `pub mod stoolap` block
- `octo_storage_core::migrations` (existing `pub mod`) — pattern reference for nested pub-use surface
- 0206-002 v3.0 audit `docs/audits/0206-002-layer-b-type-renames-audit.md` — D1 deviation commentary (line 64-66)
- 0206-008 audit `docs/audits/0206-008-layer-b-type-renames-expansion-audit.md` — D1 deviation commentary (line 101-114)

## Verification

```bash
# 0206-011b AC gates (RFC body)
grep -c "^pub mod stoolap" rfcs/draft/0206-cipherocto-octet-storage.md  # ≥ 1
grep -c "v2.1 → v2.2" rfcs/draft/0206-cipherocto-octet-storage.md  # ≥ 1

# 0206-001 v3.0b AC gates (substrate re-export block)
rg -n "^pub mod stoolap" crates/octo-storage-core/src/lib.rs  # exactly 1
rg -c '^pub use\b' crates/octo-storage-core/src/lib.rs | rg -v _legacy_  # still 8 (canonical top-level)
cargo doc -p octo-storage-core --no-deps  # re-exports resolve

# 0206-008b AC gates (13 consumer crates drop direct dep)
rg -l '^\s*stoolap\s*=' crates/*/Cargo.toml | wc -l  # ≤ 5 (Layer A pin only)

# Phase 1.9 sweep redo
cargo build --workspace --all-targets
cargo test --workspace --lib
cargo clippy --workspace --all-targets --features full -- -D warnings
cargo fmt --all -- --check
```

## Risks

1. **[CRIT]** Substrate re-export block subsumes `stoolap` namespace into own crate. If `stoolap` crate ever adds new public types that consumers need, the substrate MUST keep pace. Mitigation: §Migration Order mandates substrate-API parity with stoolap's public surface for the ≥ 6-month transition window.
2. **[HIGH]** 13 Cargo.toml dep drops are mechanical but may surface downstream test breakage (e.g., tests that destruct `stoolap::ResultRow` directly). Mitigation: full workspace test run after each crate drop.
3. **[MED]** `octo-core` Cargo.toml `stoolap = ...` line is in `description` not `[dependencies]` — verify rg doesn't catch it as a false positive.
4. **[MED]** `octo-storage-core` Cargo.toml IS one of the 13 sites (Layer A substrate). It must keep the `stoolap` dep (it's the substrate!). The gate ≤ 5 includes 1 for the substrate + 4 for Layer A internal pins (e.g., adapter crate optional-feature gates). Net: 13 → 5 acceptable.

## Out of scope (NOT this mission)

- New typed query DSL (Phase 2 typed-query expansion per RFC-0206 v2.1 §Implementation Phases 2.1)
- Wholesale rewrite of crate-internal `stoolap::Rows` decoding code (single-line import substitution per crate)
- Push to remote — push + remote writes await user instruction per `feedback_initiation_user_only`

## Termination conditions

- `missions/open/0206-011b-rfc-0206-v22-amendment-stoolap-reexport.md` filed
- `0206-001 v3.0b` substrate PR landed (re-export block + version bump)
- `0206-008b` consumer dep drop PR landed (13 crates → 5)
- Phase 1.9 terminal sweep re-run: TV-0206-A9(b) PASS (5 sites ≤ 5)
- All other TV-0206-A1..A14 gates remain PASS
- Clippy + fmt clean
- Memory cards for 0206-011b + 0206-001 v3.0b + 0206-008b + MEMORY.md updated
- Mission files `git mv` to `missions/claimed/` via chore(missions) commits
- NO push performed — push + remote writes await user instruction per `feedback_initiation_user_only`

## Version History

| Version | Date       | Change                                                          |
| ------- | ---------- | --------------------------------------------------------------- |
| v1.0    | 2026-08-20 | Initial filing; Phase 1.9 sweep blocker for TV-0206-A9(b); 13 → 5 |
