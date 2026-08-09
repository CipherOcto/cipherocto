# Mission: 0871d — Capability Issuer Node (RFC-0871 Phase 3)

## Status

Open (2026-08-09). RFC-0871 Accepted 2026-08-09 after R1–R7 adversarial review (R7 DRY). RFC-0957 + RFC-0957-A1 + RFC-0965 Accepted. Phase 3 capability issuer node mission.

## RFC

RFC-0871 (Networking): Specialized Node Protocol Envelope
RFC-0957 (Economics): Capability Token Format (Macaroon v1)
RFC-0957-A1 (Economics): Holder Registry + Catalog Storage (Amendment)
RFC-0965 (Economics): Capability Extension Format

**BLUEPRINT gate note:** All substrate RFCs Accepted. Mission 0871d implements a specialized node of type `capability-issuer` per RFC-0871 §Roles and Authorities. No new RFC required — node shape is fully defined by RFC-0871 + RFC-0957-A1.

This mission creates `crates/octo-capability-issuer-node/` — a specialized node that wraps RFC-0957 capability minting + RFC-0957-A1 `HolderRegistry` operations behind a `NodeEnvelope` interface. Advertises `CAPABILITY_MINT` + `CAPABILITY_LOOKUP` + `CAPABILITY_REVOKE` payload kinds. Reuses `octo-protocol::NodeEnvelope` from mission `0871-protocol-core-envelope.md`. Per-extension crate substrate (mission `0957-ext-macaroon-crate.md`) provides the macaroon impl; this node wraps it.

## Summary

Build `crates/octo-capability-issuer-node/` — the capability issuer specialized node. `CapabilityIssuerNode` wraps RFC-0957 `CapabilityToken` mint substrate + RFC-0957-A1 `HolderRegistry` + `Arc<NodeTransport>`. It registers as `NetworkReceiver`, advertises payload kinds `CAPABILITY_MINT` (write, requires authorization), `CAPABILITY_LOOKUP` (read, requires authentication), `CAPABILITY_REVOKE` (write, requires authorization). Reuses `octo-protocol::NodeEnvelope`. All minting goes through `Authorization::Capability` chain (issuer authority proven) + holder signature via HSM.

## Acceptance Criteria

### Top-level: Crate + node

- [ ] NEW: `crates/octo-capability-issuer-node/` crate with `Cargo.toml` + `src/lib.rs`
- [ ] `crates/octo-capability-issuer-node/src/node.rs` — `CapabilityIssuerNode { issuer_key: Arc<dyn HsmAdapter>, holder_registry: Arc<dyn HolderRegistry>, transport: Arc<NodeTransport>, handlers: HashMap<PayloadKindId, Arc<dyn EnvelopeHandler>> }`
- [ ] `CapabilityIssuerNode::new(issuer_key, holder_registry, transport) -> Self`
- [ ] `CapabilityIssuerNode::start() -> Result<ReceiverId, IssuerNodeError>` registers `NetworkReceiver`
- [ ] `CapabilityIssuerNode::broadcast_announce() -> Result<usize, TransportError>` announces `CAPABILITY_MINT` + `CAPABILITY_LOOKUP` + `CAPABILITY_REVOKE`
- [ ] `CapabilityIssuerNode::handle_envelope(envelope) -> Result<HandlerOutput, ProtocolError>`
- [ ] NetworkReceiver trait impl (RFC-0863)

### Payload kinds

- [ ] `CAPABILITY_MINT` handler: input = `(holder_did, capability, caveats)`; requires `Authorization::Signature` from issuer + holder's pre-signed commitment; calls `CapabilityToken::mint` per RFC-0957 §Algorithms; signs holder signature via HSM; registers in `HolderRegistry` per RFC-0957-A1 §Data Structures; returns minted token + holder signature
- [ ] `CAPABILITY_LOOKUP` handler: input = `(holder_did, token_id)`; validates via `octo_ident::CanonicalCodec::parse`; queries `HolderRegistry`; returns capability + caveats + revocation status (no secret material exposed)
- [ ] `CAPABILITY_REVOKE` handler: input = `token_id`; requires `Authorization::Capability(revocation_token)` carrying a revocation caveat (RFC-0965 caveat discriminator; revocation is a caveat-encoded capability, not a separate RFC-0957 section); updates `HolderRegistry` revocation status; emits revocation event per RFC-0957-A1 §HolderRecord State Machine transitions
- [ ] All handlers: DID validation via `octo_ident::CanonicalCodec::parse(s, false)`
- [ ] All handlers: rate-limited per RFC-0871 §Replay Protection

