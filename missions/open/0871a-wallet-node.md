# Mission: 0871a — Wallet Node (RFC-0871 Phase 2)

## Status

Open (2026-08-09). RFC-0871 Accepted 2026-08-09 after R1–R7 adversarial review (R7 DRY). Phase 2 wallet node mission.

## RFC

RFC-0871 (Networking): Specialized Node Protocol Envelope

**BLUEPRINT gate note:** RFC-0871 is Accepted. Mission 0871a implements Phase 2 of the RFC's §Implementation Phases.

This mission creates the wallet's specialized-node adaptation: `WalletNode` struct implementing `NetworkReceiver` (RFC-0863 trait), advertising wallet-specific payload kinds (`WALLET_SIGN_ED25519`, `WALLET_MINT_CAPABILITY`, `WALLET_ATTENUATE_CAPABILITY`, `WALLET_RESOLVE_DID`). Complements the wallet-side foundation missions `0009-a-hsm-routing.md` (HSM closure prerequisite) + `0010-d-wallet-audience-validation.md` (canonical DID validation prerequisite). Mission `0871-protocol-core-envelope.md` (Phase 1) MUST complete first — wallet node consumes `octo-protocol::NodeEnvelope`.

## Summary

Build `crates/octo-wallet-node/` — the wallet's specialized-node adapter. `WalletNode` wraps an existing wallet + `Arc<dyn HsmAdapter>` + `Arc<NodeTransport>` (RFC-0863 substrate). It registers as a `NetworkReceiver` listening for wallet-payload-kind envelopes. Each payload kind maps to a handler: `WALLET_SIGN_ED25519` → `HsmAdapter::sign`, `WALLET_MINT_CAPABILITY` → macaroon mint + holder signature, `WALLET_ATTENUATE_CAPABILITY` → caveat append + re-sign, `WALLET_RESOLVE_DID` → `octo_ident::CanonicalCodec::parse` + lookup. Replay defense + authorization verification reuse `octo-protocol` primitives (no duplication). Existing in-wallet APIs (`CapabilityToken::mint`, `IdentityKey::sign`) become wrappers around envelope handlers; direct calls preserved for in-process callers.

## Acceptance Criteria

### Top-level: Crate + node

- [ ] NEW: `crates/octo-wallet-node/` crate with `Cargo.toml` + `src/lib.rs`
- [ ] `crates/octo-wallet-node/src/node.rs` — `WalletNode { wallet: Arc<Wallet>, hsm: Arc<dyn HsmAdapter>, transport: Arc<NodeTransport>, handlers: HashMap<PayloadKindId, Arc<dyn EnvelopeHandler>> }`
- [ ] `WalletNode::new(wallet, hsm, transport) -> Self` constructor
- [ ] `WalletNode::start() -> Result<ReceiverId, WalletNodeError>` registers `NetworkReceiver` impl, returns receiver ID for transport routing
- [ ] `WalletNode::broadcast_announce() -> Result<usize, TransportError>` announces `WALLET_SIGN_ED25519`, `WALLET_MINT_CAPABILITY`, `WALLET_ATTENUATE_CAPABILITY`, `WALLET_RESOLVE_DID` payload kinds via `RouterAnnouncePayload` extension (per RFC-0871 §Wallet Node Lifecycle)
- [ ] `WalletNode::stop() -> Result<(), WalletNodeError>` deregisters + flushes pending envelopes
- [ ] `WalletNode::handle_envelope(envelope: NodeEnvelope) -> Result<HandlerOutput, ProtocolError>` dispatch entry point per RFC-0871 §Algorithms
- [ ] NetworkReceiver trait impl (RFC-0863) delegates to `handle_envelope`

### Payload kinds

- [ ] `WALLET_SIGN_ED25519` handler: verifies `Authorization::Signature` from caller; if signature valid, signs payload via `HsmAdapter::sign(msg)`; returns signed bytes as response envelope (no on-chain settlement)
- [ ] `WALLET_MINT_CAPABILITY` handler: verifies `Authorization::Signature`; calls `CapabilityToken::mint(capability, holder_did, caveats)` from macaroon substrate; signs holder signature via HSM; returns minted token + holder signature as response envelope
- [ ] `WALLET_ATTENUATE_CAPABILITY` handler: verifies `Authorization::Signature` + parses `Authorization::Capability(token)`; calls `token.attenuate(new_caveats)`; re-signs via HSM; returns attenuated token
- [ ] `WALLET_RESOLVE_DID` handler: validates `did:octo:z<base58btc>` via `octo_ident::CanonicalCodec::parse`; looks up in local resolver cache OR queries upstream identity resolver node (Phase 3); returns canonical DID + storage-pubkey form
- [ ] Payload kind UUIDs allocated per RFC-0871 §PayloadKindId namespace: `WALLET_*` range (RFC-0871 §Wallet Node Lifecycle + RFC-0965 reserved range)

### Replay + authorization

- [ ] All handlers route through `octo_protocol::EnvelopeDispatcher` for envelope_id dedup + expiry check
- [ ] All handlers verify `Vec<Authorization>` per `octo_protocol::Authorization::verify` (signature + capability + threshold + ZK) — no shortcut
- [ ] All HSM-routed signing calls go through `Arc<dyn HsmAdapter>` (no direct `ed25519_dalek` access)
- [ ] All DID validation calls `octo_ident::CanonicalCodec::parse(s, false)`

### Backward compat

