# Mission: Holder Registry + Catalog Storage (RFC-0957-A1 Amendment)

## Status

Open

## RFC

RFC-0957-A1 (Economics): Holder Registry + Catalog Storage (Amendment) — Accepted 2026-08-02

**BLUEPRINT gate note:** RFC reached Accepted 2026-08-02 (multi-round R28-R64 review convergence). Mission now CLAIMABLE per BLUEPRINT Mission Lifecycle.

This mission is the **top-level decomposition mission** for RFC-0957-A1. RFC-0957-A1 has 15 test vectors, 4 implementation phases, and 11 new types (HolderKind enum, HolderRecord struct, HolderRegistry trait, Transaction type, StoolapHolderRegistry reference impl, CapabilityCatalog extensions, CapabilityToken::mint signature amendment, compute_cap_root_hash_from_wire helper, VerifyContext::holder_registry slot, HolderRecord constructors, IdentityKey::from_public_bytes working stub). Per BLUEPRINT §Multi-Mission Decomposition (RFC with >10 types, >4 phases), this top-level mission captures acceptance criteria + Type Coverage roll-up; the actual implementation work is decomposed into 3 sub-missions (0957-c, 0957-d, 0957-e). The sub-missions inherit this mission's RFC reference + dependencies + design goals.

## Summary

Implement the `HolderRegistry` substrate that closes RFC-0957 §Wire Format's unspecified `holder_did` resolution. Bind the wallet-side catalog to RFC-0862 (Stoolap sync layer) via a new `HolderKind` enum (4 variants), `HolderRecord` content-addressable struct, `HolderRegistry` trait (6 methods), and `StoolapHolderRegistry` reference impl. Amend `CapabilityToken::mint` to the canonical 4-arg persistence-free signature (R6-C3 fix: drops `catalog` + `Option<&mut Transaction>` parameters; post-write hook removed entirely) to break the double-insert contradiction with `insert_dual` (RFC-0969).

The registry is the resolver: wire bytes carry `cap_root_hash` but exclude `holder_did`; the caller obtains `holder_did` from `HolderRegistry::lookup(cap_root_hash)` before calling `deserialize_wire(s, holder_did, holder_pub)`. Cross-node mint verifiability (G5) requires the registry to gossip per RFC-0862.

## Acceptance Criteria

### Top-level: RFC-0957-A1 acceptance roll-up

The sub-missions (0957-c, 0957-d, 0957-e) implement the ACs by RFC-0957-A1 §Test Vectors. When all 3 sub-missions are complete and merged, every AC below is satisfied.

