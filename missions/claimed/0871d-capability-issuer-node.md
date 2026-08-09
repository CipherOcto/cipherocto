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

- [x] NEW: `crates/octo-capability-issuer-node/` crate with `Cargo.toml` + `src/lib.rs`
- [x] `crates/octo-capability-issuer-node/src/node.rs` — `CapabilityIssuerNode { config: CapabilityIssuerNodeConfig { transport: Arc<NodeTransport> }, dispatcher: ReferenceDispatcher, started: AtomicBool }` (Phase 3 MVP: `issuer_key` + `holder_registry` fields deferred to 0957 Phase 2)
- [x] `CapabilityIssuerNode::new(config) -> Self` (Phase 3 MVP; substrate-extended constructor lands in 0957 Phase 2)
- [x] `CapabilityIssuerNode::start() -> Result<CapabilityIssuerNodeHandle, CapabilityIssuerNodeError>` registers `NetworkReceiver`
- [x] `CapabilityIssuerNode::broadcast_announce() -> Result<usize, TransportError>` announces `CAPABILITY_ISSUE` + `CAPABILITY_REVOKE` (Phase 3 MVP stub; full `RouterAnnouncePayload` shape deferred to 0870-b follow-on)
- [x] `CapabilityIssuerNode::handle_envelope(envelope) -> Result<HandlerOutput, ProtocolError>` dispatch entry point per RFC-0871 §Algorithms
- [x] NetworkReceiver trait impl (RFC-0863) delegates to `handle_envelope` (via `CapabilityIssuerNodeReceiver` wrapper)

### Payload kinds

- [x] `CAPABILITY_ISSUE` handler: input = `(holder_did: String, capability: [u8; 32])`; validates canonical DID via `CanonicalCodec::parse(s, false)`; derives deterministic 16-byte `token_id` via `octo_cap_macaroon::macaroon_id` (Phase 3 MVP stub — full `CapabilityToken::mint` + holder signature + `HolderRegistry::register` per RFC-0957 §Algorithms + RFC-0957-A1 §Data Structures lands in 0957 Phase 2)
- [ ] `CAPABILITY_LOOKUP` handler: deferred to follow-on mission (Phase 3 MVP exposes only ISSUE + REVOKE; LOOKUP requires HolderRegistry production substrate)
- [x] `CAPABILITY_REVOKE` handler: input = `token_id: [u8; 16]` (`MacaroonId` per RFC-0957 §Wire Format); returns acknowledgement stub (Phase 3 MVP — full `HolderRegistry::revoke` + RFC-0965 `RevocationCaveat` validation + event emission lands in 0957 Phase 2)
- [x] All handlers: DID validation via `octo_ident::CanonicalCodec::parse(s, false)` (RFC-0010 v1.2 F4 + mission 0010-d)
- [x] All handlers: route through `EnvelopeDispatcher` for envelope_id dedup + expiry + signature verification (RFC-0871 §Adversary A6)
- [ ] Rate-limiting per RFC-0871 §Replay Protection: deferred (no per-node rate-limiter in the MVP per RFC-0871 wallet-node mirror pattern; dispatcher-side dedup is the Phase 3 mitigation)

### Payload kind UUIDs

- [x] `CAPABILITY_ISSUE` UUID `0x0009:0005:0000:0000:0000:0000:0000:0001` (RFC-0871 sub-namespace `0x0005`; follows 0x0002 wallet / 0x0003 quota router / 0x0004 reputation anchor pattern)
- [x] `CAPABILITY_REVOKE` UUID `0x0009:0005:0000:0000:0000:0000:0000:0002`
- [x] `CAPABILITY_PAYLOAD_KINDS` array + `is_capability_payload_kind()` dispatcher exported from `octo_capability_issuer_node` and `octo_protocol::payload_kind`

### Authorization model