- [ ] Existing in-wallet APIs (`CapabilityToken::mint`, `IdentityKey::sign`) preserved as direct-call wrappers; envelope handlers are additive
- [ ] `cargo test -p octo-wallet --lib` continues green (no regression in existing wallet tests)
- [ ] `cargo test -p octo-wallet-node --lib` green (new crate)
- [ ] `cargo test -p octo-wallet --test eleven_step_zk` green (existing ZK acceptance)
- [ ] `cargo test -p octo-wallet --test capability_zk_acceptance` green
- [ ] `cargo clippy --workspace --all-targets --features full -- -D warnings` clean (per `[[feedback_clippy_zero_warnings]]`)
- [ ] `cargo fmt --check` clean (per `[[cargo-fmt-workflow]]`)

## Dependencies

**Requires:**

- RFC-0871 — accepted substrate
- RFC-0863 — `NodeTransport` + `NetworkReceiver` trait substrate
- RFC-0862 — gossip substrate for `broadcast_announce`
- RFC-0870 — `RouterAnnouncePayload` shape (specialized node pattern)
- `crates/octo-protocol` — Phase 1 envelope types (mission `0871-protocol-core-envelope.md`)
- `crates/octo-wallet` — wallet substrate
- `crates/octo-ident` — DID parsing
- `crates/octo-transport` — NodeTransport (RFC-0863)

**Mission gates (sequential):**

- Mission `0871-protocol-core-envelope.md` MUST complete first (Phase 1 dependency)
- Mission `0009-a-hsm-routing.md` MUST complete first (HSM routing closure — `IdentityKey::sign` must route through `Arc<dyn HsmAdapter>` before wallet node can sign via envelope)
- Mission `0010-d-wallet-audience-validation.md` MUST complete first (canonical DID validation — `WALLET_RESOLVE_DID` handler depends on `CanonicalCodec::parse` enforcement)

**Parallel with (no dependency):**

- Mission `0870-b-envelope-adoption.md` (Phase 3 quota router adoption)
- Mission `0871b-identity-resolver-node.md` (Phase 3 identity resolver)
- Mission `0871c-reputation-anchor-node.md` (Phase 3 reputation anchor)
- Mission `0871d-capability-issuer-node.md` (Phase 3 capability issuer)

**Not Requires:**

- Mission `0871e-paid-query-caveat.md` (Phase 5 — separate)
- Mission `0957-ext-macaroon-crate.md` (Phase 4 macaroon extraction — wallet node references macaroon via existing path; crate extraction is orthogonal refactor)

## Implementation Guide

- NEW crate: `crates/octo-wallet-node/` with `src/lib.rs`, `src/node.rs`, `src/handlers/{sign,mint,attenuate,resolve}.rs`, `tests/`
- `WalletNode::start()` registers `NetworkReceiver` via `transport.register_receiver(self.clone())` per RFC-0863 wiring
- `WalletNode::handle_envelope`:
  1. `EnvelopeDispatcher::dispatch` (from `octo-protocol`) — envelope_id dedup + expiry + signature verification
  2. Route by `envelope.payload_kind` to handler map
  3. Handler returns `HandlerOutput { response_envelope: Option<NodeEnvelope>, local_effect: Option<WalletEffect> }`
  4. If `response_envelope` is `Some`, send via transport back to `envelope.from_did`
- Handler tests: write one test per payload kind asserting (a) handler invoked with expected envelope, (b) response envelope correct, (c) HSM called with expected msg, (d) DID validation rejected for malformed DIDs
- `broadcast_announce`: copy pattern from `QuotaRouterNode::broadcast_announce` per RFC-0871 §Wallet Node Lifecycle (uses `RouterAnnouncePayload` extension + `network_key()` HKDF-Expand)
- `Cargo.toml` deps per CLAUDE.md crate stability: `octo-protocol` (Layer A), `octo-wallet` (Layer B), `octo-transport` (Layer D adapter), `octo-ident` (Layer B)

## Acceptance Cross-Ref

Per RFC-0871 §Implementation Phases Phase 2:

- [x] RFC Accepted (2026-08-09)
- [ ] Foundation missions complete: `0871-protocol-core-envelope.md`, `0009-a-hsm-routing.md`, `0010-d-wallet-audience-validation.md`
- [ ] Mission filed (this file)
- [ ] `WalletNode` struct + handlers implemented
- [ ] All 4 wallet payload kinds registered
- [ ] Replay + authorization + DID validation enforced

## Claimant

@unassigned

## Pull Request

#

## Notes

- Layer C crate (specialized node). Stability: per-RFC. RFC-0871 defines the wallet node; subsequent RFCs can extend payload kinds without modifying `WalletNode` (registry pattern).
- The wallet becomes a first-class network participant — deployable on Ledger/YubiHSM/TEE and joining the mesh via BLE/USB/in-process transport.
- `broadcast_announce` MUST reuse `RouterAnnouncePayload` shape from RFC-0870 (no new envelope type) per RFC-0871 §Wallet Node Lifecycle + RFC-0870 §NodeEnvelope Adoption. Wallet-specific payload kinds advertised via `AnnouncedCapability` extension (or follow-on RFC if shape changes needed).
- Hardware wallet (LedgerSigner) integration deferred to separate mission — this mission tests against `InMemorySigner` + `MockLedgerSigner`.
- Existing in-wallet APIs preserved as direct-call wrappers; envelope handlers are additive. No breaking change for existing callers.
