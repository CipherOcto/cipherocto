# Mission: 0871c — Reputation Anchor Node (RFC-0871 Phase 3)

## Status

Claimed + Landed (2026-08-09). RFC-0871 Accepted 2026-08-09 after R1–R7 adversarial review (R7 DRY). RFC-0968 Accepted 2026-07-26. Phase 3 reputation anchor node mission — Phase 3 MVP adapter stub landed; full RFC-0968 / RFC-0955-R1 reputation surface deferred to mission 0968a-reputation-anchoring (in flight per DAG memory).

## RFC

RFC-0871 (Networking): Specialized Node Protocol Envelope
RFC-0968 (Economics): Reputation Registry
RFC-0955-R1 (Economics): Reputation Anchoring (Amendment)

**BLUEPRINT gate note:** All substrate RFCs Accepted. Mission 0871c implements a specialized node of type `reputation-anchor` per RFC-0871 §Roles and Authorities. No new RFC required — node shape is fully defined by RFC-0871 + RFC-0968.

This mission creates `crates/octo-reputation-anchor-node/` — a specialized node that wraps RFC-0968 reputation registry operations behind a `NodeEnvelope` interface. Advertises `REPUTATION_QUERY` + `REPUTATION_UPDATE` + `REPUTATION_ANCHOR` payload kinds (the third for on-chain anchoring per RFC-0955-R1). Reuses `octo-protocol::NodeEnvelope` from mission `0871-protocol-core-envelope.md`. Cross-chain anchoring (specialized node ↔ external chain) deferred to RFC-0871 §Future Work.

## Summary

Build `crates/octo-reputation-anchor-node/` — the reputation anchor specialized node. `ReputationAnchorNode` wraps RFC-0968 `ReputationRegistry` + RFC-0955-R1 anchoring substrate + `Arc<NodeTransport>`. It registers as `NetworkReceiver`, advertises payload kinds `REPUTATION_QUERY` (read), `REPUTATION_UPDATE` (write, requires authorization), `REPUTATION_ANCHOR` (on-chain binding per RFC-0955-R1). Reuses `octo-protocol::NodeEnvelope`. Authorization required for `UPDATE` + `ANCHOR` (reputation has economic implications per the dual-stake model); `QUERY` requires authenticated caller (no anonymous reads).

## Acceptance Criteria

### Top-level: Crate + node

- [x] NEW: `crates/octo-reputation-anchor-node/` crate with `Cargo.toml` + `src/lib.rs`
- [x] `crates/octo-reputation-anchor-node/src/node.rs` — `ReputationAnchorNode { config: ReputationAnchorNodeConfig, dispatcher: ReferenceDispatcher, started: AtomicBool }` (Phase 3 MVP: `ReputationAnchorNodeConfig { transport: Arc<NodeTransport> }` only — registry + anchor backends deferred to 0968a)
- [x] `ReputationAnchorNode::new(config) -> Self` constructor
- [x] `ReputationAnchorNode::start() -> Result<ReputationAnchorNodeHandle, ReputationAnchorNodeError>` registers `NetworkReceiver`
- [x] `ReputationAnchorNode::broadcast_announce() -> Result<usize, TransportError>` (Phase 3 MVP stub — full RouterAnnouncePayload shape deferred to 0870-b follow-on)
- [x] `ReputationAnchorNode::handle_envelope(envelope: NodeEnvelope) -> Result<HandlerOutput, ProtocolError>` dispatch entry point per RFC-0871 §Algorithms
- [x] NetworkReceiver trait impl (RFC-0863) delegates to `handle_envelope` (via `ReputationAnchorNodeReceiver` wrapper)

### Payload kinds (Phase 3 MVP stub — single kind)