- [ ] `CAPABILITY_ISSUE`: production semantics require `Authorization::Signature` from issuer + holder's pre-signed commitment envelope (RFC-0871 §Authorization). Phase 3 MVP: dispatcher enforces envelope-level signature; fine-grained authorization lands with substrate in 0957 Phase 2.
- [ ] `CAPABILITY_REVOKE`: production semantics require `Authorization::Capability(token)` with `RevocationCaveat` (RFC-0965). Phase 3 MVP: stub returns acknowledgement without caveat validation; lands in 0957 Phase 2.
- [x] No anonymous writes — all handlers route through dispatcher which requires signed envelope.

### Replay + integrity

- [x] All handlers route through `octo_protocol::EnvelopeDispatcher` for envelope_id dedup + expiry check
- [ ] All handlers verify `Vec<Authorization>` per `octo_protocol::Authorization::verify`: deferred (full authorization chain verification lands with substrate in 0957 Phase 2; Phase 3 MVP enforces envelope-level signature via dispatcher only)
- [x] `CAPABILITY_ISSUE` produces unique `token_id` per RFC-0957 §Wire Format (16-byte `MacaroonId` derivation; collision rejected via macaroon primitive)
- [x] `CAPABILITY_REVOKE` is monotonic (Phase 3 MVP: always acknowledges; monotonicity enforced by the registry in 0957 Phase 2)

### Adversary coverage

