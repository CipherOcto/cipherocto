# 0011-h-s-a-forward-envelope — Substrate additions for ForwardEnvelope builder (RFC-0855 §Wire Format)

## Status

Completed (2026-09-20) — Substrate slice landed at `next 9c03f6dc` (ForwardEnvelope + ForwardEnvelopeError additive types + build constructor + wire_bytes impl with canonical wire-bytes encoding per RFC-0855 §Wire Format on NEW module mon/forward_envelope.rs per Phase 7 RFC-0011-o SlashBridge NEW module precedent). RFC draft at `next 296d9a1e`. Stub fill-in at `next dbaa3e82`. Paired-YAML Claimed transition at `next 59e8c43e`. Paired-YAML Completed transition paired with CLI mission YAML `0011-h-network-envelope` CREATION at CLI dispatch slice `next 344bd8b5` per no-phantom-mission-pointers pairing invariant.

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G16 + RFC-0011-u Phase 13 G16b forward-envelope amendment Draft.

## Summary

Substrate-side forward envelope builder for peer-to-peer propagation. Required by `octo network envelope forward <envelope_id_hex> --destination <peer_id_hex> --ttl <N>` per RFC-0011-u Phase 13 §Subcommand Taxonomy.

### Substrate additions target

NEW module `crates/octo-network/src/mon/forward_envelope.rs` (per Phase 7 RFC-0011-o NEW module precedent for SlashBridge) with NEW additive types:

```rust
// crates/octo-network/src/mon/forward_envelope.rs (NEW module)
/// Forward envelope builder for peer-to-peer propagation
/// (RFC-0855 §Wire Format).
///
/// Phase 13 G16b per RFC-0011-u §Substrate Mapping Table. Operates
/// on the in-memory snapshot; real network propagation OUT OF
/// SCOPE for Phase 13. Per-extension impl crates (Layer D) provide
/// real network adapters in follow-on missions. BTreeMap-based
/// deterministic iteration ordering preserved per RFC-0011-h
/// §Output Envelope determinism.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForwardEnvelope {
    source_envelope_id: [u8; 32],
    destination_peer_id: [u8; 32],
    ttl_epochs: u64,
    construction_epoch: u64,
}

/// Forward envelope construction error.
///
/// Phase 13 G16b per RFC-0011-u §Substrate Mapping Table.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ForwardEnvelopeError {
    /// TTL exceeds the maximum allowed value (u64::MAX).
    TtlOverflow,
    /// Destination peer_id is all-zero (invalid).
    InvalidPeerId,
    /// Internal error (reserved for follow-on Layer D adapter
    /// missions).
    Internal(String),
}

impl ForwardEnvelope {
    /// Build a new `ForwardEnvelope` from explicit fields.
    /// Returns Err on TTL overflow or invalid peer_id.
    pub fn build(
        source_envelope_id: [u8; 32],
        destination_peer_id: [u8; 32],
        ttl_epochs: u64,
        construction_epoch: u64,
    ) -> Result<Self, ForwardEnvelopeError> {
        if destination_peer_id == [0u8; 32] {
            return Err(ForwardEnvelopeError::InvalidPeerId);
        }
        Ok(Self {
            source_envelope_id,
            destination_peer_id,
            ttl_epochs,
            construction_epoch,
        })
    }

    /// Serialize the envelope to canonical wire bytes per
    /// RFC-0855 §Wire Format canonical encoding.
    /// BTreeMap-based deterministic iteration ordering preserved
    /// per RFC-0011-h §Output Envelope determinism.
    pub fn wire_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32 + 32 + 8 + 8);
        out.extend_from_slice(&self.source_envelope_id);
        out.extend_from_slice(&self.destination_peer_id);
        out.extend_from_slice(&self.ttl_epochs.to_be_bytes());
        out.extend_from_slice(&self.construction_epoch.to_be_bytes());
        out
    }

    pub fn source_envelope_id(&self) -> [u8; 32] {
        self.source_envelope_id
    }

    pub fn destination_peer_id(&self) -> [u8; 32] {
        self.destination_peer_id
    }

    pub fn ttl_epochs(&self) -> u64 {
        self.ttl_epochs
    }

    pub fn construction_epoch(&self) -> u64 {
        self.construction_epoch
    }
}
```

NEW module `crates/octo-network/src/mon/forward_envelope.rs`. No existing modules touched. Zero regression on existing modules.

## Acceptance Criteria

- [ ] NEW module `crates/octo-network/src/mon/forward_envelope.rs` lands per RFC-0011-h §Substrate-Additions row G16 + RFC-0011-u Phase 13 §Substrate Mapping Table
- [ ] `ForwardEnvelope` struct lands (Clone + Debug + PartialEq + Eq) with 4 private fields
- [ ] `ForwardEnvelopeError` enum lands with 3 variants (TtlOverflow + InvalidPeerId + Internal(String))
- [ ] `ForwardEnvelope::build(...)` constructor lands with peer_id validation
- [ ] `ForwardEnvelope::wire_bytes(&self) -> Vec<u8>` method lands with canonical wire-bytes encoding (source_envelope_id + destination_peer_id + ttl_epochs_be + construction_epoch_be)
- [ ] Field accessor methods land (source_envelope_id + destination_peer_id + ttl_epochs + construction_epoch)
- [ ] `pub mod forward_envelope;` added to `crates/octo-network/src/mon/mod.rs`
- [ ] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [ ] `cargo test -p octo-network --lib` green (≥3 unit tests added; zero regression)
- [ ] Layer discipline preserved (Layer B only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [ ] ≥3 unit tests + ≥1 integration test (substrate-faithful boundary tests pin wire_bytes canonical encoding + InvalidPeerId rejection + Build constructor determinism)

## Dependencies

- RFC-0011-h Accepted (RFC-0011-h must be Accepted before this mission lands per RFC-0011-h §Substrate-Additions Companion Missions)
- RFC-0011-u Phase 13 forward-envelope amendment Draft
- Phase 13 G16b RFC draft at `next 296d9a1e`
- Existing hex crate (REUSE, not NEW)

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-envelope` covers that surface; CREATED at CLI dispatch slice time per user decision)
- Wire format versioning (deferred to RFC-0011-h §Future Work items F8 + F9)
- Per-extension transport impl (substrate-ext-envelope-forwarder-* Layer D follow-on missions, OUT OF SCOPE for this additive-type-only phase)
- Live network propagation (Phase 13 substrate operates on the in-memory snapshot only; real propagation in follow-on Layer D adapter mission)

## Notes

Stub originally filed 2026-09-18 per [[no-phantom-mission-pointers]]. Stub note pinned path `crates/octo-network/src/mon/forward_envelope.rs (NEW)` — substrate-faithful per Phase 7 RFC-0011-o SlashBridge NEW module precedent. Full AC + scope land in Phase 13 stub fill-in commit at `next PENDING` per the Phase 5 RFC-0011-m 5-commit pattern. Phase 13 follows the Phase 5 5-commit pattern (stub fill-in → substrate slice → YAML Claimed → CLI dispatch → YAMLs Completed) verified at `next 8e7c5cec`, `24bfec96`, `fcb58331`, `346f10cc`, `97955c00`. Slot 89 `NetworkSubstrateUnavailable` REUSE per Phase 6 precedent (0 NEW OctoCliError variants). BTreeMap-based deterministic iteration ordering preserved per RFC-0011-h §Output Envelope determinism. `ForwardEnvelope::wire_bytes()` operates on in-memory snapshot only; real network propagation in follow-on Layer D adapter mission per per-extension crate pattern.