- [x] `REPUTATION_ANCHOR_QUERY` handler: input = `<query_did: String>` (canonical DID); validates via `octo_ident::CanonicalCodec::parse(s, false)`; returns stub `<anchor_score: u64, attestation_count: u32>` (placeholder `(0, 0)`; real lookup in mission 0968a-reputation-anchoring follow-on)
- [ ] `REPUTATION_QUERY` handler: deferred to mission 0968a-reputation-anchoring (requires RFC-0968 `ReputationRegistry` production substrate)
- [ ] `REPUTATION_UPDATE` handler: deferred to mission 0968a-reputation-anchoring (requires `ReputationUpdateCaveat` caveat + registry write substrate)
- [ ] `REPUTATION_ANCHOR` handler: deferred to mission 0968a-reputation-anchoring (requires RFC-0955-R1 anchoring substrate + `AnchorCapabilityCaveat`)
- [x] `REPUTATION_ANCHOR_QUERY` payload kind UUID allocated in `octo-protocol::payload_kind`: `0x0009:0004:0000:0000:0000:0000:0000:0001` (mission 0871c sub-namespace `0x0009:0004`; after RFC-0870 `0x0009:0003`)
- [x] All handlers: DID validation via `octo_ident::CanonicalCodec::parse(s, false)`
- [ ] All handlers: rate-limited per RFC-0871 §Replay Protection (deferred to dispatcher-config wiring in 0968a — Phase 3 MVP exposes `DispatcherConfig::permissive()` only)

### Authorization model (Phase 3 MVP stub)

- [x] `REPUTATION_ANCHOR_QUERY`: routes through `EnvelopeDispatcher::verify_all` (empty-`Vec<Authorization>` accepted; production callers use `Authorization::Signature` — gating moves into 0968a once registry substrate is live)
- [ ] `REPUTATION_QUERY`: deferred to 0968a (authenticated caller)
- [ ] `REPUTATION_UPDATE`: deferred to 0968a (`ReputationUpdateCaveat.max_delta_per_epoch` enforcement)
- [ ] `REPUTATION_ANCHOR`: deferred to 0968a (`AnchorCapabilityCaveat.anchored_period` enforcement)
- [ ] No anonymous writes — reputation has economic implications per the dual-stake model (deferred to 0968a; MVP has no write paths)

### Replay + integrity

- [x] `REPUTATION_ANCHOR_QUERY` routes through `octo_protocol::EnvelopeDispatcher` for envelope_id dedup + expiry check
- [x] `REPUTATION_ANCHOR_QUERY` verifies `Vec<Authorization>` per `octo_protocol::EnvelopeDispatcher::verify_all`
- [ ] `REPUTATION_UPDATE` append-only event emission: deferred to 0968a
- [ ] `REPUTATION_ANCHOR` anchoring receipt: deferred to 0968a

### Adversary coverage (Phase 3 MVP subset)

- [ ] Reputation inflation: deferred to 0968a (`ReputationUpdateCaveat.max_delta_per_epoch`)
- [x] Replay: envelope_id dedup via `EnvelopeDispatcher::verify_all` (seen-set + nonce + TTL)
- [ ] Unauthorized anchor: deferred to 0968a (`AnchorCapabilityCaveat`)
- [ ] Anonymous read DoS: deferred to 0968a (per-caller DID rate limit)
- [x] DID spoofing: canonical validation rejects non-canonical wire form per RFC-0010 (`CanonicalCodec::parse(s, false)` — verified by `handle_rejects_invalid_did` + `handle_rejects_legacy_bare_did` unit tests)

### Backward compat

- [x] `cargo test -p octo-reputation-anchor-node --lib` 8/8 (new crate — Phase 3 MVP)
- [x] `cargo test -p octo-protocol --lib` 44/44 (5 new tests added for `REPUTATION_ANCHOR_QUERY` UUID + RFC namespace classification; zero regressions)
- [x] `cargo clippy -p octo-reputation-anchor-node --all-targets -- -D warnings` clean
- [x] `cargo fmt --check -p octo-reputation-anchor-node` clean

## Type Coverage

Per BLUEPRINT §Mission template. RFC-0871 §Roles and Authorities + RFC-0968 + RFC-0955-R1 types mapped to this mission (Phase 3 reputation anchor):

