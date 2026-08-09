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

- [x] NEW: `crates/octo-wallet-node/` crate with `Cargo.toml` + `src/lib.rs` (commit 5cf67956)
- [x] `crates/octo-wallet-node/src/node.rs` — `WalletNode { wallet: Arc<IdentityKey>, hsm: Arc<dyn HsmAdapter> (via IdentityKey), transport: Arc<NodeTransport>, dispatcher: ReferenceDispatcher }`
- [x] `WalletNode::new(config) -> Self` constructor
- [x] `WalletNode::start() -> Result<WalletNodeHandle, WalletNodeError>` registers `NetworkReceiver` impl
- [x] `WalletNode::broadcast_announce() -> Result<usize, TransportError>` (Phase 1 MVP stub — full RouterAnnouncePayload shape deferred to 0870-b follow-on)
- [ ] `WalletNode::stop()` deregisters + flushes pending envelopes (deferred — single-receiver lifecycle does not yet warrant an explicit stop API)
- [x] `WalletNode::handle_envelope(envelope: NodeEnvelope) -> Result<HandlerOutput, ProtocolError>` dispatch entry point per RFC-0871 §Algorithms
- [x] NetworkReceiver trait impl (RFC-0863) delegates to `handle_envelope` (via `WalletNodeReceiver` wrapper)

### Payload kinds

- [x] `WALLET_SIGN_ED25519` handler: signs via `IdentityKey::sign` (HSM-routed); returns 64-byte signature
- [x] `WALLET_MINT_CAPABILITY` handler: validates canonical DID; calls `CapabilityToken::mint`; returns stub wire form (full macaroon wire format in 0957 Phase 2 follow-on)
- [x] `WALLET_ATTENUATE_CAPABILITY` handler: parses existing_token wire form; appends caveat; returns stub wire form (full macaroon attenuation in 0957 Phase 2 follow-on)
- [x] `WALLET_RESOLVE_DID` handler: validates canonical DID via `CanonicalCodec::parse`; returns canonical DID + storage-pubkey form (real lookup backend in 0871b follow-on)
- [x] Payload kind UUIDs from RFC-0871 §PayloadKindId namespace (WALLET_* from existing octo-protocol constants)

### Replay + authorization

- [x] All handlers route through `EnvelopeDispatcher` for envelope_id dedup + expiry check
- [x] Authorization verification: dispatcher's `verify_all` enforces Vec<Authorization> + sig (RFC-0871 §Adversary A6)
- [x] All HSM-routed signing calls go through `Arc<dyn HsmAdapter>` via `IdentityKey` (no direct `ed25519_dalek` access in production code)
- [x] All DID validation calls `octo_ident::CanonicalCodec::parse(s, false)` (RFC-0010 v1.2 F4)

### Backward compat

- [x] Existing in-wallet APIs (`CapabilityToken::mint`, `IdentityKey::sign`) preserved; envelope handlers are additive
- [x] `cargo test -p octo-wallet --lib` 320/320 (zero regressions)
- [x] `cargo test -p octo-wallet-node --lib` 14/14 (new crate)
- [x] `cargo test -p quota-router-core --lib` 1529/1529 (zero regressions)
- [x] `cargo test -p octo-protocol --lib` 39/39 (zero regressions)
- [x] `cargo build --workspace` green
- [x] `cargo clippy -p octo-wallet-node -p octo-protocol -p quota-router-core --all-targets -- -D warnings` clean
- [x] `cargo fmt --check` clean

## Type Coverage

Per BLUEPRINT §Mission template. RFC-0871 §Wallet Node Lifecycle types mapped to this mission (Phase 2 wallet):

| RFC-0871 Type / Section | Implemented By |
|---|---|
| `WalletNode` struct (§Wallet Node Lifecycle) | This mission — `crates/octo-wallet-node/src/node.rs` |
| `NetworkReceiver` impl (RFC-0863 substrate) | This mission — `crates/octo-wallet-node/src/node.rs` |
| `WALLET_SIGN_ED25519` payload kind (§Wallet Node Lifecycle) | This mission — `crates/octo-wallet-node/src/handlers/sign.rs` |
| `WALLET_MINT_CAPABILITY` payload kind (§Wallet Node Lifecycle) | This mission — `crates/octo-wallet-node/src/handlers/mint.rs` |
| `WALLET_ATTENUATE_CAPABILITY` payload kind (§Wallet Node Lifecycle) | This mission — `crates/octo-wallet-node/src/handlers/attenuate.rs` |
| `WALLET_RESOLVE_DID` payload kind (§Wallet Node Lifecycle) | This mission — `crates/octo-wallet-node/src/handlers/resolve.rs` |
| `WalletNode::broadcast_announce` (§Wallet Node Lifecycle) | This mission — uses `RouterAnnouncePayload` extension |
| `RouterAnnouncePayload` extension shape | Mission `0870-b-envelope-adoption.md` — RFC-0870 v2.0 §NodeEnvelope Adoption defines the shape; this mission reuses |
| `IdentityKey::sign` HSM routing (gap closure) | Mission `0009-a-hsm-routing.md` — prerequisite |
| `AudienceId` canonical validation (gap closure) | Mission `0010-d-wallet-audience-validation.md` — prerequisite |
| `NodeEnvelope` envelope shape (consumed) | Mission `0871-protocol-core-envelope.md` — Phase 1 prerequisite |

