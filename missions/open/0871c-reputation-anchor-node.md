# Mission: 0871c — Reputation Anchor Node (RFC-0871 Phase 3)

## Status

Open (2026-08-09). RFC-0871 Accepted 2026-08-09 after R1–R7 adversarial review (R7 DRY). RFC-0968 Accepted 2026-07-26. Phase 3 reputation anchor node mission.

## RFC

RFC-0871 (Networking): Specialized Node Protocol Envelope
RFC-0968 (Economics): Reputation Registry
RFC-0955-R1 (Economics): Reputation Anchoring (Amendment)

**BLUEPRINT gate note:** All substrate RFCs Accepted. Mission 0871c implements a specialized node of type `reputation-anchor` per RFC-0871 §Roles and Authorities. No new RFC required — node shape is fully defined by RFC-0871 + RFC-0968.

This mission creates `crates/octo-reputation-anchor-node/` — a specialized node that wraps RFC-0968 reputation registry operations behind a `NodeEnvelope` interface. Advertises `REPUTATION_QUERY` + `REPUTATION_UPDATE` + `REPUTATION_ANCHOR` payload kinds (the third for on-chain anchoring per RFC-0955-R1). Reuses `octo-protocol::NodeEnvelope` from mission `0871-protocol-core-envelope.md`. Cross-chain anchoring (specialized node ↔ external chain) deferred to RFC-0871 §Future Work.

## Summary

Build `crates/octo-reputation-anchor-node/` — the reputation anchor specialized node. `ReputationAnchorNode` wraps RFC-0968 `ReputationRegistry` + RFC-0955-R1 anchoring substrate + `Arc<NodeTransport>`. It registers as `NetworkReceiver`, advertises payload kinds `REPUTATION_QUERY` (read), `REPUTATION_UPDATE` (write, requires authorization), `REPUTATION_ANCHOR` (on-chain binding per RFC-0955-R1). Reuses `octo-protocol::NodeEnvelope`. Authorization required for `UPDATE` + `ANCHOR` (reputation has economic implications per the dual-stake model); `QUERY` requires authenticated caller (no anonymous reads).

## Acceptance Criteria

### Top-level: Crate + node

- [ ] NEW: `crates/octo-reputation-anchor-node/` crate with `Cargo.toml` + `src/lib.rs`
- [ ] `crates/octo-reputation-anchor-node/src/node.rs` — `ReputationAnchorNode { registry: Arc<ReputationRegistry>, anchor: Arc<ReputationAnchor>, transport: Arc<NodeTransport>, handlers: HashMap<PayloadKindId, Arc<dyn EnvelopeHandler>> }`
- [ ] `ReputationAnchorNode::new(registry, anchor, transport) -> Self`
- [ ] `ReputationAnchorNode::start() -> Result<ReceiverId, AnchorNodeError>` registers `NetworkReceiver`
- [ ] `ReputationAnchorNode::broadcast_announce() -> Result<usize, TransportError>` announces `REPUTATION_QUERY` + `REPUTATION_UPDATE` + `REPUTATION_ANCHOR`
- [ ] `ReputationAnchorNode::handle_envelope(envelope) -> Result<HandlerOutput, ProtocolError>`
- [ ] NetworkReceiver trait impl (RFC-0863)

### Payload kinds

- [ ] `REPUTATION_QUERY` handler: input = `did:octo:z<base58btc>`; validates via `octo_ident::CanonicalCodec::parse`; on success, queries `ReputationRegistry`; returns reputation record (score + history + reputation roots per RFC-0968 §5. Storage Schema)
- [ ] `REPUTATION_UPDATE` handler: input = `(did, reputation_delta, evidence_envelope_id)`; requires `Authorization::Capability(token)` with `ReputationUpdateCaveat` (RFC-0965 caveat type); applies delta to `ReputationRegistry`; emits reputation event (RFC-0968 §11. Audit Trail records the event); returns updated reputation record
- [ ] `REPUTATION_ANCHOR` handler: input = `(reputation_root, period)`; requires `Authorization::Capability(token)` with `AnchorCapabilityCaveat` (RFC-0955-R1 caveat); submits anchoring proof via `ReputationAnchor`; returns anchoring receipt
- [ ] All handlers: DID validation via `octo_ident::CanonicalCodec::parse(s, false)`
- [ ] All handlers: rate-limited per RFC-0871 §Replay Protection

