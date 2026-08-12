# Mission: 0871b-cross-node-forwarding — Cross-node resolver hop transport

## Status

Open (filed 2026-08-12). Follow-on to `0871b-cross-domain-resolution-impl` (CLOSED 2026-08-12).

## Summary

`ResolveChainHandler` + `RemoteResolverBackend` trait landed in `octo-identity-resolver-node` per mission `0871b-cross-domain-resolution-impl`. The substrate supports multi-hop resolver chains via cycle detection + TTL budget + `IDENTITY_RESOLVE_CHAIN` payload kind, but the **transport layer** for cross-node forwarding (HTTP/gossip call to the next hop with signature chain envelope per RFC-0970) is out of scope.

This mission lands the cross-node transport wiring: each hop signs the forwarded request, the final responder returns a signature chain envelope, and the original requester verifies the chain.

## Substrate (already shipped)

- `crates/octo-identity-resolver-node/src/backend.rs` — `ResolverBackend` trait + `LocalResolverBackend` (delegates to `DidRegistry`) + `RemoteResolverBackend` (signature only — no HTTP/gossip yet)
- `crates/octo-identity-resolver-node/src/handlers/chain.rs` — `ResolveChainHandler` + `ResolverChainContext { visited: HashSet<MissionId>, ttl_remaining_ms: u64 }`
- `crates/octo-protocol/src/payload_kind.rs` — `IDENTITY_RESOLVE_CHAIN` UUID in sub-namespace `0x0009:0001:...:0002`

## Scope

| AC | Description |
|----|-------------|
| AC-1 | `RemoteResolverBackend` impl backed by `octo-transport` HTTP client (Layer D; picks up the existing transport trait per RFC-0871 §Forwarding) |
| AC-2 | Hop signature chain — each intermediate hop signs the forwarded request via Ed25519 over `canonical_ser(forwarded_request)` |
| AC-3 | Final responder returns `ResolveWithChainResponse { did, signature_chain: Vec<HopSignature>, envelope_id }` |
| AC-4 | Original requester verifies the full signature chain (each hop signature + terminal signature) |
| AC-5 | Replay defense via `envelope_id` + `nonce` (inherited from RFC-0871 §Adversary A6) |
| AC-6 | TTL enforcement — each hop measures `hop_latency_ms`, decrements `ttl_remaining_ms`; abort with `IdentityResolveError::ChainTtlExpired` on expiry |
| AC-7 | Cycle detection via `visited.insert(self.mission_id)`; abort with `IdentityResolveError::ChainCycle` on revisit |
| AC-8 | Cross-domain integration TV: 3-node chain (A → B → C) with target DID only stored at C; A's request resolves correctly + TTL respected + cycle detection aborts on revisit |
| AC-9 | `cargo clippy -p octo-identity-resolver-node -p octo-protocol --all-targets -- -D warnings` clean |
| AC-10 | `cargo fmt --all -- --check` clean |

## Out of Scope

- Resolver chain discovery / DHT routing (separate mission if needed; per RFC-0871 §Future Work)
- Multi-region federation (separate mission)
- DID method interop (DIDComm URI bridge; deferred per `RFC-0XXX` placeholder)

## Cross-references

- RFC-0871 (Networking): Distributed Resolver Network — §Future Work
- RFC-0970 (Networking): Forwarding Hop Auth Envelope — signature chain pattern
- RFC-0010 v1.3 — `DidRegistry` substrate
- Mission `0871b-cross-domain-resolution-impl` (CLOSED 2026-08-12) — `ResolveChainHandler` substrate
- Mission `0871b-storage-backend` (LANDED 2026-08-11, commit `71f8d745`) — `DidRegistry` substrate
- Mission `0870-b-envelope-adoption` (CLAIMED) — forwarding hop envelope consumer
- `crates/octo-identity-resolver-node/src/backend.rs` — `RemoteResolverBackend` trait
- `crates/octo-identity-resolver-node/src/handlers/chain.rs` — `ResolveChainHandler` substrate

## Layer Discipline

- `octo-identity-resolver-node` (Layer C) — handler + backend impl
- `octo-transport` (Layer D) — HTTP client transport trait (already wired)
- `octo-ident` (Layer B) — `DidRegistry` substrate (already shipped)
- `octo-protocol` (Layer A) — payload kind + canonical serialization (already shipped)

No new Cargo deps; HTTP client transport is already in workspace via `octo-transport`.

## Version History

| Version | Date       | Status | Changes |
|---------|------------|--------|---------|
| v0.1    | 2026-08-12 | open   | Mission filed (follow-on to 0871b-cross-domain-resolution-impl per [[deferred-vs-unspecified]]) |
