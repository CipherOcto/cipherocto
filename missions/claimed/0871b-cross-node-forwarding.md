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
| v0.7    | 2026-08-12 | landed | Round-1 multi-reviewer code review (3 parallel reviewers: correctness/security, design/architecture, test+docs). AGGREGATE FINDINGS APPLIED: (1) **Cycle/TTL/shape ordering** — `chain.rs` now canonicalizes each hop BEFORE cycle insert + TTL decrement (defense-in-depth; a malformed hop no longer leaves half-walked state); visited set seeded with the post-parse canonical form; (2) **TTL DoS bounds** — `MAX_CHAIN_TTL_MS = 60_000` constant + `IdentityResolveError::ChainTtlTooLarge(u64)` rejected at `handle()` entry; (3) **Hop count bound** — `IdentityResolveError::ChainTooLong(usize)` rejected when `hops.len() > u8::MAX` (no more silent u8-cap smell); (4) **Wire-dead UUID** — `IDENTITY_RESOLVE_CHAIN_RESPONSE` (slot `:0006`) REMOVED from `payload_kind.rs` (no dispatch arm registered, no in-process producer); slot available for production cross-network response once `0870k-transport-request-response` lands; (5) **Layer-B mislabel** — `chain.rs:365` + `backend.rs:19` doc-comments corrected to "Layer C" (matches the v0.6 deviation (a)); (6) **Comment drift** — `chain.rs:33-41` "Cross-node forwarding (OUT OF SCOPE)" section REPLACED with the actual cross-node-forwarding scope statement (this commit IS cross-node forwarding); (7) **Dead code** — `ResolveChainHandler::context_after_walk` `#[cfg(test)] pub fn` deleted (zero callers; integration tests cannot reach `#[cfg(test)]` items); (8) **Misleading test name** — `hop_signature_signs_and_verifies` renamed to `hop_signature_struct_fields_and_borsh_round_trip` (no real Ed25519 sign/verify; deferred to `0870k`); (9) **Test name prefix** — `cross_node_chain_*` prefix dropped from 5 tests (file already establishes scope); (10) **Vacuous assertion** — `assert_eq!(size_of_val(...), size_of::<RemoteResolverBackend>())` replaced with a meaningful `Arc::strong_count` check; (11) **Dead `call_count`** — removed from `FakeRemoteBackend` (no test asserted it); (12) **Stale `#[derive(Default)]`** — removed from `RemoteResolverBackend` (only `arc()` constructor used); (13) **Test coverage gap** — added `rejects_oversize_ttl_dos` + `rejects_oversize_hop_count` + `rejects_malformed_hop_before_state_consumption` TV; `local_backend_yields_empty_signature_chain` extended to cover BOTH `hops: vec![]` AND `hops: vec![hop1, hop2]` (round-1 reviewer flagged "only-hop-0" coverage); (14) **Contract lock** — `remote_backend_stub_is_unsupported` now asserts the `Unsupported` message contains the substring `0870k` (so downstream operator dashboards can route on the mission reference). **DEVIATIONS DEFERRED (out of round-1 scope):** Layer-B trait relocation [(a) in v0.6], sync trait [(b) in v0.6], `IdentityResolveError::Unsupported` mapping [(c) in v0.6], 0-field `RemoteResolverBackend` [(d) in v0.6], simplified AC-14 [(e) in v0.6] — all carry forward per the layer-model rules. Design-reviewer finding IDENTITY_RESOLVE_CHAIN_RESPONSE wire-deadness closed by removal (item 4). Test+docs reviewer finding full-mission-slug references in doc-comments: codebase convention per [[memory/no-phantom-mission-pointers]] + MEMORY.md index uses full slugs; CLAUDE.md RFC-only rule does not apply to mission references. 33 lib tests + 7 cross_domain_chain TV + 8 cross_node_chain TV (5 → 8, +3 round-1) + 4 resolve_with_chain TV + 2 octo-protocol hop_signature tests pass; cargo clippy -p octo-identity-resolver-node -p octo-protocol --all-targets -- -D warnings clean; cargo fmt --all clean. | |
