# cache-bus-auth — producer_identity + signature on VaultProjectionInvalidationEnvelope

**Status:** claimed (2026-08-27)
**Substrate:** RFC-0960 §2.4 (invalidation bus) + RFC-0853 (overlay cryptography — provides `OverlayIdentity` newtype + ed25519-dalek sign/verify primitives)
**Parent:** R3 review follow-on (cache-bus auth gap finding)
**Depends on:**

- Mission B (`0960-v37-b-event-log-producer-wiring.md`) — `VaultProjectionInvalidationEnvelope` wire form must exist
- RFC-0853 §1 Cryptographic Primitives — ed25519 substrate for `Signer::sign` / `VerifyingKey::verify`
- RFC-0853 §8 Signature Model — signature envelope contract
- RFC-0009 — identity management substrate (the canonical process RFC)
- RFC-0010 — canonical DID codec (the wire-format for `OverlayIdentity`)

**Note:** earlier drafts of this mission cited RFC-0102 (wallet cryptography, Starknet ECDSA substrate) — that was a substrate-attribution error. RFC-0102 owns wallet-cryptography primitives, NOT the node-identity/DID substrate. The OverlayIdentity type is defined at `crates/octo-network/src/ocrypt/identity.rs` and re-exported at `crates/octo-cap-macaroon/src/caveat/mod.rs`, with the cryptographic primitives hosted in RFC-0853. R2 substrate-fidelity lens caught this.

## Motivation

`VaultProjectionInvalidationEnvelope` at `crates/octo-vault/src/event_log_producer.rs` is currently a 4-field struct (`chain_id`, `vault_id`, `asset_id`, `source_kind`). It is emitted over `cache:projection:<hex(vault_id)>` pub/sub channel with NO producer authentication. A compromised subscriber OR a rogue envelope emitter can invalidate arbitrary vault caches (denial-of-service attack on the projection layer) or impersonate a legitimate producer.

RFC-0960 §2.4 reserved the channel naming convention but did NOT specify an auth envelope. R3 review flagged this as a security gap. The auth contract: every envelope MUST carry (a) producer identity (DID), (b) signature over the rest of the envelope (including a cross-protocol domain separator — see sub-step 3 below), (c) monotonic per-producer sequence number for replay protection.

## Scope

Extend `VaultProjectionInvalidationEnvelope` with producer identity + signature, then update the `VaultProjectionInvalidationEmitter` trait + subscriber to verify the signature before invalidating the cache.

### Sub-steps

1. **Identity substrate import** — `crates/octo-vault/src/event_log_producer.rs`. Add `use octo_cap_macaroon::OverlayIdentity;` (RFC-0853 §3 Sovereign Identity Model — OverlayIdentity struct, re-exported from `crates/octo-network/src/ocrypt/identity.rs`) and a 64-byte signature field (`producer_signature: [u8; 64]` per RFC-0853 §8 Signature Model).

2. **Envelope struct extension** — `VaultProjectionInvalidationEnvelope` becomes 7 fields. The 64-byte witness field is renamed `producer_signature: [u8; 64]` (an ed25519 signature per RFC-0853 §8 Signature Model; the term "attenuation_witness" belongs to the macaroon substrate and is reserved vocabulary — see `cipherocto-design-principles` §Attenuation invariants cross boundaries):
   ```rust
   pub struct VaultProjectionInvalidationEnvelope {
       pub chain_id: ChainId,
       pub vault_id: VaultId,
       pub asset_id: AssetId,
       pub source_kind: ProjectionSource,
       pub producer_did: OverlayIdentity,   // NEW: RFC-0853 §3 Sovereign Identity Model
       pub sequence: u64,                    // NEW: monotonic per producer
       pub producer_signature: [u8; 64],   // NEW: ed25519 signature over the prior 6 fields
   }
   ```
   Wire-form impact: serialized payload grows by ~110 bytes (DID + u64 + 64 sig). Pub/sub bandwidth increase negligible.

3. **Producer-side signing** — `EventLogProducer::produce` default body populates `producer_did` from a new `producer_did: &OverlayIdentity` parameter. `producer_signature` is computed via `ed25519-dalek::Signer::sign` (per RFC-0853 §1 Cryptographic Primitives) over the canonical serialization of the prior 6 fields concatenated WITH the cross-protocol domain separator `b"cipherocto/cache-bus/invalidation/v2\0"` (mandatory — prevents replay across structurally similar protocols that happen to share field layout, e.g., the macaroon-issuance bus or any other `chain_id || vault_id || ...` concatenation). Without the separator, an envelope captured off the macaroon-issuance channel could replay against the cache bus. **Sequence number source:** new `&AtomicU64` parameter `producer_sequence: &AtomicU64`, fetched pre-sign and incremented post-sign via `fetch_add(1, Relaxed)`.

4. **Subscriber-side verification** — `cache_subscriber.rs` (Mission `cache-subscriber-bus-wiring.md`) loads a producer-trust-list at init (`HashMap<OverlayIdentity, VerifyingKey>` from `ed25519-dalek::VerifyingKey`). On envelope receipt: deserialize → re-serialize the prior 6 fields with the same domain separator prepended → call `verifying_key.verify(&preimage, &envelope.producer_signature)` → reject if `Err` OR if `envelope.sequence <= last_seen_sequence[&envelope.producer_did]` (replay protection). `last_seen_sequence: DashMap<OverlayIdentity, u64>` updated on accept-only.

