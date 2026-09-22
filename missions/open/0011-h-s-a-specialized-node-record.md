# 0011-h-s-a-specialized-node-record — Substrate additions for SpecializedNodeRecord (RFC-0871 substrate)

## Status

Completed (2026-09-20) — Substrate-additions prerequisite per RFC-0011-h §Substrate-Additions Companion Missions row G11 + RFC-0011-q Phase 9 §Substrate-Additions Companion Missions. Substrate slice LANDED at `next 8c11d683` (NEW `crates/octo-network/src/specialized/node_record.rs` + `mod.rs` + `pub mod specialized;` insertion). 6 unit tests added (1489/1489 octo-network lib tests pass). CLI dispatch slice LANDED at `next 54ac266d` (NetworkAction::Node + NetworkNodeAction + NodeShowArgs + NodeBindArgs + 2 envelopes + 2 handlers + specialized_node_registry + node_class_label + 6 test vectors tv_net9_1 through tv_net9_6). 421/421 octo-cli tests pass (was 415). Paired CLI mission `0011-h-network-node` CREATED Completed at `next PENDING`. Stub fill-in landed at `next c0a33e5e`. Paired-YAML Claimed at `next 7a3f0bd5`. RFC-0011-q Phase 9 specialized-node amendment Draft landed at `next c0287128`.

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G11 + RFC-0011-q Phase 9 §Substrate-Additions Companion Missions + RFC-0871 Specialized Node Protocol Envelope.

## Summary

Adds `SpecializedNodeRecord` struct + `NodeClass` enum + `SpecializedNodeRecordAccess` trait + `SpecializedNodeError` enum to NEW module `crates/octo-network/src/specialized/node_record.rs` (NEW `specialized/` subdir per RFC-0871 substrate path). Required by `octo network node show` (read) + `octo network node bind <node_id_hex> --holder-did <did>` (mutating) per RFC-0011-q Phase 9 §Subcommand Taxonomy.

### Substrate additions target

