# 0011-h-s-a-bind-envelope-lookup — Substrate additions for BindEnvelope::load lookup method

## Status

Open (2026-09-18) — Substrate-additions prerequisite per RFC-0011-h §Substrate-Additions Companion Missions row G22

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G22

## Summary

Adds lookup façade for persisted bind envelopes. Required by `octo network bind-envelope show`.

### Substrate additions target

```rust
// crates/octo-network/src/mon/bind_envelope.rs
lookup
```

(Stub: full type signatures + ACs land in Phase X of this mission's own RFC/DRY cycle per [[no-phantom-mission-pointers]].)

## Acceptance Criteria

- [ ] Substrate additions land in `crates/octo-network/src/mon/bind_envelope.rs` per RFC-0011-h §Substrate-Additions row G22
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
