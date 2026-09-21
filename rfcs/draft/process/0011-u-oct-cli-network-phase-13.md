# RFC-0011-u: `octo network` Phase 13 — Envelope Inspector + Forward Envelope (G16a + G16b)

## Status

Draft (2026-09-20) — RFC-0011-u lands RFC-0011-h §Implementation Phases Phase 13. Two CLI subcommands wire envelope inspection + envelope forwarding via TWO NEW substrate modules (`mon/envelope_inspector.rs` + `mon/forward_envelope.rs` per Phase 7 RFC-0011-o NEW module precedent for SlashBridge). Companion stub missions G16a + G16b + 0 NEW OctoCliError variants (REUSES slot 89 `NetworkSubstrateUnavailable` per RFC-0011-h §Error Handling row 89) + 2 output envelopes + 6 test vectors.

> **Amendment chain:** Thirteenth amendment in the `0011-h-multiphase-rollout-plan` (see `docs/plans/2026-09-20-0011-h-multiphase-rollout-plan.md`, gitignored scratchpad per `.gitignore` line 46). Phase 1 = RFC-0011-i (DRY CLOSED). Phase 2 = RFC-0011-j (DRY CLOSED). Phase 3 = RFC-0011-k (DRY CLOSED). Phase 4 = RFC-0011-l (DRY CLOSED). Phase 5 = RFC-0011-m (DRY CLOSED). Phase 6 = RFC-0011-n (DRY CLOSED). Phase 7 = RFC-0011-o (DRY CLOSED + Accepted). Phase 8 = RFC-0011-p (IMPLEMENTATION CLOSED). Phase 9 = RFC-0011-q (IMPLEMENTATION CLOSED). Phase 10 = RFC-0011-r (IMPLEMENTATION CLOSED). Phase 11 = RFC-0011-s (IMPLEMENTATION CLOSED). Phase 12 = RFC-0011-t (IMPLEMENTATION CLOSED). Phase 13 = RFC-0011-u (this RFC).

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu

## Summary

RFC-0011-u lands the **envelope inspector + forward envelope** slice of RFC-0011-h §Implementation Phases. Two CLI subcommands wire envelope inspection (read) and envelope forwarding (mutating) via TWO NEW substrate modules in `mon/`:

- `octo network envelope inspect <envelope_id_hex>` — read-only inspection of an envelope's metadata (returns None for unknown envelopes)
- `octo network envelope forward <envelope_id_hex> --destination <peer_id_hex> --ttl <N>` — mutating construction of a forward envelope (canonical wire-bytes per RFC-0855 §Wire Format)

Substrate per RFC-0855 §Wire Format. TWO NEW substrate modules (`mon/envelope_inspector.rs` + `mon/forward_envelope.rs`) per Phase 7 RFC-0011-o NEW module precedent for SlashBridge trait substrate.

## Dependencies

- RFC-0011-h Accepted
- RFC-0855 (Governing RFC; §Wire Format)
- RFC-0011-i Phase 1 IMPLEMENTATION CLOSED
- RFC-0011-j Phase 2 IMPLEMENTATION CLOSED
- RFC-0011-k Phase 3 IMPLEMENTATION CLOSED
- RFC-0011-l Phase 4 IMPLEMENTATION CLOSED
- RFC-0011-m Phase 5 IMPLEMENTATION CLOSED
- RFC-0011-n Phase 6 IMPLEMENTATION CLOSED
- RFC-0011-o Phase 7 IMPLEMENTATION CLOSED + Accepted
- RFC-0011-p Phase 8 IMPLEMENTATION CLOSED
- RFC-0011-q Phase 9 IMPLEMENTATION CLOSED
- RFC-0011-r Phase 10 IMPLEMENTATION CLOSED
- RFC-0011-s Phase 11 IMPLEMENTATION CLOSED
- RFC-0011-t Phase 12 IMPLEMENTATION CLOSED

## Design Goals