```rust
// crates/octo-network/src/specialized/node_record.rs (NEW module)
use octo_did::Did;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// `SpecializedNodeRecord` — substrate-faithful specialized node
/// record per RFC-0871. Per RFC-0011-q Phase 9 §Substrate Mapping
/// Table, this struct is the Layer B substrate projection consumed
/// by `octo network node show` + `octo network node bind`.
/// Per-extension transport impl crates (Layer D) are OUT OF SCOPE.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecializedNodeRecord {
    /// 32-byte canonical node identifier.
    pub node_id: [u8; 32],
    /// Holder DID post-bind (None pre-bind).
    pub holder_did: Option<Did>,
    /// Node class (Builder + Provider + Storage + Bandwidth +
    /// Orchestrator).
    pub node_class: NodeClass,
    /// Creation epoch (RFC-0855 §epoch).
    pub creation_epoch: u64,
    /// Operator metadata. BTreeMap for deterministic iteration.
    pub metadata: BTreeMap<String, String>,
}

/// `NodeClass` — closed enum of specialized node classes per
/// RFC-0871 §Node Taxonomy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeClass {
    Builder,
    Provider,
    Storage,
    Bandwidth,
    Orchestrator,
}

/// `SpecializedNodeRecordAccess` — trait abstraction over
/// specialized node record operations per RFC-0011-q Phase 9
/// per-extension crate pattern. Trait in Layer B; concrete
/// impl crates (substrate-ext-specialized-node-*) in Layer D,
/// OUT OF SCOPE.
pub trait SpecializedNodeRecordAccess: Send + Sync {
    /// Load a specialized node record by node_id (returns
    /// None if not found).
    fn load(&self, node_id: &[u8; 32]) -> Option<SpecializedNodeRecord>;
    /// Bind a node to a holder DID (reversible: returns
    /// AlreadyBound if previously bound).
    fn bind_to_did(
        &mut self,
        node_id: &[u8; 32],
        holder_did: &Did,
    ) -> Result<(), SpecializedNodeError>;
    /// Return the local node_id (32-byte canonical).
    fn node_id(&self) -> [u8; 32];
}

/// `SpecializedNodeError` — error enum for the trait surface.
/// `#[non_exhaustive]` for forward-compatible variant growth.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SpecializedNodeError {
    /// Node not found in local registry.
    NotFound,
    /// Node already bound to a different DID (re-bind returns
    /// this error).
    AlreadyBound,
    /// Holder DID string is malformed.
    InvalidDid(String),
    /// Internal error with opaque message.
    Internal(String),
}
```

Layer B substrate additions land in NEW module `crates/octo-network/src/specialized/node_record.rs`. Module registered via `pub mod specialized;` insertion in `crates/octo-network/src/lib.rs` + `pub mod node_record;` insertion in `crates/octo-network/src/specialized/mod.rs`.

`BTreeMap` chosen over `HashMap` for deterministic iteration order (RFC-0011-h §Output Envelope order determinism).

Per-extension crate pattern preserved: trait is in `octo-network` Layer B; concrete per-transport impl crates (substrate-ext-specialized-node-*) are OUT OF SCOPE for follow-on Layer D adapter missions.

## Acceptance Criteria

- [x] `SpecializedNodeRecord` struct lands in NEW module `crates/octo-network/src/specialized/node_record.rs` per RFC-0011-h §Substrate-Additions row G11 + RFC-0011-q Phase 9 §Substrate Mapping Table
- [x] `node_id: [u8; 32]` + `holder_did: Option<Did>` + `node_class: NodeClass` + `creation_epoch: u64` + `metadata: BTreeMap<String, String>` fields land
- [x] `NodeClass` enum (Builder + Provider + Storage + Bandwidth + Orchestrator) lands at same path with `#[serde(rename_all = "lowercase")]`
- [x] `SpecializedNodeRecordAccess` trait (load + bind_to_did + node_id) lands at same path
- [x] `SpecializedNodeError` enum (NotFound + AlreadyBound + InvalidDid(String) + Internal(String)) lands at same path with `#[non_exhaustive]`
- [x] `pub mod specialized;` insertion in `crates/octo-network/src/lib.rs` + `pub mod node_record;` insertion in NEW `crates/octo-network/src/specialized/mod.rs`
- [x] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-network --lib` green (≥5 unit tests added above Phase 8 baseline of 1477)
- [x] Layer discipline preserved (Layer B only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [x] ≥5 unit tests + ≥1 integration test (substrate-faithful boundary tests pin load-miss + load-hit + bind-success + bind-already-bound + bind-not-found + BTreeMap deterministic ordering)

## Dependencies

- RFC-0011-h Accepted (RFC-0011-h must be Accepted before this mission lands per RFC-0011-h §Substrate-Additions Companion Missions)
- RFC-0011-q Phase 9 specialized-node amendment Draft at `next c0287128`
- RFC-0871 Specialized Node Protocol Envelope (governing RFC)

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-node` covers that surface; CREATED at CLI dispatch slice time per user decision)
- Wire format versioning (deferred to RFC-0011-h §Future Work items F8 + F9)
- Per-extension transport impl (substrate-ext-specialized-node-* Layer D follow-on missions, OUT OF SCOPE for this trait-only phase)
- Persistence adapter (in-memory mutation only; persistence in follow-on Layer D adapter mission per per-extension crate pattern)
- Real DID registry integration (uses `octo-did` crate; out of scope per RFC-0011-h)

## Notes

Stub originally filed 2026-09-18 per [[no-phantom-mission-pointers]]. Full AC + scope land in Phase 9 stub fill-in commit at `next PENDING` per the Phase 4 paired-substrate completion pattern. Phase 9 follows the Phase 5 RFC-0011-m 5-commit pattern (stub fill-in → substrate slice → YAML Claimed → CLI dispatch → YAMLs Completed) verified at `next 8e7c5cec`, `24bfec96`, `fcb58331`, `346f10cc`, `97955c00`. Slot 89 `NetworkSubstrateUnavailable` REUSE per Phase 6 precedent (0 NEW OctoCliError variants). BTreeMap determinism per RFC-0011-h §Output Envelope order determinism. pastejacking defense via `parse_32_byte_hex` shared helper per RFC-0011-h §Pastejacking Defense pattern.