5. **Producer-trust-list at process init** — `crates/octo-vault/src/lib.rs` exports `pub fn init_producer_trust_list(keys: Vec<(OverlayIdentity, VerifyingKey)>)`. Layer C binaries call this in bootstrap with their known producer DIDs.

6. **Wire-form versioning** — bump envelope version tag from `v1` to `v2` (or add a `version: u8` field). Without versioning, old producers (without auth fields) emit envelopes that fail verification. Migration: feature flag `cache_bus_auth_v2 = "warn-only"` for one cycle, hard-reject at next cycle.

## Out of Scope

- Replacing the underlying signature scheme (ed25519 is canonical per RFC-0853 §1 Cryptographic Primitives)
- Replay protection across process restarts (in-memory sequence map suffices for current scope; persistent sequence tracked separately)
- Per-key per-vault authorization (every authorized producer can invalidate any vault; finer-grained auth is a separate RFC)
- Channel encryption (pub/sub channel is plaintext; transport-layer TLS tracked separately)
- Cross-protocol replay protection beyond the domain separator (cross-bus replay via structurally similar envelopes — out of scope, mitigated by the per-bus domain separator in sub-step 3)

## Test Vectors

- TV-CB-1: Envelope struct has 7 fields (was 4); struct construction via `..Default::default()` no longer compiles (forces explicit field listing, catches accidental omission of auth fields)
- TV-CB-2: Producer-side sign-then-emit produces an envelope that the subscriber verifies as VALID
- TV-CB-3: Tampered envelope (modify `vault_id` after signing) fails verification → subscriber drops the envelope, `cache.invalidate` is NOT called
- TV-CB-4: Replay (re-emit same envelope twice) is rejected by sequence-number check
- TV-CB-5: Unknown producer_did (not in trust list) is rejected
- TV-CB-6: Process init with empty trust list → all envelopes rejected (fail-closed default)
- TV-CB-7: Wire-form round-trip via Serde preserves all 7 fields incl. `producer_signature`

## Layer direction (per `cipherocto-design-principles`)

- `octo-vault` (Layer B) — envelope struct extension + emitter trait + subscriber verification
- `octo-cap-macaroon` (Layer A) — `OverlayIdentity` import (existing primitive, no change)
- Layer C producer impls (octo-policy, octo-wallet-node, quota-router-sm-engine) — pass `producer_did` + `producer_sequence` to `produce()`
- Wire form grows by ~110 bytes — backward-incompatible (justified; mitigated by version tag + feature-flag cycle)

## Validation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features full -- -D warnings
cargo test --workspace --lib

# No accidental struct-update regressions
grep -rn "VaultProjectionInvalidationEnvelope {" crates/ agents/ use-cases/
# Each match MUST list all 7 fields explicitly
```

## Backward compat

- **Wire form:** BREAKING (envelope size + 3 new fields). Mitigation: version tag + 1-cycle `warn-only` feature flag.
- **Trait signature:** BREAKING (`produce` gains `producer_did: &OverlayIdentity` + `producer_sequence: &AtomicU64` params). Layer C producer impls must update.
- **Semver impact:** Layer B-additive per `cipherocto-design-principles` §Layer stability; envelope extension is semver-MINOR if version-tag-gated, semver-MAJOR if not.

## Risk

- HIGH: 1-cycle warn-only window means old producers emit unverifiable envelopes that subscribers reject → cache staleness. Mitigation: feature-flag the subscriber-side rejection with a `--features accept-unauthenticated` flag for the cycle. **Hard guard:** `accept-unauthenticated` is gated behind `#[cfg(debug_assertions)]` so it physically cannot compile into release builds — `cargo build --release` MUST refuse to link the feature. Documented as a debug-only escape hatch in `Cargo.toml` feature description. Without this guard, the warn-only window becomes a production default after the cycle ends (CVSS-relevant — the auth-bypass vector is the foundational threat the entire envelope signing scheme is designed to prevent; any production-grade debug escape hatch defeats the substrate's security model per `cipherocto-design-principles` §Attenuation invariants cross boundaries).
- MEDIUM: sequence number is in-memory; process restart resets sequence → replay window. Mitigation: persist `last_seen_sequence` per producer to disk at restart (Cycle 2).
- LOW: trust list is static at init. Adding a new producer requires process restart. Mitigation: dynamic trust list (Cycle 2).

## Cross-references

- RFC-0960 §2.4 — invalidation bus + envelope wire form
- RFC-0853 §1 Cryptographic Primitives — ed25519-dalek substrate for `Signer::sign` / `VerifyingKey::verify`
- RFC-0853 §8 Signature Model — signature envelope contract
- RFC-0009 — identity management substrate (canonical process RFC)
- RFC-0010 — canonical DID codec (wire-format for `OverlayIdentity`)
- Mission B (`0960-v37-b-event-log-producer-wiring.md`) — emitter trait
- Mission `cache-subscriber-bus-wiring.md` — subscriber consumer
- RFC-0960 §5 (Single Timeline — 3-cycle deprecation pattern described within) — template for feature-flag cycle
- `cipherocto-design-principles` §Attenuation invariants cross boundaries — auth-bypass CVSS framing + `producer_signature` reserved-vocabulary disambiguation from macaroon `attenuation_witness`

## Claimant

@mmacedoeu

## Pull Request

#