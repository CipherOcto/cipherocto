# RFC-0011-u: `octo network` Phase 13 — Envelope Inspector + Forward Envelope (G16a + G16b)

## Status

Draft (2026-09-20) — RFC-0011-u lands RFC-0011-h §Implementation Phases Phase 13. Two CLI subcommands wire envelope inspection + envelope forwarding via TWO NEW substrate modules (`mon/envelope_inspector.rs` + `mon/forward_envelope.rs` per Phase 7 RFC-0011-o NEW module precedent for SlashBridge). Companion stub missions G16a + G16b + 0 NEW OctoCliError variants (REUSES slot 89 `NetworkSubstrateUnavailable` per RFC-0011-h §Error Handling row 89) + 2 output envelopes + 9 test vectors (6 dispatch + 3 dispatch body-contract expansion per R1.5 fix sweep) + 9 substrate test vectors (`tv_phase13_substrate_1` through `tv_phase13_substrate_9`, including R1.5 addition of `tv_phase13_substrate_9` for `TtlOverflow` variant) = 14 test vectors total per R1.5 fix sweep.

> **Amendment chain:** Thirteenth amendment in the `0011-h-multiphase-rollout-plan` (see `docs/plans/2026-09-20-0011-h-multiphase-rollout-plan.md`, gitignored scratchpad per [[docs-plans-scratchpad]]). Phase 1 = RFC-0011-i. Phase 2 = RFC-0011-j. Phase 3 = RFC-0011-k. Phase 4 = RFC-0011-l. Phase 5 = RFC-0011-m. Phase 6 = RFC-0011-n. Phase 7 = RFC-0011-o. Phase 8 = RFC-0011-p. Phase 9 = RFC-0011-q. Phase 10 = RFC-0011-r. Phase 11 = RFC-0011-s. Phase 12 = RFC-0011-t. Phase 13 = RFC-0011-u (this RFC).

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

- RFC-0011-h
- RFC-0855
- RFC-0011-i
- RFC-0011-j
- RFC-0011-k
- RFC-0011-l
- RFC-0011-m
- RFC-0011-n
- RFC-0011-o
- RFC-0011-p
- RFC-0011-q
- RFC-0011-r
- RFC-0011-s
- RFC-0011-t

## Design Goals

1. Wire `EnvelopeInspector::inspect()` to the CLI for operator envelope-metadata inspection (read-only)
2. Wire `ForwardEnvelope::build()` + `wire_bytes()` to the CLI for operator envelope forwarding (mutating)
3. Preserve per-extension crate pattern: substrate types in Layer B (`octo-network::mon::envelope_inspector` + `octo_network::mon::forward_envelope`); concrete per-envelope-source adapter in Layer D, OUT OF SCOPE
4. Preserve Layer A frozen contract (zero Layer A change per RFC-0011-h §Layer Discipline)
5. FIVE NEW types (`EnvelopeInspector` + `EnvelopeMeta` + `EnvelopeKind` enum + `ForwardEnvelope` + `ForwardEnvelopeError`); additive on NEW module, NOT existing sibling
6. Preserve scalar field-order determinism where substrate returns ordered data (counters and byte slices are plain scalars; zero HashMap/BTreeMap collection dependencies)
7. Preserve slot 89 REUSE per Phase 6 precedent + user decision (0 NEW OctoCliError variants)
8. 14 test vectors — 9 dispatch (`tv_net13_1` through `tv_net13_9`) + 9 substrate (`tv_phase13_substrate_1` through `tv_phase13_substrate_9`, including R1.5 expansion)
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
/// stores in follow-on missions. Substrate-faithfulness contract:
/// `inspect()` returns `None` for unknown envelopes (live store
/// unwired in this additive-type-only phase).
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
/// adapters in follow-on missions. Scalar field-order
/// determinism preserved per RFC-0011-h §Output Envelope
/// determinism (zero collection dependencies; `wire_bytes()`
/// returns a flat 80-byte scalar layout).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForwardEnvelope {
    source_envelope_id: [u8; 32],
    destination_peer_id: [u8; 32],
    ttl_epochs: u64,
    construction_epoch: u64,
}

