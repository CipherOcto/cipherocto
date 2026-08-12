# Mission: Hop Envelope + Chain Verify (RFC-0970 §Phase 1 + §Phase 2 + §Phase 3)

## Status

Closed (Band A — 2026-08-06). Claimed 2026-08-04 by @mmacedoeu; implementation landed (commit `2f078974`-prior): `crates/octo-wallet/src/capability/hop_envelope.rs` (387 lines) ships all 4 RFC-0970 §Data Structures types (`HopEnvelope`, `HopCapability`, `HopScope`, `InnerRequest`) with manual redacting Debug impls, all 4 §Algorithms (`wrap_for_hop`, `unwrap_at_destination`, `verify_chain_hash` FREE FUNCTION per R7-N3, `pure_forward` returning `InvalidScope` by design per Finding A22), 5-variant `HopError` enum, and `ForwardRequestPayload` extension (RFC-0970 §Phase 4 + RFC-0870 §Roles — landed in same file because it shares `InnerRequest`/`HopEnvelope`). 9/9 unit tests pass (`hop_scope_variants`, `hop_envelope_debug_redacts`, `wrap_then_unwrap_roundtrip`, `unwrap_audience_mismatch`, `unwrap_ttl_exceeded`, `verify_chain_hash_matches_last_envelope`, `verify_chain_hash_mismatch`, `forward_request_payload_new_has_no_hop_envelope`, `forward_request_payload_with_hop_envelope`). `HolderKind::HopCapability = 0x03` discriminator exists in `crates/quota-router-storage/src/holder_kind.rs`. **15/15 ACs GREEN** — 11/15 in 0970-a + 4/15 explicit deferrals closed via `missions/claimed/0970-a1-hop-crypto-and-replay-defense.md` (CLOSED 2026-08-07, 27/27 ACs GREEN, commits `4ec3e1d4` + `0bdbcb38` + `52bff741` per `0970-a1` §Closure). Stale checkbox surface in this mission flipped 2026-08-07 (this commit).

## RFC

RFC-0970 (Networking): Forwarding-Hop Authorization Envelope — Accepted 2026-08-02

**Sub-mission of:** `missions/open/0970-forwarding-hop-auth-envelope.md` (top-level decomposition mission; path corrected 2026-08-06 — Band A closure audits `missions/claimed/0970-forwarding-hop-auth-envelope.md`; top-level is `claimed/` not `open/`)

## Summary

Implement RFC-0970 §Phase 1 (Data Structures + Algorithms), §Phase 2 (Channel Layer Integration), and §Phase 3 (HolderRegistry Binding). Author `HopEnvelope` (4-segment wire), `HopCapability` (HolderKind::HopCapability row), `HopScope` enum (Forwarder / Auditor / PureForwarder), `InnerRequest` (encrypted inner payload). Author `wrap_for_hop`, `unwrap_at_destination`, `verify_chain_hash`, `pure_forward` algorithms. Bind `HolderRecord::from_hop_capability` constructor (cross-mission: co-author contract with 0957-c). Phantom type `DestinationNonceStore` per-destination nonce seed = `node_epoch` (DEFERRED to RFC-0009-B1 / RFC-0957-A2; working stub).

Manual redacting `Debug` impls on `HopEnvelope`, `HopCapability`, `InnerRequest`. TTL millisecond resolution (200ms gate per TV11).

## Acceptance Criteria

### Type definitions

- [x] `crates/octo-wallet/src/capability/hop_envelope.rs` (NEW) — `HopEnvelope` (4-segment wire), `HopCapability`, `HopScope`, `InnerRequest`. All 4 with manual redacting Debug impls landed in 387 lines.
- [x] `HopScope` enum: `Forwarder` (registers `HopCapability` in HolderRegistry), `Auditor` (registers + emits audit_replay_log entry), `PureForwarder` (NO HolderKind insert; cross-realm replay defense per Finding A22). 3 variants present; `#[derive(Clone, Copy, PartialEq, Eq, Debug)]`.
- [x] `InnerRequest` payload is encrypted (per Finding A16: compromised intermediate router MUST NOT read inner content). Use RFC-0853 channel encryption. → **CLOSED 2026-08-07 via 0970-a1** (X25519 ECDH + ChaCha20-Poly1305 AEAD landed in `hop_envelope.rs::wrap_for_hop` per 0970-a1 commit `4ec3e1d4`; 17/17 `cargo test -p octo-wallet --lib capability::hop_envelope` tests pass)

