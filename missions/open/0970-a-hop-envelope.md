# Mission: Hop Envelope + Chain Verify (RFC-0970 §Phase 1 + §Phase 2 + §Phase 3)

## Status

Open

## RFC

RFC-0970 (Networking): Forwarding-Hop Authorization Envelope — Accepted 2026-08-02

**Sub-mission of:** `missions/open/0970-forwarding-hop-auth-envelope.md` (top-level decomposition mission)

## Summary

Implement RFC-0970 §Phase 1 (Data Structures + Algorithms), §Phase 2 (Channel Layer Integration), and §Phase 3 (HolderRegistry Binding). Author `HopEnvelope` (4-segment wire), `HopCapability` (HolderKind::HopCapability row), `HopScope` enum (Forwarder / Auditor / PureForwarder), `InnerRequest` (encrypted inner payload). Author `wrap_for_hop`, `unwrap_at_destination`, `verify_chain_hash`, `pure_forward` algorithms. Bind `HolderRecord::from_hop_capability` constructor (cross-mission: co-author contract with 0957-c). Phantom type `DestinationNonceStore` per-destination nonce seed = `node_epoch` (DEFERRED to RFC-0009-B1 / RFC-0957-A2; working stub).

Manual redacting `Debug` impls on `HopEnvelope`, `HopCapability`, `InnerRequest`. TTL millisecond resolution (200ms gate per TV11).

## Acceptance Criteria

### Type definitions

- [ ] `crates/octo-wallet/src/capability/hop_envelope.rs` (NEW) — `HopEnvelope` (4-segment wire), `HopCapability`, `HopScope`, `InnerRequest`. All 4 with manual redacting Debug impls.
- [ ] `HopScope` enum: `Forwarder (registers HopCapability in HolderRegistry)`, `Auditor (registers HopCapability + emits audit_replay_log entry)`, `PureForwarder (NO HolderKind insert; cross-realm replay defense per Finding A22)`.
- [ ] `InnerRequest` payload is encrypted (per Finding A16: compromised intermediate router MUST NOT read inner content). Use RFC-0853 channel encryption.

### HolderRegistry binding

- [ ] `crates/octo-wallet/src/capability/holder_registry.rs` (MODIFY) — `HolderRecord::from_hop_capability(hop_cap: HopCapability, mint_at_unix_ms: i64) -> Self` constructor. Cross-mission: 0970-a owns this if 0957-c lands later; otherwise co-author.
- [ ] Schema: `HolderKind::HopCapability = 0x03` row. `holder_did` = intermediate router DID; `audience_did` = destination node DID. TV15 (`HolderRecord::from_hop_capability` holder vs audience) lives in sub-mission 0957-d.

### Algorithms

- [ ] `crates/quota-router-core/src/node/wrap.rs` (NEW) — `wrap_for_hop(inner: &InnerRequest, hop_key: &Ed25519Keypair, ttl_millis_unix: u64, node_epoch: u64) -> Result<HopEnvelope, HopError>`.
- [ ] `unwrap_at_destination(envelope: &HopEnvelope, chain: &[HopEnvelope], expected_destination: Did, clock: &dyn Clock) -> Result<InnerRequest, HopError>` — chain verify + unwrap.
- [ ] `verify_chain_hash(chain: &[HopEnvelope], expected_chain_hash: [u8; 32]) -> Result<(), HopError>` — free function (R7-N3 fix: `verify_chain_hash` is a FREE FUNCTION in RFC-0970, NOT a trait method).
- [ ] `pure_forward(inner: &InnerRequest, hop_key: &Ed25519Keypair, ttl_millis_unix: u64) -> Result<HopEnvelope, HopError>` — NO `HolderKind` insert; cross-realm replay defense.

### Phantom type

- [ ] `crates/octo-wallet/src/capability/nonce_store_stub.rs` (NEW) — working stub for `DestinationNonceStore`. Full signature DEFERRED to RFC-0009-B1 / RFC-0957-A2.
- [ ] Stub API: `pub struct DestinationNonceStoreStub; impl DestinationNonceStoreStub { pub fn new() -> Self; pub fn check_and_consume(&mut self, destination: Did, nonce: [u8;32], node_epoch: u64) -> Result<(), NonceError>; }`

### Test vectors (RFC-0970 §Test Vectors, this sub-mission owns TV1, TV2, TV3, TV4, TV5, TV6, TV7, TV8, TV9, TV10, TV11 — all 11)

