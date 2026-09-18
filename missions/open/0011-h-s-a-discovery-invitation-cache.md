# 0011-h-s-a-discovery-invitation-cache — Substrate additions for MissionInvitationCache::get lookup

## Status

Open (2026-09-18) — Substrate-additions prerequisite per RFC-0011-h §Substrate-Additions Companion Missions row G24

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G24

## Summary

Lookup façade for cached mission invitations. Required by `octo network discovery invitation show`.

### Substrate additions target

```rust
// crates/octo-network/src/discovery/invitation_cache.rs (NEW)
cache
```

(Stub: full type signatures + ACs land in Phase X of this mission's own RFC/DRY cycle per [[no-phantom-mission-pointers]].)

## Acceptance Criteria

- [ ] Substrate additions land in `crates/octo-network/src/discovery/invitation_cache.rs (NEW)` per RFC-0011-h §Substrate-Additions row G24
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
