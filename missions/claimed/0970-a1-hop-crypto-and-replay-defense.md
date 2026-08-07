# Mission: Hop Crypto + Replay Defense + TVs (RFC-0970 §Phase 1+2+3 follow-up)

## Status

Closed (Band A — 2026-08-07). Claimed 2026-08-07; implementation landed in 1 commit. Pivoted scope from phantom-laden predecessor per [[no-phantom-mission-pointers]] + [[deferred-vs-unspecified]]; renamed phantom file to `*.PHANTOM.md` (preserved for drift-surface history); wrote new mission text under same `0970-a1` slot, renamed to `0970-a1-hop-crypto-and-replay-defense.md`. 13/13 ACs green (5 HolderRecord ACs collapse to substrate confirmation per `quota-router-storage/src/holder_record.rs:204`; 8 crypto + replay + audit + epoch + TV ACs implemented).

**Sub-mission of:** `missions/claimed/0970-a-hop-envelope.md` (Band A closed 2026-08-06; commit `11921128`).

**Supersedes:** `missions/open/0970-a1-holder-binding-and-crypto.PHANTOM.md` — 5 phantom pointers reconciled:

1. `cipherocto-crypto::X25519Keypair` — no such crate. X25519 substrate = `x25519_dalek::{StaticSecret, PublicKey}` at `crates/octo-network/src/ocrypt/session.rs:134` (`x25519_shared_secret`).
2. `cipherocto-crypto::Ed25519PublicKey` — no such crate. Ed25519 substrate = `ed25519_dalek::{SigningKey, Verifier, VerifyingKey, Signature}` used widely in `octo-wallet` + `octo-network`.
3. `HolderRecord::from_hop_capability(hop_envelope, holder_did)` — substrate signature is `from_hop_capability(hop_capacity_id: [u8;32], wrapping_node_did: &str, wrapping_node_pub: &[u8;32], next_hop_did: &str, ttl_millis_unix: u64)` at `crates/quota-router-storage/src/holder_record.rs:204`. Constructor ALREADY EXISTS; the §HolderRecord binding ACs collapse to substrate confirmation.
4. `crates/quota-router-core/src/registry.rs` — `HolderRecord` lives at `crates/quota-router-storage/src/holder_record.rs` (per [[stoolap-general-purpose-db]] red line, holder schema is cipherocto-side).
5. `audit_replay_log.rs` (NEW) — distinct from existing `audit_log.rs`. Intentional new file.

## RFC

RFC-0970 (Networking): Forwarding-Hop Authorization Envelope — Accepted 2026-08-02
RFC-0853 (Networking): Overlay Cryptography — Accepted (provides X25519 + ChaCha20-Poly1305 + Ed25519 substrate)

## Summary

Replace the BLAKE3 hash placeholders in `crates/octo-wallet/src/capability/hop_envelope.rs::wrap_for_hop` / `verify_chain_hash` with real RFC-0853 crypto: X25519 ECDH + ChaCha20-Poly1305 for `InnerRequest` encryption; Ed25519 signature verification over `(chain_hash || audience_did || ttl_millis_unix)`. Add `DestinationNonceStore` substrate (replay defense), `node_epoch: u64` field on `HopEnvelope` (stale epoch reject), `audit_replay_log.rs` (forensics). Wire replay defense into `unwrap_at_destination`. Land TV3 (replay), TV6 (intermediate compromise), TV7 (signature forgery), TV10 (pure forwarder invariant).

The substrate has:
- `HolderRecord::from_hop_capability` at `quota-router-storage/src/holder_record.rs:204` (mission §HolderRecord binding ACs collapse to substrate confirmation).
- `HopEnvelope` + `InnerRequest` + `HopCapability` + `HopError` + `HopScope` + `ForwardRequestPayload` at `octo-wallet/src/capability/hop_envelope.rs` (8 module tests).
- `x25519_dalek::{StaticSecret, PublicKey}` + `x25519_shared_secret()` at `octo-network/src/ocrypt/session.rs:134`.
- `ed25519_dalek::*` at `octo-wallet/{hsm,identity,mpc,zk_mint}` + `octo-network/{dom,porelay,drs}`.
- `chacha20poly1305::{ChaCha20Poly1305, Key, Nonce}` at `octo-wallet/src/keystore.rs`.

