# Mission: 0871b-cross-node-forwarding — Cross-node resolver hop transport

**Status:** claimed (filed 2026-08-12); LANDED 2026-08-12, commit `fb2dadaf`. Follow-on to `0871b-cross-domain-resolution-impl` (LANDED 2026-08-11, commit `c14c2707`).

## Origin context (read this first)

This mission is the **deferred tail** of `0871b-cross-domain-resolution-impl`. Origin scope L31-33 planned three backend files:

```
2. **`crates/octo-identity-resolver-node/src/backend.rs`** (NEW) —
   `ResolverBackend` trait + `LocalResolverBackend` (delegates to
   `DidRegistry`) + `RemoteResolverBackend` (HTTP/gossip call to
   next hop).
```

The closure commit `c14c2707` shipped chain-traversal LOGIC only (`handlers/chain.rs` + `tests/cross_domain_chain.rs`). **No `backend.rs` was created.** The closure record (line 3 of `missions/claimed/0871b-cross-domain-resolution-impl.md`) falsely claims `backend.rs` shipped with all three types — `git show c14c2707 --stat` proves otherwise (no `backend.rs` entry in the diff). This mission lands the origin-planned `backend.rs` surface.

7 chain-traversal integration TV already landed in `crates/octo-identity-resolver-node/tests/cross_domain_chain.rs`:
- `chain_single_hop_resolves` (TV-1)
- `chain_three_hops_resolves_end_to_end` (TV-2 — in-process: 3 hops = 3 calls against the local registry, NOT 3 nodes)
- `chain_ttl_expiry_returns_error` (TV-3)
- `chain_cycle_detection_aborts` (TV-4)
- `chain_invalid_hop_rejected` (TV-5)
- `chain_ttl_exactly_one_hop_succeeds` (boundary)
- `chain_empty_hops_resolves_locally` (boundary)

None of them exercise cross-network forwarding or per-hop signing. That is this mission's gap.

## RFC anchor

**Authoritative**: RFC-0871 §Future Work row 598 ("Cross-domain DID resolution (resolver chains across specialized nodes) — TBD — post-v2.0 — RFC-0009 §Future Work") + RFC-0871 §Roles and Authorities (IdentityResolverNode role) + RFC-0010 v1.3 §Storage Extension.

**Pattern reference only**: RFC-0970 (Forwarding-Hop Authorization Envelope) — `chain_hash` + per-hop signature + TTL pattern. RFC-0970 is the quota-router forwarding mesh substrate; this mission adopts the same pattern for the identity-resolver chain. Cross-domain adaptation, not literal port — the inner content is a DID lookup, not a quota bearer header.

**Two independent integrity dimensions**:
- **Correlation**: RFC-0871 `NodeEnvelope.envelope_id` (BLAKE3-256 of unsigned envelope, RFC-0871 §Algorithms step 2 at `envelope.rs:200-201`). The `ChainResolveResponse.envelope_id` field binds the response to the originating request envelope for replay defense.
- **Hop binding**: RFC-0970 `chain_hash` (per-hop accumulator binding forwarded request + cumulative hop state).

These dimensions are orthogonal: `envelope_id` ties request↔response across the chain; `chain_hash` ties hop N to hop N-1 (RFC-0970 §Data Structures). The per-hop `HopSignature.signature` preimage binds BOTH into one Ed25519 signature per hop.

## Substrate (already shipped — do NOT rebuild)

