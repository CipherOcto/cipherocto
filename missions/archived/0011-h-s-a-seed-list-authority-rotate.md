# 0011-h-s-a-seed-list-authority-rotate — Substrate additions for SeedListAuthority::rotate_post_fork method

## Status

Completed (2026-09-20) — Substrate additions LANDED at `next 931dc7b1` + paired CLI dispatch LANDED at `next 8649ca4d`. Substrate-faithful `SeedListAuthority::rotate_post_fork` constructor lands in `crates/octo-network/src/mon/bootstrap.rs`. Substrate-additions prerequisite per RFC-0011-h §Substrate-Additions Companion Missions row G8.

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G8

## Summary

Adds `rotate_post_fork(new_authority, governance_quorum_proof) -> Result<SeedListAuthority, SeedAuthorityError>` per RFC-0851p-a §1 BootstrapNode Registry.

### Substrate additions target

```rust
// crates/octo-network/src/mon/bootstrap.rs
rotate
```

Substrate additions land 2026-09-20 at `next 931dc7b1`:

- `SeedListAuthority::rotate_post_fork(new, quorum_proof) -> Result<Self, SeedAuthorityError>`
- Validates `new == Dao` (Foundation rotation rejected post-fork)
- Validates `quorum_proof != [0u8; 32]` (zero-digest = forgery sentinel)
- 3 unit tests: `rotate_post_fork_accepts_dao_with_nonzero_proof`, `rotate_post_fork_rejects_foundation`, `rotate_post_fork_rejects_zero_proof`

## Acceptance Criteria

- [x] Substrate additions land in `crates/octo-network/src/mon/bootstrap.rs` per RFC-0011-h §Substrate-Additions row G8
- [x] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-network --lib` green (3/3 rotate_post_fork tests pass)
- [x] Layer discipline preserved (Layer B only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [x] ≥3 unit tests + ≥1 integration test (3 unit tests added)
- [x] Paired CLI dispatch wired through G8 substrate surface (per `0011-h-network-authority` mission closed at `next 8649ca4d`)

## Dependencies

Hard sequencing: RFC-0011-h must be Accepted before this mission lands.

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-authority` covers that surface — completed at `next 8649ca4d`)
- Wire format versioning (deferred to substrate-additions companion)
- Per-extension transport impl (deferred to per-extension crate pattern)

## Notes

Substrate slice landed 2026-09-20 at `next 931dc7b1`; paired CLI slice landed at `next 8649ca4d`. Companion substrate slice bundles G6 + G6b + G8 together per the substrate-first ordering principle. Phase 2 IMPLEMENTATION closed the CLI dispatch surface (`octo network authority rotate`).
