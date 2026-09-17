---
name: 0011-c-octo-runtime-revocation-store
description: Land 1 Layer D extension crate per RFC-0011-c §F.7.5 cross-process revocation substrate (paired amendment)
metadata:
  node_type: substrate-extension
  type: layer-d-extension
  originSessionId: d23cf564-d553-4e7d-be82-070883125eed
  created: 2026-09-17
  v: "1.0"
  depends_on:
    - RFC-0011-c §F.7.5
    - mission 0011-c-octo-runtime-attachhandle-substrate
  paired_rfc_section: "RFC-0011-c §F.7.5"
release_gate: RevocationStore trait + two-slot singleton + factory closure façade (per RFC-0011-c §F.7.5 paired amendment)
release_gate_cleared_at: 2026-09-17
status: Completed
claimed_by: mmacedoeu
claimed_at: 2026-09-17
completed_at: 2026-09-17
rfc_dry_closed_at: 2026-09-17
rfc_dry_closure_audit: docs/audits/2026-09-17-0011-c-phase-c-cross-process-revocation-dry-closure.md
rfc_dry_closure_commit: 642e76a8
impl_dry_closed_at: 2026-09-17
impl_dry_closure_audit: docs/audits/2026-09-17-0011-c-octo-runtime-revocation-store-impl-dry-closure.md
impl_dry_closure_commit: 6dc20103
---

# 0011-c-octo-runtime-revocation-store — cross-process revocation substrate (paired amendment)

**Status:** Completed (RFC + implementation DRY CLOSED 2026-09-17)
**Substrate:** RFC-0011-c §F.7.5 (`octo-runtime-revocation-store` Layer D + `octo-runtime` Layer B trait + two-slot singleton façade)
**Parent:** RFC-0011-c (paired Phase C amendment of RFC-0011-c)
**Depends on:** mission `0011-c-octo-runtime-attachhandle-substrate` — the Layer B persistence + revocation substrate + `AttachError::Internal(String)` + `PersistenceError` types (RFC-0011-c §F.2 step (e) + §F.4).

## Status

**Implementation DRY CLOSED 2026-09-17** — R10 + R11 = 2 consecutive zero-finding 5-len rounds = GATE GREEN at HEAD `6dc20103`. Closure audit: `docs/audits/2026-09-17-0011-c-octo-runtime-revocation-store-impl-dry-closure.md`. 6/6 lib tests + 1/1 cross-process TV-AGT27 pass.

**RFC DRY CLOSED 2026-09-17** — paired follow-on amendment to RFC-0011-c §F.7.5 DRY CLOSURE GATE GREEN. 3-commit chain: `c62ffcbd` (R3.5 — 53 findings) + `739c9c60` (R4.5 — 5 findings) + `642e76a8` (R5.5 — 5 findings). Closure audit: `docs/audits/2026-09-17-0011-c-phase-c-cross-process-revocation-dry-closure.md`. Layer D canonical definition extended to persistence adapters per [[cipherocto-design-principles]] §Layer model amendment.

Originally Claimed (RFC-0011-c §F.7.5 paired amendment, the per-extension-crates pattern + Layer D persistence adapter topology per [[cipherocto-design-principles]] §User extensibility + §Layer model).

## Substrate (RFC-0011-c §F.7.5)

Per RFC-0011-c §F.7.5 (Cross-process revocation substrate) + §F.8 (Per-Extension Crate Manifest Spec).

### Layer B substrate additions (paired amendment rewrites bodies, preserves signatures)

Located in `crates/octo-runtime/src/persistence.rs`:

- `pub trait RevocationStore: Send + Sync + std::fmt::Debug`:
  - `fn revoke_attach_token(&self, session_id: SessionId) -> Result<(), AttachError>`
  - `fn is_token_revoked(&self, session_id: &SessionId) -> bool`
  - `fn kind(&self) -> &'static str` — per-impl diagnostic identity for `tracing::error!` observability sites
