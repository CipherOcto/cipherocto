# Research: Specialized Node Protocol Envelope — Wide Survey

> **Status:** Draft (RFC-0871 input). Compiled 2026-08-08 from public sources + CipherOcto's own prior RFCs.

## Executive Summary

CipherOcto's mesh today has one specialized node type: the Quota Router (RFC-0870). Identity resolution, reputation anchoring, capability issuance, and market settlement each have RFCs but no shared envelope — each would re-invent its own wire format. This survey maps the design space for a **uniform specialized-node protocol envelope** that lets any number of node types coexist without central enum constraints.

**Key conclusion:** the right design is a typed message envelope with a 128-bit payload discriminator (RFC-0965 caveat pattern), DID-bound identity (RFC-0010), and a vector of authorization mechanisms (signature / capability / ZK proof / threshold signature) where a capability token IS one form of authorization. No central `NodeType` or `PayloadKind` enums — those are extension-hostile.

## Problem Statement

CipherOcto's grand-design mission family (RFC-0955 through RFC-0971) defines seven specialized node roles:

| Node | RFC | Today's state |
|---|---|---|
| Quota router | RFC-0870 (Accepted) | Reference impl in `crates/quota-router-core/src/node/mod.rs::QuotaRouterNode` |
| Identity resolver | RFC-0009 + RFC-0010 | DID codec in `crates/octo-ident/`; no mesh node yet |
| Reputation anchor | RFC-0968 (Accepted) | Substrate in `crates/octo-reputation/`; not network-wired |
| Capability issuer | RFC-0957-A1 (Accepted) | `CapabilityCatalog` in `crates/octo-wallet/src/capability/macaroon.rs`; gossip via `NodeTransport::broadcast` |
| Market node | RFC-0959-A1 (Accepted) | `MarketDeliveryEnvelope` substrate; settlement primitives in `octo-wallet/src/capability/settle.rs` |
| Wallet | (NEW) | `HsmAdapter` exists; not network-participating |
| Future nodes | TBD | (governance, oracle, identity federation, ...) |

Each existing node speaks a slightly different wire format. Quota router has `RouterAnnouncePayload`. Capability issuer has `MarketDeliveryEnvelope`. Reputation has anchor batch gossip. Adding a new node today = invent a new envelope = bridge code per pair.

## Research Scope

Map the design space for a uniform protocol envelope by surveying:

