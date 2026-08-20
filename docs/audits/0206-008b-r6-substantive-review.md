# 0206-008b R6 — Substantive Per-Commit Review

**Date:** 2026-08-20
**Scope:** 6 smaller dep-drop commits + Cargo.toml-only commits
**Reviewer:** Substrate redesign cascade R6 (code→substrate gap analysis)
**Result:** 4 findings; 1 substrate gap (HIGH), 1 stale comment (MED), 2 doc-only/inline (LOW)

## Commits reviewed

| # | SHA | Mission | Description |
|---|-----|---------|-------------|
| 1 | `75dc6d38` | 0206-008b | `octo-ident-storage` drop direct stoolap dep |
| 2 | `4a2b9edc` | 0206-008b | `octo-matrix-session-store` drop direct stoolap dep |
| 3 | `3b1f63da` | 0206-008b | `octo-adapter-whatsapp` partial substrate rewrite |
| 4 | `0861f2b6` | 0206-008b | `octo-adapter-telegram-mtproto` partial substrate rewrite |
| 5 | `394bf221` | 0206-008b | `quota-router-storage` drop direct stoolap dep |
| 6 | `2131fae0` | 0206-008b | Phase 2.6 partial — 5 Cargo.toml edits |

Plus 2 previously-reviewed commits (`e6abf121` octo-reputation, `dd0cb79a` quota-router-core) included in the substrate-gap analysis since the leak crosses commit boundaries.

## Findings

| # | Sev | File | Finding | Status |
|---|-----|------|---------|--------|
| 1 | HIGH | `crates/octo-storage-core/src/stoolap.rs` + 4 consumer crates | **DataType substrate leak** — `pub mod stoolap` re-exports `Value` but NOT the inner `DataType` enum; 22 consumer sites reference raw `stoolap::DataType::{Null,Integer,Blob}` because no substrate-defined helper exists for typed NULL. RFC-0206 v2.2 §Substrate Re-export Block counts 5 nested re-exports — gap is silent. | OPEN |
| 2 | MED | `crates/quota-router-core/Cargo.toml:53-62` | **Stale v2.1-era comment** — explains direct stoolap dep stays pending "v2.2 RFC-0206 amendment (filed as 0206-011b)". 0206-011b LANDED; dep stays for *different* reason (DataType/pubsub internal pin). Comment now misleading. | OPEN |
| 3 | LOW | `crates/octo-adapter-telegram-mtproto/src/session.rs:675,713,716,719` | Same DataType leak as #1 but uses 3 distinct variants (`Blob` + `Integer` ×3). Substrate helper must cover all three. | SUBSUMED by #1 |
| 4 | LOW | `crates/octo-reputation/Cargo.toml:22` + `crates/quota-router-cli/Cargo.toml:93` | `stoolap = [...]` feature-flag (substrate-feature plumbing + sibling-crate feature gate). TV-A9(b) gate `rg '^\s*stoolap\s*=\s*\{'` correctly excludes these — they are NOT direct upstream deps. | DOCUMENTED |

## Finding #1 detail — DataType leak

### Scope

22 consumer sites reference `stoolap::(core::)?DataType::{Null,Integer,Blob}`:

| File | Sites | Variants used |
|------|-------|---------------|
| `crates/quota-router-core/src/storage.rs` | 12 | `Null` |
| `crates/quota-router-core/src/cache.rs` | 1 | (none — uses `stoolap::core::Value::blob` direct, separate issue) |
| `crates/octo-adapter-whatsapp/src/store.rs` | 5+ | `Null` |
| `crates/octo-adapter-telegram-mtproto/src/session.rs` | 4 | `Blob`, `Integer` ×3 |

### Root cause

Substrate `pub mod stoolap` block (5 nested `pub use stoolap::*`) re-exports:
- `Error`
- `ApiTransaction` (the STRUCT per fork lib.rs:192 alias for `api::Transaction`)
- `ResultRow`
- `Rows`
- `Value`

