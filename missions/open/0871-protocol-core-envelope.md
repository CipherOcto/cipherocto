# Mission: 0871 — Protocol Core Envelope (RFC-0871 Phase 1)

## Status

Open (2026-08-09). RFC-0871 Accepted 2026-08-09 after R1–R7 adversarial review (R7 DRY). Phase 1 base mission.

## RFC

RFC-0871 (Networking): Specialized Node Protocol Envelope

**BLUEPRINT gate note:** RFC-0871 is Accepted. Mission 0871 implements Phase 1 of the RFC's §Implementation Phases.

This mission creates `crates/octo-protocol/` — the foundational crate that owns `NodeEnvelope`, `PayloadKindId`, `Authorization`, `RecipientRef`, `ProtocolError`. All downstream Phase 2–5 missions (`0871a` wallet node, `0870-b` quota router adoption, `0957-ext-*` capability crate extraction, `0871e` paid query) consume this crate. No upstream envelope types exist today; this mission establishes them per RFC-0871 §Data Structures + §Algorithms.

## Summary

Create `crates/octo-protocol/` Layer-1-stable crate owning the canonical envelope types + dispatch logic. Implement `NodeEnvelope` (borsh serde), `PayloadKindId` (128-bit UUID per RFC-0965 discriminator pattern), `Authorization` enum (Signature / Capability / ThresholdSignature / Proof / Raw), `RecipientRef`, `ProtocolError`. Implement `envelope_id` derivation via `blake3::derive_key("OCTO_NODEENVELOPE_V1_ID", from_did_wire || payload || nonce || expires_at_unix_ms)`. Enforce canonical DID validation via `octo_ident::CanonicalCodec::parse()` on every envelope boundary. Ship 8 test vectors (TV1–TV8) byte-exact from RFC-0871 §Test Vectors.

## Acceptance Criteria

### Top-level: Crate + types

- [ ] NEW: `crates/octo-protocol/` crate with `Cargo.toml` + `src/lib.rs`
- [ ] `crates/octo-protocol/src/envelope.rs` — `NodeEnvelope` struct per RFC-0871 §Data Structures
- [ ] `crates/octo-protocol/src/payload_kind.rs` — `PayloadKindId([u8; 16])` with RFC namespace allocation (`RFC_OWNED_*` + `USER_EXTENSION_*` ranges)
- [ ] `crates/octo-protocol/src/authorization.rs` — `Authorization` enum: `Signature`, `Capability(Macaroon)`, `ThresholdSignature { signers: Vec<WireDid>, sig: BlsSignature }`, `Proof(ZkProofBundle)`, `Raw { discriminator: [u8;16], body: Vec<u8> }`
- [ ] `crates/octo-protocol/src/recipient.rs` — `RecipientRef` enum (`NodeId`, `GroupId`, `Broadcast`, `MissionScoped`)
- [ ] `crates/octo-protocol/src/error.rs` — `ProtocolError` (thiserror enum: `InvalidEnvelopeId`, `InvalidDid`, `ExpiredEnvelope`, `ReplayedEnvelope`, `UnknownPayloadKind`, `AuthorizationFailed`, `SerializationError`)
- [ ] `crates/octo-protocol/src/dispatch.rs` — `EnvelopeDispatcher` trait: `dispatch(envelope) -> Result<HandlerOutput, ProtocolError>`
- [ ] `crates/octo-protocol/src/signing.rs` — `envelope_id` derivation + domain-separated signing preimage `blake3::derive_key("OCTO_NODEENVELOPE_V1_SIGNATURE", envelope_id || from_did_wire || payload)`
- [ ] borsh `Serialize`/`Deserialize` impls for all types; byte-exact round-trip
- [ ] Canonical DID validation: every `NodeEnvelope.from_did` field validated via `octo_ident::CanonicalCodec::parse(from_did.as_str(), false)` on construct
- [ ] Workspace `Cargo.toml` adds `crates/octo-protocol` to `members`
- [ ] `Cargo.toml` deps per CLAUDE.md crate stability: `borsh` (Layer A canonical serialization per RFC-0126), `blake3` (Layer A per RFC-0853), `octo-ident` (Layer B per RFC-0010)
- [ ] `cargo clippy -p octo-protocol --all-targets -- -D warnings` clean (per `[[feedback_clippy_zero_warnings]]`)
- [ ] `cargo fmt --check` clean (per `[[cargo-fmt-workflow]]`)

