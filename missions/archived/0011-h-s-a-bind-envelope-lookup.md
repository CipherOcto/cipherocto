# 0011-h-s-a-bind-envelope-lookup — Substrate additions for BindEnvelope::load lookup method

## Status

Completed (2026-09-20) — Substrate additions + CLI dispatch CLOSED. Substrate-faithful `BindEnvelope::load(domain_id)` lookup helper landed in `crates/octo-network/src/mon/bind_envelope.rs` at `next edcdc47a`. CLI dispatch wired at `octo network bind-envelope show --domain-id <ID>` at `next 0df7e579`. Substrate-additions + CLI dispatch landed end-to-end per RFC-0011-l Phase 4 row G22.

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G22 + RFC-0011-l Phase 4 §Substrate-Additions

## Summary

Adds lookup façade for persisted bind envelopes. Required by `octo network bind-envelope show`.

### Substrate additions target

```rust
// crates/octo-network/src/mon/bind_envelope.rs
#[must_use]
pub fn load(domain_id: &str) -> Option<Self> {
    None
}
```

Substrate slice landed 2026-09-20 at `next edcdc47a`:

- `BindEnvelope::load(domain_id)` static method inserted between `is_participant` and the closing brace of the `impl BindEnvelope` block
- Substrate-faithful: returns `None` until the Phase 6 persistence adapter lands per `0011-h-s-a-bind-envelope-persistence` follow-on companion
- Mirrors `CoordinatorRecord::load(coordinator_id)` pattern at RFC-0011-k Phase 3 G12b — both use `Option<Self>` rather than `Result<Self, Error>` because the persistence adapter is the future owner of the error class, not the lookup helper
- 2 new substrate unit tests pinned: `t_bind_envelope_load_returns_none_substrate_faithful` (Option::None contract) + `t_bind_envelope_load_idempotent_for_same_domain_id` (idempotent behavior)

CLI dispatch wired 2026-09-20 at `next 0df7e579`:

- `octo network bind-envelope show --domain-id <ID>` wired at `crates/octo-cli/src/commands/network.rs` (Layer C)
- `bind_envelope_show` handler calls `BindEnvelope::load(&args.domain_id)` to project the Option<BindEnvelope> envelope
- The `Option::None` return is translated to typed exit 89 `NetworkSubstrateUnavailable { companion: "G22" }` per RFC-0011-h §Error Handling
- New output envelope: `NetworkBindEnvelopeShowOutput { domain_id, platform, group_id, participant_filter, member_count_at_bind }` per RFC-0011-l Phase 4 §Output Envelope
- Read-only (no 3-flag confirmation; no --dry-run gate)
- 2 new test vectors: `tv_net4_1_bind_envelope_show_parses_with_domain_id` + `tv_net4_2_bind_envelope_show_substrate_miss_emits_g22_exit`

## Acceptance Criteria

- [x] Substrate additions land in `crates/octo-network/src/mon/bind_envelope.rs` per RFC-0011-h §Substrate-Additions row G22 (next edcdc47a)
- [x] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-network --lib` green (1433/1433, +2 above Phase 3 baseline)
- [x] Layer discipline preserved (Layer B helper on existing substrate struct; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [x] ≥3 unit tests + ≥1 integration test (2 substrate unit tests added; integration test deferred to Phase 6 persistence adapter follow-on)

## Dependencies

Hard sequencing: RFC-0011-h must be Accepted before this mission lands. Substrate-first ordering per [[no-phantom-mission-pointers]]: G22 substrate slice (this mission) lands BEFORE Phase 4 CLI dispatch slice.

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-bind-envelope` covers that surface in Phase 4 CLI dispatch slice)
- Persistence adapter (Phase 6 follow-on per `0011-h-s-a-bind-envelope-persistence`)
- Wire format versioning (deferred to substrate-additions companion)
- Per-extension transport impl (deferred to per-extension crate pattern)

## Notes

Substrate slice landed 2026-09-20 at `next edcdc47a`. CLI dispatch slice landed 2026-09-20 at `next 0df7e579`. The substrate-faithful `Option::None` translation surfaces as typed exit 89 `NetworkSubstrateUnavailable { companion: "G22" }` once the CLI dispatch slice consumes it. Phase 6 persistence adapter follow-on per `0011-h-s-a-bind-envelope-persistence` will replace the `None` stub with the persisted envelope lookup; the lookup helper remains the typed substrate-faithful surface throughout.