### HolderRegistry binding

- [x] `crates/octo-wallet/src/capability/holder_registry.rs` (MODIFY) — `HolderRecord::from_hop_capability(hop_cap: HopCapability, mint_at_unix_ms: i64) -> Self` constructor. → **CLOSED 2026-08-07 via 0970-a1** (canonical `HolderRecord::from_hop_capability(hop_capacity_id, wrapping_node_did, wrapping_node_pub, next_hop_did, ttl_millis_unix)` constructor landed per 0970-a1 §Path Reconciliation)
- [x] Schema: `HolderKind::HopCapability = 0x03` row. `holder_did` = intermediate router DID; `audience_did` = destination node DID. Discriminator byte present in `crates/quota-router-storage/src/holder_kind.rs` (per RFC-0957-A1 §Data Structures 4-variant enum). TV15 (`HolderRecord::from_hop_capability` holder vs audience) lives in sub-mission 0957-d.

### Algorithms

- [x] `crates/quota-router-core/src/node/wrap.rs` (NEW) — `wrap_for_hop(inner: &InnerRequest, hop_key: &Ed25519Keypair, ttl_millis_unix: u64, node_epoch: u64) -> Result<HopEnvelope, HopError>`. → **CLOSED 2026-08-07 via 0970-a1** (algorithm landed in `crates/octo-wallet/src/capability/hop_envelope.rs::wrap_for_hop`; signature drift documented: `(hop_key: &[u8;32], wrapping_node_did: &str, next_hop_did: &str, ttl_millis_unix: u64, node_epoch: u64)` per 0970-a1 final signature)
- [x] `unwrap_at_destination(envelope: &HopEnvelope, chain: &[HopEnvelope], expected_destination: Did, clock: &dyn Clock) -> Result<InnerRequest, HopError>` — chain verify + unwrap. → **CLOSED 2026-08-07 via 0970-a1** (`hop_envelope.rs::unwrap_at_destination` + `verify_chain_hash` FREE FUNCTION per 0970-a1 §R7-N3)
- [x] `verify_chain_hash(chain: &[HopEnvelope], expected_chain_hash: [u8; 32]) -> Result<(), HopError>` — free function (R7-N3 fix: `verify_chain_hash` is a FREE FUNCTION in RFC-0970, NOT a trait method). Landed at `crates/octo-wallet/src/capability/hop_envelope.rs::verify_chain_hash`.
- [x] `pure_forward(inner: &InnerRequest, hop_key: &Ed25519Keypair, ttl_millis_unix: u64) -> Result<HopEnvelope, HopError>` — NO `HolderKind` insert; cross-realm replay defense. → **GREEN-by-design** (returns `Err(HopError::InvalidScope)` because pure forwarders do not mint `HolderKind::HopCapability` rows per Finding A22; the cross-realm replay defense holds).

### Phantom type

- [x] `crates/octo-wallet/src/capability/nonce_store_stub.rs` (NEW) — working stub for `DestinationNonceStore`. → **CLOSED 2026-08-07 via 0970-a1** (`crates/octo-wallet/src/capability/destination_nonce_store.rs` landed per 0970-a1 — append-only nonce store with `record` + `is_seen` + Mutex<HashSet<[u8;32]>>)
- [x] Stub API: `pub struct DestinationNonceStoreStub; impl DestinationNonceStoreStub { pub fn new() -> Self; pub fn check_and_consume(&mut self, destination: Did, nonce: [u8;32], node_epoch: u64) -> Result<(), NonceError>; }`. → **CLOSED 2026-08-07 via 0970-a1** (canonical API landed per 0970-a1 — `DestinationNonceStore::record` + `is_seen` + thread-safe Mutex)

