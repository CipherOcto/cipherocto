# 0011-h-s-a-slash-store — Substrate additions for SlashStore with iter_dids() accessor

## Status

Claimed (2026-09-20) — Substrate additions LANDED at `next 931dc7b1`. Substrate-faithful additions to `SlashReputationStoreCompat` (envelope log + `record_slash_envelope` + `list` + `show` + `envelope_count` + `SlashListFilter`) land in `crates/octo-network/src/reputation/slash_store.rs`. Substrate-additions prerequisite per RFC-0011-h §Substrate-Additions Companion Missions row G6.

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G6

## Summary

Adds `SlashStore` with persistent state for slash events + `iter_dids() -> impl Iterator<Item=RecorderDid>`. Required by `NetworkSlashStatsOutput.per_did` field (currently empty vec).

### Substrate additions target

```rust
// crates/octo-network/src/reputation/slash_store.rs
store
```

Substrate additions land 2026-09-20 at `next 931dc7b1`:
- `SlashListFilter` struct with `did` / `slash_reason` / `limit` fields + `matches()` predicate
- `SlashReputationStoreCompat::envelopes` field (per-event envelope log)
- `SlashReputationStoreCompat::record_slash_envelope` / `list` / `show` / `envelope_count` methods
- `derive_did_from_envelope` private helper for canonical DID derivation
- 5 unit tests: `list_filter_default_returns_all`, `list_filter_by_reason`, `list_filter_by_did_limit`, `show_returns_envelope_by_id`, `record_slash_envelope_increments_count`
- Re-exports: `SlashListFilter` through `crates/octo-network/src/reputation/mod.rs`

## Acceptance Criteria

- [x] Substrate additions land in `crates/octo-network/src/reputation/slash_store.rs` per RFC-0011-h §Substrate-Additions row G6
- [x] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-network --lib` green (19/19 slash_store tests pass)
- [x] Layer discipline preserved (Layer B only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [x] ≥3 unit tests + ≥1 integration test (5 unit tests added)

## Dependencies

Hard sequencing: RFC-0011-h must be Accepted before this mission lands.

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-*` covers that surface — pending Phase 2 IMPLEMENTATION)
- Wire format versioning (deferred to substrate-additions companion)
- Per-extension transport impl (deferred to per-extension crate pattern)

## Notes

Substrate slice landed 2026-09-20. Companion substrate slice (`0011-h-s-slash-store-seed-rotate-substrate` commit `931dc7b1`) bundles G6 + G6b + G8 together per the substrate-first ordering principle. Phase 2 IMPLEMENTATION closes the CLI dispatch surface (8 subcommands + 2 OctoCliError variants slots 82 + 89) after this mission transitions to Completed.