### Authorization model

- [ ] `REPUTATION_QUERY`: requires authenticated caller (any valid `Authorization::Signature`); no anonymous reads
- [ ] `REPUTATION_UPDATE`: requires `Authorization::Capability(token)` carrying `ReputationUpdateCaveat` with bounded `max_delta_per_epoch`
- [ ] `REPUTATION_ANCHOR`: requires `Authorization::Capability(token)` carrying `AnchorCapabilityCaveat` (RFC-0955-R1) with `anchored_period` matching period request
- [ ] No anonymous writes — reputation has economic implications per the dual-stake model (whitepaper §Token role table)

### Replay + integrity

- [ ] All handlers route through `octo_protocol::EnvelopeDispatcher` for envelope_id dedup + expiry check
- [ ] All handlers verify `Vec<Authorization>` per `octo_protocol::Authorization::verify`
- [ ] `REPUTATION_UPDATE` events are append-only per RFC-0968 §11. Audit Trail; no in-place mutation
- [ ] `REPUTATION_ANCHOR` produces RFC-0955-R1 anchoring receipt (reputation_root + on-chain tx hash)

### Adversary coverage

- [ ] Reputation inflation: `ReputationUpdateCaveat.max_delta_per_epoch` enforced at handler
- [ ] Replay: envelope_id dedup + reputation event seq deduplication
- [ ] Unauthorized anchor: `AnchorCapabilityCaveat` required; cross-period anchoring rejected
- [ ] Anonymous read DoS: rate-limited per caller DID per RFC-0871 §Replay Protection
- [ ] DID spoofing: canonical validation rejects non-canonical wire form per RFC-0010

### Backward compat

- [ ] `cargo test -p quota-router-core --lib reputation` continues green (no regression in existing RFC-0968 tests)
- [ ] `cargo test -p octo-reputation-anchor-node --lib` green (new crate)
- [ ] `cargo clippy --workspace --all-targets --features full -- -D warnings` clean
- [ ] `cargo fmt --check` clean

## Type Coverage

Per BLUEPRINT §Mission template. RFC-0871 §Roles and Authorities + RFC-0968 + RFC-0955-R1 types mapped to this mission (Phase 3 reputation anchor):

| RFC Type / Section | Implemented By |
|---|---|
| `ReputationAnchorNode` struct (RFC-0871 §Roles and Authorities) | This mission — `crates/octo-reputation-anchor-node/src/node.rs` |
| `NetworkReceiver` impl (RFC-0863 substrate) | This mission — `crates/octo-reputation-anchor-node/src/node.rs` |
| `REPUTATION_QUERY` payload kind | This mission — `crates/octo-reputation-anchor-node/src/handlers/query.rs` |
| `REPUTATION_UPDATE` payload kind | This mission — `crates/octo-reputation-anchor-node/src/handlers/update.rs` |
| `REPUTATION_ANCHOR` payload kind | This mission — `crates/octo-reputation-anchor-node/src/handlers/anchor.rs` |
| `ReputationAnchorNode::broadcast_announce` | This mission — uses `RouterAnnouncePayload` extension |
| `ReputationRegistry` substrate | RFC-0968 existing — `crates/quota-router-core/src/reputation/` |
| `ReputationAnchor` substrate (on-chain anchoring) | RFC-0955-R1 — `crates/octo-reputation-anchor-node/src/anchor.rs` |
| `ReputationUpdateCaveat` (RFC-0965 reserved) | This mission — registers caveat in `crates/octo-cap-macaroon/src/caveat/reputation.rs` |
| `AnchorCapabilityCaveat` (RFC-0955-R1 caveat) | Mission `missions/claimed/0968a-reputation-anchoring.md` — defines the caveat |
| `NodeEnvelope` envelope shape (consumed) | Mission `0871-protocol-core-envelope.md` — Phase 1 prerequisite |
| Cross-chain anchoring (specialized node ↔ external chain) | Deferred to RFC-0871 §Future Work — separate future mission |

## Dependencies

**Requires:**

