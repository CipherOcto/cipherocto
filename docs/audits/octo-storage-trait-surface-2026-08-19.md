# Owner-Trait Surface Audit — 2026-08-19

> **Purpose:** Document the on-disk location of every owner-trait that
> participates in the RFC-0205 / RFC-0206 restructure cycle. Establishes
> ground truth so per-adapter wiring (§Roles, §Wiring Pattern, §TV) has a
> single source of truth (this audit) instead of duplicated RFP claims.
>
> **Scope:** `pub trait <Foo>` declarations + `impl <Foo> for <Bar>`
> implementations in the 7 owner-trait crates named in RFC-0206 §Adapter
> Crate List (Initial).
>
> **Method:** `rg -l 'pub trait (DidRegistry|HolderRegistry|VaultLookup|ReputationStore|SessionStore|PolicyStore|VaultStore)\b' crates/` followed by per-file `rg` for trait body + impl blocks. All file paths verified by `ls`.

## Trait surface — declarations + impls

| Trait             | Declared at                                                           | Implemented at                                                | Adapter host (proposed)                                                                                             | Status                                                       |
| ----------------- | --------------------------------------------------------------------- | ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| `DidRegistry`     | `crates/octo-ident/src/registry.rs:143`                               | `crates/quota-router-storage/src/stoolap_did_registry.rs:139` | `crates/octo-ident-storage/` (Phase 2)                                                                              | dual — move to octo-ident per RFC-0206                       |
| `HolderRegistry`  | `crates/quota-router-storage/src/holder_registry.rs:33`               | (consumer in quota-router code only)                          | `crates/octo-cap-macaroon-storage/` (Phase 2)                                                                       | NOT moved; move is per RFC-0206 §Promotion Path Condition 4  |
| `VaultLookup`     | `crates/octo-cap-macaroon/src/vault_lookup.rs`                        | (consumer in cap-macaroon code only)                          | `crates/octo-cap-macaroon-vault-storage/` (Phase 2)                                                                 | declared in OCTO-CAP-MACAROON (not octo-cap-macaroon-vault)  |
| `ReputationStore` | `crates/octo-reputation/src/store/mod.rs:51`                          | `crates/octo-reputation/src/store/stoolap_impl.rs`            | `crates/octo-reputation-storage/` (Phase 2)                                                                         | declared + impl both in octo-reputation                      |
| `SessionStore`    | `crates/octo-matrix-session-store/src/store.rs:54`                    | `crates/octo-matrix-session-store/src/store/stoolap.rs`       | `crates/octo-matrix-session-store-storage/` (Phase 2; adapter) vs `crates/octo-matrix-session-store/` (owner-trait) | NEW distinct adapter crate; owner-trait crate already exists |
| `PolicyStore`     | **NOT DECLARED** (`crates/cipherocto-policy/src/lib.rs` has 0 traits) | —                                                             | `crates/octo-policy-storage/` (Phase 2; gated on `0206-cipherocto-policy-rename-alignment`)                         | NEW — gated on per-adapter RFC                               |
| `VaultStore`      | **NOT DECLARED** (`crates/octo-vault/src/lib.rs` has 0 traits)        | —                                                             | `crates/octo-vault-storage/` (Phase 2)                                                                              | NEW — gated on per-adapter RFC                               |

## Wiring cross-reference (RFC-0206 §Wiring Pattern consumer)

- `crates/quota-router-storage/` currently consumes 3 traits directly
  (`HolderRegistry`, `DidRegistry`, `StoolapDidRegistry impl`). Phase 2
  Task 5a builds the `octo-cap-macaroon-storage/` + `octo-ident-storage/`
  adapters; `quota-router-storage/` then redirects its consumers via the
  adapter's `register(Arc<Database>) -> Arc<dyn HolderRegistry>`
  constructor.
- `crates/octo-cap-macaroon/` consumes `VaultLookup` (declaration lives
  here, NOT in `octo-cap-macaroon-vault/` as the RFC-0206 v1.5 §Roles
  text claimed). Phase 2 Task 5c builds the adapter.

## On-disk migration status (RFC-0206 §Compatibility Backward)