| RFC Type / Section | Implemented By |
|---|---|
| `ReputationAnchorNode` struct (RFC-0871 §Roles and Authorities) | This mission — `crates/octo-reputation-anchor-node/src/node.rs` (Phase 3 MVP; registry + anchor backends added in 0968a) |
| `NetworkReceiver` impl (RFC-0863 substrate) | This mission — `crates/octo-reputation-anchor-node/src/node.rs` |
| `REPUTATION_ANCHOR_QUERY` payload kind | This mission — `crates/octo-reputation-anchor-node/src/handlers/query.rs` (Phase 3 MVP stub) |
| `REPUTATION_QUERY` payload kind | Deferred to mission `0968a-reputation-anchoring` |
| `REPUTATION_UPDATE` payload kind | Deferred to mission `0968a-reputation-anchoring` |
| `REPUTATION_ANCHOR` payload kind | Deferred to mission `0968a-reputation-anchoring` |
| `ReputationAnchorNode::broadcast_announce` | This mission — Phase 3 MVP stub (full RouterAnnouncePayload shape deferred to 0870-b follow-on) |
| `ReputationRegistry` substrate | RFC-0968 existing — `crates/quota-router-core/src/reputation/` (consumed in 0968a follow-on) |
| `ReputationAnchor` substrate (on-chain anchoring) | RFC-0955-R1 — implemented in mission `0968a-reputation-anchoring` (this mission does NOT own `anchor.rs`) |
| `ReputationUpdateCaveat` (RFC-0965 reserved) | Deferred to mission `0968a-reputation-anchoring` — registers caveat in `crates/octo-cap-macaroon/src/caveat/reputation.rs` |
| `AnchorCapabilityCaveat` (RFC-0955-R1 caveat) | Mission `missions/claimed/0968a-reputation-anchoring.md` — defines the caveat |
| `NodeEnvelope` envelope shape (consumed) | Mission `0871-protocol-core-envelope.md` — Phase 1 prerequisite |
| Cross-chain anchoring (specialized node ↔ external chain) | Deferred to RFC-0871 §Future Work — separate future mission |

## Dependencies

**Requires:**

- RFC-0871 — accepted substrate (envelope shape)
- RFC-0968 — reputation registry substrate
- RFC-0955-R1 — on-chain anchoring substrate
- RFC-0965 — caveat discriminator (for `ReputationUpdateCaveat` + `AnchorCapabilityCaveat`)
- RFC-0863 — `NodeTransport` + `NetworkReceiver` trait
- `crates/octo-protocol` — Phase 1 envelope types (mission `0871-protocol-core-envelope.md`)
- `crates/octo-ident` — DID parsing
- `octo-transport` — NodeTransport (RFC-0863); crate lives at workspace root

**Mission gates (sequential):**

- Mission `0871-protocol-core-envelope.md` MUST complete first (Phase 1 dependency)
- RFC-0968 implementation (existing reputation registry in `quota-router-core`) MUST be production-ready before this node exists to wrap it
- RFC-0955-R1 anchoring substrate MUST be production-ready (check `missions/claimed/0968a-reputation-anchoring.md` per BLUEPRINT §Mission Lifecycle)

**Parallel with (no dependency):**

- Mission `0870-b-envelope-adoption.md` (Phase 3 quota router)
- Mission `0871a-wallet-node.md` (Phase 2 wallet node)
- Mission `0871b-identity-resolver-node.md` (Phase 3 identity resolver)
- Mission `0871d-capability-issuer-node.md` (Phase 3 capability issuer)

**Not Requires:**

- Mission `0871e-paid-query-caveat.md` (Phase 5 — separate)
- Cross-chain anchoring (RFC-0871 §Future Work — separate future mission)

## Implementation Guide

- NEW crate: `crates/octo-reputation-anchor-node/` with `src/lib.rs`, `src/node.rs`, `src/handlers/{query,update,anchor}.rs`, `tests/`
- `ReputationAnchorNode::start()` registers `NetworkReceiver` via `transport.register_receiver(self.clone())` per RFC-0863 wiring
- `ReputationAnchorNode::handle_envelope`:
  1. `EnvelopeDispatcher::dispatch` (from `octo-protocol`) — envelope_id dedup + expiry + signature verification
  2. Route by `envelope.payload_kind` to handler map
  3. Handler queries `ReputationRegistry` (RFC-0968) and/or `ReputationAnchor` (RFC-0955-R1); returns response envelope