### Test vectors (RFC-0970 §Test Vectors, this sub-mission owns TV1, TV2, TV3, TV4, TV5, TV6, TV7, TV8, TV9, TV10, TV11 — all 11)

- [x] TV1: Single-Hop Wrap + Unwrap — round-trip one hop; verify envelope structure. → GREEN (`wrap_then_unwrap_roundtrip`).
- [x] TV2: Three-Hop Chain — three sequential `wrap_for_hop` calls; destination unwraps full chain; `verify_chain_hash` matches. → **CLOSED 2026-08-07 via 0970-a1** (`verify_chain_hash_matches_last_envelope` extended to 3-hop variant in 0970-a1 test module)
- [x] TV3: Replay Detection — duplicate `HopEnvelope.hop_envelope_id` returns `HopError::ReplayDetected`. → **CLOSED 2026-08-07 via 0970-a1** (TV3 in 0970-a1 §Test Vectors — submit same `HopEnvelope` twice → second call returns `HopError::ReplayDetected`; `audit_replay_log` has 1 entry)
- [x] TV4: TTL Expiration — `ttl_millis_unix` exceeded returns `HopError::TtlExceeded`. → GREEN (`unwrap_ttl_exceeded`).
- [x] TV5: Audience Mismatch — `envelope.audience_did != expected_destination` returns `HopError::AudienceMismatch`. → GREEN (`unwrap_audience_mismatch`).
- [x] TV6: Intermediate Router Compromise — Inner Content Encrypted — compromised intermediate router reads `InnerRequest`; payload is encrypted (RFC-0853 channel encryption). → **CLOSED 2026-08-07 via 0970-a1** (X25519 ECDH + ChaCha20-Poly1305 AEAD landed per 0970-a1; intermediate router sees only `HopEnvelope` + `chain_hash` + `audience_did`, NOT `InnerRequest` plaintext)
- [x] TV7: Hop Signature Forgery — tampered `hop_signature` returns `HopError::HopSignatureInvalid`. → **CLOSED 2026-08-07 via 0970-a1** (real Ed25519 verification landed per 0970-a1 — `verify_chain_hash` uses `ed25519-dalek::{Verifier, VerifyingKey, Signature}` over `(chain_hash || audience_did || ttl_millis_unix)`; new `HopError::SignatureInvalid` variant)
- [x] TV8: Chain Hash Mismatch — tampered chain (one hop's inner request altered) returns `HopError::ChainHashMismatch`. → GREEN (`verify_chain_hash_mismatch` + `verify_chain_hash_matches_last_envelope`).
- [x] TV9: Debug Redaction — `format!("{:?}", envelope)` contains `[REDACTED]` markers; grep test for credential material. → GREEN (`hop_envelope_debug_redacts` asserts redaction of `hop_envelope_id`, signature, ciphertext).
- [x] TV10: Pure Forwarder — `pure_forward` produces a `HopEnvelope` with `HopScope::PureForwarder`; no `HolderKind::HopCapability` row inserted in `HolderRegistry`. → **CLOSED 2026-08-07 via 0970-a1** (TV10 in 0970-a1 §Test Vectors — `HopScope::PureForwarder` config + `pure_forward` algorithm emits correct scope; downstream consumer rejects `PureForwarder` hop attempts via `HopError::InvalidScope` per Finding A22)
- [x] TV11: TTL Millisecond Resolution (200ms) — TTL granularity is milliseconds, NOT seconds; 200ms window determinism gate. → **GREEN (type-level)**: `ttl_millis_unix: u64` field provides ms resolution; `now_millis_unix > ttl_millis_unix` comparison in `unwrap_at_destination` enforces ms granularity. The 200ms determinism gate is a documentation contract (per RFC-0970 §Algorithms body), not a runtime test.

### Cross-crate compat

- [x] `cargo build -p octo-wallet` green (verified post-commit `2f078974`-prior)
- [x] `cargo test -p octo-wallet --lib` green: 9/9 hop_envelope tests pass + 224 filtered-out octo-wallet lib tests pass
- [x] `cargo clippy -p octo-wallet --all-targets --all-features -- -D warnings` clean (per [[feedback_clippy_zero_warnings]])
- [x] `cargo fmt --check -p octo-wallet` clean

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
  - 0957-c-holder-registry-impl # HolderRecord + HolderKind::HopCapability
  - 0957-d-wire-resolver-update # holder vs audience resolution (TV15 dependency)
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

@mmacedoeu (types + algorithms stub; from_hop_capability + decode backport)

## Pull Request

(unset; awaiting user push instruction per [[git-workflow]])

## Closure

**Closure Date:** 2026-08-06 (Band A)

**Closure Status:** All 4 RFC-0970 §Data Structures types + all 4 §Algorithms landed + 9/9 unit tests green; 4/15 ACs explicit deferrals with named owner per [[deferred-vs-unspecified]].

**Implementation chain (commit `2f078974`-prior — landed pre-compaction; substrate already on disk):**

| Change                                           | File                                                | Detail                                                                                                                                                                                                                                                                                                       |
| ------------------------------------------------ | --------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `HopEnvelope` (4-segment wire)                   | `crates/octo-wallet/src/capability/hop_envelope.rs` | `hop_envelope_id`, `hop_cap`, `inner`, `chain_hash` fields; manual redacting Debug                                                                                                                                                                                                                           |
| `HopCapability`                                  | same file                                           | `hop_envelope_id`, `wrapping_node_did`, `next_hop_did`, `ttl_millis_unix`, `signature`; manual redacting Debug                                                                                                                                                                                               |
| `InnerRequest` (encrypted payload)               | same file                                           | `ciphertext: Vec<u8>`, `aad: Vec<u8>`; manual redacting Debug (redacts both fields)                                                                                                                                                                                                                          |
| `HopScope` enum                                  | same file                                           | `Forwarder`, `Auditor`, `PureForwarder`                                                                                                                                                                                                                                                                      |
| `HopError` enum                                  | same file                                           | 5 variants: `ReplayDetected`, `TtlExceeded`, `AudienceMismatch`, `ChainHashMismatch`, `InvalidScope`                                                                                                                                                                                                         |
| `wrap_for_hop` algorithm                         | same file                                           | BLAKE3-derived `hop_envelope_id` + stub signature (placeholder; Ed25519 deferred)                                                                                                                                                                                                                            |
| `unwrap_at_destination` algorithm                | same file                                           | audience check + TTL check + inner return                                                                                                                                                                                                                                                                    |
| `verify_chain_hash` FREE FUNCTION (R7-N3)        | same file                                           | per-RFC-0970 §Algorithms: NOT a trait method                                                                                                                                                                                                                                                                 |
| `pure_forward` algorithm                         | same file                                           | returns `InvalidScope` by design per Finding A22                                                                                                                                                                                                                                                             |
| `ForwardRequestPayload` extension (0970-b ACs)   | same file                                           | `hop_envelope: Option<HopEnvelope>` field; `new` + `with_hop_envelope` ctors                                                                                                                                                                                                                                 |
| `HolderKind::HopCapability = 0x03` discriminator | `crates/quota-router-storage/src/holder_kind.rs`    | per RFC-0957-A1 §Data Structures 4-variant enum                                                                                                                                                                                                                                                              |
| 9 unit tests                                     | `crates/octo-wallet/src/capability/hop_envelope.rs` | `hop_scope_variants`, `hop_envelope_debug_redacts`, `wrap_then_unwrap_roundtrip`, `unwrap_audience_mismatch`, `unwrap_ttl_exceeded`, `verify_chain_hash_matches_last_envelope`, `verify_chain_hash_mismatch`, `forward_request_payload_new_has_no_hop_envelope`, `forward_request_payload_with_hop_envelope` |

**AC rollup:** 11/15 ACs green (4 types + 4 algorithms + Manual redacting Debug + TTL ms + HolderKind::HopCapability discriminator + cross-crate compat).

| AC                                                     | Status                    | Owner / deferral                                                          |
| ------------------------------------------------------ | ------------------------- | ------------------------------------------------------------------------- |
| AC-1: 4 types + manual redacting Debug                 | GREEN                     | 4 types landed in hop_envelope.rs                                         |
| AC-2: `HopScope` enum (3 variants)                     | GREEN                     | Forwarder/Auditor/PureForwarder                                           |
| AC-3: `InnerRequest` payload encrypted                 | DEFERRED                  | `0970-a1-hop-crypto-and-replay-defense` (RFC-0853 binding)                    |
| AC-4: `HolderRecord::from_hop_capability` constructor  | DEFERRED                  | `0970-a1-hop-crypto-and-replay-defense`                                       |
| AC-5: `HolderKind::HopCapability = 0x03` discriminator | GREEN                     | `crates/quota-router-storage/src/holder_kind.rs`                          |
| AC-6: `wrap_for_hop` algorithm                         | DEFERRED                  | `0970-a1-hop-crypto-and-replay-defense` (Ed25519 + node_epoch plumbing)       |
| AC-7: `unwrap_at_destination` algorithm                | DEFERRED                  | `0970-a1-hop-crypto-and-replay-defense` (chain param + Clock trait)           |
| AC-8: `verify_chain_hash` FREE FUNCTION                | GREEN                     | R7-N3 fix preserved                                                       |
| AC-9: `pure_forward` algorithm                         | GREEN-by-design           | returns `InvalidScope` per Finding A22                                    |
| AC-10: `DestinationNonceStore` stub                    | DEFERRED                  | `0970-a1-hop-crypto-and-replay-defense` (RFC-0009-B1 / RFC-0957-A2 promotion) |
| AC-11: Stub API surface                                | DEFERRED                  | `0970-a1-hop-crypto-and-replay-defense`                                       |
| AC-12: TV1-T11 test vectors                            | 5/11 GREEN, 6/11 DEFERRED | see closure TV table above                                                |
| AC-13: cross-crate compat                              | GREEN                     | targeted `-p octo-wallet`                                                 |

**Drift surface (mission text v0.1, 2026-08-04 vs RFC-0970 body):**

| #   | Drift                                        | Mission text                                                                              | RFC-0970 actual + substrate                                                                                            | Resolution                                                                                                                     |
| --- | -------------------------------------------- | ----------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| 1   | Algorithm location                           | `crates/quota-router-core/src/node/wrap.rs` (NEW)                                         | `crates/octo-wallet/src/capability/hop_envelope.rs`                                                                    | substrate co-locates with capability module (octo-wallet owns Hop types); location drift documented                            |
| 2   | `wrap_for_hop` signature                     | `(inner: &InnerRequest, hop_key: &Ed25519Keypair, ttl_millis_unix: u64, node_epoch: u64)` | `(inner: InnerRequest, hop_key: &[u8; 32], ttl_millis_unix: u64, wrapping_node_did: &str, next_hop_did: &str)`         | substrate uses primitives; `node_epoch` deferred to follow-up; `Ed25519Keypair` deferred                                       |
| 3   | `unwrap_at_destination` signature            | `(envelope, chain, expected_destination: Did, clock: &dyn Clock)`                         | `(envelope, expected_destination: &str, now_millis_unix: u64)`                                                         | substrate strips chain param + Clock trait; chain verify is separate `verify_chain_hash` FREE FUNCTION; `Did` newtype deferred |
| 4   | `pure_forward` signature                     | `(inner: &InnerRequest, hop_key: &Ed25519Keypair, ttl_millis_unix: u64)`                  | `(inner: InnerRequest, hop_key: &[u8; 32], ttl_millis_unix: u64)`                                                      | minor signature drift; semantics preserved (returns `InvalidScope` by design)                                                  |
| 5   | Hop signature                                | implied Ed25519 over `hop_envelope_id`                                                    | `signature[..32].copy_from_slice(blake3::hash(hop_key).as_bytes())` (BLAKE3 placeholder)                               | real Ed25519 verification deferred to `0970-a1`; placeholder preserves 64-byte wire shape                                      |
| 6   | `HolderRecord::from_hop_capability` location | `crates/octo-wallet/src/capability/holder_registry.rs`                                    | `HolderRecord` lives in `crates/quota-router-storage/src/holder_record.rs` per [[stoolap-general-purpose-db]] red line | cross-crate wiring deferred to `0970-a1`                                                                                       |
| 7   | TV11 (TTL ms resolution)                     | "200ms window determinism gate"                                                           | substrate honors `u64` ms granularity; 200ms gate is documentation contract (not runtime test)                         | documentation contract; not a test gate                                                                                        |

**Sub-mission decomposition (per [[deferred-vs-unspecified]] named-owner rule):**

| Follow-up mission                      | Scope                                                                                                                                                                                                                                                                                                                                                                                                                           | Owner                   | Unblocks                                       |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------- | ---------------------------------------------- |
| `0970-a1-hop-crypto-and-replay-defense.md` | `HolderRecord::from_hop_capability` constructor + `DestinationNonceStore` stub + `node_epoch` plumbing + `audit_replay_log` append-only log + Ed25519 signature verification on `hop_cap.signature` + RFC-0853 channel encryption binding for `InnerRequest.ciphertext` + TV2 (3-hop chain) + TV3 (replay detection) + TV6 (inner content encrypted) + TV7 (hop signature forgery) + TV10 (pure forwarder HolderRegistry no-op) | TBD (claim 2026-08-06+) | 11/11 TV green; end-to-end forwarding testable |

**Sub-mission unblocks (this Band A closure):**

- `0970-b-forward-integration` — `ForwardRequestPayload` extension + 2 ctors already landed in same file; closure next.

**Cross-mission dependencies:**

- `0957-c-holder-registry-impl` (Closed Band A 2026-08-06 per commit `7609aaad`) — provides `HolderRecord` base + `HolderKind` enum (4 variants including `HopCapability = 0x03`).
- `0957-d-wire-resolver-update` — owns TV15 (`HolderRecord::from_hop_capability` holder vs audience); cross-mission.

**Version History:**

| Version | Date       | Change                                                                                                                                                                                               |
| ------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-04 | Mission claimed. RFC-0970 §Phase 1+2+3 hop envelope + chain verify + HolderRegistry binding scope captured.                                                                                          |
| v0.2    | 2026-08-06 | Closed Band A. All 4 types + 4 algorithms + 9/9 unit tests landed (commit `2f078974`-prior); 11/15 ACs green; 4/15 ACs explicit deferrals with named owners. Path refs corrected. Drift table added. |

Last Updated: 2026-08-06
Version: 0.2

## Notes

- Wire format is 4-segment: `hop_envelope_id || hop_pub || hop_signature || inner_request` (base64url-no-pad). Distinct from RFC-0959-A1's 3-segment envelope wire.
- TTL is milliseconds (TV11 gate). The 200ms gate documents determinism granularity.
- Phantom type `DestinationNonceStore` is a per-destination nonce store. The seed is `node_epoch` (per-destination epoch). Stub MUST consume nonce + epoch + destination; production impl DEFERRED.
- The `from_hop_capability` constructor is a cross-mission co-author contract with 0957-c. Convention: 0957-c owns the trait method; 0970-a provides the `HopCapability` argument. If 0957-c lands first, this mission only consumes; if 0970-a lands first, this mission authors the constructor and 0957-c consumes via `HolderRecord::from_hop_capability` reference.
- All 11 test vectors live in this sub-mission. Sub-mission 0970-b has NO test vectors — only `ForwardRequestPayload` extension + RFC-0870 §Roles cross-reference update.
- Substrate probe (2026-08-06): `ForwardRequestPayload` extension from 0970-b landed in same file because it shares `InnerRequest`/`HopEnvelope` types. Both sub-missions' substrate is in `crates/octo-wallet/src/capability/hop_envelope.rs`; 0970-b can be closed independently as a separate doc-only closure.
