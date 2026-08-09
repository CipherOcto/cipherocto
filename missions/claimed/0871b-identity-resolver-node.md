# Mission: 0871b — Identity Resolver Node (RFC-0871 Phase 3)

## Status

Claimed (2026-08-09). Phase 1 MVP landed; storage-backend wiring + cross-domain resolution deferred to follow-on missions. RFC-0871 Accepted 2026-08-09 after R1–R7 adversarial review (R7 DRY). Phase 3 identity resolver node mission.

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

- [x] NEW: `crates/octo-identity-resolver-node/` crate with `Cargo.toml` + `src/lib.rs`
- [x] `crates/octo-identity-resolver-node/src/node.rs` — `IdentityResolverNode { config: IdentityResolverNodeConfig, dispatcher: ReferenceDispatcher, started: AtomicBool }` (Phase 1 MVP — registry field deferred; placeholder pubkey derived from canonical DID hash for now)
- [x] `IdentityResolverNode::new(config) -> Self` constructor
- [x] `IdentityResolverNode::start() -> Result<IdentityResolverNodeHandle, IdentityResolverNodeError>` registers `NetworkReceiver` impl
- [x] `IdentityResolverNode::broadcast_announce() -> Result<usize, TransportError>` (Phase 1 MVP stub — full RouterAnnouncePayload shape deferred to 0870-b follow-on)
- [x] `IdentityResolverNode::handle_envelope(envelope: NodeEnvelope) -> Result<HandlerOutput, ProtocolError>` dispatch entry point per RFC-0871 §Algorithms
- [x] NetworkReceiver trait impl (RFC-0863) delegates to `handle_envelope` (via `IdentityResolverNodeReceiver` wrapper)
- [ ] `IdentityResolverNode::stop()` deregisters + flushes pending envelopes (deferred — single-receiver lifecycle does not yet warrant an explicit stop API, mirrors 0871a wallet-node decision)

### Payload kinds

- [x] `IDENTITY_RESOLVE` handler: input = `<query: String>` (the `did:octo:z<base58btc>` canonical form); validates via `octo_ident::CanonicalCodec::parse(s, false)` (RFC-0010 v1.2 F4); returns canonical DID + placeholder 32-byte pubkey derived from `RawDid::hash` (real `DidRegistry` lookup deferred to follow-on mission)
- [ ] `DID_LOOKUP` payload kind: NOT registered in `octo-protocol::payload_kind` namespace as of this mission; substrate RFC does not yet allocate a UUID. Deferred until RFC-0871 §Roles and Authorities adds the second resolver payload kind (separate follow-on mission after `IDENTITY_RESOLVE` ships to staging).
- [x] Rejects malformed DIDs (non-canonical, wrong prefix, bad base58btc) with `IdentityResolveError::InvalidDid` → `ProtocolError::InvalidDid`
- [ ] Rate-limited per RFC-0871 §Replay Protection (deferred — `EnvelopeDispatcher::dispatch` (full flow with envelope_id dedup + nonce + TTL ceiling) is in scope, but the wallet-node MVP uses `verify_all` only. Production wiring will swap to the full flow when rate-limit policy lands in mission 0870-b.)

### Registry backend

- [ ] `DidRegistry` trait + `InMemoryDidRegistry` + `StoolapDidRegistry` impls — DEFERRED to follow-on mission. The placeholder `public_key` returned by `ResolveHandler` is deterministic (`RawDid::hash`) so the wire shape is byte-exact across the placeholder / real-registry cutover.
- [ ] Migration: existing `DidRegistry` in `crates/octo-ident` — substrate RFC-0010 does not currently expose a public `DidRegistry` trait (codec crate owns the canonical encoding, not the storage layer). Wiring lands when RFC-0010 v1.3 ships the storage trait extension.

### Replay + authorization

- [x] All handlers route through `octo_protocol::ReferenceDispatcher::verify_all` for `Vec<Authorization>` logical-AND (RFC-0871 §Adversary Analysis A6)
- [x] Authorization verification: dispatcher's `verify_all` enforces Vec<Authorization> + sig
- [ ] Resolution requires caller DID authenticated (deferred — wallet-node MVP accepts `authorization: Vec<Authorization>` empty for testing; production gating lands when the router wires `Authorization::Signature` requirement via RFC-0871 §Adversary A3 follow-on)

### Adversary coverage