## Dependencies

**Requires:**

- RFC-0871 — accepted substrate
- RFC-0863 — `NodeTransport` + `NetworkReceiver` trait substrate
- RFC-0862 — gossip substrate for `broadcast_announce`
- RFC-0870 — `RouterAnnouncePayload` shape (specialized node pattern)
- `crates/octo-protocol` — Phase 1 envelope types (mission `0871-protocol-core-envelope.md`)
- `crates/octo-wallet` — wallet substrate
- `crates/octo-ident` — DID parsing
- `octo-transport` — NodeTransport (RFC-0863); crate lives at workspace root

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
- [x] Foundation missions complete: `0871-protocol-core-envelope.md` (commit bf58559d), `0009-a-hsm-routing.md` (commit 3eca8525), `0010-d-wallet-audience-validation.md` (commit d9070a78)
- [x] Mission filed (this file)
- [x] `WalletNode` struct + handlers implemented
- [x] All 4 wallet payload kinds registered
- [x] Replay + authorization + DID validation enforced

## Claimant

@cipherocto (claimed 2026-08-09)

## Pull Request

5cf67956 (local; push + remote writes await user instruction per [[git-workflow]])

## Closure Summary

Mission 0871a-wallet-node landed in commit `5cf67956` on `next` branch.
NEW `crates/octo-wallet-node/` Layer C crate (8 files, 14 unit tests).

**New types:**
- `WalletNode` + `WalletNodeConfig` + `WalletNodeHandle` + `WalletNodeError`
- `SignHandler` / `MintHandler` / `AttenuateHandler` / `ResolveDIDHandler`
- `HandlerOutput` (response envelope payload + payload kind)
- `WALLET_PAYLOAD_KINDS` array + `is_wallet_payload_kind()` dispatcher

**Verification chain:**
- All inbound envelopes route through `EnvelopeDispatcher` (replay defense + signature verification)
- DID validation: Mint/Resolve handlers refute malformed DIDs via `CanonicalCodec::parse(s, false)`
- HSM routing: signing flows through `Arc<IdentityKey>` → `Arc<dyn HsmAdapter>`, no direct ed25519_dalek access in production code

**Phase 1 MVP disclosures:**
- Minted wire form is a placeholder `CIPHEROCTO_MINT_V1:<holder_did>` prefix (full macaroon wire format lands in 0957 Phase 2 follow-on)
- `broadcast_announce` is a stub envelope (full RouterAnnouncePayload extend in 0870-b follow-on; opcode already allocated)
- Attenuation is a stub (caveats pass through opaquely; full caveat substrate in 0957 Phase 2 follow-on)
- `WalletNode::stop()` API deferred (single-receiver lifecycle does not yet warrant explicit stop)

**Tests:** 14 new (4 handlers × 3 tests + 3 node-level tests). Zero regressions across octo-wallet (320), quota-router-core (1529), octo-protocol (39). clippy -D warnings clean, fmt clean.

## Notes

- Layer C crate (specialized node). Stability: per-RFC. RFC-0871 defines the wallet node; subsequent RFCs can extend payload kinds without modifying `WalletNode` (registry pattern).
- The wallet becomes a first-class network participant — deployable on Ledger/YubiHSM/TEE and joining the mesh via BLE/USB/in-process transport.
- `broadcast_announce` MUST reuse `RouterAnnouncePayload` shape from RFC-0870 (no new envelope type) per RFC-0871 §Wallet Node Lifecycle + RFC-0870 §NodeEnvelope Adoption. Wallet-specific payload kinds advertised via `AnnouncedCapability` extension (or follow-on RFC if shape changes needed).
- Hardware wallet (LedgerSigner) integration deferred to separate mission — this mission tests against `InMemorySigner` + `MockLedgerSigner`.
- Existing in-wallet APIs preserved as direct-call wrappers; envelope handlers are additive. No breaking change for existing callers.
