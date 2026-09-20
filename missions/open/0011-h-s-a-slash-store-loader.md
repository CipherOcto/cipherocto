# 0011-h-s-a-slash-store-loader — Substrate additions for SlashStoreLoader for persisted filter

## Status

Open (2026-09-18) — Substrate-additions prerequisite per RFC-0011-h §Substrate-Additions Companion Missions row G6b

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G6b

## Summary

Loader façade for persisted SlashStore; integrates with RFC-0860 substrate for backward compatibility.

### Substrate additions target

```rust
// crates/octo-network/src/mon/slash_store.rs
loader
```

(Stub: full type signatures + ACs land in Phase X of this mission's own RFC/DRY cycle per [[no-phantom-mission-pointers]].)

## Acceptance Criteria

- [ ] Substrate additions land in `crates/octo-network/src/mon/slash_store.rs` per RFC-0011-h §Substrate-Additions row G6b
- [ ] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [ ] `cargo test -p octo-network --lib` green
- [ ] Layer discipline preserved (Layer B only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [ ] ≥3 unit tests + ≥1 integration test

## Dependencies

Hard sequencing: RFC-0011-h must be Accepted before this mission lands.

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-*` covers that surface)
- Wire format versioning (deferred to substrate-additions companion)
- Per-extension transport impl (deferred to per-extension crate pattern)

## Notes

Stub filed 2026-09-18 per [[no-phantom-mission-pointers]]. Full AC + scope land when work enters Phase X.
