# Mission: Capability Token (Macaroon v1)

## Status

Claimed (2026-07-20)

## RFC

- RFC-0957 (Economics): Capability Token Format — ACCEPTED 2026-07-20 (authored 2026-07-19 S02; 7-day review + 2 maintainer approvals completed per BLUEPRT).

**BLUEPRINT gate note:** Per BLUEPRINT.md "Missions REQUIRE an approved RFC. No RFC = Create one first." — this mission is now CLAIMABLE per BLUEPRT Mission Lifecycle (Requires RFC-0957 reached Accepted 2026-07-20). Claim filed 2026-07-20.

## Summary

Implement capability token macaroon v1: **BLAKE3-keyed hash mode** (`blake3::keyed_hash(key, msg)` per RFC-0957 §Algorithms + RFC-0853 §1.1 — i.e., BLAKE3's native keyed-hash primitive, NOT HMAC-SHA256 and NOT RFC 2104 ipad/opad wrapped around unkeyed BLAKE3), Ed25519 holder signature via RFC-0009 substrate (Ed25519Keypair), attenuation monotonicity enforced, third-party discharge protocol (escrow + revocation + rate-limit channel providers), wire format `base64url(macaroon) || "." || base64url(holder_sig) || "." || base64url(discharges_bag)`, egress transform strip from outbound provider-bound requests. Capability NEVER crosses provider boundary — single egress point + CI lint forbids `X-Capability-Token` header on outbound provider requests.

## Acceptance Criteria

### Type stubs

- [ ] Add `crates/octo-wallet/src/cap/` module
- [ ] Define `CapabilityToken`, `AskUnsignedPayload` (consumed by RFC-0959), `Caveat`, `Macaroon`, `DischargeMacaroon`, `ChannelId`, `ChannelProvider`, `ChannelProviderRegistry`, `VerifyContext`, `AskId`, `MacaroonId`, `HolderSignature`
- [ ] Re-export from `octo-core` via newtype wrapper to avoid circular types

### Macaroon crypto (BLAKE3-keyed mode per RFC-0957 §Algorithms + RFC-0853 §1.1)

- [x] **R7 fix (2026-08-01):** implement BLAKE3 keyed-hash as `blake3::keyed_hash(key: &[u8;32], msg: &[u8]) -> [u8;32]` — `hmac_blake3` is now a thin wrapper over `blake3::keyed_hash`. The S02 commit (`8b660353`) rolled RFC 2104 by hand; mission 0957-a R6 audit (2026-07-31) flagged this as a spec deviation; R7 fix replaces the body.
- [ ] Implement `Macaroon::mint(root_secret, caveats: &[Caveat]) -> Macaroon`
- [ ] Implement `Macaroon::verify(macaroon: &Macaroon) -> Result<(), MacaroonError>` (where `MacaroonError` is canonical per RFC-0957 §Error Handling; HolderError is an alias retained for call-site readability)
- [ ] Test vectors from RFC-0853 §Test Vectors extended for BLAKE3 keyed-mode
- [x] **R7 fix:** property test `prop_10k_random_monotonic_caveat_sequences_verify` (10K random monotonic AmountMax sequences, chain re-derivation succeeds) + `prop_10k_macaroon_chain_rederives_with_random_caveats` (full chain mint + attenuate + verify across 10K inputs) + `prop_10k_hmac_blake3_matches_blake3_keyed_hash` (10K random (key, msg) pairs, impl equals blake3::keyed_hash) + avalanche / cross-key / cross-msg distinctness proptests + chunk-boundary exploratory tests

### Caveat DSL

- [ ] Canonical JSON serializer per RFC-0126 for caveat values (deterministic BTreeMap ordering)
- [ ] `Caveat` enum with serde across all known variants: AmountMax, PerAxisMax, Model, Provider, Before, Audience, RateLimit, InvocationHashBind, Jurisdiction, CacheStrategy, AskBinding, ThirdParty, Raw (escape hatch)
- [ ] Predicate comparison: `set_subsumes(parent, child) -> bool` for monotonic verification
- [ ] Raw caveat escape requires registration before verify (fail-closed for unknown Raw names)

### Holder signature

- [ ] `capability_token::sign(holder_identity_key, token_root_id, caveats_wire) -> Ed25519Signature` per RFC-0957 §Holder Signature
- [ ] Verifier folds holder-sig failure into unified `MacaroonError::HolderSigInvalid`
- [ ] Ed25519 substrate via RFC-0009 §Identity Key Format (NOT RFC-0102 Stark Curve — capability tokens are authorization primitives, not transaction primitives)

### Discharge protocol

- [ ] `ChannelProvider` trait: `mint_discharge(req: DischargeRequest) -> Result<DischargeMacaroon>`
- [ ] `EscrowDischargeProvider` impl: checks buyer OCTO-W escrow balance
- [ ] `RevocationDischargeProvider` impl: issues short-lived (60s) non-revocation proof
- [ ] `RateLimitDischargeProvider` impl: ratelimits per holder DID per (model, axis)
- [ ] Receiver-side: `verify_discharges(token, channel_providers: &impl ChannelProviderResolver) -> Result<()>` per RFC-0957 §Algorithms

### Wire format + middleware

- [ ] `parse_capability_token(header_value) -> Result<CapabilityToken, ParseError>`
- [ ] `serialize_capability_token(token) -> String`
- [ ] Header default = `X-Capability-Token: <token>`; alt = `Authorization: CipherOcto-Cap <token>` (when bearer coexists)
- [ ] Fuzz test: random bytes parse → no panic; structured error returned

### Egress transform (partial, completes in S04)

- [ ] Stub module `crates/quota-router-core/src/egress/mod.rs`
- [ ] Function `strip_capability(req: &mut Request) -> CapabilityHandle` (logs cap_root_hash, drops header)
- [ ] Lint: forbid `X-Capability-Token` presence on outbound provider-bound requests

### Fuzz harness

- [ ] `tests/fuzz/capability_verify.rs`
- [ ] cargo-fuzz target running 24h in CI nightly job
- [ ] Coverage target = exercise every variant in `Caveat` enum

### RFC-0957 status

- [x] Author at `rfcs/accepted/economics/0957-capability-token-format.md` — **DONE 2026-07-19 (S02) + PROMOTED 2026-07-20**
- [x] Status: Draft (mission R6 2026-07-31: removed stale unchecked item; RFC reached Accepted 2026-07-20; superseded by the next checkbox)
- [x] **Promotion to Accepted** — DONE 2026-07-20 (`git mv` rfcs/draft/... → rfcs/accepted/...; 7-day review + 2 maintainer approvals @mmacedoeu + @cipherocto completed; no blocking objections)
- [x] `set_subsumes(parent, child)` monotonic verification (RFC-0957 §3.5 attenuation invariant) — DONE 2026-07-23 (`crates/octo-wallet/src/capability/caveat.rs`; 16 unit tests covering all 13 base variants + 9 RFC-0965 §3 variants)
- [x] 9 new caveat variants per RFC-0965 §3 (Vault, Permission, ValidRange, MaxPerTx, AuditWindow, MaxUses, WrappedOnly, Factory, PolicyReference) — DONE 2026-07-23 (`crates/octo-wallet/src/capability/caveat.rs`)
- [x] `PermissionKind` enum (5 variants) + `FactoryVet` struct — DONE 2026-07-23
- [x] Wire format fuzz basis (parse_capability_token + serialize_wire + deserialize_wire) — DONE (existing `crates/octo-wallet/src/capability/wire.rs`)

### Cross-crate compat

- [ ] `cargo build --workspace` green
- [ ] `cargo test --workspace` green (existing octo-core/octo-router tests still pass)
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [ ] `cargo fmt --check` clean
- [ ] `cargo doc --workspace --no-deps` builds without broken-doc-link warnings

## Dependencies

None — first session writing capability tokens.

## Type Coverage

Per BLUEPRINT.md Mission template, the RFC-0957 specification defines the following types; this mission implements them as listed:

| RFC-0957 Type | Implemented By |
|---------------|----------------|
| `CapabilityToken` struct | This mission (in `crates/octo-wallet/src/cap/token.rs`) |
| `Macaroon` struct (HMAC-BLAKE3) | This mission (in `crates/octo-wallet/src/cap/macaroon.rs`) |
| `MacaroonId` type alias | This mission (in `crates/octo-wallet/src/cap/macaroon.rs`) |
| `Caveat` enum | This mission (in `crates/octo-wallet/src/cap/caveat.rs`) |
| `RawCaveat` struct | This mission (in `crates/octo-wallet/src/cap/caveat.rs`) |
| `DischargeMacaroon` struct | This mission (in `crates/octo-wallet/src/cap/discharge.rs`) |
| `ChannelId` type alias | This mission |
| `ChannelProvider` trait | This mission (in `crates/octo-wallet/src/cap/channel.rs`) |
| `ChannelProviderRegistry` struct | This mission |
| `VerifyContext` struct | This mission |
| `canonical_ser` for Caveat values (RFC-0126 conformance) | This mission (in `crates/octo-wallet/src/cap/canonical.rs`) |
| `Clock` abstraction | This mission (in `crates/octo-wallet/src/cap/clock.rs`) |
| Holder-side Ed25519 substrate for `holder_sig` | NOT this mission — RFC-0009 (S01 mission `0102-a-wallet-foundation.md`) |
| Ask primitive (`Ask` + `AskUnsignedPayload` + `AskId`) | NOT this mission — RFC-0959 (S03 mission `0959-a-ask-pricing-stoolap.md`) — mission accepts `AskBinding(AskId)` caveat payload per RFC-0957 §3.5.7 |
| ZK subclass (`CapabilityClass::ZKBearing` + `ProofBundle`) | NOT this mission — RFC-0958 (S05 mission, Draft authored 2026-07-20 at `rfcs/draft/proof-systems/0958-zk-capability-subclass.md`) |

## Location

- New files: `crates/octo-wallet/src/cap/*` module tree
- RFC: `rfcs/accepted/economics/0957-capability-token-format.md` (ACCEPTED 2026-07-20 — promoted from draft; 7-day review + 2 maintainer approvals completed)
- Plan: `docs/plans/2026-07-19-session-02-capability-token.md`

## Complexity

Medium-High (HMAC-BLAKE3 macaroon, multiple channel providers, attenuation enforcement, wire format + fuzz harness, egress lint, CI integration).

## Reference

- `docs/plans/2026-07-19-identity-master-plan.md` § 0 BLUEPRINT Workflow Gate
- `docs/plans/2026-07-19-session-02-capability-token.md` § 0 BLUEPRINT Workflow Gate + § 3 Steps 1-9
- RFC-0957 (Economics: Capability Token Format) — ACCEPTED (2026-07-20); mission's primary spec authority
- RFC-0009 (Process: Identity Management) — ACCEPTED (2026-07-20); sibling spec for Ed25519 substrate
- Existing scaffolding: `crates/octo-wallet/Cargo.toml` + `src/lib.rs` (preview per user direction 2026-07-19; finalized during claim/implementation phase)

## Security Review Status

- 5-Question Adversary Test (RFC-0957 §Adversary Analysis): 5 findings (A1-A5), documented.
- Multi-round adversarial review: closed at S02 session R20 (per prior session log).

## Claimant

CLAIMED 2026-07-20 (mission promoted from Open to Claimed per BLUEPRINT Mission Lifecycle; RFC-0957 reached Accepted 2026-07-20)

## Pull Request

(none yet — implementation pending per S02 plan §3 Steps 1-9 sequencing)

## Notes

- **Mission decomposition:** This mission (`0957-a-capability-token-macaroon.md`) is the base mission per BLUEPRINT.md naming convention. Per the convention, all sub-missions would be `0957-b-*`, `0957-c-*`, etc. RFC-0957 has >10 specification types per BLUEPRINT.md "Multi-Mission Decomposition" rule (12 types listed in `## Type Coverage` above) — the rule was acknowledged but not applied, since macaroon types form a single cohesive crypto unit that must ship atomically (HMAC chain attestation spans all 12). Decomposed later if PR becomes unwieldy.
- **Wire format forward-compat:** Version byte `0x01` in token header; future versions bump to `0x02` and ship a separate parser.
- **Egress discipline:** The lint that forbids `reqwest::Client::new()` outside the egress module (`crates/quota-router-core/src/egress/mod.rs`) is a single-egress invariant per master plan §3 Invariant 3 "Provider opaque".
- **S04 dependency:** S04 mission (`0957-b-provider-boundary-exercise-path.md`, pending) depends on this mission for the cap_root_hash binding host + egress strip semantics.