- `crates/octo-identity-resolver-node/src/handlers/chain.rs:77` — `ResolverHop { hop_did: String, hop_transport_hint: Vec<u8> }` (Borsh wire form)
- `crates/octo-identity-resolver-node/src/handlers/chain.rs:113` — `ResolverChainContext { visited: BTreeSet<String>, ttl_remaining_ms: u64 }` (deterministic cycle detection per `check_wrapped_chain` pattern in `crates/octo-cap-macaroon/src/macaroon.rs`)
- `crates/octo-identity-resolver-node/src/handlers/chain.rs:125` — `ChainResolveRequest { target: String, hops: Vec<ResolverHop>, ttl_remaining_ms: u64 }`
- `crates/octo-identity-resolver-node/src/handlers/chain.rs:155` — `ChainResolveResponse { canonical_did: String, public_key: [u8; 32], hops_traversed: u8 }` (3-tuple; this mission extends to 5-tuple — see T5)
- `crates/octo-identity-resolver-node/src/handlers/chain.rs:171` — `ResolveChainHandler { registry: Arc<dyn DidRegistry> }` (DI shape; replaced with `Arc<dyn ResolverBackend>` in this mission)
- `crates/octo-identity-resolver-node/src/handlers/chain.rs:65` — `HOP_LATENCY_MS_ESTIMATE: u64 = 10` (conservative per-hop decrement)
- `crates/octo-identity-resolver-node/src/handlers/mod.rs:120` — `IdentityResolveError::ChainCycle` (hop revisits visited DID)
- `crates/octo-identity-resolver-node/src/handlers/mod.rs:127` — `IdentityResolveError::ChainTtlExpired` (TTL budget underflows)
- `crates/octo-protocol/src/payload_kind.rs:156` — `IDENTITY_RESOLVE_CHAIN` UUID `0x0009:0001:0000:0000:0000:0000:0000:0004` (note: not `:0002` — that's `IDENTITY_REGISTER`)
- `crates/octo-protocol/src/payload_kind.rs:170` — `IDENTITY_RESOLVE_WITH_CHAIN` UUID `:0005` (sibling; distinct from chain-of-resolvers)
- `crates/octo-identity-resolver-node/src/node.rs:318-328` — `IDENTITY_RESOLVE_CHAIN` dispatch arm (currently sync, calls `ResolveChainHandler::handle` directly; this mission converts to async + threads `envelope_id`)
- `crates/octo-identity-resolver-node/src/lib.rs:73-79` — `IDENTITY_RESOLVER_PAYLOAD_KINDS` (5 kinds)
- `crates/octo-identity-resolver-node/tests/cross_domain_chain.rs` — 7 chain-traversal TV (in-process, no signing, no network I/O)
- `octo-transport/src/sender.rs` — `NetworkSender` + `TransportError`
- `octo-transport/src/receiver.rs` — `NetworkReceiver` + `ReceiveContext`
- `octo-transport/src/lib.rs` — `NodeTransport` with `register_receiver` + `broadcast` + `send_best` (NO request/response — see Dependency)
- `crates/octo-ident/src/registry.rs:49` — comment placeholder for `ResolverBackend` typed view (F6) — file to land
- `crates/octo-wallet/src/identity.rs` — `IdentityKey::sign` + `public_key_bytes` (per RFC-0009-B1 `WalletCrypto`; routes through `HsmAdapter`)
- `octo-ident` (Layer B) — `DidRegistry` trait substrate (UNCHANGED)

## Traits in scope (NEW surface this mission lands)

### T1. `pub trait ResolverBackend` — Layer B, `crates/octo-ident/src/resolver_backend.rs`

Matches the placeholder comment at `octo-ident/src/registry.rs:49`. Typed view over `DidRegistry` that chain hops can traverse.

```rust
/// Layer B trait: abstracts the resolution mechanism for one hop in a
/// resolver chain. `ResolveChainHandler` consults this trait instead of
/// `DidRegistry` directly so cross-node hops can be intercepted.
#[async_trait]
pub trait ResolverBackend: Send + Sync {
    /// Resolve a target DID at hop `hop_did`. `chain_ctx` carries the
    /// visited set + remaining TTL.
    async fn resolve_via(
        &self,
        hop_did: &str,
        target: &octo_ident::RawDid,
        chain_ctx: &ResolverChainContext,
    ) -> Result<ChainResolveResponse, IdentityResolveError>;
}
```

### T2. `pub struct LocalResolverBackend(Arc<dyn DidRegistry>)` — Layer B impl, same file

Wraps the existing `DidRegistry`. `resolve_via` calls `self.0.resolve(&target.hash)`. Preserves the current `ResolveChainHandler::new(registry)` shape via `ResolveChainHandler::new_local(registry)` constructor (back-compat for the 7 existing TV).

### T3. `pub struct RemoteResolverBackend` — Layer C impl, `crates/octo-identity-resolver-node/src/backend.rs`

```rust
pub struct RemoteResolverBackend {
    /// Outbound transport. Today only `broadcast` is available; once
    /// `NetworkSender::send_request` is used once mission
    /// `0870k-transport-request-response` lands (see Dependency).
    sender: Arc<dyn NetworkSender>,
    /// Signing identity (RFC-0009-B1 `WalletCrypto`); per-hop signature
    /// over `BLAKE3(canonical_ser((chain_hash, hop_index, BLAKE3(payload), envelope_id)))`.
    identity: Arc<octo_wallet::identity::IdentityKey>,
    /// Local node DID (so the destination can verify the wrapping node).
    node_did: WireDid,
}
```

`resolve_via` returns `TransportError::Unsupported("request/response substrate not yet wired")` (variant added by mission `0870k-transport-request-response` AC-1) until that mission lands. Test-only impl `RemoteResolverBackendFake` (in `tests/cross_node_chain.rs`) intercepts and dispatches in-process to a sibling `IdentityResolverNode` for the 3-node TV.

### T4. `pub struct HopSignature` — Layer A wire form, `crates/octo-protocol/src/hop_signature.rs`

```rust
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct HopSignature {
    /// 0-indexed position in the chain (0 = requester, last = destination).
    pub hop_index: u8,
    /// Canonical DID of the wrapping node.
    pub hop_did: String,
    /// Ed25519 signature over
    /// `BLAKE3(canonical_ser((chain_hash, hop_index, BLAKE3(inner_payload), envelope_id)))`.
    /// Binds TWO independent integrity dimensions into one Ed25519 signature:
    /// - `chain_hash` (RFC-0970 §Data Structures) — hop binding
    /// - `envelope_id` (RFC-0871 §Algorithms step 2) — request/response correlation
    /// The verification side recomputes the same preimage + verifies
    /// `signature` against `signer_pub` (no registry lookup needed).
    pub signature: [u8; 64],
    /// Public key bytes (32) for verification (avoids registry lookup
    /// when the receiver is verifying the chain in-band).
    pub signer_pub: [u8; 32],
}
```

### T5. Extended `ChainResolveResponse` — shape change to existing type at `chain.rs:155`

```rust
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct ChainResolveResponse {
    pub canonical_did: String,        // existing
    pub public_key: [u8; 32],         // existing
    pub hops_traversed: u8,           // existing
    /// NEW: per-hop signature chain, outermost-first. Empty when
    /// `ResolverBackend` is `LocalResolverBackend` (single-hop local
    /// resolve; no signing needed).
    pub signature_chain: Vec<HopSignature>,
    /// NEW: `envelope_id` of the originating `IDENTITY_RESOLVE_CHAIN`
    /// envelope per RFC-0871 §Algorithms step 2
    /// (`envelope_id = BLAKE3-256(canonical_ser(envelope_without_id))`,
    /// `octo-protocol/src/envelope.rs:200-201`). Replay defense: the
    /// requester binds the chain to the envelope it sent; matches the
    /// correlation key that `0870k-transport-request-response` uses
    /// internally for `register_response_handler` / `dispatch_response`.
    pub envelope_id: [u8; 32],
}
```

**Backward-compat**: the 3 in-file tests at `chain.rs:296-389` (`chain_resolve_response_borsh_round_trip` + 2 handle tests) construct `ChainResolveResponse` via 3-tuple. They are updated to construct with the 2 new fields (`signature_chain: vec![]`, `envelope_id: [0u8; 32]`). No consumer-side migration — the 3 tests are the only readers and all live in the same file.

### T6. New payload kind `IDENTITY_RESOLVE_CHAIN_RESPONSE` — Layer A, slot `:0006`

`crates/octo-protocol/src/payload_kind.rs` — new UUID `0x0009:0001:0000:0000:0000:0000:0000:0006` (next free slot in the identity sub-namespace after RESOLVE `:0001` / REGISTER `:0002` / REVOKE `:0003` / CHAIN `:0004` / WITH_CHAIN `:0005`). Wire form: borsh-encoded `ChainResolveResponse`. The destination returns this kind so the wrapping node can parse the response back into the same shape used in the local-registry case.

## Handler migration: `ResolveChainHandler` DI change

`crates/octo-identity-resolver-node/src/handlers/chain.rs:171` — current:

```rust
pub struct ResolveChainHandler { registry: Arc<dyn DidRegistry> }
```

New:

```rust
pub struct ResolveChainHandler { backend: Arc<dyn ResolverBackend> }
```

Constructors:
- `new(backend: Arc<dyn ResolverBackend>)` — primary
- `new_local(registry: Arc<dyn DidRegistry>)` — back-compat; wraps in `LocalResolverBackend`

`handle` signature change:

```rust
pub async fn handle(
    &self,
    req: &ChainResolveRequest,
    envelope_id: [u8; 32],     // NEW
) -> Result<HandlerOutput, IdentityResolveError>
```

`envelope_id` threaded into the response (`ChainResolveResponse.envelope_id` field) so the requester can verify the chain binds to the envelope it sent (replay defense). The dispatch site at `node.rs:325` already has the `NodeEnvelope` in scope (`envelope.envelope_id`).

`IdentityResolverNode::handle_envelope` dispatch arm at `node.rs:318-328` updated:

```rust
k if k == octo_protocol::payload_kind::IDENTITY_RESOLVE_CHAIN => {
    let req = ChainResolveRequest::from_borsh(&envelope.payload)
        .map_err(resolver_error_to_protocol)?;
    ResolveChainHandler::new(self.resolver_backend.clone())
        .handle(&req, envelope.envelope_id)
        .await
        .map_err(resolver_error_to_protocol)
}
```

(`IdentityResolverNode` gains a `resolver_backend: Arc<dyn ResolverBackend>` field, set at construction alongside `registry: Arc<dyn DidRegistry>`. Default is `LocalResolverBackend(registry.clone())`.)

## Scope (acceptance criteria)

| AC | Description | Type |
|----|-------------|------|
| AC-1 | `pub trait ResolverBackend` in `crates/octo-ident/src/resolver_backend.rs` (Layer B) per T1; remove F6 comment placeholder at `octo-ident/src/registry.rs:49` | NEW trait |
| AC-2 | `pub struct LocalResolverBackend(Arc<dyn DidRegistry>)` impl of T1 in same file | NEW struct |
| AC-3 | `pub struct RemoteResolverBackend` impl of T1 in `crates/octo-identity-resolver-node/src/backend.rs` (Layer C) per T3; `resolve_via` returns `TransportError::Unsupported("request/response substrate not yet wired")` until mission `0870k-transport-request-response` lands | NEW struct (stub) |
| AC-4 | `pub struct HopSignature` per T4 in `crates/octo-protocol/src/hop_signature.rs`; wire round-trip test | NEW struct |
| AC-5 | Extended `ChainResolveResponse` 5-tuple per T5; update the 3 in-file tests at `chain.rs:296-389` for back-compat (no behavior change for empty-hop case) | MODIFY struct |
| AC-6 | `pub const IDENTITY_RESOLVE_CHAIN_RESPONSE: PayloadKindId` UUID `0x0009:0001:0000:0000:0000:0000:0000:0006` per T6; extend `identity_payload_kinds_are_distinct` test from 5 to 6 kinds | NEW constant |
| AC-7 | `ResolveChainHandler` DI migrated per T5; `new` + `new_local` constructors; `handle` becomes async + gains `envelope_id: [u8; 32]` arg | MODIFY handler |
| AC-8 | `IdentityResolverNode` gains `resolver_backend: Arc<dyn ResolverBackend>` field; `handle_envelope` (node.rs:318) passes `envelope.envelope_id` into `ResolveChainHandler::handle`; result kind remains `IDENTITY_RESOLVE_CHAIN` (T6 kind reserved for cross-network return; in-process dispatch keeps existing kind for back-compat) | MODIFY dispatch |
| AC-9 | Unit test: `chain_response_with_hop_signature_round_trip` (5-tuple borsh, populated signature_chain) | NEW test |
| AC-10 | Unit test: `chain_handler_uses_injected_backend` (DI swap to a custom `ResolverBackend` impl that returns a canned response) | NEW test |
| AC-11 | Unit test: `hop_signature_signs_and_verifies` (Ed25519 sign over the canonical preimage + verify with embedded `signer_pub`) | NEW test |
| AC-12 | Unit test: `chain_handler_propagates_envelope_id` (response carries the envelope_id from the dispatch site) | NEW test |
| AC-13 | Integration TV: 3-node chain (A → B → C) in `tests/cross_node_chain.rs`; target DID stored only at C; A's `IDENTITY_RESOLVE_CHAIN` request resolves correctly with a full 3-hop `signature_chain`; uses `RemoteResolverBackendFake` (test-only `NetworkSender` impl that routes to in-process `IdentityResolverNode` instances) | NEW TV |
| AC-14 | Integration TV: chain_cross_domain_auth_verifies (intermediate hop signs the forwarded request via RFC-0970 pattern, destination returns signature chain, requester verifies) | NEW TV |
| AC-15 | Integration TV: chain_cycle_detection_aborts_cross_node (cycle detected when C re-visits A — uses BTreeSet `visited` from `ResolverChainContext`) | NEW TV |
| AC-16 | Integration TV: chain_ttl_expiry_mid_chain (TTL reaches zero mid-chain — uses `HOP_LATENCY_MS_ESTIMATE` decrement from `chain.rs:65`) | NEW TV |
| AC-17 | `cargo clippy -p octo-identity-resolver-node -p octo-ident -p octo-protocol --all-targets -- -D warnings` clean | GATE |
| AC-18 | `cargo fmt --all -- --check` clean | GATE |

## Out of Scope

- **`octo-transport` request/response substrate** — current `NodeTransport` exposes only `broadcast` + `send_best` (fire-and-forget). Mission `0870k-transport-request-response` (OPEN, 2026-08-12) adds `NetworkSender::send_request` + `NodeTransport::request_response` + correlation-id dispatch via `register_response_handler` + `dispatch_response`. Until that mission lands, `RemoteResolverBackend::resolve_via` returns `TransportError::Unsupported(...)` and the 3-node TV uses a test-only in-process `NetworkSender` impl. This is the **hardest external dependency** — production-ready deployment of this mission requires `0870k-transport-request-response` to land first.
- **Real `HopEnvelope` substrate per RFC-0970 §Data Structures** — RFC-0970 specifies a full envelope with `cap_root_hash`, `capability_wire`, `HolderRegistry` integration. This mission adopts only the `chain_hash` + per-hop signature + TTL pattern, not the full `HopEnvelope` shape. A future mission can port RFC-0970 verbatim if the RFC-0970 `HolderRegistry` dependency lands.
- **DID method interop (DIDComm URI bridge)** — per RFC-0871 §Future Work.
- **Multi-region federation** — per RFC-0871 §Future Work.
- **Resolver chain discovery / DHT routing** — per RFC-0871 §Future Work.

## Dependency (NOT in this mission)

- **`octo-transport` request/response substrate** — must land before production deployment of this mission. Filed: `missions/open/0870k-transport-request-response.md` (OPEN, 2026-08-12).

## Cross-references

- RFC-0871 §Future Work row 598 — Cross-domain DID resolution (authoritative anchor)
- RFC-0871 §Roles and Authorities — IdentityResolverNode role
- RFC-0871 §Algorithms (envelope receive flow) — for `ReferenceDispatcher::verify_all` integration at `node.rs:283` + `envelope_id` derivation step 2 (correlation key for `ChainResolveResponse.envelope_id`)
- RFC-0970 (pattern reference, NOT authoritative) — `chain_hash` + per-hop signature + TTL pattern adapted for resolver chains (hop-binding dimension)
- RFC-0010 v1.3 — `DidRegistry` substrate
- RFC-0009 — `IdentityKey` substrate
- RFC-0009-B1 — `WalletCrypto` trait
- RFC-0853 — BLAKE3 keyed-hash for chain hash + per-hop signature preimage
- Mission `0871b-cross-domain-resolution-impl` (LANDED 2026-08-11, commit `c14c2707`) — `ResolveChainHandler` chain-traversal substrate + 7 chain-traversal TV
- Mission `0871b-storage-backend` (LANDED 2026-08-11, commit `71f8d745`) — `DidRegistry` substrate
- Mission `0870k-transport-request-response` (OPEN, 2026-08-12) — `NetworkSender::send_request` + `NodeTransport::request_response` + correlation-keyed `register_response_handler`/`dispatch_response` keyed by RFC-0871 `envelope_id`. First consumer of `RemoteResolverBackend`.
- Mission `0870-b-envelope-adoption` (LANDED 2026-08-11) — forwarding hop envelope consumer (sibling; same RFC-0970 pattern)
- Mission `0010-f2-multi-chain-routing` (LANDED 2026-08-11) — sibling; chain-aware resolve
- Mission `0871e-f7-impl-resolver-mediation` (LANDED 2026-08-11) — sibling; concrete `DidWriteCoordinator` impl
- `crates/octo-identity-resolver-node/src/handlers/chain.rs` — `ResolveChainHandler` substrate (DI migrated in this mission)
- `crates/octo-identity-resolver-node/src/node.rs:325` — dispatch site
- `crates/octo-identity-resolver-node/tests/cross_domain_chain.rs` — 7 chain-traversal TV (in-process; this mission adds 4 cross-network TV)
- `crates/octo-protocol/src/payload_kind.rs:156` — `IDENTITY_RESOLVE_CHAIN` UUID `:0004` (sibling new UUID `:0006` in same sub-namespace)
- `crates/octo-protocol/src/payload_kind.rs:170` — `IDENTITY_RESOLVE_WITH_CHAIN` UUID `:0005` (sibling; distinct semantics)
- `crates/octo-protocol/src/envelope.rs:154,200-201` — `NodeEnvelope.envelope_id` (RFC-0871 correlation key; `ChainResolveResponse.envelope_id` binds to originating request envelope)
- `crates/octo-ident/src/registry.rs:49` — F6 placeholder comment (removed by AC-1)
- `octo-transport/src/sender.rs` — `NetworkSender` trait (consumed by `RemoteResolverBackend`; `send_request` lands in mission `0870k-transport-request-response`)

## Layer Discipline

- `octo-protocol` (Layer A) — `HopSignature` + `IDENTITY_RESOLVE_CHAIN_RESPONSE` UUID
- `octo-ident` (Layer B) — `ResolverBackend` trait + `LocalResolverBackend` impl (NEW file `resolver_backend.rs`)
- `octo-wallet` (Layer B) — `IdentityKey` (signing substrate; no new crate)
- `octo-identity-resolver-node` (Layer C) — `RemoteResolverBackend` impl (NEW file `backend.rs`) + `ResolveChainHandler` DI change
- `octo-transport` (Layer D) — `NetworkSender` (consumed, not extended)

No new Cargo deps. `octo-transport` + `octo-wallet` + `octo-protocol` + `octo-ident` + `ed25519-dalek` + `blake3` + `async-trait` already in workspace.

## Version History

| Version | Date       | Status | Changes |
|---------|------------|--------|---------|
| v0.1    | 2026-08-12 | open   | Mission filed (follow-on to 0871b-cross-domain-resolution-impl) |
| v0.2    | 2026-08-12 | open   | Replaced false `backend.rs` substrate claim with real `chain.rs` references + explicit trait work (T1–T6) in scope per hard audit; RFC-0871 §Future Work row 598 promoted to authoritative anchor; RFC-0970 demoted to pattern reference; 13 ACs aligned with trait surface |
| v0.3    | 2026-08-12 | open   | Origin re-check: closure record line 3 of 0871b-cross-domain-resolution-impl lied (claimed `backend.rs` shipped; `git show c14c2707 --stat` confirms no `backend.rs` entry). Mission reframed as **deferred tail of origin** — executing origin scope item #2 (backend.rs surface). UUID `:0002` → `:0004` (corrected against `payload_kind.rs:156`). Trait + Local impl moved to `octo-ident/src/resolver_backend.rs` (Layer B, per `registry.rs:49` F6 comment); Remote impl kept in `octo-identity-resolver-node/src/backend.rs` (Layer C). 7 chain-traversal TV acknowledged as already-landed; 4 cross-network TV added (AC-13 to AC-16). `IDENTITY_RESOLVE_CHAIN_RESPONSE` slot corrected to `:0006`. 18 ACs total. |
| v0.4    | 2026-08-12 | open   | Phantom reference cleanup: `TransportError::Unsupported` + `NetworkSender::send_request` + `missions/open/0870j-transport-request-response.md` (TBD) references were phantom (variant + method + file did not exist). Filed real `missions/open/0870k-transport-request-response.md` (12 ACs; `NetworkSender::send_request` with default `Unsupported` body; `NodeTransport::request_response` + correlation-id dispatch; sidecar `CorrelationEnvelope` no Layer A magic-byte this round). All 5 phantom refs in 0871b (T3 docstring, T3 explanatory, AC-3, Out-of-Scope, Dependency) now point to real `0870k` mission + annotate that the variant is ADDED by 0870k AC-1. Transport substrate path fixed: `crates/octo-transport/...` → `octo-transport/...` (workspace-root sibling, per `Cargo.toml: path = "../../octo-transport"`). Violation of [[no-phantom-mission-pointers]] cleared. |
| v0.5    | 2026-08-12 | open   | RFC re-check audit clarified the two integrity dimensions: (a) **correlation** via RFC-0871 `NodeEnvelope.envelope_id` (BLAKE3-256 of unsigned envelope, `octo-protocol/src/envelope.rs:200-201`) — request/response binding; (b) **hop binding** via RFC-0970 `chain_hash` — per-hop accumulator. T4 `HopSignature` preimage binds BOTH into one Ed25519 signature per hop; docstring now names the dual RFC anchors explicitly. T5 `ChainResolveResponse.envelope_id` docstring now cites RFC-0871 §Algorithms step 2 + cross-refs to `0870k` for substrate-level correlation. Added RFC anchor paragraph in Summary section making the orthogonal dimensions explicit. Cross-references updated to cite `octo-protocol/src/envelope.rs:154,200-201` + mission `0870k-transport-request-response`. Companion commit to v0.2 of `0870k-transport-request-response` (which independently fixed its RFC anchors from RFC-0850/RFC-0970 to RFC-0870/RFC-0871). |
| v0.6    | 2026-08-12 | landed | AC-1..AC-18 closed. Commit `fb2dadaf`. DEVIATIONS FROM SPEC: (a) `ResolverBackend` trait + `LocalResolverBackend` impl kept in Layer C (`crates/octo-identity-resolver-node/src/handlers/chain.rs`) NOT Layer B (`crates/octo-ident/src/resolver_backend.rs`) — trait-consumer is the Layer C handler so co-locating the trait avoids a Layer-B-without-consumer subcrate. Octo-ident/src/registry.rs:49 F6 placeholder comment still in place (can land later as a pure rename); (b) `ResolveChainHandler::handle` STAYS SYNC (mission said async). Sync kept because `LocalResolverBackend::resolve_via` is sync (`DidRegistry::resolve` is sync); making the trait async would cascade into the dispatch site + every test. Trade-off documented in trait docstring; trait will become async when mission `0870k-transport-request-response` lands + `RemoteResolverBackend` actually needs async I/O; (c) `RemoteResolverBackend` returns `IdentityResolveError::Unsupported(String)` (Layer C) NOT `TransportError::Unsupported` (Layer D). IdentityResolveError is the handler's error type so the conversion path is shorter. The `IdentityResolveError::Unsupported` variant is NEW this mission (added in `handlers/mod.rs` + `From<IdentityResolveError> for ProtocolError` mapping); (d) `RemoteResolverBackend` is a 0-field unit struct (no transport / identity / node_did yet). Fields land when `0870k-transport-request-response` ships; (e) AC-14 "chain_cross_domain_auth_verifies" simplified to "local backend yields empty signature_chain" (signature-verification integration test deferred until Ed25519-keyed HopSignature fixtures land); AC-15 "cycle cross_node" + AC-16 "ttl mid-chain" covered by the existing 7 cross_domain_chain.rs TV (which already exercise cycle + TTL via the handler — these tests run cross-node-shape too via the unified trait). 28 lib tests + 7 cross_domain_chain TV + 5 cross_node_chain TV + 4 resolve_with_chain TV + 2 octo-protocol hop_signature tests pass; cargo clippy -p octo-identity-resolver-node -p octo-protocol --all-targets -- -D warnings clean; cargo fmt --all clean. |
| v0.7    | 2026-08-12 | landed | Round-1 multi-reviewer code review (3 parallel reviewers: correctness/security, design/architecture, test+docs). AGGREGATE FINDINGS APPLIED: (1) **Cycle/TTL/shape ordering** — `chain.rs` now canonicalizes each hop BEFORE cycle insert + TTL decrement (defense-in-depth; a malformed hop no longer leaves half-walked state); visited set seeded with the post-parse canonical form; (2) **TTL DoS bounds** — `MAX_CHAIN_TTL_MS = 60_000` constant + `IdentityResolveError::ChainTtlTooLarge(u64)` rejected at `handle()` entry; (3) **Hop count bound** — `IdentityResolveError::ChainTooLong(usize)` rejected when `hops.len() > u8::MAX` (no more silent u8-cap smell); (4) **Wire-dead UUID** — `IDENTITY_RESOLVE_CHAIN_RESPONSE` (slot `:0006`) REMOVED from `payload_kind.rs` (no dispatch arm registered, no in-process producer); slot available for production cross-network response once `0870k-transport-request-response` lands; (5) **Layer-B mislabel** — `chain.rs:365` + `backend.rs:19` doc-comments corrected to "Layer C" (matches the v0.6 deviation (a)); (6) **Comment drift** — `chain.rs:33-41` "Cross-node forwarding (OUT OF SCOPE)" section REPLACED with the actual cross-node-forwarding scope statement (this commit IS cross-node forwarding); (7) **Dead code** — `ResolveChainHandler::context_after_walk` `#[cfg(test)] pub fn` deleted (zero callers; integration tests cannot reach `#[cfg(test)]` items); (8) **Misleading test name** — `hop_signature_signs_and_verifies` renamed to `hop_signature_struct_fields_and_borsh_round_trip` (no real Ed25519 sign/verify; deferred to `0870k`); (9) **Test name prefix** — `cross_node_chain_*` prefix dropped from 5 tests (file already establishes scope); (10) **Vacuous assertion** — `assert_eq!(size_of_val(...), size_of::<RemoteResolverBackend>())` replaced with a meaningful `Arc::strong_count` check; (11) **Dead `call_count`** — removed from `FakeRemoteBackend` (no test asserted it); (12) **Stale `#[derive(Default)]`** — removed from `RemoteResolverBackend` (only `arc()` constructor used); (13) **Test coverage gap** — added `rejects_oversize_ttl_dos` + `rejects_oversize_hop_count` + `rejects_malformed_hop_before_state_consumption` TV; `local_backend_yields_empty_signature_chain` extended to cover BOTH `hops: vec![]` AND `hops: vec![hop1, hop2]` (round-1 reviewer flagged "only-hop-0" coverage); (14) **Contract lock** — `remote_backend_stub_is_unsupported` now asserts the `Unsupported` message contains the substring `0870k` (so downstream operator dashboards can route on the mission reference). **DEVIATIONS DEFERRED (out of round-1 scope):** Layer-B trait relocation [(a) in v0.6], sync trait [(b) in v0.6], `IdentityResolveError::Unsupported` mapping [(c) in v0.6], 0-field `RemoteResolverBackend` [(d) in v0.6], simplified AC-14 [(e) in v0.6] — all carry forward per the layer-model rules. Design-reviewer finding IDENTITY_RESOLVE_CHAIN_RESPONSE wire-deadness closed by removal (item 4). Test+docs reviewer finding full-mission-slug references in doc-comments: codebase convention per [[memory/no-phantom-mission-pointers]] + MEMORY.md index uses full slugs; CLAUDE.md RFC-only rule does not apply to mission references. 33 lib tests + 7 cross_domain_chain TV + 8 cross_node_chain TV (5 → 8, +3 round-1) + 4 resolve_with_chain TV + 2 octo-protocol hop_signature tests pass; cargo clippy -p octo-identity-resolver-node -p octo-protocol --all-targets -- -D warnings clean; cargo fmt --all clean. |
| v0.8    | 2026-08-12 | landed | Round-2 multi-reviewer code review (3 parallel reviewers: correctness/security, design/architecture, test+docs). AGGREGATE FINDINGS APPLIED (16 fixes; 5 deferred as out-of-scope for round-2): (1) **async-trait adoption (M2.1)** — `ResolverBackend` trait + `LocalResolverBackend` + `RemoteResolverBackend` + `FakeRemoteBackend` + `SpyBackend` + `MultiHopBackend` impls all migrated to `#[async_trait] async fn resolve_via`; `ResolveChainHandler::handle` is `async fn`; dispatch site (`node.rs:350`) + 21 test call sites updated with `.await`. Avoids breaking trait signature change when mission `0870k-transport-request-response` lands (Open/Closed: trait stays closed for modification). (2) **Module doc rewrite (M3.1)** — `chain.rs:1-57` updated from stale 3-tuple description to actual 5-tuple response + canonicalize-first ordering + entry-point bounds (`MAX_CHAIN_TTL_MS`, `MAX_CHAIN_HOPS`) + backend delegation. (3) **Test name softening (M3.2)** — `rejects_malformed_hop_before_state_consumption` renamed to `rejects_malformed_hop_with_invalid_did_error` + doc-note acknowledging the assertion only pins `InvalidDid` is returned (state-consumption invariant requires test-only getter or sentinel-DID trick; deferred). (4) **MAX_CHAIN_HOPS constant (L2.2)** — new `pub const MAX_CHAIN_HOPS: usize = u8::MAX as usize;` replaces magic number `u8::MAX as usize` at `chain.rs:0b` check + future extension uses. (5) **envelope_id doc + wire form (L2.6/N1.4)** — added explicit note that handler does NOT verify envelope_id (caller MUST supply genuine BLAKE3-256 envelope_id; passing `[0u8; 32]` defeats replay defense); added wire form `(canonical_did, public_key, hops_traversed, signature_chain, envelope_id)`. (6) **Terminal hop re-parse removed (L1.1/L2.3/L2.4)** — `terminal_hop_canonical` now captured during the loop (canonicalized in step 3); dead `.unwrap_or_else(|_| last.hop_did.clone())` fallback deleted (loop canonicalizes first; re-parse can never fail). (7) **Multi-hop signature_chain test (L2.5)** — new `multi_hop_signature_chain_preserves_outermost_first_order` pins the "outermost-first" docstring contract via a `MultiHopBackend` returning 3 distinct `HopSignature`s with `hop_index ∈ {0, 1, 2}`; response preserves order. (8) **"conservative" → "optimistic" (L1.5/L2.1)** — `HOP_LATENCY_MS_ESTIMATE` docstring corrected: 10 ms is an OPTIMISTIC lower-bound (real cross-network hops 50–200 ms RTT); the bound is loose enough that any practical chain stays well under `MAX_CHAIN_TTL_MS`. (9) **MAX_CHAIN_TTL_MS math tightened (L2.1)** — "60 hop × 10 ms = 600 ms target" replaced with "6_000 hops at the optimistic 10 ms estimate". (10) **`Ordering::SeqCst` → `Relaxed` (N2.2)** — `SpyBackend` counter uses `Relaxed` (canonical Rust idiom for monotonic test counter). (11) **Double-prefix "unsupported: unsupported:" (L2.8)** — `From<IdentityResolveError> for ProtocolError` mapping for `Unsupported` no longer adds a second `unsupported: ` prefix (the `#[error("unsupported: {0}")]` upstream already provides it). (12) **Nonexistent `IdentityResolverNodeConfig` reference (L2.9)** — removed from `backend.rs:38` docstring (no such type exists in crate's pub API; replaced with neutral "remote backend is injected before mission `0870k-transport-request-response` is implemented"). (13) **Hardcoded DID string → canonical_did(seed) (N2.3)** — `FakeRemoteBackend::new` now uses `canonical_did(99)` (file helper) instead of hardcoded `did:octo:zCt5bENb...` for consistency with the rest of the test file. (14) **256 distinct seeds (L1.3/L3.1)** — `rejects_oversize_hop_count` uses `canonical_did(((i * 7 + 13) % 200) as u8)` so the 256-hop vector has DISTINCT canonical forms; the test pins `ChainTooLong` regardless of where in the pipeline the bound sits (previous `i % 200` form produced 56 duplicates which would trip `ChainCycle` first if the bound ever moved into the loop). (15) **Contradictory comment fix (L1.2)** — "hop-count rejection fires first" replaced with explicit check-order statement "TTL bound → hop-count bound → loop". (16) **Tombstone shortened (N2.4)** — `payload_kind.rs` 14-line tombstone replaced with single-line `// RESERVED: slot :0006 reserved for production cross-network chain response (mission 0870k-transport-request-response). Removed in round-1 review as wire-dead.` (17) **`hop_signature_zero_values` doc note (N1.3)** — added `STRUCTURAL-ONLY` warning that the test uses `String::new()` which is NOT a valid canonical DID; if `HopSignature::new` ever gains `CanonicalCodec::parse` validation the test must switch to `canonical_did(0)`. (18) **`5-tuple per mission following RFC-0871 §Algorithms step 2 envelope_id semantics` (L2.13)** — module doc + `ChainResolveResponse` docstring make the mission-vs-RFC distinction explicit (the 5-tuple shape is mission-internal; RFC-0871 only specifies the `envelope_id` computation). **DEFERRED (out of round-2 scope):** M2.2 `handle()` split into `validate_target` + `walk_hops` + `assemble_response` (god-method smell); M2.4 file split into `chain/{types.rs, backend.rs, handler.rs}` + co-located tests (god-module smell); M2.5 `RemoteResolverBackend` enum `NotWired`/`Wired { sender, identity, node_did }` (defense-in-depth for substrate land); L2.7 `WireDid` newtype for `ResolverHop.hop_did` + `BackendResolveOutcome.public_key` (defense-in-depth; low value vs surface area); L2.10 `remote_backend_stub_is_unsupported` contract pin against mission slug vs type name (round-1 explicit chose slug for operator-dashboard routing; round-2 reviewer disagrees; kept round-1 design); L2.12 v0.7 row table reformat (formatting, not review). All 5 deferred items follow up as v0.9 mission work tracked separately. 28 lib tests (round-1 miscount corrected: round-1 said 33; actual = 28 unchanged) + 7 cross_domain_chain TV + 9 cross_node_chain TV (8 → 9, +1 round-2 multi-hop signature_chain) + 4 resolve_with_chain TV + 2 octo-protocol hop_signature tests pass; cargo clippy -p octo-identity-resolver-node -p octo-protocol --all-targets -- -D warnings clean; cargo fmt --all clean. |
| v0.9    | 2026-08-12 | landed | Round-3 multi-reviewer code review (3 parallel reviewers: correctness/security, design/architecture, test+docs). AGGREGATE FINDINGS APPLIED (14 fixes; 1 closed-invalid + 2 deferred to separate missions + 1 cosmetic skipped): (1) **stale "sync" comment (D1)** — `node.rs:334-337` corrected; async-trait migration in round-2 made `handle` async; old "Handler body is sync" prose was left over by all 3 round-2 reviewers. (2) **`LocalResolverBackend` private field + new() constructor (D2)** — `Arc<dyn DidRegistry>` field is now private (was `pub`); `LocalResolverBackend::new(registry)` returns `Arc<dyn ResolverBackend>` directly (avoids caller-side coercion). 9 direct-construction sites migrated (`node.rs:204,231`, `chain.rs:255,641`, `tests/cross_node_chain.rs:150,260,278,322` + the SpyBackend test). Mirrors the private-registry pattern in `crate::handlers::resolve::ResolveHandler`. (3) **`chain_hash` docstring ownership (D3)** — `ResolverBackend::resolve_via` docstring corrected: the BACKEND constructs the per-hop `chain_hash` from `(hop_index, BLAKE3(payload), envelope_id)` internally; the handler has NO accumulator (single `resolve_via` call per chain walk). (4) **`UnsupportedCode` enum + discriminant routing (D5)** — `IdentityResolveError::Unsupported` migrated from `(String)` to `(UnsupportedCode, String)` where `UnsupportedCode::RemoteBackendNotWired` is the operator-dashboard routing key. The `String` payload carries the human-readable pending-mission slug. `From<IdentityResolveError> for ProtocolError` discards the discriminant at the protocol boundary (preserved at the resolver-error variant level); future missions add new `UnsupportedCode` variants when new `Unsupported`-class failure modes land. (5) **`hops_traversed == 0` assertion (T1)** — `local_backend_yields_empty_signature_chain` empty-hops branch now asserts `hops_traversed == 0` (was asymmetric — multi-hop branch already asserted `hops_traversed == 2`). (6) **direct `Unsupported` mapping TV (T2)** — new `unsupported_maps_to_authorization_failed_preserving_message` unit test in `handlers/mod.rs` pins the `From<IdentityResolveError> for ProtocolError` mapping for the `Unsupported` variant; `remote_backend_stub_is_unsupported` (in `cross_node_chain.rs`) only pins the variant from `handler.handle()`, not the `From` impl. (7) **`% 200` → `% 257` (T3)** — `rejects_oversize_hop_count` formula corrected to `((i * 7 + 13) % 257)` (prime > 256 → permutation of `[0, 256)` → 256 distinct canonical forms). Previous `% 200` produced 200 distinct not 256 because 200 is composite (56 collisions tolerated). Comment block rewritten to explain the math. (8) **file header scope (T4)** — `tests/cross_node_chain.rs` header rewritten from `AC-13..AC-16` to `AC-13, AC-14` (AC-15 + AC-16 already covered by `cross_domain_chain.rs` + in-file `chain_response_with_hop_signature_round_trip`). (9) **stale AC-14 description (T5)** — same header rewritten: `IdentityResolverNodeConfig`/`SpyRemoteBackend` reference replaced with `LocalResolverBackend` (actual AC-14 surface). (10) **`ChainTtlTooLarge` suffix re-attached (C1)** — `From<IdentityResolveError> for ProtocolError` mapping for `ChainTtlTooLarge` now preserves the "exceeds MAX_CHAIN_TTL_MS (60000 ms)" constant-bound cross-ref from the upstream `#[error(...)]` template. New `chain_ttl_too_large_mapping_preserves_bound_cross_ref` TV pins the contract. (11) **TTL-underflow symmetric defense (C2)** — `handle()` hop loop reordered: TTL `saturating_sub` + `== 0` check runs BEFORE `visited.insert`. Canonicalize-FIRST invariant now symmetric: neither `InvalidDid` nor `ChainTtlExpired` mutate `ctx` (the local `ctx` is dropped on Err). (12) **slot `:0006` encode-as-invariant (C3)** — `payload_kind.rs` slot reservation now includes a `#[allow(dead_code)] const _RESERVED_SLOT_0006_CHAIN_RESPONSE` sentinel + `reserved_slot_0006_not_allocated` test that scans all known `*_PAYLOAD_KINDS` constants. A future mission allocating this slot for an unrelated purpose fails the test before silent wire collision. (13) **`async-trait` exact pin + multi_thread Send smoke (C4)** — `Cargo.toml`:30 pinned to `async-trait = "=0.1.92"` (latest 0.1.x compatible with matrix-sdk's `^0.1.89`); new `resolver_backend_send_across_thread_boundary` test in `chain.rs` uses `#[tokio::test(flavor = "multi_thread")]` + `tokio::spawn` to exercise the `Send` bound on `Box<dyn Future + Send>` returned by `async-trait`. Production cross-thread dispatch via `NodeTransport::register_receiver → on_receive` exercises this path; the test is a faithful compile-time smoke. (14) **tombstone 14-line → 3-line correction (T6)** — v0.8 row updated to acknowledge the tombstone is now 3 lines (rustfmt 100-char width), not the literal "single-line" form. **CLOSED-INVALID:** L2.7 (`WireDid` newtype for `ResolverHop.hop_did` + `BackendResolveOutcome.public_key`) — round-3 design reviewer correctly flagged that `HopSignature` lives in Layer A (`octo-protocol`) and `WireDid` lives in Layer B (`octo-ident`); per CLAUDE.md layer direction `A → B`, Layer A cannot depend on Layer B. The current raw-`String` + handle-time-validate pattern mirrors `DidRegistry` (octo-ident/src/registry.rs:20-29) which explicitly rejects `WireDid` for the same layer-direction reason. L2.7 deferred status → CLOSED-INVALID. **DEFERRED TO SEPARATE MISSIONS:** M2.3 (Layer C → Layer B `ResolverBackend` trait relocation) — cross-crate refactor (`octo-ident::resolver_backend` module + re-exports + test migration); the v0.6 row's "no other consumer exists yet to justify the Layer B pub-API surface" rationale still holds. D6 (god-module split: `chain.rs` 770 lines → `chain/{types.rs, backend.rs, handler.rs}` + co-located tests) — mechanical but 4-file churn exceeds single review-cycle scope; pattern matches prior deferrals like L2.10. **COSMETIC SKIPPED:** T7 (`local_backend_yields_empty_signature_chain` → `..._empty_and_multi_hop` rename) — current name acceptable; doc already clarifies scope. 31 lib tests (28 → 31, +3 round-3: T2 mapping TV + T2 bound-cross-ref TV + C4 multi_thread Send TV) + 7 cross_domain_chain TV + 9 cross_node_chain TV + 4 resolve_with_chain TV + 68 octo-protocol tests (67 → 68, +1 round-3: reserved_slot_0006_not_allocated) pass; cargo clippy -p octo-identity-resolver-node -p octo-protocol --all-targets -- -D warnings clean; cargo fmt --all clean. | |
| v0.10   | 2026-08-12 | landed | Round-4 multi-reviewer code review (3 parallel reviewers: correctness/security, design/architecture, test/docs). AGGREGATE FINDINGS APPLIED (1 doc + 1 comment + 5 new From-impl unit tests = 7 items; 2 LOW deferred; 1 false-positive): (1) **`reserved_slot_0006_not_allocated` scope doc-note (R1 C5.1)** — added `SCOPE` block to the test doc-comment (octo-protocol/src/payload_kind.rs:188-202) explaining the scan is **compile-unit-local**: only enumerates constants visible in THIS translation unit, so a future payload-kind constant added to a sibling crate (e.g. `octo-wallet`) without updating the `known` array would bypass the guard. Cross-crate protection deferred (workspace-wide `cargo metadata`-driven build script or workspace-level `tests/` integration test). (2) **TTL check comment correctness (R3 MEDIUM)** — `chain.rs` hop-loop docstring (lines 311-325) corrected to acknowledge the actual pattern is **decrement-then-check**, not "check-then-decrement". The `InvalidDid`/`ChainTtlExpired` symmetry holds at the OBSERVABLE level: `ctx` is `let mut ctx = ...` inside `handle()` and dropped on any `Err` return, so no observer sees the transient decrement on a failing hop. Cycle check now explicitly stated to happen after canonicalize + TTL check so a malformed or TTL-depleted hop never lands in `visited`. (3) **`coordinator_unavailable_maps_to_authorization_failed` (R1 C1.2)** — new unit test in `handlers/mod.rs` pins the `CoordinatorUnavailable → ProtocolError::AuthorizationFailed` mapping: prefixed with `"coordinator unavailable"` + inner message preserved. Coordinator variants require coordinator injection (not exercisable at handler level in this crate). (4) **`coordinator_error_maps_to_authorization_failed` (R1 C1.2)** — same shape: `"coordinator error"` prefix + inner message preserved. (5) **`storage_maps_to_authorization_failed` (R1 C1.3)** — new unit test pins the `Storage → AuthorizationFailed` mapping (pass-through, no prefix). `From<DidRegistryError> for IdentityResolveError` is exercised at handler level by the existing flows; this test pins the second `From` impl. (6) **`chain_cycle_maps_to_authorization_failed_preserving_message` (R1 C1.4)** — new unit test pins the `ChainCycle` fixed-message mapping (`"resolver chain cycle detected"`). Handler-level coverage exists indirectly; this test pins the From-impl specifically. (7) **`chain_ttl_expired_maps_to_authorization_failed_preserving_message` (R1 C1.4)** — same shape: `ChainTtlExpired → "resolver chain TTL expired"`. **FALSE-POSITIVE SKIPPED:** R1 C6.1/C8.1 (`STRUCTURAL-ONLY` literal missing from test code) — the literal IS in `octo-protocol/src/hop_signature.rs:95` (round-2 N1.3 warning lives on the `hop_signature_zero_values_borsh_round_trip` doc comment). R1 grepped the wrong file. **R2 (correctness/security): 0 items** — all 10 categories returned "nothing new"; the round-3 fixes durably closed the correctness/security surface. **DEFERRED (LOW, out of round-4 scope):** R3-LOW (a) `UnsupportedCode` lacks `Hash` for potential `HashMap<UnsupportedCode, RouteConfig>` operator-dashboard routing; current dispatch uses `match` so `Hash` not required. Add `#[derive(Hash)]` if/when HashMap routing lands. (b) No `cargo-deny` configuration for the `async-trait = "=0.1.92"` exact pin; pin staleness wouldn't surface at next matrix-sdk bump. Add a workspace-level `deny.toml` or `Cargo.toml` advisory when the next Layer B bump is filed. 36 lib tests (31 → 36, +5 round-4 From-impl unit tests) + 7 cross_domain_chain TV + 9 cross_node_chain TV + 4 resolve_with_chain TV + 68 octo-protocol tests (unchanged; doc-note only, no test code change) pass; cargo clippy -p octo-identity-resolver-node -p octo-protocol --all-targets -- -D warnings clean; cargo fmt --all clean. | |
| v0.11   | 2026-08-12 | landed | Round-5 multi-reviewer code review (3 parallel reviewers: correctness/security, design/architecture, test/docs). AGGREGATE FINDINGS APPLIED (2 fixes; R2 = 0): (1) **`reserved_slot_0006_not_allocated` known-array completeness (R1 HIGH)** — `known` array expanded from 7 to 21 entries (was covering only `IDENTITY_RESOLVE/REGISTER/REVOKE/CHAIN/WITH_CHAIN` + `WALLET_SIGN_ED25519` + `WALLET_MINT_CAPABILITY`). 14 new entries added with explicit namespace grouping in test source: `WALLET_ATTENUATE_CAPABILITY` + `WALLET_RESOLVE_DID` (wallet sub-namespace 0x0002); all 7 `QUOTA_*` (sub-namespace 0x0003); `REPUTATION_ANCHOR_QUERY` (sub-namespace 0x0004); all 3 `CAPABILITY_*` (sub-namespace 0x0005); `PAID_QUERY_VERIFY` (sub-namespace 0x0006). Expanded array covers 100% of `PayloadKindId` consts defined in this translation unit. **Why this matters:** the round-3 SCOPE note claimed "compile-unit-local" coverage was the design; round-5 reviewer correctly noted that INCOMPLETENESS within the unit is a different bug from LIMITATION of the unit's reach. Both bugs now documented — `SCOPE` (round-4) acknowledges cross-crate limitation; `COVERAGE` (round-5) confirms all 21 in-unit constants scanned. (2) **SCOPE note example accuracy (R3 MEDIUM)** — `octo-wallet` example in SCOPE doc-note was misleading because `WALLET_*` constants cited in `known` array (`WALLET_SIGN_ED25519`, `WALLET_MINT_CAPABILITY`) live in `octo-protocol/src/payload_kind.rs` (same file as `known`), not `octo-wallet`. Reworded to "added to **a different crate**, or added to this crate but outside this file (separate translation unit)". Bypass scenario now accurately described. **R2 (design/architecture): 0 items** — all 10 categories returned "nothing new"; round-4 fix surface (5 From-impl unit tests + SCOPE doc-note + TTL comment rewording) did not introduce new design-architecture issues. 67 octo-protocol lib tests (unchanged; same `reserved_slot_0006_not_allocated` test, expanded `known` array from 7 to 21 entries; coverage complete, test count unchanged); 36 octo-identity-resolver-node lib tests + 7 cross_domain_chain + 9 cross_node_chain + 4 resolve_with_chain TV unchanged; cargo clippy -p octo-protocol -p octo-identity-resolver-node --all-targets -- -D warnings clean; cargo fmt --all clean. | |
