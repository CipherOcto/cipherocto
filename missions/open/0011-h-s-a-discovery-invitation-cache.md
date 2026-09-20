# 0011-h-s-a-discovery-invitation-cache — Substrate additions for MissionInvitationCache::get + iter

## Status

Open (2026-09-20) — Substrate-additions prerequisite per RFC-0011-h §Substrate-Additions Companion Missions row G24 + RFC-0011-m Phase 5 §Substrate-Additions Companion Missions. Substrate slice pending per the Phase 4 paired-substrate completion pattern (companion YAML filled in → substrate lands → YAML Claimed → CLI dispatch lands → YAML Completed paired).

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G24 + RFC-0011-m Phase 5 §Substrate-Additions Companion Missions + RFC-0855 §8.2 Mission Invitation

## Summary

Adds lookup + iteration façade for cached mission invitations. Required by `octo network discovery invitation show` (Phase 5 G24 substrate companion).

### Substrate additions target

```rust
// crates/octo-network/src/mon/discovery.rs (existing module extended)
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MissionInvitationCache {
    entries: BTreeMap<[u8; 32], MissionInvitation>,
}

impl MissionInvitationCache {
    /// Substrate-faithful lookup helper for `octo network
    /// discovery invitation show --invitation-id <ID>`
    /// (RFC-0011-m Phase 5 G24).
    #[must_use]
    pub fn get(&self, invitation_id: &[u8; 32]) -> Option<&MissionInvitation> {
        self.entries.get(invitation_id)
    }

    /// Substrate-faithful iterator for `octo network
    /// discovery invitation show` (RFC-0011-m Phase 5 G24).
    /// Returns gateway-id-keyed iteration per RFC-0855 §8.2.
    pub fn iter(&self) -> impl Iterator<Item = ([u8; 32], &MissionInvitation)> {
        self.entries.iter().map(|(k, v)| (*k, v))
    }

    /// Insert or replace an invitation entry (substrate-faithful
    /// registry surface; Phase 5 closure path remains
    /// `AdapterUnwired` for write paths).
    pub fn insert(&mut self, invitation: MissionInvitation) {
        // Invitation key = BLAKE3-256 hash of signing bytes
        // (RFC-0855 §8.2 deterministic-key contract).
        let key = *blake3::hash(&invitation.to_signing_bytes()).as_bytes();
        self.entries.insert(key, invitation);
    }

    /// Number of cached invitations (operator-side
    /// observability helper).
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty (operator-side
    /// observability helper).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
```

Layer B substrate additions land in `crates/octo-network/src/mon/discovery.rs` (existing module extended alongside the G23 MissionAdvertisementCache). No new module: the cache types share the same substrate anchors (`MissionInvitation`, `MissionId`) per RFC-0855 §8.2.

`BTreeMap` chosen over `HashMap` for deterministic iteration order (RFC-0011-h §Output Envelope order determinism) and substrate-faithful ordering on `iter()` output (RFC-0855 §8.2 deterministic ordering contract). The invitation key is the BLAKE3-256 hash of `to_signing_bytes()` per RFC-0855 §8.2 deterministic-key contract (substrate-faithful mapping from RFC-0855 §8.2 "invitation-key = blake3(to_signing_bytes())").

## Acceptance Criteria

- [ ] `MissionInvitationCache` struct lands in `crates/octo-network/src/mon/discovery.rs` per RFC-0011-h §Substrate-Additions row G24 (next PENDING substrate slice)
- [ ] `get(invitation_id: &[u8; 32]) -> Option<&MissionInvitation>` method lands at same path
- [ ] `iter() -> impl Iterator<Item = ([u8; 32], &MissionInvitation)>` method lands at same path
- [ ] `insert(MissionInvitation)` registry helper lands (substrate-faithful surface; CLI dispatch does NOT call this — write paths remain `AdapterUnwired` per Phase 6 follow-on `0011-h-s-a-discovery-invitation-persistence`)
- [ ] `len()` + `is_empty()` observability helpers land
- [ ] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [ ] `cargo test -p octo-network --lib` green (≥3 unit tests added above Phase 4 baseline of 1438, paired with G23 unit tests)
- [ ] Layer discipline preserved (Layer B only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [ ] ≥3 unit tests + ≥1 integration test (substrate-faithful boundary tests pin get-miss + get-hit + iter-empty + iter-non-empty + insert-idempotent + BLAKE3 signing-bytes deterministic key derivation + BTreeMap deterministic ordering)

## Dependencies

Hard sequencing: RFC-0011-h must be Accepted before this mission lands. Substrate-first ordering per [[no-phantom-mission-pointers]]: G24 substrate slice (this mission) lands BEFORE Phase 5 CLI dispatch slice.

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-discovery` covers that surface in Phase 5 CLI dispatch slice)
- Wire format versioning (deferred to substrate-additions companion)
- Per-extension transport impl (deferred to per-extension crate pattern)
- Persistence adapter (Phase 6 follow-on per `0011-h-s-a-discovery-invitation-persistence`)
- Signature verification (substrate-side; CLI side hex-encodes signing bytes per RFC-0011-m §Security Considerations)

## Notes

Stub fill-in 2026-09-20 per RFC-0011-m closure card. Substrate slice pending per directive sequencing (substrate coding is LAST). The substrate-faithful `Option<&MissionInvitation>` translation surfaces as typed exit 89 `NetworkSubstrateUnavailable { companion: "G24" }` once the CLI dispatch slice consumes it. The `BTreeMap` choice honors the deterministic ordering contract per RFC-0855 §8.2 + RFC-0011-h §Output Envelope order determinism. The invitation key is the BLAKE3-256 hash of `to_signing_bytes()` — substrate-faithful mapping from RFC-0855 §8.2 "invitation-key = blake3(to_signing_bytes())" so the same invitation can be looked up regardless of which gateway produced it. The `iter()` method returns owned `[u8; 32]` keys so the iterator lifetime is decoupled from the cache lifetime (substrate-faithful boundary per [[cipherocto-design-principles]] §No premature coupling).