- [ ] All 15 RFC-0957-A1 §Test Vectors pass (TV1: Lookup Hit, TV2: Lookup Miss, TV3: Insert + Duplicate, TV4: Revoke + Lookup, TV5: Cross-Node Mint Verifiability, TV6: 4-Kind Agnosticism, TV7: Wire Format Unchanged, TV8: 100K Lookup Benchmark, TV9: Mint Is Persistence-Free, TV10: Caller-Side Persistence via TransactionExt, TV11: insert_dual Atomicity, TV12: lookup_by_ask UNIQUE, TV13: Debug Redaction, TV14: revoked_at_millis_unix Distinct from ttl_millis_unix, TV15: HopCapability Holder vs Audience)
- [ ] All 8 RFC-0957-A1 §Design Goals green (G1: lookup ≤5ms p99 over 100K holders, G2: wire byte-identical pre/post amendment, G3: mint signature amended to 4-arg persistence-free, G4: gossip convergence ≤30s, G5: cross-node mint verifiability, G6: 4-kind agnosticism, G7: zero credential material in Debug output, G8: insert_dual atomicity)
- [ ] All 3 RFC-0957-A1 §Adversary Analysis findings covered (A6: gossip partition → cross-node verification fails, A7: holder DID enumeration via gossip, A8: registry row spoofing via INSERT privilege escalation)
- [ ] Phantom type `IdentityKey::from_public_bytes` properly DEFERRED to RFC-0009-B1 / RFC-0957-A2 (working stub per §Phantom Types; full signature promotion deferred)
- [ ] Sub-missions 0957-c, 0957-d, 0957-e all merged and ACs flipped
- [ ] Cross-crate compat: `cargo build --workspace` green; `cargo test --workspace` green; `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean; `cargo fmt --check` clean

### Type Coverage

| RFC-0957-A1 Type | Implemented By |
|------------------|----------------|
| `HolderKind` enum (V1 = 0x00, ZKBearing = 0x01, Bearer = 0x02, HopCapability = 0x03) | Sub-mission 0957-c |
| `HolderRecord` struct (cap_root_hash PK + 9 fields including revoked_at_millis_unix) | Sub-mission 0957-c |
| `HolderRegistry` trait (6 methods: lookup, lookup_by_ask, lookup_active, insert, revoke, sync_peers) | Sub-mission 0957-c |
| `Transaction` type (atomic multi-record boundary) | Sub-mission 0957-c |
| `StoolapHolderRegistry` reference impl + UNIQUE(ask_id, kind) constraint + INDEX(ask_id, kind) | Sub-mission 0957-c |
| `CapabilityCatalog` extensions (4 methods: holder_registry, root_secret_for_ask, settlement_chain_tip, gossip_to_buyer) | Sub-mission 0957-e |
| `CapabilityToken::mint` signature amendment to 4-arg persistence-free | Sub-mission 0957-e |
| `compute_cap_root_hash_from_wire` helper | Sub-mission 0957-d |
| `VerifyContext::holder_registry` slot extension | Sub-mission 0957-d |
| `HolderRecord::from_bearer` + `from_capability` + `from_hop_capability` constructors | Sub-mission 0957-c (from_bearer + from_capability) + sub-mission 0970-a (from_hop_capability; cross-mission dependency on RFC-0970) |
| Manual redacting `Debug` impls (HolderRecord, HolderKind, etc.) | Sub-mission 0957-c |
| `WalletCrypto::IdentityKey::from_public_bytes` working stub | THIS top-level mission (stub lives in `crates/octo-wallet/src/capability/identity_stub.rs`; full signature in RFC-0009-B1) |

### Mission Dependency Model

```yaml
depends_on:
  - 0957-a-capability-token-macaroon # base mint + verify (claimed; in progress)
  - 0957-b-provider-boundary-exercise-path # R9-4 closure done (commit c87a4833)
decomposes_into:
  - 0957-c-holder-registry-impl # HolderKind + HolderRecord + HolderRegistry trait + StoolapHolderRegistry + Transaction
  - 0957-d-wire-resolver-update # compute_cap_root_hash_from_wire + VerifyContext::holder_registry extension
  - 0957-e-mint-txn-parameter # CapabilityToken::mint 4-arg amendment + CapabilityCatalog 4-method extension