- [ ] TV1: Single-Hop Wrap + Unwrap — round-trip one hop; verify envelope structure.
- [ ] TV2: Three-Hop Chain — three sequential `wrap_for_hop` calls; destination unwraps full chain; `verify_chain_hash` matches.
- [ ] TV3: Replay Detection — duplicate `HopEnvelope.hop_envelope_id` returns `HopError::ReplayDetected`.
- [ ] TV4: TTL Expiration — `ttl_millis_unix` exceeded returns `HopError::TtlExceeded`.
- [ ] TV5: Audience Mismatch — `envelope.audience_did != expected_destination` returns `HopError::AudienceMismatch`.
- [ ] TV6: Intermediate Router Compromise — Inner Content Encrypted — compromised intermediate router reads `InnerRequest`; payload is encrypted (RFC-0853 channel encryption).
- [ ] TV7: Hop Signature Forgery — tampered `hop_signature` returns `HopError::HopSignatureInvalid`.
- [ ] TV8: Chain Hash Mismatch — tampered chain (one hop's inner request altered) returns `HopError::ChainHashMismatch`.
- [ ] TV9: Debug Redaction — `format!("{:?}", envelope)` contains `[REDACTED]` markers; grep test for credential material.
- [ ] TV10: Pure Forwarder — `pure_forward` produces a `HopEnvelope` with `HopScope::PureForwarder`; no `HolderKind::HopCapability` row inserted in `HolderRegistry`.
- [ ] TV11: TTL Millisecond Resolution (200ms) — TTL granularity is milliseconds, NOT seconds; 200ms window determinism gate.

### Cross-crate compat

- [ ] `cargo build --workspace` green
- [ ] `cargo test --workspace` green
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [ ] `cargo fmt --check` clean

## Dependencies

**Requires (RFC gates):**

- RFC-0853 — per-hop channel binding (encryption for InnerRequest)
- RFC-0862 — HolderRegistry gossip + audit_replay_log sync
- RFC-0870 — Router role substrate
- RFC-0957-A1 — unified HolderRegistry (`HolderKind::HopCapability` + `from_hop_capability`)

**Requires (mission gates):**

- `missions/open/0970-forwarding-hop-auth-envelope.md` (top-level)
- `missions/open/0957-c-holder-registry-impl.md` — `HolderRecord` base struct; `from_hop_capability` constructor (cross-mission co-author contract)

```yaml
depends_on:
  - mission-0957-c-holder-registry-impl # HolderRecord + HolderKind::HopCapability
  - mission-0957-d-wire-resolver-update # holder vs audience resolution (TV15 dependency)
```

## Type Coverage

This sub-mission implements (per top-level Type Coverage table):

- `HopEnvelope` struct
- `HopCapability` struct
- `HopScope` enum
- `InnerRequest` struct
- `wrap_for_hop` algorithm
- `unwrap_at_destination` algorithm
- `verify_chain_hash` free function
- `pure_forward` algorithm
- `DestinationNonceStore` phantom type (stub)
- `node_epoch` per-destination nonce seed
- `audit_replay_log` append-only log
- TTL millisecond resolution
- `HolderRecord::from_hop_capability` constructor (cross-mission)
- Manual redacting Debug impls

`ForwardRequestPayload` extension lives in sub-mission 0970-b.

## Location

- `crates/octo-wallet/src/capability/hop_envelope.rs` (NEW)
- `crates/octo-wallet/src/capability/nonce_store_stub.rs` (NEW)
- `crates/quota-router-core/src/node/wrap.rs` (NEW)
- `crates/octo-wallet/src/capability/holder_registry.rs` (MODIFY) — `from_hop_capability` constructor

## Claimant

@unclaimed

## Pull Request

(unset)

## Notes

- Wire format is 4-segment: `hop_envelope_id || hop_pub || hop_signature || inner_request` (base64url-no-pad). Distinct from RFC-0959-A1's 3-segment envelope wire.
- TTL is milliseconds (TV11 gate). The 200ms gate documents determinism granularity.
- Phantom type `DestinationNonceStore` is a per-destination nonce store. The seed is `node_epoch` (per-destination epoch). Stub MUST consume nonce + epoch + destination; production impl DEFERRED.
- The `from_hop_capability` constructor is a cross-mission co-author contract with 0957-c. Convention: 0957-c owns the trait method; 0970-a provides the `HopCapability` argument. If 0957-c lands first, this mission only consumes; if 0970-a lands first, this mission authors the constructor and 0957-c consumes via `HolderRecord::from_hop_capability` reference.
- All 11 test vectors live in this sub-mission. Sub-mission 0970-b has NO test vectors — only `ForwardRequestPayload` extension + RFC-0870 §Roles cross-reference update.