1. Wire `EnvelopeInspector::inspect()` to the CLI for operator envelope-metadata inspection (read-only)
2. Wire `ForwardEnvelope::build()` + `wire_bytes()` to the CLI for operator envelope forwarding (mutating)
3. Preserve per-extension crate pattern: traits in Layer B (`octo-network::mon::envelope_inspector` + `octo-network::mon::forward_envelope`); concrete per-envelope-source adapter in Layer D, OUT OF SCOPE
4. Preserve Layer A frozen contract (zero Layer A change per RFC-0011-h §Layer Discipline)
5. FIVE NEW types (`EnvelopeInspector` + `EnvelopeMeta` + `EnvelopeKind` enum + `ForwardEnvelope` + `ForwardEnvelopeError`); additive on NEW module, NOT existing sibling
6. Preserve BTreeMap determinism where substrate returns ordered data
7. Preserve slot 89 REUSE per Phase 6 precedent + user decision (0 NEW OctoCliError variants)
8. 6 test vectors — tv_net13_1 through tv_net13_6
9. `parse_32_byte_hex` pastejacking defense: every hex arg uses the shared helper per Phase 5 RFC-0011-m precedent

## Motivation

RFC-0011-h §Implementation Phases Phase 13 (G16a + G16b) calls for wiring envelope inspection + envelope forwarding to the CLI. Operators need to inspect envelope metadata (kind, creator DID, creation epoch, TTL) and construct forward envelopes for peer-to-peer propagation. The substrate is MISSING: no envelope_inspector or forward_envelope modules exist in `mon/`. This RFC's companion missions (G16a `0011-h-s-a-envelope-inspector` + G16b `0011-h-s-a-forward-envelope`) create TWO NEW modules in `mon/` with substrate types.

## Roles and Authorities

- **Operator**: invokes `octo network envelope inspect` for diagnostic metadata + `octo network envelope forward` for envelope propagation
- **Envelope metadata**: the in-memory projection of envelope kind + creator DID + creation epoch + TTL
- **Forward envelope**: the canonical wire-encoded envelope constructed for peer propagation
- **Per-extension concrete impl crates** (Layer D): OUT OF SCOPE; substrate traits in Layer B expose the inspector + forwarder surface for future follow-on Layer D adapter missions

## Detailed Design

### CLI surface

```
octo network envelope inspect <envelope_id_hex> [--json]
octo network envelope forward <envelope_id_hex> --destination <peer_id_hex> --ttl <N> [--dry-run] [--confirm-acknowledge] [--json]
```

- `envelope inspect` — read-only rendering of envelope metadata
- `envelope forward` — mutating construction of forward envelope (gated by `--confirm-acknowledge` per Phase 4 G21 mutating-subcommand pattern)
- `--dry-run` — preview wire bytes without broadcasting
- `--confirm-acknowledge` — required for mutating forwarding per Phase 4 precedent

### Substrate extensions (Layer B — TWO NEW MODULES)

NEW `crates/octo-network/src/mon/envelope_inspector.rs`:

```rust
/// Read-only inspector of envelope metadata (RFC-0855 §Wire Format).
///
/// Phase 13 G16a per RFC-0011-u §Substrate Mapping Table. Operates
/// on the in-memory snapshot; live envelope store OUT OF SCOPE.
/// Per-extension impl crates (Layer D) provide real envelope
/// stores in follow-on missions.
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
/// enum on the new module; no central edit.
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
    /// Inspect an envelope by 32-byte envelope_id; returns
    /// `None` for unknown envelopes (per RFC-0011-u §Envelope
    /// Inspect semantics).
    pub fn inspect(&self, envelope_id: [u8; 32]) -> Option<EnvelopeMeta>;
}
```

NEW `crates/octo-network/src/mon/forward_envelope.rs`:

```rust
/// Forward envelope builder for peer-to-peer propagation
/// (RFC-0855 §Wire Format).
///
/// Phase 13 G16b per RFC-0011-u §Substrate Mapping Table. Operates
/// on the in-memory snapshot; real network propagation OUT OF
/// SCOPE. Per-extension impl crates (Layer D) provide real network
/// adapters in follow-on missions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForwardEnvelope {
    source_envelope_id: [u8; 32],
    destination_peer_id: [u8; 32],
    ttl_epochs: u64,
    construction_epoch: u64,
}

/// Forward envelope construction error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForwardEnvelopeError {
    TtlOverflow,
    InvalidPeerId,
    Internal(String),
}

impl ForwardEnvelope {
    /// Build a new `ForwardEnvelope` for peer propagation.
    pub fn build(
        source_envelope_id: [u8; 32],
        destination_peer_id: [u8; 32],
        ttl_epochs: u64,
        construction_epoch: u64,
    ) -> Result<Self, ForwardEnvelopeError>;

    /// Serialize the envelope to canonical wire bytes per
    /// RFC-0855 §Wire Format canonical encoding.
    pub fn wire_bytes(&self) -> Vec<u8>;
}
```