Does NOT re-export `DataType` (the inner discriminant enum used in `Value::Null(DataType)` and other typed-Value constructors). Upstream provides:

```rust
// /home/mmacedoeu/.cargo/git/checkouts/stoolap-0de5b2281a88eb98/d337010/src/core/value.rs:117-127
pub fn null(data_type: DataType) -> Self { Value::Null(data_type) }
pub fn null_unknown() -> Self { Value::Null(DataType::Null) }
```

`null_unknown()` is the no-arg convenience constructor — covers the 12 `DataType::Null` sites. But 4 sites in oatm use `DataType::Blob` + `DataType::Integer`, requiring either a full `DataType` enum re-export OR 3 typed-Null helpers.

### Fix options

**Option A (minimal):** substrate re-exports `Value::null_unknown()` + adds 2 typed helpers (`null_blob`, `null_integer`) to `pub mod stoolap`. Consumers rewrite 22 sites.

```rust
// crates/octo-storage-core/src/stoolap.rs addition
impl Value {
    pub fn null_unknown(&self) -> Value { Value::Null(DataType::Null) }
    pub fn null_blob(&self) -> Value { Value::Null(DataType::Blob) }
    pub fn null_integer(&self) -> Value { Value::Null(DataType::Integer) }
}
```

But this requires substrate to depend on `stoolap::core::DataType` directly (already does via the existing `pub use stoolap::core::Error`). Adds 3 method re-exports — bumps nested re-export count from 5 → 6+ (or stays 5 if helpers live as `pub fn` not `pub use`).

**Option B (full):** substrate re-exports the `DataType` enum. Consumers keep current code. Re-export count goes 5 → 6.

```rust
// crates/octo-storage-core/src/stoolap.rs addition
pub use stoolap::core::DataType;
```

`DataType` is `#[non_exhaustive]` upstream (per typical enum-with-reserved-bytes pattern visible in value.rs:80-107). Consumers must use `Value::Null(DataType::Null)` syntax — no breakage.

### Recommendation

