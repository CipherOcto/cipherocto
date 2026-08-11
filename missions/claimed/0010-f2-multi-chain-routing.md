# Mission: 0010-f2-multi-chain-routing — Multi-Chain Resolver Routing

## Status

open (filed 2026-08-11). Builds on
`0010-f2-registry-namespacing` (commit `a7efaabb`,
`register_in_chain` + `resolve_in_chain` trait methods + v011
schema migration).

## Problem

RFC-0010 v1.4 introduced `ChainId` + `ChainNamespace` substrate.
Mission `0010-f2-multi-chain-did-resolution` (commit `f6478bda`)
allocated the typed identifier. Mission `0010-f2-registry-namespacing`
(commit `a7efaabb`) added the additive `register_in_chain` +
`resolve_in_chain` trait methods + storage impl. But
`octo-identity-resolver-node` (Layer C) still exposes only the
single-chain `IDENTITY_RESOLVE` / `IDENTITY_LOOKUP` payloads —
operators cannot route a resolve request to a specific chain
namespace over the wire.

The `IdentityResolverNodeConfig.chain_id: Option<ChainId>` slot
(singular, defaults to `cipherocto-mainnet`) is consumed only by
the `IDENTITY_REGISTER` / `IDENTITY_REVOKE` mediation paths
(`chain.rs:208` / `chain.rs:281`). The read paths
(`IDENTITY_RESOLVE` / `IDENTITY_LOOKUP`) call `registry.resolve(...)`
which always reads the mainnet namespace.

Recon:
- Identity sub-namespace `0x0009:0001:...` slots: `0001` resolve,
  `0002` register, `0003` revoke, `0004` resolve_chain. Next
  free: `0005` (= `IDENTITY_RESOLVE_WITH_CHAIN`).
- `octo-ident` trait gained `register_in_chain` + `resolve_in_chain`
  in mission `0010-f2-registry-namespacing` (additive, default
  impls for back-compat). `StoolapDidRegistry` overrides both
  with chain-aware SQL. `InMemoryDidRegistry` falls back to
  single-chain default impl (test fixture).
- Cross-node forwarding (network call hop N → hop N+1) remains
  OUT OF SCOPE per mission `0871b-cross-domain-resolution-impl`
  — the request/response substrate does not exist in
  `octo-transport` yet. This mission lands the wire protocol +
  single-instance multi-chain resolve; cross-network forwarding
  lands in a follow-on mission when the substrate is available.

## Fix

### New payload kind (octo-protocol)

```rust
/// UUID: `0x0009:0001:0000:0000:0000:0000:0000:0005`
pub const IDENTITY_RESOLVE_WITH_CHAIN: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05,
]);
```

Added to `IDENTITY_RESOLVER_PAYLOAD_KINDS` array in
`octo-identity-resolver-node/src/lib.rs`.

`identity_payload_kinds_are_distinct` test grows to 5 kinds
(must remain pairwise distinct).

### New handler (octo-identity-resolver-node)

```rust
// In handlers/resolve_with_chain.rs (NEW file)
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct ResolveWithChainRequest {
    pub query: String,           // canonical DID wire form
    pub chain_id: String,        // RFC-0010 v1.4 ChainId literal
}

pub struct ResolveWithChainHandler {
    registry: Arc<dyn DidRegistry>,
}

impl ResolveWithChainHandler {
    pub fn new(registry: Arc<dyn DidRegistry>) -> Self { ... }
    pub fn handle(&self, req: &ResolveWithChainRequest)
        -> Result<HandlerOutput, IdentityResolveError> {
        // 1. Validate canonical DID shape (same as ResolveHandler).
        // 2. Parse chain_id via ChainId::new(req.chain_id.clone()) —
        //    fail-closed on malformed literal (no implicit default).
        // 3. Call registry.resolve_in_chain(&chain_id, &raw.hash).
        // 4. Return ResolveResponse (same shape as IDENTITY_RESOLVE).
    }
}
```

### Dispatch arm (octo-identity-resolver-node)

Add a fifth match arm to `IdentityResolverNode::handle_envelope`:

