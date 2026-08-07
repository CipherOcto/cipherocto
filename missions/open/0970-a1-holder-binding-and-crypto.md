# Mission: Holder Binding + Crypto + Test Vectors TV2/3/6/7/10 (RFC-0970 §Phase 1+2+3 follow-up)

## Status

Open (filed 2026-08-06 by mission `0970-a-hop-envelope.md` Band A closure). Per [[deferred-vs-unspecified]] named-owner rule, this follow-up mission owns the deferred `InnerRequest` RFC-0853 encryption + `HolderRecord::from_hop_capability` constructor + `DestinationNonceStore` stub + `node_epoch` + `audit_replay_log` + Ed25519 signature verification + TV2/TV3/TV6/TV7/TV10.

**Sub-mission of:** `missions/claimed/0970-a-hop-envelope.md` (Band A closed 2026-08-06; commit `11921128`).

## RFC

RFC-0970 (Networking): Forwarding-Hop Authorization Envelope — Accepted 2026-08-02

## Summary

Complete the deferred crypto surface of `crates/octo-wallet/src/capability/hop_envelope.rs`: RFC-0853 `InnerRequest` encryption (real X25519 + ChaCha20-Poly1305), `HolderRecord::from_hop_capability` constructor (cross-crate per [[stoolap-general-purpose-db]]), `DestinationNonceStore` stub for replay defense, `node_epoch: u64` for key rotation, `audit_replay_log` for forensics, Ed25519 signature verification (real, replacing the BLAKE3 placeholder), and TV2 (three-hop chain) + TV3 (replay) + TV6 (intermediate router compromise) + TV7 (hop signature forgery) + TV10 (pure forwarder).

The `0970-a` Band A closure deferred this work because (a) the real crypto (X25519 + ChaCha20-Poly1305 + Ed25519) requires the `cipherocto-crypto` substrate owned by RFC-0853, (b) `HolderRecord::from_hop_capability` is cross-crate wiring (octo-wallet ↔ quota-router-core), (c) `DestinationNonceStore` is a new substrate, and (d) the TVs require real signature forgery detection (Ed25519 verify) rather than BLAKE3 hash placeholder.

## Acceptance Criteria

### RFC-0853 InnerRequest encryption

- [ ] `crates/octo-wallet/src/capability/hop_envelope.rs` — `wrap_for_hop` performs real X25519 ECDH + ChaCha20-Poly1305 encryption (replaces the BLAKE3 placeholder). Keypair sourced from `cipherocto-crypto::X25519Keypair`.
- [ ] `unwrap_at_destination` performs real X25519 ECDH + ChaCha20-Poly1305 decryption.
- [ ] RFC-0853 §Cipher Suite reference in docstrings.

### HolderRecord binding

- [ ] `crates/quota-router-core/src/registry.rs` (or wherever `HolderRecord` lives per substrate) — `HolderRecord::from_hop_capability(hop_envelope: &HopEnvelope, holder_did: Did) -> Result<HolderRecord, HolderRecordError>` constructor.
- [ ] Cross-crate per [[stoolap-general-purpose-db]] — HolderRecord stays cipherocto-side (no stoolap fork PR).

### DestinationNonceStore

- [ ] `crates/octo-wallet/src/capability/destination_nonce_store.rs` (NEW) — append-only nonce store; `record(nonce: [u8; 32]) -> Result<(), NonceError>` (rejects duplicates); `is_seen(nonce: &HopEnvelope) -> bool`.
- [ ] `HopError::ReplayDetected` already exists from 0970-a substrate; wire the store check at `unwrap_at_destination`.

### node_epoch + audit_replay_log

- [ ] `node_epoch: u64` field on `HopEnvelope` (already in substrate from 0970-a) — destination node rejects envelopes with stale epoch (`HopError::ChainHashMismatch` or new `HopError::StaleEpoch`).
- [ ] `audit_replay_log.rs` (NEW) — append-only log of replay detections: `envelope_id`, `nonce`, `node_did`, `at_millis_unix`. Manual redacting Debug per RFC-0957-A1 §Security.

### Ed25519 signature verification

- [ ] `verify_chain_hash` (or new `verify_hop_signature` if more appropriate) performs real Ed25519 verification over `(chain_hash || audience_did || ttl_millis_unix)` using `cipherocto-crypto::Ed25519PublicKey` + `ed25519-dalek` substrate.
- [ ] Replaces the BLAKE3 hash placeholder from 0970-a substrate.