```

## Dependencies

**Requires (RFC gates):**

- RFC-0009 — `IdentityKey`, Ed25519 substrate, `holder_sign` per §Capability Keys
  - **Phantom type (DEFERRED to RFC-0009-B1 / RFC-0957-A2):** `IdentityKey::from_public_bytes(&[u8;32]) -> Result<Self, IdentityError>`. Working stub: verifies bytes are valid Ed25519 pubkey; constructs `IdentityKey` with `pub_key` set + `priv_key = None`; returns `Ok(Self { pub_key, priv_key: None, did: format!("did:octo:{}", multibase(pub_bytes)) })`. Stub referenced from 3 sites: RFC-0957-A1 §Phantom Types:IdentityKey, RFC-0959-A1 §Algorithms:phantom_call_site, RFC-0969 §Algorithms:phantom_call_site. Full signature must be promoted from this stub into RFC-0009-B1 (or inlined into RFC-0957-A1 §Data Structures) before any downstream mission accepts.
- RFC-0126 — canonical_ser for `HolderRecord::caveats_canonical` column
- RFC-0853 — BLAKE3 keyed-hash for `cap_root_hash` PK; HKDF-BLAKE3 for nonce derivation
- RFC-0862 — persistence + gossip for the holder registry table; transaction primitive
- RFC-0957 — base capability token format (Accepted 2026-07-20)

**Optional:**

- RFC-0958 — ZK subclass accommodated via `HolderKind::ZKBearing` row

**Mission gates:**

- `missions/claimed/0957-a-capability-token-macaroon.md` (in progress) — base mint + verify must precede this mission's sub-mission 0957-e (mint signature amendment)
- `missions/claimed/0957-b-provider-boundary-exercise-path.md` — DONE (R9-4 closure commit c87a4833 dropped `CapabilityHandle.holder_did` dead field)

**Not Requires:**

- RFC-0909 — coexistence only

## Implementation Guide

- RFC-0957-A1 §Specification → §System Architecture → §Data Structures → §Algorithms → §Test Vectors (single canonical reference)
- RFC-0957-A1 §Appendices: §Schema Migration Path, §Example Integration, §RFC-0957 §Roles Token Issuer Update
- Developer guide: inline §Developer Guide section in sub-mission 0957-d (inline in this mission)

## Decomposition Rationale

RFC-0957-A1 qualifies for decomposition per BLUEPRINT §Multi-Mission Decomposition:

- **13 RFC types** (HolderKind, HolderRecord, HolderRegistry, Transaction, StoolapHolderRegistry, CapabilityCatalog x4, mint signature, compute_cap_root_hash_from_wire, VerifyContext::holder_registry, HolderRecord constructors, manual Debug impls, IdentityKey stub) — exceeds the >10 threshold
- **3 implementation phases** (§Phase 1: Schema + Trait + Reference Impl, §Phase 2: Wire + Verify Updates, §Phase 3: Mission Decomposition) — does not exceed >4 but the work is naturally split by module boundary
- **Different prerequisite chains:**
  - 0957-c (registry impl) depends on RFC-0862 stoolap substrate
  - 0957-d (wire resolver) depends on RFC-0957 §Wire Format + 0957-a base mission
  - 0957-e (mint signature amendment) depends on 0957-a base mission completing mint first

Splitting by module boundary (registry / wire / mint) lets each sub-mission merge independently when its dependency is satisfied.

## Claimant

@unclaimed

## Pull Request

(unset)

## Notes

- The `IdentityKey::from_public_bytes` stub MUST live in this top-level mission (not in sub-mission 0957-c) because it is referenced from RFC-0959-A1 §Algorithms:phantom_call_site and RFC-0969 §Algorithms:phantom_call_site — both of which are independent RFCs in this batch. Stub location: `crates/octo-wallet/src/capability/identity_stub.rs`.
- RFC-0957-A1 §Future Work (F1: catalog federation across nodes; F2: 30-day GC of Revoked/Expired rows; F3: append-only audit log; F4: CapabilityCatalog V2 bundling) — concrete plan documented in RFC-0957-A1 §Future Work; not in scope for this mission; does NOT block the 13 ACs. Track via follow-up mission `missions/open/0957-f-future-work.md` (to be claimed when sub-missions 0957-c/d/e land).
- `HolderRecord::from_hop_capability` constructor is a cross-mission dependency on RFC-0970 sub-mission 0970-a. If RFC-0970 is claimed first, that constructor lives in 0970-a and is consumed here via the trait; otherwise it lives here and is consumed by 0970-a via the trait. Convention: constructor on `HolderRecord` impl, documented in RFC-0957-A1 §Data Structures; trait method or free function consumed by RFC-0970 §Algorithms:wrap_for_hop.

### Related

- [Dual-Mode Authorization Batch Accepted 2026-08-02](../rfcs/accepted/economics/0957-a1-holder-registry.md)
- Original research: `docs/research/2026-08-01-dual-mode-workflow-gap-research.md`
- Original use case: `docs/use-cases/dual-mode-authorization-workflow.md`