- `pub static DEFAULT_REVOCATION_STORE: OnceLock<Arc<InMemoryRevocationStore>>` — lazily-initialized default
- `pub static ACTIVE_REVOCATION_STORE: OnceLock<Arc<dyn RevocationStore>>` — single-shot operator install
- `fn current_revocation_store() -> Arc<dyn RevocationStore>` — module-private; reads `ACTIVE` first, falls back to lazy-init `DEFAULT`
- `pub fn set_revocation_store(store: Arc<dyn RevocationStore>) -> Result<(), AttachError>` — writes to `ACTIVE`; double-install returns `Internal(String)` per exit 64
- `pub fn install_revocation_store_default_with<F>(factory: F) -> Result<(), AttachError> where F: FnOnce() -> Result<Arc<dyn RevocationStore>, AttachError>` — factory-mediated init fn; routes through `set_revocation_store(store)`
- `pub fn revoke_attach_token(session_id: SessionId) -> Result<(), AttachError>` — paired amendment rewrites body to dispatch via `current_revocation_store()`
- `pub fn is_token_revoked(session_id: &SessionId) -> bool` — paired amendment rewrites body to dispatch via `current_revocation_store()`
- `pub struct InMemoryRevocationStore { inner: RwLock<HashSet<SessionId>> }` — additive substrate type
- `pub(crate) fn with_inner<F, R>(&self, f: F) -> R where F: FnOnce(&HashSet<SessionId>) -> R` — test injection hook
- 8 new tests in `persistence.rs::tests` covering trait dispatch + default impl + idempotent revoke + poisoned-lock fail-CLOSED + `with_inner` test hook + `set_revocation_store` re-init rejection + two-slot singleton isolation + `install_revocation_store_default_with` factory closure path

### Layer D crate (`crates/octo-runtime-revocation-store/`)

Per-extension crate pattern:

- Cargo.toml: `stoolap = { git = "...", rev = "527e8eb" }` (CipherOcto fork per [[feedback_stoolap_persistence]]) — additive dep with rationale comment per [[cipherocto-design-principles]] §Crate dependency rationale. The Stoolap fork in this position is a **frozen external primitive** (fork-pinned, non-crypto); it does not host cipherocto business schema beyond the single revocation table (HARD RED LINE per [[stoolap-general-purpose-db]]).
- `pub struct StoolapRevocationStore { db: RwLock<stoolap::Database> }` — opens / creates the ledger under `~/.local/share/octo/runtime/revocation.stoolap` (or `$CIPHEROCTO_DATA_DIR/revocation.stoolap`).
- Ledger schema (single table):
  ```sql
  CREATE TABLE IF NOT EXISTS revocation (
      session_id BLOB(32) NOT NULL PRIMARY KEY,
      revoked_at_unix INTEGER NOT NULL
  );
  ```
  No `STRICT` qualifier — fork `STRICT` keyword unverified at rev 527e8eb per substrate-discipline no-unverified-features rule.
- `impl RevocationStore for StoolapRevocationStore`:
  - `kind` returns `"StoolapRevocationStore"`
  - `revoke_attach_token` — pre-check `SELECT 1 FROM revocation WHERE session_id = ? LIMIT 1`; if not present, `INSERT INTO revocation ...` (idempotent — pre-check guards against duplicate rows because the Stoolap fork at `rev = "527e8eb"` does NOT support `INSERT OR IGNORE` / `INSERT OR REPLACE` syntax).
  - `is_token_revoked` — `SELECT 1 FROM revocation WHERE session_id = ? LIMIT 1`. Returns `true` on ledger read failure (fail-CLOSED); logs at ERROR via `tracing::error!` with `kind()` value.
- `pub fn install_default() -> Result<Arc<dyn RevocationStore>, AttachError>` — opens the ledger + returns `Arc<dyn RevocationStore>`. Does NOT self-register.
- 6 tests covering `kind()` + idempotence + existence-check + persistence across reopens + schema bootstrap + Send/Sync compile-time bounds

### CLI wiring (`octo-cli/src/main.rs`)

```rust
fn main() {
    #[cfg(feature = "revocation-store-stoolap")]
    {
        if let Err(e) = octo_runtime::install_revocation_store_default_with(
            octo_runtime_revocation_store::install_default,
        ) {
            tracing::warn!(
                reason = %e,
                "revocation store install failed; falling back to InMemoryRevocationStore"
            );
        }
    }
    // ... rest of CLI bootstrap
}
```

