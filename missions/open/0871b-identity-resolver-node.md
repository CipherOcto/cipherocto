# Mission: 0871b — Identity Resolver Node (RFC-0871 Phase 3)

## Status

Open (2026-08-09). RFC-0871 Accepted 2026-08-09 after R1–R7 adversarial review (R7 DRY). Phase 3 identity resolver node mission.

## RFC

RFC-0871 (Networking): Specialized Node Protocol Envelope
RFC-0010 (Process): Canonical DID Codec
RFC-0009 (Process): Identity Management

**BLUEPRINT gate note:** All substrate RFCs Accepted. Mission 0871b implements a specialized node of type `identity-resolver` per RFC-0871 §Roles and Authorities. No new RFC required — node shape is fully defined by RFC-0871 + substrate RFCs.

This mission creates `crates/octo-identity-resolver-node/` — a specialized node that resolves `did:octo:z<base58btc>` to canonical DID form + storage-pubkey form (per RFC-0010 dual storage/wire split). Advertises `DID_RESOLVE` + `DID_LOOKUP` payload kinds. Cross-domain DID resolution (resolver chains across specialized nodes) deferred to RFC-0871 §Future Work.

## Summary

Build `crates/octo-identity-resolver-node/` — the identity resolver specialized node. `IdentityResolverNode` wraps a DID storage backend (current: in-process `DidRegistry` or stoolap-backed per RFC-0010) + `Arc<NodeTransport>`. It registers as `NetworkReceiver`, advertises payload kinds `DID_RESOLVE` (wire-form → canonical-form lookup) + `DID_LOOKUP` (canonical-form → storage-form lookup). Reuses `octo-protocol::NodeEnvelope` from mission `0871-protocol-core-envelope.md`. All DID validation routes through `octo_ident::CanonicalCodec::parse` (no shortcut). Replay + authorization verification reuses `octo-protocol::EnvelopeDispatcher`.

## Acceptance Criteria

### Top-level: Crate + node

- [ ] NEW: `crates/octo-identity-resolver-node/` crate with `Cargo.toml` + `src/lib.rs`
- [ ] `crates/octo-identity-resolver-node/src/node.rs` — `IdentityResolverNode { registry: Arc<dyn DidRegistry>, transport: Arc<NodeTransport>, handlers: HashMap<PayloadKindId, Arc<dyn EnvelopeHandler>> }`
- [ ] `IdentityResolverNode::new(registry, transport) -> Self`
- [ ] `IdentityResolverNode::start() -> Result<ReceiverId, ResolverNodeError>` registers `NetworkReceiver`
- [ ] `IdentityResolverNode::broadcast_announce() -> Result<usize, TransportError>` announces `DID_RESOLVE` + `DID_LOOKUP` payload kinds
- [ ] `IdentityResolverNode::handle_envelope(envelope) -> Result<HandlerOutput, ProtocolError>` dispatch entry point
- [ ] NetworkReceiver trait impl (RFC-0863)

### Payload kinds

- [ ] `DID_RESOLVE` handler: input = `WireDid` (the `did:octo:z<base58btc>` form); validates via `octo_ident::CanonicalCodec::parse(wire, false)`; on success, looks up in `DidRegistry`; returns canonical DID + storage-pubkey form (per RFC-0010 dual storage/wire split)
- [ ] `DID_LOOKUP` handler: input = canonical DID form (52-byte `RawDid`); looks up in `DidRegistry`; returns storage-pubkey form
- [ ] Both handlers: rejects malformed DIDs (non-canonical, wrong prefix, bad base58btc checksum) with `ProtocolError::InvalidDid`
- [ ] Both handlers: rate-limited per RFC-0871 §Replay Protection (envelope_id dedup)

### Registry backend

- [ ] `DidRegistry` trait (RFC-0010 codec crate substrate) defined in `crates/octo-identity-resolver-node/src/registry.rs`
- [ ] `InMemoryDidRegistry` impl (test fixture, populates from `crates/octo-ident` existing storage)
- [ ] `StoolapDidRegistry` impl (production, backed by RFC-0010 stoolap schema)
- [ ] Migration: existing `DidRegistry` in `crates/octo-ident` is the source of truth; new crate wraps it (no duplicate state)

### Replay + authorization

- [ ] All handlers route through `octo_protocol::EnvelopeDispatcher` for envelope_id dedup + expiry check
- [ ] All handlers verify `Vec<Authorization>` per `octo_protocol::Authorization::verify`
- [ ] Resolution requires caller DID authenticated (no anonymous resolution per RFC-0009)

### Adversary coverage

- [ ] DID spoofing rejected: canonical validation rejects arbitrary string substitution (RFC-0010 codec crate)
- [ ] Replay rejected: `DID_RESOLVE` envelope_id dedup enforced
- [ ] Unauthorized lookup rejected: `Authorization::verify` requires valid signature from caller
- [ ] Storage-backend DoS: rate-limited per RFC-0871 §Replay Protection
- [ ] Wire-form confusion: `DID_RESOLVE` accepts only wire-form; `DID_LOOKUP` accepts only canonical form; cross-form requests rejected

### Backward compat

- [ ] `cargo test -p octo-ident --lib` continues green (no regression in existing DID codec tests)
- [ ] `cargo test -p octo-identity-resolver-node --lib` green (new crate)
- [ ] `cargo clippy --workspace --all-targets --features full -- -D warnings` clean
- [ ] `cargo fmt --check` clean

## Type Coverage