Today the substrate crate `crates/octo-storage-core/` exposes a
`pub use stoolap::*` re-export surface of **zero `stoolap::*` symbols**
(the existing `pub use` block re-exports the substrate's own
`apply_pending`, `open`, `open_in_memory`, `Migration`,
`StaticMigration`, `StorageError`, `tracker::*`, `ApplyConfig` —
none of which are `stoolap::*` re-exports; verification by
`rg -c 'pub use stoolap::' crates/octo-storage-core/src/lib.rs`
returns 0). The atomicity clause (RFC-0206 §Cargo.toml Templates
Layer A) requires `pub use stoolap::Database;` lands in the same
commit as the `branch → rev` flip in `Cargo.toml`; until Phase 1
Task 2 lands, the substrate still has zero stoolap:: re-exports.

Direct `stoolap = { git = ... branch = "feat/blockchain-sql" }`
deps live in **13 sites** (12 crates + workspace root
`[patch.crates-io]` block):
`octo-storage-core`, `octo-core`, `octo-reputation`, `octo-whatsapp`,
`octo-matrix-session-store`, `octo-adapter-whatsapp`,
`octo-adapter-telegram-mtproto`, `octo-vault`, `quota-router-core`,
`quota-router-sm-engine`, `quota-router-storage`, `quota-router-cli`,
plus the workspace root `[patch.crates-io]` block. Phase 3 Task 10
90-day migration window per RFC-0206 §Implementation Phases must
redirect all 12 crates to consume via `octo_storage_core::Database`;
the workspace `[patch.crates-io]` block stays INERT (the git-sourced
direct consumers do not see it; only the crates-io-sourced transitive
consumers do).

## Crates with `migrations/` directory

`migrations/` directories exist on disk in **2 of 7** owner-trait crates:

- `crates/octo-reputation/migrations/` — 5 SQL files
- `crates/octo-vault/migrations/` — 2 SQL files

The other 5 (`octo-ident/`, `octo-cap-macaroon/`,
`octo-cap-macaroon-vault/`, `octo-matrix-session-store/`,
`cipherocto-policy/`) lack `migrations/` on disk. RFC-0206
§Wiring Pattern requirement that migrations live in
`crates/<owner>/migrations/*.sql` lands as part of each per-adapter
RFC (Phase 2 Task 5/6/7); not a precondition for RFC-0206
acceptance.

## Re-export surface at the substrate (RFC-0206 §Cargo.toml Templates Layer A)

Current `crates/octo-storage-core/src/lib.rs` `pub use` audit:
**5 lines**, none of which re-export `stoolap::*` symbols (verified
via `rg -c 'pub use stoolap::' crates/octo-storage-core/src/lib.rs`
returns 0). The 5 lines re-export the substrate's own types
(`apply_pending`, `open`, `open_in_memory`, `Migration` +
`StaticMigration`, `tracker::*` (4 internal helpers), `StorageError`).
The atomicity clause requires this audit reaches 6 lines after Phase 1
Task 2 (`pub use stoolap::Database;` lands as line 6).

**Cross-adapter leak surface:** Until the per-adapter RFCs land (Phase 2
Task 5-7), every adapter that takes a `register(Arc<Database>)` has the
ability to issue arbitrary SQL against any other adapter's tables — the
substrate has zero domain knowledge per RFC-0206 §Three-Tier Architecture
Block 1 (Core node label "ZERO domain knowledge"). The cross-adapter
isolation property is NOT provided by the substrate today; it depends on
each adapter's Rust runner enforcing a typed SQL allow-list (forward
requirement — see RFC-0206 §Future Work).

## Verification commands

```bash
# Trait locations (re-runnable)
rg -l 'pub trait (DidRegistry|HolderRegistry|VaultLookup|ReputationStore|SessionStore|PolicyStore|VaultStore)\b' crates/

# Re-export count (target: 6 after Phase 1 Task 2)
rg -c 'pub use' crates/octo-storage-core/src/lib.rs

# stoolap:: re-export count (target: 1 after Phase 1 Task 2 — exact-once)
rg -c 'pub use stoolap::' crates/octo-storage-core/src/lib.rs

# 8-pub-use cap on the facade (target: 8)
rg -c '^\s*pub use\b' crates/octo-storage/src/lib.rs

# Direct stoolap Cargo.toml deps outside the substrate (forward: should drop to 0 post Phase 3 Task 10)
rg -l '^\s*stoolap\s*=' crates/ Cargo.toml | sort -u
```