```rust
k if k == octo_protocol::payload_kind::IDENTITY_RESOLVE_WITH_CHAIN => {
    let req = ResolveWithChainRequest::from_borsh(&envelope.payload)
        .map_err(resolver_error_to_protocol)?;
    ResolveWithChainHandler::new(self.registry.clone())
        .handle(&req)
        .map_err(resolver_error_to_protocol)
}
```

The single-chain `IDENTITY_RESOLVE` arm is UNCHANGED — back-compat
for existing callers. The new payload kind is purely additive.

## Acceptance criteria

- [ ] NEW: `octo-protocol::payload_kind::IDENTITY_RESOLVE_WITH_CHAIN`
      UUID `0x0009:0001:0000:0000:0000:0000:0000:0005`.
- [ ] `IDENTITY_RESOLVER_PAYLOAD_KINDS` array gains the new kind.
- [ ] `identity_payload_kinds_are_distinct` test updated to assert
      5 distinct kinds.
- [ ] NEW: `handlers/resolve_with_chain.rs` with `ResolveWithChainRequest`
      + `ResolveWithChainHandler` + borsh (de)serialization.
- [ ] `IdentityResolverNode::handle_envelope` adds the dispatch arm.
- [ ] NEW TV `tests/resolve_with_chain.rs` (1 TV):
      `resolve_with_chain_isolates_dids_across_chains`: register same
      `canonical_hash` on mainnet + partner chains via direct
      `InMemoryDidRegistry` calls (the in-memory impl forwards
      register_in_chain to register — single-chain mode; test
      exercises the dispatch + wire path with distinct docs);
      resolve-with-chain on each chain returns its respective doc.
- [ ] `octo-identity-resolver-node/src/lib.rs` re-exports new
      `ResolveWithChainHandler` + `ResolveWithChainRequest`.
- [ ] Existing 24 lib tests + 7 cross_domain_chain tests + any
      other existing tests still pass (no regression).

## Files

- `crates/octo-protocol/src/payload_kind.rs` — new const +
  distinct test update.
- `crates/octo-identity-resolver-node/src/lib.rs` —
  `IDENTITY_RESOLVER_PAYLOAD_KINDS` + re-exports.
- `crates/octo-identity-resolver-node/src/handlers/resolve_with_chain.rs`
  (NEW) — request struct + handler.
- `crates/octo-identity-resolver-node/src/handlers/mod.rs` —
  export new types.
- `crates/octo-identity-resolver-node/src/node.rs` — dispatch arm.
- `crates/octo-identity-resolver-node/tests/resolve_with_chain.rs`
  (NEW) — 1 TV.

## Layer discipline

- `octo-protocol` (Layer A) — payload kind UUID allocation only.
  Per [[cipherocto-design-principles]] §Stable Abstractions, the
  payload kind table is additive; existing UUIDs unchanged.
- `octo-identity-resolver-node` (Layer C) — new handler +
  dispatch arm. Existing 4 arms unchanged.
- No new deps; no trait changes; no schema migrations.

## Defer (explicit)

- `list_in_chain` / `revoke_in_chain` payload kinds — NOT in
  scope; the single-chain `list` / `revoke` payloads serve the
  read use case. Multi-chain list/revoke lands when needed.
- Cross-node forwarding (network hop) — OUT OF SCOPE; the
  request/response substrate does not exist in `octo-transport`
  yet. The wire protocol for single-instance multi-chain resolve
  lands here; cross-network forwarding lands in a follow-on
  mission when the substrate is available.

## Cross-references

- RFC-0010 v1.4 §ChainId Namespace Extension
- Mission `0010-f2-multi-chain-did-resolution` (commit `f6478bda`)
  — typed ChainId substrate
- Mission `0010-f2-registry-namespacing` (commit `a7efaabb`) —
  `register_in_chain` + `resolve_in_chain` trait methods
- Mission `0871b-cross-domain-resolution-impl` (commit `c14c2707`)
  — chain-traversal LOGIC (separate concern: chain-of-resolvers,
  not chain-of-DIDs)
- `crates/octo-protocol/src/payload_kind.rs:152` — UUID slot
  `0005` is the next free slot in `0x0009:0001:...`