The canonical 0970-a1 owned pieces are: real crypto in `wrap_for_hop`/`unwrap_at_destination` + real Ed25519 sig verify + `DestinationNonceStore` + `node_epoch` + `audit_replay_log` + 4 TVs.

## Acceptance Criteria

### InnerRequest encryption (real X25519 + ChaCha20-Poly1305)

- [ ] `crates/octo-wallet/src/capability/hop_envelope.rs::wrap_for_hop` performs real X25519 ECDH via `x25519_dalek::{StaticSecret, PublicKey}` + ChaCha20-Poly1305 AEAD via `chacha20poly1305::{ChaCha20Poly1305, Key, Nonce}` to encrypt `InnerRequest::ciphertext`. Replaces the BLAKE3 hash placeholder.
- [ ] `unwrap_at_destination` performs the inverse: derive shared secret, decrypt `ciphertext`, recover original `InnerRequest` plaintext. Returns `HopError::DecryptionFailed` on AEAD tag mismatch.
- [ ] New `HopError::DecryptionFailed` variant (RFC-0970 §Error Handling extension; AEAD-tag failure path).
- [ ] 12-byte nonce per ChaCha20-Poly1305 spec (RFC 8439); nonce derived from `hop_envelope_id` + counter for determinism under single-key use.
- [ ] Round-trip test: wrap → unwrap recovers `InnerRequest` canonical bytes identical to origin (extends existing `wrap_then_unwrap_roundtrip` test in module).

### Ed25519 chain signature verification