### Test vectors (RFC-0970 §Test Vectors)

- [ ] TV2: Three-Hop Chain — wrap at origin → unwrap at hop 1 → re-wrap for hop 2 → unwrap at hop 2 → re-wrap for hop 3 → unwrap at destination. Assert: each unwrap recovers `InnerRequest` canonical bytes identical to origin.
- [ ] TV3: Replay Detection — submit same `HopEnvelope` twice to destination; second submission returns `HopError::ReplayDetected`. Audit log has 1 entry.
- [ ] TV6: Intermediate Router Compromise — hop 1 has access only to `HopEnvelope` + `chain_hash` + `audience_did` for hop 2 (NOT `InnerRequest` plaintext); assert: hop 1's `unwrap_at_destination` for hop 2 envelope returns `HopError::AudienceMismatch` (or appropriate).
- [ ] TV7: Hop Signature Forgery — submit `HopEnvelope` with mutated `chain_hash` (forged); destination's Ed25519 verify returns `HopError::ChainHashMismatch`. (Replaces 0970-a's BLAKE3 placeholder test.)
- [ ] TV10: Pure Forwarder — `HopScope::PureForwarder` config + `pure_forward` algorithm emits correct scope; downstream consumer rejects `PureForwarder` hop attempts (`HopError::InvalidScope` per Finding A22).

### Cross-crate compat

- [ ] `cargo build --workspace` green
- [ ] `cargo test --workspace --lib` green (existing 233+ tests + 5 new TV2/3/6/7/10 tests = 238+ total)
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean (per [[feedback_clippy_zero_warnings]]); workspace-level pre-existing `tdlib-rs` build error excluded from this AC
- [ ] `cargo fmt --check --workspace` clean

## Dependencies

**Requires (RFC gates):**

- RFC-0853 — Overlay Cryptography (X25519 + ChaCha20-Poly1305 substrate + Ed25519)
- RFC-0862 — Gossip Substrate (optional; consumed for cross-node `DestinationNonceStore` sync)
- RFC-0957-A1 — unified HolderRegistry (consumed by `HolderRecord::from_hop_capability`)

**Requires (mission gates):**

- `missions/claimed/0970-a-hop-envelope.md` (Band A closed 2026-08-06) — provides `HopEnvelope` + `InnerRequest` + `HopError` types consumed here
- `missions/claimed/0970-b-forward-integration.md` (Band A closed 2026-08-06) — provides `ForwardRequestPayload.hop_envelope: Option<HopEnvelope>` extension

```yaml
depends_on:
  - 0970-a-hop-envelope # HopEnvelope + InnerRequest + HopError substrate
  - 0970-b-forward-integration # ForwardRequestPayload.hop_envelope extension
  - 0957-c-holder-registry-impl # HolderRecord + HolderKind substrate
  - RFC-0853 # X25519 + ChaCha20-Poly1305 + Ed25519 substrate
```

## Location

- `crates/octo-wallet/src/capability/hop_envelope.rs` (MODIFY) — real X25519 + ChaCha20-Poly1305 + Ed25519 verification (replaces BLAKE3 placeholders)
- `crates/octo-wallet/src/capability/destination_nonce_store.rs` (NEW) — replay defense
- `crates/octo-wallet/src/capability/audit_replay_log.rs` (NEW) — audit trail
- `crates/quota-router-core/src/registry.rs` (MODIFY) — `HolderRecord::from_hop_capability` constructor

## Claimant

TBD (claim 2026-08-06+)

## Notes

- The BLAKE3 hash placeholder from `0970-a` substrate was an intentional "real crypto deferred" honesty. RFC-0853 substrate (X25519 + ChaCha20-Poly1305 + Ed25519) is available per [[feedback_cipherocto-crypto]]; the implementation swaps the placeholder.
- `DestinationNonceStore` is a new substrate; consider whether it belongs in `octo-wallet` (per-hop destination node state) or `quota-router-core` (per-node global state). Default: `octo-wallet/src/capability/destination_nonce_store.rs` since it's per-envelope-type state.
- `HolderRecord::from_hop_capability` is the canonical substrate-level binding for RFC-0970 hop envelopes into the RFC-0957-A1 unified HolderRegistry. Required for TV2/3/6/7/10 because each hop creates a HolderRecord on the destination node's registry.
- TV11 (TTL millisecond resolution 200ms) was flipped GREEN by 0970-a Band A closure (doc-contract, not test) — no follow-up needed.