1. Smart contract message-passing (Ethereum, Solana)
2. Cosmos SDK modules + msg router + IBC packet semantics
3. Polkadot XCMP + relay chain
4. libp2p request-response pattern with protocol negotiation
5. W3C DID Resolution + DIDComm v2
6. Capability-based authorization (Cap'n Proto, E, KeyNote)
7. gRPC + protobuf service definitions

For each: extract the load-bearing abstraction, identify what CipherOcto already has, identify what would be parallel/conflicting.

## Findings

### 1. Smart contract message passing (Ethereum, Solana)

**Pattern:** Contract method dispatch via 4-byte selector (Ethereum) or program-derived instruction discriminator (Solana). The selector is a hash of the method signature `(name, arg_types)` — no central registry of method names.

**CipherOcto mapping:** `payload_kind` discriminator in the envelope. 4-byte selector is too small (256 kinds, exhausting). 128-bit UUID (`PayloadKindId`) gives 2^128 namespace per RFC allocation.

**What CipherOcto already has:** RFC-0965 caveat discriminator bytes (1-byte, RFC-allocated + application-specific ranges). The same pattern generalizes to 16 bytes for payload kinds.

**Conflict risk:** none. UUIDs are larger namespace than bytes; layering is additive.

### 2. Cosmos SDK modules + msg router + IBC

**Pattern:** Each module defines a `Msg` type with a `Type()` returning a string (e.g., `"/cosmos.bank.v1beta1.MsgSend"`). The base app's msg router dispatches by `Type()`. IBC adds cross-chain packet semantics: `Packet { source_channel, destination_channel, data, timeout_height, timeout_timestamp }` with ack + timeout callbacks.

**CipherOcto mapping:**
- Module `Msg` → envelope `payload_kind` + payload body
- Msg router → node-side handler that registers `NetworkReceiver` impls and dispatches per `payload_kind`
- IBC packet → envelope + `nonce` + `expires_at_unix_ms` (replay defense + TTL); ack = response envelope

**What CipherOcto already has:** `octo-transport::NodeTransport` does fan-out broadcast + fan-in dispatch to receivers. Same primitive as the msg router.

**Conflict risk:** the SDK approach uses strings as discriminators (no allocation authority) — CipherOcto's UUID approach is more rigorous. No conflict.

### 3. Polkadot XCMP + relay chain

**Pattern:** Parachains exchange messages via Cross-Consensus Messaging (XCM). XCM instructions are a typed enum (`Xcm::ReserveAssetDeposited`, `Xcm::Transact`, etc.). The relay chain orders messages but doesn't interpret them.

**CipherOcto mapping:**
- Parachain = specialized node
- XCM instruction = `payload_kind` + `payload` body
- Relay chain = mesh gossip layer

**Conflict risk:** XCM's enum-typed approach is the OPPOSITE of what we want. XCM upgrades require coordinated runtime upgrades across all parachains. CipherOcto's UUID approach avoids this.

**Lesson:** typed enums = upgrade-hostile. UUIDs + raw escape hatch = upgrade-friendly. **Reject the XCM typed-enum approach; adopt the discriminator-byte pattern from RFC-0965.**

### 4. libp2p request-response pattern

**Pattern:** Protocols are identified by `/proto-name/version` strings. Each protocol implementation registers a handler at the protocol ID. Multistream-select negotiates the protocol on connection.

**CipherOcto mapping:** protocol ID = `payload_kind` (128-bit UUID). Multistream-select maps to `NodeTransport::register_receiver(...)`. Each specialized node declares which payload kinds it serves.

**Conflict risk:** none. libp2p's string protocol IDs work fine; UUIDs work fine. Pick the more rigorous (UUIDs).

### 5. W3C DID Resolution + DIDComm v2

**Pattern:**
- **DID Resolution:** `did:method:identifier` → DID Document (verification methods, service endpoints, etc.). Methods are registered with W3C.
- **DIDComm v2:** Messages addressed by `did:`, encrypted to recipient's public key, signed by sender. Message types are URI strings (`https://didcomm.org/example/1.0/send`).

**CipherOcto mapping:**
- DID Resolution: RFC-0010 canonical DID codec + `DidResolver` trait
- DIDComm: envelope IS a signed+addressed message; `payload_kind` IS the message type URI

**What CipherOcto already has:** RFC-0010 (`crates/octo-ident::DidCodec`), RFC-0010 critique already identified that `octo-wallet::AudienceId::from_str` doesn't validate. **Wallet gap to close.**

**Conflict risk:** DIDComm uses URI message types (`https://...`); CipherOcto uses UUID. Both work. UUIDs are easier to allocate centrally (RFC-numbered) without registering with W3C. **Adopt UUID-based; cross-reference DIDComm in Future Work.**

### 6. Capability-based authorization (Cap'n Proto, E, KeyNote)

**Pattern:** A capability is an unforgeable token (cryptographic or capability list) that authorizes a specific action. Capabilities can be delegated (attenuated) without exposing the root authority.

**CipherOcto mapping:** RFC-0957 macaroon IS a capability token. RFC-0965 caveats ARE attenuations. `Factory` caveat (0x17) IS pre-validated invocation.

**Conflict risk:** None. RFC-0957 is already the capability substrate. The protocol envelope just **composes** with it: `Authorization::Capability(token)` is one of the `Vec<Authorization>` slots.

**Insight (from research):** Cap'n Proto has the cleanest pattern — capabilities are passed as references to opaque objects; the receiver can read them but cannot forge them. RFC-0957 macaroons are similar but signature-based. The protocol envelope treats `Authorization::Capability` as opaque (the verifier unpacks).

### 7. gRPC + protobuf service definitions

**Pattern:** Service definition in `.proto` file; method names + request/response types are typed at compile time. Streaming via `stream` keyword.

**CipherOcto mapping:**
- `.proto` service → `payload_kind` (UUID)
- method request/response → envelope request + response envelope (separate UUIDs or same kind with direction flag)
- Streaming → mesh gossip + NodeTransport::broadcast

**Conflict risk:** gRPC uses string method names; UUIDs are more rigorous. No conflict; UUIDs win.

**Insight:** gRPC's typed errors (`google.rpc.Status`) map to envelope-level errors. CipherOcto's `TransportError` (existing in `octo-transport/src/sender.rs`) covers network-level; per-payload errors live in the response envelope body.

## Recommendations

### Adopt (ground in CipherOcto's existing substrate)

| Pattern | Source | CipherOcto substrate |
|---|---|---|
| Typed message envelope with discriminator | Cosmos SDK msg router, libp2p, gRPC | `NodeEnvelope` with `PayloadKindId([u8; 16])` |
| DID-bound identity | W3C DID Core | `did:octo:z<base58btc>` per RFC-0010; `WireDid` type |
| Capability as authorization | RFC-0957 (existing) | `Authorization::Capability(token)` |
| Caveat discriminator byte | RFC-0965 (existing) | Reused for `PayloadKindId` (16 bytes) + `Authorization::Raw` discriminator (16 bytes) |
| Attenuation invariant | RFC-0957 §3.5 (existing) | Capabilities presented as auth are subject to attenuation rules |
| TTL millisecond resolution | RFC-0970 §TV11 (existing) | `expires_at_unix_ms: u64` |
| Multi-mechanism authorization composition | E capabilities | `Vec<Authorization>` for composition (capability + signature for high-value transitions) |
| Per-extension crate model | Polkadot parachains, Cosmos SDK modules | Layer E plugin space |

### Reject (parallel/conflicting with existing substrate)

| Pattern | Source | Why reject |
|---|---|---|
| Central `NodeType` enum | (naive) | Constrains future nodes; use DID self-declaration via `RouterAnnouncePayload` |
| Central `PayloadKind` enum | (naive) | Constrains to 256 kinds; use 128-bit UUID + raw escape hatch (RFC-0965 pattern) |
| Central `Authorization` enum without escape | (naive) | Same constraint; add `Authorization::Raw` escape hatch |
| New transport crate | (would create parallel) | `octo-transport` already provides `NodeTransport` |
| String-based protocol IDs | libp2p, DIDComm | UUIDs are more rigorous; W3C registration overhead not needed at MVP |
| Typed XCM-style instruction enum | Polkadot XCM | Upgrade-hostile; UUIDs allow independent node deployment |
| Method selector from signature hash | Ethereum 4-byte | Too small (256 kinds); UUIDs scale to infinity |
| Wallet depends on storage | (current `PreSharedKeyVerifier` proposal) | Direct storage coupling is weak; wallet asks nodes, nodes answer |

### Design principles

1. **No central enums for extension-bearing types.** All extension surface uses 128-bit UUIDs.
2. **Composition over enumeration.** `Vec<Authorization>` not `Authorization` enum-only.
3. **DID-first.** All identity uses `did:octo:z<base58btc>` per RFC-0010.
4. **Capability as authorization.** RFC-0957 tokens are first-class auth mechanisms.
5. **Wallet as node, not storage.** Wallet doesn't import `quota-router-storage`. Asks nodes via envelope.
6. **Per-extension crates.** New capability types = new crates (Layer E plugin space).

## Next Steps

1. File RFC-0871 (Specialized Node Protocol Envelope) at `rfcs/draft/networking/0871-specialized-node-protocol-envelope.md` (after this research informs the design).
2. Amend RFC-0010 to add a "Wallet Audience Validation" requirement: `AudienceId::from_str` MUST validate via `octo_ident::CanonicalCodec::parse()`.
3. Amend RFC-0870 §Cross-Reference to note wallet follows the same specialized-node pattern.
4. After RFC-0871 acceptance: file top-level mission `0871-specialized-node-protocol-envelope.md` + sub-missions per the decomposition in the RFC §Implementation Phases.

## Open Questions for the RFC Author

1. **Envelope serialization:** borsh (deterministic, fast) vs canonical JSON (debuggable, RFC-0126 already exists). **Recommendation:** borsh for wire, canonical JSON for debug logs.
2. **Routing:** direct-addressed only vs gossip-broadcast for state transitions. **Recommendation:** both, with `RecipientRef` enum (existing enum OK since it's about addressing, not about kinds).
3. **TTL granularity:** milliseconds (RFC-0970 §TV11) — confirmed.
4. **Replay window:** per-(sender, node_type) bounded by TTL — confirmed by RFC-0959 pattern.
5. **Cross-domain trust:** each node type has its own trust root (issuer pubkey). For wallet signing: trust root = wallet's Ed25519 pubkey.
6. **Backward compat:** new payload_kind UUID = additive. Old nodes fail-closed on unknown kinds (RFC-0965 §3.2 pattern). Confirmed.
7. **Quota for queries:** wallet may pay per query OR queries are free (gossip-funded). Implement `PaymentCaveat` as a new caveat type (RFC-0965 reserved range 0x1A-0xCF); each specialized node declares its own pricing policy in `RouterAnnouncePayload`.

## References

### CipherOcto's own RFCs

- RFC-0009 — Identity substrate, DID format
- RFC-0010 — Canonical DID codec (octo-ident crate)
- RFC-0010 v1.1 — Wallet Audience validation gap (PLANNED amendment)
- RFC-0126 — Canonical serialization
- RFC-0853 — BLAKE3 + channel binding
- RFC-0862 — Atomic transaction + gossip
- RFC-0870 — Distributed quota router network (reference specialized node)
- RFC-0903 — Virtual API key system
- RFC-0957 — Capability token format (macaroon)
- RFC-0957-A1 — HolderRegistry (capability substrate)
- RFC-0959 — Ask settlement chain (MicroOCTO_W)
- RFC-0959-A1 — Market delivery envelope
- RFC-0964 — Constraint encoding
- RFC-0965 — Capability extension format (caveat discriminator pattern)
- RFC-0968 — Reputation persistence
- RFC-0969 — Dual pipeline authorization
- RFC-0970 — Forwarding-hop auth (TTL millisecond)
- RFC-0971 — Destination-node role consolidation

### External sources surveyed

- Ethereum contract ABI specification — method selector via first 4 bytes of `keccak256(signature)`
- Solana program instruction format — Anchor IDL discriminator from `sha256("global:<name>")[..8]`
- Cosmos SDK module pattern — `Msg` interface with `Type()` returning string
- IBC protocol spec — packet structure with source/destination channels + ack/timeout
- Polkadot XCM v3 — instruction enum (`Xcm::ReserveAssetDeposited`, etc.)
- libp2p request-response protocol — `/proto-name/version` IDs + multistream-select
- W3C DID Core 1.0 — `did:method:identifier` + DID Resolution
- W3C DIDComm v2 — `https://didcomm.org/...` message types
- Cap'n Proto capability model — unforgeable references, attenuation via parent cap
- E language capabilities — capability lists, `sealed` patterns
- gRPC service definitions — typed methods + streaming
- IPFS DAG-PB — typed content addressing (relevant for envelope_id content addressing)

### CipherOcto substrate surveyed

- `crates/octo-wallet/src/hsm.rs` — `HsmAdapter` trait + `InMemorySigner` + `LedgerSigner` impls
- `crates/octo-wallet/src/identity.rs` — `IdentityKey` (Ed25519), `CapabilityKey` (HKDF-derived), `AudienceId` (DID string — gap to validate)
- `crates/octo-wallet/src/key_hierarchy.rs` — `KeyHierarchy`, `MissionKey`, `AxisSubkey`
- `crates/octo-wallet/src/keystore.rs` — Starkli-compatible keystore
- `crates/octo-wallet/src/vault.rs` + `vault_rotation.rs` — vault encryption + rotation
- `crates/octo-wallet/src/mpc.rs` — Multi-party computation substrate
- `crates/octo-wallet/src/capability/macaroon.rs` — `CapabilityCatalog`, macaroon substrate (1905 lines)
- `crates/octo-wallet/src/capability/caveat.rs` — `Caveat` enum + `RawCaveat` escape hatch (1382 lines)
- `crates/octo-wallet/src/capability/verify.rs` — `VerifyContext`, `VerifiedToken`, `VerifyError` (224 lines)
- `crates/octo-wallet/src/capability/gossip.rs` — `CapabilityGossip` (NodeTransport integration)
- `crates/octo-ident/src/lib.rs` — `DidCodec` trait + `WireDid` + `RawDid` + `LegacyWire`
- `crates/octo-transport/src/sender.rs` — `NetworkSender` trait + `SendContext` + `TransportError`
- `crates/octo-transport/src/receiver.rs` — `NetworkReceiver` trait + `ReceiveContext`
- `crates/octo-transport/src/node_transport.rs` — `NodeTransport` struct (broadcast, send_best, dispatch)
- `crates/octo-cable/src/{ble,noise,tunnel,handshake,ctap2,framing,discovery,base10,assert}.rs` — cable transport stack
- `crates/quota-router-core/src/node/mod.rs` — `QuotaRouterNode` (1400 lines, reference impl)
- `crates/quota-router-core/src/proxy.rs::handle_request` — inline Bearer strip (L697-711) — orphan `GatewayAuthenticator` substrate in `octo-wallet/src/capability/gateway_authenticator.rs`

## Conclusion

The right design is **a typed-message envelope using existing CipherOcto substrate**: 128-bit UUID discriminator (RFC-0965 caveat pattern), DID-bound identity (RFC-0010), `Vec<Authorization>` composition (capability IS one), millisecond TTL (RFC-0970). Per-extension crate model for capability types. Wallet as specialized node via existing `NodeTransport`. No central enums; no parallel transport; no wallet-to-storage coupling. RFC-0871 to specify.