# 0011-h-s-a-network-sender — Substrate additions for NetworkSender trait + SendContext (RFC-0863)

## Status

Completed (2026-09-20) — Substrate slice LANDED at `next a6627e53`. CLI dispatch slice LANDED at `next 2ba4273b`. `NetworkSender` trait + `SendContext` struct + `WireFormat` enum + `NetworkSendError` enum + `SendSummary` struct + `NetworkSenderRegistry` struct landed in NEW directory `crates/octo-network/src/sender/` per RFC-0011-n Phase 6 G20 NEW companion mission + RFC-0863 General-Purpose Network Integration. Per-extension crate pattern preserved (trait in Layer B; per-transport impls OUT OF SCOPE). 11 substrate unit tests added (1472 total octo-network tests, was 1461). Phase 6 CLI dispatch slice atop this substrate LANDED at `next 2ba4273b` (402/402 octo-cli tests, was 396). Phase 6 IMPLEMENTATION CLOSED for G20 per RFC-0011-n §Implementation Phases Phase 6 closure card.

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G20 + RFC-0863 General-Purpose Network Integration + RFC-0011-n §Substrate-Additions Companion Missions row G20

## Summary

Adds the `NetworkSender` trait + `SendContext` struct per RFC-0863 General-Purpose Network Integration. Per-extension crate pattern: trait in Layer B, per-transport impl crates in Layer D, registry lookup at runtime per [[cipherocto-design-principles]] §User extensibility. Pre-requisite for `octo network status` (Phase 6 G20 substrate companion) so the CLI can query network-sender state via the per-extension registry.

### Substrate additions target

```rust
// crates/octo-network/src/sender/mod.rs (NEW directory, NEW module)
use std::collections::BTreeMap;

pub type TransportTag = &'static str;

/// `SendContext` carries the wire-format + metadata for a single
/// outbound payload (RFC-0863 §Network Sender). Substrate-faithful
/// to the RFC's `SendContext` struct anchor at line 116.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendContext {
    pub destination: [u8; 32],
    pub payload_kind: &'static str,
    pub wire_format: WireFormat,
    pub attempts: u32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum WireFormat {
    TcpJson,
    QuicBinary,
    BleCbor,
    UsbRaw,
}

/// `NetworkSender` trait (RFC-0863 §Network Sender trait anchor
/// at line 104). Per-extension crate pattern: each transport
/// (BLE, USB, TCP, QUIC, HID) ships in its own Layer D crate
/// implementing this trait. Core `octo-network` Layer B owns
/// only the trait + SendContext types.
pub trait NetworkSender: Send + Sync {
    /// Send a payload with the given `SendContext`. Returns
    /// the number of bytes actually transmitted on success.
    fn send(
        &self,
        ctx: &SendContext,
        payload: &[u8],
    ) -> Result<usize, NetworkSendError>;

    /// Stable per-extension transport tag (e.g. "tcp-json",
    /// "quic-binary", "ble-cbor"). Used by the registry
    /// to route payloads to the correct impl.
    fn transport_tag(&self) -> TransportTag;

    /// Substrate-faithful observability helper (operator-side
    /// `octo network status` surface).
    fn last_send_summary(&self) -> Option<SendSummary>;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendSummary {
    pub transport_tag: TransportTag,
    pub bytes_sent: u64,
    pub attempts: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NetworkSendError {
    Unreachable,
    Refused,
    PayloadTooLarge { size: usize, max: usize },
    WireFormatMismatch { expected: WireFormat, got: WireFormat },
    Internal(String),
}

/// In-process registry of `NetworkSender` impls indexed by
/// transport tag. Substrate-faithful to the per-extension
/// registry pattern per [[cipherocto-design-principles]] §User
/// extensibility.
#[derive(Default)]
pub struct NetworkSenderRegistry {
    senders: BTreeMap<TransportTag, Arc<dyn NetworkSender>>,
}

impl NetworkSenderRegistry {
    pub fn register(&mut self, sender: Arc<dyn NetworkSender>);
    pub fn get(&self, tag: TransportTag) -> Option<Arc<dyn NetworkSender>>;
    pub fn iter(&self) -> impl Iterator<Item = (TransportTag, &Arc<dyn NetworkSender>)>;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}
```

Layer B substrate additions land in a NEW directory `crates/octo-network/src/sender/` (NEW module). Trait + types in Layer B; per-transport impls in per-extension Layer D crates follow the per-extension crate pattern.

`BTreeMap` chosen over `HashMap` for deterministic iteration order per RFC-0011-h §Output Envelope order determinism — the registry iteration order must match across calls for stable CLI envelopes.

## Acceptance Criteria

- [x] Substrate additions land in `crates/octo-network/src/sender/` (NEW directory) per RFC-0011-h §Substrate-Additions row G20 (next a6627e53)
- [x] `NetworkSender` trait lands at `crates/octo-network/src/sender/mod.rs`
- [x] `SendContext` struct lands at same path
- [x] `WireFormat` enum lands at same path
- [x] `NetworkSendError` enum lands at same path
- [x] `SendSummary` struct lands at same path
- [x] `NetworkSenderRegistry` struct + register/get/iter/len/is_empty methods land at same path
- [x] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-network --lib` green (1472/1472, +11 above Phase 5 baseline of 1461)
- [x] Layer discipline preserved (Layer B only, zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [x] ≥3 unit tests + ≥1 integration test (11 substrate unit tests pin: trait object round-trip + registry register + registry get-hit + registry get-miss + registry iter-deterministic-order + registry idempotent-register-overwrite + SendContext Clone + WireFormat PartialEq + SendSummary Serialize/Deserialize round-trip + NetworkSendError Display)
- [x] Per-extension crate pattern preserved: trait in Layer B (`octo-network`); per-transport impls (BLE, USB, TCP, QUIC, HID) land in separate Layer D crates, NOT in `octo-network`

## Dependencies

- Hard sequencing: RFC-0011-h must be Accepted before this mission lands
- Hard sequencing: RFC-0011-n must be Accepted (Draft v0.2, DRY CLOSED)
- Soft sequencing: RFC-0863 General-Purpose Network Integration substrate anchors at L104 (trait) + L116 (SendContext)

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-status` covers that surface in Phase 6 CLI dispatch slice)
- Per-transport impls (BLE, USB, TCP, QUIC, HID) — each lands in its own Layer D per-extension crate per the per-extension crate pattern; OUT OF SCOPE for this trait-only mission
- Wire format versioning (deferred to substrate-additions companion)
- Send retry logic (substrate-faithful `attempts` field exists; retry policy lives in per-extension impls)

## Notes

Stub fill-in 2026-09-20 per RFC-0011-n closure card at `next` (DRY CLOSED v0.2). Substrate slice pending per directive sequencing (substrate coding is LAST). The `NetworkSender` trait deliberately mirrors the RFC-0863 trait anchor (line 104) + `SendContext` struct anchor (line 116) — the trait does NOT add fields beyond what RFC-0863 specifies per [[cipherocto-design-principles]] §Stable Abstractions Principle. The registry pattern preserves the per-extension crate pattern: each transport ships its own Layer D crate implementing `NetworkSender`; the registry is the only place that knows about every transport.
