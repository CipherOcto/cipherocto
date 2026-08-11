# 0871b-cross-domain-resolution-impl — Resolver chains end-to-end

**Status:** claimed 2026-08-11 (chain-traversal LOGIC substrate landed;
cross-node forwarding deferred to follow-on mission)
**Substrate:** RFC-0010 v1.3 `DidRegistry` (LANDED 2026-08-11, commit `71f8d745`) + RFC-0871 (Accepted 2026-08-09) §Future Work
**Parent:** 0871b-storage-backend (LANDED 2026-08-11, commit `71f8d745`) + `0871b-cross-domain-resolution` (umbrella mission; scope TBD)

## Scope

Cross-domain DID resolution: a single `IDENTITY_RESOLVE` request can
traverse multiple specialized resolver nodes (resolver hops) until a
definitive answer is returned or TTL expires. Three substrate pieces
must land first:

1. **`ResolverBackend` trait** (mission 0871b-storage-backend) — typed
   view over `DidRegistry` that chain hops can traverse. Lives in
   `crates/octo-identity-resolver-node/src/backend.rs`.
2. **`ResolverHop` wire form** — chain hop record `(hop_did, hop_url,
hop_authorization)`. New `IDENTITY_RESOLVE_CHAIN` payload kind
   (sub-namespace `0x0007`) mirrors `IDENTITY_RESOLVE` but carries
   the hop chain.
3. **TTL + cycle detection** — chain hops need TTL (millisecond
   resolution per RFC-0970) to bound total latency + visited set
   per `MissionId` for cycle detection. Mirrors the existing
   `check_wrapped_chain` cycle detection in `octo-cap-macaroon`.

### Mission scope (after 0871b-storage-backend lands)

1. `crates/octo-identity-resolver-node/src/handlers/chain.rs` (NEW) —
   `ResolveChainHandler` that processes `ResolverHop` records.
2. `crates/octo-identity-resolver-node/src/backend.rs` (NEW) —
   `ResolverBackend` trait + `LocalResolverBackend` (delegates to
   `DidRegistry`) + `RemoteResolverBackend` (HTTP/gossip call to
   next hop).
3. `ResolveDIDRequest` extended with `hops: Vec<ResolverHop>` field
   (backward-compat via `serde(default)`).
4. New `IDENTITY_RESOLVE_CHAIN` payload kind — wire form mirrors
   `IDENTITY_RESOLVE` but carries the hop chain. UUID allocated in
   `crates/octo-protocol/src/payload_kind.rs` (RFC-0871 namespace
   `0x0009:0001:...:0002`).
5. Cross-domain integration TV: 3-node chain (A → B → C) with the
   target DID only stored at C; A's request resolves correctly + TTL
   respected. Cycle detection aborts on revisit. Cross-domain auth
   (intermediate hop signs the forwarded request via RFC-0970).

### Cross-domain authorization

Each hop must sign the forwarded request (RFC-0970 forwarding-hop
auth envelope pattern). The hop signature chains; the final
responder returns a chain-of-signatures envelope that the original
requester verifies. Replay defense via `envelope_id` + `nonce`
inherited from RFC-0871 §Adversary A6.

### Cycle detection

`ResolverChainContext { visited: HashSet<MissionId>, ttl_remaining_ms: u64 }`
travels with the request. Each hop checks `visited.insert(self.mission_id)`;
on collision, abort with `IdentityResolveError::ChainCycle`.
TTL check at each hop: `ttl_remaining_ms -= hop_latency_ms`; on
expiry, abort with `IdentityResolveError::ChainTtlExpired`.

## Test Vectors (preview)

- 5 new TV: single-hop resolution (baseline); 3-hop chain resolves
  correctly; TTL expiry returns `Partial` decision; cycle detection
  aborts on revisit; cross-domain auth (intermediate hop signs the
  forwarded request, final responder returns signature chain).

## Layer direction

- `octo-identity-resolver-node` (Layer C) — handler + backend trait
- `octo-ident` (Layer B) — `ChainId` from `0010-f2-multi-chain-did-resolution`
  (gated, optional)
- `octo-transport` (Layer D) — cross-node forwarding

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-identity-resolver-node --all-targets -- -D warnings
cargo test --lib -p octo-identity-resolver-node
```

## Cross-references

- [[rfc-0010-v13-storage-extension]] — `DidRegistry` substrate
- [[mission-0871b-storage-backend]] — `ResolverBackend` substrate
- [[mission-0010-f2-multi-chain-did-resolution]] — multi-chain hop routing
- [[wave-3-plan-correction-2026-08-10]] — drift context

## Claimant

@claude

## Pull Request

#