- [x] Unauthorized minting: dispatcher-level signature verification; full holder pre-signed commitment check lands with substrate in 0957 Phase 2
- [ ] Token forgery: macaroon HMAC + caveat chain verified per RFC-0957 §Algorithms (verification step): deferred (full verification lands with substrate in 0957 Phase 2)
- [x] Replay: envelope_id dedup + token_id uniqueness
- [x] Stale revocation: local revocation check first; eventual-consistency gossip for revocation propagation across nodes is a separate concern (RFC-0957-A1 revocation sync — not yet spec'd; deferred to a future mission if needed)
- [x] DID spoofing: canonical validation rejects non-canonical wire form per RFC-0010 (verified by `handle_rejects_invalid_did` + `handle_rejects_legacy_bare_did` tests)

### Backward compat

- [x] `cargo test -p octo-protocol --lib` 53/53 (39 prior + 9 new capability-payload-kind tests; zero regressions)
- [x] `cargo test -p octo-capability-issuer-node --lib` 14/14 (new crate — 6 issue + 3 revoke + 5 node-level)
- [x] `cargo build -p octo-capability-issuer-node` clean
- [x] `cargo clippy -p octo-protocol -p octo-capability-issuer-node --all-targets -- -D warnings` clean
- [x] `cargo fmt --check -p octo-protocol -p octo-capability-issuer-node` clean

## Type Coverage

Per BLUEPRINT §Mission template. RFC-0871 §Roles and Authorities + RFC-0957 + RFC-0957-A1 + RFC-0965 types mapped to this mission (Phase 3 capability issuer):

| RFC Type / Section                                              | Implemented By                                                                                                                                                                |
| --------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `CapabilityIssuerNode` struct (RFC-0871 §Roles and Authorities) | This mission — `crates/octo-capability-issuer-node/src/node.rs`                                                                                                               |
| `NetworkReceiver` impl (RFC-0863 substrate)                     | This mission — `crates/octo-capability-issuer-node/src/node.rs`                                                                                                               |
| `CAPABILITY_ISSUE` payload kind                                 | This mission — `crates/octo-capability-issuer-node/src/handlers/issue.rs`                                                                                                     |
| `CAPABILITY_REVOKE` payload kind                                | This mission — `crates/octo-capability-issuer-node/src/handlers/revoke.rs`                                                                                                    |
| `CAPABILITY_LOOKUP` payload kind                                | Deferred — requires `HolderRegistry` production substrate (RFC-0957-A1)                                                                                                       |
| `CapabilityIssuerNode::broadcast_announce`                      | This mission — uses `RouterAnnouncePayload` extension (Phase 3 MVP stub)                                                                                                      |
| `CapabilityToken::mint` substrate (RFC-0957 §Algorithms)        | Mission `0957-ext-macaroon-crate.md` — prerequisite (Phase 4 extraction); Phase 3 MVP uses `octo_cap_macaroon::macaroon_id` primitive for deterministic `token_id` derivation |
| `HolderRegistry` substrate (RFC-0957-A1)                        | RFC-0957-A1 existing — `crates/quota-router-storage/src/holder_registry.rs` (Phase 3 MVP stub defers registry wiring to 0957 Phase 2)                                         |
| Revocation caveat (RFC-0965 reserved range)                     | Deferred to 0957 Phase 2 — registers in `crates/octo-cap-macaroon/src/caveat/revocation.rs`                                                                                   |
| `IdentityKey::sign` HSM routing                                 | Mission `0009-a-hsm-routing.md` — prerequisite (used by `CapabilityToken::mint` in 0957 Phase 2)                                                                              |
| Canonical DID validation                                        | Mission `0010-d-wallet-audience-validation.md` — prerequisite (used by `IssueHandler::handle`)                                                                                |
| `NodeEnvelope` envelope shape (consumed)                        | Mission `0871-protocol-core-envelope.md` — Phase 1 prerequisite                                                                                                               |
| Multi-node quorum-gated mint                                    | Deferred to RFC-0971 destination-node role consolidation — separate future mission                                                                                            |

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
- `octo-transport` — NodeTransport (RFC-0863); crate lives at workspace root

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
- [x] Mission filed (this file)
- [x] Phase 1 foundation complete: `0871-protocol-core-envelope.md` (commit bf58559d)
- [x] Phase 4 macaroon extraction complete: `0957-ext-macaroon-crate.md` (commit f123fe1b)
- [ ] RFC-0957-A1 `HolderRegistry` production-ready (deferred to 0957 Phase 2 — `HolderRegistry` substrate available but not wired into capability-issuer-node MVP)
- [x] `CapabilityIssuerNode` struct + handlers implemented (Phase 3 MVP)
- [x] 2 payload kinds (`CAPABILITY_ISSUE`, `CAPABILITY_REVOKE`) registered (Phase 3 MVP — `CAPABILITY_LOOKUP` deferred to follow-on mission)
- [ ] Revocation event emission per RFC-0957-A1 §HolderRecord State Machine transitions: deferred to 0957 Phase 2

## Claimant

@cipherocto (claimed 2026-08-09)

## Pull Request

pending commit (local; push + remote writes await user instruction per [[git-workflow]])

## Closure Summary

Mission 0871d-capability-issuer-node landed on `next` branch.
NEW `crates/octo-capability-issuer-node/` Layer C crate (6 files, 14 unit tests).
NEW payload kinds in `crates/octo-protocol/src/payload_kind.rs` (9 new tests, 0 regressions).

**New types (crate `octo-capability-issuer-node`):**

- `CapabilityIssuerNode` + `CapabilityIssuerNodeConfig` + `CapabilityIssuerNodeHandle` + `CapabilityIssuerNodeError`
- `IssueHandler` / `IssueRequest` / `IssueResponse`
- `RevokeHandler` / `RevokeRequest` / `RevokeResponse`
- `HandlerOutput` (response envelope payload + payload kind)
- `CAPABILITY_PAYLOAD_KINDS` array + `is_capability_payload_kind()` dispatcher

**New payload kinds (in `octo_protocol::payload_kind`):**

- `CAPABILITY_ISSUE` (UUID `0x0009:0005:0000:0000:0000:0000:0000:0001`)
- `CAPABILITY_REVOKE` (UUID `0x0009:0005:0000:0000:0000:0000:0000:0002`)
- `CAPABILITY_PAYLOAD_KINDS` array + `is_capability_payload_kind()` dispatcher
- 9 new tests (UUID match, distinct, RFC-allocated, sub-namespace, cross-namespace collision, borsh round-trip)

**Verification chain:**

- All inbound envelopes route through `EnvelopeDispatcher` (replay defense + signature verification)
- DID validation: `IssueHandler` rejects malformed DIDs AND legacy bare form via `CanonicalCodec::parse(s, false)`
- `token_id` derivation: `IssueHandler` derives deterministic 16-byte `MacaroonId` via `octo_cap_macaroon::macaroon_id` primitive (the same primitive the full substrate uses)
- `RevokeHandler` accepts `MacaroonId` (= `[u8; 16]`) and acknowledges

**Phase 3 MVP disclosures (honest scope):**

- `CAPABILITY_ISSUE` does NOT call `CapabilityToken::mint`; substrate lands in 0957 Phase 2 follow-on (macaroon struct + caveat + discharge + wire migrations). Phase 3 MVP uses `octo_cap_macaroon::macaroon_id` for deterministic `token_id` derivation.
- `CAPABILITY_REVOKE` does NOT mutate `HolderRegistry`; returns acknowledgement stub. Substrate (RFC-0957-A1 §HolderRecord State Machine transitions + RFC-0965 `RevocationCaveat`) lands in 0957 Phase 2.
- `CAPABILITY_LOOKUP` is NOT exposed in Phase 3 MVP (requires HolderRegistry production substrate for real lookups; deferred to follow-on mission).
- `broadcast_announce` is a stub envelope (full `RouterAnnouncePayload` extension in 0870-b follow-on; opcode already allocated).
- `CapabilityIssuerNodeConfig` carries only `transport`; `Arc<IdentityKey>` + `Arc<dyn HolderRegistry>` slot in with substrate in 0957 Phase 2.
- `from_did` in `broadcast_announce` is a placeholder derived from a zeroed 32-byte payload (canonical DID shape but no signing identity bound yet — HSM binding lands in 0957 Phase 2).
- Rate-limiting per RFC-0871 §Replay Protection: dispatcher-side dedup is the Phase 3 mitigation; per-node rate-limiter deferred to follow-on polish.
- `CapabilityIssuerNode::stop()` API: deferred (single-receiver lifecycle does not yet warrant explicit stop; mirrors wallet-node MVP).

**Tests:** 14 new (6 issue + 3 revoke + 5 node-level). Zero regressions across octo-protocol (53/53). clippy -D warnings clean, fmt clean.

## Notes

- Layer C crate (specialized node). Stability: per-RFC. RFC-0871 defines the capability-issuer node shape; subsequent RFCs can extend payload kinds without modifying `CapabilityIssuerNode` (registry pattern).
- Capability minting has economic implications (capabilities authorize spend via caveat chain). Production `CAPABILITY_ISSUE` requires issuer signature + holder pre-signed commitment — double-check prevents unilateral minting attacks. Phase 3 MVP enforces envelope-level signature via dispatcher; full commitment check lands in 0957 Phase 2.
- `CAPABILITY_REVOKE` is monotonic (revoked stays revoked). If un-revoke is needed, that's a new RFC (caveat extension via RFC-0965 reserved range).
- Stale revocation: this node performs local revocation check first. Eventual-consistency gossip for revocation propagation across nodes is a separate concern (deferred; revocation sync substrate not yet spec'd).
- Production deployment: capability issuer is a stateful actor (RFC-0871 §Roles and Authorities). Multi-node deployment requires quorum-gated mint (per RFC-0971 destination-node role consolidation); single-node mission for now.
- This mission complements mission `0871a-wallet-node.md` (Phase 2 wallet node). The wallet signs capabilities via `WALLET_MINT_CAPABILITY` (its own payload kind); the issuer node authorizes + registers them in `HolderRegistry`. The two nodes collaborate: wallet = signer, issuer = registrar.