### Test vectors (RFC-0871 §Test Vectors TV1–TV8)

- [ ] TV1 — Self-sign envelope with default `InMemorySigner`: byte-exact reproduction per RFC-0871 §TV1 algorithm
- [ ] TV2 — Receiver rejects expired envelope (`expires_at_unix_ms < now`) returns `ProtocolError::ExpiredEnvelope`
- [ ] TV3 — Receiver rejects replayed envelope (envelope_id in seen-set) returns `ProtocolError::ReplayedEnvelope`
- [ ] TV4 — Receiver accepts envelope with `Vec<Authorization>` containing capability + signature
- [ ] TV5 — Wallet node announces payload kinds via `RouterAnnouncePayload` (placeholder: reuse RFC-0870 shape)
- [ ] TV6 — Test-only `MockLedgerSigner` signs capability mint request via envelope
- [ ] TV7 — Cross-domain envelope (identity resolve from quota node context)
- [ ] TV8 — Borsh serialization byte-exact across two independent implementations (determinism parity)
- [ ] All TVs include domain-separated preimage + cross-implementation parity assertion (per RFC-0871 §TV algorithm spec)

### Adversary coverage (RFC-0871 §Adversary Analysis A1–A7)

- [ ] A1 replay attack — covered by TV3 + `EnvelopeDispatcher::dispatch` envelope_id dedup
- [ ] A2 unauthorized sender — covered by TV4 + `Authorization::Signature` ed25519 verify
- [ ] A3 unknown payload kind — fail-closed (per RFC-0965 §3.2) returns `ProtocolError::UnknownPayloadKind`
- [ ] A4 expired envelope — covered by TV2
- [ ] A5 nonce collision — `nonce: [u8; 16]` random per sender, accepted iff unique
- [ ] A6 HSM bypass — all signing paths route through `Arc<dyn HsmAdapter>` (no direct `ed25519_dalek::SigningKey` access)
- [ ] A7 DID spoofing — canonical validation rejects non-canonical wire form per RFC-0010

### Cross-crate compat

- [ ] `cargo build --workspace --features full` green
- [ ] `cargo test --workspace --lib` green
- [ ] `cargo clippy --workspace --all-targets --features full -- -D warnings` green

## Type Coverage

Per BLUEPRINT §Mission template. RFC-0871 §Specification types mapped to this mission (Phase 1 base):

| RFC-0871 Type / Section | Implemented By |
|---|---|
| `NodeEnvelope` (§Data Structures) | This mission — `crates/octo-protocol/src/envelope.rs` |
| `PayloadKindId` (§Data Structures) | This mission — `crates/octo-protocol/src/payload_kind.rs` |
| `Authorization` enum (§Data Structures) | This mission — `crates/octo-protocol/src/authorization.rs` (signature + capability + threshold + ZK + raw variants) |
| `RecipientRef` (§Data Structures) | This mission — `crates/octo-protocol/src/recipient.rs` |
| `ProtocolError` (§Error Handling) | This mission — `crates/octo-protocol/src/error.rs` |
| `EnvelopeDispatcher` (§Algorithms) | This mission — `crates/octo-protocol/src/dispatch.rs` |
| `envelope_id` derivation (§Algorithms) | This mission — `crates/octo-protocol/src/signing.rs` (domain-separated `blake3::derive_key("OCTO_NODEENVELOPE_V1_ID", ...)`) |
| Signing preimage (§Algorithms) | This mission — `crates/octo-protocol/src/signing.rs` (domain-separated `blake3::derive_key("OCTO_NODEENVELOPE_V1_SIGNATURE", ...)`) |
| Test vectors TV1–TV8 (§Test Vectors) | This mission — `crates/octo-protocol/tests/tv{1..8}_*.rs` |
| `WalletNode` struct (§Wallet Node Lifecycle) | Mission `0871a-wallet-node.md` — consumes `NodeEnvelope` from this crate |
| Per-extension crate adapter (§Per-Extension Crate Layout) | Missions `0957-ext-macaroon-crate.md`, `0957-ext-zk-crate.md` — register `CapabilitySpec` impls that produce `Authorization::Capability` / `Authorization::Proof` |
| `PaymentCaveat` (§Implementation Phases Phase 5) | Mission `0871e-paid-query-caveat.md` — extends `Authorization::Capability` with payment caveat chain |

