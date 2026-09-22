# 0011-h-s-a-slash-bridge-trait — Substrate additions for SlashBridge trait for substrate ext bridge

## Status

Completed (2026-09-20) — RFC-0011-o Phase 7 amendment Draft landed at `next 27fe1a48`. Substrate slice LANDED at `next 8599f5c8` (SlashBridge trait + BridgedSlash + BridgeReceipt + BridgeError in crates/octo-network/src/mon/slash_bridge.rs, 206 lines + 5 unit tests). CLI dispatch slice LANDED at `next f3b9f48e`. Paired CLI mission YAML at missions/open/0011-h-network-slash-bridge.md CREATED Completed per RFC-0011-m Phase 5 step 5 precedent. Companion substrate mission G9 per RFC-0011-h §Substrate-Additions row 764 + RFC-0011-o §Substrate-Additions Companion Missions.

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G9 + RFC-0011-o §Substrate-Additions Companion Missions

## Summary

Layer B substrate addition: `SlashBridge` trait + `BridgedSlash` + `BridgeReceipt` + `BridgeError` types in NEW module `crates/octo-network/src/mon/slash_bridge.rs`. Trait bridges `SlashStore` (RFC-0011-b Phase 2 substrate) to external reputation substrate via per-extension crate pattern per [[cipherocto-design-principles]] §User extensibility. Per-extension transport impl crates (substrate-ext-bridge-*) are OUT OF SCOPE.

### Substrate additions target

```rust
// crates/octo-network/src/mon/slash_bridge.rs (NEW)
//
// Trait + types for bridging local SlashStore to external reputation substrates.
// Per RFC-0011-h §Substrate-Additions row G9 + RFC-0011-o §Substrate-Additions.
//
// Public surface:
//   trait SlashBridge: Send + Sync
//     fn list(&self) -> Vec<BridgedSlash>;
//     fn propagate_to(&self, slash_envelope_id: [u8; 32])
//         -> Result<BridgeReceipt, BridgeError>;
//
//   pub struct BridgedSlash {
//       pub slash_envelope_id: [u8; 32],
//       pub bridge_metadata: BTreeMap<String, String>, // BTreeMap for determinism
//       pub bridged_at_epoch: u64,
//   }
//
//   pub struct BridgeReceipt {
//       pub slash_envelope_id: [u8; 32],
//       pub propagated_to: Vec<u8>, // opaque per-extension destination
//       pub propagated_at_epoch: u64,
//   }
//
//   #[non_exhaustive]
//   pub enum BridgeError {
//       Unreachable,
//       Refused,
//       PayloadTooLarge,
//       WireFormatMismatch,
//       Internal(String),
//   }
//
// Concrete impl: see per-extension crates (OUT OF SCOPE for this mission).
// CLI consumes trait via runtime registry lookup per RFC-0863 NetworkSender pattern.
```

## Acceptance Criteria

- [x] `SlashBridge` trait + `BridgedSlash` + `BridgeReceipt` + `BridgeError` types land in NEW module `crates/octo-network/src/mon/slash_bridge.rs` per RFC-0011-h §Substrate-Additions row G9 + RFC-0011-o §Substrate-Additions Companion Missions
- [x] `pub mod slash_bridge;` declaration added to `crates/octo-network/src/mon/mod.rs` (alphabetically between `slash_aggregation` and `slashing`)
- [x] `BTreeMap` used (NOT `HashMap`) in `BridgedSlash::bridge_metadata` for deterministic iteration per RFC-0011-h §Output Envelope determinism pattern
- [x] `#[non_exhaustive]` on `BridgeError` enum per RFC-0011-l §non_exhaustive precedent
- [x] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-network --lib` green
- [x] ≥3 unit tests: `test_bridge_list_empty`, `test_bridge_list_populated`, `test_propagate_to_success` + 1 error path test
- [x] Layer discipline preserved (Layer B only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [x] Per-extension crate pattern preserved (trait in Layer B; concrete impl crates in Layer D, OUT OF SCOPE per [[cipherocto-design-principles]] §User extensibility)

## Dependencies

- Hard sequencing: RFC-0011-h must be Accepted before this mission lands — LANDED (`next 8696ec4b`)
- Hard sequencing: RFC-0011-o must be at Draft before this mission claims — LANDED (`next 27fe1a48`)
- Hard sequencing: This mission must close BEFORE CLI dispatch slice per [[no-phantom-mission-pointers]] pairing invariant

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-slash-bridge` covers that surface — CREATED at CLI dispatch slice time per user decision)
- Wire format versioning (deferred to substrate-additions companion per RFC-0011-h §Future Work F8)
- Per-extension transport impl (deferred to per-extension crate pattern; substrate-ext-bridge-libp2p etc OUT OF SCOPE per [[cipherocto-design-principles]] §User extensibility)
- SlashStore substrate (already present from RFC-0011-b Phase 2; this trait consumes it)
- Signer / identity layer (per-extension impl crate concern)

## Notes

Stub filed 2026-09-18 per [[no-phantom-mission-pointers]]. Stub fill-in (full type signatures + AC) landed 2026-09-20 per RFC-0011-m Phase 5 step 1 precedent. Companion mission will transition Open → Claimed → Completed paired with Phase 7 IMPLEMENTATION slice per substrate-first ordering invariant.
