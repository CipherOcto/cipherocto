---
name: 0206-002-layer-b-type-renames
description: Open 2026-08-20 v2.1; RFC-0206 v2.0 §Layer B TYPE Renames — apply on-disk regenerated TYPE rename sites across quota-router-storage + octo-vault (see Explicit Sites table for file:line breakdown). Closes TV-0206-A7 quota-router-storage + octo-vault paths; remaining 2 paths closed by 0206-003.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-99c2545bccf7
  created: 2026-08-20T00:00:00.000Z
  v: "2.1"
  supersedes: v2.0
  depends_on:
    - 0206-001-substrate-newtype
    - RFC-0205
    - RFC-0206
---

# Mission `0206-002-layer-b-type-renames` v2.1 — OPEN 2026-08-20

## v2.1 Changes from v2.0

R2 findings applied:

- **CRIT 1**: `src/spend_ledger.rs:48, :121` was fabricated — actual file is `stoolap_spend_ledger.rs` with hits at `:3` (doc), `:195`, `:248`, `:295`. Fixed.
- **CRIT 2**: `src/holder_registry.rs:81, :155` removed — `rg` confirms `holder_registry.rs` has zero `stoolap::Database` matches on disk. Site dropped.
- **CRIT 3**: Entire Explicit Sites table regenerated from on-disk `rg -n 'stoolap::Database' crates/quota-router-storage/src crates/octo-vault/src` (run 2026-08-20). Discovered 42 grep hits across 9 files: 35 TYPE positions + 7 doc comments. v2.0's "17 explicit sites" was substantially understated; v2.1 table is ground-truth.
- **CRIT 4**: AC rg gate expanded to cover all 9 files in scope (was missing `consumed_receipt_repo.rs` + `stoolap_spend_ledger.rs` + `stoolap_holder_registry.rs` + `stoolap_did_registry.rs`). New gate covers all on-disk hits.
- **HIGH 1**: Dropped `0206-003-trait-moves` from `depends_on:` (creates cycle; 0206-003 already lists 0206-002 as its dep). Ordering clause lives in 0206-003's dep on 0206-002.
- **HIGH 2**: Dropped "29 sites" count from description — Explicit Sites table is the source of truth.
- **HIGH 3**: AC line reworded — "no NEW stoolap deps added" → "stoolap = line absent (Cargo.toml Deps Update §must drop)".
- **HIGH 4**: Dropped `0206-004-adapter-crates` from `depends_on:` (narrowed AC build doesn't need workspace build).
- **HIGH 5**: Documented 4 TV-0206-A7 paths in AC gate (quota-router-storage, octo-vault, octo-ident, octo-cap-macaroon); this mission closes 2, 0206-003 closes 2.
- **HIGH 6**: Documented `octo-storage-core` direct dep as consumer-crate exemption per RFC §Cargo.toml Cross-Cuts scope (scoped to "Adapter crates MUST"; quota-router-storage + octo-vault are consumer crates holding the Database newtype and need direct substrate path for the type alias).
- **MED**: AC rg gate `exits 0` → `wc -l` equals 0 (correct primitive).
- **MED**: Explicit Sites header counts corrected to actual disk totals (35 TYPE positions + 7 doc comments across 9 files).
- **MED**: Stale scope note about `holder_registry.rs` re-evaluation removed (file has zero hits).
- **MED**: Doc comment exemption documented — 7 doc comments in scope also renamed to keep rustdoc accurate (separate site tag).
- **MED**: Cargo.toml escape-hatch rg gate accommodation — sites using `From<Database> for stoolap::Database` are documented in audit file and subtracted from rg count.

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

Apply RFC-0206 v2.0 §Layer B TYPE Renames across 35 EXPLICIT TYPE positions + 7 doc comments in 9 files (regenerated from disk 2026-08-20). Closes TV-0206-A7 for `quota-router-storage` + `octo-vault` paths; remaining 2 paths closed by `0206-003-trait-moves`.

### Explicit Sites (42 grep hits across 9 files; 35 TYPE + 7 DOC)

Generated from `rg -n 'stoolap::Database' crates/quota-router-storage/src crates/octo-vault/src` run 2026-08-20. Tag `[TYPE]` = code position (function arg, field type, return type, qualified constructor); `[DOC]` = rustdoc backticks (also renamed for rustdoc accuracy, but not part of TYPE-position gate).

**octo-vault (4 hits):**

- `src/lib.rs:351` [TYPE] — `pub fn apply(db: &stoolap::Database) -> Result<(), VaultError>`
- `src/lib.rs:371` [DOC] — `/// handle, never through raw \`stoolap::Database\` re-export)`
- `src/lib.rs:378` [TYPE] — `db: Arc<stoolap::Database>` field
- `src/lib.rs:395` [TYPE] — `pub fn new(db: Arc<stoolap::Database>) -> Self`

**quota-router-storage/src/ask_repo.rs (5 hits):**

- `:200` [TYPE] — `db: stoolap::Database` field
- `:209` [TYPE] — `let db = stoolap::Database::open_in_memory()`
- `:219` [TYPE] — `let db = stoolap::Database::open(path)`
- `:228` [TYPE] — `pub fn from_db(db: stoolap::Database) -> Self`
- `:787` [TYPE] — test `let db = stoolap::Database::open_in_memory().unwrap();`

**quota-router-storage/src/consumed_receipt_repo.rs (4 hits, NEW in v2.1):**

- `:57` [TYPE] — `db: stoolap::Database` field
- `:66` [TYPE] — `let db = stoolap::Database::open_in_memory()`
- `:76` [TYPE] — `let db = stoolap::Database::open(path)`
- `:85` [TYPE] — `pub fn from_db(db: stoolap::Database) -> Self`

**quota-router-storage/src/migrations.rs (5 hits):**

- `:185` [TYPE] — `pub fn apply_pending(db: &stoolap::Database)`
- `:274` [TYPE] — test `let db = stoolap::Database::open_in_memory().unwrap();`
- `:327` [TYPE] — test
- `:339` [TYPE] — test
- `:451` [TYPE] — test

**quota-router-storage/src/settlement_event_repo.rs (4 hits):**

- `:26` [TYPE] — `db: stoolap::Database` field
- `:69` [TYPE] — `let db = stoolap::Database::open_in_memory()`
- `:79` [TYPE] — `let db = stoolap::Database::open(path)`
- `:99` [TYPE] — `pub fn from_db(db: stoolap::Database) -> Self`

**quota-router-storage/src/slash_store.rs (5 hits):**

- `:113` [DOC] — `/// Wraps a \`stoolap::Database\` handle.`
- `:117` [TYPE] — `db: stoolap::Database` field
- `:125` [TYPE] — `let db = stoolap::Database::open_in_memory()`
- `:136` [TYPE] — `let db = stoolap::Database::open(path)`
- `:145` [TYPE] — `pub fn from_db(db: stoolap::Database) -> Self`

**quota-router-storage/src/stoolap_did_registry.rs (4 hits; `:139` and `:201` from RFC table do NOT exist on disk):**

- `:3` [DOC] — module-level doc `//! Persistent DID-document registry backed by a \`stoolap::Database\``
- `:95` [TYPE] — `db: Arc<stoolap::Database>` field
- `:110` [TYPE] — `let db = stoolap::Database::open_in_memory()`
- `:123` [TYPE] — `let db = stoolap::Database::open(path)`

**quota-router-storage/src/stoolap_holder_registry.rs (7 hits; `holder_registry.rs` file has zero matches):**

- `:3` [DOC] — module-level doc
- `:81` [DOC] — `/// Execute \`INSERT_HOLDER_SQL\` against a \`stoolap::Database\`.`
- `:82` [TYPE] — `fn execute_insert_db(db: &stoolap::Database, ...)`
- `:101` [TYPE] — `db: stoolap::Database` field
- `:114` [TYPE] — `let db = stoolap::Database::open_in_memory()`
- `:120` [DOC] — `/// Wrap an existing \`stoolap::Database\` (does NOT call \`apply_pending\`).`
- `:121` [TYPE] — `pub fn from_database(db: stoolap::Database) -> Self`

**quota-router-storage/src/stoolap_spend_ledger.rs (4 hits, NEW in v2.1; `spend_ledger.rs` file does NOT exist):**

- `:3` [DOC] — module-level doc
- `:195` [TYPE] — `db: Arc<stoolap::Database>` field
- `:248` [TYPE] — `let db = stoolap::Database::open_in_memory()`
- `:295` [TYPE] — `let db = stoolap::Database::open(path)`

**Notes on RFC-0206 §Layer B TYPE Renames table discrepancies (v2.1 fix):**

- `stoolap_did_registry.rs:139, :201` (RFC table) do NOT exist on disk; real TYPE positions at `:95, :110, :123`.
- `holder_registry.rs:81, :155` (RFC table) do NOT exist; file has zero `stoolap::Database` matches. Trait decl moves to `octo-cap-macaroon` per `0206-003-trait-moves` and is out of scope here. The actual stoolap-backed holder impl lives in `stoolap_holder_registry.rs` (7 hits at `:3, :81, :82, :101, :114, :120, :121`) — entirely absent from RFC §Layer B TYPE Renames table.
- `spend_ledger.rs` (RFC table) does NOT exist; real file is `stoolap_spend_ledger.rs` at `:195, :248, :295` (plus `:3` doc).
- `ask_repo.rs:42, :189`, `slash_store.rs:67, :289`, `settlement_event_repo.rs:36, :96`, `migrations.rs:14` (RFC table) are wrong line numbers; see regenerated table above for actual on-disk hits.

### Deferred Sites

No BLOCKED-ON-AUDIT deferred sites in v2.1. The 42-hits table above is exhaustive across the 9 files in scope. (v2.0's "12 deferred" placeholder is removed; if a future audit discovers additional files, file a new sub-mission rather than expanding this one.)

### Rename Pattern

- `stoolap::Database` → `octo_storage_core::Database` (applies to TYPE positions AND doc comment backticks)
- `Arc<stoolap::Database>` → `Arc<octo_storage_core::Database>`
- `&stoolap::Database` → `&octo_storage_core::Database`
- `&mut stoolap::Database` → `&mut octo_storage_core::Database`
- Qualified constructors `stoolap::Database::open_in_memory()` and `stoolap::Database::open(path)` — the `stoolap::Database` qualified path is renamed; constructor function name unchanged (inherited from newtype `From<Database> for stoolap::Database` escape hatch per RFC §Substrate Newtype Refactor).

### Cargo.toml Deps Update

- Each renamed crate MUST drop `stoolap` direct dep (verified by AC gate; see "stoolap = line absent" gate below)
- `octo-storage-core = { path = "../octo-storage-core" }` dep is ALREADY present in both `crates/quota-router-storage/Cargo.toml:23` and `crates/octo-vault/Cargo.toml:16` (verified 2026-08-20); no Cargo.toml edit required for substrate dep
- **HIGH 6 exemption rationale**: RFC-0206 §Cargo.toml Cross-Cuts scopes the "Adapter crates MUST declare `octo-storage` (NOT direct substrate)" rule to Tier 3 adapter crates. `quota-router-storage` + `octo-vault` are Layer B consumer crates that hold the `Database` newtype directly in struct fields and constructor signatures; they require direct substrate path access for the type alias. Routing through the `octo-storage` facade would also be valid (facade re-exports substrate) but adds an unnecessary dependency layer; direct `octo-storage-core` is the substrate's documented public API.
- Exception: typed-query allowlist sites per RFC §Substrate Newtype Refactor — use `From<Database> for stoolap::Database` escape hatch. Escape-hatch sites are documented in `docs/audits/0206-002-escape-hatch-audit.md` and SUBTRACTED from the rg count (rg gate accommodates escape-hatch sites).

## Acceptance Criterion

- TV-0206-A7 partial gate (this mission closes 2 of 4 paths; HIGH 5):
  - Path 1: `rg 'stoolap::Database' crates/quota-router-storage/src | wc -l` equals 0 (minus escape-hatch sites per `From<Database>` audit file)
  - Path 2: `rg 'stoolap::Database' crates/octo-vault/src | wc -l` equals 0 (minus escape-hatch sites)
  - Path 3 (owned by `0206-003-trait-moves`): `rg 'stoolap::Database' crates/octo-ident/src | wc -l` equals 0
  - Path 4 (owned by `0206-003-trait-moves`): `rg 'stoolap::Database' crates/octo-cap-macaroon/src | wc -l` equals 0
- `From<Database>` escape-hatch site list produced (file:line) BEFORE rename; committed as `docs/audits/0206-002-escape-hatch-audit.md`; sites counted and subtracted from rg gate
- `Deref` surface audit: list every method/property access through `Deref<Target = stoolap::Database>`; pre-rewrite baseline + post-rewrite delta in `docs/audits/0206-002-deref-surface-audit.md`
- 42 explicit sites renamed (35 TYPE + 7 DOC, across 9 files per Explicit Sites table)
- `rg 'stoolap::Database' crates/quota-router-storage/src crates/octo-vault/src 2>/dev/null | wc -l` minus escape-hatch site count equals 0
- `rg '^\s*stoolap\s*=' crates/quota-router-storage/Cargo.toml crates/octo-vault/Cargo.toml` returns zero lines (Cargo.toml Deps Update §must drop — `stoolap =` line absent from both Cargo.toml files)
- `cargo build -p quota-router-storage -p octo-vault` green (narrowed from workspace; no `0206-004` race dependency)
- `cargo test -p quota-router-storage -p octo-vault --lib` green
- `cargo clippy -p quota-router-storage -p octo-vault --all-targets --features full -- -D warnings` green (per `quota-router-core-feature-mutex` memory; `--all-features` ALWAYS fails)
- `cargo fmt --all -- --check` green

## Files / Artifacts

- Edit: `crates/quota-router-storage/Cargo.toml` (drop `stoolap` dep at `:16`; `octo-storage-core` at `:23` already present)
- Edit: `crates/octo-vault/Cargo.toml` (drop `stoolap` dep at `:22`; `octo-storage-core` at `:16` already present)
- Edit: 9 source files per Explicit Sites table (35 TYPE + 7 DOC rename sites)
- New: `docs/audits/0206-002-escape-hatch-audit.md` (From<Database> site list with line subtraction count)
- New: `docs/audits/0206-002-deref-surface-audit.md` (Deref access site list)

## Cross-references

- RFC-0206 v2.0 §Layer B TYPE Renames (note: RFC §Layer B TYPE Renames table line refs are WRONG — see Explicit Sites §Notes on RFC table discrepancies)
- RFC-0206 v2.0 TV-0206-A7 (partial closure 2 of 4 paths; full closure requires `0206-003`)
- RFC-0206 v2.0 §Substrate Newtype Refactor (`From<Database>` escape hatch)
- RFC-0206 v2.0 §Cargo.toml Cross-Cuts (consumer-crate exemption per HIGH 6)
- RFC-0205 v2.0 (coupled pair per BLUEPRINT.md §Dependency Validation Rules → 2-Cycle Atomic Promotion rule 5)
- Mission `0206-001-substrate-newtype` (substrate Database type)

## Out of scope

- Substrate newtype impl (owned by `0206-001`)
- HolderRegistry trait move (owned by `0206-003`)
- StoolapDidRegistry impl move (owned by `0206-003`)
- 5 adapter crates (owned by `0206-004`)
- TV-0206-A7 paths 3 + 4: `crates/octo-ident/src` + `crates/octo-cap-macaroon/src` (already at 0 matches; closure owned by `0206-003-trait-moves`)

## Dependencies

- `0206-001-substrate-newtype` (substrate Database type must exist)
- RFC-0205 v2.0 (coupled pair)
- RFC-0206 v2.0 (acceptance precondition)
- **Ordering clause (not a `depends_on:` cycle)**: `0206-003-trait-moves` must run AFTER this mission (0206-003 lists 0206-002 as its dep)

## Version History

| Version | Date       | Change                                                                                                                                                                                                                     |
| ------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v1.0    | 2026-08-20 | Initial filing (29 sites claimed, unverifiable)                                                                                                                                                                            |
| v2.0    | 2026-08-20 | R1 fix: 17 explicit + 12 BLOCKED-ON-AUDIT; 3 deps added; 2 audit gates added                                                                                                                                               |
| v2.1    | 2026-08-20 | R2 fix: 42 grep hits regenerated from disk (35 TYPE + 7 DOC across 9 files); CRIT 1-4 (sites table) + HIGH 1-6 + 5 MED applied; 0206-003 + 0206-004 dropped from `depends_on:`; HIGH 6 consumer-crate exemption documented |
