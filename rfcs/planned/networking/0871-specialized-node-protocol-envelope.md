# RFC-0871 (PLANNED): Specialized Node Protocol Envelope

> **Status:** Planned (placeholder for future work; defines concept and scope).
> **Author:** @cipherocto + @mmacedoeu
> **Input research:** `docs/research/2026-08-08-specialized-node-protocol-research.md`
> **Input use case:** `docs/use-cases/wallet-as-specialized-node.md`

## Status

Planned — placeholders for full RFC. This file captures the concept + scope + dependency graph + design goals. When the RFC is ready for review, convert to `rfcs/draft/networking/0871-specialized-node-protocol-envelope.md` with full §Specification, §Data Structures, §Algorithms, §Test Vectors, §Adversary Analysis.

## Authors

- Author: @cipherocto
- Contributor: @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu

## Summary

Define a uniform typed-message envelope (`NodeEnvelope`) that any CipherOcto specialized node — quota router, identity resolver, reputation anchor, capability issuer, market node, **wallet**, future governance node, future oracle node — uses to communicate over the existing `octo-transport::NodeTransport` mesh. The envelope is DID-bound (`from_did: did:octo:z<base58btc>` per RFC-0010), typed via a 128-bit `payload_kind` UUID (RFC-0965 caveat discriminator pattern, 16 bytes instead of 1), authorized via `Vec<Authorization>` (signature / capability token / ZK proof / threshold signature / raw escape hatch), and protected against replay via `envelope_id` + `nonce` + `expires_at_unix_ms`.

The envelope replaces ad-hoc per-node wire formats with a single canonical shape. New node types and new payload kinds are allocated via RFC without modifying existing code. The wallet becomes a first-class network participant — capable of being deployed on a hardware device (Ledger, YubiHSM) communicating via BLE / USB / in-process transport.

## Dependencies

**Requires:**

- RFC-0009 (Accepted) — Identity substrate, DID format
- RFC-0010 (Accepted) — Canonical DID codec, `crates/octo-ident::DidCodec` trait
- RFC-0126 (Accepted) — Canonical serialization
- RFC-0853 (Accepted) — BLAKE3 primitive source
- RFC-0862 (Accepted) — Atomic transaction + gossip substrate
- RFC-0870 (Accepted) — Distributed quota router network (reference specialized node pattern)
- RFC-0957 (Accepted) — Capability token format (macaroon)
- RFC-0957-A1 (Accepted) — `HolderRegistry` (capability substrate)
- RFC-0959 (Accepted) — Ask settlement chain (MicroOCTO_W)
- RFC-0964 (Accepted) — Constraint encoding
- RFC-0965 (Accepted) — Capability extension format (caveat discriminator pattern)
- RFC-0970 (Accepted) — Forwarding-hop auth (TTL millisecond resolution)

**Optional (cross-references):**

- RFC-0903 (Final) — Virtual API key system (bearer path coexists)
- RFC-0958 (Accepted) — ZK capability subclass (authorization via ZK proof)
- RFC-0968 (Accepted) — Reputation persistence (reputation node follows this pattern)
- RFC-0969 (Accepted) — Dual pipeline authorization (gateway authenticator discussion)
- RFC-0971 (Accepted) — Destination-node role consolidation

> **Dependency Validation Rules:**
> 1. Dependencies MUST form a DAG (no cycles): ✓ (all upstream Accepted)
> 2. All "Requires" RFCs MUST be listed as mission prerequisites (Mission creation in S5 enforces this)
> 3. Optional dependencies MUST be documented separately from required: ✓
> 4. Dependencies on "Planned" RFCs MUST note the assumption they will be Accepted: N/A (none)

## Design Goals