The Cargo.toml `revocation-store-stoolap` feature is opt-in. Default CLI builds use zero-dep `InMemoryRevocationStore`. Cross-process revocation is opt-in per operator.

## Parent

RFC-0011-c §F.7.5 (Cross-process revocation substrate) + §F.8 (Per-Extension Crate Manifest Spec).

## Acceptance Criteria

- [ ] Layer B substrate: trait `RevocationStore` + two-slot singleton + `set_revocation_store` + `install_revocation_store_default_with` + body rewrites on `revoke_attach_token` + `is_token_revoked` all land per §F.7.5 substrate additions
- [ ] `pub struct InMemoryRevocationStore` lands with `Default` impl + `Debug + Send + Sync` derives + `with_inner` test hook
- [ ] 8 substrate tests pass (trait dispatch + idempotence + poisoned-lock fail-CLOSED + two-slot isolation + factory closure path + re-init rejection)
- [ ] `crates/octo-runtime-revocation-store/` Cargo.toml + src/lib.rs land per §F.7.5 crate spec
- [ ] Ledger path `~/.local/share/octo/runtime/revocation.stoolap` honored (overridable via `$CIPHEROCTO_DATA_DIR`)
- [ ] Schema bootstrap works on first open (`CREATE TABLE IF NOT EXISTS revocation (...)`)
- [ ] `StoolapRevocationStore::is_token_revoked` returns `true` on ledger read failure + logs ERROR with `kind()` value (fail-CLOSED)
- [ ] `StoolapRevocationStore::revoke_attach_token` uses pre-check SELECT 1 + INSERT (NOT `INSERT OR IGNORE`)
- [ ] `install_default` returns `Arc<dyn RevocationStore>` and does NOT self-register
- [ ] 6 crate tests pass (`kind()` return value + idempotence + existence-check + persistence-across-reopens + schema bootstrap + Send/Sync compile-time bounds)
- [ ] CLI wiring block lands in `octo-cli/src/main.rs` behind `feature = "revocation-store-stoolap"` + logs WARN on install failure
- [ ] `octo-cli` Cargo.toml gains `revocation-store-stoolap` feature flag + `octo-runtime-revocation-store` opt-in dep
- [ ] `cargo build -p octo-runtime -p octo-runtime-revocation-store -p octo-cli --features revocation-store-stoolap` exit 0
- [ ] `cargo build -p octo-runtime -p octo-cli` (default features, no Layer D dep) exit 0 — Layer D is opt-in only
- [ ] `cargo test -p octo-runtime --lib` — 78+8 = 86 pass
- [ ] `cargo test -p octo-runtime-revocation-store --lib` — 6 pass
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [ ] `cargo fmt --all -- --check` clean
- [ ] TV-AGT27 cross-process test lands in `crates/octo-runtime-revocation-store/tests/cross_process.rs` — spawns three CLI processes + shared Stoolap ledger + verifies revocation propagates across process boundary
- [ ] TV-AGT27 happy-path: spawn-side mints token + operator-side revokes + attach-side observes via `is_token_revoked` (returns `true`)
- [ ] TV-AGT27 fail-CLOSED: revoke-side ledger write succeeds + attach-side ledger read returns `false` if shared path is empty (sanity baseline)
- [ ] RFC-0011-c §F.7.5 §Cross-process test topology matches implementation (3 processes, not 2)
- [ ] [[cipherocto-design-principles]] memory card already amended (R3.5 sweep) — implementation is substrate-faithful against the amended spec
- [ ] Hard red line preserved: Stoolap ledger hosts ONLY the `revocation` table + no other cipherocto business schema
- [ ] DRY CLOSURE gate: 2 consecutive zero-finding rounds on Phase C implementation (5-len reviewers with layer-model + substrate-faithfulness emphasis)

### Type Coverage