- RFC-0871 — accepted substrate (envelope shape)
- RFC-0968 — reputation registry substrate
- RFC-0955-R1 — on-chain anchoring substrate
- RFC-0965 — caveat discriminator (for `ReputationUpdateCaveat` + `AnchorCapabilityCaveat`)
- RFC-0863 — `NodeTransport` + `NetworkReceiver` trait
- `crates/octo-protocol` — Phase 1 envelope types (mission `0871-protocol-core-envelope.md`)
- `crates/octo-ident` — DID parsing
- `octo-transport` — NodeTransport (RFC-0863); crate lives at workspace root

**Mission gates (sequential):**

- Mission `0871-protocol-core-envelope.md` MUST complete first (Phase 1 dependency)
- RFC-0968 implementation (existing reputation registry in `quota-router-core`) MUST be production-ready before this node exists to wrap it
- RFC-0955-R1 anchoring substrate MUST be production-ready (check `missions/claimed/0968a-reputation-anchoring.md` per BLUEPRINT §Mission Lifecycle)

**Parallel with (no dependency):**

- Mission `0870-b-envelope-adoption.md` (Phase 3 quota router)
- Mission `0871a-wallet-node.md` (Phase 2 wallet node)
- Mission `0871b-identity-resolver-node.md` (Phase 3 identity resolver)
- Mission `0871d-capability-issuer-node.md` (Phase 3 capability issuer)

**Not Requires:**

- Mission `0871e-paid-query-caveat.md` (Phase 5 — separate)
- Cross-chain anchoring (RFC-0871 §Future Work — separate future mission)

## Implementation Guide

- NEW crate: `crates/octo-reputation-anchor-node/` with `src/lib.rs`, `src/node.rs`, `src/handlers/{query,update,anchor}.rs`, `tests/`
- `ReputationAnchorNode::start()` registers `NetworkReceiver` via `transport.register_receiver(self.clone())` per RFC-0863 wiring
- `ReputationAnchorNode::handle_envelope`:
  1. `EnvelopeDispatcher::dispatch` (from `octo-protocol`) — envelope_id dedup + expiry + signature verification
  2. Route by `envelope.payload_kind` to handler map
  3. Handler queries `ReputationRegistry` (RFC-0968) and/or `ReputationAnchor` (RFC-0955-R1); returns response envelope
- `Cargo.toml` deps per CLAUDE.md crate stability: `octo-protocol` (Layer A), `quota-router-core` (reputation registry, Layer B), `octo-transport` (Layer D)
- Cross-chain anchoring (specialized node ↔ external chain) deferred — single-chain anchoring via RFC-0955-R1 substrate only. Future RFC per RFC-0871 §Future Work.

## Acceptance Cross-Ref

Per RFC-0871 §Implementation Phases Phase 3 + RFC-0968 §5. Storage Schema + RFC-0955-R1 anchoring substrate:

- [x] RFCs Accepted (RFC-0871, RFC-0968, RFC-0955-R1)
- [ ] Mission filed (this file)
- [ ] Phase 1 foundation complete: `0871-protocol-core-envelope.md`
- [ ] RFC-0968 reputation registry production-ready
- [ ] RFC-0955-R1 anchoring substrate production-ready
- [ ] `ReputationAnchorNode` struct + handlers implemented
- [ ] 3 payload kinds (`REPUTATION_QUERY`, `REPUTATION_UPDATE`, `REPUTATION_ANCHOR`) registered
- [ ] Reputation event emission per RFC-0968 §11. Audit Trail

## Claimant

@unassigned

## Pull Request

#

## Notes

- Layer C crate (specialized node). Stability: per-RFC.
- Reputation has economic implications (dual-stake model uses reputation for slashing weight). `REPUTATION_UPDATE` MUST require `Authorization::Capability`, not raw signature.
- Reputation events are append-only per RFC-0968 §11. Audit Trail — never in-place mutate. This is enforced by the registry API; the node is a thin wrapper.
- Cross-chain anchoring (specialized node ↔ external chain) is RFC-0871 §Future Work — not in this mission scope. Filed separately when needed.
- Production deployment: reputation anchor is a stateful actor (RFC-0871 §Roles and Authorities). Coordinator role + election required for multi-node deployment. Single-node mission for now; multi-node federation deferred.
- The dual-stake model (whitepaper §Token role table) uses reputation as a slashing weight; reputation inflation is an economic attack vector. `ReputationUpdateCaveat.max_delta_per_epoch` is the primary defense.