- `Cargo.toml` deps per CLAUDE.md crate stability: `octo-protocol` (Layer A), `quota-router-core` (reputation registry, Layer B), `octo-transport` (Layer D)
- Cross-chain anchoring (specialized node ↔ external chain) deferred — single-chain anchoring via RFC-0955-R1 substrate only. Future RFC per RFC-0871 §Future Work.

## Acceptance Cross-Ref

Per RFC-0871 §Implementation Phases Phase 3 + RFC-0968 §5. Storage Schema + RFC-0955-R1 anchoring substrate:

- [x] RFCs Accepted (RFC-0871, RFC-0968, RFC-0955-R1)
- [x] Mission filed (this file)
- [x] Phase 1 foundation complete: `0871-protocol-core-envelope.md`
- [ ] RFC-0968 reputation registry production-ready — in flight per mission 0968a-reputation-anchoring (DAG memory)
- [ ] RFC-0955-R1 anchoring substrate production-ready — in flight per mission 0968a-reputation-anchoring
- [x] `ReputationAnchorNode` struct + `REPUTATION_ANCHOR_QUERY` handler implemented (Phase 3 MVP stub)
- [ ] 3 payload kinds (`REPUTATION_QUERY`, `REPUTATION_UPDATE`, `REPUTATION_ANCHOR`) registered — Phase 3 MVP exposes ONLY `REPUTATION_ANCHOR_QUERY`; remaining 2 deferred to 0968a
- [ ] Reputation event emission per RFC-0968 §11. Audit Trail — deferred to 0968a

## Claimant

@cipherocto (claimed + landed 2026-08-09)

## Pull Request

# (local commit; push + remote writes await user instruction per [[git-workflow]])

## Closure Summary

Mission 0871c-reputation-anchor-node landed as the Phase 3 MVP adapter stub on `next` branch.

**NEW `crates/octo-reputation-anchor-node/`** Layer C crate (5 files, 8 unit tests).

**New types:**
- `ReputationAnchorNode` + `ReputationAnchorNodeConfig` + `ReputationAnchorNodeHandle` + `ReputationAnchorNodeError`
- `QueryAnchorHandler` + `QueryAnchorRequest` + `QueryAnchorResponse` (single handler for Phase 3 MVP)
- `HandlerOutput` (response envelope payload + payload kind)
- `REPUTATION_PAYLOAD_KINDS` array + `is_reputation_payload_kind()` dispatcher
- `default_dispatcher()` helper (wall-clock + cache-backed `ReferenceDispatcher`)

**`octo-protocol` amendment:**
- `REPUTATION_ANCHOR_QUERY` payload kind constant (UUID `0x0009:0004:0000:0000:0000:0000:0000:0001`)
- `REPUTATION_PAYLOAD_KINDS` array + `is_reputation_payload_kind()` dispatcher function
- 5 new tests covering UUID shape + RFC namespace classification + quota-router non-collision + borsh round-trip

**Verification chain:**
- All inbound envelopes route through `EnvelopeDispatcher::verify_all` (replay defense + signature verification)
- DID validation: `QueryAnchorHandler` rejects malformed DIDs + legacy bare form via `CanonicalCodec::parse(s, false)`
- `NetworkReceiver` wrapper (`ReputationAnchorNodeReceiver`) delegates `on_receive` → `handle_envelope` → `borsh::from_slice` per RFC-0863 wiring

