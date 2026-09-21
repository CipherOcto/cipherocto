# 0011-h-s-a-envelope-inspector — Substrate additions for EnvelopeInspector::inspect (RFC-0855 §Wire Format)

## Status

Open (2026-09-18) — Substrate-additions prerequisite per RFC-0011-h §Substrate-Additions Companion Missions row G16

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G16 + RFC-0011-u Phase 13 G16a envelope-inspector amendment Draft.

## Summary

Substrate-side envelope metadata inspector. Required by `octo network envelope inspect <envelope_id_hex>` per RFC-0011-u Phase 13 §Subcommand Taxonomy.

### Substrate additions target

NEW module `crates/octo-network/src/mon/envelope_inspector.rs` (per Phase 7 RFC-0011-o NEW module precedent for SlashBridge) with NEW additive types:

```rust
// crates/octo-network/src/mon/envelope_inspector.rs (NEW module)
use crate::mon::mission_id::MissionId;

/// Read-only inspector of envelope metadata (RFC-0855 §Wire Format).
///
/// Phase 13 G16a per RFC-0011-u §Substrate Mapping Table. Operates
/// on the in-memory snapshot; live envelope store OUT OF SCOPE for
/// Phase 13. Per-extension impl crates (Layer D) provide real
/// envelope stores in follow-on missions. BTreeMap-based
/// deterministic iteration ordering preserved per RFC-0011-h
/// §Output Envelope determinism.
#[derive(Clone, Debug, Default)]
pub struct EnvelopeInspector;

/// Envelope metadata returned by `inspect()`.
///
/// Phase 13 G16a per RFC-0011-u §Substrate Mapping Table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvelopeMeta {
    pub envelope_id: [u8; 32],
    pub envelope_kind: EnvelopeKind,
    pub creator_did_hex: String,
    pub creation_epoch: u64,
    pub ttl_epochs: u64,
}

/// Envelope kind discriminator (RFC-0855 §Wire Format envelope kinds).
///
/// Phase 13 G16a per RFC-0011-u §Substrate Mapping Table. Additive
/// enum on the NEW module per Phase 7 RFC-0011-o precedent; no
/// central edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EnvelopeKind {
    Mission,
    Governance,
    Reputation,
    Slash,
    Forward,
    Bootstrap,
    Discovery,
    Coordinator,
}

impl EnvelopeInspector {
    /// Inspect an envelope by 32-byte envelope_id; returns `None`
    /// for unknown envelopes (per RFC-0011-u §Envelope Inspect
    /// semantics). Phase 13 returns None unconditionally (live
    /// envelope store OUT OF SCOPE for Phase 13).
    pub fn inspect(&self, envelope_id: [u8; 32]) -> Option<EnvelopeMeta> {
        // Phase 13 additive-type-only: real envelope store wiring
        // OUT OF SCOPE; this stub returns None for unknown envelopes
        // per RFC-0011-u §Envelope Inspect semantics.
        let _ = envelope_id;
        None
    }

    /// Construct a synthetic EnvelopeMeta from explicit fields
    /// (substrate-faithful projection path for CLI dispatch to
    /// exercise the type surface end-to-end).
    pub fn from_fields(
        envelope_id: [u8; 32],
        envelope_kind: EnvelopeKind,
        creator_did_hex: String,
        creation_epoch: u64,
        ttl_epochs: u64,
    ) -> EnvelopeMeta {
        EnvelopeMeta {
            envelope_id,
            envelope_kind,
            creator_did_hex,
            creation_epoch,
            ttl_epochs,
        }
    }
}
```

NEW module `crates/octo-network/src/mon/envelope_inspector.rs`. No existing modules touched. Zero regression on existing modules.

## Acceptance Criteria

- [ ] NEW module `crates/octo-network/src/mon/envelope_inspector.rs` lands per RFC-0011-h §Substrate-Additions row G16 + RFC-0011-u Phase 13 §Substrate Mapping Table
- [ ] `EnvelopeInspector` struct lands (Clone + Debug + Default)
- [ ] `EnvelopeMeta` struct lands with all 5 fields (envelope_id + envelope_kind + creator_did_hex + creation_epoch + ttl_epochs)
- [ ] `EnvelopeKind` enum lands with 8 variants (Mission + Governance + Reputation + Slash + Forward + Bootstrap + Discovery + Coordinator)
- [ ] `EnvelopeInspector::inspect(envelope_id) -> Option<EnvelopeMeta>` method lands (returns None for Phase 13 stub)
- [ ] `EnvelopeInspector::from_fields(...)` constructor lands for substrate-faithful projection exercise
- [ ] `pub mod envelope_inspector;` added to `crates/octo-network/src/mon/mod.rs`
- [ ] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [ ] `cargo test -p octo-network --lib` green (≥3 unit tests added; zero regression)
- [ ] Layer discipline preserved (Layer B only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [ ] ≥3 unit tests + ≥1 integration test (substrate-faithful boundary tests pin unknown-envelope semantics + from_fields projection + EnvelopeKind enum variants)

## Dependencies

- RFC-0011-h Accepted (RFC-0011-h must be Accepted before this mission lands per RFC-0011-h §Substrate-Additions Companion Missions)
- RFC-0011-u Phase 13 envelope-inspector amendment Draft
- Phase 13 G16a RFC draft at `next 296d9a1e`
- Existing `MissionId` at `crates/octo-network/src/mon/mission_id.rs` (REUSE, not NEW)

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-envelope` covers that surface; CREATED at CLI dispatch slice time per user decision)
- Wire format versioning (deferred to RFC-0011-h §Future Work items F8 + F9)
- Per-extension transport impl (substrate-ext-envelope-store-* Layer D follow-on missions, OUT OF SCOPE for this additive-type-only phase)
- Live envelope store adapter (Phase 13 substrate operates on the in-memory snapshot only; real store in follow-on Layer D adapter mission)

## Notes

Stub originally filed 2026-09-18 per [[no-phantom-mission-pointers]]. Stub note pinned path `crates/octo-network/src/mon/envelope_inspector.rs (NEW)` — substrate-faithful per Phase 7 RFC-0011-o SlashBridge NEW module precedent. Full AC + scope land in Phase 13 stub fill-in commit at `next PENDING` per the Phase 5 RFC-0011-m 5-commit pattern. Phase 13 follows the Phase 5 5-commit pattern (stub fill-in → substrate slice → YAML Claimed → CLI dispatch → YAMLs Completed) verified at `next 8e7c5cec`, `24bfec96`, `fcb58331`, `346f10cc`, `97955c00`. Slot 89 `NetworkSubstrateUnavailable` REUSE per Phase 6 precedent (0 NEW OctoCliError variants). BTreeMap-based deterministic iteration ordering preserved per RFC-0011-h §Output Envelope determinism. `EnvelopeInspector::inspect()` operates on in-memory snapshot only; live envelope store in follow-on Layer D adapter mission per per-extension crate pattern.
