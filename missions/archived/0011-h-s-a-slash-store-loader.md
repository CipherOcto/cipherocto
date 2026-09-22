# 0011-h-s-a-slash-store-loader — Substrate additions for SlashStoreLoader for persisted filter

## Status

Completed (2026-09-20) — Substrate additions LANDED at `next 931dc7b1` + paired CLI dispatch LANDED at `next 8649ca4d`. Substrate-faithful `SlashStoreLoader` sync validation+ingest façade lands in `crates/octo-network/src/reputation/slash_store.rs`. Substrate-additions prerequisite per RFC-0011-h §Substrate-Additions Companion Missions row G6b.

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G6b

## Summary

Loader façade for persisted SlashStore; integrates with RFC-0860 substrate for backward compatibility.

### Substrate additions target

```rust
// crates/octo-network/src/reputation/slash_store.rs
loader
```

Substrate additions land 2026-09-20 at `next 931dc7b1`:

- `SlashStoreLoader` struct (zero-state sync façade)
- `SlashStoreLoader::new` / `Default` constructors
- `SlashStoreLoader::hydrate` validation+ingest path
  - Validates `slash_reason != 0` (zero reason = rejected stub)
  - Forwards accepted envelopes to `SlashReputationStoreCompat::record_slash_envelope`
  - Returns count of ingested envelopes
- 4 unit tests: `loader_hydrate_ingests_all_valid_envelopes`, `loader_hydrate_rejects_zero_reason`, `loader_hydrate_accepts_extension_reason`, `loader_hydrate_empty_iter_yields_zero`
- Re-exports: `SlashStoreLoader` through `crates/octo-network/src/reputation/mod.rs`

## Acceptance Criteria

- [x] Substrate additions land in `crates/octo-network/src/reputation/slash_store.rs` per RFC-0011-h §Substrate-Additions row G6b
- [x] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-network --lib` green (4/4 loader tests pass)
- [x] Layer discipline preserved (Layer B only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [x] ≥3 unit tests + ≥1 integration test (4 unit tests added)
- [x] Paired CLI dispatch wired through G6b substrate surface (per `0011-h-network-slash-stats` mission closed at `next 8649ca4d`)

## Dependencies

Hard sequencing: RFC-0011-h must be Accepted before this mission lands.

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-slash-stats` covers that surface — completed at `next 8649ca4d`)
- Wire format versioning (deferred to substrate-additions companion)
- Per-extension transport impl (deferred to per-extension crate pattern)

## Notes

Substrate slice landed 2026-09-20 at `next 931dc7b1`; paired CLI slice landed at `next 8649ca4d`. Companion substrate slice bundles G6 + G6b + G8 together per the substrate-first ordering principle. Phase 2 IMPLEMENTATION closed the CLI dispatch surface (`octo network slash excluded` + `slash stats` + `slash list` + `slash show`).