Option B — `pub use stoolap::core::DataType;` as 6th nested re-export. RFC amendment to §Substrate Re-export Block to bump 5 → 6 with justification (DataType is the typed-Value discriminant; can't construct typed Null without it). Consumer sites stay 1:1; only prefix path changes from `stoolap::DataType` → `octo_storage_core::stoolap::DataType`. Mechanical sed across 22 sites.

### Out of scope (deferred)

- `stoolap::core::Value::blob` direct usage in `quota-router-core/src/cache.rs:137` — `Value::blob` IS re-exported (line 35 of stoolap.rs substrate). Cache.rs:137 line is `-` in diff (already removed per `e6abf121`/`dd0cb79a`) — verify in follow-up audit if cache.rs:137 still pre-existed.

## Finding #2 detail — stale qrc Cargo.toml comment

### Current text (Cargo.toml:53-62)

```toml
# Database
# Per RFC-0206 v2.1 §Cargo.toml Cross-Cuts + HIGH 6 consumer-crate
# exemption, the substrate redesign v3.0 does NOT yet re-export
# `stoolap::ResultRow` / `stoolap::ApiTransaction` / `stoolap::Rows`
# / `stoolap::Error` — core storage traits + adapter wiring decode
# rows directly. A v2.2 RFC-0206 amendment (filed as 0206-011b) is
# the proper scope to add a `pub mod stoolap` re-export to the
# substrate. Until then, the direct dep stays.
stoolap = { git = "https://github.com/CipherOcto/stoolap", branch = "feat/blockchain-sql" }
```

### Problem

The comment block was written for v2.1-era reality. Post-0206-011b (v2.2), substrate DOES re-export `ResultRow` / `ApiTransaction` / `Rows` / `Error`. The direct dep stays only for `DataType` (finding #1) + pubsub internal types. The v2.1-era comment is misleading.

### Fix

Update the comment to reflect v2.2 reality:

```toml
# Database
# Per RFC-0206 v2.2 §Substrate Re-export Block, the substrate
# re-exports `ResultRow` / `ApiTransaction` / `Rows` / `Error`
# via `octo_storage_core::stoolap::*`. The direct dep stays
# because quota-router-core still needs `stoolap::core::DataType`
# (typed-Value discriminant; substrate does not yet re-export it
# — see R6 finding #1 in docs/audits/0206-008b-r6-substantive-review.md)
# and the pubsub Layer B internal pin.
stoolap = { git = "https://github.com/CipherOcto/stoolap", branch = "feat/blockchain-sql" }
```

## Finding #4 detail — substrate-feature-flag deps (not direct deps)

```toml
# crates/octo-reputation/Cargo.toml:22
stoolap = ["dep:serde_json", "dep:octo-storage-core"]

# crates/quota-router-cli/Cargo.toml:93
stoolap = ["octo-reputation/stoolap"]
```

Both are feature-flag declarations in the `[features]` table, NOT direct upstream `stoolap` Cargo.toml deps. The TV-A9(b) gate `rg '^\s*stoolap\s*=\s*\{'` correctly matches only the `{ git = ... }` form. Confirmed: 4 direct deps, 2 feature flags.

## Verification

```bash
# TV-A9(b) gate (re-verify post-R6)
rg -l '^\s*stoolap\s*=\s*\{' crates/*/Cargo.toml | wc -l   # 4 (PASS ≤ 5)
rg '^\s*stoolap\s*=\s*\[' crates/*/Cargo.toml              # 2 (feature flags, exempt)

# R6 finding #1 verification (post-fix)
rg 'stoolap::DataType|stoolap::core::DataType' crates/ -l 2>/dev/null | wc -l
# pre-fix: 4 crates (octo-adapter-telegram-mtproto, octo-adapter-whatsapp, quota-router-core ×2 src)
# post-fix Option B: 0

# All builds green (re-verify after fix)
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets --features full -- -D warnings
cargo fmt --all -- --check
```

## Architectural notes

1. **`pub mod stoolap` re-export surface count** — RFC v2.2 documents "5 nested re-exports". R6 finding #1 implies the actual count needs to be 6 (add `DataType`). v2.3 amendment required to align §Substrate Re-export Block prose with the surface reality, OR the substrate re-exports `DataType` silently + RFC gets a 1-line count update.

2. **Substrate abstraction principle** — RFC §Substrate Newtype Refactor mandates substrate owns the abstraction boundary. The DataType leak is the FIRST surface where this principle has been silently violated. Catching it now (R6) is the value of multi-round review: a single round would have shipped with the gap.

3. **Approved-pin crates vs substrate-leak crates** — quota-router-core + octo-adapter-whatsapp + octo-adapter-telegram-mtproto are all approved-pin per R5 audit table. The DataType leak does NOT block dep drop (deps stay); it BLOCKS substrate abstraction integrity. Fix priority: HIGH for substrate abstraction correctness, but no urgency for dep-count TV gate.

## Out of scope

- Dep-drop count remains at 4 (no change). Finding #1 is substrate-abstraction gap, not a dep-count regression.
- Push to remote — awaits user instruction per `feedback_initiation_user_only`.

## Conclusion

R6 substantive review of 6 smaller dep-drop commits complete. **1 HIGH substrate-abstraction gap found** (DataType leak across 22 consumer sites) + **1 MED stale comment** + 2 LOW (subsumed + documented). All builds green. TV-A9(b) gate remains 4 ≤ 5 PASS.

Recommended next: file RFC-0206 v2.3 amendment "Add `DataType` to `pub mod stoolap` re-export block" (5 → 6) + consumer sed across 22 sites in a single coordinated commit. Or accept the gap as documented and move on — flag for R7.