| Goal | Target | Metric |
| --- | --- | --- |
| G1 | No central `NodeType` enum constraining future nodes | Adding a new node type = new RFC + new `payload_kind` UUIDs + new `NetworkReceiver` impl. No `octo-transport` changes. |
| G2 | No central `PayloadKind` enum constraining payload kinds | 128-bit `PayloadKindId` UUID, RFC-allocated ranges + Raw escape hatch |
| G3 | No central `Authorization` enum without escape hatch | `Authorization` enum has `Raw { discriminator: [u8;16], body: Vec<u8> }` variant for future types |
| G4 | All identities use canonical DID | 100% of `from_did` fields validated via `octo_ident::CanonicalCodec::parse()` |
| G5 | Capability IS an authorization | `Authorization::Capability(token)` variant; composition via `Vec<Authorization>` |
| G6 | Replay defense + TTL | `envelope_id` uniqueness + `nonce` per sender + `expires_at_unix_ms` TTL (millisecond) |
| G7 | Per-extension crate model | New capability type = own crate, registers via plugin. Wallet core unchanged. |
| G8 | Wallet as specialized node | Wallet signs via `HsmAdapter`. `IdentityKey::sign()` routes through HSM. Hardware wallets supported. |
| G9 | Wallet depends on storage = 0 | Wallet asks nodes via envelope. Node answers. No `quota-router-storage` import in `octo-wallet`. |
| G10 | Backward compatibility | Adding payload_kind UUID = additive. Old nodes fail-closed on unknown kinds (RFC-0965 §3.2). |

## Motivation

CipherOcto's mesh today has one specialized node type: the Quota Router (RFC-0870). Identity resolution, reputation anchoring, capability issuance, and market settlement each have RFCs but no shared envelope — each would re-invent its own wire format.

Concrete gap: `GatewayAuthenticator` at `crates/octo-wallet/src/capability/gateway_authenticator.rs` (668 lines) is orphan substrate with no production caller. `quota-router-core/src/proxy.rs::handle_request` (L697-711) does its own inline Bearer strip. The two are unrelated — no shared envelope.

Concrete gap: `IdentityKey::sign()` at `crates/octo-wallet/src/identity.rs:71` calls `ed25519_dalek::SigningKey::from_bytes(...).sign(...)` directly — bypassing the `HsmAdapter` trait at `crates/octo-wallet/src/hsm.rs:33`. Hardware wallets (`LedgerSigner`) cannot sign capability tokens today.

Concrete gap: `AudienceId::from_str` at `crates/octo-wallet/src/identity.rs:127+` accepts any non-empty string (per RFC-0010 critique). Wallet DOES NOT validate canonical DID format. RFC-0010 already ratifies the codec; wallet integration is missing.

These gaps are symptoms of a missing layer: the **specialized node protocol envelope**. This RFC defines it.

## Roles and Authorities

### Role/Authority Coverage Table

| Role | Identifier | Authority Scope | Lifecycle | Source/Ref |
| --- | --- | --- | --- | --- |
| Wallet holder | `IdentityKey` (Ed25519) | sign payloads, mint capabilities, attenuate capabilities | persistent across rotations | RFC-0009 §Identity |
| Wallet device | `HsmAdapter` impl | private-key custody (in-memory / Ledger / YubiHSM / TEE) | device-bound | RFC-0853 §F2 |
| Specialized node | `NodeEnvelope` handler | dispatch payload_kind → business logic | bound to node deployment | RFC-0870 (reference) |
| Capability issuer | `CapabilityToken::mint` | issue capabilities with caveats | persistent (capabilities outlive issuance) | RFC-0957-A1 |
| Mesh | `NodeTransport` | route envelopes to peers, dispatch to receivers | per-deployment | octo-transport |
| Quota router | `QuotaRouterNode` | forward requests, settle inference | bound to node | RFC-0870 |
| Identity resolver | `IdentityResolver` (NEW) | resolve DID → DID Document | stateless function | RFC-0009 |
| Reputation anchor | `ReputationAnchorNode` (NEW) | anchor reputation batches, gossip events | bound to node | RFC-0968 |
| Wallet node | `WalletNode` (NEW) | sign payloads, mint capabilities, advertise crypto services | bound to node | This RFC |

### Stateful actors

- `WalletNode` — owns `IdentityKey` + `HsmAdapter`, registers `NetworkReceiver` impl, advertises payload kinds via `RouterAnnouncePayload`
- `QuotaRouterNode` — existing (RFC-0870); extended to use the new envelope
- `IdentityResolverNode` — new; stateless resolver that responds to `IdentityResolveDid` payload
- `ReputationAnchorNode` — new; substrate in `crates/octo-reputation/`, network-wired via envelope

## Specification (sketch — full §Specification in Draft RFC)

### System Architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│ Layer 0 — Crypto primitives (decades-stable)                         │
│   ed25519-dalek, x25519-dalek, blake3, hkdf, chacha20-poly1305        │
└──────────────────────────────────────────────────────────────────────┘
                                ↓