**Phase 3 MVP disclosures (honest scope):**
1. **One payload kind only** — `REPUTATION_ANCHOR_QUERY`. The full 3-kind surface (`REPUTATION_QUERY`, `REPUTATION_UPDATE`, `REPUTATION_ANCHOR`) is deferred to mission `0968a-reputation-anchoring` (in flight per DAG memory). This is the contract for Phase 3 MVP per the user's instructions ("stub crate for Phase 3 — the actual reputation anchor storage/lookup is mission 0968a").
2. **Stub response** — `QueryAnchorResponse { anchor_score: 0, attestation_count: 0 }` (placeholder; real values sourced from RFC-0968 `ReputationRegistry` in 0968a).
3. **No bound identity** — `ReputationAnchorNodeConfig` carries `transport: Arc<NodeTransport>` only. No `Arc<IdentityKey>` field. `broadcast_announce` derives a placeholder `from_did` from a fixed `[0u8; 32]` placeholder pubkey via `CanonicalCodec::mint`. The user's explicit dep list (`octo-protocol`, `octo-ident`, `octo-transport`, `tokio`, `async-trait`, `borsh`, `bs58`, `thiserror`) deliberately excludes `octo-wallet` / `octo-reputation`. Real HSM-routed signing + bound identity land in 0968a.
4. **Permissive dispatcher** — `default_dispatcher()` returns `DispatcherConfig::permissive()` (1-hour TTL ceiling, all kinds served). TTL ceiling enforcement + per-node-type `served_kinds` wiring deferred to 0968a.
5. **No write paths** — Phase 3 MVP has no `REPUTATION_UPDATE` / `REPUTATION_ANCHOR` handlers, so the dual-stake `max_delta_per_epoch` enforcement + on-chain anchoring receipt generation are not yet exercised. Adversary coverage for reputation inflation + unauthorized anchor is deferred to 0968a.

**Mission DAG status:** Mission 0871c is the Phase 3 MVP adapter stub; mission 0968a-reputation-anchoring is the in-flight follow-on that completes the full RFC-0968 / RFC-0955-R1 surface. The DAG ordering (per CLAUDE.md memory `rfc-0871-mission-dag-order.md`) is `0871c (MVP stub) → 0968a (full surface)`.

**Tests:** 8 new (5 handler + 3 node-level). All 44 octo-protocol tests still green (5 new + 39 existing). clippy `-D warnings` clean, fmt clean.

## Notes

- Layer C crate (specialized node). Stability: per-RFC.
- Reputation has economic implications (dual-stake model uses reputation for slashing weight). `REPUTATION_UPDATE` MUST require `Authorization::Capability`, not raw signature. — deferred to 0968a.
- Reputation events are append-only per RFC-0968 §11. Audit Trail — never in-place mutate. This is enforced by the registry API; the node is a thin wrapper. — deferred to 0968a.
- Cross-chain anchoring (specialized node ↔ external chain) is RFC-0871 §Future Work — not in this mission scope. Filed separately when needed.
- Production deployment: reputation anchor is a stateful actor (RFC-0871 §Roles and Authorities). Coordinator role + election required for multi-node deployment. Single-node mission for now; multi-node federation deferred.
- The dual-stake model (whitepaper §Token role table) uses reputation as a slashing weight; reputation inflation is an economic attack vector. `ReputationUpdateCaveat.max_delta_per_epoch` is the primary defense. — deferred to 0968a.
- **MVP design choice:** `QueryAnchorHandler` is a unit struct (not parameterized over `IdentityKey`). The wallet-node pattern (`ResolveDIDHandler<'a> { identity: &'a IdentityKey }`) was deliberately simplified here because the Phase 3 MVP does not sign the response envelope. Re-introducing identity binding is a one-line follow-on once `octo-reputation` is wired into the dep graph in 0968a.

## Notes

- Layer C crate (specialized node). Stability: per-RFC.
- Reputation has economic implications (dual-stake model uses reputation for slashing weight). `REPUTATION_UPDATE` MUST require `Authorization::Capability`, not raw signature.
- Reputation events are append-only per RFC-0968 §11. Audit Trail — never in-place mutate. This is enforced by the registry API; the node is a thin wrapper.
- Cross-chain anchoring (specialized node ↔ external chain) is RFC-0871 §Future Work — not in this mission scope. Filed separately when needed.
- Production deployment: reputation anchor is a stateful actor (RFC-0871 §Roles and Authorities). Coordinator role + election required for multi-node deployment. Single-node mission for now; multi-node federation deferred.
- The dual-stake model (whitepaper §Token role table) uses reputation as a slashing weight; reputation inflation is an economic attack vector. `ReputationUpdateCaveat.max_delta_per_epoch` is the primary defense.