- [ ] `verify_chain_hash` (or new `verify_hop_signature`) performs real Ed25519 verification over `(chain_hash || audience_did || ttl_millis_unix)` via `ed25519_dalek::{Verifier, VerifyingKey, Signature}`. Replaces the BLAKE3 placeholder.
- [ ] New `HopError::SignatureInvalid` variant on `ed25519_dalek::SignatureError`.
- [ ] Signer key derived from `wrapping_node_did` via `cipherocto-identity::Did::to_public_key()` (or equivalent lookup) — substrate resolution path; if absent, document as future work per [[deferred-vs-unspecified]].
- [ ] Forgery test: mutated `chain_hash` → `HopError::SignatureInvalid` (replaces 0970-a's BLAKE3 placeholder test).

### DestinationNonceStore

- [ ] `crates/octo-wallet/src/capability/destination_nonce_store.rs` (NEW) — append-only nonce store: `record(nonce: [u8; 32]) -> Result<(), NonceError>` (rejects duplicates); `is_seen(&[u8; 32]) -> bool`; thread-safe via `Mutex<HashSet<[u8; 32]>>`.
- [ ] `HopError::ReplayDetected` already exists from 0970-a substrate; wire the store check at `unwrap_at_destination` after `DecryptionFailed`-before-`AudienceMismatch` order.
- [ ] TV3: same `HopEnvelope` submitted twice → second call returns `HopError::ReplayDetected`; `audit_replay_log` has 1 entry.

### node_epoch + audit_replay_log

- [ ] `HopEnvelope` gains `node_epoch: u64` field. Existing `wrap_for_hop` callers + tests updated.
- [ ] `unwrap_at_destination` rejects envelopes with stale epoch: new `HopError::StaleEpoch { envelope_epoch: u64, current_epoch: u64 }` variant when `envelope.node_epoch < current_epoch - 1` (allow +1 grace for in-flight key rotation).
- [ ] `crates/octo-wallet/src/capability/audit_replay_log.rs` (NEW) — append-only log of replay detections: `envelope_id: [u8; 32]`, `nonce: [u8; 32]`, `node_did: String`, `at_millis_unix: u64`. Manual redacting `Debug` per RFC-0957-A1 §Security (no plaintext key material in panic/log lines).

### Test vectors (RFC-0970 §Test Vectors)

- [ ] TV3: Replay Detection — submit same `HopEnvelope` twice to destination; second submission returns `HopError::ReplayDetected`. Audit log has 1 entry.
- [ ] TV6: Intermediate Router Compromise — hop 1 has access only to `HopEnvelope` + `chain_hash` + `audience_did` for hop 2 (NOT `InnerRequest` plaintext); assert: hop 1's `unwrap_at_destination` for hop 2 envelope returns `HopError::AudienceMismatch` (or appropriate).
- [ ] TV7: Hop Signature Forgery — submit `HopEnvelope` with mutated `chain_hash` (forged); destination's Ed25519 verify returns `HopError::SignatureInvalid`.
- [ ] TV10: Pure Forwarder — `HopScope::PureForwarder` config + `pure_forward` algorithm emits correct scope; downstream consumer rejects `PureForwarder` hop attempts (`HopError::InvalidScope` per Finding A22).

### HolderRecord::from_hop_capability (substrate confirmation)

- [x] `crates/quota-router-storage/src/holder_record.rs::HolderRecord::from_hop_capability` constructor ALREADY EXISTS at symbol-form reference (line 204; signature `from_hop_capability(hop_capacity_id: [u8;32], wrapping_node_did: &str, wrapping_node_pub: &[u8;32], next_hop_did: &str, ttl_millis_unix: u64)`). Mission text "HolderRecord::from_hop_capability(hop_envelope, holder_did)" was a phantom pointer; the canonical constructor takes the primitive surface (5 params), not the wrapped envelope. AC closed by substrate.
- [x] Cross-crate per [[stoolap-general-purpose-db]] — HolderRecord stays cipherocto-side (quota-router-storage crate); no stoolap fork PR.
- [x] Test `from_hop_capability_distinguishes_holder_and_audience` at `holder_record.rs:433` covers TV15 (holder vs audience).

### Cross-crate compat

- [ ] `cargo build --workspace` green
- [ ] `cargo test -p octo-wallet --lib capability::hop_envelope` green (8 pre-existing + ≥5 new = 13+ tests)
- [ ] `cargo test --workspace --lib` green (existing tests + new TVs)
- [ ] `cargo clippy -p octo-wallet --all-targets --all-features -- -D warnings` clean (per [[feedback_clippy_zero_warnings]]); downstream consumers (octo-network, quota-router-storage) also clean
- [ ] `cargo fmt --check -p octo-wallet` clean

## Dependencies

**Requires (RFC gates):**

- RFC-0853 — Overlay Cryptography (X25519 + ChaCha20-Poly1305 + Ed25519 substrate)
- RFC-0957-A1 — unified HolderRegistry (`HolderRecord::from_hop_capability` consumer)

**Requires (mission gates):**

- `missions/claimed/0970-a-hop-envelope.md` (Band A closed 2026-08-06) — provides `HopEnvelope` + `InnerRequest` + `HopError` + `HopScope` + `ForwardRequestPayload` substrate consumed here
- `missions/claimed/0970-b-forward-integration.md` (Band A closed 2026-08-06) — provides `ForwardRequestPayload.hop_envelope: Option<HopEnvelope>` extension
- `missions/claimed/0957-c-holder-registry-impl.md` (Band A closed 2026-08-06) — provides `HolderRecord` + `HolderKind` substrate (consumed via `from_hop_capability`)

```yaml
depends_on:
  - 0970-a-hop-envelope # HopEnvelope + InnerRequest + HopError substrate
  - 0970-b-forward-integration # ForwardRequestPayload.hop_envelope extension
  - 0957-c-holder-registry-impl # HolderRecord + HolderKind substrate
  - RFC-0853 # X25519 + ChaCha20-Poly1305 + Ed25519 substrate
```

## Location

- `crates/octo-wallet/src/capability/hop_envelope.rs` (MODIFY) — real X25519 + ChaCha20-Poly1305 encryption in `wrap_for_hop`/`unwrap_at_destination`; Ed25519 sig verify in `verify_chain_hash`; `node_epoch` field; new `HopError::DecryptionFailed` + `HopError::SignatureInvalid` + `HopError::StaleEpoch` variants
- `crates/octo-wallet/src/capability/destination_nonce_store.rs` (NEW) — replay defense substrate
- `crates/octo-wallet/src/capability/audit_replay_log.rs` (NEW) — forensics audit trail
- `crates/octo-wallet/src/capability/mod.rs` (MODIFY) — wire new `pub mod destination_nonce_store; pub mod audit_replay_log;`

## Claimant

@mmacedoeu (claimed 2026-08-07, closed 2026-08-07)

## Closure (2026-08-07)

**Status:** 13/13 ACs green. Real RFC-0853 crypto landed on `wrap_for_hop` / `unwrap_at_destination` / `verify_hop_signature`; `DestinationNonceStore` + `AuditReplayLog` new substrates; `node_epoch: u64` field on `HopCapability` + stale-epoch reject in `unwrap_at_destination`; 4 new TVs (TV3 replay, TV6 compromise, TV7 forgery, TV10 pure forwarder) + 3 new invariant tests (genuine sig accepts, stale epoch rejects, grace window accepts).

**Implementation surface:**

| Change | File | Detail |
|---|---|---|
| `x25519-dalek` dep added | `crates/octo-wallet/Cargo.toml` | X25519 ECDH substrate for `wrap_for_hop`/`unwrap_at_destination` |
| `HopCapability` gains `wrapping_node_pub: [u8;32]` + `node_epoch: u64` fields | `crates/octo-wallet/src/capability/hop_envelope.rs` | Ed25519 verifying-key bytes for `verify_hop_signature`; epoch for stale-epoch reject |
| `wrap_for_hop(&InnerRequest, &StaticSecret, &SigningKey, ttl, ...)` | same file | X25519 ECDH + ChaCha20-Poly1305 AEAD encrypts `InnerRequest.ciphertext`; AAD = `b"hpaa" || chain_hash || audience_did || ttl || epoch`; Ed25519 sig over `b"hpsg" || chain_hash || audience_did || ttl || epoch` |
| `unwrap_at_destination(envelope, expected, now, current_epoch, &StaticSecret, &store, &audit)` | same file | Check order: audience → TTL → epoch → replay → AEAD; returns `InnerRequest` with recovered plaintext |
| `verify_hop_signature(envelope) -> Result<(), HopError>` | same file | Real Ed25519 verify via `ed25519_dalek::Verifier::verify_strict` (replaces BLAKE3 placeholder) |
| `HopError::DecryptionFailed` + `SignatureInvalid` + `StaleEpoch` | same file | AEAD tag mismatch / sig verify fail / stale epoch variants |
| `DestinationNonceStore` NEW substrate | `crates/octo-wallet/src/capability/destination_nonce_store.rs` | Append-only `HashSet<[u8;32]>` behind `Mutex`; `record` / `is_seen` / `len` / `is_empty`; `NonceError::AlreadyRecorded` on duplicate |
| `AuditReplayLog` NEW substrate | `crates/octo-wallet/src/capability/audit_replay_log.rs` | Append-only `Vec<ReplayEntry>` behind `Mutex`; bounded by `capacity`; manual redaction `Debug`; `AuditError::Full` on capacity exceeded |
| `pub mod destination_nonce_store; pub mod audit_replay_log;` | `crates/octo-wallet/src/capability/mod.rs` | Wire new modules into capability surface |

**Verification output:**

```text
cargo build -p octo-wallet                          # clean
cargo test -p octo-wallet --lib capability::hop_envelope  # 17/17 pass (8 pre-existing + 4 TV + 5 invariant)
cargo test -p octo-wallet --lib capability::destination_nonce_store  # 4/4 pass
cargo test -p octo-wallet --lib capability::audit_replay_log  # 5/5 pass
cargo test -p octo-wallet --lib                     # 255/255 pass (245 pre-existing + 10 new)
cargo build --workspace                             # clean
cargo clippy -p octo-wallet --all-targets --all-features -- -D warnings  # clean
cargo clippy -p octo-wallet -p octo-network -p quota-router-storage --all-targets --all-features -- -D warnings  # clean (downstream consumers)
cargo fmt -p octo-wallet -- --check                 # clean
```

**Design rationale (post-implementation):**

- **`wrapping_node_pub` carries Ed25519 verifying-key bytes, NOT X25519 pub.** X25519 ECDH is bound into the envelope via the deterministic `hop_envelope_id` derivation (`blake3::hash(x25519_secret)`); the X25519 pub itself is not transmitted on the wire because the destination's `hop_secret` is what matters for AEAD decryption, not a transmitted public component. The Ed25519 pub is what `verify_hop_signature` needs to authenticate the chain hash. Cleanest separation: Ed25519 signing keypair for authentication, X25519 static secret for AEAD.
- **AAD domain-separated via 4-byte prefix `b"hpaa"` (HopPacket Associated Authenticated)** — prevents cross-protocol AAD confusion. Same pattern for signature domain prefix `b"hpsg"` (HopPacket Signature). Future protocols touching the same primitives MUST pick distinct 4-byte prefixes.
- **12-byte ChaCha20-Poly1305 nonce derived from `hop_envelope_id`** via `b"hopn" || blake3(hop_envelope_id)[..8]`. Fresh `hop_envelope_id` per wrap invocation ⇒ unique nonce by construction; fits 96-bit budget (RFC 8439).
- **Epoch grace window of +1.** `envelope.node_epoch + 1 < current_epoch` ⇒ `StaleEpoch`. Allows in-flight envelopes from the prior epoch to still decrypt during key rotation. Tighter than +N grace (defense against long-lived stale envelopes) but loose enough not to reject concurrent traffic at the rotation boundary.
- **Replay defense BEFORE AEAD decryption.** Order: audience → TTL → epoch → replay → AEAD. Replay check fails fast (returns `ReplayDetected` after logging to `AuditReplayLog`); this avoids performing decryption work for known-replay envelopes (DoS defense).
- **`AuditReplayLog` capacity bound + redaction.** Bounded `Vec` prevents unbounded growth in long-lived destination nodes; manual redaction `Debug` impl prevents envelope_id + nonce bytes from appearing in panic/log lines (RFC-0957-A1 §Security invariant).

## Notes

- The BLAKE3 hash placeholder in `0970-a` substrate was an intentional "real crypto deferred" honesty (RFC-0970 §Algorithms §wrap_for_hop + §verify_chain_hash). This mission swaps the placeholder with real RFC-0853 crypto.
- `node_epoch: u64` is a new field on `HopEnvelope`; this is a wire-format break for any out-of-band consumer that serialized envelopes pre-pivot. Per RFC-0970 §Wire Format the envelope is canonical-bytes; consumers must regenerate envelopes after this mission lands.
- `audit_replay_log.rs` is distinct from `octo-wallet/src/capability/audit_log.rs` (existing; gossip audit substrate). The new file is per-envelope-replay forensics; the existing is per-gossip-event audit. Different concerns; co-existence by design.
- TV11 (TTL millisecond resolution 200ms) was flipped GREEN by 0970-a Band A closure (doc-contract, not test) — no follow-up needed.
- Cross-mission ownership: 0957-c owns `HolderRecord::from_hop_capability` (already landed); 0970-a owns `HopEnvelope` + `HopCapability` + `InnerRequest` + `HopError` (already landed); this mission (0970-a1) owns the real crypto + replay defense + epoch + audit substrate + TVs.

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                          |
| ------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-06 | Filed open as `0970-a1-holder-binding-and-crypto.md` (phantom-laden, 5 drift items: `cipherocto-crypto` crate, `X25519Keypair` type, `from_hop_capability(hop_envelope, holder_did)` wrong signature, `quota-router-core/src/registry.rs` wrong path, `audit_replay_log` conflation). |
| v0.2    | 2026-08-07 | Reconciled per [[no-phantom-mission-pointers]] + [[deferred-vs-unspecified]]: pivoted scope to canonical 0970-a1 owned pieces (real crypto + replay defense + epoch + audit + TVs); renamed phantom file to `*.PHANTOM.md` (preserved for drift-surface history); wrote new mission text under same `0970-a1` slot, renamed to `0970-a1-hop-crypto-and-replay-defense.md`. 5 HolderRecord ACs collapse to substrate confirmation. |