---
id: 0010-c
title: RFC-0010 32-byte addendum — ChainId::as_bytes() canonical storage form
status: OPEN
opened: 2026-08-19
priority: LOW
parent: 0010
type: rfc-amendment
depends_on:
  - 0010-f2
  - 0010-f8
---

> **Retro-supersession (2026-08-24 audit):** Mission substrate effectively LANDED — `ChainId::as_bytes()` method present at `crates/octo-ident/src/chain.rs:137` (BLAKE3 derivation via domain separator `b"cipherocto/chain/v1/"` per Layer A frozen substrate pattern); `crates/octo-ident/tests/tv_0010_chain_id_32byte.rs` (NEW file, 3 byte-exact TV) PASSES (TV-1 determinism + TV-2 known-vector `eb200e7d...411eeab` + TV-3 17-byte + 32-byte coexistence); RFC-0010 v1.6 amendment VH row at `rfcs/accepted/process/0010-canonical-did-codec.md:1017` documents the 32-byte addendum + cross-ref to this mission + the 3 TV; `cargo test -p octo-ident --lib` 69/69 PASS (no regression in `canonical_bytes()` 17-byte form); `cargo clippy -p octo-ident --test tv_0010_chain_id_32byte -- -D warnings` clean. AC-1, AC-2, AC-3, AC-4, AC-5, AC-6, AC-8 PASS. Only AC-7 (all-targets clippy on `octo-ident`) blocked by pre-existing borsh generic bounds defect in `rich_did_document_tv` (per F-P5.2-3 RETAIN framework — substrate fabrication category, separate mission `0010-f8-rich-did-documents-clippy-fix`). Mission status preserved OPEN per historical-mission-preservation + R19 scope discipline; substrate landing owned by inline retro-supersession note (not separate closure pass).

# 0010-c — RFC-0010 32-byte addendum (ChainId::as_bytes)

## Context

Review doc §20.3.2 + §27 mandates a 32-byte canonical BLAKE3 form for
`ChainId` to align with the storage substrate PK column type
`BLOB(32)` (adopted by RFC-0960 v3.0 + RFC-0900 v2.0 + 0900-d1).
RFC-0010 v1.5 carries the 17-byte `ChainNamespace::canonical_bytes`
form (legacy WAL/audit log wire form). The 32-byte form is additive
— both coexist (R15-F13 reword, §1087).

## Scope

- Add `ChainId::as_bytes()` returning `[u8; 32]` via
  `BLAKE3("cipherocto/chain/v1/" || chain_string)`. Mirror of
  `AssetId::as_bytes()` canonical substrate pattern.
- RFC-0010 v1.5 → v1.6: §Version History v1.6 row documenting
  the 32-byte addendum + §Storage Substrate subsection
  cross-ref to `ChainId::as_bytes()` canonical form.
- 3 byte-exact TV fixtures in
  `crates/octo-ident/tests/tv_0010_chain_id_32byte.rs` (NEW):
  - **TV-1**: `as_bytes()` determinism — same input → same 32 bytes
    across N calls (BLAKE3 determinism lock).
  - **TV-2**: `as_bytes()` BLAKE3 known-vector — precomputed
    `BLAKE3("cipherocto/chain/v1/" + "cipherocto/testnet/v1")` =
    specific 32-byte hex (pinned).
  - **TV-3**: 17-byte + 32-byte forms coexist — `canonical_bytes()`
    (17-byte) ≠ `as_bytes()` (32-byte) for same input; both
    methods callable independently.

## Layer model

- Layer A: `ChainId::as_bytes()` is a domain-separator-derived
  canonical encoding (frozen substrate pattern; RFC-0960 v3.0 +
  RFC-0105 v2.0 use identical BLAKE3-derivation pattern).
  No new dependencies.
- Layer B: storage substrate already uses `[u8; 32]` (LANDED via
  RFC-0960 v3.0 + RFC-0900 v2.0); no new crate changes.

## Acceptance criteria

- **AC-1:** `ChainId::as_bytes()` method added at
  `crates/octo-ident/src/chain.rs` returning `[u8; 32]`,
  implementation: `*blake3::hash(format!("cipherocto/chain/v1/{}",
self.0).as_bytes()).as_bytes()` (or `Hasher::new()` form).
- **AC-2:** RFC-0010 v1.5 → v1.6 §Status version bump +
  §Version History v1.6 row documenting the 32-byte addendum.
- **AC-3:** RFC-0010 §Storage Substrate subsection (new or
  expanded) cross-references `ChainId::as_bytes()` and the
  storage `BLOB(32)` column type.
- **AC-4:** 3 TV (TV-1 determinism, TV-2 known-vector, TV-3
  coexistence) PASS in
  `crates/octo-ident/tests/tv_0010_chain_id_32byte.rs`.
- **AC-5:** Pre-existing `ChainId` tests still PASS (no
  regression in `canonical_bytes()` 17-byte form).
- **AC-6:** `cargo test -p octo-ident --lib` +
  `cargo test -p octo-ident --tests` —
  full green.
- **AC-7:** `cargo clippy --all-targets -p octo-ident -- -D warnings`
  — clean.
- **AC-8:** `cargo fmt --all -- --check` — clean.

## Risks

- **LOW:** BLAKE3 domain separator string
  `"cipherocto/chain/v1/"` must match the convention used by
  `AssetId::as_bytes()` (`"cipherocto/asset/v1/"`,
  `crates/octo-vault/src/lib.rs`) and `vault_id()`
  (`"cipherocto/vault/v1/"`, `crates/octo-vault/src/lib.rs`).
  Mitigation: grep for `cipherocto/chain/v1/` to confirm no
  pre-existing collision; use literal byte-string for
  BLAKE3 to avoid format! overhead.
- **LOW:** RFC-0010 v1.6 amendment is additive — no existing
  consumer breaks. Pre-v1.6 payloads remain valid (17-byte form
  unchanged).
- **LOW:** `as_bytes()` semantics — verify the method returns
  the canonical 32-byte form, not the 17-byte form padded.
  Mitigation: TV-3 explicitly asserts the two outputs differ.

## Out of scope

- Replacing `ChainId(pub String)` representation (would require
  RFC-0010 v2.0; defer).
- Reconciliation of 17-byte form with 32-byte form (both
  coexist per §1087; no reconciliation needed).
- Storage substrate changes (already uses `[u8; 32]`).

## Verification

```bash
cd /home/mmacedoeu/_w/ai/cipherocto

# After patch:
cargo test -p octo-ident --test tv_0010_chain_id_32byte
# Expected: 3/3 green

cargo test -p octo-ident --lib
# Expected: full green (no regression in canonical_bytes tests)

cargo clippy --all-targets -p octo-ident -- -D warnings
cargo fmt --all -- --check

# Coexistence grep:
grep -n 'pub fn as_bytes\|pub fn canonical_bytes' crates/octo-ident/src/chain.rs
# Expected: both methods present
```

## Files

- `crates/octo-ident/src/chain.rs` — 1 new method `as_bytes()` (~5 lines)
- `crates/octo-ident/tests/tv_0010_chain_id_32byte.rs` — NEW file (3 TV)
- `rfcs/accepted/process/0010-canonical-did-codec.md` — v1.5 → v1.6 bump