┌──────────────────────────────────────────────────────────────────────┐
│ Layer 1 — Years-stable core                                          │
│   octo-ident (DID codec), octo-transport (NodeTransport),            │
│   cipherocto-encoding, octo-cable (BLE/Noise/Tunnel)                 │
│   octo-wallet (identity, hsm, capability types core)                 │
└──────────────────────────────────────────────────────────────────────┘
                                ↓
┌──────────────────────────────────────────────────────────────────────┐
│ Layer 2 — Specialized nodes (per-RFC)                                │
│   octo-quota-router (RFC-0870), octo-identity-resolver (NEW),        │
│   octo-reputation-anchor (RFC-0968), octo-issuer (RFC-0957-A1),      │
│   octo-market (RFC-0959-A1), octo-wallet-node (NEW)                  │
└──────────────────────────────────────────────────────────────────────┘
                                ↓
┌──────────────────────────────────────────────────────────────────────┐
│ Layer 3 — Adapters per protocol (each adapter = own crate)           │
│   crates/octo-adapter-{tcp,quic,udp,bluetooth,webrtc,usb,...}        │
└──────────────────────────────────────────────────────────────────────┘
                                ↓
┌──────────────────────────────────────────────────────────────────────┐
│ Layer 4 — Capability extensions (each extension = own crate)         │
│   crates/octo-cap-{macaroon,zk,federation,time-lock,threshold-mpc}   │
│   crates/octo-cap-<user-extension>/                                  │
└──────────────────────────────────────────────────────────────────────┘
```

### Data Structures (sketch)

```rust
// crates/octo-protocol/src/envelope.rs (NEW crate, Layer 1 stable)

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeEnvelope {
    /// BLAKE3-256 hash of canonical_ser(all other fields). Replay defense.
    pub envelope_id: [u8; 32],
    /// Sender canonical DID. Validated via octo_ident::CanonicalCodec::parse().
    pub from_did: WireDid,
    /// Recipient reference.
    pub to_node_id: RecipientRef,
    /// Payload discriminator (128-bit UUID, RFC-allocated).
    pub payload_kind: PayloadKindId,
    /// Borsh-encoded payload body.
    pub payload: Vec<u8>,
    /// Authorization(s). Capability IS one.
    pub authorization: Vec<Authorization>,
    /// Per-sender unique nonce.
    pub nonce: [u8; 32],
    /// TTL in unix milliseconds (RFC-0970 §TV11).
    pub expires_at_unix_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RecipientRef {
    Direct([u8; 32]),       // specific node id
    Domain(WireDid),        // any node serving a given DID's domain
    Broadcast,              // mesh-broadcast
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PayloadKindId([u8; 16]);  // 128-bit UUID

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Authorization {
    /// Ed25519 signature over (envelope_id, from_did, payload).
    Signature { signer_did: WireDid, sig: Ed25519Signature },
    /// RFC-0957 capability token. Caveats gate what payload can do.
    Capability(CapabilityToken),
    /// RFC-0958 ZK proof bundle.
    Proof(ProofBundle),
    /// Threshold signature (BLS or equivalent).
    ThresholdSignature { signers: Vec<WireDid>, sig: BlsSignature },
    /// Escape hatch for unknown future authorization types.
    Raw { discriminator: [u8; 16], body: Vec<u8> },
}
```

### Algorithms (sketch)

**Envelope send:**
1. Build envelope with `from_did = wallet.canonical_did()`
2. Compute `envelope_id = BLAKE3-256(canonical_ser(envelope_without_id))`
3. Sign envelope via `HsmAdapter::sign(envelope_id || from_did || payload)` → `Authorization::Signature`
4. Set `nonce = wallet.next_nonce()`
5. Set `expires_at_unix_ms = current_unix_ms + TTL`
6. Send via `NodeTransport::send_best(envelope_bytes, &send_context)`

**Envelope receive (node-side):**
1. Receive via `NodeTransport::dispatch()` → registered `NetworkReceiver`
2. Validate `from_did` shape via `octo_ident::CanonicalCodec::parse()`
3. Check `envelope_id` uniqueness (replay defense)
4. Check `nonce` per-(sender, node_type) uniqueness within TTL
5. Check `expires_at_unix_ms > current_unix_ms`
6. For each `Authorization` in `Vec`, dispatch verifier:
   - `Signature` → `ed25519-dalek::verify(signer_did, ...)`
   - `Capability` → RFC-0957 verify (caveat check + attenuation invariant)
   - `Proof` → RFC-0958 verify
   - `ThresholdSignature` → BLS verify
   - `Raw` → dispatch to registered raw handler (RFC-allocated)
7. If all verifications pass, dispatch payload to `payload_kind` handler

### Lifecycle Requirements

#### Specialized Node Lifecycle

```rust
// crates/octo-protocol/src/node.rs (NEW crate)

#[async_trait]
pub trait SpecializedNode: Send + Sync {
    /// Node's DID (canonical did:octo:z<base58btc>).
    fn node_did(&self) -> &WireDid;
    /// Payload kinds this node serves.
    fn served_payload_kinds(&self) -> &[PayloadKindId];
    /// Build a RouterAnnouncePayload for mesh gossip.
    fn announce(&self) -> RouterAnnouncePayload;
    /// Handle an incoming payload.
    async fn handle_payload(
        &self,
        envelope: &NodeEnvelope,
        payload: &[u8],
    ) -> Result<NodeEnvelope, ProtocolError>;
}
```

#### Wallet Node Lifecycle

```rust
pub struct WalletNode {
    pub identity: Arc<IdentityKey>,
    pub signer: Arc<HsmAdapter>,
    pub transport: Arc<NodeTransport>,
    pub announce_payload: RouterAnnouncePayload,
}

impl WalletNode {
    /// Register as a receiver on the transport.
    pub async fn start(&self) -> Result<(), ProtocolError> {
        self.transport.register_receiver(Arc::new(self.clone()));
        Ok(())
    }
}

#[async_trait]
impl NetworkReceiver for WalletNode {
    async fn on_receive(
        &self,
        payload: &[u8],
        context: &ReceiveContext,
    ) -> Result<(), TransportError> {
        let envelope: NodeEnvelope = borsh::from_slice(payload)?;
        // Validate DID, nonce, TTL
        // Verify authorization (signature over envelope_id, from_did, payload)
        // Dispatch to payload_kind handler
        match envelope.payload_kind {
            WALLET_SIGN_ED25519 => self.handle_sign(envelope).await,
            WALLET_MINT_CAPABILITY => self.handle_mint(envelope).await,
            WALLET_ATTENUATE_CAPABILITY => self.handle_attenuate(envelope).await,
            WALLET_RESOLVE_DID => self.handle_resolve(envelope).await,
            _ => Err(ProtocolError::UnknownPayloadKind),
        }
    }
}
```

### Determinism Requirements

**Class A (protocol-level):** Envelope serialization is canonical via borsh. `envelope_id` is BLAKE3-256 of canonical_ser. Both must be byte-exact across independent implementations.

**Class B (off-chain):** Mesh routing, gossip ordering, transport adapter choice. May vary across nodes; consensus only requires eventual delivery.

**Class C (probabilistic):** `nonce` uniqueness assumes sender behaves correctly; replay defense requires per-(sender, node_type) cache.

### RFC-0008 Execution Class Mapping

| Component | Class | Reason |
| --- | --- | --- |
| Envelope serialization | A | Canonical borsh, BLAKE3 hash |
| `envelope_id` computation | A | BLAKE3 deterministic |
| DID parsing | A | RFC-0010 canonical |
| Signature verification | A | ed25519-dalek deterministic |
| Capability verification | A | RFC-0957 attenuation invariant deterministic |
| Mesh routing | B | Adapter-dependent |
| Replay defense | C | Sender behavior |

### Error Handling

```rust
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("invalid DID shape: {0}")]
    InvalidDid(#[from] DidError),
    #[error("unknown payload kind: {0:?}")]
    UnknownPayloadKind(PayloadKindId),
    #[error("replay detected: envelope_id={0:?}")]
    ReplayDetected([u8; 32]),
    #[error("nonce reuse: sender={0:?}, nonce={1:?}")]
    NonceReuse(WireDid, [u8; 32]),
    #[error("expired: expired_at_unix_ms={1}, now={0}")]
    Expired(u64, u64),
    #[error("authorization failed: {0}")]
    AuthorizationFailed(String),
    #[error("unknown authorization discriminator: {0:?}")]
    UnknownAuthDiscriminator([u8; 16]),
}
```

## Performance Targets

| Operation | Target | Measurement |
| --- | --- | --- |
| Envelope serialize (borsh) | < 100 µs for typical 1KB envelope | Bench `serialize_envelope` |
| DID parse | < 10 µs | Bench `wire_to_raw` |
| Signature verify (Ed25519) | < 50 µs | Bench `ed25519_dalek::verify` |
| Envelope hash (BLAKE3-256 of 1KB) | < 5 µs | Bench `blake3::hash` |
| Capability verify (RFC-0957) | < 1 ms (per RFC-0957 G1) | Bench `verify_capability_token` |
| End-to-end wallet sign request via mesh | < 100 ms (1-hop local) | Integration test |

## Implicit Assumptions Audit

| Assumption | Where Relied Upon | Blast Radius if False | Mitigation / Status |
| --- | --- | --- | --- |
| `octo-ident` is Layer 1 (decades-stable) | All `from_did` fields validated via `octo_ident::CanonicalCodec` | If octo-ident changes shape, all envelopes break | RFC-0010 locks the codec; Layer 1 freeze |
| `octo-transport::NodeTransport` continues to provide fan-out + fan-in | Envelope routing | If transport semantics change, envelope reception breaks | octo-transport is Layer 1; RFC-0870 already depends on it |
| Ed25519 remains the canonical signing curve | All signature auth | Curve migration is breaking | RFC-0853 §F1 lists PQC migration as future work |
| RFC-0957 attenuation invariant holds | `Authorization::Capability` verification | If invariant breaks, capabilities can be weakened | RFC-0957 §3.5 is type-level enforced |
| Wallet has access to a clock for TTL checks | `expires_at_unix_ms` enforcement | If clock skew, false accept/reject | RFC-0970 §11.5 handles skew tolerance |
| Mesh routing is best-effort | `RecipientRef::Broadcast` | If mesh partition, envelopes may not arrive | Existing gossip retry (RFC-0862) |

### Categories to Audit (per BLUEPRINT §Implicit Assumptions Audit)

- **Schema stability** — envelope wire format must be canonical across implementations.
- **Upgrade safety** — new payload_kind = additive (RFC-allocated UUID + Raw escape hatch).
- **Rollback safety** — old nodes fail-closed on unknown kinds (RFC-0965 §3.2 pattern).
- **Mixed-version** — old nodes reject new envelope variants; new nodes reject old auth discriminators they don't know.
- **External dependency** — depends on `octo-ident` + `octo-transport` + `ed25519-dalek` + `blake3`.

## Security Considerations

| Concern | Mitigation |
| --- | --- |
| Replay attack | `envelope_id` uniqueness + `nonce` per-(sender, node_type) cache + TTL |
| Signature forgery | Ed25519 verification per signature; RFC-0970 §Crypto substrate |
| Capability weakening | RFC-0957 §3.5 attenuation invariant type-level enforced |
| Cross-domain trust | Each node type declares own trust root via `RouterAnnouncePayload` |
| TTL skew | Tolerance window per RFC-0970 §11.5 |
| DID spoofing | `octo_ident::CanonicalCodec::parse()` validates canonical wire form |
| Unknown payload kind | RFC-0965 §3.2 fail-closed pattern (reject unknown kinds, don't silently drop) |
| Unknown auth discriminator | `Authorization::Raw` allows forward-compat; fail-closed if no handler registered |

## Adversary Analysis (sketch)

**A1 — Replay attack on signed envelope:**
- Adversary captures `NodeEnvelope` with valid signature + non-expired TTL
- Replays it to the same node
- Defense: `envelope_id` uniqueness cache + `nonce` per-sender cache

**A2 — Capability forgery:**
- Adversary mints `CapabilityToken` without being the issuer
- Defense: RFC-0957 §3.5 invariant + Ed25519 sig on token mint

**A3 — Cross-domain trust escalation:**
- Adversary sends envelope from identity node to quota node with reputation-tier payload
- Defense: `RouterAnnouncePayload` declares trust root per node type

**A4 — TTL manipulation:**
- Adversary sets `expires_at_unix_ms` far in the future
- Defense: per-node-type TTL ceiling (declared in `RouterAnnouncePayload`)

**A5 — Payload kind spoofing (unknown kind handler):**
- Adversary sends envelope with unknown `PayloadKindId`
- Defense: fail-closed rejection; old nodes reject unknown kinds (RFC-0965 pattern)

**A6 — Authorization composition attack:**
- Adversary sends envelope with `Vec<Authorization>` mixing valid + invalid
- Defense: ALL authorizations must verify (logical AND, not OR)

**A7 — DID spoofing via legacy form:**
- Adversary uses `did:octo:b<base32>` (legacy) post-deprecation
- Defense: `octo_ident::CanonicalCodec::parse(s, false)` rejects legacy post-window

## Compatibility

- **Forward:** new payload_kind UUID = additive; old nodes reject unknown kinds (safe).
- **Backward:** old envelope format (none exists yet) → N/A.
- **Cross-version:** mixed-version mesh works as long as both versions know each other's payload kinds (RFC-0965 fail-closed pattern).

## Test Vectors (sketch)

- TV1: Self-sign envelope with HSM-backed IdentityKey, verify edge case
- TV2: Receiver rejects expired envelope
- TV3: Receiver rejects replayed envelope
- TV4: Receiver accepts envelope with `Vec<Authorization>` containing capability + signature
- TV5: Wallet node announces payload kinds via `RouterAnnouncePayload`
- TV6: Hardware wallet (LedgerSigner) signs capability mint request
- TV7: Cross-domain envelope (identity resolve from quota node context)
- TV8: Borsh serialization byte-exact across two independent implementations

## Alternatives Considered

1. **Central `NodeType` enum** — REJECTED. Upgrade-hostile. Adds churn on every new node.
2. **Central `PayloadKind` enum (Rust)** — REJECTED. Same constraint.
3. **Central `Authorization` enum without Raw** — REJECTED. Same constraint; can't add new auth types.
4. **String-based protocol IDs (libp2p / DIDComm)** — REJECTED. UUIDs are more rigorous; W3C registration overhead not needed at MVP.
5. **XCM-style typed instruction enum (Polkadot)** — REJECTED. Upgrade-hostile.
6. **4-byte method selector (Ethereum ABI)** — REJECTED. Too small (256 kinds exhausting).
7. **Method selector from signature hash (Ethereum)** — REJECTED. Same constraint.
8. **gRPC + protobuf service definitions** — REJECTED for primary, accepted as inspiration. UUIDs replace string method names.

## Implementation Phases

### Phase 1 — Core envelope (Layer 1 stable)
- `crates/octo-protocol/` new crate
- `NodeEnvelope`, `PayloadKindId`, `Authorization`, `RecipientRef`, `ProtocolError`
- borsh serde impls + BLAKE3 envelope_id computation
- DID validation via `octo_ident::CanonicalCodec::parse()`

### Phase 2 — Wallet node (Layer 2 specialized)
- `crates/octo-wallet-node/` new crate (or fold into `octo-wallet`)
- `WalletNode` struct + `NetworkReceiver` impl
- `IdentityKey::sign()` routes through `HsmAdapter` (close the gap)
- Wallet payload kinds: `WALLET_SIGN_ED25519`, `WALLET_MINT_CAPABILITY`, `WALLET_ATTENUATE_CAPABILITY`, `WALLET_RESOLVE_DID`

### Phase 3 — Specialized node adoption (per-RFC)
- Quota router adopts envelope (RFC-0870 amendment)
- Identity resolver node (NEW)
- Reputation anchor node (RFC-0968 wiring)
- Capability issuer node (RFC-0957-A1 extension)

### Phase 4 — Per-extension crate extraction
- `crates/octo-cap-macaroon/` from `crates/octo-wallet/src/capability/macaroon.rs`
- `crates/octo-cap-zk/` from `crates/octo-wallet/src/capability/zk_mint.rs`
- `crates/octo-cap-federation/` from `crates/octo-wallet/src/capability/federation.rs`
- Each crate registers `CapabilitySpec` via plugin

### Phase 5 — Paid query (caveat composition)
- New caveat type `PaymentCaveat` (RFC-0965 reserved range 0x1A-0xCF)
- `RouterAnnouncePayload` declares pricing policy per payload_kind
- Wallet authorization = `Authorization::Capability(token)` with `PaymentCaveat`

## Key Files to Modify

- **NEW** `crates/octo-protocol/` — envelope types + dispatch
- **AMEND** `crates/octo-wallet/Cargo.toml` — add `octo-protocol` dep
- **AMEND** `crates/octo-wallet/src/identity.rs` — `IdentityKey::sign` routes through `HsmAdapter`
- **AMEND** `crates/octo-wallet/src/identity.rs` — `AudienceId::from_str` validates via `CanonicalCodec`
- **AMEND** `crates/octo-wallet/src/capability/macaroon.rs` — extract to `octo-cap-macaroon` crate (Phase 4)
- **AMEND** `crates/quota-router-core/src/node/mod.rs` — quota node uses envelope
- **AMEND** `crates/quota-router-core/src/proxy.rs::handle_request` — gateway authenticator via envelope

## Future Work

- ZK-authorized requests (RFC-0958 + envelope `Authorization::Proof`)
- Threshold-MPC for high-value transitions (BLS via `Authorization::ThresholdSignature`)
- Cross-chain anchoring (specialized node ↔ external chain)
- DID method registration with W3C (RFC-0009 §IA-4)
- PQC identity substrate (RFC-0853 §F1)
- Capability delegation chains (wallet → issuer → downstream capability holder)
- Paid query subscription model (pre-paid capacity, drain over time)
- Cross-domain DID resolution (resolver chains across specialized nodes)
- Mesh routing QoS (per-payload-kind priority in `SendContext`)

## Rationale

### Why no central enums

CipherOcto's RFC-0965 already established the precedent: caveat types use a 1-byte discriminator with reserved ranges for future extensions. This RFC generalizes that pattern to 16 bytes (`PayloadKindId`) for payload kinds and `Authorization::Raw` discriminator for authorization mechanisms. **New types = new RFC, not new code change in core crates.**

### Why per-extension crates

Per-RFC-0965 + RFC-0957 + RFC-0967, the capability substrate is wide-cross-cutting with infinite business scenarios. Stuffing all capability types into `crates/octo-wallet/src/capability/macaroon.rs` (1905 lines) is unsustainable. Per-extension crates (Layer 4) keep core stable.

### Why wallet as specialized node

Hardware wallets exist. They have keys, screens, buttons, transports. Treating them as "substrate called by another process" is dishonest about their nature. They ARE network nodes; the host is just one peer. The protocol envelope lets a hardware wallet on a phone (BLE) or desktop (USB HID) join the mesh directly.

### Why no wallet-to-storage dependency

`PreSharedKeyVerifier { storage: Arc<dyn KeyStorage> }` (proposed earlier) is weak design: any business rule added to verification (master key bypass, rate limit pre-check, team-based permissions) would need replication. The wallet is a pure crypto primitive; business rules live in the node-side handler. Wallet asks via envelope; node answers.

### Why `Vec<Authorization>`

Single-mechanism authorization (signature OR capability OR proof) doesn't compose. Real workflows need: "wallet presents capability token (proves authority to call this payload kind) + signature (proves freshness)". `Vec<Authorization>` with logical-AND semantics is the cleanest composition.

## Version History

| Version | Date | Author | Note |
| --- | --- | --- | --- |
| v0.1 | 2026-08-08 | @cipherocto + @mmacedoeu | Initial planned RFC. Concept + scope + design goals + dependency graph + Alternatives Considered. Full §Specification, §Data Structures, §Algorithms, §Test Vectors, §Adversary Analysis to be expanded when moved to `rfcs/draft/`. |

## Related RFCs

- RFC-0009, RFC-0010 — Identity substrate, DID codec
- RFC-0126 — Canonical serialization
- RFC-0853 — BLAKE3 + channel binding
- RFC-0862 — Atomic transaction + gossip
- RFC-0870 — Distributed quota router network (reference specialized node)
- RFC-0903 — Virtual API key system
- RFC-0957, RFC-0957-A1 — Capability token format + HolderRegistry
- RFC-0958 — ZK capability subclass
- RFC-0959, RFC-0959-A1 — Ask settlement + market delivery
- RFC-0964 — Constraint encoding
- RFC-0965 — Capability extension format (caveat discriminator pattern)
- RFC-0968 — Reputation persistence
- RFC-0969 — Dual pipeline authorization
- RFC-0970 — Forwarding-hop auth (TTL millisecond)
- RFC-0971 — Destination-node role consolidation

## Notes

- This file is **Planned**, not Draft. When ready for review, copy to `rfcs/draft/networking/0871-specialized-node-protocol-envelope.md` and expand §Specification, §Data Structures, §Algorithms, §Test Vectors, §Adversary Analysis per BLUEPRINT §RFC template.
- Multi-round adversarial review expected (estimate: R1-R28 over 4 sessions, similar to RFC-0969/0970/0971 batch).
- Implementation will land as multi-mission decomposition per RFC-0870 (SpecializedNode pattern).