## Dependencies

**Requires:**

- RFC-0871 — accepted substrate
- RFC-0010 — canonical DID codec (`octo_ident::CanonicalCodec`)
- RFC-0853 — BLAKE3 primitive source
- RFC-0126 — canonical serialization (borsh conformance)
- RFC-0965 — `PayloadKindId` 128-bit UUID discriminator pattern
- `crates/octo-ident` — DID parsing crate (exists)

**Mission gates:**

- RFC-0871 Accepted (committed 2026-08-09, commit `350ba7b8`)
- Workspace `Cargo.toml` member registration

**Consumes this mission (downstream):**

- `missions/open/0871a-wallet-node.md` — Phase 2 wallet node uses `octo-protocol::NodeEnvelope`
- `missions/open/0870-b-envelope-adoption.md` — Phase 3 quota router adoption uses `octo-protocol::NodeEnvelope` (already filed)
- `missions/open/0957-ext-macaroon-crate.md` — Phase 4 macaroon extraction uses `octo-protocol::Authorization::Capability`
- `missions/open/0957-ext-zk-crate.md` — Phase 4 ZK extraction uses `octo-protocol::Authorization::Proof`
- `missions/open/0969-a-gateway-relocation.md` — Phase 5 gateway uses `octo-protocol::dispatch`

**Not Requires:**

- RFC-0871 §Phase 2 wallet node (separate mission)
- RFC-0871 §Phase 3 specialized node adoption (separate missions per node type)
- RFC-0871 §Phase 4 per-extension crate extraction (separate missions per extension)
- RFC-0871 §Phase 5 paid query (separate mission)

## Implementation Guide

- NEW crate: `crates/octo-protocol/` with standard Rust crate layout (`src/lib.rs`, `src/envelope.rs`, `src/payload_kind.rs`, `src/authorization.rs`, `src/recipient.rs`, `src/error.rs`, `src/dispatch.rs`, `src/signing.rs`, `tests/`)
- Type definitions: copy Rust struct definitions verbatim from RFC-0871 §Data Structures
- Signing preimage: implement per RFC-0871 §Algorithms with `Clock` injection (no `std::time::SystemTime` in signature paths)
- borsh serde: derive on every type, write `wire_v1_roundtrip` test asserting byte-exact round-trip
- Test vectors: write `tests/tv1_self_sign.rs`, `tests/tv2_expired.rs`, ..., `tests/tv8_borsh_parity.rs` matching RFC-0871 §TV algorithm spec exactly
- Cross-impl parity test: build two independent `NodeEnvelope` constructors (one from raw bytes, one from typed fields) and assert `borsh::to_vec(&a) == borsh::to_vec(&b)` for the same logical envelope
- Add `pub use octo_protocol::{NodeEnvelope, PayloadKindId, Authorization, RecipientRef, ProtocolError}` re-exports

## Acceptance Cross-Ref

Per RFC-0871 §Implementation Phases Phase 1 (Core envelope acceptance):

- [x] RFC Accepted (2026-08-09)
- [ ] Mission filed (this file)
- [ ] Implementation landed (Phase 1)
- [ ] Test vectors 1–8 pass
- [ ] Adversary Analysis A1–A7 covered

## Claimant

@unassigned

## Pull Request

#

## Notes

- Layer A crate (RFC-0871 §Layer A scope: crypto primitives + canonical encoding + semantic policies). Stability: RFC-frozen, semver-major only.
- All type definitions must match RFC-0871 §Data Structures byte-for-byte. No drift between spec and impl.
- `envelope_id` uniqueness must be enforced per-receiver (local seen-set); per RFC-0871 §Algorithms.
- Cross-implementation parity (TV8) is the primary correctness gate — any drift between Rust impl + spec algorithm is a CRITICAL finding per BLUEPRINT §Adversary Analysis.
- Companion mission `0870-b-envelope-adoption.md` (Phase 3 quota router) is ALREADY filed; this mission unblocks it.