| RFC-0011-c type                            | Sub-step       | Notes                                                                                                                |
| ------------------------------------------ | -------------- | -------------------------------------------------------------------------------------------------------------------- |
| `RevocationStore` (trait)                  | §F.7.5 (sub)   | Layer B; `Send + Sync + Debug` + 3 methods (`revoke_attach_token`, `is_token_revoked`, `kind`)                        |
| `InMemoryRevocationStore`                  | §F.7.5 (sub)   | Layer B additive substrate type; default impl behind the trait                                                       |
| `DEFAULT_REVOCATION_STORE` (static)        | §F.7.5 (sub)   | `OnceLock<Arc<InMemoryRevocationStore>>` lazily-initialized default                                                  |
| `ACTIVE_REVOCATION_STORE` (static)         | §F.7.5 (sub)   | `OnceLock<Arc<dyn RevocationStore>>` single-shot operator install                                                    |
| `set_revocation_store`                     | §F.7.5 (sub)   | Writes to `ACTIVE`; double-install returns `Internal(String)` exit 64                                                |
| `install_revocation_store_default_with<F>` | §F.7.5 (sub)   | Factory closure init fn; try-install semantic at API layer (preserves fall-back on error)                            |
| `current_revocation_store`                 | §F.7.5 (sub)   | Module-private; reads `ACTIVE` first, falls back to lazy-init `DEFAULT`                                              |
| `StoolapRevocationStore`                   | §F.7.5 (crate) | Layer D; Stoolap-backed cross-process ledger impl behind the trait                                                   |
| `StoolapRevocationStore::install_default`  | §F.7.5 (crate) | Opens ledger + returns `Arc<dyn RevocationStore>`; does NOT self-register                                            |
| `octo_runtime::revoke_attach_token` (rewr) | §F.7.5 (sub)   | Body rewrite: static-access → `current_revocation_store()` dispatch; signature + error variants preserved            |
| `octo_runtime::is_token_revoked` (rewr)    | §F.7.5 (sub)   | Body rewrite: same dispatch; same signature preservation; same poisoned-lock fail-CLOSED                              |
| `with_inner` (test hook)                   | §F.7.5 (sub)   | `pub(crate)` accessor on `InMemoryRevocationStore`; replaced `with_revocation_set` seam from earlier §F.3 drafts      |

## Layer direction (RFC-0011-c §9.1 + per [[cipherocto-design-principles]])

- `octo-runtime-revocation-store` (Layer D) — depends on `octo-runtime` (Layer B) for `RevocationStore` trait + `AttachError` + `SessionId`. No reverse deps.
- `octo-runtime` (Layer B) — paired amendment additions + trait + default impl. NO compile-time dep on Layer D (factory façade accepts `Arc<dyn RevocationStore>` from any caller).
- `octo-cli` (Layer C) — depends on `octo-runtime` (Layer B) always; depends on `octo-runtime-revocation-store` (Layer D) only when feature `revocation-store-stoolap` enabled.
- Wiring direction: C → B → D via factory closure (B owns registration, D owns constructor, C wires dependency injection).

NO new Layer A types introduced. Layer A frozen; the Stoolap fork is a frozen external primitive (fork-pinned rev 527e8eb, non-crypto).

## Validation

```bash
cargo build -p octo-runtime  # exit 0 (Layer B + substrate additions)
cargo build -p octo-runtime-revocation-store  # exit 0 (Layer D crate)
cargo build -p octo-cli --features revocation-store-stoolap  # exit 0 (CLI wiring + Layer D dep)
cargo build -p octo-cli  # exit 0 (default features, no Layer D)
cargo test -p octo-runtime --lib  # 78 + 8 = 86 pass (persistence + trait tests)
cargo test -p octo-runtime-revocation-store --lib  # 6 pass
cargo test -p octo-runtime-revocation-store --test cross_process  # TV-AGT27 pass (3-process fork)
cargo clippy --workspace --all-targets --all-features -- -D warnings  # clean
cargo fmt --all -- --check  # clean
npx prettier --check rfcs/accepted/process/0011-c-agent-lifecycle.md  # clean
```

## Backward compat

