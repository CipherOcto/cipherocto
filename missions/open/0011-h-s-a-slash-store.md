# 0011-h-s-a-slash-store — Substrate additions for SlashReputationStoreCompat envelope log

## Status

Completed (2026-09-20) — Substrate additions LANDED at `next 931dc7b1` + paired CLI dispatch LANDED at `next 8649ca4d`. Substrate-faithful additions to `SlashReputationStoreCompat` (envelope log + `record_slash_envelope` + `list` + `show` + `envelope_count` + `SlashListFilter`) land in `crates/octo-network/src/reputation/slash_store.rs`. Substrate-additions prerequisite per RFC-0011-h §Substrate-Additions Companion Missions row G6.

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G6

## Summary

Adds `SlashListFilter` struct + per-event envelope log + `record_slash_envelope` + `list` + `show` + `envelope_count` methods to `SlashReputationStoreCompat`. Required by `octo network slash {excluded,stats,list,show}` CLI dispatch.

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

Paired CLI dispatch landed 2026-09-20 at `next 8649ca4d`:

- 4 handlers in `crates/octo-cli/src/commands/network.rs` route through G6 substrate-faithfully
- 7 test vectors cover the substrate-to-CLI dispatch surface

## Acceptance Criteria

- [x] Substrate additions land in `crates/octo-network/src/reputation/slash_store.rs` per RFC-0011-h §Substrate-Additions row G6
- [x] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-network --lib` green (19/19 slash_store tests pass)
- [x] Layer discipline preserved (Layer B only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [x] ≥3 unit tests + ≥1 integration test (5 unit tests added)
- [x] Paired CLI dispatch wired through G6 substrate surface (per `0011-h-network-slash-stats` mission closed at `next 8649ca4d`)

## Dependencies

Hard sequencing: RFC-0011-h must be Accepted before this mission lands.

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-slash-stats` covers that surface — completed at `next 8649ca4d`)
- Wire format versioning (deferred to substrate-additions companion)
- Per-extension transport impl (deferred to per-extension crate pattern)

## Notes

Phase 2 G6 closed. Substrate slice landed at `next 931dc7b1`; paired CLI slice landed at `next 8649ca4d`. The Phase 2 IMPLEMENTATION loop closed the substrate-to-CLI dispatch arc per the substrate-first ordering principle.
