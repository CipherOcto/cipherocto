# 0871b-cross-domain-resolution-impl — Resolver chains end-to-end

**Status:** Closed (2026-08-12). Claimed 2026-08-11; chain-traversal LOGIC substrate LANDED in `crates/octo-identity-resolver-node/src/handlers/chain.rs` (`ResolveChainHandler`) + `crates/octo-protocol/src/payload_kind.rs` (`IDENTITY_RESOLVE_CHAIN` UUID `0x0009:0001:0000:0000:0000:0000:0000:0004`). 24/24 identity-resolver lib tests pass + 7/7 integration TV (`tests/cross_domain_chain.rs`); cargo clippy clean; cargo fmt clean. **Cross-node forwarding deferred** to follow-on mission `0871b-cross-node-forwarding` (OPEN, filed 2026-08-12) — substrate supports chain-traversal logic against the local `DidRegistry` only; cross-network `ResolverBackend` + `LocalResolverBackend` + `RemoteResolverBackend` + per-hop signing per RFC-0970 pattern are out of scope per RFC-0871 §Future Work (request/response substrate missing in `octo-transport` — only `broadcast` + `send_best` fire-and-forget).
**Substrate:** RFC-0010 v1.3 `DidRegistry` (LANDED 2026-08-11, commit `71f8d745`) + RFC-0871 (Accepted 2026-08-09) §Future Work
**Parent:** 0871b-storage-backend (LANDED 2026-08-11, commit `71f8d745`) + `0871b-cross-domain-resolution` (umbrella mission; scope TBD)

## Scope

Cross-domain DID resolution: a single `IDENTITY_RESOLVE` request can
traverse multiple specialized resolver nodes (resolver hops) until a
definitive answer is returned or TTL expires. Three substrate pieces
must land first:

1. **`ResolverBackend` trait** (mission 0871b-storage-backend) — typed
   view over `DidRegistry` that chain hops can traverse. **DEFERRED**
   to follow-on mission `0871b-cross-node-forwarding`. The
   `ResolveChainHandler` lands in this mission with a direct
   `Arc<dyn DidRegistry>` DI shape; the trait abstraction is the
   next mission's work.
2. **`ResolverHop` wire form** — chain hop record `(hop_did,
hop_transport_hint)` (no auth; auth is follow-on per RFC-0970). New
   `IDENTITY_RESOLVE_CHAIN` payload kind (sub-namespace `0x0009:0001`)
   mirrors `IDENTITY_RESOLVE` but carries the hop chain.
3. **TTL + cycle detection** — chain hops need TTL (millisecond
   resolution per RFC-0970) to bound total latency + visited set
   of canonical DIDs for cycle detection. Mirrors the existing
   `check_wrapped_chain` cycle detection in `octo-cap-macaroon`.

### Mission scope (after 0871b-storage-backend lands)

1. `crates/octo-identity-resolver-node/src/handlers/chain.rs` (NEW) —
   `ResolveChainHandler` that processes `ResolverHop` records.
2. `crates/octo-identity-resolver-node/src/backend.rs` (NEW) —
   `ResolverBackend` trait + `LocalResolverBackend` (delegates to
   `DidRegistry`) + `RemoteResolverBackend` (HTTP/gossip call to
   next hop). **DEFERRED to `0871b-cross-node-forwarding`** —
   `git show c14c2707 --stat` confirms no `backend.rs` entry in this
   mission's commit.
3. `ResolveDIDRequest` extended with `hops: Vec<ResolverHop>` field
   (backward-compat via `serde(default)`). **NOT LANDED** — substrate
   uses a separate `ChainResolveRequest` payload kind instead. Scope
   self-contradicted with item #4 (separate payload kind cannot share
   the same wire form as `ResolveDIDRequest`).
4. New `IDENTITY_RESOLVE_CHAIN` payload kind — wire form is a new
   borsh-encoded `ChainResolveRequest { target, hops, ttl_remaining_ms }`,
   NOT a mirror of `IDENTITY_RESOLVE`. UUID allocated in
   `crates/octo-protocol/src/payload_kind.rs` (RFC-0871 namespace
   `0x0009:0001:...:0004`).
5. Cross-domain integration TV: 3-node chain (A → B → C) with the
   target DID only stored at C; A's request resolves correctly + TTL
   respected. Cycle detection aborts on revisit. Cross-domain auth
   (intermediate hop signs the forwarded request via RFC-0970).
   **PARTIALLY LANDED** — 7 chain-traversal integration TV landed in
   `tests/cross_domain_chain.rs` (5 named + 2 boundary); all use a
   single local `InMemoryDidRegistry` so the "3-node" TV is actually
   3 local hops against one registry. Cross-domain auth + true
   3-node TV are deferred to `0871b-cross-node-forwarding`.

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

## Version History

| Version | Date       | Status | Changes |
|---------|------------|--------|---------|
| v1.0    | 2026-08-11 | closed | Initial claim; substrate landed per scope items 1 + 4 + chain-traversal TV. |
| v1.1    | 2026-08-12 | closed | Hard audit corrections: (a) removed false `backend.rs` claim — scope item #2 was deferred, never landed (`git show c14c2707 --stat` empty for `backend.rs`); (b) UUID `:0002` → `:0004` per `payload_kind.rs:156`; (c) removed scope item #3 (`ResolveDIDRequest` extension) — substrate uses separate `ChainResolveRequest` payload kind instead; (d) clarified "3-node TV" is actually 3 local hops against single `InMemoryDidRegistry`; cross-domain auth + true 3-node TV deferred to `0871b-cross-node-forwarding`. |
