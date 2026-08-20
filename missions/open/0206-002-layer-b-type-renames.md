---
name: 0206-002-layer-b-type-renames
description: Open 2026-08-20 v2.0; RFC-0206 v2.0 §Layer B TYPE Renames — 29 sites across quota-router-storage (26) + octo-vault (3) rename `stoolap::Database` → `octo_storage_core::Database`. Closes TV-0206-A7 quota-router-storage + octo-vault paths; remaining paths closed by 0206-003.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-99c2545bccf7
  created: 2026-08-20T00:00:00.000Z
  v: "2.0"
  supersedes: v1.0
  depends_on:
    - 0206-001-substrate-newtype
    - 0206-003-trait-moves
    - 0206-004-adapter-crates
    - RFC-0205
    - RFC-0206
---

# Mission `0206-002-layer-b-type-renames` v2.0 — OPEN 2026-08-20

## v2.0 Changes from v1.0

R1 findings applied:

- HIGH: 29-site count split — 17 explicit sites + 12 deferred to `0206-002b-layer-b-type-renames-audit` BLOCKED-ON-AUDIT
- HIGH: `holder_registry.rs:33` removed from list (it's `pub trait HolderRegistry`, not `stoolap::Database`); owned by `0206-003-trait-moves`
- HIGH: Scope "must drop" + AC "no NEW stoolap deps" — wording aligned
- HIGH: Ordering pinned — `0206-002` runs FIRST (renames in place); `0206-003` runs AFTER (moves renamed files)
- MED: 0206-003 added to `depends_on:` (TV-0206-A7 scope covers `octo-cap-macaroon/src`)
- MED: 0206-004 added to `depends_on:` (`cargo build --workspace --all-targets` requires adapter crates)
- MED: `From<Database>` escape-hatch site audit gate added
- MED: `Deref` surface audit gate added
- MED: 29 SITES ≠ 29 FILES clarified (~8 files)
- MED: TV-0206-A7 reworded — partial closure (2 of 4 paths)
- LOW: call-site enumeration gate added
- LOW: RFC-0205 v2.0 cross-ref added per BLUEPRINT.md rule 5

## Scope

Apply RFC-0206 v2.0 §Layer B TYPE Renames across 17 EXPLICIT sites + 12 DEFERRED sites (audit-gated). Closes TV-0206-A7 for `quota-router-storage` + `octo-vault` paths; remaining 2 paths closed by `0206-003-trait-moves`.

### Explicit Sites (17)

**quota-router-storage (14 sites, ~7 files):**

- `src/ask_repo.rs:42`, `:189` (2 sites)
- `src/slash_store.rs:67`, `:289` (2 sites)
- `src/migrations.rs:14` (1 site)
- `src/spend_ledger.rs:48`, `:121` (2 sites)
- `src/settlement_event_repo.rs:36`, `:96` (2 sites)
- `src/stoolap_did_registry.rs:201` (1 site — `:139` excluded; impl moves per `0206-003`)
- `src/holder_registry.rs:81`, `:155` (2 sites — `:33` excluded; trait decl moves per `0206-003`)
- `src/holder_registry.rs` TYPE-rename sites will be re-evaluated after `0206-003-trait-moves` completes the move

**octo-vault (3 sites):**

- `src/lib.rs:351`, `:378`, `:395`

### Deferred Sites (12) — BLOCKED-ON-AUDIT

12 additional sites in quota-router-storage TBD on per-file audit. Tracked at mission `0206-002b-layer-b-type-renames-audit` (to be filed after v2.0 lands; audit-gated; cannot claim until per-file site table produced).

### Rename Pattern

- `stoolap::Database` → `octo_storage_core::Database` (TYPE positions only: function arg, field type, return type, struct generic parameter, trait method signature)
- `Arc<stoolap::Database>` → `Arc<octo_storage_core::Database>`
- `&stoolap::Database` → `&octo_storage_core::Database`
- `&mut stoolap::Database` → `&mut octo_storage_core::Database`

### Cargo.toml Deps Update

- Each renamed crate MUST drop `stoolap` direct dep (verified by AC gate line 47)
- Each renamed crate ADDS `octo-storage-core = { path = "../octo-storage-core" }`
- Exception: typed-query allowlist sites per RFC §Substrate Newtype Refactor — use `From<Database> for stoolap::Database` escape hatch

## Acceptance Criterion

- TV-0206-A7 partial gate: `rg 'stoolap::Database' crates/quota-router-storage crates/octo-vault/src 2>/dev/null | wc -l` equals 0
- `From<Database>` escape-hatch site list produced (file:line) BEFORE rename; committed as audit gate
- `Deref` surface audit: list every method/property access through `Deref<Target = stoolap::Database>`; pre-rewrite baseline + post-rewrite delta
- 17 explicit sites renamed; verified by `rg 'stoolap::Database' crates/quota-router-storage crates/octo-vault/src` exits 0
- `rg '^\s*stoolap\s*=' crates/quota-router-storage/Cargo.toml crates/octo-vault/Cargo.toml` exits 1 (no NEW stoolap deps added)
- `cargo build -p quota-router-storage -p octo-vault` green (narrowed from workspace to avoid `0206-004` race)
- `cargo test -p quota-router-storage -p octo-vault --lib` green
- `cargo clippy -p quota-router-storage -p octo-vault --all-targets --features full -- -D warnings` green (per `quota-router-core-feature-mutex` memory; `--all-features` ALWAYS fails)
- `cargo fmt --all -- --check` green

## Files / Artifacts

- Edit: `crates/quota-router-storage/Cargo.toml` (add `octo-storage-core` dep; remove `stoolap` dep)
- Edit: `crates/octo-vault/Cargo.toml` (add `octo-storage-core` dep; remove `stoolap` dep)
- Edit: ~8 source files (17 explicit TYPE rename sites)
- New: `docs/audits/0206-002-escape-hatch-audit.md` (From<Database> site list)
- New: `docs/audits/0206-002-deref-surface-audit.md` (Deref access site list)

## Cross-references

- RFC-0206 v2.0 §Layer B TYPE Renames
- RFC-0206 v2.0 TV-0206-A7 (partial closure; full closure requires `0206-003`)
- RFC-0206 v2.0 §Substrate Newtype Refactor (`From<Database>` escape hatch)
- RFC-0206 v2.0 §Cargo.toml Cross-Cuts
- RFC-0205 v2.0 (coupled pair per BLUEPRINT.md §Dependency Validation Rules → 2-Cycle Atomic Promotion rule 5)
- Mission `0206-001-substrate-newtype` (substrate Database type)
- Mission `0206-003-trait-moves` (moves renamed files AFTER this mission)
- Mission `0206-004-adapter-crates` (workspace build gate)

## Out of scope

- Substrate newtype impl (owned by `0206-001`)
- HolderRegistry trait move (owned by `0206-003`)
- StoolapDidRegistry impl move (owned by `0206-003`)
- 5 adapter crates (owned by `0206-004`)
- 12 deferred sites (owned by `0206-002b-layer-b-type-renames-audit` BLOCKED-ON-AUDIT)

## Dependencies

- `0206-001-substrate-newtype` (substrate Database type must exist)
- `0206-003-trait-moves` (race: must move AFTER this mission's rename)
- `0206-004-adapter-crates` (workspace build gate)
- RFC-0205 v2.0 (coupled pair)
- RFC-0206 v2.0 (acceptance precondition)

## Version History

| Version | Date | Change |
|---|---|---|
| v1.0 | 2026-08-20 | Initial filing (29 sites claimed, unverifiable) |
| v2.0 | 2026-08-20 | R1 fix: 17 explicit + 12 BLOCKED-ON-AUDIT; 3 deps added; 2 audit gates added |