- Additive Layer B substrate (new trait + new struct + new statics + 2 body rewrites preserving signatures).
- Additive Layer D crate (own Cargo.toml + own src/lib.rs).
- The `revoke_attach_token` + `is_token_revoked` signatures are UNCHANGED. Body dispatch change is internal (direct static access → trait-mediated `current_revocation_store()` lookup). Default `InMemoryRevocationStore` preserves the existing §F.3 fork-fail-closed contract.
- Per-extension crates pattern enforced via workspace `members = ["crates/*"]` glob (auto-included); no workspace Cargo.toml edit needed.
- No CLI surface change — `From<AttachError>` wildcard arm auto-handles all `AttachError` variants.

## Cross-references

- RFC-0011-c §F.7.5 — Cross-process revocation substrate (paired amendment)
- RFC-0011-c §F.7.4 — Cross-process event bridging concern (Phase B Layer D transport; out of Phase C scope)
- RFC-0011-c §F.8 — Per-Extension Crate Manifest Spec
- [[cipherocto-design-principles]] — Layer model + per-extension crates + registry pattern + Layer D persistence adapter amendment
- [[feedback_stoolap_persistence]] — Stoolap fork pin at rev 527e8eb
- [[stoolap-general-purpose-db]] — Stoolap HARD RED LINE
- [[0011-c-phase-c-cross-process-revocation-dry-closure-2026-09-17]] — paired RFC DRY CLOSURE state
- [[0011-c-attachhandle-dry-closure-2026-09-16]] — predecessor Layer B closure (trait + Register substrate)

## Why gate

Release-gated on `RevocationStore` trait + two-slot singleton + factory closure façade (RFC-0011-c §F.7.5 paired amendment landed 2026-09-17, commit `642e76a8`).

## Out of scope (Phase C explicit)

- **Phase B Layer D transport extension** — already landed 2026-09-17 (commit `b6a8ca73`). Phase C reuses the trait + registry substrate pattern, not the transport crates themselves.
- **Phase D companion missions** — `0011-c-octowallet-agents-substrate` + key rotation. Separate cycles.
- **`octo agent revoke-attach` CLI subcommand** — the operator-facing wiring point. Existing `octo_runtime::revoke_attach_token` substrate write path can be exposed as a separate mission (`0011-c-octowallet-revoke-attach-subcommand`) when product needs it.
- **Server-side event piping for `octo-runtime-transport-unix`** — Phase C ships the revocation ledger substrate; full cross-process event bridging remains Phase B scope boundary.

## Cargo deps (1 new crate)

`crates/octo-runtime-revocation-store/Cargo.toml` adds:

```toml
# Cross-process revocation ledger (Layer D persistence adapter;
# RFC-0011-c §F.7.5 paired amendment; Stoolap fork frozen external
# primitive per [[feedback_stoolap_persistence]], fork-pinned rev
# 527e8eb, non-crypto, NEVER hosts cipherocto business schema per
# [[stoolap-general-purpose-db]] HARD RED LINE)
stoolap = { git = "...", rev = "527e8eb" }
```

Plus per [[cipherocto-design-principles]] §Crate dependency rationale for the `octo-runtime` + `tokio` std/sync deps in this new crate.

## Risk

- **MED** — `octo_runtime::revoke_attach_token` + `is_token_revoked` body dispatch change. Signature + error variants + return types are preserved, so all existing call sites compile unchanged. Risk is in the dispatch path correctness — covered by 8 new substrate tests + substrate-faithfulness reviewer.
- **MED** — Stoolap fork API surface. The fork pin rev 527e8eb may lack `Send + Sync` impl on `stoolap::Database`. Per §F.7.5 substrate additions: "verified at implementation time before merge; if the fork does not satisfy these bounds, the impl is blocked at compile time, not at runtime". If compile blocks, fall back to wrapping in `tokio::sync::Mutex` + tighten `Send + Sync` bounds on the impl — do NOT relax bounds on the trait.
- **LOW** — Cross-process test TV-AGT27 spawns three child processes. Test runtime adds subprocess-latency overhead. Use `--test-threads=1` for cross-process tests to prevent fork state pollution.
- **LOW** — Stoolap ledger path `~/.local/share/octo/runtime/` may not exist on first run. `install_default` should `std::fs::create_dir_all` the parent path before opening.

## Claimant

@mmacedoeu (claimed 2026-09-17, RFC paired amendment DRY CLOSED 2026-09-17, implementation pending).