/// Maximum TTL epochs accepted by `ForwardEnvelope::build`.
///
/// Phase 13 G16b substrate contract per RFC-0011-u §Substrate
/// extensions. TTL values exceeding this limit return
/// `ForwardEnvelopeError::TtlOverflow`.
pub const FORWARD_ENVELOPE_MAX_TTL_EPOCHS: u64 = u64::MAX / 2;

/// Forward envelope construction error.
///
/// Phase 13 G16b per RFC-0011-u §Substrate Mapping Table.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ForwardEnvelopeError {
    /// TTL epochs exceeds `FORWARD_ENVELOPE_MAX_TTL_EPOCHS`.
    TtlOverflow,
    /// Destination peer_id is all-zero (invalid).
    InvalidPeerId,
    /// Internal error (reserved for follow-on Layer D adapter
    /// missions).
    Internal(String),
}

impl ForwardEnvelope {
    /// Build a new `ForwardEnvelope` from explicit fields.
    /// Returns `Err(InvalidPeerId)` if `destination_peer_id` is
    /// all-zero. Returns `Err(TtlOverflow)` if `ttl_epochs`
    /// exceeds `FORWARD_ENVELOPE_MAX_TTL_EPOCHS`.
    ///
    /// `construction_epoch` is plumbed through the substrate API
    /// but caller-stubbed at 0 in this additive-type-only phase per
    /// RFC-0011-u §Substrate-faithfulness.
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

The `wire_bytes()` impl returns the canonical wire encoding per RFC-0855 §Wire Format. The Phase 13 substrate uses scalar field-order concatenation (no HashMap/BTreeMap; zero collection dependencies per RFC-0011-h §Output Envelope determinism). The TTL field is validated against overflow in `build()` (R1.5 fix: added `TtlOverflow` variant + `FORWARD_ENVELOPE_MAX_TTL_EPOCHS` constant for unit-testable boundary).

### Output envelopes

```rust
/// `octo network envelope inspect` output envelope.
#[derive(Serialize, Deserialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkEnvelopeInspectOutput {
    /// 32-byte envelope_id hex (echo).
    pub envelope_id_hex: String,
    /// Envelope kind label (mission / governance / reputation /
    /// slash / forward / bootstrap / discovery / coordinator).
    pub envelope_kind: String,
    /// Creator DID hex (canonicalized).
    pub creator_did_hex: String,
    /// Creation epoch.
    pub creation_epoch: u64,
    /// TTL epochs.
    pub ttl_epochs: u64,
    /// Whether envelope was found (false = unknown envelope).
    pub found: bool,
}

/// `octo network envelope forward` output envelope.
#[derive(Serialize, Deserialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkEnvelopeForwardOutput {
    /// 32-byte source envelope_id hex (echo).
    pub source_envelope_id_hex: String,
    /// 32-byte destination peer_id hex (echo).
    pub destination_peer_id_hex: String,
    /// TTL epochs (echo).
    pub ttl_epochs: u64,
    /// Canonical wire-bytes hex (RFC-0855 §Wire Format).
    pub wire_bytes_hex: String,
    /// Wire-bytes length in bytes (80 per canonical layout).
    pub wire_bytes_len: usize,
    /// Dry-run preview flag (true = no broadcast).
    pub dry_run: bool,
}
```

### Test vectors (14: 9 dispatch + 9 substrate)

#### Dispatch (RFC-0011-u §Test Vectors — `tv_net13_*`)

- `tv_net13_1`: envelope inspect default (no --json) parses cleanly
- `tv_net13_2`: envelope inspect --json parses cleanly
- `tv_net13_3`: envelope forward --destination <hex> --ttl 100 parses cleanly
- `tv_net13_4`: envelope forward --dry-run parses cleanly (no broadcast)
- `tv_net13_5`: envelope forward --confirm-acknowledge parses cleanly (mutating gate)
- `tv_net13_6`: envelope forward without --confirm-acknowledge parses but handler gates (mutating-gate enforced in handler, NOT clap arg per Phase 4 RFC-0011-l precedent; R1.5 fix: corrected RFC description to match actual test)
- `tv_net13_7`: envelope inspect dispatch body-contract inspection (R1.5 fix: dispatch test invokes `network_envelope_inspect` handler + directly invokes substrate `EnvelopeInspector::inspect()` to verify body contract per Phase 10 + Phase 11 R3 MAJOR-1 lesson)
- `tv_net13_8`: envelope forward --dry-run dispatch body-contract inspection (R1.5 fix: dispatch test invokes `network_envelope_forward` handler + directly invokes substrate `ForwardEnvelope::build` + `wire_bytes()` to verify body contract per Phase 10 + Phase 11 R3 MAJOR-1 lesson)
- `tv_net13_9`: envelope output JSON serde round-trip (R1.5 fix: `Serialize + Deserialize` derives on `NetworkEnvelopeInspectOutput` + `NetworkEnvelopeForwardOutput` enable round-trip assertion)

#### Substrate (RFC-0011-u §Test Vectors — `tv_phase13_substrate_*`)

- `tv_phase13_substrate_1`: `EnvelopeInspector::inspect([0u8;32])` returns `None` (default-constructed inspector returns `None` for any envelope_id)
- `tv_phase13_substrate_2`: `EnvelopeInspector::from_fields` constructs `EnvelopeMeta` with explicit fields
- `tv_phase13_substrate_3`: `EnvelopeKind::as_str()` returns 8 distinct string labels (mission / governance / reputation / slash / forward / bootstrap / discovery / coordinator)
- `tv_phase13_substrate_4`: `EnvelopeMeta` PartialEq — equal for same fields, not equal across different envelope_kind
- `tv_phase13_substrate_5`: `ForwardEnvelope::build` succeeds for valid source + destination + ttl
- `tv_phase13_substrate_6`: `ForwardEnvelope::build` rejects all-zero destination_peer_id with `InvalidPeerId`
- `tv_phase13_substrate_7`: `ForwardEnvelope::wire_bytes()` returns canonical 80-byte layout (source[32] || dest[32] || ttl_be[8] || construction_epoch_be[8])
- `tv_phase13_substrate_8`: `ForwardEnvelope::wire_bytes()` deterministic across calls (same input → same output)
- `tv_phase13_substrate_9`: `ForwardEnvelope::build` rejects `ttl_epochs > FORWARD_ENVELOPE_MAX_TTL_EPOCHS` with `TtlOverflow` (R1.5 fix: added `TtlOverflow` variant + `FORWARD_ENVELOPE_MAX_TTL_EPOCHS` constant)

## Exit codes

Slot 89 `NetworkSubstrateUnavailable` REUSE per Phase 6 precedent + user decision. 0 NEW OctoCliError variants for Phase 13.

## Layer discipline

- **Layer A frozen preserved**: zero change to `octo-governance-core`, `octo-audit-core`, `octo-settlement-core`, `octo-vault-core`, `octo-wallet-core`.
- **Layer B substrate NEW MODULES**: `mon/envelope_inspector.rs` + `mon/forward_envelope.rs` (per Phase 7 RFC-0011-o NEW module precedent for SlashBridge).
- **Layer C CLI dispatch**: `commands/network.rs` extended with `NetworkAction::Envelope { action: NetworkEnvelopeAction }` clap variant; `NetworkEnvelopeAction::Inspect(EnvelopeInspectArgs)` + `NetworkEnvelopeAction::Forward(EnvelopeForwardArgs)`; 2 output envelopes; 2 handlers; 14 test vectors (9 dispatch + 9 substrate layer).

## Substrate-faithfulness

The `EnvelopeInspector::inspect()` + `ForwardEnvelope::build()` + `wire_bytes()` operate on the in-memory snapshot only. Per-extension Layer D adapter crates (live envelope store + network propagation) OUT OF SCOPE for Phase 13 per RFC-0011-h §Future Work items F8+F9. Scalar field-order determinism preserved per RFC-0011-h §Output Envelope determinism (counters and byte slices are plain scalars; zero HashMap/BTreeMap collection dependencies). `construction_epoch` is plumbed through `ForwardEnvelope::build` but stubbed at 0 by callers (the real RFC-0855 §Wire Format construction-epoch source is OUT OF SCOPE for Phase 13 and lands in a follow-on Layer D adapter mission).

The CLI handlers invoke the substrate methods unconditionally — no registry gate (trait dispatch is the universal code path per Phase 12 RFC-0011-t R2.5 substrate-faithfulness precedent). The R1.5 fix removed the `envelope_inspect_registry` and `envelope_forward_registry` always-false early-returns (they were residual additive-type-only phase artifacts that violated the Phase 10/11/12 trait-dispatch precedent); the handlers now exercise the substrate `EnvelopeInspector::inspect()` + `ForwardEnvelope::build()` + `wire_bytes()` paths end-to-end. `parse_32_byte_hex` pastejacking defense preserved per Phase 5 RFC-0011-m precedent.

## Companion stub missions

G16a `0011-h-s-a-envelope-inspector` + G16b `0011-h-s-a-forward-envelope` (both Open → Claimed → Completed paired with CLI dispatch slice).

## Out of Scope

- Live envelope store adapter (Layer D; follow-on per-extension crate missions)
- Per-extension impl crates (substrate-ext-envelope-store-* + substrate-ext-envelope-forwarder-*) OUT OF SCOPE for Phase 13
- Wire format versioning (RFC-0011-h §Future Work items F8+F9)
- Real network propagation (Phase 13 substrate operates on the in-memory snapshot only; real propagation in follow-on Layer D adapter mission)
- Real construction-epoch source (Phase 13 substrate plumbs `construction_epoch` slot; caller stubs at 0; Layer D adapter missions in future)

## History

- 2026-09-20 — Draft (this RFC)
- 2026-09-21 — R1.5 fix sweep: removed `envelope_inspect_registry` + `envelope_forward_registry` always-false early-return gates + helper fns (handlers now invoke substrate unconditionally per Phase 12 RFC-0011-t R2.5 substrate-faithfulness precedent); added `TtlOverflow` variant to `ForwardEnvelopeError` + `FORWARD_ENVELOPE_MAX_TTL_EPOCHS` constant + TTL overflow validation in `build()`; dropped misleading "BTreeMap-based deterministic iteration" comments from `EnvelopeInspector` + `ForwardEnvelope` struct docs + `wire_bytes()` doc (no collection dependencies; replaced with accurate "scalar field-order determinism" + "flat 80-byte scalar layout" phrasing per Phase 12 R1.5 lesson); added doc comment to `ForwardEnvelope::build` documenting caller-stubbed-zero contract for `construction_epoch` (mirrors Phase 12 R1.5 `anti_entropy_rounds` precedent); added `Deserialize` derive to `NetworkEnvelopeInspectOutput` + `NetworkEnvelopeForwardOutput` for JSON envelope round-trip test; branched `network_envelope_inspect` handler on `Some(meta)` / `None` per substrate-faithfulness contract (no synthetic projection; `found: false` echoed when inspector returns `None`); stripped status parentheticals from §Dependencies + amendment chain per CLAUDE.md §RFC Reference Conventions Reaffirmed; removed "(Governing RFC; §Wire Format)" label from §Dependencies; replaced `.gitignore` line 46 file:line ref with `[[docs-plans-scratchpad]]` memory cross-ref per `[[no-line-refs-anywhere]]` feedback; expanded test vector count from 6 to 14 (9 dispatch + 9 substrate); added `tv_net13_7` + `tv_net13_8` dispatch body-contract inspection tests + `tv_net13_9` envelope serde round-trip test + `tv_phase13_substrate_9` `TtlOverflow` variant test; replaced "this version" with "this RFC" in History line per Phase 12 R1 NIT-6 lesson; replaced "traits" with "substrate types" in §Design Goal 3 to match actual code (structs, not traits).
