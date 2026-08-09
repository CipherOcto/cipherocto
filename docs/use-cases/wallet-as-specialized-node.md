# Use Case: Wallet as Specialized Node on the CipherOcto Mesh

## Problem

Today the CipherOcto wallet (`octo-wallet`) is treated as a passive cryptographic identity primitive. It owns keys, signs payloads, mints capability tokens — but only when called by another component (issuer node, gateway node, test harness). The wallet cannot:

- Be reached directly by other nodes to request a signature on an arbitrary payload
- Advertise the cryptographic services it offers to the network
- Participate in mesh gossip for capability revocation, key rotation, or audit events
- Be deployed on a hardware device (Ledger, YubiHSM, TEE) that communicates via BLE/USB/NFC instead of in-process function calls

Meanwhile, the `octo-transport` crate (`NodeTransport` + `NetworkSender`/`NetworkReceiver` traits, `crates/octo-transport/` path dep) already defines the general transport abstraction. RFC-0870 already establishes the "specialized node" pattern: `QuotaRouterNode::builder().seed_list(path).provider(p).peer(p).build()` is the recipe for joining a quota router to the mesh.

What's missing is the canonical protocol envelope that any specialized node — quota, identity, reputation, capability issuer, market, **wallet** — uses to communicate. Without it, each new node type invents its own wire format, and the mesh becomes a federation of incompatible protocols.

## Stakeholders

- **Primary**: CipherOcto wallet users (human + agent harness) who want to sign / mint / attenuate without trusting a third-party issuer node
- **Secondary**: Specialized node operators (quota, identity, reputation, issuer, market) who need a uniform way to ask wallets for crypto operations
- **Affected**: Hardware wallet vendors (Ledger, YubiHSM, TEE) whose devices need a network-friendly transport to participate in CipherOcto without requiring a host PC

## Motivation

CipherOcto's trust model assumes keys live with the holder, not with a server. But the current architecture has keys living in process-local memory and being read by surrounding code. A real wallet — especially a hardware wallet — must:

- Hold the key in a secure element
- Communicate over a transport (BLE / USB / NFC / QR / in-process)
- Receive signed-payload requests and respond with signatures
- Not expose the raw key to the host

The `HsmAdapter` trait in `crates/octo-wallet/src/hsm.rs` already defines the adapter contract (`get_public_key`, `sign`). `LedgerSigner` and `InMemorySigner` are concrete impls. But `IdentityKey::sign` in `crates/octo-wallet/src/identity.rs` calls `ed25519_dalek::SigningKey::from_bytes(...).sign(...)` directly — **bypassing the HSM abstraction**. Hardware wallets can't sign capability tokens today.

This Use Case is part of a broader CipherOcto pivot: every crypto-bearing entity becomes a network participant. Quota router, identity resolver, reputation anchor, capability issuer, market, **wallet** — each is a specialized node. Each speaks the same envelope. Each advertises its capabilities. Each can be discovered, addressed, and rate-limited via the mesh.

## Success Metrics

| Metric | Target | Measurement |
| ------ | ------ | ----------- |
| New specialized node types can be added without modifying the protocol envelope | ≥ 3 new node types shipped (Identity, Reputation, Wallet) without touching `octo-transport` | RFC-0871 §Conformance |
| Wallet signs capability tokens via HSM end-to-end | 100% of `IdentityKey::sign()` paths route through `HsmAdapter::sign()` | `cargo test --features allow-stub-verifier` + integration tests |
| Wallet announces its services to the mesh on startup | All wallet payload kinds declared in `RouterAnnouncePayload` | Integration test: wallet boots, mesh peers can `lookup(payload_kind)` |
| Hardware wallet over BLE participates in network signing | `LedgerSigner` + `octo-cable::BleAdapter` end-to-end test passes | Hardware test rig |
| Wallet dependency on `quota-router-storage` | 0 (today: indirect via CLI subcommand) | `cargo tree` from `octo-wallet` |
| Cryptographic substrate of any new node type doesn't require wallet core changes | Adding a new capability caveat type = own crate, registers via plugin | Workspace dependency graph |

## Constraints

- Must not: introduce a parallel transport abstraction. `NodeTransport` (existing) is THE transport.
- Must not: constrain new node types via a central `NodeType` enum. New kinds are allocated via 128-bit UUID discriminators (RFC-0965 caveat pattern).
- Must not: replicate business rules (rate limit, budget, master key bypass) inside the wallet. Wallet asks; node answers.
- Must not: make wallet depend on `quota-router-core` (cycle).
- Limited to: RFC-0010 canonical DID (`did:octo:z<base58btc>`), RFC-0126 canonical serialization, RFC-0957 capability format, RFC-0959 settlement primitives.

## Non-Goals

- Replacing `octo-transport` with a new transport crate
- Designing a global node registry / discovery service (nodes discover each other via mesh gossip)
- Specifying the wire format for any specific payload (those live in their respective RFCs)
- Replacing RFC-0903 virtual API key issuance flow (still relevant; bearer-only path)
- Implementing paid-query infrastructure in this use case (covered in companion use case `docs/use-cases/paid-query-market.md` — to be filed at RFC-0871 §Implementation Phase 5 promotion; not yet a phantom pointer per `[[no-phantom-mission-pointers]]` because the substrate is documented in RFC-0871 §Implementation Phase 5)

## Impact

If implemented:

1. **Wallet becomes a first-class network participant.** Hardware wallets can join the CipherOcto mesh over BLE without requiring a host PC to bridge.
2. **New specialized nodes ship in days, not months.** Add a `RouterAnnouncePayload` declaration + a `NetworkReceiver` impl + a few `NodeEnvelope` payload kinds. No transport changes.
3. **Capability substrate becomes extensible per crate.** New capability types (ZK, federation, time-lock, threshold-MPC) ship as `crates/octo-cap-<kind>/` with a `CapabilitySpec` impl. Wallet core unchanged.
4. **The mesh gains a uniform audit surface.** Every specialized node logs the same envelope shape. Cross-node correlation via `envelope_id` + `from_did`.
5. **Layer-E plugin model becomes the norm.** Stable core (transport, identity, HSM, DID codec) decoupled from business logic (capability types, node specializations, user extensions).

## Related RFCs

- RFC-0009 (Identity substrate): DID format (`did:octo:z<base58btc>`)
- RFC-0010 (Canonical DID codec): `crates/octo-ident/` + `DidCodec` trait
- RFC-0126 (Canonical serialization)
- RFC-0853 (BLAKE3 + channel binding)
- RFC-0862 (Atomic transaction + gossip)
- RFC-0870 (Quota router network — reference specialized node pattern)
- RFC-0903 (Virtual API key — bearer path)
- RFC-0957 (Capability token format)
- RFC-0957-A1 (HolderRegistry — capability substrate)
- RFC-0959 (Ask settlement chain — `MicroOCTO_W` settlement primitives)
- RFC-0964 (Constraint encoding)
- RFC-0965 (Capability extension format — caveat discriminator pattern)
- RFC-0968 (Reputation persistence)
- RFC-0969 (Dual pipeline authorization — `GatewayAuthenticator` placement discussion)
- RFC-0970 (Forwarding-hop auth — TTL millisecond resolution)
- RFC-0971 (Destination-node role consolidation)
- **RFC-0871** (Specialized Node Protocol Envelope — currently Draft per promotion 2026-08-08; this Use Case is the motivating input)