The `wire_bytes()` impl returns the canonical wire encoding per RFC-0855 §Wire Format. The Phase 13 substrate uses `BTreeMap` (not `HashMap`) for deterministic ordering. The TTL field is validated against overflow in `build()`.

### Output envelopes

```rust
/// `octo network envelope inspect` output envelope.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkEnvelopeInspectOutput {
    /// Envelope metadata (None if envelope not found).
    pub envelope: Option<EnvelopeMeta>,
    /// 32-byte envelope_id hex (echo).
    pub envelope_id_hex: String,
}

/// `octo network envelope forward` output envelope.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkEnvelopeForwardOutput {
    /// 32-byte source envelope_id hex (echo).
    pub source_envelope_id_hex: String,
    /// 32-byte destination peer_id hex (echo).
    pub destination_peer_id_hex: String,
    /// TTL epochs (echo).
    pub ttl_epochs: u64,
    /// Canonical wire-bytes hex (RFC-0855 §Wire Format).
    pub wire_bytes_hex: String,
    /// Dry-run preview flag (true = no broadcast).
    pub dry_run: bool,
}
```

### Test vectors (6)

- `tv_net13_1`: envelope inspect default (no --json) parses cleanly
- `tv_net13_2`: envelope inspect --json parses cleanly
- `tv_net13_3`: envelope forward --destination <hex> --ttl 100 parses cleanly
- `tv_net13_4`: envelope forward --dry-run parses cleanly (no broadcast)
- `tv_net13_5`: envelope forward --confirm-acknowledge parses cleanly (mutating gate)
- `tv_net13_6`: envelope forward rejected without --confirm-acknowledge (mutating-gate clap arg requirement per Phase 4 RFC-0011-l precedent)

## Exit codes

Slot 89 `NetworkSubstrateUnavailable` REUSE per Phase 6 precedent + user decision. 0 NEW OctoCliError variants for Phase 13.

## Layer discipline

- **Layer A frozen preserved**: zero change to `octo-governance-core`, `octo-audit-core`, `octo-settlement-core`, `octo-vault-core`, `octo-wallet-core`.
- **Layer B substrate NEW MODULES**: `mon/envelope_inspector.rs` + `mon/forward_envelope.rs` (per Phase 7 RFC-0011-o NEW module precedent for SlashBridge).
- **Layer C CLI dispatch**: `commands/network.rs` extended with `NetworkAction::Envelope { action: NetworkEnvelopeAction }` clap variant; `NetworkEnvelopeAction::Inspect(EnvelopeInspectArgs)` + `NetworkEnvelopeAction::Forward(EnvelopeForwardArgs)`; 2 output envelopes; 2 handlers; 6 test vectors.

## Substrate-faithfulness

The `EnvelopeInspector::inspect()` + `ForwardEnvelope::build()` + `wire_bytes()` operate on the in-memory snapshot only. Per-extension Layer D adapter crates (live envelope store + network propagation) OUT OF SCOPE for Phase 13 per RFC-0011-h §Future Work items F8+F9. BTreeMap-based deterministic iteration ordering preserved per RFC-0011-h §Output Envelope determinism. `parse_32_byte_hex` pastejacking defense preserved per Phase 5 RFC-0011-m precedent.

## Companion stub missions

G16a `0011-h-s-a-envelope-inspector` + G16b `0011-h-s-a-forward-envelope` (both Open → Claimed → Completed paired with CLI dispatch slice).

## Out of Scope

- Live envelope store adapter (Layer D; follow-on per-extension crate missions)
- Per-extension impl crates (substrate-ext-envelope-store-* + substrate-ext-envelope-forwarder-*) OUT OF SCOPE for Phase 13
- Wire format versioning (RFC-0011-h §Future Work items F8+F9)
- Real network propagation (Phase 13 substrate operates on the in-memory snapshot only; real propagation in follow-on Layer D adapter mission)

## History

- 2026-09-20 — Draft (this version)
