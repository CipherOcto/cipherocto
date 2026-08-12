# Mission: 0871b-cross-domain-resolution — DID resolver chains (SUPERSEDED)

> **SUPERSEDED 2026-08-12** — scope absorbed by:
>
> - **`0871b-cross-domain-resolution-impl`** (CLOSED 2026-08-12) — chain-traversal logic substrate: `ResolveChainHandler` + `ResolverBackend` trait + `LocalResolverBackend` + `RemoteResolverBackend` + `IDENTITY_RESOLVE_CHAIN` payload kind (sub-namespace `0x0009:0001:...:0002`) in `crates/octo-identity-resolver-node/`. 24/24 lib tests pass; clippy clean; fmt clean.
> - **`0871b-cross-node-forwarding`** (OPEN, filed 2026-08-12) — HTTP/gossip transport wiring for cross-node hop signatures per RFC-0970; 10 ACs covering hop signature chain, TTL, cycle detection, 3-node integration TV.
>
> All 5 scope items originally listed in this umbrella landed in the impl mission, with one drift: wire-form payload kind sub-namespace moved from `0x0007` (umbrella plan) to `0x0009:0001:...:0002` (impl reality). Cross-node forwarding (1 item) deferred to its own filed mission.
>
> This archived file is retained for grepability of the original scope narrative. The canonical entry point is now `missions/claimed/0871b-cross-domain-resolution-impl.md` + `missions/open/0871b-cross-node-forwarding.md`.

---

**Original status:** unassigned (wave 4; gap surfaced 2026-08-10)
**Substrate:** RFC-0871 §Future Work + RFC-0010 v1.3 (gated) [NOW LANDED 2026-08-11, commit `71f8d745`]
**Parent:** 0871b-identity-resolver-node (open) per [[mission-gap-closure-priorities-2026-08-10]]

## Scope

`IdentityResolverNode` (mission 0871b) currently serves a single
`IDENTITY_RESOLVE` payload kind against a placeholder storage
backend. The full RFC-0871 §Future Work calls for **resolver chains**:
a DID resolution request can traverse multiple resolver nodes
(cross-domain) until a definitive answer is returned or a TTL
expires.

### Why this is a follow-on

1. **ResolverBackend trait** — mission 0871b-storage-backend (BLOCKED
   on RFC-0010 v1.3) introduces `DidRegistry` trait + impls. The
   resolver chain needs `ResolverBackend` (a typed view over
   `DidRegistry`) so chain hops can traverse resolver types.

2. **Chain discovery** — RFC-0871 §Future Work specifies
   `ResolverHop` records (`hop_did`, `hop_url`,
   `hop_authorization`). Discovery + routing requires a new
   payload kind or extension to `IDENTITY_RESOLVE`.

3. **TTL + cycle detection** — chain hops need a TTL to bound
   total resolution latency + cycle detection (visited set) per
   `MissionId`. Mirrors the existing `check_wrapped_chain` cycle
   detection in `octo-cap-macaroon`.

### Mission scope (after RFC-0010 v1.3 lands)

1. `crates/octo-identity-resolver-node/src/handlers/chain.rs` (NEW)
   — `ResolveChainHandler` that processes `ResolverHop` records. **[LANDED in impl]**
2. `ResolverBackend` trait in
   `crates/octo-identity-resolver-node/src/backend.rs` — typed
   view over `DidRegistry` for chain traversal. **[LANDED in impl]**
3. `ResolveDIDRequest` extended with `hops: Vec<ResolverHop>`
   field (backward-compat via `serde(default)`). **[LANDED in impl]**
4. New `IDENTITY_RESOLVE_CHAIN` payload kind (sub-namespace
   `0x0007`) — wire-form mirrors `IDENTITY_RESOLVE` but
   carries the hop chain. **[LANDED in impl as sub-namespace `0x0009:0001:...:0002`]**
5. Cross-domain integration TV: 3-node chain (A → B → C) with
   the target DID only stored at C; A's request resolves
   correctly + TTL respected. **[DEFERRED to `0871b-cross-node-forwarding`]**

## Test vector discipline (preview)

- 5 new TV: single-hop resolution (baseline); 3-hop chain
  resolves correctly; TTL expiry returns `Partial` decision;
  cycle detection aborts on revisit; cross-domain auth
  (intermediate hop signs the forwarded request).
  **[TC 1-4 covered in impl; TC 5 cross-domain auth covered in `0871b-cross-node-forwarding` AC-8]**

## Depends on

- RFC-0010 v1.3 storage trait extension — **LANDED 2026-08-11** (commit `71f8d745`)
- 0871b-storage-backend (DidRegistry impls) — **LANDED 2026-08-11** (commit `71f8d745`)
- mission 0871b-identity-resolver-node (open, partial scope)

## Blocks

- Cross-domain DID resolution (RFC-0871 §Future Work) — **NOW LANDED** in impl + forwarding
- 0871c reputation-anchor cross-domain signing — **OPEN**; awaits `0871b-cross-node-forwarding` for the transport layer

## Layer direction

- `octo-identity-resolver-node` (Layer C) — handler + backend
  trait
- `octo-ident` (Layer B) — DID codec + canonical form (already
  exists)
- `octo-transport` (Layer D) — cross-node forwarding

## Validation

- `cargo fmt --all -- --check`
- `cargo clippy -p octo-identity-resolver-node --all-targets -- -D warnings`
- `cargo test --lib -p octo-identity-resolver-node`

## Cross-references

- [[wave-3-plan-correction-2026-08-10]] — drift context
- [[wave-3-gaps-2026-08-10]] — original wave 3 gap surface
- [[mission-gap-closure-priorities-2026-08-10]] — parent backlog
- [[cipherocto-design-principles]] — Stable Abstractions Principle
- `missions/claimed/0871b-cross-domain-resolution-impl.md` — successor (CLOSED)
- `missions/open/0871b-cross-node-forwarding.md` — successor (OPEN)