### Authorization model

- [ ] `CAPABILITY_MINT`: requires `Authorization::Signature` from issuer (issuer's HSM key) + holder's pre-signed commitment envelope (proves holder consent)
- [ ] `CAPABILITY_LOOKUP`: requires authenticated caller (any valid `Authorization::Signature`); no anonymous lookups
- [ ] `CAPABILITY_REVOKE`: requires `Authorization::Capability(token)` with `RevocationCaveat` (RFC-0965 caveat type) issued by either the original issuer OR a higher-authority governance capability
- [ ] No anonymous writes — capability minting has economic implications (capabilities authorize spend via caveat chain)

### Replay + integrity

- [ ] All handlers route through `octo_protocol::EnvelopeDispatcher` for envelope_id dedup + expiry check
- [ ] All handlers verify `Vec<Authorization>` per `octo_protocol::Authorization::verify`
- [ ] `CAPABILITY_MINT` produces unique `token_id` per RFC-0957 §Wire Format (token_id is the macaroon identifier bytes) — collision rejected at registry level
- [ ] `CAPABILITY_REVOKE` is monotonic (revoked stays revoked; no un-revoke)

### Adversary coverage

- [ ] Unauthorized minting: issuer signature required; holder pre-signed commitment required; double-check enforced at handler
- [ ] Token forgery: macaroon HMAC + caveat chain verified per RFC-0957 §Algorithms (verification step)
- [ ] Replay: envelope_id dedup + token_id uniqueness
- [ ] Stale revocation: local revocation check first; eventual-consistency gossip for revocation propagation across nodes is a separate concern (RFC-0957-A1 revocation sync — not yet spec'd; deferred to a future mission if needed)
- [ ] DID spoofing: canonical validation rejects non-canonical wire form per RFC-0010

### Backward compat

- [ ] `cargo test -p octo-wallet --lib capability` continues green (no regression in existing macaroon tests)
- [ ] `cargo test -p octo-capability-issuer-node --lib` green (new crate)
- [ ] `cargo clippy --workspace --all-targets --features full -- -D warnings` clean
- [ ] `cargo fmt --check` clean

## Type Coverage

Per BLUEPRINT §Mission template. RFC-0871 §Roles and Authorities + RFC-0957 + RFC-0957-A1 + RFC-0965 types mapped to this mission (Phase 3 capability issuer):

| RFC Type / Section | Implemented By |
|---|---|
| `CapabilityIssuerNode` struct (RFC-0871 §Roles and Authorities) | This mission — `crates/octo-capability-issuer-node/src/node.rs` |
| `NetworkReceiver` impl (RFC-0863 substrate) | This mission — `crates/octo-capability-issuer-node/src/node.rs` |
| `CAPABILITY_MINT` payload kind | This mission — `crates/octo-capability-issuer-node/src/handlers/mint.rs` |
| `CAPABILITY_LOOKUP` payload kind | This mission — `crates/octo-capability-issuer-node/src/handlers/lookup.rs` |
| `CAPABILITY_REVOKE` payload kind | This mission — `crates/octo-capability-issuer-node/src/handlers/revoke.rs` |
| `CapabilityIssuerNode::broadcast_announce` | This mission — uses `RouterAnnouncePayload` extension |
| `CapabilityToken::mint` substrate (RFC-0957 §Algorithms) | Mission `0957-ext-macaroon-crate.md` — prerequisite (Phase 4 extraction) |
| `HolderRegistry` substrate (RFC-0957-A1) | RFC-0957-A1 existing — `crates/quota-router-storage/src/holder_registry.rs` |
| Revocation caveat (RFC-0965 reserved range) | This mission — registers in `crates/octo-cap-macaroon/src/caveat/revocation.rs` |
| `IdentityKey::sign` HSM routing | Mission `0009-a-hsm-routing.md` — prerequisite |
| Canonical DID validation | Mission `0010-d-wallet-audience-validation.md` — prerequisite |
| `NodeEnvelope` envelope shape (consumed) | Mission `0871-protocol-core-envelope.md` — Phase 1 prerequisite |
| Multi-node quorum-gated mint | Deferred to RFC-0971 destination-node role consolidation — separate future mission |

## Dependencies

**Requires:**

- RFC-0871 — accepted substrate (envelope shape)
- RFC-0957 — capability token format (macaroon v1)
- RFC-0957-A1 — holder registry + catalog storage substrate
- RFC-0965 — caveat discriminator (for `RevocationCaveat`)
- RFC-0863 — `NodeTransport` + `NetworkReceiver` trait
- `crates/octo-protocol` — Phase 1 envelope types (mission `0871-protocol-core-envelope.md`)
- `crates/octo-wallet` — wallet substrate (`CapabilityToken::mint`, macaroon substrate)
- `crates/octo-ident` — DID parsing
- `crates/octo-transport` — NodeTransport (RFC-0863)

**Mission gates (sequential):**

- Mission `0871-protocol-core-envelope.md` MUST complete first (Phase 1 dependency)
- Mission `0957-ext-macaroon-crate.md` MUST complete first (Phase 4 — `CapabilityIssuerNode` wraps the extracted `crates/octo-cap-macaroon/` crate, not the old monolithic `crates/octo-wallet/src/capability/macaroon.rs` path)
- RFC-0957-A1 `HolderRegistry` MUST be production-ready

**Parallel with (no dependency):**

- Mission `0870-b-envelope-adoption.md` (Phase 3 quota router)
- Mission `0871a-wallet-node.md` (Phase 2 wallet node)
- Mission `0871b-identity-resolver-node.md` (Phase 3 identity resolver)
- Mission `0871c-reputation-anchor-node.md` (Phase 3 reputation anchor)

**Not Requires:**

- Mission `0871e-paid-query-caveat.md` (Phase 5 — separate; this mission is minting substrate; paid query is a usage pattern built on top)
- Mission `0957-ext-zk-crate.md` (Phase 4 ZK extension — orthogonal, separate envelope handlers if needed)

## Implementation Guide

- NEW crate: `crates/octo-capability-issuer-node/` with `src/lib.rs`, `src/node.rs`, `src/handlers/{mint,lookup,revoke}.rs`, `tests/`
- `CapabilityIssuerNode::start()` registers `NetworkReceiver` via `transport.register_receiver(self.clone())` per RFC-0863 wiring
- `CapabilityIssuerNode::handle_envelope`:
  1. `EnvelopeDispatcher::dispatch` (from `octo-protocol`) — envelope_id dedup + expiry + signature verification
  2. Route by `envelope.payload_kind` to handler map
  3. Handler calls macaroon substrate (`CapabilityToken::mint`, etc.) + `HolderRegistry` (RFC-0957-A1); returns response envelope
- `Cargo.toml` deps per CLAUDE.md crate stability: `octo-protocol` (Layer A), `octo-cap-macaroon` (Layer E extension; post Phase 4 extraction), `octo-wallet` (Layer B substrate), `octo-transport` (Layer D)

## Acceptance Cross-Ref

Per RFC-0871 §Implementation Phases Phase 3 + RFC-0957 §Algorithms + RFC-0957-A1 §Data Structures:

- [x] RFCs Accepted (RFC-0871, RFC-0957, RFC-0957-A1, RFC-0965)
- [ ] Mission filed (this file)
- [ ] Phase 1 foundation complete: `0871-protocol-core-envelope.md`
- [ ] Phase 4 macaroon extraction complete: `0957-ext-macaroon-crate.md` (recommended)
- [ ] RFC-0957-A1 `HolderRegistry` production-ready
- [ ] `CapabilityIssuerNode` struct + handlers implemented
- [ ] 3 payload kinds (`CAPABILITY_MINT`, `CAPABILITY_LOOKUP`, `CAPABILITY_REVOKE`) registered
- [ ] Revocation event emission per RFC-0957-A1 §HolderRecord State Machine transitions

## Claimant

@unassigned

## Pull Request

#

## Notes

- Layer C crate (specialized node). Stability: per-RFC.
- Capability minting has economic implications (capabilities authorize spend via caveat chain). `CAPABILITY_MINT` requires issuer signature + holder pre-signed commitment — double-check prevents unilateral minting attacks.
- `CAPABILITY_REVOKE` is monotonic (revoked stays revoked). If un-revoke is needed, that's a new RFC (caveat extension via RFC-0965 reserved range).
- Stale revocation: this node performs local revocation check first. Eventual-consistency gossip for revocation propagation across nodes is a separate concern (deferred; revocation sync substrate not yet spec'd).
- Production deployment: capability issuer is a stateful actor (RFC-0871 §Roles and Authorities). Multi-node deployment requires quorum-gated mint (per RFC-0971 destination-node role consolidation); single-node mission for now.
- This mission complements mission `0871a-wallet-node.md` (Phase 2 wallet node). The wallet signs capabilities via `WALLET_MINT_CAPABILITY` (its own payload kind); the issuer node authorizes + registers them in `HolderRegistry`. The two nodes collaborate: wallet = signer, issuer = registrar.