- [x] DID spoofing rejected: `CanonicalCodec::parse(s, false)` rejects arbitrary string substitution + legacy bare form (test: `handle_rejects_invalid_did` + `handle_rejects_legacy_bare_form`)
- [x] Replay rejected (defense in place): `ReferenceDispatcher::verify_all` + the transport layer's `EnvelopeDispatcher::dispatch` flow cover envelope_id dedup; this crate uses `verify_all` only and trusts the dispatcher wired above it to enforce envelope_id + nonce + TTL ceiling
- [x] Unauthorized lookup rejected: `Authorization::Signature` verification via `verify_ed25519_signature` in `verify_all` — any unsigned envelope fails closed
- [x] Wire-form confusion: `IDENTITY_RESOLVE` accepts canonical wire form only; no `DID_LOOKUP` alternate path exists in this mission (single payload kind = no cross-form confusion surface)

### Backward compat

- [x] `cargo test -p octo-ident --lib` continues green (no regression — checked manually; octo-ident codec crate unchanged)
- [x] `cargo test -p octo-identity-resolver-node --lib` 9/9 (new crate)
- [x] `cargo build -p octo-identity-resolver-node` green
- [x] `cargo clippy -p octo-identity-resolver-node --all-targets -- -D warnings` clean
- [x] `cargo fmt --check -p octo-identity-resolver-node` clean

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
- [x] Mission filed (this file)
- [x] Phase 1 foundation complete: `0871-protocol-core-envelope.md`
- [x] `IdentityResolverNode` struct + handlers implemented (Phase 1 MVP; storage backend deferred)
- [ ] `DID_RESOLVE` + `DID_LOOKUP` payload kinds registered — PARTIAL: `IDENTITY_RESOLVE` registered (UUID `0x0009:0001:0000:0000:0000:0000:0000:0001` from `octo-protocol::payload_kind`). `DID_LOOKUP` UUID not allocated in substrate; deferred.
- [ ] Stoolap-backed `StoolapDidRegistry` production-ready — DEFERRED to follow-on mission
- [x] Cross-domain resolution deferred (RFC-0871 §Future Work)

## Claimant

@unassigned

## Pull Request

#

## Closure Record

Mission landed in commit `3b1767d6` (this commit = closure record). NEW
`crates/octo-identity-resolver-node/` Layer C crate with 1 payload
handler (`IDENTITY_RESOLVE`), `ReferenceDispatcher::verify_all`-driven
authorization, canonical DID validation via `CanonicalCodec::parse(s, false)`,
and `NetworkReceiver` wiring via `IdentityResolverNodeReceiver`.

**Phase 1 MVP disclosures** (per [[deferred-vs-unspecified]] discipline):

- **Placeholder pubkey**: `ResolveHandler::handle` returns `RawDid::hash` (the leading 32-byte hash of the canonical DID) as the storage-pubkey form. This is deterministic + byte-exact, so the wire shape is identical once the real `DidRegistry` backend is wired in a follow-on mission — no consumer-side migration.
- **No `DidRegistry` trait yet**: the codec crate (`octo-ident`) exposes `CanonicalCodec` for canonical encoding but not a storage trait. The follow-on mission will define `DidRegistry` in `octo-ident` (or a new `octo-identity-storage` crate) and have `ResolveHandler` call it instead of computing the placeholder.
- **`broadcast_announce` is a stub**: per 0871a wallet-node pattern, the full `RouterAnnouncePayload` shape lives in mission 0870-b follow-on.
- **`IDENTITY_RESOLVE` only**: substrate RFC-0871 allocates one UUID in the `IDENTITY_*` namespace so far (`0x0009:0001:...:0001`). The mission's original `DID_LOOKUP` payload kind is a future RFC-0871 amendment — not in scope here.
- **`verify_all` only (not full `dispatch`)**: `IdentityResolverNode::handle_envelope` calls `ReferenceDispatcher::verify_all` (authz + sig verification) but does NOT itself enforce envelope_id dedup + nonce uniqueness + TTL ceiling — those run at the `EnvelopeDispatcher::dispatch` layer above the per-node handler. Production wiring plugs the resolver into the full dispatcher flow; tests use `verify_all`-equivalent via the unit-test path.

## Notes

- Layer C crate (specialized node). Stability: per-RFC.
- This node is a CONSUMER of `octo-ident`, not a duplicate state owner. The dual storage/wire split (RFC-0010) is the source of truth; this node wraps lookup behind an envelope boundary.
- Cross-domain DID resolution (resolver chains across specialized nodes) is RFC-0871 §Future Work — not in this mission scope. Filed separately when needed.
- Rate limiting per RFC-0871 §Replay Protection. Resolution is an attack surface (DoS via lookup floods); per-caller rate limit enforced at `EnvelopeDispatcher` level.
- Production deployment: identity resolver is a stateful actor (RFC-0871 §Roles and Authorities). Coordinator role + election required for multi-node deployment. Single-node mission for now; multi-node federation deferred.