Per BLUEPRINT §Mission template. RFC-0871 §Roles and Authorities + RFC-0010 + RFC-0009 types mapped to this mission (Phase 3 identity resolver):

| RFC Type / Section | Implemented By |
|---|---|
| `IdentityResolverNode` struct (RFC-0871 §Roles and Authorities) | This mission — `crates/octo-identity-resolver-node/src/node.rs` |
| `NetworkReceiver` impl (RFC-0863 substrate) | This mission — `crates/octo-identity-resolver-node/src/node.rs` |
| `DID_RESOLVE` payload kind | This mission — `crates/octo-identity-resolver-node/src/handlers/resolve.rs` |
| `DID_LOOKUP` payload kind | This mission — `crates/octo-identity-resolver-node/src/handlers/lookup.rs` |
| `IdentityResolverNode::broadcast_announce` | This mission — uses `RouterAnnouncePayload` extension |
| `DidRegistry` trait + impls | This mission — wraps `crates/octo-ident` existing registry; new `InMemoryDidRegistry` + `StoolapDidRegistry` in `crates/octo-identity-resolver-node/src/registry.rs` |
| Canonical DID validation | Mission `0010-d-wallet-audience-validation.md` — `octo_ident::CanonicalCodec::parse` enforcement is prerequisite |
| Canonical wire-form + storage-pubkey dual-form split | RFC-0010 substrate (codec crate) |
| `NodeEnvelope` envelope shape (consumed) | Mission `0871-protocol-core-envelope.md` — Phase 1 prerequisite |
| Cross-domain DID resolution (resolver chains) | Deferred to RFC-0871 §Future Work — separate future mission |

## Dependencies

**Requires:**

- RFC-0871 — accepted substrate (envelope shape)
- RFC-0010 — canonical DID codec + dual storage/wire split
- RFC-0009 — identity substrate (DID format spec)
- RFC-0863 — `NodeTransport` + `NetworkReceiver` trait
- RFC-0862 — gossip substrate for `broadcast_announce`
- `crates/octo-protocol` — Phase 1 envelope types (mission `0871-protocol-core-envelope.md`)
- `crates/octo-ident` — existing DID codec + registry
- `octo-transport` — NodeTransport (RFC-0863); crate lives at workspace root

**Mission gates (sequential):**

- Mission `0871-protocol-core-envelope.md` MUST complete first (Phase 1 dependency)
- Mission `0010-d-wallet-audience-validation.md` MUST complete first (canonical DID validation must be enforced before this node exists to consume it)

**Parallel with (no dependency):**

- Mission `0870-b-envelope-adoption.md` (Phase 3 quota router)
- Mission `0871a-wallet-node.md` (Phase 2 wallet node)
- Mission `0871c-reputation-anchor-node.md` (Phase 3 reputation anchor)
- Mission `0871d-capability-issuer-node.md` (Phase 3 capability issuer)

**Not Requires:**

- Mission `0871e-paid-query-caveat.md` (Phase 5 — separate)
- Cross-domain DID resolution (RFC-0871 §Future Work — separate future mission)

## Implementation Guide

- NEW crate: `crates/octo-identity-resolver-node/` with `src/lib.rs`, `src/node.rs`, `src/handlers/{resolve,lookup}.rs`, `src/registry.rs`, `tests/`
- `IdentityResolverNode::start()` registers `NetworkReceiver` via `transport.register_receiver(self.clone())` per RFC-0863 wiring
- `IdentityResolverNode::handle_envelope`:
  1. `EnvelopeDispatcher::dispatch` (from `octo-protocol`) — envelope_id dedup + expiry + signature verification
  2. Route by `envelope.payload_kind` to handler map
  3. Handler queries `DidRegistry` and returns response envelope
- `Cargo.toml` deps per CLAUDE.md crate stability: `octo-protocol` (Layer A), `octo-ident` (Layer B), `octo-transport` (Layer D)
- Cross-domain resolution (resolver chains across specialized nodes) deferred — single-node resolution only in this mission. Future RFC per RFC-0871 §Future Work.

## Acceptance Cross-Ref

Per RFC-0871 §Implementation Phases Phase 3 + RFC-0010 (codec crate):

- [x] RFCs Accepted (RFC-0871, RFC-0010, RFC-0009)
- [ ] Mission filed (this file)
- [ ] Phase 1 foundation complete: `0871-protocol-core-envelope.md`
- [ ] `IdentityResolverNode` struct + handlers implemented
- [ ] `DID_RESOLVE` + `DID_LOOKUP` payload kinds registered
- [ ] Stoolap-backed `StoolapDidRegistry` production-ready
- [ ] Cross-domain resolution deferred (RFC-0871 §Future Work)

## Claimant

@unassigned

## Pull Request

#

## Notes

- Layer C crate (specialized node). Stability: per-RFC.
- This node is a CONSUMER of `octo-ident`, not a duplicate state owner. The dual storage/wire split (RFC-0010) is the source of truth; this node wraps lookup behind an envelope boundary.
- Cross-domain DID resolution (resolver chains across specialized nodes) is RFC-0871 §Future Work — not in this mission scope. Filed separately when needed.
- Rate limiting per RFC-0871 §Replay Protection. Resolution is an attack surface (DoS via lookup floods); per-caller rate limit enforced at `EnvelopeDispatcher` level.
- Production deployment: identity resolver is a stateful actor (RFC-0871 §Roles and Authorities). Coordinator role + election required for multi-node deployment. Single-node mission for now; multi-node federation deferred.
